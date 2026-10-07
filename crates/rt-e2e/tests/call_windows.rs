//! Call windows (`:open --call`): their tab shares a screen, window or tab
//! through Chrome's picker instead of riptide's prompt.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_call_window_shares_through_chromes_picker() {
    // A Chromium test switch: Chrome's picker chooses this source by itself.
    // Only Chrome's picker reads it, so a share that works came through it.
    let b = Browser::launch()
        .arg("--auto-select-desktop-capture-source=Entire screen")
        .start("page.html");
    let share = b.url("share.html");
    b.run(&format!("open --call {share}"));
    let s = b.wait_until("the call window shows the page", |s| {
        s.windows.len() == 2 && s.window().call && s.tab().is_loaded(&share)
    });
    assert!(!s.windows[0].call, "only the new window is a call window");
    b.wait_painted();

    b.follow_hint("hint", |h| h.text.contains("share screen"));
    b.wait_eval("String(window.__share)", "monitor");
    assert!(b.state().prompt.is_none(), "riptide's own prompt opened");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn other_tabs_keep_riptides_prompt() {
    let b = Browser::start("share.html");
    b.follow_hint("hint", |h| h.text.contains("share screen"));
    b.wait_until("riptide asks", |s| s.prompt.is_some());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_call_tab_takes_riptides_commands() {
    let b = Browser::start("page.html");
    let search = b.url("search.html");
    b.run(&format!("open --call {search}"));
    b.wait_until("the call window shows the page", |s| {
        s.windows.len() == 2 && s.window().call && s.tab().is_loaded(&search)
    });
    b.wait_painted();
    b.keys("+");
    b.wait_until("zoomed in", |s| s.tab().zoom > 100);
    b.keys("/needle<Return>");
    b.wait_eval("String(scrollY > 1000)", "true");
    b.run("devtools");
    b.run("close");
    let s = b.wait_until("only the first window is left", |s| s.windows.len() == 1);
    assert!(!s.window().call);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_call_window_takes_more_tabs_and_popups() {
    let b = Browser::launch()
        .toml("content.javascript.can_open_tabs_automatically = true\n")
        .start("page.html");
    b.run(&format!("open --call {}", b.url("popup.html")));
    // CEF gives a call tab's popup the call tab's style, and a window holds
    // one such tab, so the popup gets a call window of its own.
    let page = b.url("page.html");
    let s = b.wait_until("the popup opens in another call window", |s| {
        s.windows.len() == 3
            && s.windows[1..].iter().all(|w| w.call && w.tabs.len() == 1)
            && s.windows.iter().any(|w| w.tabs[0].url == page && w.call)
    });
    let call = s
        .windows
        .iter()
        .position(|w| w.tabs[0].title == "popup=open")
        .expect("the call tab");
    b.run(&format!("tab-select {}/1", call + 1));
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("an ordinary tab opens in the call window", |s| {
        s.window().call && s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_call_reopens_the_tab_in_a_call_window() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    b.run("tab-call");
    let s = b.wait_until("the tab moved to a call window", |s| {
        s.windows.len() == 2
            && s.windows
                .iter()
                .any(|w| w.call && w.tabs.len() == 1 && w.tabs[0].url == second)
    });
    let first = s
        .windows
        .iter()
        .find(|w| !w.call)
        .expect("the first window");
    assert_eq!(first.tabs.len(), 1, "{:?}", first.tabs);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn call_sites_open_in_a_call_window_from_open_and_from_links() {
    let b = Browser::launch()
        .toml("\"content.call_sites\" = [\"127.0.0.1/nav2.html\"]\n")
        .start("nav1.html");
    let nav1 = b.url("nav1.html");
    let nav2 = b.url("nav2.html");
    // A link in an ordinary tab: the call opens beside it, and the tab stays.
    b.follow_hint("hint", |h| h.text.contains("next"));
    let s = b.wait_until("nav2 opens in a call window", |s| {
        s.windows.len() == 2 && s.windows.iter().any(|w| w.call && w.tabs[0].url == nav2)
    });
    let first = s
        .windows
        .iter()
        .find(|w| !w.call)
        .expect("the first window");
    assert_eq!(first.tabs.len(), 1);
    assert_eq!(first.tabs[0].url, nav1);
    b.wait_until("the call window says why it opened", |s| {
        s.message().is_some_and(|m| m.contains("in a call window"))
    });
    // :open -t too, without leaving an empty tab behind.
    b.run("tab-select 1/1");
    b.run(&format!("open -t {nav2}"));
    b.wait_until("another call window", |s| {
        s.windows.iter().filter(|w| w.call).count() == 2
            && s.windows
                .iter()
                .filter(|w| !w.call)
                .all(|w| w.tabs.len() == 1)
    });
}
