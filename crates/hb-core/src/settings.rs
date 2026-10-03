//! Typed settings with qutebrowser-style dotted names. Values arrive as JSON
//! from TOML, Lua or `:set`, so validation lives in one place.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde_json::Value as Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<String>),
    Map(BTreeMap<String, String>),
}

impl Value {
    pub fn to_json(&self) -> Json {
        match self {
            Value::Bool(b) => Json::Bool(*b),
            Value::Int(i) => Json::from(*i),
            Value::Str(s) => Json::String(s.clone()),
            Value::List(l) => Json::from(l.clone()),
            Value::Map(m) => Json::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                    .collect(),
            ),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Str(s) => f.write_str(s),
            other => write!(f, "{}", other.to_json()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Str,
    Enum(&'static [&'static str]),
    List,
    Map,
}

type Validator = fn(&Value) -> Result<(), String>;

pub struct SettingDef {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    default: fn() -> Value,
    validate: Option<Validator>,
}

impl SettingDef {
    pub fn default_value(&self) -> Value {
        (self.default)()
    }

    /// Convert and validate a JSON value (from TOML, Lua or `:set`).
    pub fn from_json(&self, json: &Json) -> Result<Value, String> {
        let expected = |what: &str| format!("{}: expected {what}, got {json}", self.name);
        let value = match self.kind {
            Kind::Bool => Value::Bool(json.as_bool().ok_or_else(|| expected("true or false"))?),
            Kind::Int { min, max } => {
                let i = json.as_i64().ok_or_else(|| expected("an integer"))?;
                if !(min..=max).contains(&i) {
                    return Err(format!("{}: {i} is outside {min}..={max}", self.name));
                }
                Value::Int(i)
            }
            Kind::Str => Value::Str(
                json.as_str()
                    .ok_or_else(|| expected("a string"))?
                    .to_string(),
            ),
            Kind::Enum(options) => {
                let s = json.as_str().ok_or_else(|| expected("a string"))?;
                if !options.contains(&s) {
                    return Err(format!(
                        "{}: {s:?} is not one of {}",
                        self.name,
                        options.join(", ")
                    ));
                }
                Value::Str(s.to_string())
            }
            Kind::List => {
                let items = json
                    .as_array()
                    .ok_or_else(|| expected("a list of strings"))?;
                let items = items
                    .iter()
                    .map(|i| {
                        i.as_str()
                            .map(String::from)
                            .ok_or_else(|| expected("a list of strings"))
                    })
                    .collect::<Result<_, _>>()?;
                Value::List(items)
            }
            Kind::Map => {
                let map = json
                    .as_object()
                    .ok_or_else(|| expected("a table of strings"))?;
                let map = map
                    .iter()
                    .map(|(k, v)| {
                        v.as_str()
                            .map(|v| (k.clone(), v.to_string()))
                            .ok_or_else(|| expected("a table of strings"))
                    })
                    .collect::<Result<_, _>>()?;
                Value::Map(map)
            }
        };
        if let Some(validate) = self.validate {
            validate(&value).map_err(|e| format!("{}: {e}", self.name))?;
        }
        Ok(value)
    }

