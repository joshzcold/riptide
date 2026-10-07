//! Input settings: forgetting half-typed keys, and a mode per site.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_partial_timeout_forgets_a_half_typed_chain() {
    let b = Browser::launch()
        .toml("\"input.partial_timeout\" = 300\n")
        .start("page.html");
    b.keys("g");
    b.wait_until("g is waiting", |s| s.status["keystring"] == "g");
    b.wait_until("it's forgotten", |s| s.status["keystring"] == "");
    assert_eq!(b.state().mode, "normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_mode_override_enters_the_sites_mode_on_load() {
    let b = Browser::start("page.html");
    let site = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set -u {site} input.mode_override passthrough"));
    b.open("second.html");
    b.wait_mode("passthrough");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_mouse_rocker_gestures_go_back_and_drop_the_context_menu() {
    let b = Browser::launch()
        .toml("\"input.mouse.rocker_gestures\" = true\n")
        .start("page.html");
    b.open("second.html");
    let menu = b.eval(
        "String(!document.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })))",
    );
    assert_eq!(menu, "true", "the context menu is cancelled");
    // Right button held, then the left: back.
    b.eval(
        "for (const button of [2, 0]) document.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button })); ''",
    );
    let first = b.url("page.html");
    b.wait_until("back on the first page", |s| s.tab().is_loaded(&first));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn mouse_back_and_forward_buttons_navigate() {
    let b = Browser::start("page.html");
    b.open("second.html");
    let (first, second) = (b.url("page.html"), b.url("second.html"));
    // Real X button presses over the page: buttons 8 and 9 are back and forward.
    let click = |button: &str| {
        let status = std::process::Command::new("xdotool")
            .args(["mousemove", "400", "300", "click", button])
            .env("DISPLAY", format!(":{}", b.display()))
            .status()
            .expect("xdotool");
        assert!(status.success());
    };
    click("8");
    b.wait_until("back on the first page", |s| s.tab().is_loaded(&first));
    click("9");
    b.wait_until("forward again", |s| s.tab().is_loaded(&second));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn middle_and_ctrl_clicks_open_links_in_background_tabs() {
    let b = Browser::start("biglink.html");
    let page = b.url("biglink.html");
    let second = b.url("second.html");
    let xdotool = |args: &[&str]| {
        let status = std::process::Command::new("xdotool")
            .args(args)
            .env("DISPLAY", format!(":{}", b.display()))
            .status()
            .expect("xdotool");
        assert!(status.success());
    };
    // The link fills the page.
    xdotool(&["mousemove", "400", "300", "click", "2"]);
    let s = b.wait_until("a background tab", |s| s.tabs().len() == 2);
    assert!(s.tab().is_loaded(&page), "the page itself navigated");
    b.wait_until("the new tab loads the link", |s| {
        s.tabs().iter().any(|t| t.url == second)
    });
    xdotool(&[
        "mousemove",
        "400",
        "300",
        "keydown",
        "ctrl",
        "click",
        "1",
        "keyup",
        "ctrl",
    ]);
    let s = b.wait_until("another background tab", |s| s.tabs().len() == 3);
    assert!(s.tab().is_loaded(&page), "the page itself navigated");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ctrl_scroll_over_the_bars_does_not_zoom_them() {
    let b = Browser::start("page.html");
    let zoom = |b: &Browser| b.eval_bar("statusbar", "String(devicePixelRatio)");
    assert_eq!(zoom(&b), "1");
    // Ctrl+scroll up over the status bar, at the bottom of the window.
    let height: i32 = b.eval("String(outerHeight)").parse().unwrap_or(800);
    let y = (height - 8).to_string();
    let status = std::process::Command::new("xdotool")
        .args([
            "mousemove",
            "300",
            &y,
            "keydown",
            "ctrl",
            "click",
            "4",
            "click",
            "4",
            "keyup",
            "ctrl",
        ])
        .env("DISPLAY", format!(":{}", b.display()))
        .status()
        .expect("xdotool");
    assert!(status.success());
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(zoom(&b), "1", "the status bar zoomed");
    assert_eq!(b.eval_bar("tabbar", "String(devicePixelRatio)"), "1");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_spatial_navigation_moves_focus_with_arrows() {
    let b = Browser::launch()
        .toml("\"input.spatial_navigation\" = true\n")
        .start("links.html");
    b.keys("<Ctrl-v>");
    b.wait_mode("passthrough");
    b.keys("<Right>");
    b.wait_until("a link has focus", |_| {
        b.try_eval("document.activeElement.tagName")
            .is_ok_and(|t| t == "A")
    });
}
