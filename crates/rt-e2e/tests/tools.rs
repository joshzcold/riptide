//! Debugging and reporting tools, the editor's temporary file, and which
//! window a second riptide invocation uses.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn debug_keytester_names_keys_until_escape() {
    let b = Browser::launch()
        .toml("messages.timeout = 0\n")
        .start("page.html");
    b.keys(":debug-keytester<Return>");
    b.wait_mode("normal");
    b.keys("J");
    b.wait_until("J is described", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("J runs :tab-next"))
    });
    b.keys("<Escape>");
    b.wait_until("the tester ends", |s| s.message() == Some("Key tester off"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn debug_log_filter_and_report() {
    let b = Browser::start("page.html");
    b.run("debug-log-filter rt_cef=debug");
    b.wait_until("the filter changes", |s| {
        s.message() == Some("Log filter: rt_cef=debug")
    });
    b.run("debug-log-filter =nonsense=");
    b.wait_until("a bad filter is refused", |s| {
        s.message().is_some_and(|m| m.starts_with("Bad log filter"))
    });
    b.run("report");
    b.wait_until("a new issue opens in a tab", |s| {
        s.tabs().len() == 2
            && s.tab()
                .url
                .starts_with("https://github.com/joshzcold/riptide/issues/new?body=")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn editor_remove_file_false_keeps_the_text() {
    let b = Browser::launch()
        .toml(
            "editor.command = [\"{scratch}/editor.sh\", \"{file}\"]\n\
             \"editor.remove_file\" = false\n",
        )
        .start("page.html");
    // The editor notes which file it got, so the test can look for it afterwards.
    let script = b.scratch().join("editor.sh");
    let note = b.scratch().join("edited-path");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", note.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    b.run("edit-url");
    let edited = b.wait_file(&note);
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        std::path::Path::new(&edited).exists(),
        "{edited} was removed"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn new_instance_open_target_window_first_opened() {
    let b = Browser::launch()
        .toml("new_instance_open_target_window = \"first-opened\"\n")
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -w {second}"));
    b.wait_until("a second window is active", |s| {
        s.windows.len() == 2 && s.current_window == 1
    });
    let nav = b.url("nav1.html");
    assert!(b.invoke(&[&nav]).success());
    b.wait_until("the URL opens in the first window", |s| {
        s.windows[0].tabs.iter().any(|t| t.url == nav)
    });
}
