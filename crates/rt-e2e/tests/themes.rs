//! Themes: `:theme` and `colors.*` reach the bars' pages.
#![cfg(unix)]

use rt_e2e::Browser;

fn statusbar_bg(b: &Browser) -> String {
    b.eval_bar(
        "statusbar",
        "getComputedStyle(document.body).backgroundColor",
    )
}

fn wait_bg(b: &Browser, expected: &str) {
    let start = std::time::Instant::now();
    while statusbar_bg(b) != expected {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the status bar is {}, not {expected}",
            statusbar_bg(b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn theme_and_color_settings_restyle_the_bars() {
    let b = Browser::start("page.html");
    // riptide's own #061826.
    wait_bg(&b, "rgb(6, 24, 38)");
    b.run("theme nord");
    wait_bg(&b, "rgb(36, 41, 51)");
    b.run("set colors.statusbar.normal.bg #102030");
    wait_bg(&b, "rgb(16, 32, 48)");
    b.run("set colors.statusbar.normal.bg nonsense;");
    b.wait_until("a bad color is refused", |s| {
        s.message().is_some_and(|m| m.contains("isn't a color"))
    });
}
