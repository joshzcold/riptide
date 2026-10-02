//! Internal pages that make up the browser chrome.

use hb_core::ui_message::UiMessage;

pub const TABBAR_HTML: &str = include_str!("../ui/tabbar.html");
pub const STATUSBAR_HTML: &str = include_str!("../ui/statusbar.html");
pub const COMPLETION_HTML: &str = include_str!("../ui/completion.html");

pub const TABBAR_URL: &str = "hb://ui/tabbar.html";
pub const STATUSBAR_URL: &str = "hb://ui/statusbar.html";
pub const COMPLETION_URL: &str = "hb://ui/completion.html";

/// Act on a validated message from a UI page.
pub fn handle_message(message: UiMessage) {
    match message {
        UiMessage::SelectTab { index } => crate::tabs::select(index),
    }
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

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
