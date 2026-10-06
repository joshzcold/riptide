//! Content settings that map onto Chromium: JavaScript, images, sound,
//! popups, clipboard, cookies and the user agent.
//! Globals apply when settings change; per-site values apply just before
//! each navigation, so URL patterns work too.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use cef::*;
use rt_core::settings::{Settings, Value};

use crate::shell;

/// Settings that are one Chromium content setting each, with how a value
/// maps to Chromium's.
const SITE_SETTINGS: &[(&str, ContentSettingTypes)] = &[
    (
        "content.javascript.enabled",
        ContentSettingTypes::JAVASCRIPT,
    ),
    ("content.images", ContentSettingTypes::IMAGES),
    ("content.mute", ContentSettingTypes::SOUND),
    (
        "content.javascript.can_open_tabs_automatically",
        ContentSettingTypes::POPUPS,
    ),
    (
        "content.javascript.clipboard",
        ContentSettingTypes::CLIPBOARD_READ_WRITE,
    ),
];

fn chromium_value(name: &str, value: &Value) -> ContentSettingValues {
    let on = match value {
        Value::Bool(b) => *b,
        Value::Str(s) => match s.as_str() {
            "true" | "access-paste" => true,
            "ask" | "access" => return ContentSettingValues::ASK,
            _ => false,
        },
        _ => return ContentSettingValues::DEFAULT,
    };
    // content.mute is the other way round: true blocks sound.
    let allow = if name == "content.mute" { !on } else { on };
    if allow {
        ContentSettingValues::ALLOW
    } else {
        ContentSettingValues::BLOCK
    }
}

