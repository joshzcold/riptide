//! `config.toml` and `autoconfig.toml`:
//!
//! ```toml
//! hints.chars = "asdfjkl"          # any qutebrowser-style setting name
//! url.searchengines = { DEFAULT = "https://duckduckgo.com/?q={}", w = "https://en.wikipedia.org/?search={}" }
//!
//! [bindings.normal]
//! "<Ctrl-x>" = "quit"
//! d = ""                           # an empty command unbinds the key
//!
//! [per_domain."https://meet.example.com"]
//! "content.media.video_capture" = "true"
//! ```

use hb_core::Mode;
use hb_core::config::ConfigOp;
use hb_core::key::Key;
use hb_core::settings;

/// Parse TOML text into config operations. Errors are collected so one bad
/// line doesn't discard the rest of the file.
pub fn parse(text: &str, source: &str) -> (Vec<ConfigOp>, Vec<String>) {
    let mut ops = Vec::new();
    let mut errors = Vec::new();
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(e) => {
            errors.push(format!("{source}: {e}"));
            return (ops, errors);
        }
    };
    for (key, value) in &table {
        if key == "bindings" {
            parse_bindings(value, source, &mut ops, &mut errors);
        } else if key == "per_domain" {
            parse_per_domain(value, source, &mut ops, &mut errors);
        } else {
            parse_setting(key, value, source, &mut ops, &mut errors);
        }
    }
    (ops, errors)
}

/// `[per_domain."<pattern>"]` tables: settings for matching pages.
fn parse_per_domain(
    value: &toml::Value,
    source: &str,
    ops: &mut Vec<ConfigOp>,
    errors: &mut Vec<String>,
) {
    let Some(patterns) = value.as_table() else {
        return errors.push(format!(
            "{source}: per_domain must be a table of URL patterns"
        ));
    };
    for (pattern, table) in patterns {
        let Some(table) = table.as_table() else {
            errors.push(format!(
                "{source}: per_domain.{pattern:?} must be a table of settings"
            ));
            continue;
        };
        for (key, value) in table {
            let mut found = Vec::new();
            parse_setting(key, value, source, &mut found, errors);
            for op in found {
                let ConfigOp::Set { name, value } = op else {
                    continue;
                };
                if settings::PER_DOMAIN.contains(&name.as_str()) {
                    ops.push(ConfigOp::SetFor {
                        pattern: pattern.clone(),
                        name,
                        value,
                    });
                } else {
                    errors.push(format!("{source}: {name} can't be set per site"));
                }
            }
        }
    }
}

/// Walk nested tables until the dotted path names a known setting, so both
/// `hints.chars = …` and `[hints] chars = …` work.
fn parse_setting(
    path: &str,
    value: &toml::Value,
    source: &str,
    ops: &mut Vec<ConfigOp>,
    errors: &mut Vec<String>,
) {
    if let Some(def) = settings::find(path) {
        let result = serde_json::to_value(value)
            .map_err(|e| e.to_string())
            .and_then(|json| def.from_json(&json));
        match result {
            Ok(value) => ops.push(ConfigOp::Set {
                name: def.name.to_string(),
                value,
            }),
            Err(e) => errors.push(format!("{source}: {e}")),
        }
        return;
    }
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                parse_setting(&format!("{path}.{key}"), value, source, ops, errors);
            }
        }
        _ => errors.push(format!("{source}: no option {path:?}")),
    }
}

fn parse_bindings(
    value: &toml::Value,
    source: &str,
    ops: &mut Vec<ConfigOp>,
    errors: &mut Vec<String>,
) {
    let Some(modes) = value.as_table() else {
        errors.push(format!(
            "{source}: [bindings] must contain [bindings.<mode>] tables"
        ));
        return;
    };
    for (mode_name, table) in modes {
        let mode = match mode_name.parse::<Mode>() {
            Ok(mode) => mode,
            Err(e) => {
                errors.push(format!("{source}: bindings.{mode_name}: {e}"));
                continue;
            }
        };
        let Some(table) = table.as_table() else {
            errors.push(format!(
                "{source}: bindings.{mode_name} must be a table of key = \"command\""
            ));
            continue;
        };
        for (keys, command) in table {
            let Some(command) = command.as_str() else {
                errors.push(format!(
                    "{source}: bindings.{mode_name}.{keys}: the command must be a string"
                ));
                continue;
            };
            if let Err(e) = Key::parse_sequence(keys) {
                errors.push(format!("{source}: bindings.{mode_name}: {e}"));
                continue;
            }
            ops.push(if command.trim().is_empty() {
                ConfigOp::Unbind {
                    mode,
                    keys: keys.clone(),
                }
            } else {
                ConfigOp::Bind {
                    mode,
                    keys: keys.clone(),
                    command: command.to_string(),
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hb_core::settings::Value;

    #[test]
    fn dotted_and_nested_settings() {
        let (ops, errors) = parse(
            r#"
            hints.chars = "qwer"
            [tabs]
            last_close = "blank"
            "#,
            "t",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            ops,
            vec![
                ConfigOp::Set {
                    name: "hints.chars".into(),
                    value: Value::Str("qwer".into())
                },
                ConfigOp::Set {
                    name: "tabs.last_close".into(),
                    value: Value::Str("blank".into())
                },
            ]
        );
    }

    #[test]
    fn map_settings_are_not_walked_into() {
        let (ops, errors) = parse(
            r#"url.searchengines = { DEFAULT = "https://x.org/?q={}", g = "https://g.co/?q={}" }"#,
            "t",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            matches!(&ops[0], ConfigOp::Set { name, value: Value::Map(m) } if name == "url.searchengines" && m.len() == 2)
        );
    }

    #[test]
    fn bindings_and_unbinds() {
        let (ops, errors) = parse(
            r#"
            [bindings.normal]
            "<Ctrl-x>" = "quit"
            d = ""
            [bindings.insert]
            "<Ctrl-e>" = "mode-leave"
            "#,
            "t",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(ops.len(), 3);
        assert!(ops.contains(&ConfigOp::Unbind {
            mode: Mode::Normal,
            keys: "d".into()
        }));
    }

    #[test]
    fn collects_errors_and_keeps_good_lines() {
        let (ops, errors) = parse(
            r#"
            hints.chars = "x"
            nope.option = 1
            hints.uppercase = true
            [bindings.sideways]
            a = "quit"
            [bindings.normal]
            "<Bogus>" = "quit"
            "#,
            "config.toml",
        );
        assert_eq!(ops.len(), 1, "{ops:?}");
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.iter().all(|e| e.starts_with("config.toml: ")));
    }

    #[test]
    fn per_domain_tables() {
        let (ops, errors) = parse(
            "[per_domain.\"*.example.com\"]\n\"content.geolocation\" = \"true\"\nhints.chars = \"ab\"\n",
            "t.toml",
        );
        assert_eq!(
            ops,
            [ConfigOp::SetFor {
                pattern: "*.example.com".into(),
                name: "content.geolocation".into(),
                value: hb_core::settings::Value::Str("true".into()),
            }]
        );
        assert_eq!(errors, ["t.toml: hints.chars can't be set per site"]);
    }

    #[test]
    fn syntax_errors_mention_the_file() {
        let (ops, errors) = parse("hints.chars = ", "config.toml");
        assert!(ops.is_empty());
        assert!(errors[0].starts_with("config.toml: "), "{errors:?}");
    }
}
