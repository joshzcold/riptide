//! Site permission requests (camera, microphone, location, notifications…),
//! decided by `content.*` settings or by asking.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use rt_core::permissions::{self, Decision, Feature, MEDIA_FEATURES, PROMPT_FEATURES, Remembered};
use rt_core::prompt::{PromptAnswer, PromptKind, Remember};

use crate::prompts::{self, Scope};
use crate::shell;

thread_local! {
    static REMEMBERED: RefCell<Remembered> = RefCell::new(Remembered::default());
    /// Chromium's prompt ids → ours, so Chromium can dismiss them.
    static OPEN_PROMPTS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
}

/// How a request ended.
#[derive(Debug, PartialEq, Eq)]
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
/// Desktop audio and video capture: `getDisplayMedia`.
const DESKTOP_CAPTURE: u32 = 4 | 8;

/// A screen-share request from a call window's tab goes to Chrome's own
/// picker, which only Chrome-style tabs have; any other answer would share
/// the whole screen. `content.desktop_capture = false` still refuses it here.
fn leave_to_chrome(browser: Option<&Browser>, requested: u32) -> bool {
    let chrome_style = browser
        .and_then(|b| b.host())
        .is_some_and(|host| host.runtime_style() == RuntimeStyle::CHROME);
    let refused = shell::with(|s| s.engine.settings().str("content.desktop_capture") == "false")
        .unwrap_or(false);
    requested & DESKTOP_CAPTURE != 0 && chrome_style && !refused
}

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
    let id = prompts::ask_about(
        browser,
        Scope::Other,
        rt_core::prompt::Topic::Permission,
        "Permission request",
        message,
        kind,
        Some(origin.clone()),
        false,
        move |answer| {
            let (outcome, saved) = answer_outcome(&answer);
            if let Some(allow) = saved {
                if !site {
                    REMEMBERED.with(|r| r.borrow_mut().remember(&origin, bits, features, allow));
                }
                save_for_site(&origin, bits, features, allow);
            }
            done(outcome);
        },
    );
    Some(id)
}

/// An "always" answer becomes a per-site setting in `autoconfig.toml`, as in
/// qutebrowser, so it survives restarts and can be changed with `:set -u`.
fn save_for_site(origin: &str, bits: u32, features: &[Feature], allow: bool) {
    let (pattern, names) = site_settings(origin, bits, features);
    let value = rt_core::settings::Value::Str(allow.to_string());
    for name in names {
        let op = rt_core::config::ConfigOp::SetFor {
            pattern: pattern.clone(),
            name: name.to_string(),
            value: value.clone(),
        };
        if shell::with(|s| s.engine.apply_config(&op)).is_some_and(|r| r.is_ok()) {
            shell::apply(vec![rt_core::Effect::ConfigChanged(op)]);
        }
    }
}

/// What an answer does, and whether it's kept ("always"): `Some(allow)`.
fn answer_outcome(answer: &PromptAnswer) -> (Outcome, Option<bool>) {
    match answer {
        PromptAnswer::Yes { remember } => (Outcome::Allow, remember.then_some(true)),
        PromptAnswer::No { remember: true } => (Outcome::Block, Some(false)),
        _ => (Outcome::NotNow, None),
    }
}

/// The `per_domain` pattern and the settings an "always" answer for the
/// requested `bits` is saved as, each once.
fn site_settings(origin: &str, bits: u32, features: &[Feature]) -> (String, Vec<&'static str>) {
    let mut names: Vec<&'static str> = Vec::new();
    for name in features
        .iter()
        .filter(|f| bits & f.bit != 0)
        .filter_map(|f| f.setting)
    {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    (origin.trim_end_matches('/').to_string(), names)
}

wrap_permission_handler! {
    pub struct RtPermissionHandler {}

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
            if leave_to_chrome(browser.as_deref(), requested_permissions) {
                return 0;
            }
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
    settings: &rt_core::settings::Settings,
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
                rt_core::settings::Value::Str(v) if v == "true" => ContentSettingValues::ALLOW,
                rt_core::settings::Value::Str(v) if v == "false" => ContentSettingValues::BLOCK,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_allow_block_or_wait() {
        assert_eq!(
            answer_outcome(&PromptAnswer::Yes { remember: false }),
            (Outcome::Allow, None)
        );
        assert_eq!(
            answer_outcome(&PromptAnswer::Yes { remember: true }),
            (Outcome::Allow, Some(true))
        );
        assert_eq!(
            answer_outcome(&PromptAnswer::No { remember: true }),
            (Outcome::Block, Some(false))
        );
        assert_eq!(
            answer_outcome(&PromptAnswer::No { remember: false }),
            (Outcome::NotNow, None)
        );
        assert_eq!(
            answer_outcome(&PromptAnswer::Cancelled),
            (Outcome::NotNow, None)
        );
    }

    #[test]
    fn always_answers_save_each_requested_setting_once() {
        // Desktop audio and screen share one setting.
        let (pattern, names) = site_settings("https://meet.example/", 1 | 4 | 8, MEDIA_FEATURES);
        assert_eq!(pattern, "https://meet.example");
        assert_eq!(
            names,
            ["content.media.audio_capture", "content.desktop_capture"]
        );
        let (_, names) = site_settings("https://a.example/", 256, PROMPT_FEATURES);
        assert_eq!(names, ["content.geolocation"]);
        // The clipboard has no setting, so nothing is saved for it.
        let (_, names) = site_settings("https://a.example/", 16, PROMPT_FEATURES);
        assert!(names.is_empty());
    }
}
