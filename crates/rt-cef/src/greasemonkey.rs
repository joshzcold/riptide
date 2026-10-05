//! Greasemonkey scripts, browser side: loads them and hands them to every
//! renderer, which runs them (see `renderer.rs`).

use std::cell::RefCell;

use cef::*;
use rt_core::Command;
use rt_core::engine::Level;

use crate::renderer::{GREASEMONKEY_MESSAGE, Scripts};
use crate::shell;

thread_local! {
    /// The scripts as JSON, for new browsers' `extra_info`.
    static JSON: RefCell<String> = const { RefCell::new(String::new()) };
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Read the scripts; returns load errors and how many were loaded.
pub fn load() -> (usize, Vec<String>) {
    let Some(paths) = shell::with(|s| s.paths.clone()) else {
        return (0, Vec::new());
    };
    let (scripts, errors) = rt_config::greasemonkey::load(&paths);
    let generation = GENERATION.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    let json = serde_json::to_string(&Scripts {
        generation,
        scripts: scripts.clone(),
    })
    .unwrap_or_default();
    JSON.with(|j| *j.borrow_mut() = json);
    (scripts.len(), errors)
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
    let json = JSON.with(|j| j.borrow().clone());
    let browsers: Vec<Browser> =
        shell::with(|s| s.tabs.iter().filter_map(|t| t.browser()).collect()).unwrap_or_default();
    for browser in browsers {
        let (Some(frame), Some(mut message)) = (
            browser.main_frame(),
            process_message_create(Some(&CefString::from(GREASEMONKEY_MESSAGE))),
        ) else {
            continue;
        };
        if let Some(args) = message.argument_list() {
            args.set_string(0, Some(&CefString::from(json.as_str())));
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    }
    match errors.as_slice() {
        [] => shell::show_message(
            Level::Info,
            format!("Loaded {count} Greasemonkey script(s); reload pages to run them"),
        ),
        [first, ..] => shell::show_message(Level::Error, format!("Greasemonkey: {first}")),
    }
    true
}
