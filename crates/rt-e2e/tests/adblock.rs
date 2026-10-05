//! Content blocking with an Adblock Plus filter list (pages/filters.txt).
#![cfg(unix)]

use rt_e2e::Browser;

/// Start with the test filter list compiled and loaded.
fn with_filters() -> Browser {
    let b = Browser::launch()
        .toml("content.blocking.adblock.lists = [\"file://{pages}/filters.txt\"]\n")
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
