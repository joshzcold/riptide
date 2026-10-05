//! How `:open` turns text into a URL, and `Ctrl-a` / `Ctrl-x`.
#![cfg(unix)]

use rt_e2e::Browser;

const ENGINES: &str = r#"
[url.searchengines]
DEFAULT = "{server}/second.html?q={}"
loc = "{server}/nav1.html?q={}"
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn url_auto_search_schemeless_searches_bare_hosts() {
    let b = Browser::launch()
        .toml(&format!("\"url.auto_search\" = \"schemeless\"\n{ENGINES}"))
        .start("page.html");
    b.run("open example.org");
    let s = b.wait_until("the search loads", |s| {
        s.tab().url.contains("second.html?q=")
    });
    assert!(s.tab().url.ends_with("q=example.org"), "{}", s.tab().url);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn url_open_base_url_opens_an_engines_home_page() {
    let b = Browser::launch()
        .toml(&format!("\"url.open_base_url\" = true\n{ENGINES}"))
        .start("page.html");
    let home = b.url("");
    b.run("open loc");
    b.wait_until("the engine's home page opens", |s| s.tab().url == home);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn url_incdec_segments_choose_what_ctrl_a_changes() {
    let b = Browser::launch()
        .toml("\"url.incdec_segments\" = [\"anchor\"]")
        .start("page.html");
    let start = format!("{}?n=1#s1", b.url("nav1.html"));
    b.run(&format!("open {start}"));
    b.wait_until("the page loads", |s| s.tab().url == start);
    b.keys("<Ctrl-a>");
    let next = format!("{}?n=1#s2", b.url("nav1.html"));
    b.wait_until("only the anchor changes", |s| s.tab().url == next);
}
