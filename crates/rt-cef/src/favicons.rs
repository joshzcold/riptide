//! Site icons for the tab bar, downloaded by Chromium and kept as PNG data URLs.

use cef::*;

use crate::shell;

/// Icons are drawn at 16px; 32px covers high-DPI screens.
const MAX_SIZE: u32 = 32;
/// Reject anything unexpectedly large rather than embedding it in the UI.
const MAX_PNG_BYTES: usize = 64 * 1024;

/// Read a CEF string list in place. (`CefStringList::clone` copies the opaque
/// C struct rather than the pointer, so iterating a clone yields nothing.)
pub fn read_list(list: &mut CefStringList) -> Vec<String> {
    let raw: *mut sys::_cef_string_list_t = list.into();
    if raw.is_null() {
        return Vec::new();
    }
    // SAFETY: `raw` is the live list CEF passed to the callback.
    let count = unsafe { sys::cef_string_list_size(raw) };
    (0..count)
        // SAFETY: `i` is below the list's size, and each value is cleared after it's copied.
        .filter_map(|i| unsafe {
            let mut value = std::mem::zeroed();
            (sys::cef_string_list_value(raw, i, &mut value) > 0).then(|| {
                let text = CefString::from(std::ptr::from_ref(&value)).to_string();
                sys::cef_string_utf16_clear(&mut value);
                text
            })
        })
        .collect()
}

/// The first icon we can fetch: http(s) or an inline image, never another scheme.
fn pick_icon(urls: Vec<String>) -> Option<String> {
    urls.into_iter()
        .find(|u| u.starts_with("http") || u.starts_with("data:image/"))
}

/// A download counts when it succeeded, or had no HTTP status (data: URLs).
fn usable_status(code: i32) -> bool {
    (200..300).contains(&code) || code == 0
}

/// A page announced its icons: fetch the first one, or clear it if none.
pub fn changed(browser: &Browser, urls: Vec<String>) {
    let id = browser.identifier();
    let Some(url) = pick_icon(urls) else {
        set(id, None);
        return;
    };
    if let Some(host) = browser.host() {
        let mut callback = RtFaviconCallback::new(id);
        host.download_image(
            Some(&CefString::from(url.as_str())),
            1,
            MAX_SIZE,
            0,
            Some(&mut callback),
        );
    }
}

fn set(browser: i32, favicon: Option<String>) {
    shell::with(|s| {
        if let Some(tab) = s
            .tabs
            .iter_mut()
            .find(|t| t.browser().is_some_and(|b| b.identifier() == browser))
        {
            tab.favicon = favicon;
        }
    });
    shell::refresh_ui();
}

fn png_data_url(image: &Image) -> Option<String> {
    let (mut width, mut height) = (0, 0);
    let png = image.as_png(1.0, 1, Some(&mut width), Some(&mut height))?;
    let size = png.size();
    if size == 0 || size > MAX_PNG_BYTES {
        return None;
    }
    let mut bytes = vec![0u8; size];
    png.data(Some(&mut bytes), 0);
    let encoded = CefString::from(&base64_encode(Some(&bytes))).to_string();
    Some(format!("data:image/png;base64,{encoded}"))
}

wrap_download_image_callback! {
    struct RtFaviconCallback {
        browser: i32,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            http_status_code: ::std::os::raw::c_int,
            image: Option<&mut Image>,
        ) {
            let icon = image.filter(|_| usable_status(http_status_code));
            set(self.browser, icon.and_then(|i| png_data_url(i)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_fetchable_icon() {
        let urls = |list: &[&str]| list.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            pick_icon(urls(&[
                "chrome://x/icon.png",
                "https://a.example/favicon.ico",
                "https://a.example/b.png"
            ])),
            Some("https://a.example/favicon.ico".into())
        );
        assert_eq!(
            pick_icon(urls(&["data:image/png;base64,AAAA"])),
            Some("data:image/png;base64,AAAA".into())
        );
        assert_eq!(
            pick_icon(urls(&["data:text/html,x", "file:///icon.png"])),
            None
        );
        assert_eq!(pick_icon(Vec::new()), None);
    }

    #[test]
    fn only_successful_downloads_are_used() {
        assert!(usable_status(200));
        assert!(usable_status(0));
        assert!(!usable_status(404));
        assert!(!usable_status(301));
    }
}
