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
    /// A click on a line of an `rt.ui.panel`.
    PanelClick { id: u32, line: u32 },
    /// An `rt.ui.float`'s page measured its content.
    FloatSize { id: u32, width: u32, height: u32 },
    /// The overlay's row height for its fonts.
    RowHeight { height: u32 },
    /// Whether pages are asked for dark colors, for `ui.theme = auto`.
    ColorScheme { dark: bool },
    /// The settings page changed a setting; `value` is checked like `:set`'s.
    SettingsSet {
        name: String,
        value: serde_json::Value,
        /// Only for pages matching this pattern, like `:set -u`.
        pattern: Option<String>,
    },
    /// The settings page's reset button.
    SettingsReset { name: String },
    /// The Keys tab bound `keys` to `command`; checked like `:bind`.
    SettingsBind {
        mode: crate::mode::Mode,
        keys: String,
        command: String,
    },
    /// The Keys tab's remove button.
    SettingsUnbind {
        mode: crate::mode::Mode,
        keys: String,
    },
    /// The Sites tab forgot a setting's value for a pattern.
    SettingsUnsetSite { pattern: String, name: String },
    /// The Sites tab's "clear cookies and site data" for a host or origin.
    ClearSite { site: String },
    /// The Plugins tab's buttons for one plugin.
    Plugin { name: String, action: PluginAction },
    /// The Extensions tab's buttons; `id` is empty for checking all.
    Extension { id: String, action: ExtensionAction },
    /// The Extensions tab's Install box: a Web Store page or an id.
    ExtensionInstall { source: String },
    /// A page's "restart now" button.
    Restart,
    /// The recover page: reopen these `(window, tab)`s of a crash's session.
    RecoverReopen {
        session: String,
        tabs: Vec<(usize, usize)>,
    },
    /// The recover page: delete a crash's session.
    RecoverForget { session: String },
}

/// What the Plugins tab asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginAction {
    /// Fetch and list new commits, changing nothing.
    Check,
    /// Move to the commits a check found.
    Update,
    /// Forget its approved permissions.
    Revoke,
    /// Delete its installed copy and lockfile entry.
    Remove,
    /// Load a plugin that waits for an event, command or key now.
    Load,
}

/// What the Extensions tab asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionAction {
    /// Open its toolbar popup in a tab.
    Popup,
    /// Open its options page in a tab.
    Options,
    /// Delete an installed extension.
    Remove,
    /// Ask the Web Store for newer versions.
    Check,
    /// Install the newer version a check found.
    Update,
    /// Chrome's own extensions page.
    Chrome,
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
                | UiMessage::FloatSize { .. }
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

/// The plugin a `riptide://<name>.plugin/…` page belongs to. Each plugin's
/// pages are their own origin, so plugins can't read each other's storage.
pub fn plugin_page(url: &str) -> Option<&str> {
    let (host, _) = split_url(url)?;
    let name = host.strip_suffix(".plugin")?;
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    ok.then_some(name)
}

/// The `page=<id>` an `rt.ui.page` URL carries, which says which handle its messages go to.
pub fn plugin_page_id(url: &str) -> Option<u32> {
    let query = url.split_once('?')?.1.split('#').next()?;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("page="))?
        .parse()
        .ok()
}

/// The page at `url` that may send messages: a `riptide://ui/` page's file
/// name, or `settings` and `recover` for `riptide://settings/` and
/// `riptide://recover/`.
pub fn sender(url: &str) -> Option<&str> {
    if let Some(page) = url.strip_prefix(UI_PREFIX) {
        return page.split(['?', '#']).next();
    }
    match split_url(url) {
        Some(("settings", "/")) => Some("settings"),
        Some(("recover", "/")) => Some("recover"),
        _ => None,
    }
}

/// Whether a page in a tab (not one of riptide's bars) may send messages.
/// Only the settings and recover pages may; web pages can't load or frame them.
pub fn tab_may_send(url: &str) -> bool {
    matches!(sender(url), Some("settings" | "recover"))
}

