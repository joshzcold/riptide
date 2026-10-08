//! `rt.page.*`: Lua acting on the current tab's page. A plugin may only act on
//! the sites its `pages` permission names, checked here against the tab (and
//! the focused frame) at the moment it runs, never on riptide's own pages.
//! What's typed or filled is never logged or shown.

use cef::*;

use rt_config::lua::PageRequest;
use rt_core::engine::Level;

use crate::shell;

/// Finds the login form (the focused field's, or the first with a visible
/// password field) and fills it with the page's own value setter and input
/// events, so frameworks see the change. Returns what it filled, comma
/// separated, or "" when there's no form.
const FILL_LOGIN_JS: &str = r#"(function (username, password, submit) {
  const visible = (e) => e && !e.disabled && !e.readOnly && e.getClientRects().length > 0
    && getComputedStyle(e).visibility !== "hidden";
  const texty = (e) => e && e.tagName === "INPUT" && /^(text|email|tel|)$/.test(e.type) && visible(e);
  const set = (el, value) => {
    el.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };
  const active = document.activeElement;
  const passwords = [...document.querySelectorAll("input[type=password]")].filter(visible);
  let pass = active && active.type === "password" && visible(active) ? active : null;
  const form = (active && active.form) || (pass && pass.form) || (passwords[0] && passwords[0].form) || null;
  if (!pass) pass = passwords.find((p) => !form || p.form === form) || null;
  let user = texty(active) ? active : null;
  if (!user) {
    const inputs = [...(form || document).querySelectorAll("input")];
    const before = pass ? inputs.slice(0, inputs.indexOf(pass)) : inputs;
    const texts = before.filter(texty);
    user = texts.find((e) => /user|login|mail|account|name/i.test(`${e.autocomplete} ${e.name} ${e.id}`))
      || texts[texts.length - 1] || null;
  }
  const filled = [];
  if (username !== null && user) { set(user, username); filled.push("username"); }
  if (password !== null && pass) { set(pass, password); filled.push("password"); }
  const target = (pass || user);
  if (submit && filled.length && target && target.form) {
    target.form.requestSubmit ? target.form.requestSubmit() : target.form.submit();
  }
  return filled.join(",");
})"#;

/// Whether `url` is one a plugin may act on: not riptide's own, and among
/// `pages` when it has a list.
fn allowed(url: &str, pages: Option<&[String]>) -> bool {
    let own = ["riptide:", "chrome:", "devtools:", "chrome-extension:"]
        .iter()
        .any(|s| url.starts_with(s));
    !own && pages.is_none_or(|pages| pages.iter().any(|p| rt_core::url::pattern_matches(p, url)))
}

/// The most of a page's answer handed to Lua.
const MAX_ANSWER: usize = 1 << 20;

/// Hand an `eval` or `selection` answer to Lua callback `callback`, as JSON
/// `{"ok": value}` or `{"error": why}`.
fn answer(plugin: Option<&str>, callback: u32, result: Result<String, String>) {
    let envelope = match result {
        Ok(text) if text.len() > MAX_ANSWER => {
            serde_json::json!({ "error": "the answer is too big" })
        }
        // The page wrote this JSON, so it's parsed rather than trusted.
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => serde_json::json!({ "ok": value }),
            Err(_) => serde_json::json!({ "error": "the page's answer wasn't JSON" }),
        },
        Err(why) => serde_json::json!({ "error": why }),
    };
    let context = crate::lua::current_context();
    let result = rt_config::lua::answered(callback, Some(envelope.to_string()), &context);
    crate::lua::carry_out_for(plugin.unwrap_or("config.lua"), result);
}

/// The element an `rt.page.hint` picked: its URL and text go to Lua.
pub fn hint_chosen(hint: rt_core::hints::LuaHint, url: Option<String>, text: String) {
    let element = serde_json::json!({ "url": url, "text": text });
    answer(
        hint.plugin.as_deref(),
        hint.callback,
        Ok(element.to_string()),
    );
}

/// Whether a frame at `url` may be hinted for a plugin with `pages`.
pub fn may_hint(url: &str, pages: Option<&[String]>) -> bool {
    url.is_empty() || url == "about:blank" || allowed(url, pages)
}

