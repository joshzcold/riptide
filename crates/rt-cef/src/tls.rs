//! Untrusted TLS certificates: ask, block or load, per
//! `content.tls.certificate_errors` (which can be set per site).

use cef::*;
use rt_core::config::ConfigOp;
use rt_core::prompt::{PromptAnswer, PromptKind, Remember};
use rt_core::settings::Value;

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

/// `scheme://host[:port]` of a URL, the pattern an "always" answer is saved
/// for. User info is dropped, so a password in the URL never reaches the config.
fn origin(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    format!("{scheme}://{authority}")
}

/// What `content.tls.certificate_errors` says to do before asking anyone.
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Load,
    Block,
    Ask,
}

fn decide(setting: &str) -> Decision {
    match setting {
        "load-insecurely" => Decision::Load,
        "block" => Decision::Block,
        _ => Decision::Ask,
    }
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
    match decide(&setting) {
        Decision::Load => {
            tracing::warn!(url, code, "loading despite an untrusted certificate");
            callback.cont();
            return true;
        }
        Decision::Block => return false,
        Decision::Ask => {}
    }
    let host = rt_core::url::host(url).to_string();
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
        shell::apply(vec![rt_core::Effect::ConfigChanged(op)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_scheme_host_and_port_without_user_info() {
        assert_eq!(origin("https://a.example:8443/x?y#z"), "https://a.example:8443");
        assert_eq!(origin("https://a.example"), "https://a.example");
        assert_eq!(origin("https://a.example?q"), "https://a.example");
        assert_eq!(origin("https://me:secret@a.example/x"), "https://a.example");
        assert_eq!(origin("not a url"), "not a url");
    }

    #[test]
    fn the_setting_decides_before_asking() {
        assert_eq!(decide("load-insecurely"), Decision::Load);
        assert_eq!(decide("block"), Decision::Block);
        assert_eq!(decide("ask"), Decision::Ask);
        assert_eq!(decide(""), Decision::Ask);
    }

    #[test]
    fn certificate_errors_are_described_in_words() {
        assert_eq!(describe(-201), "it has expired or isn't valid yet");
        assert_eq!(describe(-202), "it was issued by an unknown authority");
        assert_eq!(describe(-299), "certificate error -299");
    }
}
