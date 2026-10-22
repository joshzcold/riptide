//! Talking to an already running browser: `riptide url ':command'`
//! from a terminal hands its arguments to the instance for the same profile.
//!
//! One JSON line per connection, answered by one JSON line:
//! `{"version":1,"cwd":"/home/u","args":["example.com",":tab-focus 1"],"target":null}`
//! → `{"ok":true}`. On Unix the socket lives in a `0700` directory and is
//! `0600`, so only the same user can connect; on Windows it's a named pipe
//! that only its own user may open (see `remote_pipe.rs`).
//!
//! Test builds also answer [`TestRequest`]s (`"test":{"op":"state"}`), whose
//! replies carry `data`; release builds refuse them.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
#[path = "remote_pipe.rs"]
mod pipe;

pub const PROTOCOL_VERSION: u32 = 1;
/// Bigger requests are refused rather than read into memory.
const MAX_REQUEST_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub cwd: PathBuf,
    pub args: Vec<String>,
    /// `--target` override: tab, tab-bg, window or current.
    pub target: Option<String>,
    /// Drive the browser from a test instead of handing it arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestRequest>,
}

/// What end-to-end tests can ask a test build (`debug_assertions` or the
/// `test-control` feature) to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum TestRequest {
    /// Press keys, e.g. `5j` or `:open -t x<Return>`, as if typed.
    Keys { keys: String },
    /// Run a command line, as `:` would.
    Run { command: String },
    /// A JSON snapshot of modes, windows, tabs, the status bar and prompts.
    State,
    /// Evaluate JavaScript in a tab of the current window (default: the
    /// current tab); the reply is the script's string result.
    Eval { code: String, tab: Option<usize> },
    /// Evaluate JavaScript in the current window's `tabbar`, `statusbar` or `completion` overlay.
    EvalBar { code: String, bar: String },
    /// Crash the current tab's renderer process.
    CrashTab,
    /// Move and size the current window, in screen DIPs, as a window manager would.
    SetWindowBounds {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    /// Panic on the browser's UI thread, as a bug in riptide would.
    Panic,
}

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// What one command line argument asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A `:command` (without the colon).
    Command(String),
    /// A URL, search text or `file://` URL for an existing path.
    Url(String),
}

/// Commands start with `:`; existing files become `file://` URLs (relative
/// to the caller's directory); anything else is left for `:open` to guess.
pub fn classify(arg: &str, cwd: &Path) -> Item {
    if let Some(command) = arg.strip_prefix(':') {
        return Item::Command(command.to_string());
    }
    let path = cwd.join(arg);
    let looks_like_path = arg.starts_with(['/', '.', '~']) || path.exists();
    if looks_like_path
        && !arg.contains("://")
        && let Ok(full) = path.canonicalize()
    {
        return Item::Url(file_url(&full));
    }
    Item::Url(arg.to_string())
}

fn file_url(path: &Path) -> String {
    file_url_from(&path.to_string_lossy(), cfg!(windows))
}

