//! Internal pages that make up the browser chrome.

use rt_core::html::escape;
use rt_core::ui_message::UiMessage;

pub const TABBAR_HTML: &str = include_str!("../ui/tabbar.html");
pub const STATUSBAR_HTML: &str = include_str!("../ui/statusbar.html");
pub const COMPLETION_HTML: &str = include_str!("../ui/completion.html");
pub const CRASHED_HTML: &str = include_str!("../ui/crashed.html");

pub const TABBAR_URL: &str = "riptide://ui/tabbar.html";
pub const STATUSBAR_URL: &str = "riptide://ui/statusbar.html";
pub const COMPLETION_URL: &str = "riptide://ui/completion.html";
pub const CRASHED_URL: &str = "riptide://ui/crashed.html";

/// Act on a validated message from a UI page.
pub fn handle_message(message: UiMessage) {
    match message {
        UiMessage::SelectTab { index } => crate::tabs::select(index),
        UiMessage::CloseTab { index } => {
            crate::tabs::close_unless_pinned(index, false);
            crate::shell::refresh_ui();
        }
        UiMessage::CycleTab { forward } => crate::tabs::cycle(forward),
        UiMessage::MoveTab { from, to } => crate::tabs::move_tab(from, to),
        UiMessage::BarClick => bar_click(),
        UiMessage::PromptKey { key } => prompt_key(&key),
    }
}

/// A prompt's button: press its key, but only if the prompt on screen
/// offers it, so the overlay can't type anything else.
fn prompt_key(key: &str) {
    let offered = crate::shell::with(|s| {
        s.engine
            .prompt_view()
            .is_some_and(|p| p.options.iter().any(|o| o.key == key))
    })
    .unwrap_or(false);
    let Ok(keys) = rt_core::key::Key::parse_sequence(key) else {
        return;
    };
    let [key] = keys.as_slice() else {
        return;
    };
    let key = *key;
    if !offered {
        return;
    }
    if let Some(outcome) = crate::shell::with(|s| s.engine.handle_key(key)) {
        crate::shell::apply(outcome.effects);
    }
    crate::shell::refresh_ui();
}

/// `tabs.close_mouse_button_on_bar`: what the close button does on the
/// empty part of the tab bar.
fn bar_click() {
    let Some((action, current, len)) = crate::shell::with(|s| {
        let action = s
            .engine
            .settings()
            .str("tabs.close_mouse_button_on_bar")
            .to_string();
        (action, s.tabs.current_index(), s.tabs.len())
    }) else {
        return;
    };
    match action.as_str() {
        "new-tab" => {
            if let Some(effects) = crate::shell::with(|s| s.engine.execute_str("open -t", None)) {
                crate::shell::apply(effects);
            }
        }
        "close-current" => crate::tabs::close_unless_pinned(current, false),
        "close-last" if len > 0 => crate::tabs::close_unless_pinned(len - 1, false),
        _ => {}
    }
    crate::shell::refresh_ui();
}

/// Why a tab's renderer process ended, in words for the crash notice.
pub fn crash_reason(status: cef::sys::cef_termination_status_t) -> &'static str {
    use cef::sys::cef_termination_status_t as ts;
    match status {
        ts::TS_PROCESS_WAS_KILLED => "Its process was killed.",
        ts::TS_PROCESS_CRASHED => "Its process crashed.",
        ts::TS_PROCESS_OOM => "It ran out of memory.",
        ts::TS_LAUNCH_FAILED => "Its process couldn't start.",
        ts::TS_INTEGRITY_FAILURE => "Its process failed a code integrity check.",
        _ => "Its process ended unexpectedly.",
    }
}

/// The crash notice's URL for a tab that was showing `url`.
pub fn crashed_url(url: &str, reason: &str) -> String {
    let info = serde_json::json!({ "url": url, "reason": reason }).to_string();
    format!("{CRASHED_URL}#{}", rt_core::url::encode_query(&info))
}

/// JavaScript that replaces the current document with an error description.
pub fn error_page_js(url: &str, error: &str) -> String {
    let html = format!(
        r#"<head><meta charset="utf-8"><title>Error loading page</title></head>
<body style="font: 11pt sans-serif; margin: 3em; background: #1e1e1e; color: #ddd">
<h2>Error while loading {url}</h2><p>{error}</p></body>"#,
        url = escape(url),
        error = escape(error),
    );
    let html = serde_json::to_string(&html).unwrap_or_default();
    format!("document.documentElement.innerHTML = {html};")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_reasons_name_the_cause() {
        use cef::sys::cef_termination_status_t as ts;
        assert_eq!(crash_reason(ts::TS_PROCESS_OOM), "It ran out of memory.");
        assert_eq!(
            crash_reason(ts::TS_PROCESS_WAS_KILLED),
            "Its process was killed."
        );
        assert_eq!(
            crash_reason(ts::TS_ABNORMAL_TERMINATION),
            "Its process ended unexpectedly."
        );
    }

    #[test]
    fn the_crash_notice_url_carries_the_page_in_its_fragment() {
        let url = crashed_url("https://x.example/a b?q=1&r=#f", "It crashed.");
        let fragment = url.strip_prefix("riptide://ui/crashed.html#").expect(&url);
        // Nothing in the fragment can end it or start a query.
        assert!(
            fragment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.~%+".contains(&b)),
            "{url}"
        );
        assert!(fragment.contains("It+crashed."), "{url}");
        assert!(fragment.contains("a+b%3Fq%3D1%26r%3D%23f"), "{url}");
    }

    #[test]
    fn the_error_page_escapes_the_url_and_error() {
        let js = error_page_js("https://x.example/<script>", "it's \"bad\" & <b>");
        assert!(
            js.starts_with("document.documentElement.innerHTML = \""),
            "{js}"
        );
        assert!(js.contains("https://x.example/&lt;script&gt;"), "{js}");
        assert!(
            js.contains("it&#39;s &quot;bad&quot; &amp; &lt;b&gt;"),
            "{js}"
        );
        assert!(!js.contains("<script>"), "{js}");
    }
}
