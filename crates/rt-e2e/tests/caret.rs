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

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn yank_selection_takes_text_selected_in_a_field() {
    let b = Browser::start("page.html");
    b.eval("const f = document.getElementById('f'); f.value = 'one two three'; f.focus(); f.setSelectionRange(4, 7); 'ok'");
    b.run("yank selection");
    b.wait_until("the field's selection is yanked", |s| {
        s.message().is_some_and(|m| m.ends_with(": two"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn yank_selection_takes_text_selected_in_an_iframe() {
    let b = Browser::start("page.html");
    b.eval(
        "window.framed = 'no'; const f = document.createElement('iframe'); \
         f.onload = () => { framed = 'yes' }; f.srcdoc = '<p id=p>inside the frame</p>'; \
         document.body.append(f); 'ok'",
    );
    b.wait_eval("framed", "yes");
    b.eval(
        "const w = document.querySelector('iframe').contentWindow; w.focus(); \
         w.getSelection().selectAllChildren(w.document.getElementById('p')); 'ok'",
    );
    b.run("yank selection");
    b.wait_until("the frame's selection is yanked", |s| {
        s.message()
            .is_some_and(|m| m.ends_with(": inside the frame"))
    });
}