pub fn carry_out(plugin: Option<&str>, pages: Option<&[String]>, request: PageRequest) {
    let who = plugin.map_or_else(|| "config.lua".to_string(), |p| format!("Plugin {p}"));
    // A refused eval or selection answers its callback with why, instead of a message.
    let callback = match &request {
        PageRequest::Eval { callback, .. }
        | PageRequest::Selection { callback }
        | PageRequest::Hint { callback, .. } => Some(*callback),
        _ => None,
    };
    let refuse = |why: String| match callback {
        Some(callback) => answer(plugin, callback, Err(why)),
        None => shell::show_message(Level::Error, format!("{who}: {why}")),
    };
    let Some((url, browser)) = shell::with(|s| {
        let tab = s.tabs.current()?;
        Some((tab.url.clone(), tab.browser()?))
    })
    .flatten() else {
        return refuse("there's no page to act on".into());
    };
    let site = || {
        let host = rt_core::url::host(&url);
        if host.is_empty() {
            url.clone()
        } else {
            host.to_string()
        }
    };
    if !allowed(&url, pages) {
        return refuse(format!("may not act on {}", site()));
    }
    match request {
        PageRequest::Type(_) | PageRequest::Key(_) => {
            // Typing goes to the focused frame, which may be another site's.
            let frame_url = browser
                .focused_frame()
                .map(|f| CefString::from(&f.url()).to_string())
                .unwrap_or_default();
            if !frame_url.is_empty() && !allowed(&frame_url, pages) {
                return refuse(format!(
                    "may not type into {}, a frame in this page",
                    rt_core::url::host(&frame_url)
                ));
            }
            match request {
                PageRequest::Type(text) => crate::actions::insert_text(&text),
                PageRequest::Key(keys) => crate::actions::fake_keys(&keys, false),
                _ => {}
            }
        }
        PageRequest::Css(css) => crate::adblock::inject_css(&browser, &css),
        PageRequest::Hint { selector, callback } => {
            use rt_core::hints::{HintRequest, HintTarget, LuaHint};
            let request = HintRequest {
                group: String::new(),
                target: HintTarget::Lua,
                rapid: false,
                fill: None,
                lua: Some(LuaHint {
                    selector,
                    callback,
                    plugin: plugin.map(str::to_string),
                    pages: pages.map(<[String]>::to_vec),
                }),
            };
            shell::apply(vec![rt_core::engine::Effect::Run {
                command: rt_core::Command::Hint(request),
                count: None,
            }]);
        }
        PageRequest::Eval { code, callback } => {
            // An expression, its value as JSON; it runs in the page's own world.
            let code = format!("JSON.stringify(({code}\n) ?? null)");
            let plugin = plugin.map(str::to_string);
            crate::eval::eval(&browser, &code, move |result| {
                answer(plugin.as_deref(), callback, result)
            });
        }
        PageRequest::Selection { callback } => {
            let plugin = plugin.map(str::to_string);
            crate::eval::eval(
                &browser,
                "JSON.stringify(String(getSelection()))",
                move |result| answer(plugin.as_deref(), callback, result),
            );
        }
        PageRequest::FillLogin {
            host,
            username,
            password,
            submit,
        } => {
            // The tab may have moved on while the password manager ran.
            let current = rt_core::url::host(&url);
            if !current.eq_ignore_ascii_case(&host) {
                return refuse(format!(
                    "didn't fill the login for {host}: the tab is now on {}",
                    site()
                ));
            }
            let args = serde_json::to_string(&(username, password, submit)).unwrap_or_default();
            let code = format!("{FILL_LOGIN_JS}(...{args})");
            crate::eval::eval(&browser, &code, move |result| match result.as_deref() {
                Ok("") => shell::show_message(
                    Level::Error,
                    format!("{who}: no login form on this page to fill"),
                ),
                Ok(filled) => {
                    let what = filled.replace(',', " and ");
                    shell::show_message(Level::Info, format!("{who} filled the {what}"));
                }
                Err(_) => shell::show_message(
                    Level::Error,
                    format!("{who}: couldn't fill the login form"),
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_act_only_on_their_sites_and_never_on_riptides_pages() {
        let star = ["*".to_string()];
        let one = ["*.example.com".to_string()];
        assert!(allowed("https://example.com/login", Some(&star)));
        assert!(allowed("https://a.example.com/", Some(&one)));
        assert!(!allowed("https://notexample.com/", Some(&one)));
        assert!(!allowed("https://example.com/", Some(&[])));
        for own in [
            "riptide://settings/",
            "chrome://extensions/",
            "devtools://x",
        ] {
            assert!(!allowed(own, None), "{own}");
            assert!(!allowed(own, Some(&star)), "{own}");
        }
        assert!(
            allowed("https://example.com/", None),
            "config.lua acts anywhere"
        );
    }
}
