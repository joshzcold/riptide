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

/// Pages served under `riptide://`, by host and path.
fn page(host: &str, path: &str) -> Option<(Arc<[u8]>, &'static str)> {
    let html = "text/html";
    let embedded = |text: &'static str| Some((Arc::from(text.as_bytes()), html));
    match (host, path) {
        ("ui", "/tabbar.html") => embedded(ui::TABBAR_HTML),
        ("ui", "/statusbar.html") => embedded(ui::STATUSBAR_HTML),
        ("ui", "/completion.html") => embedded(ui::COMPLETION_HTML),
        ("help", "/") => Some((crate::help::page(), html)),
        ("changelog", "/") => Some((crate::help::changelog_page(), html)),
        ("history", "/") => Some((crate::help::history_page(), html)),
        ("messages", "/") => Some((crate::view::messages_page(), html)),
        ("config-diff", "/") => Some((crate::configcmd::diff_page(), html)),
        ("bookmarks", "/") => Some((crate::storage::bookmarks_page(), html)),
        ("downloads", "/") => Some((crate::downloads::page(), html)),
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
            let found = split_url(&url).and_then(|(host, path)| page(host, path));
            let (body, mime, status) = match found {
                Some((body, mime)) => (body, mime, 200),
                None => (Arc::from(&b"<!doctype html><title>Not found</title>Not found"[..]), "text/html", 404),
            };
            Some(RtResource::new(body, mime, status, Arc::new(AtomicUsize::new(0))))
        }
    }
}

wrap_resource_handler! {
    struct RtResource {
        body: Arc<[u8]>,
        mime: &'static str,
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
                header("Content-Security-Policy", CSP);
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
        for path in ["/tabbar.html", "/statusbar.html", "/completion.html"] {
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
