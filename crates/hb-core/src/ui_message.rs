//! Messages from the browser's own `hb://ui/` pages to Rust. Each page may
//! send only the messages listed for it, and every field is checked, so a
//! compromised or confused page can't do more than its buttons could.

use serde::Deserialize;

#[derive(Debug, PartialEq, Eq)]
pub enum UiMessage {
    /// A click on a tab in the tab bar.
    SelectTab { index: usize },
    /// A middle-click on a tab.
    CloseTab { index: usize },
    /// The mouse wheel over the tab bar.
    CycleTab { forward: bool },
    /// A tab dragged to another position.
    MoveTab { from: usize, to: usize },
}

/// Pages allowed to send messages, by `hb://ui/` path.
pub const UI_PREFIX: &str = "hb://ui/";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TabIndex {
    index: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wheel {
    forward: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    from: usize,
    to: usize,
}

fn payload<T: serde::de::DeserializeOwned>(name: &str, json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("{name}: {e}"))
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
pub fn parse(url: &str, name: &str, json: &str) -> Result<UiMessage, String> {
    let page = url
        .strip_prefix(UI_PREFIX)
        .ok_or_else(|| format!("{url} may not send UI messages"))?;
    let page = page.split(['?', '#']).next().unwrap_or_default();
    match (page, name) {
        ("tabbar.html", "select-tab") => {
            let TabIndex { index } = payload(name, json)?;
            Ok(UiMessage::SelectTab { index })
        }
        ("tabbar.html", "close-tab") => {
            let TabIndex { index } = payload(name, json)?;
            Ok(UiMessage::CloseTab { index })
        }
        ("tabbar.html", "cycle-tab") => {
            let Wheel { forward } = payload(name, json)?;
            Ok(UiMessage::CycleTab { forward })
        }
        ("tabbar.html", "move-tab") => {
            let Move { from, to } = payload(name, json)?;
            Ok(UiMessage::MoveTab { from, to })
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
    fn tab_bar_mouse_messages() {
        let tabbar = "hb://ui/tabbar.html";
        assert_eq!(
            parse(tabbar, "close-tab", r#"{"index": 1}"#),
            Ok(UiMessage::CloseTab { index: 1 })
        );
        assert_eq!(
            parse(tabbar, "cycle-tab", r#"{"forward": false}"#),
            Ok(UiMessage::CycleTab { forward: false })
        );
        assert_eq!(
            parse(tabbar, "move-tab", r#"{"from": 0, "to": 3}"#),
            Ok(UiMessage::MoveTab { from: 0, to: 3 })
        );
        assert!(parse(tabbar, "move-tab", r#"{"from": 0}"#).is_err());
        assert!(parse(tabbar, "cycle-tab", r#"{"forward": 1}"#).is_err());
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
