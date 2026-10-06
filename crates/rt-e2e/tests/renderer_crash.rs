//! A tab whose renderer process dies shows a notice in its place until it reloads.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_crashed_tab_shows_a_notice_and_r_reloads_it() {
    let b = Browser::start("page.html");
    let url = b.url("page.html");
    // A debug URL: Chromium crashes the renderer it would load in.
    b.run("open chrome://crash");
    let state = b.wait_until("the tab is crashed", |s| {
        s.tab().crashed && s.window().crash_notice
    });
    assert_eq!(state.tab().url, url, "the tab keeps its page");
    let message = state.message().unwrap_or_default();
    assert!(message.contains("This tab crashed"), "{message}");
    b.wait_until("the notice names the page", |_| {
        b.eval_bar("crash_notice", "document.body.innerText")
            .contains(&url)
    });

    b.keys("r");
    b.wait_until("the page is back", |s| {
        !s.tab().crashed && !s.window().crash_notice && s.tab().is_loaded(&url)
    });
    b.wait_eval("location.href", &url);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_notice_follows_the_crashed_tab() {
    let b = Browser::start("page.html");
    let url = b.url("page.html");
    b.run("open -b chrome://crash");
    let state = b.wait_until("the background tab is crashed", |s| {
        s.tabs().len() == 2 && s.tabs()[1].crashed
    });
    assert!(
        !state.window().crash_notice,
        "only the current tab's crash is shown"
    );

    b.keys("J");
    b.wait_until("the notice shows", |s| {
        s.window().current_tab == 1 && s.window().crash_notice
    });
    b.keys("K");
    b.wait_until("the page shows again", |s| {
        s.window().current_tab == 0 && !s.window().crash_notice
    });
    b.wait_eval("location.href", &url);
}
