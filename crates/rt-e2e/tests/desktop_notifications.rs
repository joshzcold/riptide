//! `content.notifications.presenter = libnotify`: riptide sends page
//! notifications with `notify-send`, here a stand-in that records them.
#![cfg(unix)]

use rt_e2e::Browser;

/// Records its arguments, one per line, and "clicks" when asked to wait.
const NOTIFY_SEND: &str = r#"#!/bin/sh
if [ "$1" = "--help" ]; then echo "  -A, --action=[NAME=]Text..."; exit 0; fi
for a in "$@"; do printf '%s\n' "$a"; done >> "{scratch}/notify.log"
for a in "$@"; do if [ "$a" = "--wait" ]; then echo default; fi; done
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn libnotify_sends_notifications_with_riptide_settings_and_a_click_shows_the_tab() {
    let b = Browser::launch()
        .toml(
            "\"content.notifications.enabled\" = \"true\"\n\
             \"content.notifications.presenter\" = \"libnotify\"\n\
             \"content.notifications.app_name\" = \"rt-test\"\n\
             \"content.notifications.urgency\" = \"critical\"\n\
             \"content.notifications.timeout\" = 5000\n",
        )
        .command("notify-send", NOTIFY_SEND)
        .start("page.html");
    let log = b.scratch().join("notify.log");
    // content.notifications.enabled answers the page's request.
    b.eval("Notification.requestPermission(), ''");
    let start = std::time::Instant::now();
    while b.eval("Notification.permission") != "granted" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "permission wasn't granted"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // From the first tab, once the second one is in front.
    b.eval("setTimeout(() => new Notification('Hello', { body: '--from the test' }), 1500), ''");
    b.run("open -t about:blank");
    b.wait_until("the second tab is current", |s| s.window().current_tab == 1);
    b.wait_until("clicking the notification shows its tab", |s| {
        s.window().current_tab == 0
    });
    let args = std::fs::read_to_string(&log).unwrap_or_default();
    let args: Vec<&str> = args.lines().collect();
    for expected in [
        "--app-name=rt-test",
        "--urgency=critical",
        "--expire-time=5000",
        "--wait",
        "--",
        "Hello",
        "--from the test",
    ] {
        assert!(args.contains(&expected), "{expected} not in {args:?}");
    }
    // The page's text comes after `--`, so it can't pass as an option.
    let end = args.iter().position(|a| *a == "--").unwrap();
    assert_eq!(args.get(end + 1), Some(&"Hello"));
    // The body starts with the site (content.notifications.show_origin).
    assert!(
        args.get(end + 2)
            .is_some_and(|a| a.starts_with("http://127.0.0.1")),
        "{args:?}"
    );
}
