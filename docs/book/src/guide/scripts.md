# Programs, userscripts and Greasemonkey

## Programs and userscripts

- `:spawn [-v] [-m] [-o] [-d] <cmd> [args]` runs a program, with arguments split like a shell would (no shell runs). `{url}` is the current page. `-v` reports success too, `-m` shows the program's output as messages, `-o` shows it in a new tab (`riptide://process/`), and `-d` detaches. A non-zero exit is shown as an error. For example, `rt.bind(",m", "spawn -d mpv {url}")`.
- `:spawn -u <name>` runs a **userscript**. It is looked up in `<config>/userscripts/`, then `<data>/userscripts/`, then `PATH`. It gets `RIPTIDE_URL`, `RIPTIDE_CURRENT_URL`, `RIPTIDE_TITLE`, `RIPTIDE_SELECTED_TEXT`, `RIPTIDE_SELECTED_HTML`, `RIPTIDE_HTML`/`RIPTIDE_TEXT` (files with the page's HTML and text), `RIPTIDE_TAB_INDEX`, `RIPTIDE_COUNT`, `RIPTIDE_MODE`, `RIPTIDE_USER_AGENT`, `RIPTIDE_CONFIG_DIR`, `RIPTIDE_DATA_DIR`, `RIPTIDE_DOWNLOAD_DIR` and `RIPTIDE_VERSION`. Commands it writes to `RIPTIDE_FIFO`, one per line, run when it exits. A one-word argument is unquoted as qutebrowser does it, so `message-info 'two words'` and `fake-key \a` work.
- **qutebrowser's userscripts** use the same variables named `QUTE_*`. Rename them to `RIPTIDE_*` and they run as they are: the commands they send (`fake-key` with escaped characters, `jseval -q`, `mode-enter insert`, `message-*`, `open -t`, `config-dict-add`) all exist. For a script you'd rather not edit, a small wrapper can copy the variables:

  ```sh
  #!/bin/sh
  # riptide userscript that runs a qutebrowser one
  for v in $(env | sed -n 's/^RIPTIDE_\([A-Z_]*\)=.*/\1/p'); do
      export "QUTE_$v=$(printenv "RIPTIDE_$v")"
  done
  exec /path/to/qute-pass "$@"
  ```

  `format_json` was checked this way. The password scripts haven't been run against a real password store yet.
- Hints can run them on a link: `:hint links spawn mpv {hint-url}` (the URL is appended if there's no `{hint-url}`), or `:hint links userscript name`, which gets the link as `RIPTIDE_URL` and `RIPTIDE_MODE=hints`. For example, `rt.bind(";m", "hint links spawn mpv")`.
- `:open-editor`, or `Ctrl-e` in insert mode, edits the focused text field in `editor.command` (default `gvim -f {file} -c "normal {line}G{column0}l"`, as in qutebrowser). The text is written back when the editor exits successfully. For a terminal editor: `c.editor.command = { "foot", "nvim", "+call cursor({line}, {column})", "{file}" }`. `:edit-text` is the same command under qutebrowser's name.
- `:edit-url` edits the page's address in the editor and opens what you save. It takes `:open`'s flags, so `:edit-url -t` opens the result in a new tab.
- `:cmd-edit` edits the command line you're typing in the editor and puts it back, or runs it with `--run`. It works from command mode, so bind it there: `rt.bind("<Ctrl-x>", "cmd-edit", "command")`.

## Greasemonkey scripts

`*.js` files in `<data>/greasemonkey/` (as in qutebrowser) or `<config>/greasemonkey/` run in matching pages. They follow the usual `// ==UserScript==` block: `@match`, `@include`, `@exclude`, `@run-at` (`document-start`, `document-end` (the default) or `document-idle`) and `@noframes`. Scripts get `GM_info`, `GM_addStyle`, `unsafeWindow`, and `GM_getValue`, `GM_setValue`, `GM_deleteValue` and `GM_listValues`, plus the promise versions under `GM.*`. Values are kept per script and survive restarts; a page sees the values as they were when it loaded. `@require` URLs are downloaded once, into the data directory, and run before the script. A new `@require` is fetched in the background; reload the page once it says the download is done. `GM_xmlhttpRequest` and `GM_openInTab` aren't there yet. `:greasemonkey-reload` reads the files again; reload a page to run the new versions.
