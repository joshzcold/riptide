//! Site permission requests (camera, microphone, location, notifications…),
//! decided by `content.*` settings or by asking.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use hb_core::permissions::{self, Decision, Feature, MEDIA_FEATURES, PROMPT_FEATURES, Remembered};
use hb_core::prompt::{PromptAnswer, PromptKind, Remember};

use crate::prompts::{self, Scope};
use crate::shell;

thread_local! {
    static REMEMBERED: RefCell<Remembered> = RefCell::new(Remembered::default());
    /// Chromium's prompt ids → ours, so Chromium can dismiss them.
    static OPEN_PROMPTS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
}

/// How a request ended.
enum Outcome {
    Allow,
    /// The user chose to block (Chromium saves this for permission prompts).
    Block,
    /// "Not now": dismissed, nothing saved.
    NotNow,
    /// Refused by a `content.*` setting; never saved, so the setting stays in charge.
    Refused,
}

/// Decide from settings, or ask and call `done` later. Returns the prompt id if it
/// asked. `site` means Chromium saves the answer per site itself (permission
/// prompts); otherwise A/N answers are kept for the session (camera/microphone).
fn resolve(
    browser: Option<i32>,
    origin: String,
    bits: u32,
    features: &'static [Feature],
    site: bool,
    done: impl FnOnce(Outcome) + 'static,
) -> Option<u64> {
    let decision = shell::with(|s| {
        REMEMBERED.with(|r| {
            let empty = Remembered::default();
            let remembered = if site { &empty } else { &r.borrow() };
            permissions::decide(&origin, bits, features, s.engine.settings(), remembered)
        })
    });
    let message = match decision.unwrap_or(Decision::Deny) {
        Decision::Allow => {
            done(Outcome::Allow);
            return None;
        }
        Decision::Deny => {
            done(Outcome::Refused);
            return None;
        }
        Decision::Ask(message) => message,
    };
    let remember = if site {
        Remember::Site
    } else {
        Remember::Always
    };
    let kind = PromptKind::YesNo {
        default: false,
        remember,
    };
    let id = prompts::ask(
        browser,
        Scope::Other,
        "Permission request",
        message,
        kind,
        move |answer| {
            let outcome = match answer {
                PromptAnswer::Yes { remember } => {
                    if remember {
                        if !site {
                            REMEMBERED
                                .with(|r| r.borrow_mut().remember(&origin, bits, features, true));
                        }
                        save_for_site(&origin, bits, features, true);
                    }
                    Outcome::Allow
                }
                PromptAnswer::No { remember: true } => {
                    if !site {
                        REMEMBERED
                            .with(|r| r.borrow_mut().remember(&origin, bits, features, false));
                    }
                    save_for_site(&origin, bits, features, false);
                    Outcome::Block
                }
                _ => Outcome::NotNow,
            };
            done(outcome);
        },
    );
    Some(id)
}

/// An "always" answer becomes a per-site setting in `autoconfig.toml`, as in
/// qutebrowser, so it survives restarts and can be changed with `:set -u`.
fn save_for_site(origin: &str, bits: u32, features: &[Feature], allow: bool) {
    let pattern = origin.trim_end_matches('/').to_string();
    let value = hb_core::settings::Value::Str(allow.to_string());
    let mut names: Vec<&str> = features
        .iter()
        .filter(|f| bits & f.bit != 0)
        .filter_map(|f| f.setting)
        .collect();
    names.dedup();
    for name in names {
        let op = hb_core::config::ConfigOp::SetFor {
            pattern: pattern.clone(),
            name: name.to_string(),
            value: value.clone(),
        };
        if shell::with(|s| s.engine.apply_config(&op)).is_some_and(|r| r.is_ok()) {
            shell::apply(vec![hb_core::Effect::ConfigChanged(op)]);
        }
    }
}

wrap_permission_handler! {
    pub struct HbPermissionHandler {}

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            resolve(browser.map(|b| b.identifier()), origin, requested_permissions, MEDIA_FEATURES, false, move |outcome| {
                match outcome {
                    Outcome::Allow => callback.cont(requested_permissions),
                    _ => callback.cancel(),
                }
            });
            1
        }

        fn on_show_permission_prompt(
            &self,
            browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback.map(|c| c.clone()) else { return 0 };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let ours = resolve(browser.map(|b| b.identifier()), origin, requested_permissions, PROMPT_FEATURES, true, move |outcome| {
                OPEN_PROMPTS.with(|p| p.borrow_mut().remove(&prompt_id));
                callback.cont(match outcome {
                    Outcome::Allow => PermissionRequestResult::ACCEPT,
                    Outcome::Block => PermissionRequestResult::DENY,
                    Outcome::NotNow => PermissionRequestResult::DISMISS,
                    Outcome::Refused => PermissionRequestResult::IGNORE,
                });
            });
            if let Some(ours) = ours {
                OPEN_PROMPTS.with(|p| p.borrow_mut().insert(prompt_id, ours));
            }
            1
        }

        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            _result: PermissionRequestResult,
        ) {
            if let Some(ours) = OPEN_PROMPTS.with(|p| p.borrow_mut().remove(&prompt_id)) {
                prompts::withdraw(ours);
            }
        }
    }
}

/// Per-site permission settings Chromium also keeps itself: it remembers
/// `y` answers and then never asks us, so a later per-site `false` must be
/// written into Chromium's content settings too. Only exact origins
/// (`https://host[:port]`, what `A`/`N` save) can be; wildcard patterns
/// only apply when Chromium asks.
pub fn sync_site_settings(
    settings: &hb_core::settings::Settings,
) -> Vec<(String, ContentSettingTypes, ContentSettingValues)> {
    const TYPES: &[(&str, ContentSettingTypes)] = &[
        ("content.geolocation", ContentSettingTypes::GEOLOCATION),
        (
            "content.notifications.enabled",
            ContentSettingTypes::NOTIFICATIONS,
        ),
        (
            "content.media.audio_capture",
            ContentSettingTypes::MEDIASTREAM_MIC,
        ),
        (
            "content.media.video_capture",
            ContentSettingTypes::MEDIASTREAM_CAMERA,
        ),
    ];
    let mut out = Vec::new();
    for (name, kind) in TYPES {
        for (pattern, value) in settings.overrides(name) {
            let origin = pattern.trim_end_matches('/');
            let exact = (origin.starts_with("https://") || origin.starts_with("http://"))
                && !origin.contains('*')
                && origin
                    .split_once("://")
                    .is_some_and(|(_, rest)| !rest.contains('/'));
            if !exact {
                continue;
            }
            let value = match value {
                hb_core::settings::Value::Str(v) if v == "true" => ContentSettingValues::ALLOW,
                hb_core::settings::Value::Str(v) if v == "false" => ContentSettingValues::BLOCK,
                _ => ContentSettingValues::DEFAULT,
            };
            out.push((format!("{origin}/"), *kind, value));
        }
    }
    out
}

/// Write [`sync_site_settings`] into Chromium. Outside the shell borrow.
pub fn apply_site_settings(entries: Vec<(String, ContentSettingTypes, ContentSettingValues)>) {
    let Some(context) = request_context_get_global_context() else {
        return;
    };
    for (url, kind, value) in entries {
        let url = CefString::from(url.as_str());
        if context.content_setting(Some(&url), Some(&url), kind) != value {
            context.set_content_setting(Some(&url), Some(&url), kind, value);
        }
    }
}
