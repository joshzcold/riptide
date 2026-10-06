# From the terminal

While the browser is running, `riptide` hands its arguments to that instance (per profile, so `--basedir` instances stay separate) and exits:

```sh
riptide https://example.com        # opens per new_instance_open_target (default: new tab)
riptide --target tab-bg notes.html # relative files become file:// URLs
riptide ':tab-focus 1' ':reload'   # arguments starting with ':' run as commands
```

With several windows open, URLs go to the one you used last; `new_instance_open_target_window` can pick the `first-opened` or `last-opened` window instead.

The browser listens on a Unix socket in `$XDG_RUNTIME_DIR/riptide/` (or the data directory), inside a `0700` directory and with `0600` permissions, so only your user can send commands. On Windows each start is a new instance for now.

## Internal pages

The tab bar, status bar and overlay are HTML pages served from the browser itself at `riptide://ui/…`. Web pages can't link to, frame or redirect to `riptide://` addresses, and only `riptide://ui/` pages get the `rt.send()` channel to Rust. The browser accepts only the messages each page is allowed to send.

Pages you can open: `riptide://help/` (`:help`), `riptide://history/` (`:history`), `riptide://downloads/` (`:downloads`) and `riptide://changelog/` (`:changelog`). The first start after an update opens the changelog in a background tab when the version's minor or major number changed; `changelog_after_upgrade` (`major`, `minor`, `patch` or `never`) sets how big a step that takes.
