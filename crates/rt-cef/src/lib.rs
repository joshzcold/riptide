//! CEF integration for riptide. Translates CEF callbacks into
//! `rt-core` inputs and carries out the effects the engine returns.

mod actions;
mod adblock;
mod caret;
mod client;
mod clipboard;
mod configcmd;
mod content;
mod crash;
mod dialogs;
mod downloads;
mod eval;
mod favicons;
mod fetch;
mod fileselect;
mod gm_api;
mod greasemonkey;
mod help;
mod hints;
mod history;
mod lua;
mod marks;
mod navigate;
mod notifications;
mod page;
mod permissions;
mod plugins;
mod privacy;
mod prompts;
mod recover;
mod remote;
mod renderer;
mod scheme;
mod screenshot;
mod search;
mod settings_page;
mod shell;
#[cfg(unix)]
mod signals;
mod spawn;
mod spell;
mod statusbar;
mod storage;
mod tabs;
mod test_control;
mod tls;
mod ui;
mod userstyle;
mod view;
mod window;

use std::path::Path;

use cef::*;
use rt_config::{Cli, Paths};
use rt_core::engine::Level;
use rt_core::{Engine, Keymap};

/// Set by `:restart`: start a new browser once this one has shut down.
pub(crate) static RESTART: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The session `:restart` saves and the new browser loads.
pub(crate) const RESTART_SESSION: &str = "_restart";

