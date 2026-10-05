//! Opening, closing, reopening and moving between tabs.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_t_opens_and_focuses_a_new_tab() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    let s = b.wait_until("the second tab loads", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    assert_eq!(s.window().current_tab, 1);
    assert_eq!(s.tab().title, "second");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn d_closes_and_u_reopens_a_tab() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    // A tab closed before its first page commits has no URL to reopen yet (PLAN.md, M26).
    b.wait_until("the second tab loads", |s| {
        s.tabs().len() == 2 && s.tab().url == second
    });
    b.keys("d");
    let s = b.wait_until("one tab", |s| s.tabs().len() == 1);
    assert_eq!(s.tab().title, "ready");
    b.keys("u");
    let s = b.wait_until("the tab is back", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    assert_eq!(s.window().current_tab, 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn j_and_k_switch_tabs() {
    let b = Browser::start("page.html");
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("the second tab is current", |s| s.window().current_tab == 1);
    b.keys("K");
    b.wait_until("the first tab is current", |s| s.window().current_tab == 0);
    b.keys("J");
    b.wait_until("the second tab is current again", |s| {
        s.window().current_tab == 1
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn u_reopens_a_tab_closed_before_its_page_loaded() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.keys("d");
    b.wait_until("one tab", |s| s.tabs().len() == 1);
    b.keys("u");
    b.wait_until("the tab is back", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
}
