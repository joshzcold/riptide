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

pub fn carry_out(plugin: Option<&str>, pages: Option<&[String]>, request: PageRequest) {
    let who = plugin.map_or_else(|| "config.lua".to_string(), |p| format!("Plugin {p}"));
    let refuse = |why: String| shell::show_message(Level::Error, format!("{who}: {why}"));
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
                PageRequest::FillLogin { .. } => {}
            }
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
