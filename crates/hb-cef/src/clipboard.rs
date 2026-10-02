//! System clipboard. On X11 the clipboard is served by the owning process, so
//! one `arboard::Clipboard` stays alive for the whole session.

use std::cell::RefCell;

use hb_core::engine::Level;

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

/// Copy `text` and report it in the status bar, like qutebrowser.
pub fn yank(text: &str, what: &str) {
    match with_clipboard(|c| c.set_text(text)) {
        Some(Ok(())) => {
            shell::show_message(Level::Info, format!("Yanked {what} to clipboard: {text}"))
        }
        Some(Err(e)) => shell::show_message(Level::Error, format!("Could not set clipboard: {e}")),
        None => shell::show_message(Level::Error, "No clipboard available"),
    }
}
