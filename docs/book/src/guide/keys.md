# Keys and modes

Riptide is modal, like vim and qutebrowser. Keys do different things depending on the mode, which the status bar shows:

| Mode | What keys do | Enter with | Leave with |
|---|---|---|---|
| normal | Run commands: scroll, follow links, switch tabs | `Escape` from any other mode | — |
| insert | Go to the page, for typing into text fields | `i`, or automatically when a text field gets focus | `Escape` |
| command | Edit a `:command` line | `:` (or `o`, `O`, `go`…) | `Return` runs it, `Escape` cancels |
| hint | Pick an element by its label | `f`, `F`, `;y`… | choosing a label, or `Escape` |
| caret | Move a text cursor and select text | `v` / `V` | `Escape` |
| passthrough | Every key goes to the page | `Ctrl-v` | `Shift-Escape` |

Most keys take a count first: `5j` scrolls five times, `3J` moves three tabs right, `50G` jumps to 50%.

## Common keys

This is a tour, not the whole list. The [default key bindings](../reference/bindings.md) page has every binding in every mode, and `:help bindings` shows yours.

| Keys | Command |
|---|---|
| `j` `k` `h` `l` | `scroll down/up/left/right` (accept a count, e.g. `5j`) |
| `gg` / `G` | `scroll-to-perc 0` / `scroll-to-perc` (`50G` = 50%) |
| `0` / `$` | Scroll to the far left / right |
| `Ctrl-d` `Ctrl-u` | Half page down / up |
| `Ctrl-f` `Ctrl-b` | Full page down / up |
| `H` / `L` | `back` / `forward` |
| `r` / `R` | `reload` / `reload -f` |
| `o` / `O` | `:open ` / `:open -t ` (new tab) |
| `go` / `gO` | Edit the current URL, in this tab / a new tab |
| `Ctrl-t` | `open -t` (start page in a new tab) |
| `J` `K`, `gT` | `tab-next` / `tab-prev` |
| `Alt-1`…`Alt-9`, `g0` `g$` | `tab-focus N` / first / last (a count also works, e.g. `3J`) |
| `Ctrl-Tab`, `Ctrl-^` | `tab-focus last` (previously focused tab) |
| `gt`, `T` | `:tab-select`: a list of the open tabs, this window's first (by number; other windows' as `window/tab`). Type a number to pick that tab, or words to filter by title or URL, then `Return` |
| `gD` | `tab-give`: move the tab to a new window |
| `:tab-clone [-b] [-w]`, `:tab-give [N]`, `:tab-take W/T` | Duplicate the tab (in the background / a new window), move it to window N or a new window, or bring a tab here from another window. Pages are reopened, so their back/forward history stays behind. |
| `d`, `Ctrl-w` | `tab-close` |
| `u`, `Ctrl-Shift-t` | `undo` (reopen the last closed tab where it was) |
| `gJ` `gK`, `gm` | `tab-move +` / `-` / to the start (or to the count) |
| `co` | `tab-only` (keeps pinned tabs; `:tab-only --force` closes them too) |
| `Ctrl-p` | `tab-pin`: pin or unpin the tab (a count picks one, e.g. `3 Ctrl-p`) |
| `f` / `F` / `;b` | Hint elements; click / open in a new tab / open in a background tab. With `hints.mode = "number"`, labels are numbers and typing letters narrows the elements by their text (a single match is followed). Elements inside iframes (from any site, such as Gmail's Chat) and inside web components' shadow DOM are hinted too. |
| `;y` / `;h` / `;t` | Hint a link to yank / an element to hover / an input to focus |
| `;i` / `;I` | Hint an image; open it here / in a new tab |
| `;o` / `;O` | Hint a link and put `:open` (or `:open -t`) with its URL on the command line |
| `;r` | Rapid hinting: open several links in background tabs (leave with `Escape`) |
| `yy` / `yt` / `yd` | Yank the URL / title / domain (`yY` `yT` `yD`: to the primary selection). Tracking parameters in `url.yank_ignored_parameters` (`utm_*`, `fbclid`…) are left out of the URL. |
| `pp` / `Pp` | Open the clipboard contents here / in a new tab (`pP` / `PP`: the primary selection) |
| `m` | Quickmark this page (type a name, then `Return`) |
| `b` / `B` | Open a quickmark here / in a new tab |
| `M` | Bookmark this page |
| `gb` / `gB` | Open a bookmark here / in a new tab |
| `` `a `` / `'a` | Set / jump to mark `a`: `a`–`z` remember this page's scroll position, `A`–`Z` also the page itself; `''` returns to where the last jump started |
| `Ctrl-e` (insert mode) | `open-editor`: edit the text field in `editor.command` |
| `gu` / `gU` | `navigate up`: one level up the URL, here / in a new tab (a count goes further) |
| `[[` `]]` / `{{` `}}` | `navigate prev` / `next`: follow the page's previous/next link (`rel` links, or link text matching `hints.prev_regexes` / `hints.next_regexes`, such as "Next »"), here / in a new tab |
| `Ctrl-a` / `Ctrl-x` | `navigate increment` / `decrement`: change the last number in the URL (`page/9` → `page/10`) |
| `Return` / `Ctrl-Return` | `selection-follow`: follow the link a search found (or the focused link) here / in a new tab; otherwise the page gets the key |
| `/` `?` then `n` `N` | Find text in the page forward / backward, then go to the next / previous match. Matches highlight as you type (`search.incremental`); case is ignored unless the text has a capital (`search.ignore_case`). `:search` with no text clears it. |
| `v` / `V` | Caret mode: move with `h` `j` `k` `l` `w` `b` `e` `0` `$` `gg` `G`, and between blocks with `[` `]` (start of the previous/next) and `{` `}` (end of the previous/next). Select with `v` (or `V` for lines), drop the selection with `Ctrl-Space`, swap the ends with `o`, yank with `y` (`Y`: to the primary selection), leave with `Escape` |
| `qa` … `q` / `@a` | Record a macro into register `a` / replay it (`@@` repeats the last one, `3@a` runs it three times). Keys typed into pages are replayed too. |
| `F1`, `:help [topic]` | Help: every command, setting (with its current value and where it was set) and key binding, generated from the running browser. `:help :open`, `:help hints.chars`, `:help bindings` jump to an entry; `/` searches. |
| `:version` | Version, git commit, CEF/Chromium versions, paths and loaded config files |
| `:history [-t]` | Browsing history by day, with a search box |
| `:history-import [path]` | Import qutebrowser's `history.sqlite` (default: qutebrowser's data directory); importing twice adds nothing new |
| `:q`, `:qa` | Close this window (closing the last one quits) / quit, closing every window. As in qutebrowser, these are aliases for `:close` and `:quit` (`aliases`). |
| `ZZ`, `:wq` | Save the tabs as the `default` session and quit (`ZQ` quits without saving) |
| `:` | Command line |
| `i` | Insert mode (also entered automatically when a text field gets focus) |
| `Ctrl-v` | Passthrough mode (leave with `Shift-Escape`) |
| `Escape` | Leave insert mode, or clear a pending key sequence |
| `ZQ` `ZZ` `Ctrl-q` | `quit` |

