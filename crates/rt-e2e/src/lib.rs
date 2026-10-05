//! End-to-end test harness. Each [`Browser`] is a real riptide on its own
//! Xvfb display, with a scratch profile, its own command socket and a local
//! HTTP server for the fixture pages in `pages/`. Tests drive it through the
//! test channel (`rt_config::remote::TestRequest`) and poll its state instead
//! of sleeping.
//!
//! Needs a debug (or `--features test-control`) build of riptide, Xvfb, and
//! Linux. The tests are `#[ignore]`d so a plain `cargo test` never starts
//! browsers; `./task e2e` builds riptide and runs them with `--ignored`.
#![cfg(unix)]

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rt_config::remote::{self, Request, SendError, TestRequest};
use serde::Deserialize;
use serde_json::Value;

/// How long a `wait_*` polls before failing the test.
pub const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Deserialize)]
pub struct State {
    pub mode: String,
    pub current_window: usize,
    pub windows: Vec<WindowState>,
    pub prompt: Option<Value>,
    pub status: Value,
    pub completion: Value,
    /// The hints on screen in hint mode.
    #[serde(default)]
    pub hints: Vec<Hint>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hint {
    pub label: String,
    /// The element's text, lowercased.
    pub text: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WindowState {
    pub private: bool,
    pub current_tab: usize,
    pub tabs: Vec<TabState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TabState {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub loading: bool,
    pub mode: String,
    /// Percent, e.g. 100.
    pub zoom: u32,
}

impl TabState {
    /// Showing `url`, finished loading and titled. A new tab knows its URL
    /// before it starts loading, so the URL alone doesn't mean it's loaded.
    pub fn is_loaded(&self, url: &str) -> bool {
        self.url == url && !self.loading && !self.title.is_empty()
    }
}

impl State {
    pub fn window(&self) -> &WindowState {
        &self.windows[self.current_window]
    }

    pub fn tabs(&self) -> &[TabState] {
        &self.window().tabs
    }

    pub fn tab(&self) -> &TabState {
        &self.window().tabs[self.window().current_tab]
    }

    /// The status bar message, if one is showing.
    pub fn message(&self) -> Option<&str> {
        self.status.pointer("/message/text")?.as_str()
    }
}

/// Options for starting a [`Browser`]: `Browser::launch().lua("…").start("page.html")`.
#[derive(Default)]
pub struct Launch {
    config_toml: Option<String>,
    config_lua: Option<String>,
}

impl Launch {
    /// The profile's `config.toml`.
    pub fn toml(mut self, text: &str) -> Self {
        self.config_toml = Some(text.into());
        self
    }

    /// The profile's `config.lua`.
    pub fn lua(mut self, text: &str) -> Self {
        self.config_lua = Some(text.into());
        self
    }

    /// Start on `page` (a file in `pages/`) and wait until it has loaded.
    pub fn start(self, page: &str) -> Browser {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        // Short, because a Unix socket path must fit in ~108 bytes.
        let dir = std::env::temp_dir().join(format!("rt-e2e-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let run = dir.join("run");
        let base = dir.join("base");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::create_dir_all(base.join("config")).unwrap();
        set_private(&run);
        if let Some(text) = &self.config_toml {
            std::fs::write(base.join("config/config.toml"), text).unwrap();
        }
        if let Some(text) = &self.config_lua {
            std::fs::write(base.join("config/config.lua"), text).unwrap();
        }

        let port = serve_pages();
        let (xvfb, display) = start_xvfb();
        let url = format!("http://127.0.0.1:{port}/{page}");
        let socket = remote::socket_path(&base.join("data"), Some(&run));
        let browser = spawn_browser(&dir, &display, &[url.as_str()]);
        let b = Browser {
            dir,
            socket,
            port,
            display,
            browser: RefCell::new(browser),
            xvfb,
        };
        b.wait_until("the start page loads", |s| {
            s.tabs().len() == 1 && s.tab().is_loaded(&url)
        });
        b.wait_painted();
        b
    }
}

/// A running browser, stopped (with its display and server) when dropped.
pub struct Browser {
    dir: PathBuf,
    socket: PathBuf,
    port: u16,
    display: String,
    browser: RefCell<Child>,
    xvfb: Child,
}

impl Browser {
    /// Start on `page` (a file in `pages/`) with no config, and wait until it has loaded.
    pub fn start(page: &str) -> Self {
        Launch::default().start(page)
    }

    /// Start with options, e.g. a `config.lua`.
    pub fn launch() -> Launch {
        Launch::default()
    }

    /// The profile's config directory (`config.toml`, `autoconfig.toml`, quickmarks…).
    pub fn config_dir(&self) -> PathBuf {
        self.dir.join("base/config")
    }

    /// The profile's data directory (history, sessions, downloads…).
    pub fn data_dir(&self) -> PathBuf {
        self.dir.join("base/data")
    }

    /// Wait for the browser to exit by itself, e.g. after `:quit`.
    pub fn wait_exit(&self) -> std::process::ExitStatus {
        let start = Instant::now();
        while start.elapsed() < TIMEOUT {
            if let Ok(Some(status)) = self.browser.borrow_mut().try_wait() {
                self.stop_helpers();
                return status;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("riptide didn't exit:\n{}", self.log_tail());
    }

    /// Send SIGTERM, as `kill`, `pkill` or a logout would.
    pub fn terminate(&self) {
        let pid = self.browser.borrow().id();
        let _ = Command::new("kill")
            .args(["-s", "TERM", &pid.to_string()])
            .status();
    }

    /// Kill the browser without letting it shut down, as a crash would.
    pub fn crash(&self) {
        self.stop_browser();
    }

    /// Start the browser again on the same profile and display, with no URL,
    /// once the last one has exited or crashed. Waits until it answers.
    pub fn restart(&self) {
        assert!(
            matches!(self.browser.borrow_mut().try_wait(), Ok(Some(_))),
            "restart() needs the browser to have exited first"
        );
        // A crash can leave the socket behind; the new browser replaces it.
        *self.browser.borrow_mut() = spawn_browser(&self.dir, &self.display, &[]);
        self.wait_until("the browser is back", |s| !s.tabs().is_empty());
    }

    /// The URL of a fixture page on this browser's server.
    pub fn url(&self, page: &str) -> String {
        format!("http://127.0.0.1:{}/{page}", self.port)
    }

    /// Press keys, e.g. `5j`, `<Escape>` or `:open x<Return>`.
    pub fn keys(&self, keys: &str) {
        self.request(TestRequest::Keys { keys: keys.into() })
            .unwrap_or_else(|e| panic!("keys {keys:?}: {e}"));
    }

    /// Run a command line, as `:` would.
    pub fn run(&self, command: &str) {
        self.request(TestRequest::Run {
            command: command.into(),
        })
        .unwrap_or_else(|e| panic!("run {command:?}: {e}"));
    }

    pub fn state(&self) -> State {
        let data = self
            .request(TestRequest::State)
            .unwrap_or_else(|e| panic!("state: {e}"));
        serde_json::from_value(data.unwrap_or_default()).expect("bad state")
    }

    /// Evaluate JavaScript in the current tab; its value must be a string.
    pub fn eval(&self, code: &str) -> String {
        self.try_eval(code)
            .unwrap_or_else(|e| panic!("eval {code:?}: {e}"))
    }

    pub fn try_eval(&self, code: &str) -> Result<String, String> {
        let data = self.request(TestRequest::Eval {
            code: code.into(),
            tab: None,
        })?;
        Ok(data
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default())
    }

    /// Poll the state until `test` holds; fails the test after [`TIMEOUT`].
    pub fn wait_until(&self, what: &str, test: impl Fn(&State) -> bool) -> State {
        let start = Instant::now();
        let mut last = None;
        while start.elapsed() < TIMEOUT {
            self.check_running();
            // The socket isn't there until the browser has started.
            if let Ok(Some(data)) = self.request(TestRequest::State)
                && let Ok(state) = serde_json::from_value::<State>(data)
            {
                if !state.windows.is_empty() && test(&state) {
                    return state;
                }
                last = Some(state);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out waiting until {what}; last state: {last:#?}");
    }

    /// Start hints with `command` (e.g. `hint` or `hint inputs`) and press the
    /// label of the first element `pick` chooses, which clicks it for real.
    pub fn follow_hint(&self, command: &str, pick: impl Fn(&Hint) -> bool) {
        self.run(command);
        let state = self.wait_until("hints are shown", |s| {
            s.mode == "hint" && !s.hints.is_empty()
        });
        let Some(hint) = state.hints.iter().find(|h| pick(h)) else {
            panic!("no hint matches; hints: {:#?}", state.hints);
        };
        self.keys(&hint.label);
    }

    pub fn wait_mode(&self, mode: &str) -> State {
        self.wait_until(&format!("the mode is {mode}"), |s| s.mode == mode)
    }

    /// Poll a script until it returns `expected`.
    pub fn wait_eval(&self, code: &str, expected: &str) {
        let start = Instant::now();
        let mut last = Err(String::new());
        while start.elapsed() < TIMEOUT {
            last = self.try_eval(code);
            if last.as_deref() == Ok(expected) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out waiting for {code:?} to be {expected:?}; last: {last:?}");
    }

    /// Open a fixture page in the current tab and wait until it has loaded.
    pub fn open(&self, page: &str) {
        let url = self.url(page);
        self.run(&format!("open {url}"));
        self.wait_until(&format!("{page} loads"), |s| s.tab().is_loaded(&url));
        self.wait_painted();
    }

    /// Wait until the current page has drawn a frame. Under load, Chromium
    /// drops keys sent to a page that has loaded but not painted yet.
    pub fn wait_painted(&self) {
        self.eval(
            "requestAnimationFrame(() => requestAnimationFrame(() => { window.__rtPainted = true })); 'ok'",
        );
        self.wait_eval("String(window.__rtPainted === true)", "true");
    }

    fn request(&self, test: TestRequest) -> Result<Option<Value>, String> {
        let request = Request {
            version: remote::PROTOCOL_VERSION,
            cwd: PathBuf::from("/"),
            args: Vec::new(),
            target: None,
            test: Some(test),
        };
        remote::exchange(&self.socket, &request, Duration::from_secs(30)).map_err(|e| match e {
            SendError::NotRunning => "the browser isn't listening".into(),
            SendError::Failed(e) => e,
        })
    }

    fn check_running(&self) {
        if let Ok(Some(status)) = self.browser.borrow_mut().try_wait() {
            panic!("riptide exited early ({status}):\n{}", self.log_tail());
        }
    }

    /// Only this browser's processes, never another riptide: its process
    /// group, then the CEF helpers (the zygote leaves the group), which all
    /// carry this profile's --user-data-dir.
    fn stop_browser(&self) {
        let mut browser = self.browser.borrow_mut();
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{}", browser.id())])
            .status();
        let _ = browser.wait();
        drop(browser);
        self.stop_helpers();
    }

    fn stop_helpers(&self) {
        let profile = self.data_dir();
        let _ = Command::new("pkill")
            .args([
                "-KILL",
                "-f",
                "--",
                &format!("[-]-user-data-dir={}", profile.display()),
            ])
            .status();
    }

    fn log_tail(&self) -> String {
        let log = std::fs::read_to_string(self.dir.join("browser.log")).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(40)..].join("\n")
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "--- riptide log ({}):\n{}",
                self.dir.display(),
                self.log_tail()
            );
        }
        self.stop_browser();
        let _ = self.xvfb.kill();
        let _ = self.xvfb.wait();
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn spawn_browser(dir: &Path, display: &str, args: &[&str]) -> Child {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("browser.log"))
        .unwrap();
    let mut command = Command::new(binary());
    command
        .arg("--basedir")
        .arg(dir.join("base"))
        .args(args)
        .env("DISPLAY", format!(":{display}"))
        .env("XDG_RUNTIME_DIR", dir.join("run"))
        .env(
            "RT_LOG",
            std::env::var("RT_LOG").unwrap_or_else(|_| "info".into()),
        )
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    // Its own process group, so CEF's helpers are stopped with it.
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn().expect("can't start riptide")
}

/// The browser under test: `RIPTIDE_BIN`, or the workspace's debug build.
fn binary() -> PathBuf {
    let path = std::env::var_os("RIPTIDE_BIN").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/riptide"),
        PathBuf::from,
    );
    assert!(
        path.exists(),
        "{} not found; run `cargo build` first (./task e2e does)",
        path.display()
    );
    path
}

fn set_private(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
}

/// Xvfb picks a free display itself and reports it on stdout (`-displayfd 1`).
fn start_xvfb() -> (Child, String) {
    let (reader, writer) = std::io::pipe().unwrap();
    let mut xvfb = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1280x800x24",
            "-nolisten",
            "tcp",
        ])
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(Stdio::null())
        .spawn()
        .expect("can't start Xvfb; install it to run the e2e tests");
    let mut display = String::new();
    BufReader::new(reader).read_line(&mut display).unwrap();
    let display = display.trim().to_string();
    if display.is_empty() {
        let _ = xvfb.kill();
        panic!("Xvfb didn't report a display");
    }
    (xvfb, display)
}

/// Serve `pages/` on a free local port; the thread lives as long as the test process.
fn serve_pages() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("pages");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || serve(stream, &root));
        }
    });
    port
}

fn serve(mut stream: std::net::TcpStream, root: &Path) {
    let mut head = [0u8; 4096];
    let n = stream.read(&mut head).unwrap_or(0);
    let request = String::from_utf8_lossy(&head[..n]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let file = (!path.contains("..")).then(|| root.join(path));
    let (status, body) = match file.and_then(|f| std::fs::read(f).ok()) {
        Some(body) => ("200 OK", body),
        None => ("404 Not Found", b"not found".to_vec()),
    };
    let kind = match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        _ => "text/plain",
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(&body);
}
