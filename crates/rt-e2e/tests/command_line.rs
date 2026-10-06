//! The command line: typing, completion and running commands.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn typed_commands_run_on_return() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.keys(&format!(":open -t {second}"));
    let s = b.wait_mode("command");
    assert_eq!(
        s.status
            .pointer("/command_line/text")
            .and_then(|t| t.as_str()),
        Some(format!(":open -t {second}").as_str())
    );
    b.keys("<Return>");
    b.wait_until("the new tab loads", |s| {
        s.mode == "normal" && s.tabs().len() == 2 && s.tab().url == second
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn set_completes_setting_names() {
    let b = Browser::start("page.html");
    b.keys(":set hints.");
    let s = b.wait_until("completion lists hints settings", |s| {
        s.completion["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i.to_string().contains("hints.chars")))
    });
    assert_eq!(s.mode, "command");
    b.keys("<Escape>");
    b.wait_mode("normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn set_changes_a_setting() {
    let b = Browser::start("page.html");
    b.run("set messages.timeout 0");
    b.run("set hints.chars");
    let s = b.wait_until("the value is shown", |s| {
        s.message().is_some_and(|m| m.contains("hints.chars"))
    });
    assert!(
        s.message().unwrap().contains("asdfghjkl"),
        "{:?}",
        s.message()
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ctrl_v_pastes_the_clipboard_into_the_command_line() {
    let b = Browser::start("page.html");
    let url = b.url("page.html");
    b.keys("yy");
    b.wait_until("the URL is yanked", |s| {
        s.message().is_some_and(|m| m.contains("Yanked"))
    });
    b.keys(":open <Ctrl-v>");
    b.wait_until("the URL is pasted", |s| {
        s.status["command_line"]["text"].as_str() == Some(&format!(":open {url}"))
    });
}
