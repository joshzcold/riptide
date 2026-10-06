//! Greasemonkey scripts, browser side: loads them and hands them to every
//! renderer, which runs them (see `renderer.rs`).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use cef::*;
use rt_core::Command;
use rt_core::engine::Level;

use crate::renderer::{GM_VALUES_MESSAGE, GREASEMONKEY_MESSAGE, Scripts};
use crate::shell;

thread_local! {
    /// The scripts as loaded, kept to update their values.
    static SCRIPTS: RefCell<Vec<rt_config::greasemonkey::Script>> = const { RefCell::new(Vec::new()) };
    /// The scripts as JSON, for new browsers' `extra_info`.
    static JSON: RefCell<String> = const { RefCell::new(String::new()) };
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// `@require` URLs already tried this session, so a failing one isn't
    /// fetched again on every reload.
    static TRIED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Read the scripts; returns load errors and how many were loaded. Missing
/// `@require` files are downloaded, and the scripts reloaded once they're in.
pub fn load() -> (usize, Vec<String>) {
    let Some(paths) = shell::with(|s| s.paths.clone()) else {
        return (0, Vec::new());
    };
    let (scripts, errors) = rt_config::greasemonkey::load(&paths);
    let missing: Vec<String> = rt_config::greasemonkey::missing_requires(&paths, &scripts)
        .into_iter()
        .filter(|url| TRIED.with(|t| t.borrow_mut().insert(url.clone())))
        .collect();
    let count = scripts.len();
    SCRIPTS.with(|s| *s.borrow_mut() = scripts);
    publish();
    if !missing.is_empty() {
        fetch_requires(&paths, missing);
    }
    (count, errors)
}

/// Rebuild the JSON new browsers get; returns its generation.
fn publish() -> u64 {
    let generation = GENERATION.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    let json = SCRIPTS.with(|s| {
        serde_json::to_string(&Scripts {
            generation,
            scripts: s.borrow().clone(),
        })
        .unwrap_or_default()
    });
    JSON.with(|j| *j.borrow_mut() = json);
    generation
}

fn fetch_requires(paths: &rt_config::Paths, urls: Vec<String>) {
    let remaining = Rc::new(Cell::new(urls.len()));
    let failed = Rc::new(RefCell::new(Vec::new()));
    for url in urls {
        let path = rt_config::greasemonkey::require_path(paths, &url);
        let (remaining, failed) = (remaining.clone(), failed.clone());
        crate::fetch::get(&url.clone(), move |result| {
            let saved = result.and_then(|body| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&path, body).map_err(|e| e.to_string())
            });
            if let Err(e) = saved {
                failed.borrow_mut().push(format!("{url} ({e})"));
            }
            remaining.set(remaining.get() - 1);
            if remaining.get() > 0 {
                return;
            }
            let failed = failed.borrow();
            match failed.first() {
                None => {
                    load();
                    send_to_renderers();
                    shell::show_message(
                        Level::Info,
                        "Downloaded the Greasemonkey @require files; reload pages to use them",
                    );
                }
                Some(first) => shell::show_message(
                    Level::Error,
                    format!("Greasemonkey @require failed: {first}"),
                ),
            }
            shell::refresh_ui();
        });
    }
}

/// The loaded script called `name`.
pub fn script(name: &str) -> Option<rt_config::greasemonkey::Script> {
    SCRIPTS.with(|s| s.borrow().iter().find(|s| s.name == name).cloned())
}

/// A script's `GM_setValue` or `GM_deleteValue` (`value: None`), from a renderer.
pub fn set_value(script: &str, key: &str, value: Option<&str>) {
    let Some(paths) = shell::with(|s| s.paths.clone()) else {
        return;
    };
    let values = SCRIPTS.with(|s| {
        let mut scripts = s.borrow_mut();
        let found = scripts.iter_mut().find(|s| s.name == script)?;
        match value.and_then(|v| serde_json::from_str(v).ok()) {
            Some(value) => found.values.insert(key.to_string(), value),
            None => found.values.remove(key),
        };
        Some(found.values.clone())
    });
    let Some(values) = values else { return };
    let path = rt_config::greasemonkey::values_path(&paths, script);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            std::fs::write(&path, serde_json::Value::Object(values.clone()).to_string())
        });
    if let Err(e) = written {
        tracing::warn!("can't save Greasemonkey values to {}: {e}", path.display());
    }
    let generation = publish();
    // Pages loading later in other renderers see the new values. The
    // generation stops a reload's original `extra_info` from undoing them.
    let payload =
        serde_json::json!({ "generation": generation, "script": script, "values": values })
            .to_string();
    broadcast(GM_VALUES_MESSAGE, &payload);
}

fn send_to_renderers() {
    let json = JSON.with(|j| j.borrow().clone());
    broadcast(GREASEMONKEY_MESSAGE, &json);
}

fn broadcast(name: &str, payload: &str) {
    let browsers: Vec<Browser> = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter().filter_map(|t| t.browser()))
            .collect()
    })
    .unwrap_or_default();
    for browser in browsers {
        let (Some(frame), Some(mut message)) = (
            browser.main_frame(),
            process_message_create(Some(&CefString::from(name))),
        ) else {
            continue;
        };
        if let Some(args) = message.argument_list() {
            args.set_string(0, Some(&CefString::from(payload)));
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    }
}

/// `extra_info` for a new tab's browser.
pub fn extra_info() -> Option<DictionaryValue> {
    let info = dictionary_value_create()?;
    let json = JSON.with(|j| j.borrow().clone());
    info.set_string(
        Some(&CefString::from(GREASEMONKEY_MESSAGE)),
        Some(&CefString::from(json.as_str())),
    );
    Some(info)
}

pub fn run_command(command: &Command) -> bool {
    if !matches!(command, Command::GreasemonkeyReload) {
        return false;
    }
    let (count, errors) = load();
    send_to_renderers();
    match errors.as_slice() {
        [] => shell::show_message(
            Level::Info,
            format!("Loaded {count} Greasemonkey script(s); reload pages to run them"),
        ),
        [first, ..] => shell::show_message(Level::Error, format!("Greasemonkey: {first}")),
    }
    true
}