    /// Parse `:set` text: plain words for scalars, JSON for lists and tables.
    pub fn parse(&self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        let json = match self.kind {
            Kind::Bool => match text {
                "true" | "yes" | "on" | "1" => Json::Bool(true),
                "false" | "no" | "off" | "0" => Json::Bool(false),
                _ => {
                    return Err(format!(
                        "{}: expected true or false, got {text:?}",
                        self.name
                    ));
                }
            },
            Kind::Int { .. } => Json::from(
                text.parse::<i64>()
                    .map_err(|_| format!("{}: expected an integer, got {text:?}", self.name))?,
            ),
            Kind::Str | Kind::Enum(_) => Json::String(text.to_string()),
            Kind::List | Kind::Map => serde_json::from_str(text)
                .map_err(|e| format!("{}: expected JSON ({e})", self.name))?,
        };
        self.from_json(&json)
    }
}

fn editor_command(value: &Value) -> Result<(), String> {
    match value {
        Value::List(argv) if argv.iter().any(|a| a.contains("{file}")) => Ok(()),
        _ => Err("editor.command must contain {file}".into()),
    }
}

/// `editor.command` with its fields filled in. `line` and `column` count from 1.
pub fn editor_argv(template: &[String], file: &str, line: usize, column: usize) -> Vec<String> {
    template
        .iter()
        .map(|arg| {
            arg.replace("{file}", file)
                .replace("{line0}", &line.saturating_sub(1).to_string())
                .replace("{column0}", &column.saturating_sub(1).to_string())
                .replace("{line}", &line.to_string())
                .replace("{column}", &column.to_string())
        })
        .collect()
}

fn hint_chars(value: &Value) -> Result<(), String> {
    let Value::Str(s) = value else { return Ok(()) };
    let mut chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    chars.sort_unstable();
    chars.dedup();
    if len < 2 || chars.len() != len {
        return Err("needs at least two distinct characters".into());
    }
    Ok(())
}

fn search_engines(value: &Value) -> Result<(), String> {
    let Value::Map(map) = value else {
        return Ok(());
    };
    if !map.contains_key("DEFAULT") {
        return Err("needs a DEFAULT entry".into());
    }
    match map.iter().find(|(_, url)| !url.contains("{}")) {
        Some((name, _)) => Err(format!("engine {name:?} needs a {{}} placeholder")),
        None => Ok(()),
    }
}

const POSITIONS: &[&str] = &["prev", "next", "first", "last"];
const ASK: &[&str] = &["ask", "true", "false"];

macro_rules! def {
    ($name:literal, $kind:expr, $default:expr, $desc:literal $(, $validate:expr)?) => {
        SettingDef {
            name: $name,
            description: $desc,
            kind: $kind,
            default: || $default,
            validate: def!(@validate $($validate)?),
        }
    };
    (@validate) => { None };
    (@validate $v:expr) => { Some($v) };
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn map(pairs: &[(&str, &str)]) -> Value {
    Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

pub static SETTINGS: &[SettingDef] = &[
    def!(
        "aliases",
        Kind::Map,
        map(&[("q", "quit"), ("qa", "quit"), ("wq", "quit --save")]),
        "Command aliases: name → command"
    ),
    def!(
        "auto_save.session",
        Kind::Bool,
        Value::Bool(false),
        "Save the open tabs as the 'default' session on quit, and restore them at startup"
    ),
    def!(
        "colors.webpage.darkmode.enabled",
        Kind::Bool,
        Value::Bool(false),
        "Render light pages dark with Chromium's automatic dark mode (takes effect after a restart)"
    ),
    def!(
        "colors.webpage.preferred_color_scheme",
        Kind::Enum(&["auto", "light", "dark"]),
        s("auto"),
        "The color scheme pages see in prefers-color-scheme: auto follows the system"
    ),
    def!(
        "completion.web_history.max_items",
        Kind::Int {
            min: 0,
            max: 10_000
        },
        Value::Int(100),
        "How many history entries :open completion shows (0 turns history completion off)"
    ),
    def!(
        "content.blocking.adblock.lists",
        Kind::List,
        Value::List(vec![
            "https://easylist.to/easylist/easylist.txt".to_string(),
            "https://easylist.to/easylist/easyprivacy.txt".to_string(),
        ]),
        "Adblock Plus filter lists that :adblock-update downloads (https://, or file:// for local lists)"
    ),
    def!(
        "content.blocking.enabled",
        Kind::Bool,
        Value::Bool(true),
        "Block ads and trackers with the filter lists from content.blocking.adblock.lists"
    ),
    def!(
        "content.blocking.whitelist",
        Kind::List,
        Value::List(Vec::new()),
        "Sites where nothing is blocked, as host names; a host also covers its subdomains"
    ),
    def!(
        "content.desktop_capture",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites capture your screen or desktop audio: ask, true or false"
    ),
    def!(
        "content.geolocation",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites know your location: ask, true or false"
    ),
    def!(
        "content.media.audio_capture",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites use your microphone: ask, true or false"
    ),
    def!(
        "content.media.video_capture",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites use your camera: ask, true or false"
    ),
    def!(
        "content.notifications.enabled",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites show notifications: ask, true or false"
    ),
    def!(
        "content.tls.certificate_errors",
        Kind::Enum(&["ask", "block", "load-insecurely"]),
        s("ask"),
        "Pages whose TLS certificate isn't trusted: ask, block, or load-insecurely"
    ),
    def!(
        "downloads.location.directory",
        Kind::Str,
        s(""),
        "Where downloads go; empty means the system Downloads folder"
    ),
    def!(
        "downloads.location.prompt",
        Kind::Bool,
        Value::Bool(true),
        "Ask where to save each download (false saves straight to the directory)"
    ),
    def!(
        "editor.command",
        Kind::List,
        Value::List(
            ["gvim", "-f", "{file}", "-c", "normal {line}G{column0}l"]
                .map(String::from)
                .to_vec()
        ),
        "Editor for :open-editor; fields: {file}, {line}, {column}, {line0}, {column0}",
        editor_command
    ),
    def!(
        "hints.chars",
        Kind::Str,
        s(crate::hints::DEFAULT_HINT_CHARS),
        "Characters used for hint labels",
        hint_chars
    ),
    def!(
        "hints.uppercase",
        Kind::Bool,
        Value::Bool(false),
        "Show hint labels in upper case"
    ),
    def!(
        "input.forward_unbound_keys",
        Kind::Enum(&["all", "auto", "none"]),
        s("auto"),
        "Pass unbound keys to the page in normal mode (auto: all but plain letters and digits)"
    ),
    def!(
        "input.insert_mode.auto_enter",
        Kind::Bool,
        Value::Bool(true),
        "Enter insert mode when an editable element gets focus"
    ),
    def!(
        "input.insert_mode.auto_leave",
        Kind::Bool,
        Value::Bool(true),
        "Leave insert mode when focus leaves an editable element"
    ),
    def!(
        "input.insert_mode.leave_on_load",
        Kind::Bool,
        Value::Bool(true),
        "Leave insert mode when a new page starts loading"
    ),
    def!(
        "messages.timeout",
        Kind::Int {
            min: 0,
            max: 3_600_000
        },
        Value::Int(3000),
        "Milliseconds before a status bar message clears (0 keeps it)"
    ),
    def!(
        "new_instance_open_target",
        Kind::Enum(&["tab", "tab-bg", "window"]),
        s("tab"),
        "Where URLs from a second hackers-browser invocation open"
    ),
    def!(
        "search.ignore_case",
        Kind::Enum(&["smart", "always", "never"]),
        s("smart"),
        "Case in searches: smart ignores it unless the text has a capital, always, or never"
    ),
    def!(
        "search.incremental",
        Kind::Bool,
        Value::Bool(true),
        "Search while typing after / or ?"
    ),
    def!(
        "spellcheck.languages",
        Kind::List,
        Value::List(Vec::new()),
        "Spell-check languages such as en-US (empty: off); Chromium downloads each dictionary from Google once"
    ),
    def!(
        "tabs.favicons.show",
        Kind::Enum(&["always", "never", "pinned"]),
        s("always"),
        "Show site icons in the tab bar: always, never, or only on pinned tabs"
    ),
    def!(
        "tabs.last_close",
        Kind::Enum(&["ignore", "blank", "startpage", "default-page", "close"]),
        s("ignore"),
        "What closing the last tab does"
    ),
    def!(
        "tabs.mode_on_change",
        Kind::Enum(&["normal", "persist", "restore"]),
        s("normal"),
        "Mode after switching tabs: normal, persist (keep insert/passthrough), or restore (the mode the tab was left in)"
    ),
    def!(
        "tabs.mousewheel_switching",
        Kind::Bool,
        Value::Bool(true),
        "Switch tabs with the mouse wheel over the tab bar"
    ),
    def!(
        "tabs.new_position.related",
        Kind::Enum(POSITIONS),
        s("next"),
        "Where tabs opened from a page go (popups, hints)"
    ),
    def!(
        "tabs.new_position.unrelated",
        Kind::Enum(POSITIONS),
        s("last"),
        "Where other new tabs go (:open -t)"
    ),
    def!(
        "tabs.pinned.frozen",
        Kind::Bool,
        Value::Bool(true),
        "Keep pinned tabs on their page: :open in a pinned tab opens a new tab"
    ),
    def!(
        "tabs.pinned.shrink",
        Kind::Bool,
        Value::Bool(true),
        "Shrink pinned tabs to their icon and number"
    ),
    def!(
        "url.default_page",
        Kind::Str,
        s(crate::url::DEFAULT_START_PAGE),
        "Page for :open without a URL"
    ),
    def!(
        "url.searchengines",
        Kind::Map,
        map(&[("DEFAULT", crate::url::DEFAULT_SEARCH_ENGINE)]),
        "Search engines; ':open g rust' uses the 'g' entry, anything else DEFAULT",
        search_engines
    ),
    def!(
        "url.start_pages",
        Kind::List,
        Value::List(vec![crate::url::DEFAULT_START_PAGE.to_string()]),
        "Pages opened at startup when no URL is given"
    ),
    def!(
        "window.title_format",
        Kind::Str,
        s("{current_title}{title_sep}hackers-browser"),
        "Window title; fields: {current_title}, {title_sep}, {current_url}, {host}, {mode}"
    ),
];

pub fn find(name: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|d| d.name == name)
}

/// Current values, starting from the defaults.
/// Settings that can differ per site (`:set -u <pattern>`, `[per_domain]`).
pub const PER_DOMAIN: &[&str] = &[
    "content.blocking.enabled",
    "content.desktop_capture",
    "content.geolocation",
    "content.media.audio_capture",
    "content.media.video_capture",
    "content.notifications.enabled",
    "content.tls.certificate_errors",
];

#[derive(Clone, Debug)]
pub struct Settings {
    values: HashMap<&'static str, Value>,
    /// URL pattern overrides, in the order they were set; the last match wins.
    per_domain: Vec<(String, &'static str, Value)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            values: SETTINGS
                .iter()
                .map(|d| (d.name, d.default_value()))
                .collect(),
            per_domain: Vec::new(),
        }
    }
}

impl Settings {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Store an already validated value.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        let def = find(name).ok_or_else(|| format!("No option {name:?}"))?;
        self.values.insert(def.name, value);
        Ok(())
    }

