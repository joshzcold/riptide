//! Questions the browser asks: JavaScript dialogs.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn confirm_is_answered_with_y() {
    let b = Browser::start("dialogs.html");
    b.eval("setTimeout(ask, 0); 'ok'");
    let s = b.wait_mode("yesno");
    assert!(
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("Sure?")),
        "{:?}",
        s.prompt
    );
    b.keys("y");
    b.wait_eval("document.title", "confirm true");
    assert_eq!(b.state().mode, "normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn confirm_is_refused_with_n() {
    let b = Browser::start("dialogs.html");
    b.eval("setTimeout(ask, 0); 'ok'");
    b.wait_mode("yesno");
    b.keys("n");
    b.wait_eval("document.title", "confirm false");
}
