<div align="center">

<img src="packaging/riptide.svg" alt="Riptide logo" width="120">

# Riptide

*Surfing the web really fast.*<br>
A keyboard-driven browser with vim-like bindings, built on Chromium and Rust.

[![Build](https://img.shields.io/github/actions/workflow/status/joshzcold/riptide/check.yml?branch=main&label=build&logo=github)](https://github.com/joshzcold/riptide/actions/workflows/check.yml)
[![Release](https://img.shields.io/github/v/release/joshzcold/riptide?label=release&color=2ec4b6)](https://github.com/joshzcold/riptide/releases/latest)
[![Nightly](https://img.shields.io/github/actions/workflow/status/joshzcold/riptide/nightly.yml?branch=main&label=nightly)](https://github.com/joshzcold/riptide/releases/tag/nightly)
[![Docs](https://img.shields.io/github/actions/workflow/status/joshzcold/riptide/docs.yml?branch=main&label=docs)](https://joshzcold.github.io/riptide/)
[![Security audit](https://img.shields.io/github/actions/workflow/status/joshzcold/riptide/audit.yml?branch=main&label=security%20audit)](https://github.com/joshzcold/riptide/actions/workflows/audit.yml)
[![License](https://img.shields.io/github/license/joshzcold/riptide?color=0b2a3f)](LICENSE)

**[Documentation](https://joshzcold.github.io/riptide/)** ·
**[Install](#install)** ·
**[Releases](https://github.com/joshzcold/riptide/releases)** ·
**[Roadmap](docs/PLAN.md)**

<br><br>

<img src="https://github.com/user-attachments/assets/aaee1bc0-06ed-410d-9a03-a468f2312066" alt="Riptide in hint mode on a Wikipedia article, with a two-letter label on every link and the tab bar above" width="900">

</div>

Riptide is built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate, with everything else in Rust.

> [!NOTE]
> **Status:** early. Linux on X11 only; Wayland is untested, the Windows (installer) and macOS (`.dmg`) builds start but are untested by hand. It's usable day to day, but expect rough edges; the [roadmap](docs/PLAN.md) tracks what's done and what's missing.

<table>
  <tr>
    <td width="33%"><img src="https://github.com/user-attachments/assets/8566f10b-9db4-42c4-b19e-70c803fa63e1" alt="The command line with completion listing the :tab commands and what each does"></td>
    <td width="33%"><img src="https://github.com/user-attachments/assets/ff7ea072-6191-4e1b-8bfa-dc9f6b57db6f" alt="Two tabs side by side in a vertical split"></td>
    <td width="33%"><img src="https://github.com/user-attachments/assets/f4c79f97-8e4a-4ea7-a826-9151238201b6" alt="The settings page with its categories, search box and editable settings"></td>
  </tr>
  <tr>
    <td align="center">Command line with completion</td>
    <td align="center">Splits (<code>sv</code>, <code>ss</code>)</td>
    <td align="center">The <code>:settings</code> page</td>
  </tr>
</table>

## Features

- **Keyboard first:** normal, insert, command, hint, caret and passthrough modes, with vim-like bindings. Also counts, marks, macros, `/` search, `:navigate`, and a command line with history and completion.
- **Tabs and windows:** pinned tabs, a tab bar that works with the mouse, favicons, `:tab-select`, moving tabs between windows, private windows, [splits](https://joshzcold.github.io/riptide/guide/tabs.html#splits) that show several tabs side by side (`sv`, `ss`, `<Ctrl-h/j/k/l>`), and windows that remember their size and place.
- **Video calls:** Google Meet, Teams, Zoom, Webex, Jitsi and Whereby open in their own [call windows](https://joshzcold.github.io/riptide/guide/tabs.html#call-windows), where screen sharing lets you pick a tab, a window or the whole screen. `cm` mutes your microphone in the call from any tab or window, `:media-devices` picks each site's camera, microphone and speaker (switching them mid-call), `:share-stop` ends a share from anywhere, tabs using the camera, microphone or screen are marked, and `gp` floats the call's video over your other tabs.
- **Hints:** for links, inputs, images, yanking and downloads, including number hints, iframes from any site and shadow DOM.
- **Privacy:** ad and tracker blocking with EasyList, EasyPrivacy and uBlock Origin's own lists, including its scriptlets (which strip YouTube's video ads), stand-ins for blocked files, tracking parameters taken off links, and element hiding with `:has-text()` and the like, in frames too, plus your own rules, which an element picker writes for you. Also: Google background calls turned off, the Chromium sandbox where Linux allows it, and per-site permissions and certificate decisions.
- **Extensions:** Chrome (Manifest V3) extensions from the Web Store, such as uBlock Origin Lite, with their popups floating over the page and updates in the background.
- **Lua plugins:** sandboxed plugins you add from git on the `:plugins` page, pinned in a lockfile, and loaded only once you approve the permissions they ask for. A plugin can:
  - add commands with completion, key bindings, and hooks on events such as pages loading, tabs opening or the mode changing;
  - draw status bar widgets, boxes floating over the page, panels docked beside it, and HTML pages of its own;
  - ask you to pick from a list or type an answer, hint elements on the page and act on the one you pick, and fill in logins;
  - keep data between runs, run timers, and, with your permission, run programs, read files or reach given hosts.

  Its options sit in `:settings`, and its secrets in the OS keyring. [riptide-plugins](https://github.com/joshzcold/riptide-plugins) has ready-made ones, such as filling logins from pass, Bitwarden or KeePassXC. See [Plugins](https://joshzcold.github.io/riptide/configuration/plugins.html) for using and writing them, and the [Lua API](https://joshzcold.github.io/riptide/reference/lua-api.html).
- **Configuration:** `config.toml`, or `config.lua` with full scripting (functions on keys, custom commands, event hooks, timers). A `:settings` page to change settings, keys, site data, plugins and extensions; live `:set`; per-site settings; config files that reload when you save them; and a generated `:help` page.
- **Scripts and data:** userscripts, Greasemonkey scripts, `:open-editor`, quickmarks and bookmarks in plain text files, and history imported from qutebrowser (`:history-import`).
- **Page tools:** zoom (`+` `-` `=`), DevTools (`wi`), print or save as PDF, fullscreen, picture-in-picture (`gp`), view source (`gf`), `:jseval`, tab muting, `:messages`, and `.` to repeat the last command.
- **Everything else:** sessions with crash recovery (`:recover` picks which tabs to reopen), history and downloads pages, desktop notifications, spell checking with keyboard-driven fixes and `:spell-install` for dictionaries, dark mode, opt-in Widevine, and handing commands to a running browser from the terminal (`riptide ':open -t x'`).

## Install

Each [release](https://github.com/joshzcold/riptide/releases/latest) has Linux x86_64 packages, with `SHA256SUMS` and build provenance:

| On | Get |
|---|---|
| **Ubuntu 24.04+, Debian 13+** | the `.deb`: `sudo apt install ./riptide_*_amd64.deb` |
| **Any distribution** | the `.AppImage` (`chmod +x` it and run it), or the `.tar.gz` (unpack it and run `riptide`) |
| **Nix** | `nix run github:joshzcold/riptide`, or `github:joshzcold/riptide#riptide-source` to build it from source |
| **Arch Linux** | `yay -S riptide-browser-bin` from the AUR, or `riptide-browser` to build it from source |

The [`nightly`](https://github.com/joshzcold/riptide/releases/tag/nightly) pre-release is rebuilt from `main` every night it changes. [Installing](https://joshzcold.github.io/riptide/guide/installing.html) covers the sandbox setup.

## Documentation

**<https://joshzcold.github.io/riptide/>**, built from [`docs/book/`](docs/book/src/SUMMARY.md):

- [Installing](docs/book/src/guide/installing.md), including the Linux sandbox setup
- [Keys and modes](docs/book/src/guide/keys.md) and [configuration](docs/book/src/configuration/files.md)
- Reference: [commands](docs/book/src/reference/commands.md), [key bindings](docs/book/src/reference/bindings.md), [settings](docs/settings.md), [Lua API](docs/book/src/reference/lua-api.md)
- [Developer guide](docs/book/src/dev/building.md)

In the browser, `F1` or `:help` shows every command, setting and binding with your own config applied. The roadmap is in [docs/PLAN.md](docs/PLAN.md).

## Quick start

Requirements: Rust 1.88+ (edition 2024).

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
./task check          # lint, unit, e2e and smoke tests (needs Xvfb and xdotool)
./task docs-serve     # preview the documentation at http://localhost:3000
```

`./task` downloads a pinned, checksum-verified [Task](https://taskfile.dev) if you don't have it. [Building](docs/book/src/dev/building.md) covers the rest, including building without Task.

## Contributing

Commits follow [Conventional Commits](https://www.conventionalcommits.org), and changes that users or contributors would notice update the documentation in the same commit. See [Contributing](docs/book/src/dev/contributing.md) and [Writing documentation](docs/book/src/dev/docs.md). AI agents start at [AGENTS.md](AGENTS.md).

## Acknowledgements

Riptide owes its design to [qutebrowser](https://qutebrowser.org/) by Florian Bruhin and its contributors. The modes, key bindings, command and setting names, hint labels, quickmark and bookmark files, userscripts and Greasemonkey support all follow qutebrowser, and qutebrowser's documentation was the reference for how they should behave. Riptide is an independent project, written from scratch in Rust on Chromium, and isn't affiliated with qutebrowser.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE), with an additional permission under section 7 for loading a content decryption module such as Widevine at run time ([LICENSE-EXCEPTION](LICENSE-EXCEPTION)).