    /// Store an already validated value for URLs matching `pattern`.
    pub fn set_for(&mut self, pattern: &str, name: &str, value: Value) -> Result<(), String> {
        let def = find(name).ok_or_else(|| format!("No option {name:?}"))?;
        if !PER_DOMAIN.contains(&def.name) {
            return Err(format!("{name} can't be set per site"));
        }
        self.per_domain
            .retain(|(p, n, _)| !(p == pattern && *n == def.name));
        self.per_domain.push((pattern.to_string(), def.name, value));
        Ok(())
    }

    /// The value for a page: the last matching `per_domain` entry, or the global one.
    pub fn get_for(&self, name: &str, url: &str) -> Option<&Value> {
        self.per_domain
            .iter()
            .rev()
            .find(|(pattern, n, _)| *n == name && crate::url::pattern_matches(pattern, url))
            .map(|(_, _, v)| v)
            .or_else(|| self.get(name))
    }

    pub fn str_for(&self, name: &str, url: &str) -> &str {
        match self.get_for(name, url) {
            Some(Value::Str(s)) => s,
            _ => "",
        }
    }

    pub fn bool_for(&self, name: &str, url: &str) -> bool {
        matches!(self.get_for(name, url), Some(Value::Bool(true)))
    }

    /// Every per-site value of `name`, as `(pattern, value)`.
    pub fn overrides(&self, name: &str) -> Vec<(String, Value)> {
        self.per_domain
            .iter()
            .filter(|(_, n, _)| *n == name)
            .map(|(p, _, v)| (p.clone(), v.clone()))
            .collect()
    }

