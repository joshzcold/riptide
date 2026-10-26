//! The browser starts on Windows and macOS and opens a window. CI runs it
//! with CEF's runtime next to `riptide.exe`, or on macOS in `riptide.app`
//! (`RIPTIDE_EXE` names the binary inside it).
#![cfg(any(windows, target_os = "macos"))]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rt_config::remote::{self, Request, SendError, TestRequest};

const TIMEOUT: Duration = Duration::from_secs(90);

fn request(socket: &Path, test: TestRequest) -> Result<Option<serde_json::Value>, SendError> {
    let request = Request {
        version: remote::PROTOCOL_VERSION,
        cwd: std::env::temp_dir(),
        args: Vec::new(),
        target: None,
        test: Some(test),
    };
    remote::exchange(socket, &request, Duration::from_secs(30))
}

/// The end of riptide's own log and CEF's, for a failure message.
fn logs(base: &Path) -> String {
    ["riptide.log", "data/cef.log"]
        .iter()
        .map(|name| {
            let text = std::fs::read_to_string(base.join(name)).unwrap_or_default();
            let tail: Vec<&str> = text.lines().rev().take(40).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            format!("--- {name}\n{}", tail.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[ignore = "starts the browser; needs CEF's runtime next to riptide.exe or an app bundle"]
fn the_browser_starts_opens_a_window_and_quits() {
    // macOS's temporary folder is too long for the socket's path.
    let tmp = if cfg!(windows) {
        std::env::temp_dir()
    } else {
        "/tmp".into()
    };
    let base = tmp.join(format!("rt-start-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let exe =
        std::env::var_os("RIPTIDE_EXE").unwrap_or_else(|| env!("CARGO_BIN_EXE_riptide").into());
    let mut browser = Command::new(exe)
        .arg("--basedir")
        .arg(&base)
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(base.join("riptide.log")).unwrap())
        .spawn()
        .unwrap();
    let socket = remote::socket_path(&base.join("data"), None);
    let start = Instant::now();
    loop {
        if let Some(status) = browser.try_wait().unwrap() {
            panic!("riptide exited early ({status}):\n{}", logs(&base));
        }
        // The socket answers once the first window exists.
        if let Ok(Some(state)) = request(&socket, TestRequest::State)
            && state["windows"].as_array().is_some_and(|w| !w.is_empty())
        {
            break;
        }
        if start.elapsed() > TIMEOUT {
            let _ = browser.kill();
            panic!("no window after {TIMEOUT:?}:\n{}", logs(&base));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // riptide's own renderer code gives its UI pages `rt.send`; on macOS it runs in riptide-helper.
    let start = Instant::now();
    loop {
        let reply = request(
            &socket,
            TestRequest::EvalBar {
                code: "typeof rt === 'object' ? typeof rt.send : 'none'".into(),
                bar: "statusbar".into(),
            },
        );
        if let Ok(Some(serde_json::Value::String(kind))) = &reply
            && kind == "function"
        {
            break;
        }
        if start.elapsed() > TIMEOUT {
            let _ = browser.kill();
            panic!(
                "the status bar has no rt.send ({reply:?}):\n{}",
                logs(&base)
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    request(
        &socket,
        TestRequest::Run {
            command: "quit".into(),
        },
    )
    .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = browser.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = browser.kill();
            panic!(":quit didn't exit:\n{}", logs(&base));
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    assert!(
        status.success(),
        "riptide exited with {status}:\n{}",
        logs(&base)
    );
    let _ = std::fs::remove_dir_all(&base);
}
