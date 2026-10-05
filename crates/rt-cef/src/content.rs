//! Content settings that map onto Chromium: JavaScript, cookies and the
//! user agent. Globals apply when settings change; per-site values apply
//! just before each navigation, so URL patterns work too.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use cef::*;
use rt_core::settings::Settings;

use crate::shell;

#[derive(Default)]
struct State {
    /// The global values last given to Chromium.
    applied: Option<(bool, String, bool)>,
    /// Origins given their own JavaScript setting, to undo when no override matches.
    javascript_origins: HashSet<String>,
    /// The user agent last set per browser id ("" is Chromium's own).
    user_agents: HashMap<i32, String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Apply `content.javascript.enabled`, `content.cookies.accept` and
/// `content.cookies.store` as Chromium's defaults.
pub fn apply_globals(settings: &Settings) {
    let wanted = (
        settings.bool("content.javascript.enabled"),
        settings.str("content.cookies.accept").to_string(),
        settings.bool("content.cookies.store"),
    );
    if STATE.with(|s| s.borrow().applied.as_ref() == Some(&wanted)) {
        return;
    }
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    let (javascript, accept, store) = &wanted;
    // Empty URLs set the default for every site.
    let allow = |on: bool| {
        if on {
            ContentSettingValues::ALLOW
        } else {
            ContentSettingValues::BLOCK
        }
    };
    context.set_content_setting(
        None,
        None,
        ContentSettingTypes::JAVASCRIPT,
        allow(*javascript),
    );
    let cookies = match (accept.as_str(), store) {
        ("never", _) => ContentSettingValues::BLOCK,
        (_, false) => ContentSettingValues::SESSION_ONLY,
        _ => ContentSettingValues::ALLOW,
    };
    context.set_content_setting(None, None, ContentSettingTypes::COOKIES, cookies);
    // 0: allow third-party cookies, 1: block them.
    let third_party = i32::from(matches!(
        accept.as_str(),
        "no-3rdparty" | "no-unknown-3rdparty"
    ));
    if let Some(mut value) = value_create() {
        value.set_int(third_party);
        // CEF rejects a null error string, which is what an empty CefString becomes.
        let mut error = CefString::from(" ");
        let name = CefString::from("profile.cookie_controls_mode");
        if context.set_preference(Some(&name), Some(&mut value), Some(&mut error)) == 0 {
            tracing::warn!(%error, "could not set the third-party cookie preference");
        }
    }
    STATE.with(|s| s.borrow_mut().applied = Some(wanted));
}

/// A tab is about to load `url`: give its site the JavaScript setting and
/// user agent the per-site settings ask for.
pub fn before_navigation(browser: &Browser, url: &str) {
    let Some((javascript, global_javascript, user_agent)) = shell::with(|s| {
        let settings = s.engine.settings();
        (
            settings.bool_for("content.javascript.enabled", url),
            settings.bool("content.javascript.enabled"),
            settings
                .str_for("content.headers.user_agent", url)
                .to_string(),
        )
    }) else {
        return;
    };
    if let Some(origin) = rt_core::url::origin(url) {
        set_site_javascript(
            &origin,
            (javascript != global_javascript).then_some(javascript),
        );
    }
    set_user_agent(browser, &user_agent);
}

/// Give `origin` its own JavaScript setting, or (`None`) take it away.
fn set_site_javascript(origin: &str, value: Option<bool>) {
    let known = STATE.with(|s| s.borrow().javascript_origins.contains(origin));
    if value.is_none() && !known {
        return;
    }
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    let setting = match value {
        Some(true) => ContentSettingValues::ALLOW,
        Some(false) => ContentSettingValues::BLOCK,
        None => ContentSettingValues::DEFAULT,
    };
    let url = CefString::from(origin);
    context.set_content_setting(
        Some(&url),
        Some(&url),
        ContentSettingTypes::JAVASCRIPT,
        setting,
    );
    STATE.with(|s| {
        let origins = &mut s.borrow_mut().javascript_origins;
        if value.is_some() {
            origins.insert(origin.to_string());
        } else {
            origins.remove(origin);
        }
    });
}

/// `Emulation.setUserAgentOverride` changes both the request header and
/// `navigator.userAgent`; an empty string goes back to Chromium's own.
fn set_user_agent(browser: &Browser, user_agent: &str) {
    let id = browser.identifier();
    let current = STATE.with(|s| s.borrow().user_agents.get(&id).cloned().unwrap_or_default());
    if current == user_agent {
        return;
    }
    let Some(host) = browser.host() else { return };
    let message = serde_json::json!({
        "id": 1,
        "method": "Emulation.setUserAgentOverride",
        "params": { "userAgent": user_agent },
    })
    .to_string();
    if host.send_dev_tools_message(Some(message.as_bytes())) != 0 {
        STATE.with(|s| {
            s.borrow_mut()
                .user_agents
                .insert(id, user_agent.to_string())
        });
    }
}
