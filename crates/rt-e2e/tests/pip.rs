//! `:pip` (`gp`) floats the page's video in a picture-in-picture window.
#![cfg(unix)]

use std::process::Command;

use rt_e2e::Browser;

/// Whether the display has Chromium's picture-in-picture window.
fn pip_window(b: &Browser) -> bool {
    let out = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Picture in picture$"])
        .env("DISPLAY", format!(":{}", b.display()))
        .output()
        .expect("xdotool is needed");
    !String::from_utf8_lossy(&out.stdout).trim().is_empty()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gp_floats_the_video_and_brings_it_back() {
    let b = Browser::start("pip.html");
    b.keys("gp");
    b.wait_until("the video floats", |s| {
        s.message()
            .is_some_and(|m| m.contains("Playing in picture-in-picture"))
    });
    b.wait_until("Chromium's floating window is up", |_| pip_window(&b));
    b.keys("gp");
    b.wait_until("it comes back", |s| {
        s.message()
            .is_some_and(|m| m.contains("Picture-in-picture closed"))
    });
    b.wait_until("the floating window is gone", |_| !pip_window(&b));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gp_says_when_there_is_no_video() {
    let b = Browser::start("page.html");
    b.keys("gp");
    b.wait_until("it says so", |s| {
        s.message().is_some_and(|m| m.contains("no video playing"))
    });
}
