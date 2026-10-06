//! The Greasemonkey APIs a script must `@grant`: GM_xmlhttpRequest (limited
//! to `@connect` hosts) and GM_openInTab. The fixture server answers on both
//! 127.0.0.1 and localhost, which gives a second origin.
#![cfg(unix)]

use rt_e2e::Browser;

const XHR: &str = r#"
const other = location.href.replace('127.0.0.1', 'localhost').replace(/[^/]*$/, 'second.html');
GM_xmlhttpRequest({
  url: other,
  onload: (r) => { document.title = `xhr ${r.status} ${(r.responseText.match(/<title>(.*)<\/title>/) || [])[1]} ${r.finalUrl === other}`; },
  onerror: (r) => { document.title = `xhr error ${r.error}`; },
});
"#;

fn script(meta: &str, body: &str) -> String {
    format!("// ==UserScript==\n// @include *gmapi.html\n{meta}// ==/UserScript==\n{body}")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gm_xmlhttprequest_reaches_connect_hosts() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/xhr.user.js",
            &script("// @grant GM_xmlhttpRequest\n// @connect localhost\n", XHR),
        )
        .start("page.html");
    b.open("gmapi.html");
    b.wait_until("the cross-origin request returns", |s| {
        s.tab().title == "xhr 200 second true"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gm_xmlhttprequest_refuses_hosts_without_connect() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/xhr.user.js",
            &script("// @grant GM_xmlhttpRequest\n", XHR),
        )
        .start("page.html");
    b.open("gmapi.html");
    b.wait_until("the request is refused", |s| {
        s.tab().title == "xhr error not allowed"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gm_apis_need_a_grant() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/none.user.js",
            &script(
                "",
                "document.title = `${typeof GM_xmlhttpRequest} ${typeof GM_openInTab}`;",
            ),
        )
        .start("page.html");
    b.open("gmapi.html");
    b.wait_until("neither API is there", |s| {
        s.tab().title == "undefined undefined"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn gm_openintab_opens_a_background_tab() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/open.user.js",
            &script(
                "// @grant GM_openInTab\n",
                "GM_openInTab(location.href.replace(/[^/]*$/, 'second.html'), true);",
            ),
        )
        .start("page.html");
    b.open("gmapi.html");
    let second = b.url("second.html");
    let s = b.wait_until("the tab opens", |s| {
        s.tabs().iter().any(|t| t.url == second)
    });
    assert!(
        s.tab().url.ends_with("gmapi.html"),
        "it opened in the background"
    );
}
