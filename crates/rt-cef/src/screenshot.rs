//! `:screenshot`, through the DevTools protocol's `Page.captureScreenshot`,
//! which needs no DevTools window.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use cef::*;
use rt_core::Command;
use rt_core::engine::Level;

use crate::shell;

thread_local! {
    static NEXT_ID: Cell<i32> = const { Cell::new(1) };
    /// The observer for the capture in flight; dropping it unregisters it.
    static PENDING: RefCell<Option<Registration>> = const { RefCell::new(None) };
}

pub fn run_command(command: &Command) -> bool {
    let Command::Screenshot { path, force } = command else {
        return false;
    };
    let path = expand_home(path);
    if path.exists() && !force {
        shell::show_message(
            Level::Error,
            format!(
                "{} exists; use :screenshot --force to replace it",
                path.display()
            ),
        );
        return true;
    }
    let format = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpeg",
        Some("webp") => "webp",
        _ => "png",
    };
    let Some(host) = shell::with(|s| s.current_browser())
        .flatten()
        .and_then(|b| b.host())
    else {
        return true;
    };
    let id = NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    let mut observer = Capture::new(id, path);
    let registration = host.add_dev_tools_message_observer(Some(&mut observer));
    PENDING.with(|p| *p.borrow_mut() = registration);
    let message = serde_json::json!({
        "id": id,
        "method": "Page.captureScreenshot",
        "params": { "format": format },
    })
    .to_string();
    if host.send_dev_tools_message(Some(message.as_bytes())) == 0 {
        PENDING.with(|p| p.borrow_mut().take());
        shell::show_message(Level::Error, "Could not take a screenshot");
    }
    true
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Write the base64 image in a `Page.captureScreenshot` result to `path`.
fn save(result: &[u8], path: &PathBuf) -> Result<(), String> {
    let json: serde_json::Value = serde_json::from_slice(result).map_err(|e| e.to_string())?;
    let data = json["data"].as_str().ok_or("no image in the result")?;
    let binary =
        base64_decode(Some(&CefString::from(data))).ok_or("the image isn't valid base64")?;
    let mut bytes = vec![0; binary.size()];
    binary.data(Some(&mut bytes), 0);
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

wrap_dev_tools_message_observer! {
    struct Capture {
        id: i32,
        path: PathBuf,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            if message_id != self.id {
                return;
            }
            let outcome = match (success != 0, result) {
                (true, Some(result)) => save(result, &self.path),
                (_, result) => Err(result.map(|r| String::from_utf8_lossy(r).into_owned()).unwrap_or_default()),
            };
            match outcome {
                Ok(()) => shell::show_message(Level::Info, format!("Saved {}", self.path.display())),
                Err(e) => shell::show_message(Level::Error, format!("Screenshot failed: {e}")),
            }
            shell::refresh_ui();
            // Unregistering inside the observer's own callback isn't safe; do it next.
            let registration = PENDING.with(|p| p.borrow_mut().take());
            let mut task = Release::new(RefCell::new(registration));
            post_task(ThreadId::UI, Some(&mut task));
        }
    }
}

wrap_task! {
    struct Release {
        registration: RefCell<Option<Registration>>,
    }

    impl Task {
        fn execute(&self) {
            self.registration.borrow_mut().take();
        }
    }
}
