# Sessions, history and bookmarks

## Where browsing data lives

| What | Where | Format |
|---|---|---|
| History | `<data>/history.sqlite` | SQLite; `:history-clear --force` empties it |
| Quickmarks | `<config>/quickmarks` | qutebrowser's: one `name url` per line |
| Bookmarks | `<config>/bookmarks/urls` | qutebrowser's: one `url title` per line |
| Sessions | `<data>/sessions/<name>.toml` | TOML |

Quickmarks and bookmarks sit next to the config, so you can keep them in dotfiles. Sessions keep each tab's page, its back/forward history (up to 50 pages each way) and how far it was scrolled.

In a restored tab, `H` and `L` load the saved pages again, so they come from the network rather than the cache, and anything typed into forms on them is gone. The scroll position is the one at the last autosave (`auto_save.interval`).

## Sessions

`:session-save [name]`, `:session-load name` and `:session-delete name` manage sessions, and `:session-load` completes their names. `ZZ` or `:wq` saves the tabs and quits; `ZQ` quits without saving. With `auto_save.session = true`, the tabs are saved on quit and restored at the next start.

Unnamed saves and the restore at startup use `session.default_name`. Left empty (the default), that's the session you last loaded with `:session-load`, or `default` if you haven't loaded one, so after `:session-load work` a `:wq` saves `work`.

`:save` writes everything to disk now: `config`, `cookies`, `quickmarks`, `bookmarks` and the `session`. Name some of them to save only those, e.g. `:save cookies session`.

With `session.lazy_restore = true`, a restored session loads only the tab you're on. The others keep their titles in the tab bar and load when you first switch to them, which makes restoring many tabs fast.

`confirm_quit` asks before `:quit`, or before closing the last window, quits the browser. List the reasons to ask: `multiple-tabs` (more than one tab is open), `downloads` (downloads are still running), `always`, or the default `never`:

```toml
confirm_quit = ["multiple-tabs", "downloads"]
```

## Crash recovery

Every `auto_save.interval` milliseconds (15 s by default, `0` turns it off), the open tabs are saved for crash recovery. Quitting riptide deletes that save. Being stopped from outside keeps it, like a crash: Ctrl-C in its terminal, the terminal closing, `kill`, or logging out.

After a crash, the next start keeps the saved tabs as a session named after the time of the crash, such as `_crashed-2026-10-06-115803` (UTC). The last five crashes are kept, and `:session-load _crashed-` completes their names.

| Start after a crash | What happens |
|---|---|
| Plain `riptide` | The crashed tabs reopen. |
| With URLs on the command line | Only those URLs open. A message names the session holding the crashed tabs, and the new run's autosaves don't touch it. |
| The browser crashed again within a minute of reopening a crash's tabs | They aren't reopened a second time, in case they caused the crash. The start page opens, and a message names the session to load them from when you choose. |

### Crash reports

When riptide itself crashes because of a bug (a Rust panic), it writes a report to `crashes/` in its data directory (`riptide --paths` shows where). The report has the version, the error and a backtrace, and no URLs or page content. The next start shows a message with the report's path. The newest ten reports are kept, and nothing is sent anywhere unless you send it.

`:crash-report` shows the newest report in a tab, where you can edit it before sending:

- **Open a GitHub issue:** fills in a new issue with the report. A report too long for a link is shortened, and the page asks you to attach the file.
- **Email it:** opens your mail program with the report. It only appears when `crash_report.email` is set.
- **Copy:** copies the report.

Backtraces name the source files of the build, which for a build of your own include its directory.

### A crashed tab

Pages run in processes separate from the browser. If a page's process crashes, runs out of memory or is killed, the browser keeps running and the tab shows a notice saying why. `r` reloads the page, and the tab keeps its back and forward history.

## History

`:history` (`-t` for a new tab) shows your browsing history by day, with a search box. `:open` completes from history as you type, with every typed word matching somewhere in the title or URL. `completion.web_history.max_items` sets how many entries completion shows. `:history-clear --force` empties it, and `:history-import` brings in qutebrowser's.

## Quickmarks and bookmarks

| Keys | Does |
|---|---|
| `m` | Quickmark this page (type a name, then `Return`) |
| `b` / `B` | Open a quickmark here / in a new tab |
| `M` | Bookmark this page |
| `gb` / `gB` | Open a bookmark here / in a new tab |

`:quickmark-add`, `:quickmark-del`, `:bookmark-add` and `:bookmark-del` do the same from the command line.

`:bookmark-list` shows your quickmarks and bookmarks on a page (`-t` in a new tab). If you edit the `quickmarks` or `bookmarks/urls` files by hand, `:quickmarks-reload` (or `:bookmarks-reload`) reads them again. `:quickmark-save` writes the quickmarks file, although every change is saved anyway.

## Restarting

`:restart` saves your tabs, quits, and starts riptide again with the same config and data directories, then restores them. Use it after changing a setting that only applies at startup, like `colors.webpage.darkmode.enabled`.
