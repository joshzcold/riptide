//! Commands that act on the page or the browser for scripts and bindings:
//! `:fake-key`, `:insert-text`, `:click-element`, `:scroll-to-anchor`,
//! `:cmd-later` and `:window-only`.

use cef::*;
use rt_core::Command;
use rt_core::Key;
use rt_core::command::ElementFilter;
use rt_core::engine::Level;

use crate::shell;

pub fn run_command(command: &Command) -> bool {
    match command {
        Command::Later { ms, command } => {
            let mut task = RunLater::new(command.clone());
            post_delayed_task(
                ThreadId::UI,
                Some(&mut task),
                i64::try_from(*ms).unwrap_or(i64::MAX),
            );
        }
        Command::FakeKey { keys, global } => fake_keys(keys, *global),
        Command::InsertText { text } => {
            let code = format!(
                "document.execCommand('insertText', false, {})",
                serde_json::to_string(text).unwrap_or_default()
            );
            focused_js(&code);
        }
        Command::ClickElement { filter, value } => click(*filter, value),
        Command::ScrollToAnchor { name } => {
            let name = serde_json::to_string(name).unwrap_or_default();
            let code = format!(
                "(() => {{ const e = document.getElementById({name}) || document.getElementsByName({name})[0]; \
                 if (e) e.scrollIntoView(); return !!e; }})()"
            );
            report_missing(&code, "No element with that id or name");
        }
        Command::WindowOnly => {
            let others: Vec<Window> = shell::with(|s| {
                let active = s.active;
                s.windows
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != active)
                    .filter_map(|(_, w)| w.window.clone())
                    .collect()
            })
            .unwrap_or_default();
            for window in others {
                window.close();
            }
        }
        _ => return false,
    }
    true
}

fn fake_keys(keys: &str, global: bool) {
    let keys = match Key::parse_sequence(keys) {
        Ok(keys) => keys,
        Err(e) => return shell::show_message(Level::Error, e.to_string()),
    };
    for key in keys {
        if global {
            if let Some(outcome) = shell::with(|s| s.engine.handle_key(key)) {
                shell::apply(outcome.effects);
            }
        } else {
            crate::client::send_to_page(key);
        }
    }
}

fn click(filter: ElementFilter, value: &str) {
    let value = serde_json::to_string(value).unwrap_or_default();
    let find = match filter {
        ElementFilter::Id => format!("document.getElementById({value})"),
        ElementFilter::Css => format!(
            "(() => {{ try {{ return document.querySelector({value}); }} catch (_) {{ return null; }} }})()"
        ),
        ElementFilter::Focused => "document.activeElement".to_string(),
    };
    let code = format!(
        "(() => {{ const e = {find}; if (e && e !== document.body) e.click(); return !!e && e !== document.body; }})()"
    );
    report_missing(&code, "No element matches");
}

/// Runs `code` in the main frame and shows `missing` if it returns false.
fn report_missing(code: &str, missing: &'static str) {
    let Some(browser) = shell::with(|s| s.current_browser()).flatten() else {
        return;
    };
    crate::eval::eval(&browser, code, move |result| {
        if !matches!(result.as_deref(), Ok("true")) {
            shell::show_message(Level::Error, missing);
            shell::refresh_ui();
        }
    });
}

/// Runs `code` in the frame with focus, so it reaches fields in iframes.
fn focused_js(code: &str) {
    let frame = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.focused_frame().or_else(|| b.main_frame()));
    if let Some(frame) = frame {
        shell::exec_js(&frame, code);
    }
}

wrap_task! {
    struct RunLater {
        command: String,
    }

    impl Task {
        fn execute(&self) {
            if let Some(effects) = shell::with(|s| s.engine.execute_str(&self.command, None)) {
                shell::apply(effects);
            }
        }
    }
}
