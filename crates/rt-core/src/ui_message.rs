//! Messages from the browser's own `riptide://ui/` pages to Rust. Each page may
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
    /// `tabs.close_mouse_button` clicked on the bar outside any tab.
    BarClick,
    /// A prompt's button: press its key. Only keys the prompt offers count.
    PromptKey { key: String },
    /// The tab bar's or status bar's natural height for its font and padding.
    BarHeight { bar: Bar, height: u32 },
    /// The overlay's row height for its fonts.
    RowHeight { height: u32 },
    /// Whether pages are asked for dark colors, for `ui.theme = auto`.
    ColorScheme { dark: bool },
}

/// Which bar a size is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    Tabbar,
    Statusbar,
}

impl UiMessage {
    /// Whether the user did something, as opposed to a page reporting its size.
    pub fn is_input(&self) -> bool {
        !matches!(
            self,
            UiMessage::BarHeight { .. }
                | UiMessage::RowHeight { .. }
                | UiMessage::ColorScheme { .. }
        )
    }
}

/// Pages allowed to send messages, by `riptide://ui/` path.
pub const UI_PREFIX: &str = "riptide://ui/";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Nothing {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Height {
    height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowHeight {
    row_height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scheme {
    dark: bool,
}

/// Sizes a page may ask for, in pixels.
const SIZES: std::ops::RangeInclusive<u32> = 8..=200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptKey {
    key: String,
}

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

/// `riptide://host/path?query#frag` → `(host, path)`; `None` for other schemes
/// and paths containing `..`.
pub fn split_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("riptide://")?;
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
        ("tabbar.html", "bar-click") => {
            let Nothing {} = payload(name, json)?;
            Ok(UiMessage::BarClick)
        }
        (page @ ("tabbar.html" | "statusbar.html"), "size") => {
            let Height { height } = payload(name, json)?;
            if !SIZES.contains(&height) {
                return Err(format!("{name}: height {height} out of range"));
            }
            let bar = if page == "tabbar.html" {
                Bar::Tabbar
            } else {
                Bar::Statusbar
            };
            Ok(UiMessage::BarHeight { bar, height })
        }
        ("statusbar.html", "color-scheme") => {
            let Scheme { dark } = payload(name, json)?;
            Ok(UiMessage::ColorScheme { dark })
        }
        ("completion.html", "size") => {
            let RowHeight { row_height } = payload(name, json)?;
            if !SIZES.contains(&row_height) {
                return Err(format!("{name}: row height {row_height} out of range"));
            }
            Ok(UiMessage::RowHeight { height: row_height })
        }
        ("completion.html", "prompt-key") => {
            let PromptKey { key } = payload(name, json)?;
            if key.is_empty() || key.len() > 20 {
                return Err(format!("{name}: bad key {key:?}"));
            }
            Ok(UiMessage::PromptKey { key })
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
            parse("riptide://ui/tabbar.html", "select-tab", r#"{"index": 2}"#),
            Ok(UiMessage::SelectTab { index: 2 })
        );
        assert_eq!(
            parse(
                "riptide://ui/tabbar.html#x",
                "select-tab",
                r#"{"index": 0}"#
            ),
            Ok(UiMessage::SelectTab { index: 0 })
        );
    }

    #[test]
    fn tab_bar_mouse_messages() {
        let tabbar = "riptide://ui/tabbar.html";
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
        assert_eq!(parse(tabbar, "bar-click", "{}"), Ok(UiMessage::BarClick));
        let overlay = "riptide://ui/completion.html";
        assert_eq!(
            parse(overlay, "prompt-key", r#"{"key": "y"}"#),
            Ok(UiMessage::PromptKey { key: "y".into() })
        );
        assert!(parse(tabbar, "prompt-key", r#"{"key": "y"}"#).is_err());
        assert_eq!(
            parse(tabbar, "size", r#"{"height": 24}"#),
            Ok(UiMessage::BarHeight {
                bar: Bar::Tabbar,
                height: 24
            })
        );
        assert!(parse(tabbar, "size", r#"{"height": 5000}"#).is_err());
        assert_eq!(
            parse(overlay, "size", r#"{"row_height": 22}"#),
            Ok(UiMessage::RowHeight { height: 22 })
        );
        let statusbar = "riptide://ui/statusbar.html";
        assert_eq!(
            parse(statusbar, "color-scheme", r#"{"dark": false}"#),
            Ok(UiMessage::ColorScheme { dark: false })
        );
        assert!(parse(tabbar, "color-scheme", r#"{"dark": false}"#).is_err());
        assert!(parse(overlay, "prompt-key", r#"{"key": ""}"#).is_err());
        assert!(parse(tabbar, "bar-click", r#"{"x": 1}"#).is_err());
        assert!(parse(tabbar, "cycle-tab", r#"{"forward": 1}"#).is_err());
    }

    #[test]
    fn rejects_other_pages_and_names() {
        assert!(parse("https://evil.example/", "select-tab", r#"{"index": 0}"#).is_err());
        assert!(parse("riptide://help/", "select-tab", r#"{"index": 0}"#).is_err());
        assert!(
            parse(
                "riptide://ui/statusbar.html",
                "select-tab",
                r#"{"index": 0}"#
            )
            .is_err()
        );
        assert!(parse("riptide://ui/tabbar.html", "quit", "{}").is_err());
    }

    #[test]
    fn splits_urls() {
        assert_eq!(
            split_url("riptide://ui/tabbar.html"),
            Some(("ui", "/tabbar.html"))
        );
        assert_eq!(
            split_url("riptide://ui/tabbar.html?x#y"),
            Some(("ui", "/tabbar.html"))
        );
        assert_eq!(split_url("riptide://help"), Some(("help", "/")));
        assert_eq!(split_url("riptide://help/"), Some(("help", "/")));
        assert_eq!(split_url("riptide://ui/../etc/passwd"), None);
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
                parse("riptide://ui/tabbar.html", "select-tab", payload).is_err(),
                "{payload}"
            );
        }
    }
}
