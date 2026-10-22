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
    b.wait_eval("String(document.getElementById('dumps').hidden)", "true");
}

/// Wait until Chromium's crash reporter has left a dump in `pending/`.
fn wait_for_dump(b: &Browser) {
    let start = std::time::Instant::now();
    while !std::fs::read_dir(b.data_dir().join("pending")).is_ok_and(|d| {
        d.flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "dmp"))
    }) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "no crash dump in {}",
            b.data_dir().display()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// End the browser process with `signal`, as a crash in native code would,
/// and check that its tabs and a crash dump survive it.
fn native_crash_keeps_the_tabs_and_leaves_a_dump(signal: &str) {
    let b = Browser::launch()
        .toml(
            "url.start_pages = [\"about:blank\"]\nauto_save.interval = 500\nmessages.timeout = 0\n",
        )
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    let autosave = b.data_dir().join("sessions/_autosave.toml");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&autosave).is_ok_and(|t| t.contains("second.html")) {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "no autosave");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    b.signal(signal);
    assert!(!b.wait_exit().success(), "SIG{signal} doesn't exit cleanly");
    wait_for_dump(&b);
    assert!(
        autosave.exists(),
        "SIG{signal} removed the crash-recovery save"
    );

    b.restart();
    b.wait_until("the tabs are restored", |s| {
        s.tabs().len() == 2 && s.tabs()[1].url == second
    });
    b.run("crash-report");
    b.wait_until("the report page opens", |s| {
        s.tab().url == "riptide://crash/" && !s.tab().loading
    });
    b.wait_eval(
        "String(document.querySelectorAll('#dump-list li').length)",
        "1",
    );
}

// Native code (CEF, Chromium, a C library) aborts or faults without running
// riptide's panic hook; Chromium's crash reporter catches both.
#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_native_abort_in_the_browser_keeps_the_tabs_and_leaves_a_dump() {
    native_crash_keeps_the_tabs_and_leaves_a_dump("ABRT");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_segfault_in_the_browser_keeps_the_tabs_and_leaves_a_dump() {
    native_crash_keeps_the_tabs_and_leaves_a_dump("SEGV");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_chromium_crash_leaves_a_dump_that_the_next_start_mentions() {
    let b = Browser::launch()
        .toml("url.start_pages = [\"about:blank\"]\nmessages.timeout = 0\n")
        .start("page.html");
    // Crashpad, turned on by the crash_reporter.cfg riptide writes, keeps a
    // dump of the tab's renderer.
    b.crash_tab();
    b.wait_until("the tab crashed", |s| s.tab().crashed);
    wait_for_dump(&b);
    b.run("quit");
    b.wait_exit();
    b.restart();
    b.wait_until("the dump is mentioned", |s| {
        s.message().is_some_and(|m| m.contains("left a crash dump"))
    });
    b.run("crash-report");
    b.wait_until("the report page opens", |s| {
        s.tab().url == "riptide://crash/" && !s.tab().loading
    });
    b.wait_eval(
        "String(document.querySelectorAll('#dump-list li').length)",
        "1",
    );
}
