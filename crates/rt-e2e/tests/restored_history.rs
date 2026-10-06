//! A restored tab keeps its back/forward history and scroll position.
#![cfg(unix)]

use rt_e2e::Browser;

const CONFIG: &str =
    "url.start_pages = [\"about:blank\"]\nauto_save.interval = 500\nmessages.timeout = 0\n";

fn wait_loaded(b: &Browser, page: &str) {
    let url = b.url(page);
    b.wait_until(&format!("{page} is showing"), |s| s.tab().is_loaded(&url));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_restored_tab_goes_back_and_forward_through_its_saved_pages() {
    let b = Browser::launch().toml(CONFIG).start("nav1.html");
    b.open("nav2.html");
    b.open("page.html");
    b.keys("H");
    wait_loaded(&b, "nav2.html");

    b.run("session-save history");
    let saved = std::fs::read_to_string(b.data_dir().join("sessions/history.toml")).unwrap();
    assert!(
        saved.contains("[[windows.tabs.back]]") && saved.contains("[[windows.tabs.forward]]"),
        "{saved}"
    );

    b.run("session-load history");
    wait_loaded(&b, "nav2.html");
    b.keys("H");
    wait_loaded(&b, "nav1.html");
    b.keys("H");
    b.keys("2L");
    wait_loaded(&b, "page.html");
    b.keys("L");
    b.keys("H");
    wait_loaded(&b, "nav2.html");

    // A new page drops the pages ahead, as in any browser.
    b.open("second.html");
    b.keys("H");
    wait_loaded(&b, "nav2.html");
    b.keys("L");
    wait_loaded(&b, "second.html");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_crashed_tab_comes_back_scrolled_where_it_was_with_its_history() {
    let b = Browser::launch().toml(CONFIG).start("nav1.html");
    b.open("page.html");
    b.eval("scrollTo(0, 1500); 'ok'");
    let autosave = b.data_dir().join("sessions/_autosave.toml");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&autosave).is_ok_and(|t| t.contains("scroll = 1500")) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the autosave has no scroll position"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    b.crash();
    b.restart();
    wait_loaded(&b, "page.html");
    b.wait_eval("String(scrollY)", "1500");
    b.keys("H");
    wait_loaded(&b, "nav1.html");
}