/// A crash's session name, `_crashed-` and a date: the only sessions the
/// recover page may reopen or delete.
fn crashed_session(session: String) -> Result<String, String> {
    let date = session.strip_prefix("_crashed-").unwrap_or_default();
    if !date.is_empty() && date.len() <= 32 && date.chars().all(|c| c.is_ascii_digit() || c == '-')
    {
        Ok(session)
    } else {
        Err(format!("not a crash's session: {session:?}"))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reopen {
    session: String,
    tabs: Vec<(usize, usize)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    session: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsSet {
    name: String,
    value: serde_json::Value,
    #[serde(default)]
    pattern: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingName {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindMessage {
    mode: String,
    keys: String,
    #[serde(default)]
    command: String,
}

fn bind_message(name: &str, json: &str) -> Result<(crate::mode::Mode, String, String), String> {
    let BindMessage {
        mode,
        keys,
        command,
    } = payload(name, json)?;
    let mode = mode.parse().map_err(|e| format!("{name}: {e}"))?;
    if keys.is_empty() || keys.len() > 100 || command.len() > 2000 {
        return Err(format!("{name}: keys or command too long"));
    }
    Ok((mode, keys, command))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsetSite {
    pattern: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelClick {
    id: u32,
    line: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FloatSize {
    id: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionSource {
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extension {
    #[serde(default)]
    id: String,
    action: ExtensionAction,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plugin {
    name: String,
    action: PluginAction,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClearSite {
    site: String,
}

/// A host (`example.com`, `*.example.com`) or origin (`https://example.com`),
/// as the Sites tab clears data for.
fn site(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/');
    let host = text.split_once("://").map_or(text, |(scheme, rest)| {
        if matches!(scheme, "http" | "https") {
            rest
        } else {
            ""
        }
    });
    let host = host.strip_prefix("*.").unwrap_or(host);
    let ok = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    ok.then(|| text.to_string())
}

fn setting_name(name: String) -> Result<String, String> {
    if crate::settings::find(&name).is_some() {
        Ok(name)
    } else {
        Err(format!("no setting {name:?}"))
    }
}

/// Validate a message from the page at `url`.
pub fn parse(url: &str, name: &str, json: &str) -> Result<UiMessage, String> {
    let page = sender(url).ok_or_else(|| format!("{url} may not send UI messages"))?;
    match (page, name) {
        ("settings", "set") => {
            let SettingsSet {
                name: setting,
                value,
                pattern,
            } = payload(name, json)?;
            if pattern.as_ref().is_some_and(|p| {
                p.trim().is_empty() || p.len() > 500 || p.contains(char::is_whitespace)
            }) {
                return Err(format!("{name}: bad pattern"));
            }
            Ok(UiMessage::SettingsSet {
                name: setting_name(setting)?,
                value,
                pattern,
            })
        }
        ("settings", "bind") => {
            let (mode, keys, command) = bind_message(name, json)?;
            Ok(UiMessage::SettingsBind {
                mode,
                keys,
                command,
            })
        }
        ("settings", "unbind") => {
            let (mode, keys, _) = bind_message(name, json)?;
            Ok(UiMessage::SettingsUnbind { mode, keys })
        }
        ("settings", "unset-site") => {
            let UnsetSite {
                pattern,
                name: setting,
            } = payload(name, json)?;
            if pattern.is_empty() || pattern.len() > 500 {
                return Err(format!("{name}: bad pattern"));
            }
            Ok(UiMessage::SettingsUnsetSite {
                pattern,
                name: setting_name(setting)?,
            })
        }
        ("settings", "clear-site") => {
            let ClearSite { site: text } = payload(name, json)?;
            let site =
                site(&text).ok_or_else(|| format!("{text:?} isn't a site, e.g. example.com"))?;
            Ok(UiMessage::ClearSite { site })
        }
        ("settings", "plugin") => {
            let Plugin {
                name: plugin,
                action,
            } = payload(name, json)?;
            let ok = !plugin.is_empty()
                && plugin.len() <= 100
                && plugin
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && !plugin.starts_with('.');
            if !ok {
                return Err(format!("{name}: bad plugin name"));
            }
            Ok(UiMessage::Plugin {
                name: plugin,
                action,
            })
        }
        ("settings", "extension") => {
            let Extension { id, action } = payload(name, json)?;
            let all =
                id.is_empty() && matches!(action, ExtensionAction::Check | ExtensionAction::Chrome);
            if !all && crate::extensions::parse_id(&id).as_deref() != Some(id.as_str()) {
                return Err(format!("{name}: bad extension id"));
            }
            Ok(UiMessage::Extension { id, action })
        }
        ("settings", "restart") => Ok(UiMessage::Restart),
        ("settings", "extension-install") => {
            let ExtensionSource { source } = payload(name, json)?;
            // Only the Web Store: never a file a page names.
            if source.len() > 500 || crate::extensions::parse_id(&source).is_none() {
                return Err(format!("{name}: not a Web Store page or extension id"));
            }
            Ok(UiMessage::ExtensionInstall { source })
        }
        ("settings", "reset") => {
            let SettingName { name: setting } = payload(name, json)?;
            Ok(UiMessage::SettingsReset {
                name: setting_name(setting)?,
            })
        }
        ("recover", "reopen") => {
            let Reopen { session, tabs } = payload(name, json)?;
            if tabs.is_empty() || tabs.len() > 1000 {
                return Err(format!("{name}: {} tabs", tabs.len()));
            }
            Ok(UiMessage::RecoverReopen {
                session: crashed_session(session)?,
                tabs,
            })
        }
        ("recover", "forget") => {
            let Session { session } = payload(name, json)?;
            Ok(UiMessage::RecoverForget {
                session: crashed_session(session)?,
            })
        }
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
        ("float.html", "size") => {
            let FloatSize { id, width, height } = payload(name, json)?;
            if !(1..=10_000).contains(&width) || !(1..=10_000).contains(&height) {
                return Err(format!("{name}: size {width}x{height} out of range"));
            }
            Ok(UiMessage::FloatSize { id, width, height })
        }
        ("panel.html", "click") => {
            let PanelClick { id, line } = payload(name, json)?;
            Ok(UiMessage::PanelClick { id, line })
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
    fn the_recover_page_reopens_and_forgets_only_crash_sessions() {
        let page = "riptide://recover/";
        assert!(tab_may_send(page));
        assert_eq!(
            parse(
                page,
                "reopen",
                r#"{"session":"_crashed-2026-10-07-101500","tabs":[[0,1],[1,0]]}"#
            ),
            Ok(UiMessage::RecoverReopen {
                session: "_crashed-2026-10-07-101500".into(),
                tabs: vec![(0, 1), (1, 0)],
            })
        );
        assert_eq!(
            parse(
                page,
                "forget",
                r#"{"session":"_crashed-2026-10-07-101500"}"#
            ),
            Ok(UiMessage::RecoverForget {
                session: "_crashed-2026-10-07-101500".into()
            })
        );
        for bad in [
            r#"{"session":"default"}"#,
            r#"{"session":"_crashed-../x"}"#,
            r#"{"session":"_crashed-"}"#,
        ] {
            assert!(parse(page, "forget", bad).is_err(), "{bad}");
        }
        assert!(parse(page, "reopen", r#"{"session":"_crashed-1","tabs":[]}"#).is_err());
        assert!(parse(page, "set", r#"{"name":"zoom.default","value":"100%"}"#).is_err());
        assert!(
            parse(
                "riptide://settings/",
                "forget",
                r#"{"session":"_crashed-1"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn the_extensions_tab_installs_only_from_the_web_store() {
        let page = "riptide://settings/";
        let id = "ddkjiahejlhfcafbddmgiahcphecmpfh";
        let store = format!("https://chromewebstore.google.com/detail/ublock-origin-lite/{id}");
        assert_eq!(
            parse(
                page,
                "extension-install",
                &serde_json::json!({ "source": store }).to_string()
            ),
            Ok(UiMessage::ExtensionInstall {
                source: store.clone()
            })
        );
        for bad in ["/home/a/evil.crx", "https://example.com/x", ""] {
            let json = serde_json::json!({ "source": bad }).to_string();
            assert!(parse(page, "extension-install", &json).is_err(), "{bad}");
        }
        let json = serde_json::json!({ "id": id, "action": "popup" }).to_string();
        assert_eq!(
            parse(page, "extension", &json),
            Ok(UiMessage::Extension {
                id: id.into(),
                action: ExtensionAction::Popup
            })
        );
        let json = serde_json::json!({ "id": "../x", "action": "remove" }).to_string();
        assert!(parse(page, "extension", &json).is_err());
        assert!(
            parse(
                "https://example.com/",
                "extension-install",
                &serde_json::json!({ "source": id }).to_string()
            )
            .is_err()
        );
    }

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
    fn the_settings_page_may_set_and_reset_known_settings_only() {
        let page = "riptide://settings/";
        assert!(tab_may_send(page));
        assert!(tab_may_send("riptide://settings"));
        assert!(!tab_may_send("riptide://ui/statusbar.html"));
        assert!(!tab_may_send("riptide://help/"));
        assert!(!tab_may_send("https://settings/"));
        assert_eq!(
            parse(page, "set", r#"{"name": "hints.chars", "value": "abc"}"#),
            Ok(UiMessage::SettingsSet {
                name: "hints.chars".into(),
                value: serde_json::json!("abc"),
                pattern: None,
            })
        );
        assert_eq!(
            parse(
                page,
                "set",
                r#"{"name": "content.javascript.enabled", "value": false, "pattern": "*.example.com"}"#
            ),
            Ok(UiMessage::SettingsSet {
                name: "content.javascript.enabled".into(),
                value: serde_json::json!(false),
                pattern: Some("*.example.com".into()),
            })
        );
        assert!(
            parse(
                page,
                "set",
                r#"{"name": "hints.chars", "value": "a", "pattern": "a b"}"#
            )
            .is_err()
        );
        assert_eq!(
            parse(page, "reset", r#"{"name": "hints.chars"}"#),
            Ok(UiMessage::SettingsReset {
                name: "hints.chars".into()
            })
        );
        assert_eq!(
            parse(
                page,
                "bind",
                r#"{"mode": "normal", "keys": "gx", "command": "reload"}"#
            ),
            Ok(UiMessage::SettingsBind {
                mode: crate::mode::Mode::Normal,
                keys: "gx".into(),
                command: "reload".into()
            })
        );
        assert!(
            parse(
                page,
                "bind",
                r#"{"mode": "nope", "keys": "gx", "command": "reload"}"#
            )
            .is_err()
        );
        assert!(parse(page, "unbind", r#"{"mode": "insert", "keys": ""}"#).is_err());
        assert_eq!(
            parse(page, "clear-site", r#"{"site": "https://example.com/"}"#),
            Ok(UiMessage::ClearSite {
                site: "https://example.com".into()
            })
        );
        assert!(parse(page, "clear-site", r#"{"site": "*.example.org"}"#).is_ok());
        assert_eq!(
            plugin_page("riptide://notes.plugin/index.html?page=3"),
            Some("notes")
        );
        assert_eq!(
            plugin_page_id("riptide://notes.plugin/index.html?page=3#top"),
            Some(3)
        );
        assert_eq!(plugin_page("riptide://settings/"), None);
        assert_eq!(plugin_page("riptide://a.b.plugin/"), None);
        assert_eq!(plugin_page("riptide://.plugin/"), None);
        assert_eq!(plugin_page("https://notes.plugin/"), None);
        assert_eq!(
            parse(page, "plugin", r#"{"name": "hello", "action": "check"}"#),
            Ok(UiMessage::Plugin {
                name: "hello".into(),
                action: PluginAction::Check,
            })
        );
        assert!(parse(page, "plugin", r#"{"name": "../x", "action": "remove"}"#).is_err());
        assert!(parse(page, "plugin", r#"{"name": "hello", "action": "run"}"#).is_err());
        assert!(parse(page, "clear-site", r#"{"site": "file:///etc"}"#).is_err());
        assert!(parse(page, "clear-site", r#"{"site": "a b"}"#).is_err());
        assert!(
            parse(
                page,
                "unset-site",
                r#"{"pattern": "x.com", "name": "nope"}"#
            )
            .is_err()
        );
        assert!(parse(page, "set", r#"{"name": "nope", "value": 1}"#).is_err());
        assert!(parse(page, "set", r#"{"name": "hints.chars"}"#).is_err());
        assert!(parse(page, "select-tab", r#"{"index": 0}"#).is_err());
        assert!(
            parse(
                "riptide://ui/tabbar.html",
                "set",
                r#"{"name": "hints.chars", "value": "a"}"#
            )
            .is_err()
        );
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
