//! Windows and private windows.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_w_opens_a_window_and_close_closes_it() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -w {second}"));
    let s = b.wait_until("a second window", |s| {
        s.windows.len() == 2 && s.current_window == 1 && s.tab().url == second
    });
    assert!(!s.window().private);
    // Keys go to the new window.
    b.run("set messages.timeout 0");
    b.keys(":close<Return>");
    b.wait_until("one window", |s| s.windows.len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_p_opens_a_private_window() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -p {second}"));
    let s = b.wait_until("a private window", |s| {
        s.windows.len() == 2 && s.window().private && s.tab().url == second
    });
    assert!(!s.windows[0].private);
}
