//! How tabs look in the tab bar: widths, title alignment and the indicator.
#![cfg(unix)]

use rt_e2e::Browser;

/// Poll a value in the tab bar until it is `expected`.
fn wait_bar(b: &Browser, code: &str, expected: &str) {
    let start = std::time::Instant::now();
    loop {
        let got = b.eval_bar("tabbar", code);
        if got == expected {
            return;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{code} is {got:?}, not {expected:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

const FIRST_WIDTH: &str = "String(document.querySelector('.tab').getBoundingClientRect().width)";

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_max_and_min_width_size_the_tabs() {
    let b = Browser::start("nav1.html");
    b.run("set tabs.max_width 150");
    wait_bar(&b, FIRST_WIDTH, "150");
    b.run("set tabs.max_width -1");
    b.run("set tabs.min_width 400");
    for _ in 0..4 {
        b.run("open -t nav2.html");
    }
    b.wait_until("five tabs", |s| s.tabs().len() == 5);
    wait_bar(&b, FIRST_WIDTH, "400");
    // The tabs don't fit, so the bar scrolls to show the current one.
    wait_bar(
        &b,
        "String(document.querySelector('#tabs').scrollLeft > 0)",
        "true",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_title_alignment_and_indicator_width() {
    let b = Browser::start("nav1.html");
    b.run("set tabs.title.alignment center");
    wait_bar(
        &b,
        "getComputedStyle(document.querySelector('.title')).textAlign",
        "center",
    );
    b.run("set tabs.indicator.width 7");
    wait_bar(
        &b,
        "String(document.querySelector('.indicator').getBoundingClientRect().width)",
        "7",
    );
}
