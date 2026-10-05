//! Data for status bar widgets that pages don't report themselves.

use cef::*;

use crate::{eval, shell};

/// How often the `scroll` widget reads the page's scroll position.
const SCROLL_POLL_MS: i64 = 500;

/// The eval channel takes a string, hence `String(…)`.
const SCROLL_JS: &str = "String((() => { \
    const max = Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight ?? 0) - innerHeight; \
    return max <= 0 ? -1 : Math.min(100, Math.round(100 * scrollY / max)); })())";

/// Start reading the scroll position of the current tab while a scroll
/// widget is shown. Pages can't report scrolling to us without being able
/// to detect the browser, so it's read from outside instead.
pub fn start() {
    let mut task = PollScroll::new();
    post_delayed_task(ThreadId::UI, Some(&mut task), SCROLL_POLL_MS);
}

fn poll_scroll() {
    let browser = shell::with(|s| {
        let widgets = s.engine.settings().list("statusbar.widgets");
        let wanted = widgets.iter().any(|w| w == "scroll" || w == "scroll_raw");
        wanted.then(|| s.current_browser()).flatten()
    })
    .flatten();
    let Some(browser) = browser else { return };
    let target = browser.clone();
    eval::eval(&browser, SCROLL_JS, move |result| {
        let Ok(json) = result else { return };
        let scroll = json.parse::<i32>().ok();
        let changed = shell::with_tab(Some(&mut target.clone()), |s, index, _| {
            let tab = s.tabs.get_mut(index)?;
            (tab.scroll != scroll).then(|| tab.scroll = scroll)
        })
        .flatten();
        if changed.is_some() {
            shell::refresh_ui();
        }
    });
}

wrap_task! {
    struct PollScroll {}

    impl Task {
        fn execute(&self) {
            poll_scroll();
            start();
        }
    }
}
