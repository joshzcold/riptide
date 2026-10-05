//! System clipboard. On X11 the clipboard is served by the owning process, so
//! one `arboard::Clipboard` stays alive for the whole session.

use std::cell::RefCell;

use rt_core::engine::Level;

use crate::shell;

thread_local! {
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

fn with_clipboard<R>(f: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
    CLIPBOARD.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => *c = Some(clipboard),
                Err(e) => {
                    tracing::warn!(%e, "clipboard unavailable");
                    return None;
                }
            }
        }
        c.as_mut().map(f)
    })
}

pub fn read() -> Option<String> {
    with_clipboard(|c| c.get_text().ok()).flatten()
}

/// The primary selection (what's selected, pasted with the middle button) on
/// X11; other platforms have only the clipboard.
pub fn read_primary() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        use arboard::{GetExtLinux, LinuxClipboardKind};
        with_clipboard(|c| c.get().clipboard(LinuxClipboardKind::Primary).text().ok()).flatten()
    }
    #[cfg(not(target_os = "linux"))]
    {
        read()
    }
}

/// Copy `text` and report it in the status bar, like qutebrowser.
pub fn yank(text: &str, what: &str) {
    yank_to(text, what, false);
}

/// `yank`, or with `primary` into the primary selection.
pub fn yank_to(text: &str, what: &str, primary: bool) {
    let result = with_clipboard(|c| {
        #[cfg(target_os = "linux")]
        if primary {
            use arboard::{LinuxClipboardKind, SetExtLinux};
            return c
                .set()
                .clipboard(LinuxClipboardKind::Primary)
                .text(text.to_string());
        }
        c.set_text(text)
    });
    let target = if primary && cfg!(target_os = "linux") {
        "primary selection"
    } else {
        "clipboard"
    };
    match result {
        Some(Ok(())) => {
            shell::show_message(Level::Info, format!("Yanked {what} to {target}: {text}"))
        }
        Some(Err(e)) => {
            shell::show_message(Level::Error, format!("Could not set the {target}: {e}"))
        }
        None => shell::show_message(Level::Error, "No clipboard available"),
    }
}
