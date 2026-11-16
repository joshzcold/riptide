//! Riptide on a Wayland desktop: a headless sway, with riptide on its
//! Xwayland (the default) or as a native Wayland client. The clipboard is
//! read and written by Wayland apps (`wl-paste`, `wl-copy`), and keys come
//! from the compositor's virtual keyboard (`wtype`), not the test channel.
#![cfg(unix)]

use std::process::{Child, Stdio};

use rt_e2e::{Browser, Desktop};

fn start(desktop: Desktop) -> Browser {
    Browser::launch().desktop(desktop).start("page.html")
}

/// What a Wayland app pastes.
fn wl_paste(b: &Browser) -> String {
    let out = b
        .client("wl-paste")
        .arg("--no-newline")
        .output()
        .expect("wl-paste is needed (wl-clipboard)");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A Wayland app copying `text`, until the returned process is killed.
fn wl_copy(b: &Browser, text: &str) -> Child {
    b.client("wl-copy")
        .args(["--foreground", text])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("wl-copy is needed (wl-clipboard)")
}

/// Type through the compositor, as a keyboard would, with wtype 0.5 or newer.
/// Each `wtype` adds a keyboard to the seat, so it waits for riptide to bind
/// that before typing.
fn wtype(b: &Browser, args: &[&str]) {
    let status = b
        .client("wtype")
        .args(["-s", "500"])
        .args(args)
        .status()
        .expect("wtype is needed");
    assert!(status.success(), "wtype {args:?}");
}

fn swaymsg(b: &Browser, kind: &str) -> String {
    let out = b
        .client("swaymsg")
        .args(["-t", kind])
        .output()
        .expect("swaymsg is needed (sway)");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn yank_reaches_wayland_apps(desktop: Desktop) {
    let b = start(desktop);
    let url = b.url("page.html");
    b.keys("yy");
    b.wait_until("the URL is yanked", |s| {
        s.message().is_some_and(|m| m.contains("Yanked"))
    });
    b.wait_until("a Wayland app pastes the URL", |_| wl_paste(&b) == url);
}

fn ctrl_v_pastes_what_a_wayland_app_copied(desktop: Desktop) {
    let b = start(desktop);
    let text = "https://example.com/from-wayland";
    let mut copy = wl_copy(&b, text);
    b.wait_until("wl-copy owns the clipboard", |_| wl_paste(&b) == text);
    b.keys(":open <Ctrl-v>");
    b.wait_until("the copied text is pasted", |s| {
        s.status["command_line"]["text"].as_str() == Some(&format!(":open {text}"))
    });
    let _ = copy.kill();
    let _ = copy.wait();
}

/// Only natively: `wtype` sends each run with a keymap of its own, which
/// Chromium on Xwayland doesn't pick up (`:` arrives as `1`).
#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn wayland_keyboard_input_reaches_riptide() {
    let b = start(Desktop::Wayland);
    // The compositor gives keys to the focused window; sway focuses new ones.
    wtype(&b, &[":"]);
    b.wait_mode("command");
    wtype(&b, &["open wayland"]);
    b.wait_until("typed text reaches the command line", |s| {
        s.status["command_line"]["text"].as_str() == Some(":open wayland")
    });
    wtype(&b, &["-k", "Escape"]);
    b.wait_mode("normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn xwayland_yank_reaches_wayland_apps() {
    yank_reaches_wayland_apps(Desktop::XWayland);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn wayland_yank_reaches_wayland_apps() {
    yank_reaches_wayland_apps(Desktop::Wayland);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn xwayland_ctrl_v_pastes_what_a_wayland_app_copied() {
    ctrl_v_pastes_what_a_wayland_app_copied(Desktop::XWayland);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn wayland_ctrl_v_pastes_what_a_wayland_app_copied() {
    ctrl_v_pastes_what_a_wayland_app_copied(Desktop::Wayland);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn wayland_window_has_the_riptide_app_id() {
    let b = start(Desktop::Wayland);
    b.wait_until("sway lists an xdg_shell window with app_id riptide", |_| {
        let tree = swaymsg(&b, "get_tree");
        tree.contains(r#""app_id": "riptide""#) && tree.contains(r#""shell": "xdg_shell""#)
    });
}
