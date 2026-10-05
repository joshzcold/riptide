//! Sessions and crash recovery, across restarts of the same profile.
#![cfg(unix)]

use rt_e2e::Browser;

const CONFIG: &str =
    "url.start_pages = [\"about:blank\"]\nauto_save.interval = 500\nmessages.timeout = 0\n";

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn wq_saves_the_tabs_and_a_restart_restores_them() {
    // Restoring the saved session at startup is what auto_save.session turns on.
    let config = format!("{CONFIG}auto_save.session = true\n");
    let b = Browser::launch().toml(&config).start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2 && s.tab().url == second);
    b.run("wq");
    assert!(b.wait_exit().success());
    b.restart();
    let s = b.wait_until("both tabs are back", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    assert_eq!(s.tabs()[0].url, b.url("page.html"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_come_back_after_a_crash() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2 && s.tab().url == second);
    wait_for_autosave(&b, "second.html");
    b.crash();
    b.restart();
    // The "Restored the tabs" message is soon replaced by the content
    // blocking notice, so check the tabs themselves.
    let s = b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    assert_eq!(s.tabs()[0].url, b.url("page.html"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_clean_quit_leaves_nothing_to_recover() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.run("quit");
    assert!(b.wait_exit().success());
    assert!(!b.data_dir().join("sessions/_autosave.toml").exists());
    b.restart();
    let s = b.wait_until("the start page loads", |s| s.tab().url == "about:blank");
    assert_eq!(s.tabs().len(), 1, "{:?}", s.tabs());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_come_back_after_sigterm() {
    // kill, pkill and logging out send SIGTERM; it mustn't count as a clean quit.
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2 && s.tab().url == second);
    let autosave = wait_for_autosave(&b, "second.html");
    b.terminate();
    b.wait_exit();
    assert!(autosave.exists(), "SIGTERM removed the crash-recovery save");
    b.restart();
    b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
}

/// Wait until auto_save.interval has saved a tab whose URL contains `page`.
fn wait_for_autosave(b: &Browser, page: &str) -> std::path::PathBuf {
    let autosave = b.data_dir().join("sessions/_autosave.toml");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&autosave).is_ok_and(|t| t.contains(page)) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "no autosave at {}",
            autosave.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    autosave
}
