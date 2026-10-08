//! Plugin pages: HTML a plugin ships in its `pages/` folder, served as
//! `riptide://<name>.plugin/<path>`. Each plugin is its own origin; its
//! pages may only run their own scripts, frame the hosts its `frames`
//! permission names, and talk to its own Lua through `rt.send`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cef::*;
use rt_config::plugins::Permissions;
use rt_core::engine::Level;

use crate::shell;

/// What the IO thread needs to serve a loaded plugin's pages.
struct Served {
    pages: PathBuf,
    csp: String,
}

/// Loaded plugins' pages; written on the UI thread, read on the IO thread.
static SERVED: RwLock<Option<HashMap<String, Served>>> = RwLock::new(None);

/// The most a page may send in one message.
const MAX_MESSAGE: usize = 1 << 20;

/// A host from a permission, safe inside a CSP: letters, digits, `.`, `-`,
/// `:` (a port) and a leading `*.`.
fn csp_source(host: &str) -> Option<String> {
    let bare = host.strip_prefix("*.").unwrap_or(host);
    let ok = !bare.is_empty()
        && bare
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    ok.then(|| format!("https://{host}"))
}

fn csp(permissions: &Permissions, trusted: bool) -> String {
    let list = |hosts: &[String]| -> String {
        if trusted {
            return "https:".into();
        }
        let sources: Vec<String> = hosts.iter().filter_map(|h| csp_source(h)).collect();
        if sources.is_empty() {
            "'none'".into()
        } else {
            sources.join(" ")
        }
    };
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'none'; \
         form-action 'none'; frame-src {}; connect-src {}",
        list(&permissions.frames),
        list(&permissions.network),
    )
}

/// `name` loaded: serve its `pages/` folder, if it has one.
pub fn register(name: &str, dir: &Path, permissions: &Permissions, trusted: bool) {
    let pages = dir.join("pages");
    let Ok(mut served) = SERVED.write() else {
        return;
    };
    let served = served.get_or_insert_with(HashMap::new);
    match pages.canonicalize() {
        Ok(pages) if pages.is_dir() => {
            let csp = csp(permissions, trusted);
            served.insert(name.to_string(), Served { pages, csp });
        }
        _ => {
            served.remove(name);
        }
    }
}

/// The config is loading again: plugins serve pages once they've loaded.
pub fn unregister_all() {
    if let Ok(mut served) = SERVED.write() {
        *served = None;
    }
}

/// `%xx` escapes in a URL path, or `None` if they don't make UTF-8.
pub(crate) fn percent_decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// A file of plugin `name`'s pages at URL `path`, with its type and CSP.
/// Only files inside `pages/` are served, whatever `..`, `%2e` or links say.
pub fn serve(name: &str, path: &str) -> Option<(Arc<[u8]>, &'static str, String)> {
    let (pages, csp) = {
        let served = SERVED.read().ok()?;
        let entry = served.as_ref()?.get(name)?;
        (entry.pages.clone(), entry.csp.clone())
    };
    let path = percent_decode(path)?;
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() || path.ends_with('/') {
        format!("{path}index.html")
    } else {
        path.to_string()
    };
    if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.contains('\\'))
    {
        return None;
    }
    let file = pages.join(&path).canonicalize().ok()?;
    if !file.starts_with(&pages) || !file.is_file() {
        return None;
    }
    let body = std::fs::read(&file).ok()?;
    Some((Arc::from(body), mime(&file), csp))
}

/// The URL `rt.ui.page` opens for page `id` of plugin `name`.
pub fn url(name: &str, path: &str, id: u32) -> String {
    let path = path.trim_start_matches('/');
    format!("riptide://{name}.plugin/{path}?page={id}")
}

/// A message from a page of plugin `plugin`, at `url` (the browser's view of
/// the frame, not the page's say). It goes to the `rt.ui.page` handle the
/// URL names, and only if that handle is the same plugin's.
pub fn message(plugin: &str, url: &str, name: &str, payload: &str) {
    if name.is_empty() || name.len() > 100 || payload.len() > MAX_MESSAGE {
        tracing::warn!(plugin, "dropped an oversized plugin page message");
        return;
    }
    let Some(id) = rt_core::ui_message::plugin_page_id(url) else {
        return;
    };
    let context = crate::lua::current_context();
    let result = rt_config::lua::page_message(plugin, id, name, payload, &context);
    crate::lua::carry_out_for(plugin, result);
}

