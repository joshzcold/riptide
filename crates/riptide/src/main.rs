use std::process::ExitCode;

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt, reload};

fn main() -> ExitCode {
    // Chromium names its desktop notifications' app (and so their icon) after
    // this desktop file; it would otherwise say chromium-browser.desktop.
    if std::env::var_os("CHROME_DESKTOP").is_none() {
        // SAFETY: the first thing main does, before any other thread exists.
        unsafe { std::env::set_var("CHROME_DESKTOP", "riptide.desktop") };
    }
    let default = std::env::var("RT_LOG").unwrap_or_else(|_| "info".into());
    let filter = EnvFilter::try_new(&default).unwrap_or_else(|_| EnvFilter::new("info"));
    // Reloadable, so :debug-log-filter can change it while running.
    let (filter, handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();
    rt_cef::set_log_filter_hook(Box::new(move |wanted| {
        let wanted = if wanted == "default" {
            default.as_str()
        } else {
            wanted
        };
        let filter = EnvFilter::try_new(wanted).map_err(|e| e.to_string())?;
        handle.reload(filter).map_err(|e| e.to_string())
    }));
    ExitCode::from(u8::try_from(rt_cef::run()).unwrap_or(1))
}
