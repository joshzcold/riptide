//! Settings for the save prompt, opening downloads and the downloads list.
#![cfg(unix)]

use rt_e2e::Browser;

fn launch(extra: &str) -> Browser {
    Browser::launch()
        .toml(&format!(
            "downloads.location.directory = \"{{scratch}}/dl\"\n{extra}\n"
        ))
        .start("download.html")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn downloads_suggestion_filename_prefills_just_the_name() {
    let b =
        launch("downloads.location.prompt = true\ndownloads.location.suggestion = \"filename\"");
    b.follow_hint("hint", |h| h.text == "download");
    let s = b.wait_mode("prompt");
    let input = s
        .prompt
        .as_ref()
        .and_then(|p| p["input"].as_str())
        .unwrap_or_default()
        .to_string();
    assert_eq!(input, "saved.txt");
    // A bare name is saved in the download directory.
    b.keys("<Return>");
    assert_eq!(b.wait_file(&b.scratch().join("dl/saved.txt")), "hello");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn downloads_open_dispatcher_opens_finished_downloads() {
    let b = launch(
        "downloads.location.prompt = false\n\
         downloads.open_dispatcher = \"sh -c 'echo \\\"$1\\\" > {scratch}/opened' sh\"",
    );
    b.follow_hint("hint", |h| h.text == "download");
    b.wait_file(&b.scratch().join("dl/saved.txt"));
    b.run("download-open");
    let opened = b.wait_file(&b.scratch().join("opened"));
    assert!(opened.trim().ends_with("dl/saved.txt"), "{opened:?}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn downloads_remove_finished_clears_the_list() {
    let b = launch("downloads.location.prompt = false\ndownloads.remove_finished = 0");
    b.follow_hint("hint", |h| h.text == "download");
    b.wait_file(&b.scratch().join("dl/saved.txt"));
    std::thread::sleep(std::time::Duration::from_millis(500));
    b.run("download-open");
    b.wait_until("the finished download is gone from the list", |s| {
        s.message()
            .is_some_and(|m| m.contains("No finished downloads"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn downloads_location_remember_starts_in_the_last_folder() {
    let b = launch("downloads.location.prompt = true");
    std::fs::create_dir_all(b.scratch().join("dl/subdir")).unwrap();
    b.follow_hint("hint", |h| h.text == "download");
    b.wait_mode("prompt");
    b.keys("<Ctrl-w>su<Tab>first.txt<Return>");
    b.wait_file(&b.scratch().join("dl/subdir/first.txt"));
    b.wait_mode("normal");
    b.follow_hint("hint", |h| h.text == "download");
    let s = b.wait_mode("prompt");
    let input = s
        .prompt
        .as_ref()
        .and_then(|p| p["input"].as_str())
        .unwrap_or_default()
        .to_string();
    assert!(input.contains("/dl/subdir/"), "{input:?}");
}
