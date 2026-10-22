//! Spell checking from the keyboard. Chromium checks with a real dictionary,
//! which isn't downloaded in tests: these use one already installed, from
//! `RT_E2E_DICTIONARY` or riptide's own data directory, and skip without one.
#![cfg(unix)]

use rt_e2e::Browser;

/// An installed en-US dictionary to copy in, if there is one.
fn dictionary() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("RT_E2E_DICTIONARY") {
        return Some(path.into());
    }
    let dir = std::path::PathBuf::from(std::env::var_os("HOME")?)
        .join(".local/share/riptide/Dictionaries");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("en-US-") && n.to_string_lossy().ends_with(".bdic")
            })
        })
}

/// Start on `page` with spell checking on, or None without a dictionary.
fn with_spelling(page: &str) -> Option<Browser> {
    let Some(source) = dictionary() else {
        eprintln!("skipped: no en-US dictionary (set RT_E2E_DICTIONARY to a .bdic file)");
        return None;
    };
    let b = Browser::start(page);
    let dir = b.data_dir().join("Dictionaries");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&source, dir.join(source.file_name().unwrap())).unwrap();
    b.run("config-list-add spellcheck.languages en-US");
    Some(b)
}

/// From the start of the field (`reset` puts the cursor there), press `keys`
/// until the text cursor is at `at`. Chromium marks mistakes a little after a
/// field gets focus, so the first tries can skip or find none.
fn press_until(b: &Browser, reset: &str, keys: &str, at: &str) {
    let start = std::time::Instant::now();
    loop {
        b.eval(reset);
        b.keys(keys);
        std::thread::sleep(std::time::Duration::from_millis(300));
        if b.eval("window.cursor()") == at {
            return;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{keys} never reached {at}; the cursor is at {} ({:?})",
            b.eval("window.cursor()"),
            b.state().message()
        );
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bracket_s_moves_between_misspelled_words_in_a_text_field() {
    let Some(b) = with_spelling("spellnav.html") else {
        return;
    };
    // Clicked into, as you would, then back to normal mode, which keeps the
    // field focused.
    b.follow_hint("hint inputs", |h| h.text.starts_with("this is"));
    b.wait_mode("insert");
    b.keys("<Escape>");
    b.wait_mode("normal");
    // "sentance" starts at 10, "mispeled" at 28.
    press_until(
        &b,
        "document.activeElement.setSelectionRange(0, 0)",
        "]s",
        "10",
    );
    // The search is over once it names the word; back in insert mode,
    // Alt-s offers the fix.
    b.wait_until("it names the word", |s| {
        s.message().is_some_and(|m| m.contains("sentance"))
    });
    b.keys("i");
    b.wait_mode("insert");
    b.keys("<Alt-s>");
    b.wait_until("sentence is suggested", |s| {
        s.completion["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["name"].as_str() == Some("sentence")))
    });
    // Leaving the command line leaves the field in normal mode again.
    b.keys("<Escape>");
    b.wait_mode("normal");
    b.keys("]s");
    b.wait_eval("window.cursor()", "28");
    b.keys("[s");
    b.wait_eval("window.cursor()", "10");
    // Past the last one, it says so and leaves the cursor where it was.
    b.keys("]s");
    b.wait_eval("window.cursor()", "28");
    b.keys("]s");
    b.wait_until("it says there are no more", |s| {
        s.message()
            .is_some_and(|m| m.contains("No misspelled word after the cursor"))
    });
    assert_eq!(b.eval("window.cursor()"), "28");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bracket_s_works_in_editable_text_too() {
    let Some(b) = with_spelling("spellnav.html") else {
        return;
    };
    b.follow_hint("hint inputs", |h| h.text.starts_with("another"));
    b.wait_mode("insert");
    b.keys("<Escape>");
    b.wait_mode("normal");
    // "paragrap" starts at 8.
    press_until(
        &b,
        "getSelection().collapse(document.activeElement.firstChild, 0)",
        "]s",
        "8",
    );
}
