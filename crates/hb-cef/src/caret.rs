//! Caret mode: moves a text cursor and selection with the keyboard through
//! `js/caret.js`, and yanks the selection.

use hb_core::Command;
use hb_core::command::CaretMove;
use hb_core::engine::Level;

use crate::{clipboard, eval, shell};

const CARET_JS: &str = include_str!("../js/caret.js");

fn call(code: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    eval::eval(
        &browser,
        &format!("{CARET_JS}\nwindow.__hbCaret.{code}"),
        done,
    );
}

/// The mode changed to or from caret mode.
pub fn mode_changed(entered: bool) {
    if !entered {
        return call("leave()", |_| {});
    }
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
            let kind = if *movement == CaretMove::NextWord {
                "next-word"
            } else {
                ""
            };
            let count = count.unwrap_or(1).clamp(1, 10_000);
            call(
                &format!("move('{direction}', '{granularity}', {count}, '{kind}')"),
                |_| {},
            );
        }
        Command::SelectionToggle { line } => call(&format!("toggle({line})"), |_| {}),
        Command::SelectionReverse => call("reverse()", |_| {}),
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
