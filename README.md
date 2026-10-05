<img src="packaging/riptide.svg" alt="" width="96" align="right">

# Riptide

*Surfing the web really fast.* A modern browser with vim-like bindings, using Rust and CEF.

A keyboard-driven browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate.

**Status:** early prototype, Linux/X11 only, not ready for daily browsing. See the plan for what's done and what's missing.

What works today:

- **Keyboard first:** normal, insert, command, hint, caret and passthrough modes, with qutebrowser's bindings. Also counts, marks, macros, `/` search, `:navigate`, and a command line with history and completion.
- **Tabs and windows:** pinned tabs, a tab bar that works with the mouse, favicons, `:tab-select`, moving tabs between windows, and private windows.
- **Hints:** for links, inputs, images, yanking and downloads, including number hints and same-origin iframes.
- **Privacy:** an Adblock Plus engine (EasyList and EasyPrivacy), Google background calls turned off, the Chromium sandbox where Linux allows it, and per-site permissions and certificate decisions.
- **Configuration:** `config.toml`, or `config.lua` with full scripting (functions on keys, custom commands, event hooks). Live `:set`, per-site settings, and a generated `:help` page.
- **qutebrowser compatibility:** quickmarks and bookmarks files, userscripts (`QUTE_*`), Greasemonkey scripts, `:open-editor`, and `:history-import`.
- **Page tools:** zoom (`+` `-` `=`), DevTools (`wi`), print or save as PDF, fullscreen, view source (`gf`), `:jseval`, tab muting, `:messages`, and `.` to repeat the last command.
- **Everything else:** sessions with crash recovery, history and downloads pages, spell checking with keyboard-driven fixes, dark mode, opt-in Widevine, and handing commands to a running browser from the terminal (`riptide ':open -t x'`).

## Documentation

**<https://joshzcold.github.io/riptide/>**, built from [`docs/book/`](docs/book/src/SUMMARY.md):

- [Installing](docs/book/src/guide/installing.md), including the Linux sandbox setup
- [Moving from qutebrowser](docs/book/src/guide/qutebrowser.md)
- [Keys and modes](docs/book/src/guide/keys.md) and [configuration](docs/book/src/configuration/files.md)
- Reference: [commands](docs/book/src/reference/commands.md), [key bindings](docs/book/src/reference/bindings.md), [settings](docs/settings.md), [Lua API](docs/book/src/reference/lua-api.md)
- [Developer guide](docs/book/src/dev/building.md)

In the browser, `F1` or `:help` shows every command, setting and binding with your own config applied. The roadmap is in [docs/PLAN.md](docs/PLAN.md).

## Quick start

Requirements: Rust 1.88+ (edition 2024).

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
./task check          # lint, unit tests and the smoke test (needs Xvfb and xdotool)
./task docs-serve     # preview the documentation at http://localhost:3000
```

`./task` downloads a pinned, checksum-verified [Task](https://taskfile.dev) if you don't have it. [Building](docs/book/src/dev/building.md) covers the rest, including building without Task.

## Contributing

Commits follow [Conventional Commits](https://www.conventionalcommits.org), and changes that users or contributors would notice update the documentation in the same commit. See [Contributing](docs/book/src/dev/contributing.md) and [Writing documentation](docs/book/src/dev/docs.md). AI agents start at [AGENTS.md](AGENTS.md).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
