//! The `riptide://` scheme for the browser's own pages. CEF calls the factory and
//! resource handler on its IO thread, so everything here is static or atomic.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cef::*;

use rt_core::ui_message::split_url;

use crate::ui;

pub const SCHEME: &str = "riptide";

/// Inline code only, no network: a UI page can't load anything from outside,
/// even if page-controlled text were ever rendered unescaped.
const CSP: &str =
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:";

/// `theme::page_css` for the current theme, kept up to date by the UI thread
/// because pages are served on the IO thread.
static PAGE_CSS: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

pub fn set_page_css(css: String) {
    if let Ok(mut current) = PAGE_CSS.write()
        && *current != css
    {
        *current = css;
    }
}

/// `html` with the theme's colors added at the end of its `<head>`.
fn themed(html: Arc<[u8]>) -> Arc<[u8]> {
    let css = PAGE_CSS.read().map(|c| c.clone()).unwrap_or_default();
    let text = String::from_utf8_lossy(&html);
    match text.find("</head>") {
        Some(at) if !css.is_empty() => {
            Arc::from(format!("{}<style>{css}</style>{}", &text[..at], &text[at..]).into_bytes())
        }
        _ => html,
    }
}

/// Pages served under `riptide://`, by host and path.
fn page(host: &str, path: &str) -> Option<(Arc<[u8]>, &'static str)> {
    let html = "text/html";
    let embedded = |text: &'static str| Some((Arc::from(text.as_bytes()), html));
    match (host, path) {
        ("ui", "/tabbar.html") => embedded(ui::TABBAR_HTML),
        ("ui", "/statusbar.html") => embedded(ui::STATUSBAR_HTML),
        ("ui", "/completion.html") => embedded(ui::COMPLETION_HTML),
        ("ui", "/float.html") => embedded(ui::FLOAT_HTML),
        ("ui", "/panel.html") => embedded(ui::PANEL_HTML),
        ("ui", "/crashed.html") => embedded(ui::CRASHED_HTML),
        ("ui", "/popup.html") => embedded(ui::POPUP_HTML),
        ("help", "/") => Some((crate::help::page(), html)),
        ("changelog", "/") => Some((crate::help::changelog_page(), html)),
        ("history", "/") => Some((themed(crate::help::history_page()), html)),
        ("crash", "/") => Some((themed(crate::crash::page()), html)),
        ("recover", "/") => Some((themed(crate::recover::page()), html)),
        ("messages", "/") => Some((crate::view::messages_page(), html)),
        ("config-diff", "/") => Some((crate::configcmd::diff_page(), html)),
        ("bookmarks", "/") => Some((crate::storage::bookmarks_page(), html)),
        ("downloads", "/") => Some((themed(crate::downloads::page()), html)),
        ("settings", "/") => Some((themed(crate::settings_page::page()), html)),
        ("process", "/") => Some((crate::spawn::output_page(), html)),
        _ => None,
    }
}

/// Called in every process: Chromium must know the scheme's rules everywhere.
pub fn register(registrar: &mut SchemeRegistrar) {
    // Standard + secure: real URLs and a secure context. Display-isolated:
    // only riptide:// pages may link to, frame or redirect to riptide:// pages.
    use sys::cef_scheme_options_t as opt;
    let options = opt::CEF_SCHEME_OPTION_STANDARD as i32
        | opt::CEF_SCHEME_OPTION_SECURE as i32
        | opt::CEF_SCHEME_OPTION_DISPLAY_ISOLATED as i32;
    registrar.add_custom_scheme(Some(&CefString::from(SCHEME)), options);
}

/// Called once in the browser process, before any window opens.
pub fn install() {
    let mut factory = RtSchemeFactory::new();
    register_scheme_handler_factory(Some(&CefString::from(SCHEME)), None, Some(&mut factory));
    if TEST_PAGES.get().is_some() {
        let mut pages = RtTestPagesFactory::new();
        register_scheme_handler_factory(
            Some(&CefString::from("http")),
            Some(&CefString::from(TEST_HOST)),
            Some(&mut pages),
        );
    }
}

/// `riptide --plugin-test`: the plugin's `test/` folder, which its specs
/// open as ordinary web pages at `http://plugin-test.localhost/<path>`.
pub static TEST_PAGES: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub const TEST_HOST: &str = "plugin-test.localhost";

/// A file under [`TEST_PAGES`] for a request path, never outside it.
fn test_page(path: &str) -> Option<(Arc<[u8]>, &'static str)> {
    let dir = TEST_PAGES.get()?;
    let path = path.split(['?', '#']).next()?.trim_start_matches('/');
    let path = crate::pages::percent_decode(path)?;
    if path
        .split('/')
        .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return None;
    }
    let file = dir.join(&path);
    let body = std::fs::read(&file).ok()?;
    Some((Arc::from(body), crate::pages::mime(&file)))
}

