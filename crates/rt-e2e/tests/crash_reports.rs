//! A panic leaves a crash report, and the next start says where it is.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_panic_writes_a_report_that_the_next_start_mentions_once() {
    let b = Browser::launch()
        .toml("url.start_pages = [\"about:blank\"]\nmessages.timeout = 0\n")
        .start("page.html");
    b.panic();
    assert!(!b.wait_exit().success(), "a panic doesn't exit cleanly");

    let dir = b.data_dir().join("crashes");
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("crash-"))
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let report = std::fs::read_to_string(dir.join(&names[0])).unwrap();
    for part in [
        "Version:  riptide ",
        "a panic the test channel asked for",
        "test_control.rs",
        "Backtrace:",
    ] {
        assert!(report.contains(part), "{part:?} in\n{report}");
    }

    b.restart();
    let state = b.wait_until("the report is mentioned", |s| {
        s.message().is_some_and(|m| m.contains("crashed last time"))
    });
    let message = state.message().unwrap();
    assert!(
        message.contains(&names[0]) && message.contains(":crash-report"),
        "{message}"
    );

    // :crash-report shows it, ready to send as an issue; no email address is set.
    b.run("crash-report");
    b.wait_until("the report page opens", |s| {
        s.tab().url == "riptide://crash/" && !s.tab().loading
    });
    b.wait_eval(
        "String(document.getElementById('text').value.includes('a panic the test channel asked for'))",
        "true",
    );
    b.wait_eval(
        "String(document.getElementById('issue').href.startsWith('https://github.com/joshzcold/riptide/issues/new?title=Crash%3A%20a%20panic'))",
        "true",
    );
    b.wait_eval("String(document.getElementById('email').hidden)", "true");
    // Edits go into what's sent.
    b.eval("const t = document.getElementById('text'); t.value = 'edited by the user'; t.dispatchEvent(new Event('input')); ''");
    b.wait_eval(
        "String(decodeURIComponent(document.getElementById('issue').href).includes('edited by the user'))",
        "true",
    );
    b.run("set crash_report.email bugs@example.invalid");
    b.run("crash-report");
    b.wait_until("a second report page", |s| {
        s.tabs().len() == 3 && !s.tab().loading
    });
    b.wait_eval(
        "String(document.getElementById('email').href.startsWith('mailto:bugs@example.invalid?'))",
        "true",
    );

    b.run("quit");
    b.wait_exit();
    b.restart();
    b.wait_until("the start page loads", |s| {
        s.tab().url == "about:blank" && !s.tab().loading
    });
    b.wait_painted();
    let state = b.state();
    assert!(
        !state
            .message()
            .unwrap_or_default()
            .contains("crashed last time"),
        "mentioned only once: {state:?}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn crash_report_says_when_there_are_none() {
    let b = Browser::start("page.html");
    b.run("crash-report");
    b.wait_until("the report page opens", |s| {
        s.tab().url == "riptide://crash/" && !s.tab().loading
    });
    b.wait_eval("String(document.getElementById('none').hidden)", "false");
}
