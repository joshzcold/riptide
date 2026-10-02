//! Talking to an already running browser: `hackers-browser url ':command'`
//! from a terminal hands its arguments to the instance for the same profile.
//!
//! One JSON line per connection, answered by one JSON line:
//! `{"version":1,"cwd":"/home/u","args":["example.com",":tab-focus 1"],"target":null}`
//! → `{"ok":true}`. Unix only for now; the socket lives in a `0700`
//! directory and is `0600`, so only the same user can connect.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
    let mut url = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
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
/// instances don't talk to each other.
pub fn socket_path(data_dir: &Path, runtime_dir: Option<&Path>) -> PathBuf {
    let name = format!("{}.sock", &short_hash(&data_dir.to_string_lossy())[..12]);
    match runtime_dir {
        Some(dir) => dir.join("hackers-browser").join(name),
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

/// Unix sockets aren't implemented on Windows yet; every start is a new instance.
#[cfg(not(unix))]
pub fn send(_socket: &Path, _request: &Request) -> Result<(), SendError> {
    Err(SendError::NotRunning)
}

#[cfg(unix)]
pub fn send(socket: &Path, request: &Request) -> Result<(), SendError> {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let mut stream = UnixStream::connect(socket).map_err(|_| SendError::NotRunning)?;
    let fail = |e: std::io::Error| SendError::Failed(e.to_string());
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(fail)?;
    let mut line = serde_json::to_string(request).map_err(|e| SendError::Failed(e.to_string()))?;
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(fail)?;
    let mut reply = String::new();
    BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(fail)?;
    match serde_json::from_str::<Reply>(&reply) {
        Ok(Reply { ok: true, .. }) => Ok(()),
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
    path: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Server {
    #[cfg(not(unix))]
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
        handle: impl Fn(Request) -> Result<(), String> + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            for stream in self.listener.incoming() {
                let Ok(stream) = stream else { continue };
                let reply = match read_request(&stream) {
                    Ok(request) => match handle(request) {
                        Ok(()) => Reply {
                            ok: true,
                            error: None,
                        },
                        Err(e) => Reply {
                            ok: false,
                            error: Some(e),
                        },
                    },
                    Err(e) => Reply {
                        ok: false,
                        error: Some(e),
                    },
                };
                let mut line = serde_json::to_string(&reply).unwrap_or_default();
                line.push('\n');
                let _ = (&stream).write_all(line.as_bytes());
            }
            drop(self);
        })
    }

    #[cfg(not(unix))]
    pub fn spawn(
        self,
        _handle: impl Fn(Request) -> Result<(), String> + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(|| {})
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn read_request(stream: &std::os::unix::net::UnixStream) -> Result<Request, String> {
    use std::io::Read;
    use std::time::Duration;

    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    BufReader::new(stream.take(MAX_REQUEST_BYTES))
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let request: Request = serde_json::from_str(&line).map_err(|e| format!("bad request: {e}"))?;
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
        let cwd = std::env::temp_dir().join(format!("hb-remote-{}", std::process::id()));
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
    fn socket_paths_are_per_profile() {
        let run = Path::new("/run/user/1000");
        let a = socket_path(Path::new("/home/u/.local/share/hackers-browser"), Some(run));
        let b = socket_path(Path::new("/tmp/other/data"), Some(run));
        assert_ne!(a, b);
        assert!(a.starts_with("/run/user/1000/hackers-browser"));
        assert_eq!(
            a,
            socket_path(Path::new("/home/u/.local/share/hackers-browser"), Some(run))
        );
        assert!(socket_path(Path::new("/d"), None).starts_with("/d/ipc"));
    }

    #[cfg(unix)]
    #[test]
    fn round_trip_over_a_socket() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::mpsc;

        let dir = std::env::temp_dir().join(format!("hb-sock-{}", std::process::id()));
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
            tx.send(req).unwrap();
            if refuse {
                Err("no thanks".into())
            } else {
                Ok(())
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

        let mut old = request(vec![]);
        old.version = 99;
        assert!(
            matches!(send(&path, &old), Err(SendError::Failed(e)) if e.contains("protocol version 99"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn replaces_a_stale_socket() {
        let dir = std::env::temp_dir().join(format!("hb-stale-{}", std::process::id()));
        let path = dir.join("stale.sock");
        drop(Server::bind(&path).unwrap());
        // Simulate a crash: the file exists but nobody listens.
        std::os::unix::net::UnixListener::bind(&path)
            .map(drop)
            .unwrap();
        assert!(Server::bind(&path).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn request(args: Vec<String>) -> Request {
        Request {
            version: PROTOCOL_VERSION,
            cwd: PathBuf::from("/"),
            args,
            target: None,
        }
    }
}
