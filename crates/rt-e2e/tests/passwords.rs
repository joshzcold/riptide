//! The passwords plugin (`plugins/passwords`) fills logins from fake password
//! manager tools, through `rt.ui.select`, `rt.ui.input` and
//! `rt.page.fill_login`.
#![cfg(unix)]

use rt_e2e::Browser;

/// A config that loads the plugin with `opts`, a Lua table's contents.
fn config(opts: &str) -> String {
    format!("rt.pack.add({{ builtin = \"passwords\", opts = {{ {opts} }} }})\n")
}

/// Start on the login page with the plugin approved.
fn start(launch: rt_e2e::Launch) -> Browser {
    let b = launch.toml("messages.timeout = 0\n").start("login.html");
    b.wait_until("the plugin asks for its permissions", |s| s.mode == "yesno");
    b.keys("y");
    b.wait_until("approved", |s| s.mode == "normal");
    b
}

fn field(b: &Browser, id: &str) -> String {
    b.eval(&format!("document.getElementById('{id}').value"))
}

const FAKE_PASS: &str = r#"#!/bin/sh
[ "$1" = show ] || exit 2
case $2 in
  127.0.0.1/alice) printf 'pw-alice\nlogin: alice@example.com\nurl: x\n' ;;
  127.0.0.1/bob) printf 'pw-bob\n' ;;
  *) echo "Error: $2 is not in the password store." >&2; exit 1 ;;
esac
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn pass_offers_the_sites_logins_and_fills_the_one_picked() {
    let lua = config(
        "backend = 'pass', command = rt.config_dir .. '/../bin/fakepass', store = rt.config_dir .. '/../store'",
    );
    let b = start(
        Browser::launch()
            .lua(&lua)
            .script("bin/fakepass", FAKE_PASS)
            .file("store/127.0.0.1/alice.gpg", "")
            .file("store/127.0.0.1/bob.gpg", "")
            .file("store/127.0.0.1.evil.net/mallory.gpg", "")
            .file("store/web/not127.0.0.1/eve.gpg", ""),
    );
    b.run("password-fill");
    let s = b.wait_until("a picker opens", |s| {
        s.prompt.as_ref().is_some_and(|p| p["kind"] == "select")
    });
    let labels: Vec<String> = s.prompt.as_ref().unwrap()["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{} {}",
                o["key"].as_str().unwrap(),
                o["label"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        labels,
        ["1 127.0.0.1/alice", "2 127.0.0.1/bob", "<Escape> cancel"]
    );
    assert!(
        s.prompt.as_ref().unwrap()["title"]
            .as_str()
            .unwrap()
            .contains("passwords"),
        "the question names the plugin"
    );
    b.keys("1");
    b.wait_eval("document.getElementById('pass').value", "pw-alice");
    assert_eq!(field(&b, "user"), "alice@example.com");
    assert_eq!(
        field(&b, "search"),
        "",
        "the search box outside the form is left alone"
    );
    b.wait_until("it says so", |s| {
        s.message()
            .is_some_and(|m| m.contains("filled the username and password"))
    });

    // The username alone comes from the path after the site.
    b.run("password-fill-username");
    b.wait_until("a picker opens", |s| s.prompt.is_some());
    b.keys("2");
    b.wait_eval("document.getElementById('user').value", "bob");
    assert_eq!(field(&b, "pass"), "pw-alice", "only the username changed");
}

const FAKE_RBW: &str = r#"#!/bin/sh
case "$1 $2" in
  "search --fields") printf 'id-1\t127.0.0.1\tann\nid-2\tlookalike\teve\n' ;;
  "get --raw")
    case $3 in
      id-1) sleep "${RBW_DELAY:-0}"; echo '{"id":"id-1","name":"127.0.0.1","data":{"username":"ann","password":"pw-ann","totp":null,"uris":[{"uri":"http://127.0.0.1/","match_type":null}]}}' ;;
      id-2) echo '{"id":"id-2","name":"lookalike","data":{"username":"eve","password":"pw-eve","uris":[{"uri":"https://127.0.0.1.evil.net/"}]}}' ;;
    esac ;;
  *) exit 2 ;;
