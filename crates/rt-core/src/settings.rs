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
/// `hints.selectors` with the built-in groups filled in, so adding a group
/// doesn't take the others away.
pub fn hint_selectors(settings: &Settings) -> BTreeMap<String, String> {
    let mut groups: BTreeMap<String, String> = crate::hints::DEFAULT_SELECTORS
        .iter()
        .map(|(name, selectors)| (name.to_string(), selectors.to_string()))
        .collect();
    if let Some(user) = settings.map("hints.selectors") {
        groups.extend(user.clone());
    }
    groups
}

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

/// What `statusbar.widgets` can show, besides `clock[:format]` and `text:…`.
pub const STATUSBAR_WIDGETS: &[&str] = &[
    "keypress",
    "downloads",
    "muted",
    "zoom",
    "search_match",
    "url",
    "scroll",
    "scroll_raw",
    "history",
    "tabs",
    "progress",
];

fn statusbar_widgets(value: &Value) -> Result<(), String> {
    let Value::List(widgets) = value else {
        return Ok(());
    };
    for widget in widgets {
        let known = STATUSBAR_WIDGETS.contains(&widget.as_str())
            || widget == "clock"
            || widget.starts_with("clock:")
            || widget.starts_with("text:");
        if !known {
            return Err(format!(
                "unknown widget {widget:?}; use {}, clock[:format] or text:…",
                STATUSBAR_WIDGETS.join(", ")
            ));
        }
    }
    Ok(())
}

fn key_mappings(value: &Value) -> Result<(), String> {
    let Value::Map(map) = value else {
        return Ok(());
    };
    for (from, to) in map {
        for keys in [from, to] {
            match crate::key::Key::parse_sequence(keys) {
                Ok(parsed) if parsed.len() == 1 => {}
                Ok(_) => return Err(format!("{keys:?} must be a single key")),
                Err(e) => return Err(format!("{keys:?}: {e}")),
            }
        }
    }
    Ok(())
}

const CONFIRM_QUIT: &[&str] = &["always", "multiple-tabs", "downloads", "never"];

fn confirm_quit(value: &Value) -> Result<(), String> {
    let Value::List(values) = value else {
        return Ok(());
    };
    match values.iter().find(|v| !CONFIRM_QUIT.contains(&v.as_str())) {
        Some(v) => Err(format!(
            "unknown value {v:?}; use {}",
            CONFIRM_QUIT.join(", ")
        )),
        None => Ok(()),
    }
}

/// Why quitting should be confirmed under `confirm_quit`, given the open
/// tabs and running downloads; `None` to quit right away.
pub fn confirm_quit_reason(values: &[String], tabs: usize, downloads: usize) -> Option<String> {
    let has = |v: &str| values.iter().any(|x| x == v);
    if has("downloads") && downloads > 0 {
        let s = if downloads == 1 {
            "download is"
        } else {
            "downloads are"
        };
        return Some(format!("{downloads} {s} still running"));
    }
    if has("multiple-tabs") && tabs > 1 {
        return Some(format!("{tabs} tabs are open"));
    }
    has("always").then(|| "Quit riptide?".to_string())
}

fn zoom_levels(value: &Value) -> Result<(), String> {
    let Value::List(levels) = value else {
        return Ok(());
    };
    if levels.is_empty() {
        return Err("needs at least one level".into());
    }
    match levels
        .iter()
        .find(|l| crate::zoom::parse_percent(l).is_none())
    {
        Some(bad) => Err(format!("{bad:?} isn't a percentage like 110%")),
        None => Ok(()),
    }
}

/// What `completion.open_categories` can list.
pub const OPEN_CATEGORIES: &[&str] = &[
    "searchengines",
    "quickmarks",
    "bookmarks",
    "history",
    "filesystem",
];

