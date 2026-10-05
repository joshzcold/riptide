//! Hints: labels for elements, followed with a real click.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_hint_clicks_a_button_for_real() {
    let b = Browser::start("page.html");
    b.follow_hint("hint", |h| h.text == "b");
    b.wait_eval("document.title", "clicked true");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hint_links_tab_opens_the_link_in_a_new_tab() {
    let b = Browser::start("links.html");
    let second = b.url("second.html");
    b.follow_hint("hint links tab", |h| {
        h.url.as_deref() == Some(second.as_str())
    });
    let s = b.wait_until("the link opens in a new tab", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    assert_eq!(s.window().current_tab, 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_reach_into_same_origin_iframes() {
    let b = Browser::start("frames.html");
    // The srcdoc frame can finish after the page itself.
    b.wait_eval(
        "String(!!document.querySelector('iframe').contentDocument?.querySelector('button'))",
        "true",
    );
    b.follow_hint("hint", |h| h.text == "inside");
    b.wait_eval("document.title", "inner clicked true");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn number_hints_filter_by_the_text_typed() {
    let b = Browser::launch()
        .toml("hints.mode = \"number\"\n")
        .start("links.html");
    b.run("hint links");
    b.wait_mode("hint");
    // Only "About us" contains "ou", so it's followed without a number.
    b.keys("ou");
    b.wait_eval("document.title", "clicked about");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn auto_follow_never_waits_for_return() {
    let b = Browser::launch()
        .toml("hints.auto_follow = \"never\"\n")
        .start("links.html");
    b.follow_hint("hint", |h| h.text == "news");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(
        b.eval("document.title"),
        "links",
        "it followed without Return"
    );
    b.keys("<Return>");
    b.wait_eval("document.title", "clicked news");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_selectors_adds_a_group() {
    let b = Browser::launch()
        .toml("[hints.selectors]\nnews = \"a[href='#news']\"\n")
        .start("links.html");
    let s = {
        b.run("hint news");
        b.wait_until("the group's hints", |s| {
            s.mode == "hint" && !s.hints.is_empty()
        })
    };
    assert_eq!(s.hints.len(), 1, "{:?}", s.hints);
    b.keys(&s.hints[0].label);
    b.wait_eval("document.title", "clicked news");
}
