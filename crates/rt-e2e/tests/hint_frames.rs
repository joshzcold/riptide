//! Hints inside cross-origin iframes and open shadow roots.
#![cfg(unix)]

use rt_e2e::{Browser, State};

/// Hint mode with the elements `ready` wants; tries again while frames load.
fn hints_once(b: &Browser, ready: impl Fn(&State) -> bool) -> State {
    let start = std::time::Instant::now();
    loop {
        b.run("hint");
        let s = b.wait_until("hints are shown", |s| {
            s.mode == "hint" && !s.hints.is_empty()
        });
        if ready(&s) {
            return s;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "hints never covered the frame: {:#?}",
            s.hints
        );
        b.keys("<Escape>");
        b.wait_mode("normal");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_reach_into_cross_origin_iframes_but_not_hidden_ones() {
    let b = Browser::start("xframe.html");
    let start = std::time::Instant::now();
    loop {
        let s = hints_once(&b, |s| s.hints.iter().any(|h| h.text == "inner"));
        let texts: Vec<&str> = s.hints.iter().map(|h| h.text.as_str()).collect();
        assert!(texts.contains(&"outer"), "{texts:?}");
        assert_eq!(
            texts.iter().filter(|t| **t == "inner").count(),
            1,
            "the hidden iframe's button got a hint too: {texts:?}"
        );
        let label = s
            .hints
            .iter()
            .find(|h| h.text == "inner")
            .unwrap()
            .label
            .clone();
        b.keys(&label);
        // A real click, at the button's place in the top page. Under load the
        // frame's own renderer can miss the first one, as a page that hasn't
        // painted does.
        let clicked = std::time::Instant::now();
        while clicked.elapsed() < std::time::Duration::from_secs(3) {
            if b.state().tab().title == "inner clicked true" {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT * 2,
            "the iframe's button was never clicked"
        );
        b.wait_mode("normal");
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_reach_into_open_shadow_roots() {
    let b = Browser::start("shadow.html");
    b.follow_hint("hint", |h| h.text == "shadowed");
    b.wait_until("the shadowed button is clicked", |s| {
        s.tab().title == "shadow clicked true"
    });
}
