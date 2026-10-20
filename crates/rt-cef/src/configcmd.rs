//! `:config-diff`, `:config-edit` and `:config-write-toml`, and `config.watch`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use cef::*;
use rt_config::watch::Watcher;

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

/// How often `config.watch` looks at the config files.
const WATCH_POLL_MS: i64 = 1000;

thread_local! {
    static WATCHER: RefCell<Watcher> = RefCell::new(Watcher::default());
    static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The config was just read from `files`: watch them from here, so riptide's
/// own reloads don't count as changes.
pub fn watch(files: Vec<(PathBuf, rt_config::watch::Stamp)>) {
    WATCHER.with(|w| *w.borrow_mut() = Watcher::new(files));
    if !WATCHING.replace(true) {
        schedule_poll();
    }
}

fn schedule_poll() {
    let mut task = WatchPoll::new();
    post_delayed_task(ThreadId::UI, Some(&mut task), WATCH_POLL_MS);
}

wrap_task! {
    struct WatchPoll {}

    impl Task {
        fn execute(&self) {
            if shell::with(|s| s.engine.settings().bool("config.watch")).unwrap_or(false) {
                reload_if_changed();
            }
            schedule_poll();
        }
    }
}

fn reload_if_changed() {
    let modules = rt_config::lua::config_modules();
    let changed = WATCHER.with(|w| {
        let mut watcher = w.borrow_mut();
        for (module, seen) in modules {
            watcher.add(module, seen);
        }
        watcher.poll()
    });
    let Some(file) = changed.first() else { return };
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let errors = shell::load_config();
    if errors.is_empty() {
        shell::show_message(Level::Info, format!("{name} changed: config reloaded"));
    } else {
        crate::report_config_errors(&errors);
    }
}
