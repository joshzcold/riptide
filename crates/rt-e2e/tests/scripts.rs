//! External programs, userscripts, the editor and Greasemonkey scripts.
#![cfg(unix)]

use rt_e2e::Browser;

/// Records what qutebrowser-style userscripts get, and opens a page through the FIFO.
const USERSCRIPT: &str = r#"#!/bin/sh
printf '%s|%s|%s|%s' "$RIPTIDE_URL" "$RIPTIDE_MODE" "$RIPTIDE_CURRENT_URL" "${QUTE_URL:-}" >"{scratch}/us.out"
echo "open -t {server}/second.html" >>"$RIPTIDE_FIFO"
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_userscript_gets_qute_variables_and_runs_its_fifo_commands() {
    let b = Browser::launch()
        .script("config/userscripts/us", USERSCRIPT)
        .start("page.html");
    let page = b.url("page.html");
    b.run("spawn -u us");
    let out = b.wait_file(&b.scratch().join("us.out"));
    assert_eq!(out, format!("{page}|command|{page}|"));
    let second = b.url("second.html");
    b.wait_until("the FIFO's :open ran", |s| s.tab().is_loaded(&second));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_run_a_userscript_or_a_program_on_a_link() {
    let b = Browser::launch()
        .script("config/userscripts/us", USERSCRIPT)
        .start("nav1.html");
    let (nav1, nav2) = (b.url("nav1.html"), b.url("nav2.html"));
    b.follow_hint("hint links userscript us", |h| h.text.starts_with("next"));
    let out = b.wait_file(&b.scratch().join("us.out"));
    assert_eq!(out, format!("{nav2}|hints|{nav1}|"));

    b.open("nav1.html");
    let hinted = b.scratch().join("hinted");
    b.follow_hint(
        &format!(
            "hint links spawn sh -c 'echo \"$1\" > {}' sh",
            hinted.display()
        ),
        |h| h.text.starts_with("next"),
    );
    assert_eq!(b.wait_file(&hinted).trim(), nav2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn spawn_o_shows_the_output_in_a_new_tab() {
    let b = Browser::start("page.html");
    b.run("spawn -o echo hello from spawn");
    b.wait_until("the output tab opens", |s| {
        s.tabs().len() == 2 && s.tab().title == "echo output"
    });
    b.wait_eval(
        "String(document.body.innerText.includes('hello from spawn'))",
        "true",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ctrl_e_edits_a_text_field_in_editor_command() {
    let b = Browser::launch()
        .script(
            "config/editor.sh",
            "#!/bin/sh\nprintf 'edited text\\n' >\"$1\"\n",
        )
        .lua("c.editor.command = { rt.config_dir .. \"/editor.sh\", \"{file}\" }\n")
        .start("editor.html");
    b.follow_hint("hint inputs", |_| true);
    b.wait_mode("insert");
    b.keys("abc");
    b.wait_eval("document.title", "v=abc");
    b.keys("<Ctrl-e>");
    b.wait_eval("document.title", "v=edited text");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn greasemonkey_scripts_run_at_document_start_and_end() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/start.user.js",
            "// ==UserScript==\n// @name   Start\n// @include http://127.0.0.1:*/gm.html\n\
             // @run-at document-start\n// ==/UserScript==\nwindow.__gmStart = GM_info.script.name;\n",
        )
        .file(
            "data/greasemonkey/end.user.js",
            "// ==UserScript==\n// @include http://127.0.0.1:*/gm.html\n// ==/UserScript==\n\
             GM_addStyle('body { color: rgb(1, 2, 3); }');\n\
             document.title += ' end=' + getComputedStyle(document.body).color;\n",
        )
        .start("page.html");
    b.open("gm.html");
    b.wait_eval("document.title", "start=Start end=rgb(1, 2, 3)");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn greasemonkey_values_persist_and_require_runs_first() {
    let b = Browser::launch()
        .file(
            "data/greasemonkey/values.user.js",
            "// ==UserScript==\n// @name    Counter\n// @include http://127.0.0.1:*/gmvalues.html\n\
             // @require {server}/lib.js\n// ==/UserScript==\n\
             const n = GM_getValue('n', 0) + 1;\nGM_setValue('n', n);\n\
             document.title = 'gm n=' + n + ' lib=' + (typeof rtLib === 'undefined' ? 'no' : rtLib);\n",
        )
        .start("page.html");
    b.open("gmvalues.html");
    b.wait_eval("document.title", "gm n=1 lib=loaded");
    b.run("reload");
    b.wait_eval("document.title", "gm n=2 lib=loaded");
}
