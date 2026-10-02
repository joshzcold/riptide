//! Internal pages that make up the browser chrome.

use cef::*;

pub const STATUSBAR_HTML: &str = include_str!("../ui/statusbar.html");
pub const COMPLETION_HTML: &str = include_str!("../ui/completion.html");

pub fn data_uri(html: &str) -> String {
    let encoded = CefString::from(&base64_encode(Some(html.as_bytes())));
    let encoded = CefString::from(&uriencode(Some(&encoded), 0)).to_string();
    format!("data:text/html;base64,{encoded}")
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
