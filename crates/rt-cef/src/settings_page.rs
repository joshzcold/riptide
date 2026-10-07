//! `riptide://settings/` (`:settings`): every setting, editable. The page is
//! served with a snapshot of the settings and gets fresh data after each
//! change; its edits come back as `set` and `reset` UI messages.

use std::sync::{Arc, RwLock};

use cef::ImplBrowser;

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
    shell::with(|s| {
        let entries = rt_core::settings_page::build(
            s.engine.settings(),
            s.engine.keymap(),
            &s.setting_sources,
            &s.overridden,
        );
        // `</` would end the inline <script> early.
        serde_json::to_string(&entries)
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
    if !matches!(command, Command::Settings) {
        return false;
    }
    refresh();
    shell::open(OpenTarget::Tab, true, Some(URL.to_string()));
    true
}

/// A `set` message from the page.
pub fn set(name: &str, value: &serde_json::Value) {
    let result = shell::with(|s| s.engine.set_from_page(name, value));
    apply(name, result);
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
