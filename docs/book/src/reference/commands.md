# Commands

<!-- Generated from the command and binding registries; regenerate with UPDATE_LUA_TYPES=1 cargo test -p rt-config. -->

Type these after `:`, or bind them to keys. `:help` shows the same list in the browser, with your own bindings.

| Command | Default keys | Description |
|---|---|---|
| `:open` | `<Ctrl-t>` `O` `PP` `Pp` `gO` `go` `o` `pP` `pp` | Open a URL or search for text |
| `:back` | `H` | Go back in history |
| `:forward` | `L` | Go forward in history |
| `:reload` | `<Ctrl-r>` `<F5>` `R` `r` | Reload the current page |
| `:stop` |  | Stop loading the current page |
| `:scroll` | `<Down>` `<Up>` `h` `j` `k` `l` | Scroll in a direction |
| `:scroll-page` | `<Ctrl-b>` `<Ctrl-d>` `<Ctrl-f>` `<Ctrl-u>` | Scroll by a multiple of the page size |
| `:scroll-to-perc` | `$` `0` `G` `gg` | Scroll to a percentage of the page |
| `:mode-enter` | `'` `<Ctrl-v>` `V` `` ` `` `i` `v` | Enter a key mode |
| `:mode-leave` |  | Leave the current mode |
| `:cmd-set-text` |  | Preset the command line text |
| `:tab-close` | `<Ctrl-w>` `d` | Close the current tab (--force: a pinned one without asking) |
| `:tab-pin` | `<Ctrl-p>` | Pin or unpin the current tab (count: tab number) |
| `:tab-next` | `<Ctrl-PgDown>` `J` | Switch to the next tab |
| `:tab-prev` | `<Ctrl-PgUp>` `K` `gT` | Switch to the previous tab |
| `:tab-focus` | `<Alt-1>` `<Alt-2>` `<Alt-3>` `<Alt-4>` `<Alt-5>` `<Alt-6>` `<Alt-7>` `<Alt-8>` `<Alt-9>` `<Ctrl-Tab>` `<Ctrl-^>` `g$` `g0` `g^` | Select a tab by number, or 'last' |
| `:tab-move` | `gJ` `gK` `gm` | Move the current tab: +, -, start, end or a number |
| `:tab-only` | `co` | Close all tabs except the current one |
| `:tab-clone` |  | Duplicate the current tab: :tab-clone [-b] [-w] |
| `:tab-give` | `gD` | Move the current tab to window N, or to a new window: :tab-give [N] |
| `:tab-call` |  | Reopen the current tab in a call window, where screen sharing picks a tab, window or screen |
| `:tab-take` |  | Move a tab from another window here: :tab-take &lt;window/tab&gt; |
| `:undo` | `<Ctrl-T>` `u` | Re-open the last closed tab |
| `:hint` | `;I` `;O` `;b` `;d` `;f` `;h` `;i` `;o` `;r` `;t` `;y` `F` `f` | Label elements to follow: [--rapid] [group] [target] [fill text] |
| `:yank` | `yD` `yT` `yY` `yd` `yt` `yy` | Copy the page's url, title or domain to the clipboard |
| `:set` |  | Show or change an option: :set name [value], :set name! toggles |
| `:bind` |  | Show or set a key binding: :bind [--mode m] keys [command] |
| `:unbind` |  | Remove a key binding: :unbind [--mode m] keys |
| `:config-source` |  | Reload the configuration files |
| `:help` | `<F1>` | Show help: :help [-t] [:command \| setting \| section] |
| `:version` |  | Show version, paths and loaded config files |
| `:report` |  | Report a bug: opens a new GitHub issue with the version filled in |
| `:debug-keytester` |  | Show the name and binding of each key you press, until Escape |
| `:debug-log-filter` |  | Change the log filter while running, e.g. rt_cef=debug; default goes back to RT_LOG |
| `:changelog` |  | Show what changed in each version: :changelog [-t] |
| `:quickmark-add` | `m` | Save a quickmark: :quickmark-add &lt;url&gt; &lt;name&gt; |
| `:quickmark-load` | `B` `b` | Open a quickmark: :quickmark-load [-t\|-b] &lt;name&gt; |
| `:quickmark-del` |  | Delete a quickmark (default: the current page's) |
| `:bookmark-add` | `M` | Bookmark a URL (default: the current page) |
| `:bookmark-load` | `gB` `gb` | Open a bookmark: :bookmark-load [-t\|-b] &lt;url&gt; |
| `:bookmark-del` |  | Delete a bookmark (default: the current page) |
| `:save` |  | Write config, cookies, quickmarks, bookmarks and the session to disk now: :save [what…] |
| `:session-save` |  | Save the open tabs: :session-save [name] |
| `:session-load` |  | Replace the open tabs with a saved session |
| `:session-delete` |  | Delete a saved session |
| `:history-clear` |  | Delete all browsing history (needs --force) |
| `:history-import` |  | Import qutebrowser's history: :history-import [path to history.sqlite] |
| `:adblock-update` |  | Download the filter lists in content.blocking.adblock.lists |
| `:spell-suggest` |  | Suggest fixes for the misspelled word at the cursor (insert mode) |
| `:spell-replace` |  | Replace the misspelled word: :spell-replace &lt;word&gt; |
| `:spell-add` |  | Add the word from the last :spell-suggest to your dictionary |
| `:spell-install` |  | Download spell-check dictionaries (checked against pinned checksums) and turn them on: :spell-install en-US de-DE, or pick from a list without a language |
| `:spawn` |  | Run a program: :spawn [-u] [-v] [-m] [-o] [-d] &lt;cmd&gt; [args]; -u runs a userscript |
| `:open-editor` |  | Edit the focused text field in editor.command (also :edit-text) |
| `:edit-text` |  | Edit the focused text field in editor.command (qutebrowser's name for :open-editor) |
| `:edit-url` |  | Edit the page's URL in editor.command, then open it: [-t\|-b\|-w\|-p] [-r] [url] |
| `:cmd-edit` |  | Edit the command line in editor.command, then put it back: [--run] runs it instead |
| `:greasemonkey-reload` |  | Read the scripts in the greasemonkey directories again |
| `:close` |  | Close the current window (:quit closes all of them) |
| `:tab-select` | `T` `gt` | Go to a tab in any window: :tab-select &lt;window/tab \| text&gt; (T) |
| `:history` |  | Show the browsing history: :history [-t] |
| `:settings` |  | Open the settings page, to browse and change every setting |
| `:plugins` |  | Show your plugins, their permissions and updates |
| `:pack-update` |  | Check plugins from git for new commits to review on :plugins: :pack-update [name] |
| `:recover` |  | Show the tabs open at each recent crash, to reopen some or all of them |
| `:crash-report` |  | Show the newest crash report, to check and send as a GitHub issue or by email |
| `:selection-follow` | `<Ctrl-Return>` `<Return>` | Follow the link around the selection, e.g. after a search (Return; -t: new tab) |
| `:zoom` | `=` | Set the zoom: :zoom [percent] (=; no value: zoom.default) |
| `:zoom-in` | `+` | Zoom in a level (+; a count zooms further) |
| `:zoom-out` | `-` | Zoom out a level (-) |
| `:devtools-focus` |  | Bring this tab's developer tools to the front |
| `:bookmark-list` |  | List quickmarks and bookmarks on a page: [-t] in a new tab |
| `:quickmark-save` |  | Write the quickmarks file now |
| `:quickmarks-reload` |  | Read the quickmarks and bookmarks files again |
| `:bookmarks-reload` |  | Read the quickmarks and bookmarks files again |
| `:debug-dump-page` |  | Save the page's HTML to a file: :debug-dump-page &lt;file&gt; |
| `:debug-clear-ssl-errors` |  | Forget the certificate errors allowed this session |
| `:restart` |  | Save the session, quit and start again |
| `:devtools` | `wi` | Open the developer tools for this tab (wi) |
| `:print` |  | Print the page, or save it: :print [--pdf file] |
| `:fullscreen` | `<F11>` | Toggle fullscreen (F11) |
| `:screenshot` |  | Save what the tab shows as an image: :screenshot [--force] file (.png, .jpg or .webp) |
| `:view-source` | `gf` | Show the page source in a new tab (gf) |
| `:jseval` |  | Evaluate a JavaScript expression in the page: :jseval &lt;code&gt; |
| `:home` |  | Open the start page |
| `:tab-mute` | `<Alt-m>` | Mute or unmute this tab (Alt-m) |
| `:pip` | `gp` | Float the page's main video in a picture-in-picture window, or bring it back (gp) |
| `:call-mute` | `cm` | Mute or unmute your microphone in the call, from any tab or window, with the site's own mute key (content.call_mute_keys) |
| `:share-stop` |  | Stop sharing your screen, a window or a tab, from any tab or window |
| `:messages` |  | Show this session's messages |
| `:repeat-command` | `.` | Run the last command again (.) |
| `:theme` |  | Switch the color theme (ui.theme), or list the themes: :theme [name] |
| `:cmd-repeat-last` |  | Run the last command again, as . does |
| `:cmd-repeat` |  | Run a command several times: :cmd-repeat N command |
| `:cmd-run-with-count` |  | Run a command with a count, multiplied by any count typed first: :cmd-run-with-count N command |
| `:scroll-px` |  | Scroll by pixels: :scroll-px &lt;dx&gt; &lt;dy&gt; |
| `:cmd-later` |  | Run a command later: :cmd-later &lt;ms&gt; &lt;command&gt; |
| `:message-info` |  | Show a message: :message-info &lt;text&gt; |
| `:message-warning` |  | Show a warning: :message-warning &lt;text&gt; |
| `:message-error` |  | Show an error: :message-error &lt;text&gt; |
| `:clear-messages` |  | Take the messages off the screen |
| `:config-cycle` |  | Cycle a setting: :config-cycle &lt;option&gt; [values…] (no values: toggle) |
| `:config-unset` |  | Put a setting back to its default, or forget its value for one site: :config-unset [-u pattern] &lt;option&gt; |
| `:config-list-add` |  | Add a value to a list setting: :config-list-add &lt;option&gt; &lt;value&gt; |
| `:config-list-remove` |  | Remove a value from a list setting: :config-list-remove &lt;option&gt; &lt;value&gt; |
| `:config-dict-add` |  | Set a key in a map setting: :config-dict-add [--replace] &lt;option&gt; &lt;key&gt; &lt;value&gt; |
| `:config-dict-remove` |  | Remove a key from a map setting: :config-dict-remove &lt;option&gt; &lt;key&gt; |
| `:config-clear` |  | Put every setting back to its default |
| `:config-diff` |  | Show the settings that differ from their defaults |
| `:config-edit` |  | Edit config.lua (or config.toml) in editor.command, then load it again |
| `:config-write-toml` |  | Write the current settings to config.toml: [--force] replaces an existing one |
| `:insert-text` |  | Type text into the focused field: :insert-text &lt;text&gt; |
| `:fake-key` |  | Send keys to the page: :fake-key [-g] &lt;keys&gt; (-g: to the browser) |
| `:click-element` |  | Click an element: :click-element id\|css\|focused [value] |
| `:scroll-to-anchor` |  | Scroll to the element with this id or name |
| `:window-only` |  | Close every other window |
| `:nop` |  | Do nothing (to make a key do nothing) |
| `:navigate` | `<Ctrl-a>` `<Ctrl-x>` `[[` `]]` `gU` `gu` `{{` `}}` | Go up, prev, next, increment or decrement the URL: :navigate &lt;where&gt; [-t] |
| `:search` |  | Find text in the page: :search [-r] [text] (no text clears it); / and ? type one |
| `:search-next` | `n` | Go to the next match of the last search |
| `:search-prev` | `N` | Go to the previous match of the last search |
| `:set-mark` |  | Remember the scroll position as a mark: a-z for this page, A-Z with its URL |
| `:jump-mark` |  | Go back to a mark; ' is where the last jump started |
| `:macro-record` | `q` | Record keys into a register until macro-record again (q + register) |
| `:macro-run` | `@` | Replay a macro (@ + register; @@ repeats the last one; a count repeats it) |
| `:selection-toggle` |  | Start or stop selecting in caret mode (--line selects whole lines) |
| `:selection-reverse` |  | Swap the ends of the selection (caret mode) |
| `:download` |  | Download a URL (default: the current page) |
| `:download-cancel` |  | Cancel a download (count: its number) |
| `:download-open` |  | Open a finished download (count: its number) |
| `:download-clear` |  | Remove finished downloads from the list |
| `:download-retry` |  | Start a failed or cancelled download again (count: its number) |
| `:download-remove` |  | Take a download off the list, cancelling it if it runs (count: its number; --all: every finished one) |
| `:download-delete` |  | Delete a finished download's file and take it off the list (count: its number) |
| `:downloads` |  | List this session's downloads and their progress |
| `:quit` | `<Ctrl-q>` `ZQ` `ZZ` | Quit the browser; --save keeps the tabs as the default session |
| `:prompt-fileselect-external` |  | In a file prompt, pick the folder with fileselect.folder.command (Alt-e) |
| `:hint-follow` |  | Follow the hint with this label, or the match waiting for Return (Return in hint mode) |
| `:completion-item-del` |  | Delete the selected completion: history entry, quickmark, bookmark or session, or close the tab (Ctrl-d) |
| `:completion-item-yank` |  | Yank the selected completion's text: [--sel] for the primary selection (Ctrl-c) |
