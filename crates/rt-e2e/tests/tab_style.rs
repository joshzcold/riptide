//! How the tab bar marks the current tab.
#![cfg(unix)]

use rt_e2e::Browser;

/// The current tab's background and the color of its marker line.
fn current_tab_colors(b: &Browser) -> (String, String) {
    let text = b.eval_bar(
        "tabbar",
        "(() => { const s = getComputedStyle(document.querySelector('.tab.selected')); \
         return s.backgroundColor + '|' + s.boxShadow; })()",
    );
    let (bg, shadow) = text.split_once('|').unwrap_or_default();
    // e.g. "rgb(4, 18, 28) 0px -2px 0px 0px inset"
    let line = shadow.split(')').next().unwrap_or_default().to_string() + ")";
    (bg.to_string(), line)
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_current_tab_has_no_marker_line_unless_a_color_is_set() {
    let b = Browser::start("page.html");
    let (bg, line) = current_tab_colors(&b);
    assert_eq!(line, bg, "by default the line matches the tab");

    b.run("set colors.tabs.selected.accent #ff0000");
    let start = std::time::Instant::now();
    while current_tab_colors(&b).1 != "rgb(255, 0, 0)" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the accent color wasn't applied"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    b.run("config-unset colors.tabs.selected.accent");
    while current_tab_colors(&b).1 != bg {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            ":config-unset didn't hide the line"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
