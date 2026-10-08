//! Content blocking with an Adblock Plus filter list (pages/filters.txt).
#![cfg(unix)]

use rt_e2e::Browser;

/// Start with the test filter list compiled and loaded.
fn with_filters() -> Browser {
    with_lists(&["filters.txt"])
}

fn with_lists(lists: &[&str]) -> Browser {
    let lists: Vec<String> = lists
        .iter()
        .map(|l| format!("\"file://{{pages}}/{l}\""))
        .collect();
    let b = Browser::launch()
        .toml(&format!(
            "content.blocking.adblock.lists = [{}]\n",
            lists.join(", ")
        ))
        .start("page.html");
    b.run("adblock-update");
    let engine = b.data_dir().join("adblock/engine.dat");
    let start = std::time::Instant::now();
    while std::fs::metadata(&engine).map_or(0, |m| m.len()) == 0 {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "no {}", engine.display());
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // Loading the new engine happens in the background after it's written.
    std::thread::sleep(std::time::Duration::from_millis(500));
    b
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn adblock_update_blocks_requests_from_the_filter_list() {
    let b = with_filters();
    b.open("adblock.html");
    b.wait_eval("document.title", "ads b=no a=yes");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn element_hiding_rules_hide_ads_including_late_ones() {
    let b = with_filters();
    b.open("cosmetic.html");
    b.wait_eval(
        "document.title",
        "banner=none local=none content=block late=none",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn nothing_is_blocked_without_filter_lists() {
    let b = Browser::start("adblock.html");
    b.wait_eval("document.title", "ads b=yes a=yes");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_status_bar_counts_blocked_requests_per_page() {
    let b = with_filters();
    b.open("adblock.html");
    b.wait_eval("document.title", "ads b=no a=yes");
    let bar = |b: &Browser| b.eval_bar("statusbar", "document.body.innerText");
    let start = std::time::Instant::now();
    while !bar(&b).contains("⊘1") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "no count in {:?}",
            bar(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // A new page starts from nothing.
    b.open("page.html");
    let start = std::time::Instant::now();
    while bar(&b).contains('⊘') {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the count stayed: {:?}",
            bar(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn scriptlets_run_first_and_redirects_stand_in() {
    let b = with_lists(&["filters-scriptlets.txt"]);
    b.open("scriptlets.html");
    b.wait_eval(
        "document.title",
        "constant=false aopr=aborted script=loaded image=loaded",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tracking_parameters_come_off_page_addresses() {
    let b = with_lists(&["filters-scriptlets.txt"]);
    let url = b.url("page.html");
    b.run(&format!("open {url}?utm_source=mail&id=7"));
    let clean = format!("{url}?id=7");
    b.wait_until("the page loads without utm_source", |s| {
        s.tab().is_loaded(&clean)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn procedural_filters_hide_remove_and_restyle_even_late_content() {
    let b = with_lists(&["filters-procedural.txt"]);
    b.open("procedural.html");
    b.wait_eval(
        "window.report()",
        &[
            "sponsored=true promoted=true news=false box=true grandparent=true",
            "underlined=true plain=false attr=true banner=removed",
            "restyle=rgb(0, 128, 0) tracked=false classy=classy",
            "long=true brief=false late=true",
        ]
        .join(" "),
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn element_hiding_reaches_frames_from_other_sites() {
    let b = with_lists(&["filters-frames.txt"]);
    b.open("adframe.html");
    // The frame (localhost, in a page on 127.0.0.1) reports into the title.
    b.wait_eval(
        "document.title",
        "site=true generic=true sponsored=true news=false",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn your_own_rules_apply_without_downloading_lists() {
    let b = Browser::launch()
        .toml(
            "content.blocking.adblock.lists = []\n\
             content.blocking.adblock.rules = [\"/ads/banner.js\", \"127.0.0.1##.local-ad\"]\n",
        )
        .start("adblock.html");
    b.wait_eval("document.title", "ads b=no a=yes");
    b.open("cosmetic.html");
    b.wait_eval(
        "document.title",
        "banner=block local=none content=block late=block",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_picker_hides_an_element_and_keeps_the_rule() {
    let b = Browser::launch()
        .toml("content.blocking.adblock.lists = []\n")
        .start("picker.html");
    b.wait_eval("document.title", "promo=block,block content=block");
    b.follow_hint("hint blocks hide", |h| h.text == "buy now");
    // The rule is offered for editing, then hides every match at once.
    let s = b.wait_until("the rule is offered", |s| s.prompt.is_some());
    assert_eq!(s.prompt.unwrap()["input"], "127.0.0.1##.promo-box");
    b.keys("<Return>");
    b.wait_eval("document.title", "promo=none,none content=block");
    let autoconfig = b.config_dir().join("autoconfig.toml");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&autoconfig).is_ok_and(|t| t.contains("127.0.0.1##.promo-box")) {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "the rule wasn't saved");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Later visits hide it through the engine.
    b.open("cosmetic.html");
    b.open("picker.html");
    b.wait_eval("document.title", "promo=none,none content=block");
}