#[derive(Default)]
struct State {
    /// The global values last given to Chromium, by setting.
    applied: HashMap<&'static str, String>,
    /// Whether the protocol handler default stored by an earlier version is gone.
    cleared_protocol_handlers: bool,
    /// The header preferences last given to Chromium.
    prefs: Option<String>,
    /// The cookie settings last given to Chromium.
    cookies: Option<(String, bool)>,
    /// (setting, origin) pairs given their own value, to undo when no override matches.
    site_values: HashSet<(&'static str, String)>,
    /// The user agent last set per browser id ("" is Chromium's own).
    user_agents: HashMap<i32, String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// What the IO thread changes in every request: `content.headers.referer`
/// and `content.headers.custom`.
#[derive(Clone, Debug, Default, PartialEq)]
struct HeaderRules {
    referer: String,
    /// `content.headers.accept_language`; empty leaves Chromium's.
    accept_language: String,
    custom: Vec<(String, String)>,
}

static HEADER_RULES: std::sync::RwLock<Option<HeaderRules>> = std::sync::RwLock::new(None);

/// On the IO thread, before a request goes out: drop a referrer
/// `content.headers.referer` doesn't allow and add the custom headers.
pub fn before_request(request: &mut Request) {
    let Some(rules) = HEADER_RULES.read().ok().and_then(|r| r.clone()) else {
        return;
    };
    let referrer = CefString::from(&request.referrer_url()).to_string();
    if !referrer.is_empty() {
        let url = CefString::from(&request.url()).to_string();
        if !rt_core::url::keep_referrer(&rules.referer, &referrer, &url) {
            request.set_referrer(None, ReferrerPolicy::NO_REFERRER);
        }
    }
    if !rules.accept_language.is_empty() {
        request.set_header_by_name(
            Some(&CefString::from("Accept-Language")),
            Some(&CefString::from(rules.accept_language.as_str())),
            1,
        );
    }
    for (name, value) in &rules.custom {
        request.set_header_by_name(
            Some(&CefString::from(name.as_str())),
            Some(&CefString::from(value.as_str())),
            1,
        );
    }
}

/// The settings that are profile preferences (`content.headers.do_not_track`,
/// `content.dns_prefetch`, `content.webrtc_ip_handling_policy`,
/// `content.proxy`, `content.pdf_viewer`): set on the profile and on the
/// private windows' context.
pub fn apply_prefs(context: &RequestContext, settings: &Settings) {
    let set = |name: &str, value: Option<cef::Value>| {
        let Some(mut value) = value else { return };
        let mut error = CefString::from(" ");
        let pref = CefString::from(name);
        if context.set_preference(Some(&pref), Some(&mut value), Some(&mut error)) == 0 {
            tracing::warn!(%error, name, "could not set a preference");
        }
    };
    set(
        "enable_do_not_track",
        value_create().inspect(|v| {
            v.set_bool(settings.bool("content.headers.do_not_track").into());
        }),
    );
    set(
        "plugins.always_open_pdf_externally",
        value_create().inspect(|v| {
            v.set_bool((!settings.bool("content.pdf_viewer")).into());
        }),
    );
    // Chromium's NetworkPredictionOptions: 0 standard, 2 disabled.
    let prediction = if settings.bool("content.dns_prefetch") {
        0
    } else {
        2
    };
    set(
        "net.network_prediction_options",
        value_create().inspect(|v| {
            v.set_int(prediction);
        }),
    );
    let policy = rt_core::network::webrtc_policy(settings.str("content.webrtc_ip_handling_policy"));
    set(
        "webrtc.ip_handling_policy",
        value_create().inspect(|v| {
            v.set_string(Some(&CefString::from(policy)));
        }),
    );
    // The setting's validator already rejected anything unparsable.
    if let Ok(proxy) = rt_core::network::parse_proxy(settings.str("content.proxy"))
        && let Some(dict) = dictionary_value_create()
    {
        let (mode, extra) = proxy.pref();
        dict.set_string(Some(&CefString::from("mode")), Some(&CefString::from(mode)));
        if let Some((key, value)) = extra {
            dict.set_string(Some(&CefString::from(key)), Some(&CefString::from(value)));
        }
        set(
            "proxy",
            value_create().inspect(|v| {
                v.set_dictionary(Some(&mut dict.clone()));
            }),
        );
    }
}

/// Apply the content settings as Chromium's defaults for every site.
pub fn apply_globals(settings: &Settings) {
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    let rules = HeaderRules {
        referer: settings.str("content.headers.referer").to_string(),
        accept_language: settings.str("content.headers.accept_language").to_string(),
        custom: settings
            .map("content.headers.custom")
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };
    if let Ok(mut current) = HEADER_RULES.write() {
        *current = Some(rules);
    }
    let prefs = [
        "content.headers.do_not_track",
        "content.dns_prefetch",
        "content.webrtc_ip_handling_policy",
        "content.proxy",
        "content.pdf_viewer",
    ]
    .map(|name| {
        settings
            .get(name)
            .map(ToString::to_string)
            .unwrap_or_default()
    })
    .join("\n");
    if STATE.with(|s| s.borrow().prefs.as_ref() != Some(&prefs)) {
        apply_prefs(&context, settings);
        if let Some(private) = shell::with(|s| s.private_context.clone()).flatten() {
            apply_prefs(&private, settings);
        }
        STATE.with(|s| s.borrow_mut().prefs = Some(prefs));
    }
    // A stored PROTOCOL_HANDLERS default makes private windows crash a Chromium CHECK.
    if !STATE.with(|s| std::mem::replace(&mut s.borrow_mut().cleared_protocol_handlers, true)) {
        context.set_content_setting(
            None,
            None,
            ContentSettingTypes::PROTOCOL_HANDLERS,
            ContentSettingValues::DEFAULT,
        );
    }
    for (name, kind) in SITE_SETTINGS {
        let Some(value) = settings.get(name) else {
            continue;
        };
        let text = value.to_string();
        if STATE.with(|s| s.borrow().applied.get(name) == Some(&text)) {
            continue;
        }
        // Empty URLs set the default for every site.
        context.set_content_setting(None, None, *kind, chromium_value(name, value));
        STATE.with(|s| s.borrow_mut().applied.insert(name, text));
    }
    let cookies = (
        settings.str("content.cookies.accept").to_string(),
        settings.bool("content.cookies.store"),
    );
    if STATE.with(|s| s.borrow().cookies.as_ref() == Some(&cookies)) {
        return;
    }
    let (accept, store) = &cookies;
    let value = match (accept.as_str(), store) {
        ("never", _) => ContentSettingValues::BLOCK,
        (_, false) => ContentSettingValues::SESSION_ONLY,
        _ => ContentSettingValues::ALLOW,
    };
    context.set_content_setting(None, None, ContentSettingTypes::COOKIES, value);
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
    STATE.with(|s| s.borrow_mut().cookies = Some(cookies));
}

/// A tab is about to load `url`: give its site the content settings and
/// user agent the per-site settings ask for.
pub fn before_navigation(browser: &Browser, url: &str) {
    let Some((site, user_agent)) = shell::with(|s| {
        let settings = s.engine.settings();
        let site: Vec<(
            &'static str,
            ContentSettingTypes,
            Option<ContentSettingValues>,
        )> = SITE_SETTINGS
            .iter()
            .map(|(name, kind)| {
                let here = settings.get_for(name, url);
                let global = settings.get(name);
                let differs = here != global;
                (
                    *name,
                    *kind,
                    here.filter(|_| differs).map(|v| chromium_value(name, v)),
                )
            })
            .collect();
        (
            site,
            settings
                .str_for("content.headers.user_agent", url)
                .to_string(),
        )
    }) else {
        return;
    };
    if let Some(origin) = rt_core::url::origin(url) {
        for (name, kind, value) in site {
            set_site_value(name, kind, &origin, value);
        }
    }
    set_user_agent(browser, &user_agent);
}

/// Give `origin` its own value for one content setting, or (`None`) take it away.
fn set_site_value(
    name: &'static str,
    kind: ContentSettingTypes,
    origin: &str,
    value: Option<ContentSettingValues>,
) {
    let key = (name, origin.to_string());
    let known = STATE.with(|s| s.borrow().site_values.contains(&key));
    if value.is_none() && !known {
        return;
    }
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    let url = CefString::from(origin);
    context.set_content_setting(
        Some(&url),
        Some(&url),
        kind,
        value.unwrap_or(ContentSettingValues::DEFAULT),
    );
    STATE.with(|s| {
        let set = &mut s.borrow_mut().site_values;
        if value.is_some() {
            set.insert(key);
        } else {
            set.remove(&key);
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
