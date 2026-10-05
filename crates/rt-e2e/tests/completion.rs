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
