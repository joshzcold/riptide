//! Window settings that reach the window manager (read back with xprop),
//! and what the first start after an upgrade shows.
#![cfg(unix)]

use std::process::Command;

use rt_e2e::Browser;

/// The decorations field of `_MOTIF_WM_HINTS` on the browser window:
/// 1 with a title bar and borders, 0 without.
fn decorations(b: &Browser) -> String {
    let display = format!(":{}", b.display());
    let tree = Command::new("xwininfo")
        .args(["-display", &display, "-root", "-tree"])
        .output()
        .expect("xwininfo is needed for window tests");
    let tree = String::from_utf8_lossy(&tree.stdout).to_string();
    let id = tree
        .lines()
        .find(|l| l.contains("Riptide\":"))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_else(|| panic!("no Riptide window in\n{tree}"))
        .to_string();
    let hints = Command::new("xprop")
        .args(["-display", &display, "-id", &id, "_MOTIF_WM_HINTS"])
        .output()
        .expect("xprop is needed for window tests");
    let hints = String::from_utf8_lossy(&hints.stdout).to_string();
    // "_MOTIF_WM_HINTS(_MOTIF_WM_HINTS) = flags, functions, decorations, …"
    hints
        .split_once('=')
        .and_then(|(_, values)| values.split(',').nth(2))
        .unwrap_or_else(|| panic!("unexpected hints: {hints}"))
        .trim()
        .to_string()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn window_hide_decoration_asks_for_no_title_bar() {
    let b = Browser::start("page.html");
    assert_eq!(decorations(&b), "0x1");
    let b = Browser::launch()
        .toml("window.hide_decoration = true\n")
        .start("page.html");
    assert_eq!(decorations(&b), "0x0");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn changelog_after_upgrade_opens_the_changelog_once() {
    let changelog = |s: &rt_e2e::State| s.tabs().iter().any(|t| t.url == "riptide://changelog/");
    let b = Browser::start("page.html");
    assert!(!changelog(&b.state()), "the first start isn't an upgrade");
    let restart = |b: &Browser| {
        b.run("q");
        assert!(b.wait_exit().success());
        b.restart();
        b.wait_until("the start page", |s| !s.tabs().is_empty());
    };
    // As if the previous start was an older release.
    std::fs::write(b.data_dir().join("last-version"), "0.0.1").unwrap();
    restart(&b);
    b.wait_until("the changelog opens", changelog);
    restart(&b);
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!changelog(&b.state()), "the changelog opened again");
}
