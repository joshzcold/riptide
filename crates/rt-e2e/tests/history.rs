//! History: completion in :open and importing qutebrowser's.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_completes_from_history_with_tab() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.open("second.html");
    b.open("page.html");
    b.keys(":open secon");
    b.wait_until("history completes the visit", |s| {
        s.completion.to_string().contains("second.html")
    });
    b.keys("<Tab><Return>");
    b.wait_until("the completed page loads", |s| s.tab().is_loaded(&second));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn history_import_reads_qutebrowsers_history() {
    let b = Browser::start("page.html");
    let db = b.scratch().join("qb-history.sqlite");
    let status = std::process::Command::new("python3")
        .arg("-c")
        .arg(
            "import sqlite3, sys; db = sqlite3.connect(sys.argv[1]); \
             db.execute('CREATE TABLE History (url TEXT, title TEXT, atime INTEGER, redirect BOOLEAN)'); \
             db.execute(\"INSERT INTO History VALUES ('https://imported.example/', 'Imported page', 1700000000, 0)\"); \
             db.commit()",
        )
        .arg(&db)
        .status()
        .expect("python3 is needed to write a qutebrowser history file");
    assert!(status.success());
    b.run(&format!("history-import {}", db.display()));
    // The import runs in the background; completion shows it once it's done.
    let start = std::time::Instant::now();
    loop {
        b.keys(":open imported");
        let s = b.wait_mode("command");
        if s.completion.to_string().contains("imported.example") {
            break;
        }
        b.keys("<Escape>");
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the visit wasn't imported"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
