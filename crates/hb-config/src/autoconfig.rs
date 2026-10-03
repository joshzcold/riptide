//! `autoconfig.toml`: where `:set`, `:bind` and `:unbind` persist changes, so
//! the user's own config files are never rewritten.

use std::path::{Path, PathBuf};

use hb_core::config::ConfigOp;

const HEADER: &str = "\
# Written by hackers-browser when you use :set, :bind or :unbind.
# Loaded before config.toml and config.lua, which override it.
# Prefer editing those files; changes here may be overwritten.

";

pub struct AutoConfig {
    path: PathBuf,
    table: toml::Table,
}

impl AutoConfig {
    /// Read the file if it exists, returning its operations and any errors.
    pub fn load(path: &Path) -> (Self, Vec<ConfigOp>, Vec<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return (
                    Self::empty(path),
                    Vec::new(),
                    vec![format!("{}: {e}", path.display())],
                );
            }
        };
        let (ops, errors) = crate::toml_file::parse(&text, &path.display().to_string());
        let table = text.parse().unwrap_or_default();
        (
            Self {
                path: path.to_path_buf(),
                table,
            },
            ops,
            errors,
        )
    }

    fn empty(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            table: toml::Table::new(),
        }
    }

    pub fn record(&mut self, op: &ConfigOp) {
        match op {
            ConfigOp::Set { name, value } => {
                // Quoted flat keys ("hints.chars") avoid clashing with nested tables.
                if let Ok(value) = toml::Value::try_from(value.to_json()) {
                    self.table.insert(name.clone(), value);
                }
            }
            ConfigOp::SetFor {
                pattern,
                name,
                value,
            } => {
                let Ok(value) = toml::Value::try_from(value.to_json()) else {
                    return;
                };
                let per_domain = self
                    .table
                    .entry("per_domain")
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
                if let Some(site) = per_domain.as_table_mut().map(|t| {
                    t.entry(pattern.clone())
                        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                }) && let Some(site) = site.as_table_mut()
                {
                    site.insert(name.clone(), value);
                }
            }
            ConfigOp::Bind {
                mode,
                keys,
                command,
            } => self.binding(mode.name(), keys, command),
            ConfigOp::Unbind { mode, keys } => self.binding(mode.name(), keys, ""),
            ConfigOp::Unset { name } => {
                self.table.remove(name);
            }
        }
    }

    fn binding(&mut self, mode: &str, keys: &str, command: &str) {
        let bindings = self
            .table
            .entry("bindings")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(bindings) = bindings.as_table_mut() else {
            return;
        };
        let modes = bindings
            .entry(mode)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(modes) = modes.as_table_mut() {
            modes.insert(keys.to_string(), toml::Value::String(command.to_string()));
        }
    }

    /// Write atomically so a crash never leaves a half-written file.
    pub fn save(&self) -> Result<(), String> {
        let body = toml::to_string(&self.table).map_err(|e| e.to_string())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, format!("{HEADER}{body}"))
            .map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hb_core::Mode;
    use hb_core::settings::Value;

    #[test]
    fn per_site_values_round_trip() {
        let dir = std::env::temp_dir().join(format!("hb-autoconfig-site-{}", std::process::id()));
        let path = dir.join("autoconfig.toml");
        let (mut auto, _, _) = AutoConfig::load(&path);
        let op = ConfigOp::SetFor {
            pattern: "https://meet.example".into(),
            name: "content.media.audio_capture".into(),
            value: Value::Str("true".into()),
        };
        auto.record(&op);
        auto.save().unwrap();
        let (_, ops, errors) = AutoConfig::load(&path);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(ops, [op]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("hb-autoconfig-test-{}", std::process::id()));
        let path = dir.join("autoconfig.toml");
        let (mut auto, ops, errors) = AutoConfig::load(&path);
        assert!(ops.is_empty() && errors.is_empty());
        let changes = vec![
            ConfigOp::Set {
                name: "hints.chars".into(),
                value: Value::Str("qwer".into()),
            },
            ConfigOp::Set {
                name: "url.start_pages".into(),
                value: Value::List(vec!["about:blank".into()]),
            },
            ConfigOp::Bind {
                mode: Mode::Normal,
                keys: "<Ctrl-x>".into(),
                command: "quit".into(),
            },
            ConfigOp::Unbind {
                mode: Mode::Normal,
                keys: "d".into(),
            },
        ];
        for op in &changes {
            auto.record(op);
        }
        auto.save().unwrap();
        let (_, ops, errors) = AutoConfig::load(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        for op in &changes {
            assert!(ops.contains(op), "missing {op:?} in {ops:?}");
        }
    }
}
