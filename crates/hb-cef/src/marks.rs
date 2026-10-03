//! Marks, as in vim and qutebrowser: `` `a `` remembers the scroll position
//! on this page, `` `A `` also remembers the page, and `'a` goes back. `''`
//! returns to where the last jump started. Marks last for the session.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use hb_core::Command;
use hb_core::command::OpenTarget;
use hb_core::engine::Level;

use crate::{eval, shell};

type Position = (f64, f64);

#[derive(Default)]
struct Marks {
    local: HashMap<(String, char), Position>,
    global: HashMap<char, (String, Position)>,
    /// Scroll to apply once this browser's page has loaded.
    pending: HashMap<i32, Position>,
}

thread_local! {
    static MARKS: RefCell<Marks> = RefCell::new(Marks::default());
}

/// Where the previous jump started.
const BACK: char = '\'';

pub fn run_command(command: &Command) -> bool {
    let Command::Mark { set, key } = command else {
        return false;
    };
    let (set, key) = (*set, *key);
    let Some(Some((browser, url))) = shell::with(|s| {
        let url = s.tabs.current()?.url.clone();
        Some((s.current_browser()?, url))
    }) else {
        return true;
    };
    position(&browser, move |browser, here| {
        if set {
            MARKS.with(|m| {
                let mut m = m.borrow_mut();
                if key.is_uppercase() {
                    m.global.insert(key, (url.clone(), here));
                } else {
                    m.local.insert((url.clone(), key), here);
                }
            });
            shell::show_message(Level::Info, format!("Mark {key} set"));
            return shell::refresh_ui();
        }
        jump(browser, &url, here, key);
    });
    true
}

fn jump(browser: Browser, url: &str, here: Position, key: char) {
    let target = MARKS.with(|m| {
        let m = m.borrow();
        if key.is_uppercase() {
            m.global.get(&key).cloned()
        } else {
            m.local
                .get(&(url.to_string(), key))
                .map(|p| (url.to_string(), *p))
        }
    });
    let Some((target_url, position)) = target else {
        shell::show_message(Level::Error, format!("Mark {key} isn't set"));
        return shell::refresh_ui();
    };
    MARKS.with(|m| m.borrow_mut().local.insert((url.to_string(), BACK), here));
    if target_url == url {
        scroll_to(&browser, position);
    } else {
        MARKS.with(|m| {
            m.borrow_mut()
                .pending
                .insert(browser.identifier(), position)
        });
        shell::open(OpenTarget::Current, false, Some(target_url));
    }
}

/// A page finished loading: apply a scroll a global mark is waiting for.
pub fn loaded(browser: &Browser) {
    if let Some(position) = MARKS.with(|m| m.borrow_mut().pending.remove(&browser.identifier())) {
        scroll_to(browser, position);
    }
}

fn position(browser: &Browser, then: impl FnOnce(Browser, Position) + 'static) {
    let target = browser.clone();
    eval::eval(
        browser,
        "JSON.stringify([scrollX, scrollY])",
        move |result| {
            let here = result
                .ok()
                .and_then(|json| serde_json::from_str::<Position>(&json).ok())
                .unwrap_or_default();
            then(target, here);
        },
    );
}

fn scroll_to(browser: &Browser, (x, y): Position) {
    eval::eval(browser, &format!("scrollTo({x}, {y}); 'null'"), |_| {});
}
