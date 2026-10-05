//! `:config-diff`, `:config-edit` and `:config-write-toml`.

use std::sync::{Arc, RwLock};

use rt_core::Command;
use rt_core::command::OpenTarget;
use rt_core::engine::Level;
use rt_core::html::escape;

use crate::{shell, spawn};

/// `riptide://config-diff/`, rebuilt each time `:config-diff` runs.
static DIFF_PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn diff_page() -> Arc<[u8]> {
    DIFF_PAGE
        .read()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(|| Arc::from(&b""[..]))
}

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::ConfigDiff => diff(),
        Command::ConfigWriteToml { force } => write_toml(*force),
        Command::ConfigEdit => {
            let Some(paths) = shell::with(|s| s.paths.clone()) else {
                return true;
            };
            let lua = paths.config_lua();
            let file = if lua.exists() {
                lua
            } else {
                paths.config_toml()
            };
            if !file.exists() {
                let created = std::fs::create_dir_all(&paths.config_dir)
                    .and_then(|()| std::fs::write(&file, "# riptide settings; see :help\n"));
                if let Err(e) = created {
                    shell::show_message(
                        Level::Error,
                        format!("Can't create {}: {e}", file.display()),
                    );
                    return true;
                }
            }
            spawn::edit_config(file);
        }
        _ => return false,
    }
    true
}

fn diff() {
    let Some(settings) = shell::with(|s| s.engine.settings().clone()) else {
        return;
    };
    let mut rows = String::new();
    for (name, value) in settings.changed() {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            escape(name),
            escape(&value.to_string())
        ));
    }
    for (pattern, name, value) in settings.all_overrides() {
        rows.push_str(&format!(
            "<tr><td>{} <span class=site>for {}</span></td><td>{}</td></tr>",
            escape(name),
            escape(&pattern),
            escape(&value.to_string())
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=2>Every setting has its default value.</td></tr>");
    }
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><title>Changed settings</title><style>\
         :root {{ color-scheme: light dark; }} body {{ margin: 1.5rem; font: 14px/1.5 system-ui, sans-serif; }} \
         td {{ padding: .2rem 1.5rem .2rem 0; font-family: \"DejaVu Sans Mono\", monospace; vertical-align: top; }} \
         .site {{ color: gray; }}</style></head><body><h1>Changed settings</h1><table>{rows}</table></body></html>"
    );
    if let Ok(mut page) = DIFF_PAGE.write() {
        *page = Some(Arc::from(html.into_bytes()));
    }
    shell::open(
        OpenTarget::Tab,
        true,
        Some("riptide://config-diff/".to_string()),
    );
}

fn write_toml(force: bool) {
    let Some((paths, settings)) = shell::with(|s| (s.paths.clone(), s.engine.settings().clone()))
    else {
        return;
    };
    let file = paths.config_toml();
    if file.exists() && !force {
        return shell::show_message(
            Level::Error,
            format!(
                "{} exists; use :config-write-toml --force to replace it",
                file.display()
            ),
        );
    }
    let written = std::fs::create_dir_all(&paths.config_dir)
        .and_then(|()| std::fs::write(&file, rt_config::toml_file::write(&settings)));
    match written {
        Ok(()) => shell::show_message(Level::Info, format!("Wrote {}", file.display())),
        Err(e) => shell::show_message(Level::Error, format!("Can't write {}: {e}", file.display())),
    }
}
