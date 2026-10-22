# Riptide

*Surfing the web really fast.* Riptide is a keyboard-driven browser built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) and controlled entirely from Rust through the [`cef`](https://github.com/tauri-apps/cef-rs) crate.

> **Status:** early (v0.4). Linux on X11 only; Wayland is untested, and the macOS and Windows builds compile but can't run the browser yet. It's usable day to day, but expect rough edges. The [project plan](https://github.com/joshzcold/riptide/blob/main/docs/PLAN.md) tracks what's done and what's missing.

## Features

- **Keyboard first:** normal, insert, command, hint, caret and passthrough modes, with vim-like bindings. Also counts, marks, macros, `/` search, `:navigate`, and a command line with history and completion.
- **Tabs and windows:** pinned tabs, a tab bar that works with the mouse, favicons, `:tab-select`, moving tabs between windows, private windows, [splits](guide/tabs.md#splits) that show several tabs side by side (`sv`, `ss`, `<Ctrl-h/j/k/l>`), and windows that remember their size and place.
- **Video calls:** Google Meet, Teams, Zoom, Webex, Jitsi and Whereby open in their own [call windows](guide/tabs.md#call-windows), where screen sharing lets you pick a tab, a window or the whole screen. `cm` mutes your microphone in the call from any tab or window, `:media-devices` picks each site's camera, microphone and speaker (switching them mid-call), `:share-stop` ends a share from anywhere, tabs using the camera, microphone or screen are marked, and `gp` floats the call's video over your other tabs.
- **Hints:** for links, inputs, images, yanking and downloads, including number hints, iframes from any site and shadow DOM.
- **Privacy:** ad and tracker blocking with EasyList, EasyPrivacy and uBlock Origin's own lists, including its scriptlets (which strip YouTube's video ads), stand-ins for blocked files, tracking parameters taken off links, and element hiding with `:has-text()` and the like, in frames too, plus your own rules, which an element picker writes for you. Also: Google background calls turned off, the Chromium sandbox where Linux allows it, and per-site permissions and certificate decisions.
- **Configuration:** `config.toml`, or `config.lua` with full scripting (functions on keys, custom commands, event hooks, timers). A `:settings` page to change settings, keys, site data, plugins and extensions; live `:set`; per-site settings; config files that reload when you save them; and a generated `:help` page.
- **Extensions and plugins:** Chrome (Manifest V3) extensions from the Web Store, such as uBlock Origin Lite, with their popups floating over the page and updates in the background; and sandboxed Lua plugins installed from git, like [the ones that fill logins](guide/passwords.md) from pass, Bitwarden or KeePassXC. Plugins ask for permissions, have their options in `:settings`, and keep secrets in the OS keyring.
- **Scripts and data:** userscripts, Greasemonkey scripts, `:open-editor`, quickmarks and bookmarks in plain text files, and history imported from qutebrowser (`:history-import`).
- **Page tools:** zoom (`+` `-` `=`), DevTools (`wi`), print or save as PDF, fullscreen, picture-in-picture (`gp`), view source (`gf`), `:jseval`, tab muting, `:messages`, and `.` to repeat the last command.
- **Everything else:** sessions with crash recovery (`:recover` picks which tabs to reopen), history and downloads pages, desktop notifications, spell checking with keyboard-driven fixes and `:spell-install` for dictionaries, dark mode, opt-in Widevine, and handing commands to a running browser from the terminal (`riptide ':open -t x'`).

## Where to start

- New here: [Installing](guide/installing.md), then [Keys and modes](guide/keys.md).
- Looking something up: the [Reference](reference/commands.md) lists every command, binding and setting. In the browser, `F1` or `:help` shows the same, with your own config applied.
- Working on riptide itself: the [Developer guide](dev/building.md).

Riptide is licensed under GPL-3.0-or-later.
