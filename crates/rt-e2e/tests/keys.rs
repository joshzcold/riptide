//! Modes and keys: the engine path every key takes.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn counts_scroll_and_g_jumps() {
    let b = Browser::start("page.html");
    b.keys("5j");
    b.wait_eval("String(scrollY)", "200");
    b.keys("G");
    b.wait_eval("String(scrollY > 3000)", "true");
    b.keys("gg");
    b.wait_eval("String(scrollY)", "0");
    // Plain letters bound to commands never reach the page.
    assert_eq!(b.state().mode, "normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn focusing_a_field_enters_insert_mode_and_escape_leaves_it() {
    let b = Browser::start("page.html");
    // Through a hint, which clicks for real: insert mode ignores a script's
    // focus(), so pages can't switch it on. The only input gets the label a.
    b.run("hint inputs");
    b.wait_mode("hint");
    b.keys("a");
    b.wait_mode("insert");
    b.keys("abc");
    b.wait_eval("document.getElementById('f').value", "abc");
    b.keys("<Escape>");
    b.wait_mode("normal");
    // Back in normal mode, j scrolls instead of typing.
    b.keys("j");
    b.wait_eval("String(scrollY > 0)", "true");
    assert_eq!(b.eval("document.getElementById('f').value"), "abc");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn passthrough_sends_letters_to_the_page() {
    let b = Browser::start("page.html");
    b.eval("window.keys = ''; addEventListener('keydown', e => { keys += e.key }); 'ok'");
    b.keys("<Ctrl-v>");
    b.wait_mode("passthrough");
    b.keys("jk");
    b.wait_eval("keys", "jk");
    b.keys("<Shift-Escape>");
    b.wait_mode("normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_field_focused_on_load_takes_insert_mode_only_with_auto_load() {
    let b = Browser::start("autofocus.html");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(b.state().mode, "normal");
    b.run("set input.insert_mode.auto_load true");
    b.run("reload");
    b.wait_mode("insert");
}
