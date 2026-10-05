//! Site permission prompts and the answers saved for a site.
#![cfg(unix)]

use rt_e2e::Browser;

/// Ask for the location from geo.html and wait for the prompt.
fn ask(b: &Browser) {
    b.follow_hint("hint", |h| h.text == "ask");
    let s = b.wait_mode("yesno");
    assert!(
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("location")),
        "{:?}",
        s.prompt
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn always_allow_is_saved_for_the_site() {
    let b = Browser::start("geo.html");
    ask(&b);
    b.keys("A");
    // Xvfb has no location source, so an allowed request still fails, but
    // not with PERMISSION_DENIED.
    b.wait_until("the page got an answer", |s| {
        s.tab().title.starts_with("geo=")
    });
    assert_ne!(b.eval("document.title"), "geo=denied");
    let autoconfig = b.config_dir().join("autoconfig.toml");
    let site = format!("per_domain.\"{}\"", b.url("").trim_end_matches('/'));
    let text = b.wait_file(&autoconfig);
    let after = text
        .split_once(&site)
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    assert!(
        after.contains("\"content.geolocation\" = \"true\""),
        "{text}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn always_block_is_saved_and_the_site_isnt_asked_again() {
    let b = Browser::start("geo.html");
    ask(&b);
    b.keys("N");
    b.wait_eval("document.title", "geo=denied");
    b.wait_file(&b.config_dir().join("autoconfig.toml"));
    b.run("reload");
    b.wait_until("the page reloads", |s| {
        s.tab().title == "geo" && !s.tab().loading
    });
    b.wait_painted();
    b.follow_hint("hint", |h| h.text == "ask");
    b.wait_eval("document.title", "geo=denied");
    assert_eq!(b.state().mode, "normal", "it asked again");
}
