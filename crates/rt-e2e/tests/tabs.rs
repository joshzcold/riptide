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

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn pinned_tabs_ask_before_d_closes_them() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("the second tab loads", |s| s.tab().is_loaded(&second));
    b.keys("<Ctrl-p>");
    b.wait_until("the tab is pinned", |s| s.tab().pinned);
    b.keys("d");
    b.wait_mode("yesno");
    b.keys("n");
    let s = b.wait_mode("normal");
    assert_eq!(s.tabs().len(), 2, "n keeps the tab");
    b.keys("d");
    b.wait_mode("yesno");
    b.keys("y");
    b.wait_until("y closes it", |s| s.tabs().len() == 1);
    b.keys("u");
    b.wait_until("u brings it back pinned", |s| {
        s.tabs().len() == 2 && s.tabs().iter().any(|t| t.url == second && t.pinned)
    });
    b.run("tab-close --force");
    b.wait_until("--force closes it without asking", |s| s.tabs().len() == 1);
    b.keys("u");
    b.wait_until("the tab is back", |s| s.tabs().len() == 2 && s.tab().pinned);
    b.run("set tabs.pinned.close refuse");
    b.keys("d");
    let s = b.wait_until("refuse says why", |s| {
        s.message().is_some_and(|m| m.contains("pinned"))
    });
    assert_eq!(s.tabs().len(), 2);
}

/// Three tabs (nav1, second, nav2) with the middle one current.
fn three_tabs(b: &Browser) {
    for page in ["second.html", "nav2.html"] {
        let url = b.url(page);
        b.run(&format!("open -t {url}"));
        b.wait_until(&format!("{page} loads"), |s| s.tab().is_loaded(&url));
    }
    b.run("tab-focus 2");
    b.wait_until("the middle tab is current", |s| s.tab().title == "second");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn select_on_remove_picks_the_tab_after_a_close() {
    for (setting, expected) in [("prev", "nav1"), ("next", "nav2")] {
        let b = Browser::launch()
            .toml(&format!("tabs.select_on_remove = \"{setting}\"\n"))
            .start("nav1.html");
        three_tabs(&b);
        b.run("tab-close");
        let s = b.wait_until("the tab closes", |s| s.tabs().len() == 2);
        assert_eq!(s.tab().title, expected, "tabs.select_on_remove = {setting}");
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_wrap_false_stops_at_the_first_tab() {
    let b = Browser::launch()
        .toml("tabs.wrap = false\n")
        .start("nav1.html");
    three_tabs(&b);
    b.run("tab-focus 1");
    b.wait_until("the first tab is current", |s| s.window().current_tab == 0);
    b.run("tab-prev");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(b.state().window().current_tab, 0);
    b.run("set tabs.wrap true");
    b.run("tab-prev");
    b.wait_until("tab-prev wraps to the last tab", |s| {
        s.window().current_tab == 2
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn capital_t_picks_a_tab_by_title() {
    let b = Browser::start("search.html");
    b.run("open -t about:blank");
    b.wait_until("about:blank is current", |s| s.tab().url == "about:blank");
    b.keys("T");
    b.wait_mode("command");
    b.keys("search");
    b.wait_until("completion offers the tab", |s| {
        s.completion.to_string().contains("search")
    });
    b.keys("<Tab><Return>");
    b.wait_until("the search tab is current", |s| s.window().current_tab == 0);
}
