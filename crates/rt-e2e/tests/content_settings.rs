//! Content settings that map onto Chromium's: images and popups.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_block_images_per_site() {
    let b = Browser::start("images.html");
    b.wait_until("the image loads", |s| s.tab().title == "image=8");
    let site = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set -u {site} content.images false"));
    b.run("reload");
    b.wait_until("the image is blocked", |s| s.tab().title == "image=0");
    b.run(&format!("set -u {site} content.images true"));
    b.run("reload");
    b.wait_until("the image loads again", |s| s.tab().title == "image=8");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_popups_need_can_open_tabs_automatically() {
    let b = Browser::start("popup.html");
    b.wait_until("the popup is blocked", |s| s.tab().title == "popup=blocked");
    assert_eq!(b.state().tabs().len(), 1);
    b.run("set content.javascript.can_open_tabs_automatically true");
    b.run("reload");
    b.wait_until("the popup opens", |s| s.tabs().len() == 2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_leave_private_windows_working() {
    let b = Browser::launch()
        .toml(
            "content.images = false\ncontent.mute = true\n\
             content.javascript.can_open_tabs_automatically = true\n\
             content.javascript.clipboard = \"access-paste\"\n",
        )
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -p {second}"));
    b.wait_until("the private window loads", |s| {
        s.window().private && s.tab().is_loaded(&second)
    });
}
