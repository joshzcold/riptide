//! `riptide --plugin-test DIR`: a throwaway profile whose config.lua loads the
//! plugin in DIR, with the permissions its manifest asks for already
//! approved, and runs its `test/*_spec.lua` files.

use std::path::{Path, PathBuf};

use crate::plugins::{Locked, Lockfile, Manifest};

const RUNNER: &str = include_str!("test_runner.lua");

/// How long one test may take.
const TEST_TIMEOUT_MS: u32 = 10_000;

/// `text` as a Lua string literal.
fn lua_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\{}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The spec files to run: `test/*_spec.lua`, in name order.
pub fn specs(plugin: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = plugin.join("test");
    let none = || format!("no test/*_spec.lua files in {}", plugin.display());
    let mut specs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|_| none())?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_spec.lua"))
        })
        .collect();
    specs.sort();
    if specs.is_empty() {
        return Err(none());
    }
    Ok(specs)
}

/// Fill `basedir` with a config that tests the plugin in `plugin`.
pub fn prepare(plugin: &Path, basedir: &Path) -> Result<(), String> {
    let plugin = plugin
        .canonicalize()
        .map_err(|e| format!("{}: {e}", plugin.display()))?;
    let specs = specs(&plugin)?;
    let manifest = Manifest::read(&plugin)?;
    let name = manifest
        .name
        .clone()
        .unwrap_or_else(|| crate::plugins::name_from(&plugin.display().to_string()));
    let config_dir = basedir.join("config");
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    // The author's own plugin: what it asks for is approved, so nothing asks.
    let mut lock = Lockfile::default();
    lock.plugins.insert(
        name.clone(),
        Locked {
            approved: manifest.permissions,
            ..Locked::default()
        },
    );
    lock.save(&config_dir)?;
    let specs: Vec<String> = specs
        .iter()
        .map(|s| lua_string(&s.display().to_string()))
        .collect();
    let config = format!(
        "plugin_test = {{ dir = {}, name = {}, specs = {{ {} }}, timeout = {TEST_TIMEOUT_MS} }}\n{RUNNER}",
        lua_string(&plugin.display().to_string()),
        lua_string(&name),
        specs.join(", "),
    );
    std::fs::write(config_dir.join("config.lua"), config).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_a_profile_that_approves_the_plugin_and_lists_its_specs() {
        let root = std::env::temp_dir().join(format!("rt-plugin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let plugin = root.join("my-plugin");
        std::fs::create_dir_all(plugin.join("test")).unwrap();
        std::fs::write(
            plugin.join("riptide-plugin.toml"),
            "[permissions]\nspawn = true\n",
        )
        .unwrap();
        std::fs::write(plugin.join("test/b_spec.lua"), "").unwrap();
        std::fs::write(plugin.join("test/a_spec.lua"), "").unwrap();
        std::fs::write(plugin.join("test/helper.lua"), "").unwrap();

        let base = root.join("base");
        prepare(&plugin, &base).unwrap();
        let lock = Lockfile::load(&base.join("config")).unwrap();
        assert!(lock.plugins["my-plugin"].approved.spawn);
        let config = std::fs::read_to_string(base.join("config/config.lua")).unwrap();
        let a = config.find("a_spec.lua").unwrap();
        let b = config.find("b_spec.lua").unwrap();
        assert!(a < b, "{config}");
        assert!(!config.contains("helper.lua"));
        assert!(config.contains("rt.on(\"startup\""));

        assert!(prepare(&root.join("missing"), &base).is_err());
        std::fs::remove_dir_all(plugin.join("test")).unwrap();
        assert!(prepare(&plugin, &base).unwrap_err().contains("no test/"));
        assert_eq!(lua_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
