//! CEF integration for hackers-browser. Translates CEF callbacks into
//! `hb-core` inputs and carries out the effects the engine returns.

mod client;
mod clipboard;
mod eval;
mod hints;
mod renderer;
mod shell;
mod tabs;
mod ui;
mod window;

use std::path::PathBuf;

use cef::*;
use hb_core::url::DEFAULT_START_PAGE;
use hb_core::{Engine, Keymap};

wrap_app! {
    struct HbApp;

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(HbBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(renderer::HbRenderProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct HbBrowserProcessHandler {}

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let mut engine = Engine::new(Keymap::defaults());
            engine.set_clipboard_reader(clipboard::read);
            shell::install(shell::Shell::new(engine));
            window::create(start_url());
        }
    }
}

/// The first non-switch argument, so `hackers-browser example.com` works.
fn start_url() -> String {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|a| hb_core::url::fuzzy_url(&a, hb_core::url::DEFAULT_SEARCH_ENGINE))
        .unwrap_or_else(|| DEFAULT_START_PAGE.to_string())
}

fn data_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "hackers-browser")?;
    let dir = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(dir.join("default")).ok()?;
    Some(dir)
}

fn path_string(path: PathBuf) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

/// Entry point for the browser process and every CEF subprocess. Returns the exit code.
pub fn run() -> i32 {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = args::Args::new();
    let mut app = HbApp::new();
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        return code;
    }

    let mut settings = Settings {
        no_sandbox: (!cfg!(feature = "sandbox")).into(),
        persist_session_cookies: 1,
        log_severity: LogSeverity::WARNING,
        ..Default::default()
    };
    if let Some(dir) = data_dir() {
        settings.root_cache_path = path_string(dir.clone());
        settings.cache_path = path_string(dir.join("default"));
        settings.log_file = path_string(dir.join("cef.log"));
    } else {
        tracing::warn!("no data directory available; browsing data will not persist");
    }

    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        tracing::error!("CEF failed to initialize");
        return 1;
    }
    run_message_loop();
    shutdown();
    0
}
