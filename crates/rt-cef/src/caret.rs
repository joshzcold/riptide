//! Caret mode: moves a text cursor and selection with the keyboard through
//! `js/caret.js`, and yanks the selection.

use cef::*;
use rt_core::Command;
use rt_core::engine::Level;

use crate::{clipboard, eval, shell};

const CARET_JS: &str = include_str!("../js/caret.js");

thread_local! {
    /// The frame caret mode runs in, by id, until it's left: the one it was
    /// started in. None for the main frame.
    static FRAME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// The frame `rt.enter_mode("caret", { frame })` asked for, for the next start.
    static NEXT_FRAME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Start caret mode in frame `id` next time, rather than the focused one.
pub fn use_frame(id: String) {
    NEXT_FRAME.with(|f| *f.borrow_mut() = Some(id));
}

fn call(code: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    let frame = FRAME
        .with(|f| f.borrow().clone())
        .and_then(|id| browser.frame_by_identifier(Some(&CefString::from(id.as_str()))))
        .filter(|f| f.is_valid() != 0)
        .or_else(|| browser.main_frame());
    let Some(frame) = frame else {
        return;
    };
    eval::eval_frame(
        &frame,
        &format!("{CARET_JS}\nwindow.__rtCaret.{code}"),
        done,
    );
}

/// The frame caret mode starts in: the one asked for, else the focused one.
fn starting_frame() -> Option<String> {
    let asked = NEXT_FRAME.with(|f| f.borrow_mut().take());
    asked.or_else(|| {
        let browser = shell::with(|s| s.current_browser()).flatten()?;
        let focused = browser.focused_frame().filter(|f| f.is_main() == 0)?;
        Some(CefString::from(&focused.identifier()).to_string())
    })
}

/// The mode changed to or from caret mode.
pub fn mode_changed(entered: bool) {
    if !entered {
        call("leave()", |_| {});
        return FRAME.with(|f| *f.borrow_mut() = None);
    }
    let frame = starting_frame();
    FRAME.with(|f| *f.borrow_mut() = frame);
    call("enter()", |result| {
        if result.as_deref() != Ok("true") {
            shell::show_message(Level::Error, "No text on screen for the caret");
            let effects = shell::with(|s| s.engine.execute_str("mode-leave", None));
            shell::apply(effects.unwrap_or_default());
        }
    });
}

pub fn run_command(command: &Command, count: Option<u32>) -> bool {
    match command {
        Command::CaretMove(movement) => {
            let (direction, granularity) = movement.js();
            let kind = movement.kind();
            let count = count.unwrap_or(1).clamp(1, 10_000);
            call(
                &format!("move('{direction}', '{granularity}', {count}, '{kind}')"),
                |_| {},
            );
        }
        Command::SelectionToggle { line } => call(&format!("toggle({line})"), |_| {}),
        Command::SelectionReverse => call("reverse()", |_| {}),
        Command::SelectionDrop => call("drop()", |_| {}),
        _ => return false,
    }
    true
}

/// `:yank selection`, to the primary selection with `primary`.
pub fn yank(primary: bool) {
    call("text()", move |result| {
        let text = result
            .ok()
            .and_then(|json| serde_json::from_str::<String>(&json).ok())
            .unwrap_or_default();
        if text.is_empty() {
            shell::show_message(Level::Error, "Nothing is selected");
            shell::refresh_ui();
        } else {
            clipboard::yank_to(&text, "selection", primary);
            shell::refresh_ui();
        }
    });
}
