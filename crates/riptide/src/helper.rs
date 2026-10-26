//! The executable in riptide.app's helper apps: CEF's subprocesses without
//! the browser's code, so the five copies stay small.
use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let filter = std::env::var("RT_LOG").unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();
    ExitCode::from(u8::try_from(rt_cef::run_helper()).unwrap_or(1))
}