## Opening pages

`o` (`:open`) takes an address or words to search for. Text that looks like an address (`example.org`, `localhost:8080`) opens; anything else goes to the `DEFAULT` search engine in `url.searchengines`. Start with an engine's name to use it instead:

```toml
[url.searchengines]
DEFAULT = "https://duckduckgo.com/?q={}"
w = "https://en.wikipedia.org/w/index.php?search={}"
```

`:open w riptide` searches Wikipedia. `url.auto_search` changes when text is searched: `schemeless` searches everything without `https://` or another scheme, and `never` opens it as an address unless it starts with an engine's name. With `url.open_base_url = true`, `:open w` alone opens Wikipedia's home page.

`Ctrl-a` and `Ctrl-x` add or take one from the last number in the URL's path or query (`page/9` → `page/10`). `url.incdec_segments` picks which parts they look at: `host`, `port`, `path`, `query`, `anchor`.

## Searching, scrolling and zoom

`/` and `?` search the page, and `n`/`N` move between matches. Searches go on from the top after the last match, saying so, unless you set `search.wrap = false` (they stop at the last match) or `search.wrap_messages = false` (they wrap quietly). `search.ignore_case` (`smart`, `always`, `never`) and `search.incremental` set how matching works.

`scrolling.smooth = true` animates scrolling by keys, and `scrolling.bar` shows page scrollbars `always`, `never` or as thin `overlay` ones (after a restart). `+` and `-` step through `zoom.levels`, and `=` goes back to `zoom.default`:

