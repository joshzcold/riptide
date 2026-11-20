//! `:edit` and `editor.file_urls`: the built-in editor page opens, edits and
//! writes local files.
#![cfg(unix)]

use rt_e2e::Browser;

const STATUS: &str = "document.getElementById('status').textContent";

/// Poll a file until it holds `expected`.
fn wait_contents(path: &std::path::Path, expected: &str) {
    let start = std::time::Instant::now();
    loop {
        let got = std::fs::read_to_string(path).unwrap_or_default();
        if got == expected {
            return;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{} holds {got:?}, not {expected:?}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_editor_page_writes_files() {
    let b = Browser::start("page.html");
    std::fs::create_dir_all(b.scratch()).unwrap();
    let file = b.scratch().join("notes.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();

    b.run(&format!("edit {}", file.display()));
    b.wait_until("the editor opens in passthrough", |s| {
        s.tab().url.starts_with("riptide://edit/") && s.mode == "passthrough"
    });
    b.wait_eval(
        "document.querySelector('.cm-content')?.textContent ?? ''",
        "fn main() {}",
    );
    b.wait_painted();

    // Vim keys go to the page. Keys sent through the test channel lose Shift,
    // so `:w` can't be typed here; Ctrl+S writes the same way.
    b.keys("ggo// hi<Escape><Ctrl-s>");
    wait_contents(&file, "fn main() {}\n// hi\n");
    b.wait_eval(&format!("String({STATUS}.includes('written'))"), "true");

    // A file changed elsewhere isn't overwritten without :w!.
    std::fs::write(&file, "changed elsewhere\n").unwrap();
    b.keys("x<Ctrl-s>");
    b.wait_eval(
        &format!("String({STATUS}.includes('changed on disk'))"),
        "true",
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "changed elsewhere\n"
    );
    // What :w! sends.
    b.eval(
        "rt.send('save', JSON.stringify({ text: 'forced\\n', version: null, force: true })), ''",
    );
    wait_contents(&file, "forced\n");

    // Undo leaves nothing to write, and then what :q sends closes the tab.
    b.keys("u");
    b.wait_eval("String(document.title)", "notes.rs");
    b.eval("rt.send('quit', '{}'), ''");
    b.wait_until("the editor's tab closes", |s| s.tabs().len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn local_text_files_open_in_the_editor_and_html_still_renders() {
    let b = Browser::start("page.html");
    let dir = b.scratch().join("files");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("todo.md"), "# Todo\n").unwrap();
    std::fs::write(dir.join("page.html"), "<title>local</title>").unwrap();

    b.run(&format!("open file://{}/todo.md", dir.display()));
    b.wait_until("the text file opens in the editor", |s| {
        s.tab().url.starts_with("riptide://edit/") && s.tab().url.ends_with("/todo.md")
    });

    b.run(&format!("open file://{}/page.html", dir.display()));
    b.wait_until("the HTML file renders", |s| s.tab().title == "local");
    assert!(b.state().tab().url.starts_with("file://"));

    // A new file is written on the first save.
    let new = dir.join("new.txt");
    b.run(&format!("edit {}", new.display()));
    b.wait_eval(STATUS, "New file");
    b.wait_painted();
    b.keys("ihello<Escape><Ctrl-s>");
    wait_contents(&new, "hello");

    // Web pages don't get the editor's message channel, nor can they open it.
    b.open("page.html");
    assert_eq!(b.eval("typeof rt"), "undefined");
    b.eval(&format!(
        "location.href = 'riptide://edit{}', ''",
        new.display()
    ));
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(b.state().tab().url.starts_with("http://"));

    // With the setting off, file:// pages show as Chromium shows them.
    b.run("set editor.file_urls false");
    b.run(&format!("open file://{}/todo.md", dir.display()));
    b.wait_until("the text file shows as text", |s| {
        s.tab().url.starts_with("file://") && s.tab().is_loaded(&s.tab().url)
    });
}
