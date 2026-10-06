//! Which session is saved by default, and `:save`.
#![cfg(unix)]

use rt_e2e::Browser;

fn wait_for(path: &std::path::Path) {
    let start = std::time::Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{} wasn't written",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn session_default_name_is_where_wq_saves_and_restart_restores() {
    let b = Browser::launch()
        .toml("session.default_name = \"work\"\nauto_save.session = true\n")
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.run("wq");
    assert!(b.wait_exit().success());
    let sessions = b.data_dir().join("sessions");
    assert!(sessions.join("work.toml").exists());
    assert!(!sessions.join("default.toml").exists());
    b.restart();
    b.wait_until("both tabs are back", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn session_save_without_a_name_saves_the_last_loaded_session() {
    let b = Browser::start("page.html");
    let sessions = b.data_dir().join("sessions");
    b.run("session-save trip");
    wait_for(&sessions.join("trip.toml"));
    b.run("session-load trip");
    b.open("second.html");
    b.run("session-save");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(sessions.join("trip.toml"))
        .unwrap_or_default()
        .contains("second.html")
    {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "trip wasn't saved again");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(!sessions.join("default.toml").exists());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn save_writes_the_session_and_rejects_unknown_things() {
    let b = Browser::start("page.html");
    b.run("save session");
    wait_for(&b.data_dir().join("sessions/default.toml"));
    b.run("save");
    b.wait_until("everything is saved", |s| {
        s.message() == Some("Saved everything")
    });
    b.run("save passwords");
    b.wait_until("an error", |s| {
        s.message()
            .is_some_and(|m| m.contains("can't save passwords"))
    });
}