/// What the browser process needs once CEF is up.
#[derive(Clone)]
struct Startup {
    paths: Paths,
    urls: Vec<String>,
    commands: Vec<String>,
    /// The config, read before CEF starts so startup-only settings can
    /// become Chromium switches; taken once CEF is up.
    config: std::sync::Arc<std::sync::Mutex<Option<rt_config::Loaded>>>,
    /// Switches from startup-only settings such as `content.cache.size`,
    /// `content.canvas_reading` and `content.webgl`.
    switches: Vec<(&'static str, Option<String>)>,
    /// `content.widevine` is on but the CDM isn't downloaded yet.
    fetch_widevine: bool,
}

wrap_app! {
    struct RtApp {
        startup: Option<Startup>,
    }

    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(registrar) = registrar {
                scheme::register(registrar);
            }
        }

        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // CEF runs Chrome underneath even for Alloy-style windows, and
            // Chrome's own login prompt would swallow HTTP auth (cef#3603).
            let browser_process = process_type.is_none_or(|t| t.to_string().is_empty());
            if browser_process && let Some(command_line) = command_line {
                command_line.append_switch(Some(&CefString::from("disable-chrome-login-prompt")));
                privacy::append_switches(command_line);
                for (name, value) in self.startup.as_ref().map_or(&[][..], |s| &s.switches) {
                    let name = CefString::from(*name);
                    match value {
                        // Keep features the user passed on the command line.
                        Some(value) if matches!(name.to_string().as_str(), "enable-features" | "disable-features") => {
                            append_to_list_switch(command_line, &name.to_string(), value)
                        }
                        Some(value) => command_line.append_switch_with_value(Some(&name), Some(&CefString::from(value.as_str()))),
                        None => command_line.append_switch(Some(&name)),
                    }
                }
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(RtBrowserProcessHandler::new(self.startup.clone()))
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(renderer::RtRenderProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct RtBrowserProcessHandler {
        startup: Option<Startup>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let Some(startup) = self.startup.clone() else { return };
            let mut engine = Engine::new(Keymap::defaults());
            engine.set_clipboard_reader(clipboard::read);
            engine.set_primary_reader(clipboard::read_primary);
            engine.set_completion_source(storage::complete);
            let mut errors = storage::open(&startup.paths);
            shell::install(shell::Shell::new(engine, startup.paths));
            scheme::install();
            let loaded = startup.config.lock().ok().and_then(|mut c| c.take());
            errors.extend(match loaded {
                Some(loaded) => shell::apply_config(loaded),
                None => shell::load_config(),
            });
            errors.extend(greasemonkey::load().1);
            if let Some((data_dir, lists)) = shell::with(|s| {
                (s.paths.data_dir.clone(), s.engine.settings().list("content.blocking.adblock.lists").to_vec())
            }) {
                adblock::load(data_dir, lists);
            }
            let data_dir = shell::with(|s| s.paths.data_dir.clone());
            let mut commands = startup.commands;
            if data_dir.as_deref().is_some_and(help::note_upgrade) {
                commands.push("open -b riptide://changelog/".to_string());
            }
            window::create(startup.urls, commands, false);
            report_config_errors(&errors);
            if let Some(data_dir) = data_dir
                && startup.fetch_widevine
            {
                privacy::watch_widevine_download(data_dir);
            }
            remote::listen();
            statusbar::start();
            userstyle::start();
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

/// The settings from config read before the engine exists.
fn startup_settings(loaded: &rt_config::Loaded) -> rt_core::settings::Settings {
    let mut engine = Engine::new(Keymap::defaults());
    for op in &loaded.ops {
        let _ = engine.apply_config(op);
    }
    engine.settings().clone()
}

type LogFilterHook = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

static LOG_FILTER: std::sync::OnceLock<LogFilterHook> = std::sync::OnceLock::new();

/// Let `:debug-log-filter` change the log filter; `main` owns the logger.
pub fn set_log_filter_hook(hook: LogFilterHook) {
    let _ = LOG_FILTER.set(hook);
}

pub(crate) fn set_log_filter(filter: &str) -> Result<(), String> {
    match LOG_FILTER.get() {
        Some(hook) => hook(filter),
        None => Err("the log filter can't be changed in this build".into()),
    }
}

/// A setting's value from config read before the engine exists.
fn startup_bool(loaded: &rt_config::Loaded, name: &str) -> bool {
    startup_settings(loaded).bool(name)
}

/// Add `value` to a comma-separated switch, keeping what the user passed.
pub(crate) fn append_to_list_switch(command_line: &mut CommandLine, name: &str, value: &str) {
    let name = CefString::from(name);
    let given = CefStringUtf16::from(&command_line.switch_value(Some(&name))).to_string();
    let joined = if given.is_empty() {
        value.to_string()
    } else {
        format!("{given},{value}")
    };
    command_line.append_switch_with_value(Some(&name), Some(&CefString::from(joined.as_str())));
}

fn path_string(path: &Path) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

/// Answer `--help`, `--version`, `--paths` and `--lua-types` without CEF.
/// Returns `Err(exit code)` when the process should stop here.
fn early_cli() -> Result<(Cli, Paths), i32> {
    let cli = match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("riptide: {e}\n\n{}", rt_config::cli::USAGE);
            return Err(2);
        }
    };
    if cli.help {
        println!("{}", rt_config::cli::USAGE);
        return Err(0);
    }
    if cli.lua_types {
        print!("{}", rt_config::lua_types::generate());
        return Err(0);
    }
    if cli.version {
        println!("{}", help::version_line());
        return Err(0);
    }
    let paths = Paths::resolve(cli.basedir.as_deref()).map_err(|e| {
        eprintln!("riptide: {e}");
        2
    })?;
    if cli.print_paths {
        println!("config: {}", paths.config_dir.display());
        println!("data:   {}", paths.data_dir.display());
        return Err(0);
    }
    Ok((cli, paths))
}

/// Entry point for the browser process and every CEF subprocess. Returns the exit code.
pub fn run() -> i32 {
    // CEF subprocesses carry `--type=…`; everything else is the browser process.
    let subprocess = std::env::args().any(|a| a.starts_with("--type="));
    let early = if subprocess { None } else { Some(early_cli()) };
    if let Some(Err(code)) = early {
        return code;
    }
    // Before CEF starts, which reads it in every process.
    if !subprocess {
        crash::write_reporter_config();
    }

    // On macOS the CEF framework is loaded at runtime from the app bundle.
    #[cfg(target_os = "macos")]
    let _library = {
        let loader = library_loader::LibraryLoader::new(
            &std::env::current_exe().unwrap_or_default(),
            subprocess,
        );
        if !loader.load() {
            eprintln!("riptide: could not load the Chromium Embedded Framework");
            return 1;
        }
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = args::Args::new();
    let mut app = RtApp::new(None);
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        return code;
    }
    let Some(Ok((cli, paths))) = early else {
        return 1;
    };
    if let Some(code) = remote::hand_off(&paths, &cli) {
        return code;
    }
    crash::install_panic_hook(&paths.data_dir);
    recover::set_dir(&paths.data_dir);
    let cwd = std::env::current_dir().unwrap_or_default();
    let (mut urls, mut commands) = (Vec::new(), Vec::new());
    for arg in &cli.urls {
        match rt_config::remote::classify(arg, &cwd) {
            rt_config::remote::Item::Url(url) => urls.push(url),
            rt_config::remote::Item::Command(command) => commands.push(command),
        }
    }
    // Chromium names its profile directory "Default" whatever cache_path says.
    let profile = paths.data_dir.join("Default");
    if let Err(e) = std::fs::create_dir_all(&profile) {
        eprintln!("riptide: cannot create {}: {e}", profile.display());
        return 1;
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let sandbox = rt_config::sandbox::detect(&exe_dir, cli.no_sandbox);
    if sandbox.is_on() {
        tracing::info!("Chromium sandbox {}", sandbox.describe());
    } else {
        tracing::warn!("Chromium sandbox {}", sandbox.describe());
    }
    let _ = help::SANDBOX.set(sandbox.describe());
    let data_dir = paths.data_dir.clone();
    let loaded = rt_config::load(&paths);
    let widevine = startup_bool(&loaded, "content.widevine");
    let fetch_widevine = widevine && !privacy::widevine_installed(&paths.data_dir);
    privacy::seed_prefs(&paths.data_dir, &profile, fetch_widevine);
    let settings = Settings {
        no_sandbox: (!sandbox.is_on()).into(),
        persist_session_cookies: 1,
        log_severity: LogSeverity::WARNING,
        root_cache_path: path_string(&paths.data_dir),
        cache_path: path_string(&profile),
        log_file: path_string(&paths.data_dir.join("cef.log")),
        // Also what navigator.languages reports; the header follows changes made later.
        accept_language_list: CefString::from(
            startup_settings(&loaded).str("content.headers.accept_language"),
        ),
        ..Default::default()
    };
    let switches = {
        let settings = startup_settings(&loaded);
        let mut switches = Vec::new();
        let cache = settings.int("content.cache.size");
        if cache > 0 {
            switches.push(("disk-cache-size", Some(cache.to_string())));
        }
        if !settings.bool("content.canvas_reading") {
            switches.push(("disable-reading-from-canvas", None));
        }
        if !settings.bool("content.autoplay") {
            switches.push((
                "autoplay-policy",
                Some("document-user-activation-required".to_string()),
            ));
        }
        if !settings.bool("content.webgl") {
            switches.push(("disable-webgl", None));
        }
        if settings.bool("input.spatial_navigation") {
            switches.push(("enable-spatial-navigation", None));
        }
        if !settings.bool("input.media_keys") {
            switches.push((
                "disable-features",
                Some("HardwareMediaKeyHandling".to_string()),
            ));
        }
        if settings.bool("content.local_content_can_access_file_urls") {
            switches.push(("allow-file-access-from-files", None));
        }
        if settings.str("scrolling.bar") == "overlay" {
            switches.push(("enable-features", Some("OverlayScrollbar".to_string())));
        }
        if settings.bool("content.prefers_reduced_motion") {
            switches.push(("force-prefers-reduced-motion", None));
        }
        switches
    };
    let mut app = RtApp::new(Some(Startup {
        paths,
        urls,
        commands,
        config: std::sync::Arc::new(std::sync::Mutex::new(Some(loaded))),
        switches,
        fetch_widevine,
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
    #[cfg(unix)]
    signals::install();
    run_message_loop();
    #[cfg(unix)]
    let signaled = signals::received();
    #[cfg(not(unix))]
    let signaled = false;
    if client::CLOSED_EVERY_BROWSER.load(std::sync::atomic::Ordering::SeqCst) && !signaled {
        // A clean exit: no crash to recover from next time.
        let autosave = data_dir
            .join("sessions")
            .join(format!("{}.toml", storage::AUTOSAVE_SESSION));
        if let Err(e) = std::fs::remove_file(&autosave)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("can't remove {}: {e}", autosave.display());
        }
        // Reopened crashed tabs that got as far as a clean exit are fine.
        rt_storage::Sessions::new(&data_dir.join("sessions")).set_recovering(false);
    } else {
        tracing::warn!("stopped by a signal; keeping the tabs for crash recovery");
    }
    shutdown();
    remote::cleanup();
    if RESTART.load(std::sync::atomic::Ordering::SeqCst) {
        restart(&cli);
    }
    0
}

/// Start the browser again with the same directories, restoring the session
/// `:restart` saved.
fn restart(cli: &Cli) {
    let Ok(exe) = std::env::current_exe() else {
        return eprintln!("riptide: can't find the program to restart");
    };
    let mut command = std::process::Command::new(exe);
    if let Some(basedir) = &cli.basedir {
        command.arg("--basedir").arg(basedir);
    }
    if cli.no_sandbox {
        command.arg("--no-sandbox");
    }
    command
        .arg(format!(":session-load {RESTART_SESSION}"))
        .arg(format!(":session-delete {RESTART_SESSION}"));
    if let Err(e) = command.spawn() {
        eprintln!("riptide: can't restart: {e}");
    }
}
