//! Commands that don't belong to a bigger area.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn screenshot_saves_a_png_and_wont_overwrite_without_force() {
    let b = Browser::start("page.html");
    let shot = b.scratch().join("shot.png");
    b.run(&format!("screenshot {}", shot.display()));
    let start = std::time::Instant::now();
    while std::fs::metadata(&shot).map_or(0, |m| m.len()) < 1000 {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "no screenshot");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let png = std::fs::read(&shot).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let modified = std::fs::metadata(&shot).unwrap().modified().unwrap();
    b.run("set messages.timeout 0");
    b.run(&format!("screenshot {}", shot.display()));
    b.wait_until("it refuses to overwrite", |s| {
        s.message().is_some_and(|m| m.contains("--force"))
    });
    assert_eq!(
        std::fs::metadata(&shot).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn config_diff_and_config_write_toml() {
    let b = Browser::start("page.html");
    b.run("set hints.chars xyz");
    b.run("config-diff");
    b.wait_until("the changed-settings page opens", |s| {
        s.tab().title == "Changed settings"
    });
    b.run("config-write-toml");
    let toml = b.wait_file(&b.config_dir().join("config.toml"));
    assert!(toml.contains("hints.chars"), "{toml}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bookmark_list_debug_dump_page_and_quickmarks_reload() {
    let b = Browser::start("nav1.html");
    let dump = b.scratch().join("dump.html");
    b.run(&format!("debug-dump-page {}", dump.display()));
    assert!(b.wait_file(&dump).contains("<title>nav1</title>"));
    std::fs::write(
        b.config_dir().join("quickmarks"),
        format!("byhand {}\n", b.url("nav2.html")),
    )
    .unwrap();
    b.run("quickmarks-reload");
    b.run("quickmark-load byhand");
    b.wait_until("the hand-written quickmark opens", |s| {
        s.tab().title == "nav2"
    });
    b.run("bookmark-list -t");
    b.wait_until("the bookmarks page opens", |s| s.tab().title == "Bookmarks");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn messages_lists_this_sessions_messages() {
    let b = Browser::start("page.html");
    b.run("messages");
    b.wait_until("the messages page opens", |s| s.tab().title == "Messages");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn cmd_later_insert_text_and_click_element() {
    let b = Browser::start("editor.html");
    b.run("cmd-later 500 insert-text hi");
    b.eval("t.focus(); 'ok'");
    b.wait_eval("document.title", "v=hi");
    b.open("links.html");
    b.run("click-element css a[href='#news']");
    b.wait_eval("document.title", "clicked news");
}
