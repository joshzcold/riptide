//! Messages from the browser's own `hb://ui/` pages to Rust. Each page may
//! send only the messages listed for it, and every field is checked, so a
//! compromised or confused page can't do more than its buttons could.

use serde::Deserialize;

#[derive(Debug, PartialEq, Eq)]
pub enum UiMessage {
    /// A click on a tab in the tab bar.
    SelectTab { index: usize },
}

/// Pages allowed to send messages, by `hb://ui/` path.
pub const UI_PREFIX: &str = "hb://ui/";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TabIndex {
    index: usize,
}

/// `hb://host/path?query#frag` → `(host, path)`; `None` for other schemes
/// and paths containing `..`.
pub fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("hb://")?;
    let rest = rest.split(['?', '#']).next()?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    (!path.contains("..")).then_some((host, path))
}

/// Validate a message from the page at `url`.
pub fn parse(url: &str, name: &str, payload: &str) -> Result<UiMessage, String> {
    let page = url
        .strip_prefix(UI_PREFIX)
        .ok_or_else(|| format!("{url} may not send UI messages"))?;
    let page = page.split(['?', '#']).next().unwrap_or_default();
    match (page, name) {
        ("tabbar.html", "select-tab") => {
            let TabIndex { index } =
                serde_json::from_str(payload).map_err(|e| format!("select-tab: {e}"))?;
            Ok(UiMessage::SelectTab { index })
        }
        _ => Err(format!("{page} may not send {name:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_listed_messages() {
        assert_eq!(
            parse("hb://ui/tabbar.html", "select-tab", r#"{"index": 2}"#),
            Ok(UiMessage::SelectTab { index: 2 })
        );
        assert_eq!(
            parse("hb://ui/tabbar.html#x", "select-tab", r#"{"index": 0}"#),
            Ok(UiMessage::SelectTab { index: 0 })
        );
    }

    #[test]
    fn rejects_other_pages_and_names() {
        assert!(parse("https://evil.example/", "select-tab", r#"{"index": 0}"#).is_err());
        assert!(parse("hb://help/", "select-tab", r#"{"index": 0}"#).is_err());
        assert!(parse("hb://ui/statusbar.html", "select-tab", r#"{"index": 0}"#).is_err());
        assert!(parse("hb://ui/tabbar.html", "quit", "{}").is_err());
    }

    #[test]
    fn splits_urls() {
        assert_eq!(
            split_url("hb://ui/tabbar.html"),
            Some(("ui", "/tabbar.html"))
        );
        assert_eq!(
            split_url("hb://ui/tabbar.html?x#y"),
            Some(("ui", "/tabbar.html"))
        );
        assert_eq!(split_url("hb://help"), Some(("help", "/")));
        assert_eq!(split_url("hb://help/"), Some(("help", "/")));
        assert_eq!(split_url("hb://ui/../etc/passwd"), None);
        assert_eq!(split_url("https://ui/tabbar.html"), None);
    }

    #[test]
    fn rejects_bad_payloads() {
        for payload in [
            "",
            "{}",
            r#"{"index": -1}"#,
            r#"{"index": "1"}"#,
            r#"{"index": 1, "extra": true}"#,
        ] {
            assert!(
                parse("hb://ui/tabbar.html", "select-tab", payload).is_err(),
                "{payload}"
            );
        }
    }
}
