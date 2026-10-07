//! Crash reports: a panic in the browser process writes one to
//! `<data>/crashes/`, and the next start says where it is.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use rt_core::engine::Level;
use rt_storage::crash_reports::{CrashReports, Report, dumps};

use crate::shell;

const TEMPLATE: &str = include_str!("../ui/crash.html");
const ISSUES: &str = "https://github.com/joshzcold/riptide/issues/new";

/// The reports' directory, for the page, which is served on CEF's IO thread.
static DIR: OnceLock<PathBuf> = OnceLock::new();
/// `crash_report.email`, read on the UI thread when `:crash-report` runs.
static EMAIL: Mutex<String> = Mutex::new(String::new());

pub fn set_email(email: String) {
    if let Ok(mut current) = EMAIL.lock() {
        *current = email;
    }
}

/// Turn on Chromium's crash reporter (Crashpad), which reads
/// `crash_reporter.cfg` next to the executable. Release packages ship the
/// file; this writes it for builds run from where they were built. A
/// read-only install directory is fine: then it's the package's own.
pub fn write_reporter_config() {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let path = dir.join("crash_reporter.cfg");
    let config = dumps::reporter_config(env!("CARGO_PKG_VERSION"));
    if std::fs::read_to_string(&path).ok().as_deref() != Some(config.as_str())
        && let Err(e) = std::fs::write(&path, config)
    {
        tracing::debug!("can't write {}: {e}", path.display());
    }
}

/// `riptide://crash/`: the newest reports, read from disk each time.
pub fn page() -> Arc<[u8]> {
    let dir = DIR.get().cloned().unwrap_or_default();
    let store = CrashReports::new(&dir);
    let reports: Vec<serde_json::Value> = store
        .list()
        .into_iter()
        .rev()
        .filter_map(|name| {
            let text = std::fs::read_to_string(dir.join(&name)).ok()?;
            Some(serde_json::json!({ "name": name, "text": text }))
        })
        .collect();
    let email = EMAIL.lock().map(|e| e.clone()).unwrap_or_default();
    let dumps: Vec<serde_json::Value> = dir
        .parent()
        .map(dumps::list)
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            let when = rt_storage::recovery::utc_stamp(d.modified);
            serde_json::json!({ "path": d.path.display().to_string(), "when": when, "size": d.size })
        })
        .collect();
    let data = serde_json::json!({
        "dir": dir.display().to_string(),
        "reports": reports,
        "dumps": dumps,
        "email": email,
        "issues": ISSUES,
    });
    // `</` would end the inline <script> early.
    let json = data.to_string().replace("</", "<\\/");
    Arc::from(TEMPLATE.replace("/*RT_DATA*/null", &json).into_bytes())
}

fn reports(data_dir: &Path) -> CrashReports {
    CrashReports::new(&data_dir.join("crashes"))
}

/// Write a report for any panic, then carry on as before (the default hook
/// prints the panic). Called once, in the browser process.
pub fn install_panic_hook(data_dir: &Path) {
    let _ = DIR.set(data_dir.join("crashes"));
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

/// At startup: say where the report the last run left is, if that hasn't been said yet.
pub fn mention_last_report() {
    let Some(data_dir) = shell::with(|s| s.paths.data_dir.clone()) else {
        return;
    };
    dumps::prune(&data_dir, dumps::KEEP_DUMPS);
    // A Rust panic aborts, so Crashpad leaves a dump of it too; the report
    // says more, and the page lists both.
    let new_dumps = dumps::take_new(&data_dir);
    if let Some(path) = reports(&data_dir).take_unseen() {
        shell::show_message_after_load(
            Level::Error,
            format!(
                "riptide crashed last time. :crash-report shows the report ({})",
                path.display()
            ),
        );
    } else if new_dumps > 0 {
        let what = if new_dumps == 1 {
            "a crash dump".to_string()
        } else {
            format!("{new_dumps} crash dumps")
        };
        shell::show_message_after_load(
            Level::Error,
            format!(
                "Chromium crashed since the last start and left {what}; :crash-report lists them"
            ),
        );
    }
}
