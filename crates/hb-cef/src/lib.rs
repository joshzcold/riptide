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

use std::path::Path;

use cef::*;
use hb_config::{Cli, Paths};
use hb_core::engine::Level;
use hb_core::{Engine, Keymap};

/// What the browser process needs once CEF is up.
#[derive(Clone)]
struct Startup {
    paths: Paths,
    urls: Vec<String>,
}

wrap_app! {
    struct HbApp {
        startup: Option<Startup>,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(HbBrowserProcessHandler::new(self.startup.clone()))
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(renderer::HbRenderProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct HbBrowserProcessHandler {
        startup: Option<Startup>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let Some(startup) = self.startup.clone() else { return };
            let mut engine = Engine::new(Keymap::defaults());
            engine.set_clipboard_reader(clipboard::read);
            shell::install(shell::Shell::new(engine, startup.paths));
            let errors = shell::load_config();
            window::create(startup.urls);
            report_config_errors(&errors);
        }
    }
}

pub(crate) fn report_config_errors(errors: &[String]) {
    match errors {
        [] => {}
        [only] => shell::show_message(Level::Error, format!("Config: {only}")),
        [first, rest @ ..] => shell::show_message(
            Level::Error,
            format!("Config: {first} (and {} more; see the log)", rest.len()),
        ),
    }
}

fn path_string(path: &Path) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

/// Entry point for the browser process and every CEF subprocess. Returns the exit code.
pub fn run() -> i32 {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = args::Args::new();
    let mut app = HbApp::new(None);
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        return code;
    }

    let cli = match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("hackers-browser: {e}\n\n{}", hb_config::cli::USAGE);
            return 2;
        }
    };
    if cli.help {
        println!("{}", hb_config::cli::USAGE);
        return 0;
    }
    if cli.lua_types {
        print!("{}", hb_config::lua_types::generate());
        return 0;
    }
    if cli.version {
        println!("hackers-browser {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let paths = match Paths::resolve(cli.basedir.as_deref()) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("hackers-browser: {e}");
            return 2;
        }
    };
    if cli.print_paths {
        println!("config: {}", paths.config_dir.display());
        println!("data:   {}", paths.data_dir.display());
        return 0;
    }
    let profile = paths.data_dir.join("default");
    if let Err(e) = std::fs::create_dir_all(&profile) {
        eprintln!("hackers-browser: cannot create {}: {e}", profile.display());
        return 1;
    }

    let settings = Settings {
        no_sandbox: (!cfg!(feature = "sandbox")).into(),
        persist_session_cookies: 1,
        log_severity: LogSeverity::WARNING,
        root_cache_path: path_string(&paths.data_dir),
        cache_path: path_string(&profile),
        log_file: path_string(&paths.data_dir.join("cef.log")),
        ..Default::default()
    };
    let mut app = HbApp::new(Some(Startup {
        paths,
        urls: cli.urls,
    }));
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
