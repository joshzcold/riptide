//! Finding text in the page (`/`, `?`, `n`, `N`) with Chromium's find bar
//! machinery, minus the bar: matches are highlighted in the page.

use std::cell::RefCell;

use cef::*;
use hb_core::Command;
use hb_core::engine::Level;

use crate::shell;

#[derive(Default)]
struct Last {
    text: String,
    reverse: bool,
    /// Report "not found" once the final result arrives.
    report: bool,
}

thread_local! {
    static LAST: RefCell<Last> = RefCell::new(Last::default());
}

fn match_case(text: &str) -> bool {
    let mode = shell::with(|s| s.engine.settings().str("search.ignore_case").to_string())
        .unwrap_or_default();
    match mode.as_str() {
        "always" => false,
        "never" => true,
        _ => text.chars().any(char::is_uppercase),
    }
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    let Some(host) = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.host())
    else {
        return matches!(command, Command::Search { .. } | Command::SearchNext { .. });
    };
    match command {
        Command::Search {
            text,
            reverse,
            incremental,
        } => {
            if text.is_empty() {
                host.stop_finding(1);
                LAST.with(|l| l.borrow_mut().text.clear());
                return true;
            }
            let same = LAST.with(|l| l.borrow().text == *text);
            LAST.with(|l| {
                *l.borrow_mut() = Last {
                    text: text.clone(),
                    reverse: *reverse,
                    report: !incremental,
                }
            });
            if same {
                host.stop_finding(1);
            }
            let text_cef = CefString::from(text.as_str());
            host.find(
                Some(&text_cef),
                (!reverse).into(),
                match_case(text).into(),
                1,
            );
        }
        Command::SearchNext { prev } => {
            let (text, reverse) = LAST.with(|l| {
                let mut l = l.borrow_mut();
                l.report = true;
                (l.text.clone(), l.reverse)
            });
            if text.is_empty() {
                shell::show_message(Level::Error, "No search yet; type / to search");
                return true;
            }
            let forward = reverse == *prev;
            let text_cef = CefString::from(text.as_str());
            for _ in 0..count.unwrap_or(1).clamp(1, 1000) {
                host.find(Some(&text_cef), forward.into(), match_case(&text).into(), 1);
            }
        }
        _ => return false,
    }
    true
}

wrap_find_handler! {
    pub struct HbFindHandler {}

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            final_update: ::std::os::raw::c_int,
        ) {
            if final_update == 0 {
                return;
            }
            let (text, report) = LAST.with(|l| {
                let mut l = l.borrow_mut();
                (l.text.clone(), std::mem::take(&mut l.report))
            });
            if text.is_empty() || !report {
                return;
            }
            if count == 0 {
                shell::show_message(Level::Warning, format!("Text '{text}' not found on page"));
            } else {
                shell::show_message(Level::Info, format!("Match {active_match_ordinal} of {count} for '{text}'"));
            }
            shell::refresh_ui();
        }
    }
}
