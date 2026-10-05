//! Internal pages that make up the browser chrome.

use rt_core::html::escape;
use rt_core::ui_message::UiMessage;

pub const TABBAR_HTML: &str = include_str!("../ui/tabbar.html");
pub const STATUSBAR_HTML: &str = include_str!("../ui/statusbar.html");
pub const COMPLETION_HTML: &str = include_str!("../ui/completion.html");

pub const TABBAR_URL: &str = "riptide://ui/tabbar.html";
pub const STATUSBAR_URL: &str = "riptide://ui/statusbar.html";
pub const COMPLETION_URL: &str = "riptide://ui/completion.html";

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

#[cfg(test)]
mod tests {
    use super::*;

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
