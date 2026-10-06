//! Settings that change how hints are labelled, and the links ]] follows.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_min_chars_lengthens_every_label() {
    let b = Browser::launch()
        .toml("\"hints.min_chars\" = 2")
        .start("links.html");
    b.keys("f");
    let s = b.wait_until("hints are shown", |s| {
        s.mode == "hint" && !s.hints.is_empty()
    });
    assert!(
        s.hints.iter().all(|h| h.label.chars().count() == 2),
        "{:?}",
        s.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
    b.keys("<Escape>");
    b.wait_mode("normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_next_regexes_choose_the_next_link() {
    let b = Browser::launch()
        .toml("\"hints.next_regexes\" = [\"\\\\bweiter\\\\b\"]")
        .start("weiter.html");
    let next = b.url("nav2.html");
    b.keys("]]");
    b.wait_until("the Weiter link is followed", |s| s.tab().url == next);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_mode_word_labels_links_with_their_own_words() {
    let b = Browser::launch()
        .toml("\"hints.mode\" = \"word\"\n\"hints.dictionary\" = \"{scratch}/words\"\n")
        .start("links.html");
    std::fs::write(
        b.scratch().join("words"),
        "home\nnews\nabout\nsecond\nzap\n",
    )
    .unwrap();
    b.keys("f");
    let s = b.wait_until("hints are shown", |s| {
        s.mode == "hint" && !s.hints.is_empty()
    });
    let labels: Vec<&str> = s.hints.iter().map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["home", "news", "about", "second"]);
    b.keys("news");
    b.wait_until("the News link is followed", |s| {
        s.tab().title == "clicked news"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_radius_padding_and_hints_css_shape_the_labels() {
    let b = Browser::launch()
        .toml("\"hints.radius\" = 0\n\"hints.padding\" = \"2px 6px\"\n")
        .file(
            "config/hints.css",
            ".label { color: rgb(1, 2, 3) !important; }",
        )
        .start("links.html");
    // The labels' shadow root is closed; open it so the test can look inside.
    b.eval(
        "(() => { const attach = Element.prototype.attachShadow; \
         Element.prototype.attachShadow = function (o) { return attach.call(this, { ...o, mode: 'open' }); }; \
         return ''; })()",
    );
    b.keys("f");
    b.wait_until("hints are shown", |s| {
        s.mode == "hint" && !s.hints.is_empty()
    });
    let style = b.eval(
        "(s => [s.borderTopLeftRadius, s.padding, s.color].join(' | '))\
         (getComputedStyle(document.querySelector('rt-hints').shadowRoot.querySelector('.label')))",
    );
    assert_eq!(style, "0px | 2px 6px | rgb(1, 2, 3)");
    b.keys("<Escape>");
    b.run("set hints.padding 1em");
    b.wait_until("a bad padding is refused", |s| {
        s.message().is_some_and(|m| m.contains("padding"))
    });
}
