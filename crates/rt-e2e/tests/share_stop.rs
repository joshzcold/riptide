//! A tab sharing a screen, window or tab is marked, and `:share-stop` ends
//! the share from any tab.
#![cfg(unix)]

use rt_e2e::Browser;

fn sharing() -> Browser {
    let b = Browser::launch()
        .toml("content.desktop_capture = \"true\"\nmessages.timeout = 0\n")
        .start("share.html");
    b.follow_hint("hint", |h| h.text.contains("share screen"));
    // Alloy's whole-screen share doesn't say what it shares; riptide takes it as the screen.
    b.wait_eval(
        "String(['monitor', 'unknown'].includes(window.__share))",
        "true",
    );
    let s = b.wait_until("the tab is marked as sharing", |s| {
        s.tab().sharing.as_deref() == Some("monitor")
    });
    let message = s.message().unwrap_or_default();
    assert!(
        message.contains("Sharing your screen with") && message.contains(":share-stop"),
        "{message}"
    );
    b
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn share_stop_ends_the_share_from_another_tab() {
    let b = sharing();
    b.run("open -t about:blank");
    b.wait_until("the new tab shows", |s| {
        s.tabs().len() == 2 && s.window().current_tab == 1
    });
    b.run("share-stop");
    b.wait_until("the mark goes", |s| s.tabs()[0].sharing.is_none());
    b.run("tab-prev");
    b.wait_until("back on the call", |s| s.window().current_tab == 0);
    // The page hears about it as if the user had stopped it in Chrome.
    b.wait_eval("String(window.__share)", "ended");
    b.run("share-stop");
    b.wait_until("nothing left to stop", |s| {
        s.message()
            .is_some_and(|m| m.contains("Nothing is being shared"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_mark_goes_when_the_page_stops_sharing() {
    let b = sharing();
    b.follow_hint("hint", |h| h.text.contains("stop from the page"));
    b.wait_until("the mark goes", |s| s.tab().sharing.is_none());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_call_window_share_is_marked_and_stopped() {
    let b = Browser::launch()
        .arg("--auto-select-desktop-capture-source=Entire screen")
        .toml("messages.timeout = 0\n")
        .start("page.html");
    let share = b.url("share.html");
    b.run(&format!("open --call {share}"));
    b.wait_until("the call window shows the page", |s| {
        s.windows.len() == 2 && s.window().call && s.tab().is_loaded(&share)
    });
    b.wait_painted();
    b.follow_hint("hint", |h| h.text.contains("share screen"));
    b.wait_eval("String(window.__share)", "monitor");
    b.wait_until("the tab is marked as sharing", |s| {
        s.tab().sharing.as_deref() == Some("monitor")
    });
    b.run("share-stop");
    b.wait_eval("String(window.__share)", "ended");
    b.wait_until("the mark goes", |s| s.tab().sharing.is_none());
}
