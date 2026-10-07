//! Data for the `riptide://settings` page: every setting with what the page
//! needs to show and edit it.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::settings::{Kind, RESTART_REQUIRED, SETTINGS, Settings};

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
                Kind::Str if def.name.starts_with("colors.") => json!({ "type": "color" }),
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
        let entries = build(&settings, &sources, &overridden);
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

        assert_eq!(entry(&entries, "colors.hints.bg").editor["type"], "color");
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
}
