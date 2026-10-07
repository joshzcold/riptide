//! Moving around a page: marks, macros, search, links between pages and zoom.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn marks_remember_scroll_positions() {
    let b = Browser::start("page.html");
    b.keys("5j");
    b.wait_eval("String(scrollY)", "200");
    b.keys("`a");
    b.keys("G");
    b.wait_eval("String(scrollY > 3000)", "true");
    b.keys("'a");
    b.wait_eval("String(scrollY)", "200");
    // '' goes back to where the last jump started.
    b.keys("''");
    b.wait_eval("String(scrollY > 3000)", "true");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_macro_replays_its_keys() {
    let b = Browser::start("page.html");
    b.keys("qajjq");
    b.wait_eval("String(scrollY)", "80");
    b.keys("@a");
    b.wait_eval("String(scrollY)", "160");
    b.keys("2@a");
    b.wait_eval("String(scrollY)", "320");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn search_finds_text_and_n_goes_to_the_next_match() {
    let b = Browser::start("search.html");
    b.keys("/needle<Return>");
    b.wait_eval("String(scrollY > 1000 && scrollY < 3000)", "true");
    b.keys("n");
    b.wait_eval("String(scrollY > 3000)", "true");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn brackets_follow_next_and_prev_links_and_ctrl_a_increments() {
    let b = Browser::start("nav1.html");
    b.keys("]]");
    b.wait_until("nav2 loads", |s| s.tab().title == "nav2");
    b.keys("[[");
    b.wait_until("nav1 loads", |s| s.tab().title == "nav1");
    b.keys("<Ctrl-a>");
    b.wait_until("the URL's number went up", |s| {
        s.tab().url.ends_with("/nav2.html")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plus_zooms_in_dot_repeats_and_equals_resets() {
    let b = Browser::start("nav1.html");
    b.keys("+");
    b.wait_until("zoomed in", |s| s.tab().zoom > 100);
    b.keys(".");
    b.wait_until("zoomed in again", |s| s.tab().zoom == 125);
    b.wait_eval("String(Math.round(devicePixelRatio * 100))", "125");
    b.keys("=");
    b.wait_until("zoom reset", |s| s.tab().zoom == 100);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_macro_types_the_keys_it_recorded_into_the_page() {
    let b = Browser::start("editor.html");
    b.follow_hint("hint inputs", |_| true);
    b.wait_mode("insert");
    b.keys("<Escape>");
    b.wait_mode("normal");
    b.keys("qzixy<Escape>q");
    b.wait_eval("document.title", "v=xy");
    b.keys("@z");
    b.wait_eval("document.title", "v=xyxy");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn search_wrap_false_keeps_n_at_the_last_match() {
    let b = Browser::launch()
        .toml("search.wrap = false\n")
        .start("search.html");
    b.keys("/needle<Return>");
    b.wait_eval("String(scrollY > 1000 && scrollY < 3000)", "true");
    b.keys("n");
    b.wait_eval("String(scrollY > 3000)", "true");
    let last = b.eval("String(Math.round(scrollY))");
    b.keys("n");
    std::thread::sleep(std::time::Duration::from_millis(800));
    assert_eq!(b.eval("String(Math.round(scrollY))"), last);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn return_follows_the_link_a_search_found() {
    let b = Browser::start("follow.html");
    b.keys("/target<Return>");
    // Return follows the link the match is in, once Chromium has found it.
    b.wait_until("the search found the link", |s| {
        s.mode == "normal" && s.tab().search_match.is_some_and(|(_, count)| count > 0)
    });
    b.keys("<Return>");
    b.wait_until("the link is followed", |s| s.tab().title == "nav2");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn preferred_color_scheme_applies_live() {
    let b = Browser::start("scheme.html");
    b.run("set colors.webpage.preferred_color_scheme dark");
    b.wait_eval("document.title", "dark=true");
    b.run("set colors.webpage.preferred_color_scheme light");
    b.wait_eval("document.title", "dark=false");
}
