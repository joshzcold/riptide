//! `gt`: pick a tab by number or text from a filtered list, and `gD`.
#![cfg(unix)]

use rt_e2e::Browser;

fn three_tabs(b: &Browser) {
    for page in ["second.html", "nav2.html"] {
        let url = b.url(page);
        b.run(&format!("open -t {url}"));
        b.wait_until("the tab loads", |s| s.tab().is_loaded(&url));
    }
}

fn completion_names(s: &rt_e2e::State) -> Vec<String> {
    s.completion["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|i| i["name"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gt_filters_tabs_by_number_or_text() {
    let b = Browser::start("page.html");
    three_tabs(&b);
    b.keys("gt");
    let s = b.wait_until("every tab is listed", |s| completion_names(s).len() == 3);
    assert_eq!(s.mode, "command");
    b.keys("2");
    b.wait_until("only tab 2", |s| completion_names(s) == ["2"]);
    b.keys("<Return>");
    b.wait_until("tab 2 is current", |s| s.window().current_tab == 1);
    b.keys("gtnav2<Return>");
    b.wait_until("the nav2 tab is current", |s| s.window().current_tab == 2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gd_gives_the_tab_to_a_new_window() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.keys("gD");
    b.wait_until("a second window has the tab", |s| {
        s.windows.len() == 2
            && s.windows
                .iter()
                .any(|w| w.tabs.len() == 1 && w.tabs[0].url == second)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn cmd_repeat_and_run_with_count_move_through_tabs() {
    let b = Browser::start("page.html");
    three_tabs(&b);
    b.run("cmd-repeat 2 tab-prev");
    b.wait_until("two tabs back", |s| s.window().current_tab == 0);
    b.run("cmd-run-with-count 3 tab-focus");
    b.wait_until("tab 3", |s| s.window().current_tab == 2);
}
