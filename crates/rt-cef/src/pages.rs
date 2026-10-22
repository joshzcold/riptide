//! Plugin pages: HTML a plugin ships in its `pages/` folder, served as
//! `riptide://<name>.plugin/<path>`. Each plugin is its own origin; its
//! pages may only run their own scripts, frame the hosts its `frames`
//! permission names, and talk to its own Lua through `rt.send`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use cef::*;
use rt_config::lua::PagePlace;
use rt_config::plugins::Permissions;
use rt_core::engine::Level;

use crate::shell;

/// What the IO thread needs to serve a loaded plugin's pages.
struct Served {
    pages: PathBuf,
    csp: String,
    /// Its `network` lists a loopback host.
    loopback: bool,
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

/// Hosts on this computer, which `network` may also reach over plain http
/// and WebSockets, on any port: a program the plugin runs, say.
const LOOPBACK: &[&str] = &["localhost", "127.0.0.1"];

/// `network`'s sources for `host`: https, or any scheme and port on loopback.
fn connect_sources(host: &str) -> Vec<String> {
    if LOOPBACK.contains(&host) {
        return ["http", "https", "ws", "wss"]
            .iter()
            .map(|scheme| format!("{scheme}://{host}:*"))
            .collect();
    }
    csp_source(host).into_iter().collect()
}

fn csp(permissions: &Permissions, trusted: bool) -> String {
    let join = |sources: Vec<String>| -> String {
        if sources.is_empty() {
            "'none'".into()
        } else {
            sources.join(" ")
        }
    };
    let frames = if trusted {
        "https:".into()
    } else {
        join(
            permissions
                .frames
                .iter()
                .filter_map(|h| csp_source(h))
                .collect(),
        )
    };
    let loopback = permissions
        .network
        .iter()
        .filter(|h| LOOPBACK.contains(&h.as_str()))
        .flat_map(|h| connect_sources(h));
    let connect = if trusted {
        join(
            std::iter::once("https:".to_string())
                .chain(loopback)
                .collect(),
        )
    } else {
        join(
            permissions
                .network
                .iter()
                .flat_map(|h| connect_sources(h))
                .collect(),
        )
    };
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'none'; \
         form-action 'none'; frame-src {frames}; connect-src {connect}",
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
            let loopback = permissions
                .network
                .iter()
                .any(|h| LOOPBACK.contains(&h.as_str()));
            served.insert(
                name.to_string(),
                Served {
                    pages,
                    csp,
                    loopback,
                },
            );
        }
        _ => {
            served.remove(name);
        }
    }
}

/// Whether `origin` is a plugin page whose `network` permission lists a
/// loopback host, so Chromium needn't ask before it connects there.
pub fn reaches_loopback(origin: &str) -> bool {
    let Some(name) = origin
        .strip_prefix("riptide://")
        .map(|o| o.trim_end_matches('/'))
        .and_then(|host| host.strip_suffix(".plugin"))
    else {
        return false;
    };
    SERVED.read().ok().is_some_and(|s| {
        s.as_ref()
            .and_then(|s| s.get(name))
            .is_some_and(|p| p.loopback)
    })
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

pub(crate) fn mime(path: &Path) -> &'static str {
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

/// `page:resize`: the float showing page `id` of plugin `source`, if one does.
pub fn resize(source: &str, id: u32, width: u32, height: u32) {
    if crate::popup::page_frame().is_some_and(|(url, _)| is_page(&url, source, id)) {
        crate::popup::resize(width as i32, height as i32);
    }
}

/// Float page `id` of plugin `source` was dragged to `x`, `y`: its `on_move`.
pub fn moved(source: &str, id: u32, x: i32, y: i32) {
    let context = crate::lua::current_context();
    let result = rt_config::lua::page_moved(source, id, x, y, &context);
    crate::lua::carry_out_for(source, result);
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
            .chain(crate::popup::page_frame())
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

/// `page:close()`: close its panel or float, or the tabs showing it.
pub fn close(source: &str, id: u32) {
    crate::panel::close(PANEL_IDS | id, true);
    if crate::popup::page_frame().is_some_and(|(url, _)| is_page(&url, source, id)) {
        crate::popup::close();
    }
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

/// `rt.ui.page`: open page `id` of plugin `source` in a tab, a panel or a float.
pub fn open(source: &str, id: u32, path: &str, title: String, place: PagePlace) {
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
    match place {
        PagePlace::Panel { side, size } => {
            let spec = rt_config::lua::PanelSpec {
                title,
                side,
                size,
                page: Some(url),
                ..Default::default()
            };
            crate::panel::show(PANEL_IDS | id, source.to_string(), spec);
        }
        // The bar names the plugin, so the page can't pass for riptide's own.
        PagePlace::Float {
            size,
            over,
            offset,
            all_keys,
        } => {
            let bar = if title.is_empty() {
                format!("Plugin {source}")
            } else {
                format!("{title} · plugin {source}")
            };
            let size = size.map(|(w, h)| (w as i32, h as i32));
            crate::popup::open_with(
                &bar,
                &url,
                crate::popup::Placement {
                    size,
                    over,
                    offset,
                    all_keys,
                    plugin_page: Some((source.to_string(), id)),
                },
            );
        }
        PagePlace::Tab => shell::open(rt_core::command::OpenTarget::Tab, true, Some(url)),
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

        let local = Permissions {
            network: vec!["localhost".into(), "api.example.com".into()],
            frames: vec!["localhost".into()],
            ..Permissions::default()
        };
        register("vault", &dir.join("vault"), &local, false);
        let (_, _, csp) = serve("vault", "/").unwrap();
        assert!(
            csp.contains(
                "connect-src http://localhost:* https://localhost:* ws://localhost:* \
                 wss://localhost:* https://api.example.com"
            ),
            "{csp}"
        );
        assert!(csp.contains("frame-src https://localhost;"), "{csp}");
        assert!(reaches_loopback("riptide://vault.plugin/"));
        assert!(!reaches_loopback("riptide://other.plugin"));
        assert!(!reaches_loopback("https://vault.plugin"));
        register("vault", &dir.join("vault"), &permissions, false);
        assert!(!reaches_loopback("riptide://vault.plugin"));
        unregister_all();
        assert!(serve("vault", "/").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
