//! `:navigate`: up the URL, to the page's previous or next link, or to the
//! next or previous number in the URL.

use rt_core::Command;
use rt_core::command::{NavigateTo, OpenTarget};
use rt_core::engine::Level;

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
    let Some((url, browser, segments)) = shell::with(|s| {
        (
            s.tabs.current().map(|t| t.url.clone()).unwrap_or_default(),
            s.current_browser(),
            s.engine.settings().list("url.incdec_segments").to_vec(),
        )
    }) else {
        return true;
    };
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    let steps = i64::from(count.unwrap_or(1).max(1));
    let go = |url: Option<String>, what: &str| match url {
        Some(url) => shell::open(target, true, Some(url)),
        None => shell::show_message(Level::Error, format!("This page has no {what}")),
    };
    match to {
        NavigateTo::Up => {
            let mut next = Some(url);
            for _ in 0..steps {
                next = next.as_deref().and_then(rt_core::url::up);
            }
            go(next, "parent URL");
        }
        NavigateTo::Increment => go(
            rt_core::url::increment_in(&url, steps, &segments),
            "number in its URL",
        ),
        NavigateTo::Decrement => go(
            rt_core::url::increment_in(&url, -steps, &segments),
            "number above zero in its URL",
        ),
        NavigateTo::Prev | NavigateTo::Next => {
            let Some(browser) = browser else { return true };
            let which = if *to == NavigateTo::Next {
                "next"
            } else {
                "prev"
            };
            let setting = format!("hints.{which}_regexes");
            let regexes =
                shell::with(|s| s.engine.settings().list(&setting).to_vec()).unwrap_or_default();
            let regexes = serde_json::to_string(&regexes).unwrap_or_else(|_| "[]".into());
            eval::eval(
                &browser,
                &format!("{NAVIGATE_JS}('{which}', {regexes})"),
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
