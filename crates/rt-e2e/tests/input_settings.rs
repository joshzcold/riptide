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
