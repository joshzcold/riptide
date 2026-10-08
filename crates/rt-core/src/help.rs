//! Data for the `riptide://help/` page, built from the live command, setting and
//! binding registries so the help always matches what the browser does.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::command::COMMANDS;
use crate::keymap::Keymap;
use crate::mode::Mode;
use crate::settings::{Kind, SETTINGS, Settings};

#[derive(Debug, Serialize)]
pub struct HelpData {
    pub version: String,
    pub commands: Vec<CommandHelp>,
    pub settings: Vec<SettingHelp>,
    pub modes: Vec<ModeHelp>,
    /// `(label, value)` rows for the version section.
    pub info: Vec<(String, String)>,
    /// The Lua API, from the type stubs.
    pub lua: Vec<serde_json::Value>,
    /// The plugins `config.lua` adds, with their README.
    pub plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CommandHelp {
    pub name: String,
    pub description: String,
    /// Normal-mode keys whose binding starts with this command.
    pub keys: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingHelp {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: String,
    pub default: String,
    pub value: String,
    /// Where the current value came from: "default", "config.toml", ….
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ModeHelp {
    pub name: &'static str,
    pub bindings: Vec<BindingHelp>,
    /// Default bindings the user removed.
    pub removed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BindingHelp {
    pub keys: String,
    pub command: String,
    /// Added or changed by the user's config.
    pub changed: bool,
}

pub fn kind_description(kind: Kind) -> String {
    match kind {
        Kind::Bool => "true or false".into(),
        Kind::Int { min, max } => format!("integer, {min} to {max}"),
        Kind::Str => "string".into(),
        Kind::Enum(options) => format!("one of: {}", options.join(", ")),
        Kind::List => "list of strings".into(),
        Kind::Map => "table of strings".into(),
    }
}

/// The command a binding runs, for matching it to `COMMANDS`. Bindings like
/// `o` → `cmd-set-text -s :open` count for the command they prefill.
fn command_name(binding: &str) -> &str {
    let mut words = binding.trim_start_matches(':').split_whitespace();
    let first = words.next().unwrap_or_default();
    if first != "cmd-set-text" {
        return first;
    }
    words
        .find(|w| !w.starts_with('-'))
        .map(|w| w.trim_start_matches(':'))
        .unwrap_or(first)
}

/// `user_commands` are the ones `config.lua` defined, as `(name, description)`.
pub fn build(
    keymap: &Keymap,
    settings: &Settings,
    sources: &HashMap<String, String>,
    info: Vec<(String, String)>,
    user_commands: &[(String, String)],
) -> HelpData {
    let defaults = Keymap::defaults();
    let normal = keymap.bindings(Mode::Normal);
    let keys_for = |name: &str| -> Vec<String> {
        normal
            .iter()
            .filter(|(_, cmd)| command_name(cmd) == name)
            .map(|(keys, _)| keys.clone())
            .collect()
    };
    let mut commands: Vec<CommandHelp> = COMMANDS
        .iter()
        .filter(|c| !c.hidden)
        .map(|c| CommandHelp {
            name: c.name.to_string(),
            description: c.description.to_string(),
            keys: keys_for(c.name),
        })
        .collect();
    commands.extend(user_commands.iter().map(|(name, description)| CommandHelp {
        name: name.clone(),
        description: format!("{description} (config.lua)"),
        keys: keys_for(name),
    }));

    let default_settings = Settings::default();
    let settings = SETTINGS
        .iter()
        .map(|d| {
            let value = settings
                .get(d.name)
                .map(ToString::to_string)
                .unwrap_or_default();
            let default = d.default_value().to_string();
            let source = sources.get(d.name).cloned().unwrap_or_else(|| {
                let same = default_settings
                    .get(d.name)
                    .map(ToString::to_string)
                    .as_deref()
                    == Some(value.as_str());
                if same { "default" } else { "changed" }.to_string()
            });
            SettingHelp {
                name: d.name,
                description: d.description,
                kind: kind_description(d.kind),
                default,
                value,
                source,
            }
        })
        .collect();

    let modes = Mode::ALL
        .iter()
        .map(|&mode| {
            let current = keymap.bindings(mode);
            let default: BTreeMap<String, String> = defaults.bindings(mode).into_iter().collect();
            let bindings = current
                .iter()
                .map(|(keys, command)| BindingHelp {
                    keys: keys.clone(),
                    command: command.clone(),
                    changed: default.get(keys) != Some(command),
                })
                .collect();
            let current_keys: BTreeMap<&String, ()> =
                current.iter().map(|(k, _)| (k, ())).collect();
            let removed = default
                .keys()
                .filter(|k| !current_keys.contains_key(k))
                .cloned()
                .collect();
            ModeHelp {
                name: mode.name(),
                bindings,
                removed,
            }
        })
        .collect();

    HelpData {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commands,
        settings,
        modes,
        info,
        lua: Vec::new(),
        plugins: Vec::new(),
    }
}

/// The page anchor for `:help <topic>`: `:open` → a command, `hints.chars`
/// → a setting, anything else → a section. `None` means the top.
pub fn anchor(topic: Option<&str>) -> Result<Option<String>, String> {
    let Some(topic) = topic.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if let Some(command) = topic.strip_prefix(':') {
        return COMMANDS
            .iter()
            .any(|c| c.name == command && !c.hidden)
            .then(|| format!("cmd-{command}"))
            .map(Some)
            .ok_or_else(|| format!("No command :{command}"));
    }
    if SETTINGS.iter().any(|d| d.name == topic) {
        return Ok(Some(format!("set-{topic}")));
    }
    match topic {
        "commands" | "settings" | "bindings" | "modes" | "hints" | "config" | "lua" | "lua-api"
        | "plugins" | "version" => Ok(Some(topic.to_string())),
        _ => Err(format!(
            "No help for {topic:?} (try a :command, a setting, rt.<function>, a plugin or a section)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;

    #[test]
    fn covers_every_visible_command_and_setting() {
        let data = build(
            &Keymap::defaults(),
            &Settings::default(),
            &HashMap::new(),
            Vec::new(),
            &[],
        );
        let visible = COMMANDS.iter().filter(|c| !c.hidden).count();
        assert_eq!(data.commands.len(), visible);
        assert_eq!(data.settings.len(), SETTINGS.len());
        assert!(data.settings.iter().all(|s| s.source == "default"));
        let open = data.commands.iter().find(|c| c.name == "open").unwrap();
        for key in ["o", "O", "go", "gO", "<Ctrl-t>"] {
            assert!(
                open.keys.contains(&key.to_string()),
                "{key} not in {:?}",
                open.keys
            );
        }
        assert!(
            data.modes
                .iter()
                .all(|m| m.bindings.iter().all(|b| !b.changed) && m.removed.is_empty())
        );
    }

    #[test]
    fn marks_user_changes() {
        let mut keymap = Keymap::defaults();
        keymap.bind(Mode::Normal, "X", "reload").unwrap();
        keymap.bind(Mode::Normal, "j", "scroll up").unwrap();
        keymap.unbind(Mode::Normal, "d").unwrap();
        let mut settings = Settings::default();
        settings
            .set("hints.chars", Value::Str("qw".into()))
            .unwrap();
        settings.set("hints.uppercase", Value::Bool(true)).unwrap();
        let sources = HashMap::from([("hints.chars".to_string(), "config.lua".to_string())]);
        let data = build(
            &keymap,
            &settings,
            &sources,
            Vec::new(),
            &[("wiki".into(), "Look it up".into())],
        );
        assert!(
            data.commands
                .iter()
                .any(|c| c.name == "wiki" && c.description.contains("config.lua"))
        );

        let normal = data.modes.iter().find(|m| m.name == "normal").unwrap();
        let changed: Vec<&str> = normal
            .bindings
            .iter()
            .filter(|b| b.changed)
            .map(|b| b.keys.as_str())
            .collect();
        assert_eq!(changed, ["X", "j"]);
        assert_eq!(normal.removed, ["d"]);
        let setting = |n: &str| data.settings.iter().find(|s| s.name == n).unwrap();
        assert_eq!(setting("hints.chars").source, "config.lua");
        assert_eq!(setting("hints.chars").value, "qw");
        assert_eq!(setting("hints.uppercase").source, "changed");
        assert_eq!(setting("hints.uppercase").kind, "true or false");
    }

    #[test]
    fn anchors() {
        assert_eq!(anchor(None), Ok(None));
        assert_eq!(anchor(Some(":open")), Ok(Some("cmd-open".into())));
        assert_eq!(
            anchor(Some("hints.chars")),
            Ok(Some("set-hints.chars".into()))
        );
        assert_eq!(anchor(Some("lua")), Ok(Some("lua".into())));
        assert!(anchor(Some(":nope")).is_err());
        assert!(anchor(Some(":rl-rubout")).is_err());
        assert!(anchor(Some("nonsense")).is_err());
    }
}
