//! Data for the `riptide://settings` page: every setting with what the page
//! needs to show and edit it.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::keymap::Keymap;
use crate::mode::Mode;
use crate::settings::{Kind, RESTART_REQUIRED, SETTINGS, Settings};

/// What the page draws: every setting, and the theme's colors for the
/// colors preview and the swatches of colors left to the theme.
#[derive(Serialize, Debug)]
pub struct Page {
    pub entries: Vec<Entry>,
    /// `--rt-<token>` values, as the bars get them.
    pub theme: BTreeMap<&'static str, String>,
    /// The Keys tab: each mode's bindings.
    pub keys: Vec<ModeKeys>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ModeKeys {
    pub mode: &'static str,
    pub bindings: Vec<Binding>,
    /// Default bindings that were unbound, as `[keys, command]`, to restore.
    pub removed: Vec<(String, String)>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Binding {
    /// In riptide's notation, e.g. `<Ctrl-d>` or `gg`.
    pub keys: String,
    pub command: String,
    /// The default command for these keys, if there is one.
    pub default: Option<String>,
}

/// Every mode's bindings, compared with the defaults.
fn keys(keymap: &Keymap) -> Vec<ModeKeys> {
    let defaults = Keymap::defaults();
    Mode::ALL
        .iter()
        .map(|&mode| {
            let default: BTreeMap<String, String> = defaults.bindings(mode).into_iter().collect();
            let now = keymap.bindings(mode);
            let removed = default
                .iter()
                .filter(|(k, _)| !now.iter().any(|(n, _)| n == *k))
                .map(|(k, c)| (k.clone(), c.clone()))
                .collect();
            ModeKeys {
                mode: mode.name(),
                bindings: now
                    .into_iter()
                    .map(|(keys, command)| Binding {
                        default: default.get(&keys).cloned(),
                        keys,
                        command,
                    })
                    .collect(),
                removed,
            }
        })
        .collect()
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Entry {
    pub name: &'static str,
    /// The part of the name before the first `.`, e.g. `tabs`.
    pub section: &'static str,
    pub description: &'static str,
    /// How the page edits it: `bool`, `int` (with `min`, `max`), `enum` (with
    /// `options`), `color`, `str`, `list` or `map`.
    pub editor: Json,
    pub value: Json,
    pub default: Json,
    /// Where the value came from, e.g. `config.toml`; empty for the default.
    pub source: String,
    /// A config file sets it, so that value wins at the next start.
    pub config_wins: bool,
    pub restart: bool,
    /// Per-site values, as `[pattern, value]`.
    pub sites: Vec<(String, Json)>,
}

/// Every setting, in name order. `sources` names where each changed setting
/// was last set, and `overridden` lists those a config file sets.
pub fn build(
    settings: &Settings,
    keymap: &Keymap,
    sources: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
) -> Page {
    Page {
        entries: entries(settings, sources, overridden),
        theme: crate::theme::resolve(settings),
        keys: keys(keymap),
    }
}

fn entries(
    settings: &Settings,
    sources: &BTreeMap<String, String>,
    overridden: &BTreeSet<String>,
) -> Vec<Entry> {
    let mut entries: Vec<Entry> = SETTINGS
        .iter()
        .map(|def| {
            let editor = match def.kind {
                Kind::Bool => json!({ "type": "bool" }),
                Kind::Int { min, max } => json!({ "type": "int", "min": min, "max": max }),
                Kind::Enum(options) => json!({ "type": "enum", "options": options }),
                Kind::Str if def.name.starts_with("colors.") => {
                    let token = crate::theme::TOKENS
                        .iter()
                        .find(|(_, setting)| *setting == def.name)
                        .map(|(token, _)| *token);
                    json!({ "type": "color", "token": token })
                }
                Kind::Str if theme_setting(def.name) => {
                    let mut names = crate::theme::names();
                    if def.name != "ui.theme" {
                        names.retain(|n| n != "auto");
                    }
                    json!({ "type": "enum", "options": names })
                }
                Kind::Str => json!({ "type": "str" }),
                Kind::List => json!({ "type": "list" }),
                Kind::Map => json!({ "type": "map" }),
            };
            let value = settings.get(def.name).map_or(Json::Null, |v| v.to_json());
            let default = def.default_value().to_json();
            let source = if value == default {
                String::new()
            } else {
                sources.get(def.name).cloned().unwrap_or_default()
            };
            Entry {
                name: def.name,
                section: def.name.split('.').next().unwrap_or(def.name),
                description: def.description,
                editor,
                value,
                default,
                source,
                config_wins: overridden.contains(def.name),
                restart: RESTART_REQUIRED.contains(&def.name),
                sites: settings
                    .overrides(def.name)
                    .into_iter()
                    .map(|(pattern, value)| (pattern, value.to_json()))
                    .collect(),
            }
        })
        .collect();
    entries.sort_by_key(|e| e.name);
    entries
}

fn theme_setting(name: &str) -> bool {
    name == "ui.theme" || name.starts_with("ui.auto_theme.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;

    fn entry<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
        entries.iter().find(|e| e.name == name).unwrap()
    }

    #[test]
    fn every_setting_with_its_editor_value_and_source() {
        let mut settings = Settings::default();
        settings
            .set("hints.chars", Value::Str("abc".into()))
            .unwrap();
        settings
            .set_for(
                "*.example.com",
                "content.javascript.enabled",
                Value::Bool(false),
            )
            .unwrap();
        let sources = BTreeMap::from([("hints.chars".to_string(), "config.toml".to_string())]);
        let overridden = BTreeSet::from(["hints.chars".to_string()]);
        let page = build(&settings, &Keymap::defaults(), &sources, &overridden);
        assert_eq!(page.theme["statusbar-bg"], "#061826");
        let entries = page.entries;
        assert_eq!(entries.len(), SETTINGS.len());
        assert!(entries.windows(2).all(|w| w[0].name < w[1].name));

        let chars = entry(&entries, "hints.chars");
        assert_eq!(chars.section, "hints");
        assert_eq!(chars.value, json!("abc"));
        assert_eq!(chars.source, "config.toml");
        assert!(chars.config_wins);

        let uppercase = entry(&entries, "hints.uppercase");
        assert_eq!(uppercase.editor, json!({ "type": "bool" }));
        assert_eq!(uppercase.source, "");
        assert!(!uppercase.config_wins);

        assert_eq!(
            entry(&entries, "colors.hints.bg").editor,
            json!({ "type": "color", "token": "hints-bg" })
        );
        assert_eq!(entry(&entries, "prompt.position").editor["type"], "enum");
        assert_eq!(entry(&entries, "prompt.width").editor["min"], 200);
        let themes = &entry(&entries, "ui.theme").editor;
        assert_eq!(themes["type"], "enum");
        assert_eq!(themes["options"][0], "auto");
        assert_ne!(
            entry(&entries, "ui.auto_theme.dark").editor["options"][0],
            "auto"
        );

        let js = entry(&entries, "content.javascript.enabled");
        assert_eq!(js.sites, vec![("*.example.com".to_string(), json!(false))]);
    }

    #[test]
    fn keys_show_changed_added_and_removed_bindings() {
        let mut keymap = Keymap::defaults();
        keymap
            .bind(Mode::Normal, "gg", "scroll-to-perc 50")
            .unwrap();
        keymap.bind(Mode::Normal, "<Ctrl-y>", "reload").unwrap();
        keymap.unbind(Mode::Normal, "d").unwrap();
        let modes = keys(&keymap);
        assert_eq!(modes.len(), Mode::ALL.len());
        let normal = modes.iter().find(|m| m.mode == "normal").unwrap();
        let find = |k: &str| normal.bindings.iter().find(|b| b.keys == k).unwrap();
        assert_eq!(find("gg").command, "scroll-to-perc 50");
        assert_eq!(find("gg").default.as_deref(), Some("scroll-to-perc 0"));
        assert_eq!(find("<Ctrl-y>").default, None);
        assert!(normal.bindings.iter().all(|b| b.keys != "d"));
        assert!(normal.removed.iter().any(|(k, _)| k == "d"));
    }
}
