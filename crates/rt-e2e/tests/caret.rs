//! Caret mode: moving and selecting text from the keyboard.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn caret_mode_moves_and_selects_by_word() {
    let b = Browser::start("caret.html");
    b.keys("v");
    b.wait_mode("caret");
    b.keys("wwvee");
    b.wait_eval("document.title", "sel=brown fox");
    b.keys("<Escape>");
    b.wait_mode("normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bracket_moves_to_the_start_of_the_next_block() {
    let b = Browser::start("blocks.html");
    b.keys("v");
    b.wait_mode("caret");
    b.keys("]ve");
    b.wait_eval("document.title", "sel=Second");
}
