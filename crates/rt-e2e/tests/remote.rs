//! Running `riptide …` again hands its arguments to the running browser.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_second_invocation_opens_its_url_and_runs_its_commands_here() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    let status = b.invoke(&[&second, ":set messages.timeout 0"]);
    assert!(status.success(), "{status}");
    let s = b.wait_until("the URL opens in a new tab", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    assert_eq!(s.windows.len(), 1, "it started a second window");
    b.run("set messages.timeout");
    b.wait_until("the command ran", |s| {
        s.message()
            .is_some_and(|m| m.contains("messages.timeout = 0"))
    });
}
