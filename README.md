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

</div>

A browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate, with everything else in Rust.

> [!NOTE]
> **Status:** early (v0.1). Linux on X11 only. It's usable day to day, but expect rough edges; the [roadmap](docs/PLAN.md) tracks what's done and what's missing.

## What works today

- **Keyboard first:** normal, insert, command, hint, caret and passthrough modes, with qutebrowser's bindings. Also counts, marks, macros, `/` search, `:navigate`, and a command line with history and completion.
- **Tabs and windows:** pinned tabs, a tab bar that works with the mouse, favicons, `:tab-select`, moving tabs between windows, and private windows.
- **Hints:** for links, inputs, images, yanking and downloads, including number hints, iframes from any site and shadow DOM.
- **Privacy:** an Adblock Plus engine (EasyList and EasyPrivacy), Google background calls turned off, the Chromium sandbox where Linux allows it, and per-site permissions and certificate decisions.
- **Configuration:** `config.toml`, or `config.lua` with full scripting (functions on keys, custom commands, event hooks). Live `:set`, per-site settings, and a generated `:help` page.
- **Scripts and data:** userscripts, Greasemonkey scripts, `:open-editor`, and qutebrowser's quickmark, bookmark and history formats (`:history-import`).
- **Page tools:** zoom (`+` `-` `=`), DevTools (`wi`), print or save as PDF, fullscreen, view source (`gf`), `:jseval`, tab muting, `:messages`, and `.` to repeat the last command.
- **Everything else:** sessions with crash recovery, history and downloads pages, spell checking with keyboard-driven fixes, dark mode, opt-in Widevine, and handing commands to a running browser from the terminal (`riptide ':open -t x'`).

## Install

Each [release](https://github.com/joshzcold/riptide/releases/latest) has Linux x86_64 packages, with `SHA256SUMS` and build provenance:

| On | Get |
|---|---|
| **Ubuntu 24.04+, Debian 13+** | the `.deb` (from v0.2, and in the nightly): `sudo apt install ./riptide_*_amd64.deb` |
| **Any distribution** | the `.AppImage` (`chmod +x` it and run it), or the `.tar.gz` (unpack it and run `riptide`) |
| **Nix** | `nix run github:joshzcold/riptide` |
| **Arch Linux** | [`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD) (`makepkg -si`) |

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

GPL-3.0-or-later. See [LICENSE](LICENSE).
