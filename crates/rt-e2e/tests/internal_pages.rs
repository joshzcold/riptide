//! The browser's own pages, and keeping web pages away from them.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn help_opens_the_generated_help_page() {
    let b = Browser::start("page.html");
    b.run("help :open");
    b.wait_until("the help page loads", |s| s.tab().title == "Riptide help");
    b.wait_eval("String(location.hash)", "#cmd-open");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn changelog_history_and_downloads_pages_open() {
    let b = Browser::start("page.html");
    for (command, title) in [
        ("changelog", "riptide changelog"),
        ("history", "History"),
        ("downloads", "Downloads"),
    ] {
        b.run(command);
        b.wait_until(&format!(":{command} opens"), |s| s.tab().title == title);
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn web_pages_cant_see_or_embed_ui_pages() {
    let b = Browser::start("isolation.html");
    b.wait_eval("document.title", "rt=undefined frame=empty");
}