fn open_categories(value: &Value) -> Result<(), String> {
    let Value::List(items) = value else {
        return Ok(());
    };
    match items
        .iter()
        .find(|i| !OPEN_CATEGORIES.contains(&i.as_str()))
    {
        Some(bad) => Err(format!(
            "unknown category {bad:?}; use {}",
            OPEN_CATEGORIES.join(", ")
        )),
        None => Ok(()),
    }
}

fn completion_height(value: &Value) -> Result<(), String> {
    let Value::Str(text) = value else {
        return Ok(());
    };
    match parse_height(text) {
        Some(_) => Ok(()),
        None => Err(format!(
            "{text:?} isn't a number of rows (12) or a percentage of the window (50%)"
        )),
    }
}

/// How tall the completion list may be: rows, or a percentage of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    Rows(usize),
    Percent(f64),
}

pub fn parse_height(text: &str) -> Option<Height> {
    let text = text.trim();
    match text.strip_suffix('%') {
        Some(p) => p
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|p| *p > 0.0 && *p <= 100.0)
            .map(Height::Percent),
        None => text
            .parse::<usize>()
            .ok()
            .filter(|r| *r > 0)
            .map(Height::Rows),
    }
}

fn incdec_segments(value: &Value) -> Result<(), String> {
    const SEGMENTS: &[&str] = &["host", "port", "path", "query", "anchor"];
    let Value::List(items) = value else {
        return Ok(());
    };
    match items.iter().find(|i| !SEGMENTS.contains(&i.as_str())) {
        Some(bad) => Err(format!("unknown part {bad:?}; use {}", SEGMENTS.join(", "))),
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
        "auto_save.interval",
        Kind::Int {
            min: 0,
            max: 3_600_000
        },
        Value::Int(15000),
        "Milliseconds between crash-recovery saves of the open tabs (0 turns them off)"
    ),
    def!(
        "auto_save.session",
        Kind::Bool,
        Value::Bool(false),
        "Save the open tabs as the 'default' session on quit, and restore them at startup"
    ),
    def!(
        "bindings.key_mappings",
        Kind::Map,
        map(&[
            ("<Ctrl-[>", "<Escape>"),
            ("<Ctrl-6>", "<Ctrl-^>"),
            ("<Ctrl-m>", "<Return>"),
            ("<Ctrl-j>", "<Return>"),
            ("<Ctrl-i>", "<Tab>"),
            ("<Shift-Return>", "<Return>"),
        ]),
        "Keys treated as other keys in every mode, before bindings are looked up, e.g. Ctrl-[ as Escape",
        key_mappings
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
        "completion.cmd_history_max_items",
        Kind::Int {
            min: 0,
            max: 100_000
        },
        Value::Int(100),
        "How many command lines Up and Down remember"
    ),
    def!(
        "completion.height",
        Kind::Str,
        s("12"),
        "Height of the completion list: rows (12) or a percentage of the window (50%)",
        completion_height
    ),
    def!(
        "completion.min_chars",
        Kind::Int { min: 0, max: 100 },
        Value::Int(0),
        "Characters to type after a command before its arguments complete"
    ),
    def!(
        "completion.open_categories",
        Kind::List,
        Value::List(OPEN_CATEGORIES.iter().map(|c| c.to_string()).collect()),
        "What :open completes from, in order: searchengines, quickmarks, bookmarks, history, filesystem",
        open_categories
    ),
    def!(
        "completion.show",
        Kind::Enum(&["always", "auto", "never"]),
        s("always"),
        "When to show completions: always, only after pressing Tab (auto), or never"
    ),
    def!(
        "completion.web_history.exclude",
        Kind::List,
        Value::List(Vec::new()),
        "URL globs (e.g. *://*.bank.example/*) that :open never suggests from history"
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
        "confirm_quit",
        Kind::List,
        Value::List(vec!["never".to_string()]),
        "Ask before quitting: always, multiple-tabs (more than one tab open), downloads (downloads still running), or never",
        confirm_quit
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
        "content.cookies.accept",
        Kind::Enum(&["all", "no-3rdparty", "no-unknown-3rdparty", "never"]),
        s("all"),
        "Which cookies sites may set: all, none from other sites (no-3rdparty; no-unknown-3rdparty is the same here), or never"
    ),
    def!(
        "content.cookies.store",
        Kind::Bool,
        Value::Bool(true),
        "Keep cookies after the browser closes; false makes every cookie last only for the session"
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
        "content.headers.user_agent",
        Kind::Str,
        s(""),
        "User agent sent to sites and shown to their scripts; empty for Chromium's own. Can be set per site"
    ),
    def!(
        "content.images",
        Kind::Bool,
        Value::Bool(true),
        "Load images; can be set per site"
    ),
    def!(
        "content.javascript.can_open_tabs_automatically",
        Kind::Bool,
        Value::Bool(false),
        "Let pages open tabs and windows without a click (popups); can be set per site"
    ),
    def!(
        "content.javascript.clipboard",
        Kind::Enum(&["none", "access", "access-paste"]),
        s("access"),
        "What pages may do with the clipboard: nothing, copy with a click (access), or also read it (access-paste); can be set per site"
    ),
    def!(
        "content.javascript.enabled",
        Kind::Bool,
        Value::Bool(true),
        "Run JavaScript on pages; can be set per site"
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
        "content.mute",
        Kind::Bool,
        Value::Bool(false),
        "Mute pages; can be set per site"
    ),
    def!(
        "content.notifications.enabled",
        Kind::Enum(ASK),
        s("ask"),
        "Let sites show notifications: ask, true or false"
    ),
    def!(
        "content.register_protocol_handler",
        Kind::Enum(&["ask", "true", "false"]),
        s("ask"),
        "Let sites register as handlers for links like mailto:; can be set per site"
    ),
    def!(
        "content.tls.certificate_errors",
        Kind::Enum(&["ask", "block", "load-insecurely"]),
        s("ask"),
        "Pages whose TLS certificate isn't trusted: ask, block, or load-insecurely"
    ),
    def!(
        "content.widevine",
        Kind::Bool,
        Value::Bool(false),
        "Allow Widevine DRM: Chromium downloads Google's CDM once (takes effect after a restart)"
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
        "downloads.location.remember",
        Kind::Bool,
        Value::Bool(true),
        "Start the save prompt in the folder the last download went to"
    ),
    def!(
        "downloads.location.suggestion",
        Kind::Enum(&["both", "path", "filename"]),
        s("both"),
        "What the save prompt starts with: the folder and file name (both), the folder (path), or the file name"
    ),
    def!(
        "downloads.open_dispatcher",
        Kind::Str,
        s(""),
        "Program that opens downloads (:download-open); {} is the file, or it's added at the end. Empty for the desktop's default"
    ),
    def!(
        "downloads.remove_finished",
        Kind::Int {
            min: -1,
            max: 86_400_000
        },
        Value::Int(-1),
        "Take finished downloads off the list after this many milliseconds; -1 keeps them"
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
        "fileselect.folder.command",
        Kind::List,
        Value::List(vec![
            "xterm".to_string(),
            "-e".to_string(),
            "ranger".to_string(),
            "--choosedir={}".to_string()
        ]),
        "Program that picks a folder for fileselect.handler = external; {} is the file it writes the path to"
    ),
    def!(
        "fileselect.handler",
        Kind::Enum(&["default", "external"]),
        s("default"),
        "File pickers for upload fields: Chromium's own (default), or the fileselect.*.command programs (external)"
    ),
    def!(
        "fileselect.multiple_files.command",
        Kind::List,
        Value::List(vec![
            "xterm".to_string(),
            "-e".to_string(),
            "ranger".to_string(),
            "--choosefiles={}".to_string()
        ]),
        "Program that picks several files for fileselect.handler = external; {} is the file it writes the paths to, one per line"
    ),
    def!(
        "fileselect.single_file.command",
        Kind::List,
        Value::List(vec![
            "xterm".to_string(),
            "-e".to_string(),
            "ranger".to_string(),
            "--choosefile={}".to_string()
        ]),
        "Program that picks a file for fileselect.handler = external; {} is the file it writes the path to"
    ),
    def!(
        "hints.auto_follow",
        Kind::Enum(&["always", "unique-match", "full-match", "never"]),
        s("unique-match"),
        "When a hint is followed without Return: when one is left (unique-match), only when its label is typed in full (full-match), always, or never"
    ),
    def!(
        "hints.auto_follow_timeout",
        Kind::Int {
            min: 0,
            max: 10_000
        },
        Value::Int(0),
        "Ignore keys for this many milliseconds after following a hint, so extra typing doesn't reach the page"
    ),
    def!(
        "hints.chars",
        Kind::Str,
        s(crate::hints::DEFAULT_HINT_CHARS),
        "Characters used for hint labels",
        hint_chars
    ),
    def!(
        "hints.leave_on_load",
        Kind::Bool,
        Value::Bool(true),
        "Leave hint mode when the page starts loading something new"
    ),
    def!(
        "hints.min_chars",
        Kind::Int { min: 1, max: 5 },
        Value::Int(1),
        "The shortest hint label, in characters"
    ),
    def!(
        "hints.mode",
        Kind::Enum(&["letter", "number"]),
        s("letter"),
        "letter: labels from hints.chars; number: numbered labels, and typing letters filters by text"
    ),
    def!(
        "hints.next_regexes",
        Kind::List,
        Value::List(vec![
            r"\bnext\b".to_string(),
            r"\bmore\b".to_string(),
            r"\bnewer\b".to_string(),
            r"\b[>→≫]\b".to_string(),
            r"\b(>>|»)\b".to_string(),
            r"\bcontinue\b".to_string()
        ]),
        "Link texts ]] follows to the next page, as JavaScript regular expressions (case doesn't matter)"
    ),
    def!(
        "hints.prev_regexes",
        Kind::List,
        Value::List(vec![
            r"\bprev(ious)?\b".to_string(),
            r"\bback\b".to_string(),
            r"\bolder\b".to_string(),
            r"\b[<←≪]\b".to_string(),
            r"\b(<<|«)\b".to_string()
        ]),
        "Link texts [[ follows to the previous page, as JavaScript regular expressions (case doesn't matter)"
    ),
    def!(
        "hints.scatter",
        Kind::Bool,
        Value::Bool(true),
        "Spread hint labels over the alphabet so neighbours differ; false labels in order"
    ),
    def!(
        "hints.selectors",
        Kind::Map,
        map(crate::hints::DEFAULT_SELECTORS),
        "Hint groups for :hint, as CSS selector lists; your entries are added to the built-in all, links, images, media and inputs"
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
        "input.insert_mode.auto_load",
        Kind::Bool,
        Value::Bool(false),
        "Enter insert mode when a page focuses a text field by itself, as autofocus does on load"
    ),
    def!(
        "input.insert_mode.leave_on_load",
        Kind::Bool,
        Value::Bool(true),
        "Leave insert mode when a new page starts loading"
    ),
    def!(
        "keyhint.blacklist",
        Kind::List,
        Value::List(Vec::new()),
        "Key chains the key hint popup leaves out, as globs on the whole chain (e.g. g* for every chain starting with g)"
    ),
    def!(
        "keyhint.delay",
        Kind::Int {
            min: 0,
            max: 10_000
        },
        Value::Int(500),
        "How long after a partial key chain the popup listing its continuations appears, in milliseconds"
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
        "Where URLs from a second riptide invocation open"
    ),
    def!(
        "scrolling.smooth",
        Kind::Bool,
        Value::Bool(false),
        "Animate scrolling by keys instead of jumping"
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
        "search.wrap",
        Kind::Bool,
        Value::Bool(true),
        "Go on from the top when a search passes the last match (or from the bottom, searching up)"
    ),
    def!(
        "search.wrap_messages",
        Kind::Bool,
        Value::Bool(true),
        "Say when a search wraps around the page"
    ),
    def!(
        "session.lazy_restore",
        Kind::Bool,
        Value::Bool(false),
        "When restoring a session, load background tabs only when they are first shown"
    ),
    def!(
        "spellcheck.languages",
        Kind::List,
        Value::List(Vec::new()),
        "Spell-check languages such as en-US (empty: off); Chromium downloads each dictionary from Google once"
    ),
    def!(
        "statusbar.position",
        Kind::Enum(&["top", "bottom"]),
        s("bottom"),
        "Where the status bar is"
    ),
    def!(
        "statusbar.show",
        Kind::Enum(&["always", "never", "in-mode"]),
        s("always"),
        "When to show the status bar: always, only while typing a command or answering a prompt (never), or also outside normal mode and while a message is shown (in-mode)"
    ),
    def!(
        "statusbar.widgets",
        Kind::List,
        Value::List(
            [
                "keypress",
                "downloads",
                "muted",
                "zoom",
                "search_match",
                "url",
                "scroll",
                "history",
                "tabs",
                "progress"
            ]
            .map(String::from)
            .to_vec()
        ),
        "What the right side of the status bar shows, in order: keypress, downloads, muted, zoom, search_match, url, scroll, scroll_raw, history, tabs, progress, clock[:strftime format], text:…",
        statusbar_widgets
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
        "tabs.position",
        Kind::Enum(&["top", "bottom", "left", "right"]),
        s("top"),
        "Where the tab bar is; left and right list the tabs vertically"
    ),
    def!(
        "tabs.select_on_remove",
        Kind::Enum(&["next", "prev", "last-used"]),
        s("next"),
        "Which tab to show after closing the current one: the next, the previous, or the one used before"
    ),
    def!(
        "tabs.show",
        Kind::Enum(&["always", "never", "multiple", "switching"]),
        s("always"),
        "When to show the tab bar: always, never, with more than one tab, or briefly after switching tabs"
    ),
    def!(
        "tabs.show_switching_delay",
        Kind::Int {
            min: 0,
            max: 60_000
        },
        Value::Int(800),
        "How long the tab bar stays after switching tabs with tabs.show = switching, in milliseconds"
    ),
    def!(
        "tabs.title.format",
        Kind::Str,
        s("{audio}{index}: {current_title}"),
        "Tab titles; fields: {index}, {aligned_index}, {current_title}, {current_url}, {host}, {perc}, {audio}, {private}"
    ),
    def!(
        "tabs.title.format_pinned",
        Kind::Str,
        s("{index}"),
        "Titles of pinned tabs while tabs.pinned.shrink shrinks them; same fields as tabs.title.format"
    ),
    def!(
        "tabs.tooltips",
        Kind::Bool,
        Value::Bool(true),
        "Show a tab's title and URL when the mouse rests on it"
    ),
    def!(
        "tabs.undo_stack_size",
        Kind::Int {
            min: 0,
            max: 10_000
        },
        Value::Int(100),
        "How many closed tabs u can reopen; 0 keeps none"
    ),
    def!(
        "tabs.width",
        Kind::Int { min: 50, max: 1000 },
        Value::Int(200),
        "Width of the tab bar in pixels when tabs.position is left or right"
    ),
    def!(
        "tabs.wrap",
        Kind::Bool,
        Value::Bool(true),
        "Wrap around from the last tab to the first (and back) when switching tabs"
    ),
    def!(
        "url.auto_search",
        Kind::Enum(&["naive", "schemeless", "never"]),
        s("naive"),
        "When :open searches: text that doesn't look like an address (naive), anything without a scheme:// (schemeless), or never"
    ),
    def!(
        "url.default_page",
        Kind::Str,
        s(crate::url::DEFAULT_START_PAGE),
        "Page for :open without a URL"
    ),
    def!(
        "url.incdec_segments",
        Kind::List,
        Value::List(vec!["path".to_string(), "query".to_string()]),
        "Parts of the URL Ctrl-a and Ctrl-x change: host, port, path, query, anchor",
        incdec_segments
    ),
    def!(
        "url.open_base_url",
        Kind::Bool,
        Value::Bool(false),
        "Open a search engine's home page when :open gets just its name"
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
        "url.yank_ignored_parameters",
        Kind::List,
        Value::List(
            [
                "ref",
                "utm_source",
                "utm_medium",
                "utm_campaign",
                "utm_term",
                "utm_content",
                "utm_name",
                "fbclid",
                "gclid"
            ]
            .map(String::from)
            .to_vec()
        ),
        "Query parameters dropped when yanking a URL, such as tracking tags"
    ),
    def!(
        "window.title_format",
        Kind::Str,
        s("{current_title}{title_sep}Riptide"),
        "Window title; fields: {current_title}, {title_sep}, {current_url}, {host}, {mode}"
    ),
    def!(
        "zoom.default",
        Kind::Int { min: 25, max: 500 },
        Value::Int(100),
        "Zoom in percent for pages, and what :zoom without a value resets to"
    ),
    def!(
        "zoom.levels",
        Kind::List,
        Value::List(
            crate::zoom::LEVELS
                .iter()
                .map(|l| format!("{l}%"))
                .collect()
        ),
        "The zoom levels + and - step through, in percent",
        zoom_levels
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
    "content.headers.user_agent",
    "content.images",
    "content.javascript.can_open_tabs_automatically",
    "content.javascript.clipboard",
    "content.javascript.enabled",
    "content.mute",
    "content.register_protocol_handler",
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
    pub fn unset(&mut self, name: &str) -> Result<(), String> {
        let def = find(name).ok_or_else(|| format!("No option {name:?}"))?;
        self.set(name, def.default_value())
    }

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
    /// Settings whose value differs from the default, sorted by name.
    pub fn changed(&self) -> Vec<(&'static str, Value)> {
        SETTINGS
            .iter()
            .filter_map(|def| {
                let value = self.values.get(def.name)?;
                (*value != def.default_value()).then(|| (def.name, value.clone()))
            })
            .collect()
    }

    /// Every per-site value, as (pattern, setting, value), in the order set.
    pub fn all_overrides(&self) -> Vec<(String, &'static str, Value)> {
        self.per_domain.clone()
    }

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
        let widgets = find("statusbar.widgets").unwrap();
        assert!(
            widgets
                .from_json(&json!(["url", "clock:%H:%M", "text:hi"]))
                .is_ok()
        );
        assert!(widgets.from_json(&json!(["url", "weather"])).is_err());
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

    #[test]
    fn confirm_quit_reasons() {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(confirm_quit_reason(&v(&["never"]), 9, 9), None);
        assert_eq!(confirm_quit_reason(&v(&["multiple-tabs"]), 1, 0), None);
        assert_eq!(
            confirm_quit_reason(&v(&["multiple-tabs"]), 3, 0).unwrap(),
            "3 tabs are open"
        );
        assert_eq!(
            confirm_quit_reason(&v(&["multiple-tabs", "downloads"]), 3, 1).unwrap(),
            "1 download is still running"
        );
        assert!(confirm_quit_reason(&v(&["always"]), 1, 0).is_some());
        assert!(
            find("confirm_quit")
                .unwrap()
                .from_json(&json!(["sometimes"]))
                .is_err()
        );
    }

    #[test]
    fn completion_heights() {
        assert_eq!(parse_height("12"), Some(Height::Rows(12)));
        assert_eq!(parse_height(" 50% "), Some(Height::Percent(50.0)));
        assert_eq!(parse_height("0"), None);
        assert_eq!(parse_height("150%"), None);
        assert!(
            find("completion.height")
                .unwrap()
                .from_json(&json!("tall"))
                .is_err()
        );
        assert!(
            find("completion.open_categories")
                .unwrap()
                .from_json(&json!(["history", "web"]))
                .is_err()
        );
    }
}
