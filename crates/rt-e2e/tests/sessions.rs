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
    let s = b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    assert_eq!(s.tabs()[0].url, b.url("page.html"));
    // Shown once a restored page has loaded, which would otherwise clear it.
    wait_message(&b, "Restored the tabs open before the crash");
}

fn wait_message(b: &Browser, text: &str) {
    b.wait_until(&format!("the message says {text:?}"), |s| {
        s.message().is_some_and(|m| m.contains(text))
    });
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

/// Open a second tab, wait for it to be autosaved, end the browser with
/// `stop`, and check that a restart brings both tabs back.
fn tabs_survive(stop: impl Fn(&Browser), what: &str) {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2 && s.tab().url == second);
    let autosave = wait_for_autosave(&b, "second.html");
    stop(&b);
    b.wait_exit();
    assert!(autosave.exists(), "{what} removed the crash-recovery save");
    b.restart();
    b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_come_back_after_ctrl_c() {
    tabs_survive(|b| b.signal("INT"), "SIGINT");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_come_back_after_the_terminal_closes() {
    tabs_survive(|b| b.signal("HUP"), "SIGHUP");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_come_back_after_the_desktop_session_ends() {
    // Logging out ends the X server; the browser loses its display.
    tabs_survive(Browser::lose_display, "losing the X display");
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

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn lazy_restore_loads_a_background_tab_only_when_its_shown() {
    let b = Browser::launch()
        .toml("session.lazy_restore = true\n")
        .start("nav1.html");
    let lazy = b.url("second.html?lazy=1");
    b.run(&format!("open -t {lazy}"));
    b.wait_until("the second tab loads", |s| s.tab().is_loaded(&lazy));
    b.run("tab-focus 1");
    b.run("session-save lazy");
    b.run("session-load lazy");
    b.wait_until("the session is back with nav1 in front", |s| {
        s.tabs().len() == 2 && s.tab().title == "nav1"
    });
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let db = b.data_dir().join("history.sqlite");
    let count = "SELECT COUNT(*) FROM visits WHERE url LIKE '%second.html?lazy=1'";
    assert_eq!(
        rt_e2e::sqlite(&db, count),
        "1",
        "the background tab loaded early"
    );
    b.run("tab-focus 2");
    b.wait_until("showing it loads it", |s| s.tab().is_loaded(&lazy));
    let start = std::time::Instant::now();
    while rt_e2e::sqlite(&db, count) != "2" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "showing it didn't load it"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn confirm_quit_asks_with_several_tabs() {
    let b = Browser::launch()
        .toml("confirm_quit = [\"multiple-tabs\"]\n")
        .start("page.html");
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.run("quit");
    b.wait_mode("yesno");
    b.keys("n");
    b.wait_mode("normal");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(b.state().tabs().len(), 2, "n quit anyway");
    b.run("quit");
    b.wait_mode("yesno");
    b.keys("y");
    assert!(b.wait_exit().success());
}

/// The `_crashed-…` sessions in the profile, oldest first.
fn crashed_sessions(b: &Browser) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(b.data_dir().join("sessions"))
        .map(|dir| {
            dir.filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.starts_with("_crashed-") && n.ends_with(".toml"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Open a second tab and crash once the autosave has it.
fn crash_with_two_tabs(b: &Browser) -> String {
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    wait_for_autosave(b, "second.html");
    b.crash();
    second
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_crash_is_kept_under_its_own_name() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = crash_with_two_tabs(&b);
    b.restart();
    b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    let crashed = crashed_sessions(&b);
    assert_eq!(crashed.len(), 1, "{crashed:?}");
    let text = std::fs::read_to_string(b.data_dir().join("sessions").join(&crashed[0])).unwrap();
    assert!(text.contains("second.html"), "{text}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_url_after_a_crash_opens_alone_and_the_crashed_tabs_survive_autosaves() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    crash_with_two_tabs(&b);
    let nav1 = b.url("nav1.html");
    b.restart_with(&[&nav1]);
    let s = b.wait_until("only the given URL opens", |s| s.tab().is_loaded(&nav1));
    assert_eq!(s.tabs().len(), 1, "{:?}", s.tabs());
    wait_message(
        &b,
        "The tabs open before the crash are in :session-load _crashed-",
    );
    // The new run autosaves its own tabs; the crashed ones must survive that.
    wait_for_autosave(&b, "nav1.html");
    let crashed = crashed_sessions(&b);
    assert_eq!(crashed.len(), 1, "{crashed:?}");
    let text = std::fs::read_to_string(b.data_dir().join("sessions").join(&crashed[0])).unwrap();
    assert!(
        text.contains("second.html"),
        "the crashed tabs were overwritten: {text}"
    );
    b.run(&format!(
        "session-load {}",
        crashed[0].trim_end_matches(".toml")
    ));
    b.wait_until("loading it brings them back", |s| {
        s.tabs().iter().any(|t| t.url.ends_with("/second.html"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_that_crash_again_right_after_reopening_arent_reopened() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = crash_with_two_tabs(&b);
    b.restart();
    b.wait_until("the tabs are reopened", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    let marker = b.data_dir().join("sessions/.recovering");
    assert!(marker.exists(), "reopened tabs should be on probation");
    // Crash again well within the probation period.
    wait_for_autosave(&b, "second.html");
    b.crash();
    b.restart();
    let s = b.wait_until("the start page opens instead", |s| {
        s.tab().url == "about:blank"
    });
    assert_eq!(s.tabs().len(), 1, "{:?}", s.tabs());
    wait_message(&b, "crashed again soon after reopening");
    assert!(!marker.exists(), "the probation mark should be cleared");
    assert!(!crashed_sessions(&b).is_empty());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_clean_quit_after_reopening_crashed_tabs_ends_probation() {
    let b = Browser::launch().toml(CONFIG).start("page.html");
    let second = crash_with_two_tabs(&b);
    b.restart();
    b.wait_until("the tabs are reopened", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    b.run("quit");
    assert!(b.wait_exit().success());
    assert!(!b.data_dir().join("sessions/.recovering").exists());
}
