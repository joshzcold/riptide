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
    b.wait_download_finished();
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

/// The downloads page of the active window, once it shows `title`.
fn downloads_page(b: &Browser, title: &str) -> String {
    b.run("downloads");
    b.wait_until(title, |s| s.tab().title == title);
    b.eval("document.body.innerText")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn private_downloads_are_listed_only_in_private_windows_until_they_close() {
    let b = launch(false);
    b.run(&format!("open -p {}", b.url("download.html")));
    b.wait_until("the private window", |s| {
        s.window().private && s.tab().title == "download"
    });
    b.follow_hint("hint", |h| h.text == "download");
    // The file goes where downloads usually go.
    assert_eq!(b.wait_file(&b.scratch().join("dl/saved.txt")), "hello");
    b.wait_download_finished();
    assert!(downloads_page(&b, "Private downloads").contains("saved.txt"));

    b.run(&format!("open -w {}", b.url("page.html")));
    b.wait_until("a normal window", |s| {
        s.windows.len() == 3 && !s.window().private
    });
    let normal = downloads_page(&b, "Downloads");
    assert!(normal.contains("No downloads"), "{normal}");

    // Closing every private window forgets the list.
    b.run("window-only");
    b.wait_until("only the normal window", |s| s.windows.len() == 1);
    b.run(&format!("open -p {}", b.url("page.html")));
    b.wait_until("a private window again", |s| s.window().private);
    let private = downloads_page(&b, "Private downloads");
    assert!(private.contains("No downloads"), "{private}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn closing_the_last_private_window_asks_before_cancelling_its_downloads() {
    let b = launch(false);
    b.run(&format!("open -p {}", b.url("page.html")));
    b.wait_until("the private window", |s| s.window().private);
    b.run(&format!("download {}", b.url("slow-download")));
    let start = std::time::Instant::now();
    while !b.scratch().join("dl/slow.bin").exists() {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the download didn't start"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.run("close");
    let s = b.wait_mode("yesno");
    let prompt = s.prompt.as_ref().map(|p| p.to_string()).unwrap_or_default();
    assert!(
        prompt.contains("1 private download will be cancelled"),
        "{prompt}"
    );
    b.keys("n");
    b.wait_mode("normal");
    assert_eq!(b.state().windows.len(), 2, "no keeps the window");
    b.run("close");
    b.wait_mode("yesno");
    b.keys("y");
    b.wait_until("the private window closes", |s| s.windows.len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn alt_e_in_the_save_prompt_picks_the_folder() {
    let b = Browser::launch()
        .toml(
            "downloads.location.directory = \"{scratch}/dl\"\ndownloads.location.prompt = true\n\
             fileselect.folder.command = [\"{scratch}/folder-picker.sh\", \"{}\"]\n",
        )
        .start("download.html");
    let picked = b.scratch().join("picked");
    std::fs::create_dir_all(&picked).unwrap();
    let picker = b.scratch().join("folder-picker.sh");
    std::fs::write(
        &picker,
        format!("#!/bin/sh\nprintf '%s\\n' '{}' >\"$1\"\n", picked.display()),
    )
    .unwrap();
    std::fs::set_permissions(&picker, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    b.follow_hint("hint", |h| h.text == "download");
    b.wait_mode("prompt");
    b.keys("<Alt-e>");
    // The picker runs in the background and fills in the prompt.
    b.wait_until("the picked folder is in the prompt", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("picked"))
    });
    b.keys("<Return>");
    assert_eq!(b.wait_file(&picked.join("saved.txt")), "hello");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn fileselect_external_answers_upload_fields_with_a_picker() {
    let b = Browser::launch()
        .toml(
            "fileselect.handler = \"external\"\n\
             fileselect.single_file.command = [\"{scratch}/picker.sh\", \"{}\"]\n",
        )
        .start("upload.html");
    let chosen = b.scratch().join("chosen.txt");
    std::fs::write(&chosen, "x\n").unwrap();
    let picker = b.scratch().join("picker.sh");
    std::fs::write(
        &picker,
        format!("#!/bin/sh\nprintf '%s\\n' '{}' >\"$1\"\n", chosen.display()),
    )
    .unwrap();
    std::fs::set_permissions(&picker, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    b.follow_hint("hint", |_| true);
    b.wait_eval("document.title", "picked=chosen.txt");
}
