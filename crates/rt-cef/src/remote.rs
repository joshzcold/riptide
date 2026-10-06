//! Single instance: hand arguments to a running browser, or listen for them.

use std::cell::RefCell;
use std::sync::Mutex;

use cef::*;
use rt_config::remote::{self, Item, Request, SendError, Server};
use rt_config::{Cli, Paths};
use rt_core::command::OpenTarget;
use rt_core::engine::Level;

use crate::shell;

/// Bound before CEF starts (so a second process sees it), served once CEF is up.
static SERVER: Mutex<Option<Server>> = Mutex::new(None);
/// Removed on exit, since the listener thread never returns to drop the server.
static SOCKET: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Send `cli`'s arguments to a running instance for this profile. Returns the
/// exit code if one took them; `None` means this process should start.
pub fn hand_off(paths: &Paths, cli: &Cli) -> Option<i32> {
    let socket = remote::current_socket_path(&paths.data_dir);
    let request = Request {
        version: remote::PROTOCOL_VERSION,
        cwd: std::env::current_dir().unwrap_or_default(),
        args: cli.urls.clone(),
        target: cli.target.clone(),
        test: None,
    };
    match remote::send(&socket, &request) {
        Ok(()) => Some(0),
        Err(SendError::Failed(e)) => {
            eprintln!("riptide: the running instance refused: {e}");
            Some(1)
        }
        Err(SendError::NotRunning) => {
            match Server::bind(&socket) {
                Ok(server) => {
                    *SOCKET.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(server.path().to_path_buf());
                    *SERVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(server);
                }
                Err(e) => tracing::warn!("not accepting commands from other processes: {e}"),
            }
            None
        }
    }
}

/// Remove our socket after the browser has shut down.
pub fn cleanup() {
    if let Some(path) = SOCKET.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = std::fs::remove_file(path);
    }
}

/// Start accepting requests; each one runs on the UI thread.
pub fn listen() {
    let Some(server) = SERVER.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    server.spawn(|mut request| {
        if let Some(test) = request.test.take() {
            return crate::test_control::handle(test);
        }
        let mut task = HandleRequest::new(RefCell::new(Some(request)));
        if post_task(ThreadId::UI, Some(&mut task)) == 0 {
            return Err("the browser is shutting down".into());
        }
        Ok(None)
    });
}

fn target(name: Option<&str>) -> OpenTarget {
    match name {
        Some("current") => OpenTarget::Current,
        Some("tab-bg") => OpenTarget::Background,
        Some("window") => OpenTarget::Window,
        _ => OpenTarget::Tab,
    }
}

/// Run `:commands` given on the command line (first instance or remote).
pub fn run_commands(commands: &[String]) {
    for command in commands {
        if let Some(effects) = shell::with(|s| s.engine.execute_str(command, None)) {
            shell::apply(effects);
        }
    }
}

/// `new_instance_open_target_window`: make that window the active one.
/// The active window is the last focused one already.
fn choose_window() {
    shell::with(|s| {
        let open: Vec<usize> = s
            .windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.window.is_some() && !w.private)
            .map(|(i, _)| i)
            .collect();
        let chosen = match s.engine.settings().str("new_instance_open_target_window") {
            "first-opened" => open.first(),
            "last-opened" => open.last(),
            _ => None,
        };
        if let Some(&index) = chosen {
            s.active = index;
        }
    });
}

fn handle(request: Request) {
    let configured = shell::with(|s| {
        s.engine
            .settings()
            .str("new_instance_open_target")
            .to_string()
    });
    let target = target(request.target.as_deref().or(configured.as_deref()));
    choose_window();
    let mut commands = Vec::new();
    for arg in &request.args {
        match remote::classify(arg, &request.cwd) {
            Item::Url(url) => shell::open(target, false, Some(url)),
            Item::Command(command) => commands.push(command),
        }
    }
    run_commands(&commands);
    if request.args.is_empty() {
        shell::show_message(
            Level::Info,
            "Another riptide was started; using this window",
        );
    }
    if let Some(window) = shell::with(|s| s.window.clone()).flatten() {
        window.activate();
    }
}

wrap_task! {
    struct HandleRequest {
        // A task runs once; the cell lets `execute(&self)` take the request.
        request: RefCell<Option<Request>>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(request) = self.request.borrow_mut().take() {
                handle(request);
            }
        }
    }
}
