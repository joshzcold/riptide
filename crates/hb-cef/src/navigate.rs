//! `:navigate`: up the URL, to the page's previous or next link, or to the
//! next or previous number in the URL.

use hb_core::Command;
use hb_core::command::{NavigateTo, OpenTarget};
use hb_core::engine::Level;

use crate::{eval, shell};

const NAVIGATE_JS: &str = include_str!("../js/navigate.js");

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    let Command::Navigate { to, tab } = command else {
        return false;
    };
    let target = if *tab {
        OpenTarget::Tab
    } else {
        OpenTarget::Current
    };
    let Some((url, browser)) = shell::with(|s| {
        (
            s.tabs.current().map(|t| t.url.clone()).unwrap_or_default(),
            s.current_browser(),
        )
    }) else {
        return true;
    };
    let steps = i64::from(count.unwrap_or(1).max(1));
    let go = |url: Option<String>, what: &str| match url {
        Some(url) => shell::open(target, true, Some(url)),
        None => shell::show_message(Level::Error, format!("This page has no {what}")),
    };
    match to {
        NavigateTo::Up => {
            let mut next = Some(url);
            for _ in 0..steps {
                next = next.as_deref().and_then(hb_core::url::up);
            }
            go(next, "parent URL");
        }
        NavigateTo::Increment => go(hb_core::url::increment(&url, steps), "number in its URL"),
        NavigateTo::Decrement => go(
            hb_core::url::increment(&url, -steps),
            "number above zero in its URL",
        ),
        NavigateTo::Prev | NavigateTo::Next => {
            let Some(browser) = browser else { return true };
            let which = if *to == NavigateTo::Next {
                "next"
            } else {
                "prev"
            };
            eval::eval(
                &browser,
                &format!("{NAVIGATE_JS}('{which}')"),
                move |result| {
                    let url = result
                        .ok()
                        .and_then(|json| serde_json::from_str::<Option<String>>(&json).ok())
                        .flatten();
                    match url {
                        Some(url) => shell::open(target, true, Some(url)),
                        None => {
                            shell::show_message(
                                Level::Error,
                                format!("No {which} link on this page"),
                            );
                            shell::refresh_ui();
                        }
                    }
                },
            );
        }
    }
    true
}