```toml
"zoom.levels" = ["50%", "75%", "100%", "125%", "150%", "200%"]
```

## Insert mode and key mappings

Clicking into a text field enters insert mode (`input.insert_mode.auto_enter`), and loading a new page leaves it (`input.insert_mode.leave_on_load`). A field the page focuses by itself, like a search box with `autofocus`, doesn't take insert mode unless you set `input.insert_mode.auto_load = true`. Your keys keep working in normal mode until you click or press `i`.

`input.mode_override` picks the mode a site's pages start in, and when you switch to their tab. Set it per site, e.g. `:set -u ssh.example.com input.mode_override passthrough` for a web terminal.

Digits before a binding are a count (`3j`); `input.match_counts = false` lets you bind digits themselves. A half-typed chain like `g` waits for the next key, or for `input.partial_timeout` milliseconds if you set one. `:cmd-repeat 3 tab-next` runs a command several times and `:cmd-run-with-count 3 tab-focus` gives it a count, which helps in bindings. `:debug-keytester` shows the name and binding of each key you press, until `Escape`, which helps when writing bindings.

`bindings.key_mappings` treats one key as another in every mode, before bindings are looked up. By default `Ctrl-[` is `Escape`, `Ctrl-m` and `Ctrl-j` are `Return`, `Ctrl-i` is `Tab` and `Ctrl-6` is `Ctrl-^`. To add your own, include the defaults you want to keep:

```toml
[bindings.key_mappings]
"<Ctrl-[>" = "<Escape>"
"<Ctrl-m>" = "<Return>"
"<Ctrl-g>" = "<Escape>"
```

The mouse's back and forward buttons go back and forward. With `input.mouse.rocker_gestures = true`, holding the right button and clicking the left goes back, and the other way round goes forward; pages lose their context menu.

`input.spatial_navigation = true` moves focus between links and fields with the arrow keys in passthrough and insert mode. `input.media_keys = false` stops the keyboard's play and pause keys from controlling pages. Both apply after a restart.

## Hints

`:hint [group] [target]` labels elements and acts on the one you pick. `f` is `:hint`, and `;y` is `:hint links yank`. The groups come from `hints.selectors`:
- **Built in:** `all`, `links`, `images`, `media` and `inputs`.
- **Your own:** add a group with a CSS selector list. Your groups are added to the built-in ones, which stay.

```lua
c.hints.selectors = { code = "pre, code" }
rt.bind(";c", "hint code yank")
```

When a hint is followed is set by `hints.auto_follow`:

| Value | Follows |
|---|---|
| `unique-match` (default) | As soon as one hint is left |
| `full-match` | Only when you type a whole label, not when number-mode text narrows to one |
| `always` | Either way |
| `never` | Only when you press `Return` (`:hint-follow`) |

Labels use `hints.chars`. `hints.min_chars` makes them at least that long, and `hints.scatter = false` hands them out in order instead of spreading them over the alphabet. A page that starts loading ends hint mode unless `hints.leave_on_load = false`.

`hints.mode = "word"` labels each link with a word from its own text or URL, so you type `news` for a News link. Words come from `hints.dictionary` (default `/usr/share/dict/words`); links without a usable word get the shortest unused one. In rapid mode (`:hint --rapid`), `hints.hide_unmatched_rapid_hints = false` keeps the labels you're not typing on screen.

`hints.auto_follow_timeout` ignores keys for a moment after a hint is followed, so a fast second keystroke doesn't land in the page. `:hint --rapid` (`;r`) keeps the labels up after each pick.

## Key hints

Type the start of a key chain, like `g` or `;`, and pause: after `keyhint.delay` (500 ms), a popup lists every binding the keys can still become, with its command. Finish the chain to run it, or press `Escape`.

To leave chains out of the popup, add globs to `keyhint.blacklist`. They match the whole chain:

```toml
keyhint.blacklist = ["<Ctrl-x>*", "g$"]
```

## The command line

In the command line, `Tab` / `Shift-Tab` cycle through completions. `:open` completes from search engines, quickmarks, bookmarks, history and, for paths starting with `/` or `~/`, files. Every typed word must match, in any order.