    pub fn bool(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Value::Bool(true)))
    }

    pub fn int(&self, name: &str) -> i64 {
        match self.get(name) {
            Some(Value::Int(i)) => *i,
            _ => 0,
        }
    }

    pub fn str(&self, name: &str) -> &str {
        match self.get(name) {
            Some(Value::Str(s)) => s,
            _ => "",
        }
    }

    pub fn list(&self, name: &str) -> &[String] {
        match self.get(name) {
            Some(Value::List(l)) => l,
            _ => &[],
        }
    }

    pub fn map(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        match self.get(name) {
            Some(Value::Map(m)) => Some(m),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_site_values_override_the_global_one() {
        let mut s = Settings::default();
        let ask = |s: &Settings, url| s.str_for("content.geolocation", url).to_string();
        assert_eq!(ask(&s, "https://maps.example/"), "ask");
        s.set_for(
            "*.example",
            "content.geolocation",
            Value::Str("true".into()),
        )
        .unwrap();
        s.set_for(
            "https://bad.example",
            "content.geolocation",
            Value::Str("false".into()),
        )
        .unwrap();
        assert_eq!(ask(&s, "https://maps.example/"), "true");
        assert_eq!(ask(&s, "https://bad.example/x"), "false");
        assert_eq!(ask(&s, "https://other.org/"), "ask");
        // Setting the same pattern again replaces it and makes it the newest.
        s.set_for(
            "*.example",
            "content.geolocation",
            Value::Str("false".into()),
        )
        .unwrap();
        assert_eq!(ask(&s, "https://maps.example/"), "false");
        assert_eq!(s.overrides("content.geolocation").len(), 2);
        assert!(
            s.set_for("x.org", "hints.chars", Value::Str("ab".into()))
                .is_err()
        );
    }

    #[test]
    fn editor_fields() {
        let template: Vec<String> = [
            "vim",
            "+call cursor({line}, {column})",
            "{file}",
            "{line0}:{column0}",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            editor_argv(&template, "/tmp/x.txt", 3, 5),
            ["vim", "+call cursor(3, 5)", "/tmp/x.txt", "2:4"]
        );
        let def = find("editor.command").unwrap();
        assert!(def.from_json(&serde_json::json!(["vim"])).is_err());
        assert!(def.from_json(&serde_json::json!(["vim", "{file}"])).is_ok());
    }
    use serde_json::json;

    #[test]
    fn defaults_pass_their_own_validation() {
        for def in SETTINGS {
            let value = def.default_value();
            assert_eq!(def.from_json(&value.to_json()), Ok(value), "{}", def.name);
        }
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names: Vec<_> = SETTINGS.iter().map(|d| d.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn parses_set_text() {
        let def = find("hints.uppercase").unwrap();
        assert_eq!(def.parse("yes"), Ok(Value::Bool(true)));
        assert!(def.parse("maybe").is_err());
        let def = find("messages.timeout").unwrap();
        assert_eq!(def.parse("500"), Ok(Value::Int(500)));
        assert!(def.parse("-1").is_err());
        let def = find("url.start_pages").unwrap();
        assert_eq!(
            def.parse(r#"["a", "b"]"#),
            Ok(Value::List(vec!["a".into(), "b".into()]))
        );
        assert!(def.parse("a, b").is_err());
    }

    #[test]
    fn validates() {
        assert!(
            find("hints.chars")
                .unwrap()
                .from_json(&json!("aa"))
                .is_err()
        );
        assert!(find("hints.chars").unwrap().from_json(&json!("a")).is_err());
        assert!(
            find("tabs.last_close")
                .unwrap()
                .from_json(&json!("explode"))
                .is_err()
        );
        let engines = find("url.searchengines").unwrap();
        assert!(
            engines
                .from_json(&json!({"g": "https://g.co/?q={}"}))
                .is_err()
        );
        assert!(
            engines
                .from_json(&json!({"DEFAULT": "https://x.org/"}))
                .is_err()
        );
        assert!(
            engines
                .from_json(&json!({"DEFAULT": "https://x.org/?q={}"}))
                .is_ok()
        );
        assert!(
            find("hints.uppercase")
                .unwrap()
                .from_json(&json!("true"))
                .is_err()
        );
    }
}