/// Whether `url` is page `id` of plugin `source`.
fn is_page(url: &str, source: &str, id: u32) -> bool {
    rt_core::ui_message::plugin_page(url) == Some(source)
        && rt_core::ui_message::plugin_page_id(url) == Some(id)
}

/// Panels for pages are numbered apart from `rt.ui.panel`'s.
const PANEL_IDS: u32 = 1 << 31;

/// `page:send(name, data)`: hand `json` to the plugin's open pages for `id`.
pub fn send(source: &str, id: u32, name: &str, json: &str) {
    let mut frames = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| is_page(&t.url, source, id))
            .filter_map(|t| t.browser()?.main_frame())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    frames.extend(
        crate::panel::page_frames()
            .into_iter()
            .filter(|(url, _)| is_page(url, source, id))
            .map(|(_, frame)| frame),
    );
    let quote = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let code = format!(
        "window.dispatchEvent(new CustomEvent('rtmessage', {{ detail: {{ name: {}, data: JSON.parse({}) }} }}))",
        quote(name),
        quote(json),
    );
    for frame in frames {
        shell::exec_js(&frame, &code);
    }
}

/// `page:close()`: close its panel, or the tabs showing it.
pub fn close(source: &str, id: u32) {
    crate::panel::close(PANEL_IDS | id, true);
    let tabs: Vec<usize> = shell::with(|s| {
        s.tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| is_page(&t.url, source, id))
            .map(|(i, _)| i)
            .collect()
    })
    .unwrap_or_default();
    for index in tabs.into_iter().rev() {
        crate::tabs::close(index);
    }
}

/// `rt.ui.page`: open page `id` of plugin `source` in a tab, or in a panel.
pub fn open(source: &str, id: u32, path: &str, title: String, panel: Option<(String, u32)>) {
    if source.is_empty() {
        return shell::show_message(Level::Error, "config.lua: only plugins have pages");
    }
    let served = SERVED
        .read()
        .ok()
        .is_some_and(|s| s.as_ref().is_some_and(|s| s.contains_key(source)));
    if !served {
        return shell::show_message(
            Level::Error,
            format!("Plugin {source}: it has no pages folder"),
        );
    }
    let url = url(source, path, id);
    match panel {
        Some((side, size)) => {
            let spec = rt_config::lua::PanelSpec {
                title,
                side,
                size,
                page: Some(url),
                ..Default::default()
            };
            crate::panel::show(PANEL_IDS | id, source.to_string(), spec);
        }
        None => shell::open(rt_core::command::OpenTarget::Tab, true, Some(url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_served_from_their_folder_only() {
        let dir = std::env::temp_dir().join(format!("rt-pages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("vault/pages/js")).unwrap();
        std::fs::write(dir.join("vault/pages/index.html"), "<!doctype html>hi").unwrap();
        std::fs::write(dir.join("vault/pages/js/app.js"), "1").unwrap();
        std::fs::write(dir.join("vault/secret.lua"), "x").unwrap();
        let permissions = Permissions {
            frames: vec!["vault.example.com".into(), "bad host; script-src *".into()],
            ..Permissions::default()
        };
        register("vault", &dir.join("vault"), &permissions, false);

        let (body, mime, csp) = serve("vault", "/").unwrap();
        assert_eq!((&*body, mime), (&b"<!doctype html>hi"[..], "text/html"));
        assert!(
            csp.contains("frame-src https://vault.example.com;"),
            "{csp}"
        );
        assert!(csp.contains("script-src 'self';"), "{csp}");
        assert!(csp.contains("connect-src 'none'"), "{csp}");
        assert_eq!(serve("vault", "/js/app.js").unwrap().1, "text/javascript");
        for bad in [
            "/../secret.lua",
            "/%2e%2e/secret.lua",
            "/js/../../secret.lua",
            "//etc/passwd",
            "/missing.html",
        ] {
            assert!(serve("vault", bad).is_none(), "{bad}");
        }
        assert!(serve("other", "/").is_none());
        unregister_all();
        assert!(serve("vault", "/").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