/// Windows paths lose `canonicalize`'s `\\?\` prefix and use forward slashes,
/// so `C:\a b` becomes `file:///C:/a%20b`.
fn file_url_from(path: &str, windows: bool) -> String {
    let path = if windows {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('\\', "/")
    } else {
        path.to_string()
    };
    let mut url = String::from("file://");
    if !path.starts_with('/') {
        url.push('/');
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                url.push(byte as char)
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`.
fn short_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// The socket for a profile. One per data directory, so `--basedir`
/// instances don't talk to each other. On Windows it's a named pipe, and the
/// runtime directory doesn't matter.
pub fn socket_path(data_dir: &Path, runtime_dir: Option<&Path>) -> PathBuf {
    let hash = &short_hash(&data_dir.to_string_lossy())[..12];
    if cfg!(windows) {
        return PathBuf::from(format!(r"\\.\pipe\riptide-{hash}"));
    }
    let name = format!("{hash}.sock");
    match runtime_dir {
        Some(dir) => dir.join("riptide").join(name),
        None => data_dir.join("ipc").join(name),
    }
}

pub fn current_socket_path(data_dir: &Path) -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    socket_path(data_dir, runtime.as_deref())
}

#[derive(Debug)]
pub enum SendError {
    /// Nobody is listening: start a new instance.
    NotRunning,
    /// Someone answered, but refused or broke the exchange.
    Failed(String),
}

pub fn send(socket: &Path, request: &Request) -> Result<(), SendError> {
    exchange(socket, request, std::time::Duration::from_secs(5)).map(drop)
}

/// Send `request` and return the reply's data, waiting up to `timeout`.
#[cfg(not(any(unix, windows)))]
pub fn exchange(
    _socket: &Path,
    _request: &Request,
    _timeout: std::time::Duration,
) -> Result<Option<serde_json::Value>, SendError> {
    Err(SendError::NotRunning)
}

#[cfg(unix)]
pub fn exchange(
    socket: &Path,
    request: &Request,
    timeout: std::time::Duration,
) -> Result<Option<serde_json::Value>, SendError> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket).map_err(|_| SendError::NotRunning)?;
    let fail = |e: std::io::Error| SendError::Failed(e.to_string());
    stream.set_read_timeout(Some(timeout)).map_err(fail)?;
    let mut line = serde_json::to_string(request).map_err(|e| SendError::Failed(e.to_string()))?;
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(fail)?;
    let mut reply = String::new();
    BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(fail)?;
    parse_reply(&reply)
}

/// A pipe held by another user's process is refused, rather than taken for
/// a browser that isn't running.
#[cfg(windows)]
pub fn exchange(
    socket: &Path,
    request: &Request,
    timeout: std::time::Duration,
) -> Result<Option<serde_json::Value>, SendError> {
    let stream = pipe::connect(socket).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => SendError::Failed(e.to_string()),
        _ => SendError::NotRunning,
    })?;
    let mut line = serde_json::to_string(request).map_err(|e| SendError::Failed(e.to_string()))?;
    line.push('\n');
    (&stream)
        .write_all(line.as_bytes())
        .map_err(|e| SendError::Failed(e.to_string()))?;
    // Pipes have no read timeout: read on a thread, and stop waiting for it.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reply = String::new();
        let read = BufReader::new(&stream).read_line(&mut reply).map(|_| reply);
        let _ = tx.send(read);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(reply)) => parse_reply(&reply),
        Ok(Err(e)) => Err(SendError::Failed(e.to_string())),
        Err(_) => Err(SendError::Failed("no reply".into())),
    }
}

#[cfg(any(unix, windows))]
fn parse_reply(reply: &str) -> Result<Option<serde_json::Value>, SendError> {
    match serde_json::from_str::<Reply>(reply) {
        Ok(Reply { ok: true, data, .. }) => Ok(data),
        Ok(Reply { error, .. }) => {
            Err(SendError::Failed(error.unwrap_or_else(|| "refused".into())))
        }
        Err(e) => Err(SendError::Failed(format!("bad reply: {e}"))),
    }
}

/// A listening socket, removed again when dropped.
pub struct Server {
    #[cfg(unix)]
    listener: std::os::unix::net::UnixListener,
    #[cfg(windows)]
    listener: pipe::Listener,
    path: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Server {
    #[cfg(not(any(unix, windows)))]
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no single-instance support on this platform yet",
        ))
    }

    /// Bind, replacing a stale socket a crashed instance left behind.
    #[cfg(unix)]
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        use std::os::unix::net::{UnixListener, UnixStream};

        if let Some(dir) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "another instance is running",
                ));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Accept requests on a background thread, calling `handle` for each.
    /// `handle` runs on that thread; the browser posts work to its UI thread.
    #[cfg(unix)]
    pub fn spawn(
        self,
        handle: impl Fn(Request) -> Result<Option<serde_json::Value>, String> + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            for stream in self.listener.incoming() {
                let Ok(stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let reply = reply_for(
                    read_line(&stream)
                        .and_then(|l| parse_request(&l))
                        .and_then(&handle),
                );
                let _ = (&stream).write_all(reply.as_bytes());
            }
            drop(self);
        })
    }

    /// A second instance fails with `AddrInUse`; a crashed one leaves no
    /// pipe behind, so there's nothing stale to replace.
    #[cfg(windows)]
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            listener: pipe::Listener::bind(path)?,
            path: path.to_path_buf(),
        })
    }

    /// As on Unix, but each connection's reading and writing happens on its
    /// own thread, since a pipe has no timeout: a client that never sends
    /// can't hold up the others.
    #[cfg(windows)]
    pub fn spawn(
        mut self,
        handle: impl Fn(Request) -> Result<Option<serde_json::Value>, String> + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        use std::sync::mpsc;
        use std::time::Duration;

        std::thread::spawn(move || {
            while let Ok(stream) = self.listener.accept() {
                let (request_tx, request_rx) = mpsc::channel();
                let (reply_tx, reply_rx) = mpsc::channel::<String>();
                std::thread::spawn(move || {
                    let _ = request_tx.send(read_line(&stream));
                    if let Ok(line) = reply_rx.recv() {
                        let _ = (&stream).write_all(line.as_bytes());
                        // The reply must reach the client before the pipe closes.
                        let _ = stream.sync_all();
                    }
                });
                let line = match request_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(line) => line,
                    Err(_) => continue,
                };
                let reply = reply_for(line.and_then(|l| parse_request(&l)).and_then(&handle));
                let _ = reply_tx.send(reply);
            }
            drop(self);
        })
    }

    #[cfg(not(any(unix, windows)))]
    pub fn spawn(
        self,
        _handle: impl Fn(Request) -> Result<Option<serde_json::Value>, String> + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(|| {})
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The reply line for a handled (or refused) request.
#[cfg(any(unix, windows))]
fn reply_for(result: Result<Option<serde_json::Value>, String>) -> String {
    let reply = match result {
        Ok(data) => Reply {
            ok: true,
            error: None,
            data,
        },
        Err(e) => Reply {
            ok: false,
            error: Some(e),
            data: None,
        },
    };
    let mut line = serde_json::to_string(&reply).unwrap_or_default();
    line.push('\n');
    line
}

/// One request line, at most [`MAX_REQUEST_BYTES`].
#[cfg(any(unix, windows))]
fn read_line(stream: impl std::io::Read) -> Result<String, String> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_REQUEST_BYTES))
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line)
}

#[cfg(any(unix, windows))]
fn parse_request(line: &str) -> Result<Request, String> {
    let request: Request = serde_json::from_str(line).map_err(|e| format!("bad request: {e}"))?;
    if request.version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol version {} not supported (this browser speaks {PROTOCOL_VERSION})",
            request.version
        ));
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_arguments() {
        let cwd = std::env::temp_dir().join(format!("rt-remote-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(cwd.join("page one.html"), "x").unwrap();
        assert_eq!(
            classify(":tab-focus 1", &cwd),
            Item::Command("tab-focus 1".into())
        );
        assert_eq!(
            classify("example.com", &cwd),
            Item::Url("example.com".into())
        );
        assert_eq!(
            classify("https://x.org/a", &cwd),
            Item::Url("https://x.org/a".into())
        );
        let Item::Url(url) = classify("page one.html", &cwd) else {
            panic!()
        };
        assert!(
            url.starts_with("file:///") && url.ends_with("/page%20one.html"),
            "{url}"
        );
        std::fs::remove_dir_all(&cwd).unwrap();
    }

    #[test]
    fn file_urls() {
        assert_eq!(file_url_from("/a b/c#d", false), "file:///a%20b/c%23d");
        assert_eq!(
            file_url_from(r"\\?\C:\Users\me\page one.html", true),
            "file:///C:/Users/me/page%20one.html"
        );
    }

    #[cfg(unix)]
    #[test]
    fn socket_paths_are_per_profile() {
        let run = Path::new("/run/user/1000");
        let a = socket_path(Path::new("/home/u/.local/share/riptide"), Some(run));
        let b = socket_path(Path::new("/tmp/other/data"), Some(run));
        assert_ne!(a, b);
        assert!(a.starts_with("/run/user/1000/riptide"));
        assert_eq!(
            a,
            socket_path(Path::new("/home/u/.local/share/riptide"), Some(run))
        );
        assert!(socket_path(Path::new("/d"), None).starts_with("/d/ipc"));
    }

    #[cfg(unix)]
    #[test]
    fn round_trip_over_a_socket() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::mpsc;

        let dir = std::env::temp_dir().join(format!("rt-sock-{}", std::process::id()));
        let path = dir.join("s").join("test.sock");
        assert!(matches!(
            send(&path, &request(vec![])),
            Err(SendError::NotRunning)
        ));

        let server = Server::bind(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
        // A second instance can't take over a live socket.
        assert!(Server::bind(&path).is_err());

        let (tx, rx) = mpsc::channel();
        server.spawn(move |req| {
            let refuse = req.args.iter().any(|a| a == "refuse");
            let state = req.test == Some(TestRequest::State);
            tx.send(req).unwrap();
            if refuse {
                Err("no thanks".into())
            } else if state {
                Ok(Some(serde_json::json!({"mode": "normal"})))
            } else {
                Ok(None)
            }
        });
        send(
            &path,
            &request(vec!["example.com".into(), ":reload".into()]),
        )
        .unwrap();
        assert_eq!(rx.recv().unwrap().args, ["example.com", ":reload"]);
        assert!(
            matches!(send(&path, &request(vec!["refuse".into()])), Err(SendError::Failed(e)) if e == "no thanks")
        );

        let mut state = request(vec![]);
        state.test = Some(TestRequest::State);
        let reply = exchange(&path, &state, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(reply, Some(serde_json::json!({"mode": "normal"})));

        let mut old = request(vec![]);
        old.version = 99;
        assert!(
            matches!(send(&path, &old), Err(SendError::Failed(e)) if e.contains("protocol version 99"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_requests_are_tagged_by_op() {
        let keys = TestRequest::Keys { keys: "5j".into() };
        let json = serde_json::to_string(&keys).unwrap();
        assert_eq!(json, r#"{"op":"keys","keys":"5j"}"#);
        assert_eq!(serde_json::from_str::<TestRequest>(&json).unwrap(), keys);
        // Requests from older clients have no test field.
        let plain: Request =
            serde_json::from_str(r#"{"version":1,"cwd":"/","args":[],"target":null}"#).unwrap();
        assert_eq!(plain.test, None);
    }

    #[cfg(windows)]
    #[test]
    fn pipe_names_are_per_profile() {
        let a = socket_path(Path::new(r"C:\Users\u\AppData\Roaming\riptide"), None);
        let b = socket_path(Path::new(r"C:\tmp\other"), None);
        assert_ne!(a, b);
        assert!(
            a.to_string_lossy().starts_with(r"\\.\pipe\riptide-"),
            "{a:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn round_trip_over_a_pipe() {
        let path = PathBuf::from(format!(r"\\.\pipe\riptide-test-{}", std::process::id()));
        assert!(matches!(
            send(&path, &request(vec![])),
            Err(SendError::NotRunning)
        ));
        let server = Server::bind(&path).unwrap();
        // A second instance can't take over a live pipe.
        let second = Server::bind(&path).err().map(|e| e.kind());
        assert_eq!(second, Some(std::io::ErrorKind::AddrInUse));

        let (tx, rx) = std::sync::mpsc::channel();
        server.spawn(move |req| {
            let refuse = req.args.iter().any(|a| a == "refuse");
            let state = req.test == Some(TestRequest::State);
            tx.send(req).unwrap();
            if refuse {
                Err("no thanks".into())
            } else if state {
                Ok(Some(serde_json::json!({"mode": "normal"})))
            } else {
                Ok(None)
            }
        });
        send(
            &path,
            &request(vec!["example.com".into(), ":reload".into()]),
        )
        .unwrap();
        assert_eq!(rx.recv().unwrap().args, ["example.com", ":reload"]);
        assert!(
            matches!(send(&path, &request(vec!["refuse".into()])), Err(SendError::Failed(e)) if e == "no thanks")
        );
        let mut state = request(vec![]);
        state.test = Some(TestRequest::State);
        let reply = exchange(&path, &state, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(reply, Some(serde_json::json!({"mode": "normal"})));
        // One after another, each finds an instance waiting.
        for _ in 0..5 {
            send(&path, &request(vec![])).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn replaces_a_stale_socket() {
        let dir = std::env::temp_dir().join(format!("rt-stale-{}", std::process::id()));
        let path = dir.join("stale.sock");
        drop(Server::bind(&path).unwrap());
        // Simulate a crash: the file exists but nobody listens. A listener
        // could live on briefly in a child another test is forking.
        std::os::unix::net::UnixDatagram::bind(&path)
            .map(drop)
            .unwrap();
        Server::bind(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn request(args: Vec<String>) -> Request {
        Request {
            version: PROTOCOL_VERSION,
            cwd: PathBuf::from("/"),
            args,
            target: None,
            test: None,
        }
    }
}
