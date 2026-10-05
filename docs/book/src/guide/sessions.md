# Sessions, history and bookmarks

## Where browsing data lives

| What | Where | Format |
|---|---|---|
| History | `<data>/history.sqlite` | SQLite; `:history-clear --force` empties it |
| Quickmarks | `<config>/quickmarks` | qutebrowser's: one `name url` per line |
| Bookmarks | `<config>/bookmarks/urls` | qutebrowser's: one `url title` per line |
| Sessions | `<data>/sessions/<name>.toml` | TOML |

Quickmarks and bookmarks sit next to the config, so you can keep them in dotfiles. Sessions keep each tab's current page; CEF cannot restore a tab's back/forward history.

## Sessions

`:session-save [name]`, `:session-load name` and `:session-delete name` manage sessions, and `:session-load` completes their names. `ZZ` or `:wq` saves the tabs as the `default` session and quits; `ZQ` quits without saving. With `auto_save.session = true`, the tabs are saved on quit and restored at the next start.

With `session.lazy_restore = true`, a restored session loads only the tab you're on. The others keep their titles in the tab bar and load when you first switch to them, which makes restoring many tabs fast.

`confirm_quit` asks before `:quit`, or before closing the last window, quits the browser. List the reasons to ask: `multiple-tabs` (more than one tab is open), `downloads` (downloads are still running), `always`, or the default `never`:

```toml
confirm_quit = ["multiple-tabs", "downloads"]
```

## Crash recovery

Every `auto_save.interval` milliseconds (15 s by default, `0` turns it off), the open tabs are saved for crash recovery. A normal exit deletes that save. If the browser crashed, the next start reopens those tabs; with URLs on the command line, it says where they are (`:session-load _autosave`).

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
