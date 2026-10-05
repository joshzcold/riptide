//! Where the tab bar and status bar go, measured from the page's size.
#![cfg(unix)]

use rt_e2e::Browser;

/// The page's `innerWidth` and `innerHeight` once they settle on new values.
fn size_after(b: &Browser, command: &str, before: (i64, i64)) -> (i64, i64) {
    b.run(command);
    let start = std::time::Instant::now();
    loop {
        let size = size(b);
        if size != before {
            // Let the layout finish moving before reading the final size.
            std::thread::sleep(std::time::Duration::from_millis(300));
            return self::size(b);
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{command} didn't change the page size"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn size(b: &Browser) -> (i64, i64) {
    let text = b.eval("innerWidth + 'x' + innerHeight");
    let (w, h) = text.split_once('x').unwrap();
    (w.parse().unwrap(), h.parse().unwrap())
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_position_left_moves_the_tab_bar_beside_the_page() {
    let b = Browser::start("nav1.html");
    let (w, h) = size(&b);
    let left = size_after(&b, "set tabs.position left", (w, h));
    // tabs.width (200) comes off the width; the bar's height goes back to the page.
    assert_eq!(left.0, w - 200, "{left:?} from {w}x{h}");
    assert!(left.1 > h, "{left:?} from {w}x{h}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_show_and_statusbar_show_never_give_the_page_their_space() {
    let b = Browser::start("nav1.html");
    let (w, h) = size(&b);
    let no_tabs = size_after(&b, "set tabs.show never", (w, h));
    assert_eq!(no_tabs.0, w);
    let tab_bar = no_tabs.1 - h;
    assert!(tab_bar > 0, "hiding the tab bar didn't grow the page");
    let neither = size_after(&b, "set statusbar.show never", no_tabs);
    assert!(
        neither.1 > no_tabs.1,
        "hiding the status bar didn't grow the page"
    );
    // Typing a command still shows the status bar.
    b.keys(":");
    b.wait_mode("command");
    b.keys("<Escape>");
    b.wait_mode("normal");
}
