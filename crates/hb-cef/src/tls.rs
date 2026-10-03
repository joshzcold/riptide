//! Untrusted TLS certificates: ask, block or load, per
//! `content.tls.certificate_errors` (which can be set per site).

use cef::*;
use hb_core::config::ConfigOp;
use hb_core::prompt::{PromptAnswer, PromptKind, Remember};
use hb_core::settings::Value;

use crate::prompts::{self, Scope};
use crate::shell;

/// Chromium's net error for a certificate problem, in words.
fn describe(code: i32) -> String {
    match code {
        -200 => "it is for a different site".into(),
        -201 => "it has expired or isn't valid yet".into(),
        -202 => "it was issued by an unknown authority".into(),
        -203 => "it contains errors".into(),
        -206 => "it has been revoked".into(),
        -208 => "it uses a weak signature algorithm".into(),
        -213 => "its name constraints are violated".into(),
        -218 => "it uses a weak key".into(),
        code => format!("certificate error {code}"),
    }
}

/// `scheme://host[:port]` of a URL, the pattern an "always" answer is saved for.
fn origin(url: &str) -> String {
    let Some(start) = url.find("://").map(|i| i + 3) else {
        return url.to_string();
    };
    let end = url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| start + i);
    url[..end].to_string()
}

/// Returns true if CEF should wait for `callback`.
pub fn certificate_error(browser: Option<i32>, code: i32, url: &str, callback: Callback) -> bool {
    let site = origin(url);
    let setting = shell::with(|s| {
        s.engine
            .settings()
            .str_for("content.tls.certificate_errors", url)
            .to_string()
    })
    .unwrap_or_default();
    match setting.as_str() {
        "load-insecurely" => {
            tracing::warn!(url, code, "loading despite an untrusted certificate");
            callback.cont();
            return true;
        }
        "block" => return false,
        _ => {}
    }
    let host = hb_core::url::host(url).to_string();
    let message = format!(
        "The certificate for {host} isn't trusted: {}. Load it anyway?",
        describe(code)
    );
    let kind = PromptKind::YesNo {
        default: false,
        remember: Remember::Always,
    };
    prompts::ask(
        browser,
        Scope::Other,
        "Untrusted certificate",
        message,
        kind,
        move |answer| match answer {
            PromptAnswer::Yes { remember } => {
                if remember {
                    save(&site, "load-insecurely");
                }
                callback.cont();
            }
            PromptAnswer::No { remember } => {
                if remember {
                    save(&site, "block");
                }
                callback.cancel();
            }
            _ => callback.cancel(),
        },
    );
    true
}

fn save(site: &str, value: &str) {
    let op = ConfigOp::SetFor {
        pattern: site.to_string(),
        name: "content.tls.certificate_errors".into(),
        value: Value::Str(value.into()),
    };
    if shell::with(|s| s.engine.apply_config(&op)).is_some_and(|r| r.is_ok()) {
        shell::apply(vec![hb_core::Effect::ConfigChanged(op)]);
    }
}