wrap_scheme_handler_factory! {
    struct RtTestPagesFactory {}

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            let path = url.split_once(TEST_HOST).map_or("", |(_, rest)| rest);
            let (body, mime, status) = match test_page(path) {
                Some((body, mime)) => (body, mime, 200),
                None => (Arc::from(&b"Not found"[..]), "text/plain", 404),
            };
            // Test pages are the author's own: no restrictions.
            let csp = "default-src * data: blob: 'unsafe-inline' 'unsafe-eval'".to_string();
            Some(RtResource::new(body, mime, csp, status, Arc::new(AtomicUsize::new(0))))
        }
    }
}

wrap_scheme_handler_factory! {
    struct RtSchemeFactory {}

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            let plugin = rt_core::ui_message::plugin_page(&url).and_then(|name| {
                let (_, path) = split_url(&url)?;
                crate::pages::serve(name, path)
            });
            let found = plugin.or_else(|| {
                split_url(&url)
                    .and_then(|(host, path)| page(host, path))
                    .map(|(body, mime)| (body, mime, CSP.to_string()))
            });
            let (body, mime, csp, status) = match found {
                Some((body, mime, csp)) => (body, mime, csp, 200),
                None => (Arc::from(&b"<!doctype html><title>Not found</title>Not found"[..]), "text/html", CSP.to_string(), 404),
            };
            Some(RtResource::new(body, mime, csp, status, Arc::new(AtomicUsize::new(0))))
        }
    }
}

wrap_resource_handler! {
    struct RtResource {
        body: Arc<[u8]>,
        mime: &'static str,
        // UI pages get [`CSP`]; plugin pages their own (`pages::serve`).
        csp: String,
        status: i32,
        // Shared so clones of the handler agree on how much was read.
        offset: Arc<AtomicUsize>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            if let Some(handle) = handle_request {
                *handle = 1;
            }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            if let Some(response) = response {
                response.set_status(self.status);
                response.set_mime_type(Some(&CefString::from(self.mime)));
                response.set_charset(Some(&CefString::from("utf-8")));
                let header = |name: &str, value: &str| {
                    response.set_header_by_name(Some(&CefString::from(name)), Some(&CefString::from(value)), 1)
                };
                header("Content-Security-Policy", &self.csp);
                header("X-Content-Type-Options", "nosniff");
                header("Cache-Control", "no-store");
            }
            if let Some(length) = response_length {
                *length = self.body.len() as i64;
            }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let start = self.offset.load(Ordering::SeqCst).min(self.body.len());
            let n = (self.body.len() - start).min(bytes_to_read.max(0) as usize);
            if n > 0 && !data_out.is_null() {
                // SAFETY: CEF guarantees `data_out` holds `bytes_to_read` bytes.
                unsafe { std::ptr::copy_nonoverlapping(self.body[start..].as_ptr(), data_out, n) };
                self.offset.store(start + n, Ordering::SeqCst);
            }
            if let Some(read) = bytes_read {
                *read = n as i32;
            }
            (n > 0).into()
        }

        fn skip(
            &self,
            bytes_to_skip: i64,
            bytes_skipped: Option<&mut i64>,
            _callback: Option<&mut ResourceSkipCallback>,
        ) -> ::std::os::raw::c_int {
            let start = self.offset.load(Ordering::SeqCst).min(self.body.len());
            let n = (self.body.len() - start).min(bytes_to_skip.max(0) as usize);
            self.offset.store(start + n, Ordering::SeqCst);
            if let Some(skipped) = bytes_skipped {
                *skipped = n as i64;
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_pages_are_served_with_their_html() {
        for path in [
            "/tabbar.html",
            "/statusbar.html",
            "/completion.html",
            "/float.html",
            "/panel.html",
            "/crashed.html",
        ] {
            let (body, mime) = page("ui", path).unwrap_or_else(|| panic!("{path}"));
            assert_eq!(mime, "text/html");
            assert!(
                body.starts_with(b"<!doctype html>") || body.starts_with(b"<!DOCTYPE html>"),
                "{path}"
            );
        }
    }

    #[test]
    fn anything_else_is_not_found() {
        for (host, path) in [
            ("ui", "/"),
            ("ui", "/../help/"),
            ("ui", "/TABBAR.html"),
            ("help", "/x"),
            ("evil", "/"),
            ("", "/"),
        ] {
            assert!(page(host, path).is_none(), "{host}{path}");
        }
    }
}