In `:open`, history, bookmarks, quickmarks and tabs show each site's icon (remembered from your visits, but not from private windows). Command names, settings, setting values and themes match anywhere in the name: `:set hints` offers `hints.chars` and also `colors.hints.bg`. Names that start with what you typed come first, then names with a part that starts with it (after a `.`, `-` or `_`), then the rest. When no name contains what you typed, its letters in order still match, so `:set clrhnt` offers `colors.hints.*`; names with fewer gaps come first. The matched text is highlighted in the list (`colors.completion.match.fg`).

| Setting | What it does |
|---|---|
| `completion.open_categories` | Which of those `:open` offers, in order |
| `completion.web_history.exclude` | URL globs never suggested from history, e.g. `["*://*.bank.example/*"]` |
| `completion.show` | `always`, only after pressing `Tab` (`auto`), or `never` |
| `completion.height` | Rows (`12`) or a share of the window (`50%`) |
| `completion.min_chars` | Characters to type after the command before its arguments complete |
| `completion.cmd_history_max_items` | How many command lines `Up` and `Down` remember |
| `completion.shrink` | `false` keeps the list `completion.height` tall however few items it has |
| `completion.timestamp_format` | When each history entry was last visited, e.g. `%d %b %H:%M`; empty hides it |
| `completion.delay` | Milliseconds to wait after a key before updating the list, for slow history searches |
| `completion.quick` | With one command or setting name left, `Tab` takes it and goes on to its arguments |
| `ui.overlay.position` | `floating` shows the command and its list in a box near the top of the page ([Themes](../configuration/themes.md#floating-command-line)) |
| `completion.use_best_match` | `Return` on an unknown command runs the first one it starts, so `:rel` runs `:reload` |
 `:set`, `:quickmark-load`, `:bookmark-load` and `:session-load` complete their own names. `:session-save [name]`, `:session-load name` and `:session-delete name` manage sessions. With `auto_save.session = true`, the tabs are saved on quit and restored at the next start.

The command line and prompts support readline keys (`Ctrl-a/e/u/k/w/h`, `Alt-b`/`Alt-f` by word, `Alt-d`/`Alt-Backspace` to delete a word, `Ctrl-y` to paste what was last deleted, `Ctrl-v` to paste the clipboard and `Shift-Insert` the primary selection, arrows), history (`Up`/`Down`), command chaining with `;;`, and completion of command names.

To clean up what completion offers, select an entry with `Tab` and press `Ctrl-d` (`:completion-item-del`). It deletes history entries, quickmarks, bookmarks and sessions, and closes tabs listed by `T`. `Ctrl-c` (`:completion-item-yank`) copies the selected entry, and `Ctrl-Shift-c` copies it to the primary selection.

Every command is listed in the [commands reference](../reference/commands.md). `.` repeats the last command, and `:messages` shows earlier status bar messages.

## The status bar

The status bar shows the mode, messages and the command line on the left. On the right are widgets, in the order `statusbar.widgets` lists them:

| Widget | Shows |
|---|---|
| `keypress` | Keys typed so far, and the count |
| `url` | The page's address, green for HTTPS |
| `scroll` / `scroll_raw` | How far down the page you are: `[Top]`, `[42%]`, `[Bot]`, `[All]` / just the number |
| `history` | `[<]` and `[>]` when you can go back or forward |
| `tabs` | `[current/total]` |
| `progress` | Loading progress |
| `search_match` | `Match [2/14]` after a `/` search |
| `downloads`, `muted`, `zoom` | Running downloads, a muted tab, and a zoom other than 100% |
| `media` | What the tab captures: `[V]`, `[A]`, `[A/V]`, `[Share]` |
| `sharing` | `[sharing your screen]` (or a window or tab) while any tab shares; `:share-stop` stops it |
| `clock`, `clock:%a %H:%M` | The time, in an optional strftime format |
| `text:…` | Fixed text |
| `lua:<name>` | Text from `config.lua` or a plugin; see [Status bar widgets](../configuration/lua.md#status-bar-widgets) |

```toml
statusbar.widgets = ["keypress", "url", "scroll", "tabs", "clock:%H:%M"]
```

`statusbar.position` puts the bar at the `top` or `bottom`. `statusbar.show` hides it:

- `never` keeps it hidden except while you type a command or answer a prompt, since those happen in the bar.
- `in-mode` also shows it outside normal mode, and while a message is up.
