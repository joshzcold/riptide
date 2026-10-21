//! A tab whose renderer process dies shows a notice in its place until it reloads.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_crashed_tab_shows_a_notice_and_r_reloads_it() {
    let b = Browser::start("page.html");
    let url = b.url("page.html");
    b.crash_tab();
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
    let first = b.url("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("the second tab loads", |s| s.tab().is_loaded(&second));
    b.crash_tab();
    b.wait_until("the second tab is crashed", |s| {
        s.tabs()[1].crashed && s.window().crash_notice
    });

    b.keys("K");
    b.wait_until("the first tab shows", |s| {
        s.window().current_tab == 0 && !s.window().crash_notice
    });
    b.wait_eval("location.href", &first);
    b.keys("J");
    b.wait_until("the notice shows again", |s| {
        s.window().current_tab == 1 && s.window().crash_notice && s.tab().crashed
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_crashed_tab_in_another_split_keeps_its_notice() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("vsplit {second}"));
    b.wait_until("the right pane loads", |s| {
        s.window().panes.len() == 2 && s.tab().is_loaded(&second)
    });
    b.crash_tab();
    b.wait_until("the right pane's tab is crashed", |s| {
        s.tab().crashed && s.window().crash_notice
    });
    b.keys("<Ctrl-h>");
    b.wait_until("the right pane keeps a notice of its own", |s| {
        s.window().current_tab == 0
            && !s.window().crash_notice
            && s.window().pane_crash_notices == 1
    });
    b.keys("<Ctrl-l>");
    b.wait_until("focused again, it has the usual notice", |s| {
        s.tab().crashed && s.window().crash_notice && s.window().pane_crash_notices == 0
    });
    b.keys("r");
    b.wait_until("the page is back", |s| {
        !s.tab().crashed && !s.window().crash_notice && s.tab().is_loaded(&second)
    });
}
