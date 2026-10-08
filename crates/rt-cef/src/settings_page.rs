//! `riptide://settings/` (`:settings`): every setting, editable. The page is
//! served with a snapshot of the settings and gets fresh data after each
//! change; its edits come back as `set` and `reset` UI messages.

use std::sync::{Arc, RwLock};

use cef::*;

use rt_core::command::{Command, OpenTarget};

use crate::shell;

pub const URL: &str = "riptide://settings/";

const TEMPLATE: &str = include_str!("../ui/settings.html");

/// The page as served; rebuilt by [`refresh`] because pages are served on the IO thread.
static PAGE: RwLock<Option<Arc<[u8]>>> = RwLock::new(None);

pub fn page() -> Arc<[u8]> {
    PAGE.read()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_else(|| Arc::from(TEMPLATE.as_bytes()))
}

/// The settings as JSON, safe inside an inline `<script>`.
fn data() -> Option<String> {
    let plugins = crate::plugins::page_data();
    shell::with(|s| {
        let entries = rt_core::settings_page::build(
            s.engine.settings(),
            s.engine.keymap(),
            &s.setting_sources,
            &s.overridden,
        );
        let mut json = serde_json::to_value(&entries).unwrap_or_default();
        json["plugins"] = plugins;
        // `</` would end the inline <script> early.
        serde_json::to_string(&json)
            .unwrap_or_else(|_| "[]".into())
            .replace("</", "<\\/")
    })
}

/// The main frames of tabs showing the settings page.
fn open_pages() -> Vec<cef::Frame> {
    shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| t.url.starts_with(URL))
            .filter_map(|t| t.browser()?.main_frame())
            .collect()
    })
    .unwrap_or_default()
}

/// After any settings change: a new snapshot, and fresh data in open pages.
pub fn refresh() {
    let Some(json) = data() else { return };
    if let Ok(mut page) = PAGE.write() {
        *page = Some(Arc::from(
            TEMPLATE.replace("/*RT_DATA*/null", &json).into_bytes(),
        ));
    }
    for frame in open_pages() {
        shell::exec_js(&frame, &format!("rtSettings({json})"));
    }
}

/// Tell open settings pages why a change to `name` was refused.
fn refused(name: &str, error: &str) {
    let json = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let call = format!("rtSettingsError({}, {})", json(name), json(error));
    for frame in open_pages() {
        shell::exec_js(&frame, &call);
    }
}

pub fn run_command(command: &Command) -> bool {
    let url = match command {
        Command::Settings => URL.to_string(),
        Command::Plugins => format!("{URL}#plugins"),
        _ => return false,
    };
    refresh();
    shell::open(OpenTarget::Tab, true, Some(url));
    true
}

/// A `set` message from the page.
pub fn set(name: &str, value: &serde_json::Value, pattern: Option<&str>) {
    let result = shell::with(|s| s.engine.set_from_page(name, value, pattern));
    apply(name, result);
}

/// In normal mode on the settings page, with nothing typed yet, `j` and `k`
/// move between settings and `Return` edits one: the page gets them instead
/// of riptide. True if the key was handed over.
pub fn forward_key(key: &rt_core::key::Key) -> bool {
    use rt_core::key::KeyCode;
    let name = match key.code {
        _ if !key.mods.is_empty() => return false,
        KeyCode::Char('j') => "j",
        KeyCode::Char('k') => "k",
        KeyCode::Enter => "Return",
        _ => return false,
    };
    let frame = shell::with(|s| {
        if s.engine.mode() != rt_core::Mode::Normal || !s.engine.status().keystring.is_empty() {
            return None;
        }
        let tab = s.tabs.current()?;
        if !tab.url.starts_with(URL) {
            return None;
        }
        tab.browser()?.main_frame()
    })
    .flatten();
    let Some(frame) = frame else { return false };
    shell::exec_js(&frame, &format!("window.rtKey?.({name:?})"));
    true
}

/// A `bind` message from the Keys tab. Its refusals are shown under the name `keys`.
pub fn bind(mode: rt_core::Mode, keys: &str, command: &str) {
    let result = shell::with(|s| s.engine.bind_from_page(mode, keys, command));
    apply("keys", result);
}

/// An `unbind` message from the Keys tab.
pub fn unbind(mode: rt_core::Mode, keys: &str) {
    let result = shell::with(|s| s.engine.unbind_from_page(mode, keys));
    apply("keys", result);
}

/// The Sites tab forgot `name` for `pattern`; refusals show under the name `sites`.
pub fn unset_site(pattern: &str, name: &str) {
    let result = shell::with(|s| s.engine.unset_site_from_page(pattern, name));
    apply("sites", result);
}

/// The origins a Sites tab entry stands for: itself if it's an origin, or
/// the host over https and http.
fn origins(site: &str) -> Vec<String> {
    if site.contains("://") {
        return vec![site.to_string()];
    }
    let host = site.strip_prefix("*.").unwrap_or(site);
    vec![format!("https://{host}"), format!("http://{host}")]
}

/// Clear `site`'s cookies and stored data (local storage, IndexedDB, cache…).
pub fn clear_site(site: &str) {
    let origins = origins(site);
    if let Some(manager) = cookie_manager_get_global_manager(None) {
        for origin in &origins {
            manager.delete_cookies(Some(&CefString::from(origin.as_str())), None, None);
        }
    }
    // The Storage domain is the whole profile's; any tab can ask.
    let host = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.host());
    if let Some(host) = host {
        for (id, origin) in (1000..).zip(&origins) {
            let message = serde_json::json!({
                "id": id,
                "method": "Storage.clearDataForOrigin",
                "params": { "origin": origin, "storageTypes": "all" },
            })
            .to_string();
            host.send_dev_tools_message(Some(message.as_bytes()));
        }
    }
    shell::show_message(
        rt_core::engine::Level::Info,
        format!("Cleared cookies and site data for {site}"),
    );
    shell::refresh_ui();
}

/// A `reset` message from the page.
pub fn reset(name: &str) {
    let result = shell::with(|s| s.engine.reset_from_page(name));
    apply(name, result);
}

fn apply(name: &str, result: Option<Result<Vec<rt_core::engine::Effect>, String>>) {
    match result {
        // Saving the change refreshes the page (see `shell::persist`).
        Some(Ok(effects)) => shell::apply(effects),
        Some(Err(error)) => refused(name, &error),
        None => {}
    }
    shell::refresh_ui();
}