esac
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn rbw_skips_lookalike_sites_and_submits() {
    let lua =
        config("backend = 'rbw', command = rt.config_dir .. '/../bin/fakerbw', submit = true");
    let b = start(Browser::launch().lua(&lua).script("bin/fakerbw", FAKE_RBW));
    b.run("password-fill");
    // One login belongs here, so there's nothing to pick.
    b.wait_eval("String(window.__submitted)", "ann/pw-ann");
    assert!(b.state().prompt.is_none());
}

const FAKE_BW: &str = r#"#!/bin/sh
case "$1 $2" in
  "unlock --passwordenv")
    [ "$(printenv "$3")" = correct ] || { echo "Invalid master password." >&2; exit 1; }
    echo SESSION-KEY ;;
  "list items")
    [ "$BW_SESSION" = SESSION-KEY ] || { echo "Vault is locked." >&2; exit 1; }
    echo '[{"name":"local","login":{"username":"bea","password":"pw-bea","uris":[{"uri":"http://127.0.0.1:1/"}]}}]' ;;
  *) exit 2 ;;
esac
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bw_asks_for_the_master_password_once_masked() {
    let lua = config("backend = 'bw', command = rt.config_dir .. '/../bin/fakebw'");
    let b = start(Browser::launch().lua(&lua).script("bin/fakebw", FAKE_BW));
    b.run("password-fill");
    b.wait_until("it asks for the master password", |s| s.mode == "prompt");
    b.keys("correct");
    let s = b.state();
    assert_eq!(s.prompt.as_ref().unwrap()["input"], "*******", "masked");
    b.keys("<Return>");
    b.wait_eval("document.getElementById('pass').value", "pw-bea");
    // The session is kept: no question the second time.
    b.eval("document.getElementById('pass').value = ''; ''");
    b.run("password-fill-password");
    b.wait_eval("document.getElementById('pass').value", "pw-bea");
    assert!(b.state().prompt.is_none());
    // :password-lock forgets it.
    b.run("password-lock");
    b.run("password-fill");
    b.wait_until("it asks again", |s| s.mode == "prompt");
}

const FAKE_KEEPASSXC: &str = r#"#!/bin/sh
case $3 in "$HOME"/x.kdbx) ;; *) echo "No database at $3" >&2; exit 1 ;; esac
read -r pw
[ "$pw" = dbpass ] || { echo "Error while reading the database: Invalid credentials were provided" >&2; exit 1; }
case $1 in
  search) printf 'Web/Local\nWeb/Elsewhere\n' ;;
  show)
    for last; do :; done
    case $last in
      Web/Local) printf 'Local\ncarl\npw-carl\nhttp://127.0.0.1/\n' ;;
      Web/Elsewhere) printf 'Elsewhere\ndan\npw-dan\nhttps://example.org/\n' ;;
    esac ;;
esac
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn keepassxc_reads_the_database_with_its_password() {
    let lua = config(
        "backend = 'keepassxc', command = rt.config_dir .. '/../bin/fakekp', database = '~/x.kdbx'",
    );
    let b = start(
        Browser::launch()
            .lua(&lua)
            .script("bin/fakekp", FAKE_KEEPASSXC),
    );
    b.run("password-fill");
    b.wait_until("it asks for the database password", |s| s.mode == "prompt");
    b.keys("dbpass<Return>");
    b.wait_eval("document.getElementById('pass').value", "pw-carl");
    assert_eq!(field(&b, "user"), "carl");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn nothing_is_filled_once_the_tab_has_moved_to_another_site() {
    let lua = config("backend = 'rbw', command = rt.config_dir .. '/../bin/fakerbw'");
    let b = start(
        Browser::launch()
            .lua(&lua)
            .script("bin/fakerbw", &FAKE_RBW.replace("${RBW_DELAY:-0}", "2")),
    );
    b.run("password-fill");
    // While rbw is still answering, the tab goes to another host.
    let elsewhere = b.url("login.html").replace("127.0.0.1", "localhost");
    b.run(&format!("open {elsewhere}"));
    b.wait_until("the other site loads", |s| s.tab().is_loaded(&elsewhere));
    b.wait_until("it refuses", |s| {
        s.message().is_some_and(|m| {
            m.contains("didn't fill the login for 127.0.0.1: the tab is now on localhost")
        })
    });
    assert_eq!(field(&b, "pass"), "");
}
