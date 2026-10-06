# Riptide

*Surfing the web really fast.* Riptide is a keyboard-driven browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) and controlled entirely from Rust through the [`cef`](https://github.com/tauri-apps/cef-rs) crate.

> **Status:** early prototype, Linux/X11 only, not ready for daily browsing. The [project plan](https://github.com/joshzcold/riptide/blob/main/docs/PLAN.md) tracks what's done and what's missing.

## What works today

- **Keyboard first:** normal, insert, command, hint, caret and passthrough modes, with qutebrowser's bindings. Also counts, marks, macros, `/` search, `:navigate`, and a command line with history and completion.
- **Tabs and windows:** pinned tabs, a tab bar that works with the mouse, favicons, `:tab-select`, moving tabs between windows, and private windows.
- **Hints:** for links, inputs, images, yanking and downloads, including number hints, iframes from any site and shadow DOM.
- **Privacy:** an Adblock Plus engine (EasyList and EasyPrivacy), Google background calls turned off, the Chromium sandbox where Linux allows it, and per-site permissions and certificate decisions.
- **Configuration:** `config.toml`, or `config.lua` with full scripting (functions on keys, custom commands, event hooks). Live `:set`, per-site settings, and a generated `:help` page.
- **Scripts and data:** userscripts, Greasemonkey scripts, `:open-editor`, and qutebrowser's quickmark, bookmark and history formats (`:history-import`).
- **Page tools:** zoom (`+` `-` `=`), DevTools (`wi`), print or save as PDF, fullscreen, view source (`gf`), `:jseval`, tab muting, `:messages`, and `.` to repeat the last command.
- **Everything else:** sessions with crash recovery, history and downloads pages, spell checking with keyboard-driven fixes, dark mode, opt-in Widevine, and handing commands to a running browser from the terminal (`riptide ':open -t x'`).

## Where to start

- New here: [Installing](guide/installing.md), then [Keys and modes](guide/keys.md).
- Looking something up: the [Reference](reference/commands.md) lists every command, binding and setting. In the browser, `F1` or `:help` shows the same, with your own config applied.
- Working on riptide itself: the [Developer guide](dev/building.md).

Riptide is licensed under GPL-3.0-or-later.
