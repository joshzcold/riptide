//! Crash reports: a panic in the browser process writes one to
//! `<data>/crashes/`, and the next start says where it is.

use std::path::{Path, PathBuf};

use rt_core::engine::Level;
use rt_storage::crash_reports::{CrashReports, Report};

use crate::shell;

fn reports(data_dir: &Path) -> CrashReports {
    CrashReports::new(&data_dir.join("crashes"))
}

/// Write a report for any panic, then carry on as before (the default hook
/// prints the panic). Called once, in the browser process.
pub fn install_panic_hook(data_dir: &Path) {
    let reports = reports(data_dir);
    let previous = std::panic::take_hook();
    thread_local! {
        static REPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    std::panic::set_hook(Box::new(move |info| {
        // A panic reaching CEF's C callers panics again ("cannot unwind"); the first one is the cause.
        if REPORTED.with(|r| r.replace(true)) {
            return previous(info);
        }
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no message)");
        let location = info.location().map(ToString::to_string);
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let version = crate::help::version_line();
        let text = Report {
            version: &version,
            thread: thread.name().unwrap_or("unnamed"),
            message,
            location: location.as_deref(),
            backtrace: &backtrace,
        }
        .text();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        match reports.write(now, &text) {
            Ok(path) => eprintln!("riptide: crash report written to {}", path.display()),
            Err(e) => eprintln!("riptide: can't write a crash report: {e}"),
        }
        previous(info);
    }));
}

thread_local! {
    static UNMENTIONED: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// At startup: find the report the last run left, if it hasn't been mentioned yet.
pub fn find_last_report() {
    let Some(data_dir) = shell::with(|s| s.paths.data_dir.clone()) else {
        return;
    };
    let path = reports(&data_dir).take_unseen();
    UNMENTIONED.with(|u| *u.borrow_mut() = path);
}

/// Once a page has loaded, say where that report is. Earlier, the page
/// starting to load would clear the message.
pub fn mention_last_report() {
    if let Some(path) = UNMENTIONED.with(|u| u.borrow_mut().take()) {
        shell::show_message(
            Level::Error,
            format!(
                "riptide crashed last time. The report is in {}",
                path.display()
            ),
        );
    }
}
