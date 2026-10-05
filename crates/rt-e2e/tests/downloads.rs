//! Downloads: where they go, the save prompt and the downloads page.
#![cfg(unix)]

use rt_e2e::Browser;

fn launch(prompt: bool) -> Browser {
    Browser::launch()
        .toml(&format!(
            "downloads.location.directory = \"{{scratch}}/dl\"\ndownloads.location.prompt = {prompt}\n"
        ))
        .start("download.html")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn downloads_save_to_the_download_directory() {
    let b = launch(false);
    b.follow_hint("hint", |h| h.text == "download");
    assert_eq!(b.wait_file(&b.scratch().join("dl/saved.txt")), "hello");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_completes_paths_in_the_save_prompt() {
    let b = launch(true);
    std::fs::create_dir_all(b.scratch().join("dl/subdir")).unwrap();
    b.follow_hint("hint", |h| h.text == "download");
    b.wait_mode("prompt");
    // Ctrl-w deletes the file name, leaving the directory.
    b.keys("<Ctrl-w>su<Tab>via-tab.txt<Return>");
    assert_eq!(
        b.wait_file(&b.scratch().join("dl/subdir/via-tab.txt")),
        "hello"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn download_delete_removes_the_newest_download() {
    let b = launch(false);
    b.follow_hint("hint", |h| h.text == "download");
    let saved = b.scratch().join("dl/saved.txt");
    b.wait_file(&saved);
    b.run("download-delete");
    let start = std::time::Instant::now();
    while saved.exists() {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{} is still there",
            saved.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
