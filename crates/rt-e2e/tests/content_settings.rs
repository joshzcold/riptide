//! Content settings that map onto Chromium's: images and popups.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_block_images_per_site() {
    let b = Browser::start("images.html");
    b.wait_until("the image loads", |s| s.tab().title == "image=8");
    let site = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set -u {site} content.images false"));
    b.run("reload");
    b.wait_until("the image is blocked", |s| s.tab().title == "image=0");
    b.run(&format!("set -u {site} content.images true"));
    b.run("reload");
    b.wait_until("the image loads again", |s| s.tab().title == "image=8");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_popups_need_can_open_tabs_automatically() {
    let b = Browser::start("popup.html");
    b.wait_until("the popup is blocked", |s| s.tab().title == "popup=blocked");
    assert_eq!(b.state().tabs().len(), 1);
    b.run("set content.javascript.can_open_tabs_automatically true");
    b.run("reload");
    b.wait_until("the popup opens", |s| s.tabs().len() == 2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_leave_private_windows_working() {
    let b = Browser::launch()
        .toml(
            "content.images = false\ncontent.mute = true\n\
             content.javascript.can_open_tabs_automatically = true\n\
             content.javascript.clipboard = \"access-paste\"\n",
        )
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -p {second}"));
    b.wait_until("the private window loads", |s| {
        s.window().private && s.tab().is_loaded(&second)
    });
}

/// The request headers the fixture server saw for `/headers`, lowercased.
fn headers_seen(b: &Browser) -> String {
    b.wait_until("the headers page", |s| {
        s.tab().url.ends_with("/headers") && !s.tab().loading
    });
    b.eval("document.body.innerText")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_headers_language_dnt_and_custom() {
    let b = Browser::launch()
        .toml(
            "\"content.headers.accept_language\" = \"de-DE,de;q=0.9\"\n\
             [\"content.headers.custom\"]\nX-Riptide = \"yes\"\n",
        )
        .start("page.html");
    b.open("headers");
    let seen = headers_seen(&b);
    assert!(seen.contains("accept-language: de-de,de;q=0.9"), "{seen}");
    assert!(seen.contains("x-riptide: yes"), "{seen}");
    assert!(seen.contains("dnt: 1"), "{seen}");
    assert_eq!(b.eval("navigator.languages[0]"), "de-DE");
    b.run("set content.headers.do_not_track false");
    b.run("reload");
    b.wait_until("reloaded without DNT", |s| !s.tab().loading);
    let seen = headers_seen(&b);
    assert!(!seen.contains("dnt:"), "{seen}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_headers_referer_same_domain_drops_cross_site_referrers() {
    let b = Browser::start("page.html");
    let go = |b: &Browser, url: String| {
        b.eval(&format!("location.href = {url:?}; ''"));
        headers_seen(b)
    };
    let same = b.url("headers");
    let other = same.replace("127.0.0.1", "localhost");
    let seen = go(&b, same.clone());
    assert!(
        seen.contains("referer: http://127.0.0.1"),
        "same host keeps it: {seen}"
    );
    b.open("page.html");
    let seen = go(&b, other.clone());
    assert!(
        !seen.contains("referer:"),
        "another site doesn't get it: {seen}"
    );
    b.run("set content.headers.referer always");
    b.open("page.html");
    let seen = go(&b, other);
    assert!(
        seen.contains("referer: http://127.0.0.1"),
        "always sends it: {seen}"
    );
}
