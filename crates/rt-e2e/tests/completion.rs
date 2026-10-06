//! What the command line's completion offers, from the completion settings.
#![cfg(unix)]

use rt_e2e::{Browser, State};

/// `(category, name)` of every completion on screen.
fn items(s: &State) -> Vec<(String, String)> {
    s.completion["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|i| {
                    let text = |k: &str| i[k].as_str().unwrap_or_default().to_string();
                    (text("category"), text("name"))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_open_categories_set_the_sources_and_their_order() {
    let b = Browser::launch()
        .toml(
            r#"
"completion.open_categories" = ["searchengines", "history"]
[url.searchengines]
DEFAULT = "https://duckduckgo.com/?q={}"
docs = "https://docs.rs/{}"
"#,
        )
        .start("page.html");
    b.keys(":open d");
    let s = b.wait_until("the search engine is offered", |s| !items(s).is_empty());
    assert_eq!(
        items(&s)[0],
        ("Search engines".into(), "docs".into()),
        "{:?}",
        items(&s)
    );
    assert!(
        items(&s)
            .iter()
            .all(|(c, _)| c != "Quickmarks" && c != "Bookmarks"),
        "only the listed categories: {:?}",
        items(&s)
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_leaves_out_excluded_history() {
    let b = Browser::start("page.html");
    b.open("second.html");
    b.keys(":open second");
    b.wait_until("the visit is offered", |s| {
        items(s)
            .iter()
            .any(|(c, n)| c == "History" && n.contains("second.html"))
    });
    b.keys("<Escape>");
    b.wait_mode("normal");
    b.run("set completion.web_history.exclude [\"*second.html*\"]");
    b.keys(":open second");
    let s = b.wait_mode("command");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let s = if items(&s).is_empty() { b.state() } else { s };
    assert!(
        !items(&s).iter().any(|(c, _)| c == "History"),
        "{:?}",
        items(&s)
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_lists_files_for_paths() {
    let b = Browser::start("page.html");
    let dir = b.scratch();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("picked.txt"), "x").unwrap();
    b.keys(&format!(":open {}/pi", dir.display()));
    let s = b.wait_until("the file is offered", |s| {
        items(s)
            .iter()
            .any(|(c, n)| c == "Filesystem" && n.ends_with("/picked.txt"))
    });
    assert_eq!(s.mode, "command");
}

/// Poll JavaScript in the completion overlay until `test` holds.
fn wait_overlay(b: &Browser, code: &str, test: impl Fn(&str) -> bool) -> String {
    let start = std::time::Instant::now();
    loop {
        let got = b.eval_bar("completion", code);
        if test(&got) {
            return got;
        }
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "{code} is still {got:?}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

const LIST: &str = "document.getElementById('list').innerText";

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_history_shows_when_it_was_visited() {
    let b = Browser::launch()
        .toml("completion.timestamp_format = \"visited %Y\"\n")
        .start("page.html");
    b.keys(":open page");
    let year = wait_overlay(
        &b,
        "document.querySelector('.time')?.textContent ?? ''",
        |t| !t.is_empty(),
    );
    let now = b.eval_bar("completion", "String(new Date().getFullYear())");
    assert_eq!(year, format!("visited {now}"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_shrink_false_keeps_the_list_completion_height_tall() {
    let b = Browser::launch()
        .toml("completion.height = \"10\"\n")
        .start("page.html");
    let height = |b: &Browser| -> i64 {
        b.keys(":open page");
        wait_overlay(b, LIST, |t| t.contains("page.html"));
        // Let the overlay take its new size.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let h = b
            .eval_bar("completion", "String(innerHeight)")
            .parse()
            .unwrap();
        b.keys("<Escape>");
        b.wait_mode("normal");
        h
    };
    let shrunk = height(&b);
    b.run("set completion.shrink false");
    let tall = height(&b);
    assert!(tall > shrunk, "{tall} isn't taller than {shrunk}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_delay_waits_for_typing_to_pause() {
    let b = Browser::launch()
        .toml("completion.delay = 2000\n")
        .start("page.html");
    b.keys(":");
    wait_overlay(&b, LIST, |t| t.contains("Commands"));
    b.keys("open page");
    let typed = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        !b.eval_bar("completion", LIST).contains("page.html"),
        "completions updated before the delay"
    );
    wait_overlay(&b, LIST, |t| t.contains("page.html"));
    assert!(typed.elapsed() >= std::time::Duration::from_millis(1900));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn completion_set_shows_current_values_then_the_choices() {
    let b = Browser::launch()
        .toml("hints.mode = \"number\"\n")
        .start("page.html");
    b.keys(":set hints.mod");
    wait_overlay(
        &b,
        "document.querySelector('.row .detail')?.textContent ?? ''",
        |t| t == "number",
    );
    b.keys("<Tab>");
    wait_overlay(&b, LIST, |t| t.contains("letter") && t.contains("word"));
    let list = b.eval_bar("completion", LIST);
    assert!(list.contains("current"), "{list}");
    b.keys("w<Tab><Return>");
    b.wait_mode("normal");
    b.keys(":set hints.mode");
    wait_overlay(
        &b,
        "document.querySelector('.row .detail')?.textContent ?? ''",
        |t| t == "word",
    );
}
