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

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ctrl_d_deletes_the_selected_history_entry() {
    let b = Browser::start("page.html");
    b.open("second.html");
    b.open("page.html");
    let db = b.data_dir().join("history.sqlite");
    let count = "SELECT COUNT(*) FROM completion WHERE url LIKE '%/second.html%'";
    assert_eq!(rt_e2e::sqlite(&db, count), "1");
    b.keys("o");
    b.wait_mode("command");
    b.keys("second.html");
    b.wait_until("completion offers the visit", |s| {
        s.completion.to_string().contains("second.html")
    });
    b.keys("<Tab><Ctrl-d><Escape>");
    b.wait_mode("normal");
    let start = std::time::Instant::now();
    while rt_e2e::sqlite(&db, count) != "0" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the entry wasn't deleted"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn private_windows_keep_no_history() {
    let b = Browser::start("page.html");
    let private = b.url("private.html");
    b.run(&format!("open -p {private}"));
    b.wait_until("the private page loads", |s| {
        s.window().private && s.tab().is_loaded(&private)
    });
    b.run("close");
    b.wait_until("the private window closes", |s| s.windows.len() == 1);
    let db = b.data_dir().join("history.sqlite");
    let visits = |page: &str| {
        rt_e2e::sqlite(
            &db,
            &format!("SELECT COUNT(*) FROM visits WHERE url LIKE '%/{page}'"),
        )
    };
    assert_eq!(visits("page.html"), "1", "normal visits are recorded");
    assert_eq!(
        visits("private.html"),
        "0",
        "the private visit is in history"
    );
}
