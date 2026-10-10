# riptide — Project Plan

A keyboard-driven, vim-like browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (modern Chromium) and controlled entirely from Rust.

This file tracks what's left to do. Finished work is one line per milestone below: the [user guide](book/src/introduction.md) documents what it does, the [developer guide](book/src/dev/architecture.md) how it's built and [CEF pitfalls](book/src/dev/cef-pitfalls.md) what it taught us, and `git log -p docs/PLAN.md` keeps the old notes (this file was condensed on 2026-10-09).

## Goals

1. **Up-to-date browsing.** Track current CEF/Chromium releases closely (the `cef` crate ships `154.x` bindings for Chromium 154).
2. **Rust control plane.** All browser logic — modes, keybindings, commands, config, storage — in Rust. No Python, no Qt.
3. **qutebrowser feel.** Same modes, default bindings, command names, and workflow, so existing qutebrowser users feel at home.

## Non-goals (for now)

- Reimplementing qutebrowser's Python `config.py` API verbatim.
- Mobile platforms.
- Building Chromium from source (we consume CEF binary distributions).

---

## Key technical decisions

1. **Bindings:** the `cef` crate (tauri-apps/cef-rs), pinned to an exact version and upgraded deliberately. Binaries come from `export-cef-dir`.
2. **Windowing: CEF Views, Alloy style.** One `BrowserView` per tab in a content panel, plus privileged `riptide://ui/` views for the tab bar, status bar and overlay, driven over process messages. Off-screen rendering was the fallback and wasn't needed. The exception is **call windows** (`:open --call`, `content.call_sites`): their first tab is Chrome style, so Chrome's screen-share picker works (M22). Floats, panels and extension popups are further views in the same window.
3. **Keys:** `OnPreKeyEvent` sees every key first, and the Rust engine consumes it or passes it on. A renderer script reports focus on editable elements for insert mode.
4. **Processes:** one executable; CEF's subprocesses re-execute it and `main()` dispatches. Page logic (hints, caret, scrolling, insert detection) is injected JavaScript answering through an eval channel pages can't forge.
5. **Core rule:** `rt-core` has no CEF dependency. Modes, keys, commands, settings and their checks are unit tested without a browser; `rt-cef` turns CEF events into `rt-core` inputs and carries out its effects.

## Architecture

| Crate | |
|---|---|
| `riptide` | the binary: logging, then `rt_cef::run` |
| `rt-core` | CEF-free logic: engine, modes, keymap, commands, settings, completion, hints, prompts, themes, UI message checks |
| `rt-config` | config files, paths, the Lua runtime and plugin sandbox, plugins and their git handling, the remote socket, `--plugin-test` |
| `rt-storage` | history (SQLite), bookmarks, quickmarks, sessions, crash recovery |
| `rt-adblock` | adblock-rust, filter lists, scriptlets and procedural filters |
| `rt-cef` | everything CEF: windows, tabs, handlers, UI pages (`ui/`), page scripts (`js/`), extensions, keyring secrets |
| `rt-e2e` | end-to-end tests: a real browser per test on its own Xvfb, driven over a test channel |

The [architecture page](book/src/dev/architecture.md) explains the threading rules, the UI channel and the engine loop.

---

## Done

| | Milestone | Where it's documented |
|---|---|---|
| M0 | Spike: Views + an HTML UI overlay work; no OSR needed (2026-10-02) | dev/architecture.md |
| M1–M2 | Modes, keys, scrolling, navigation, the command line with history, `;;` chaining and fuzzy completion | guide/keys.md |
| M3–M4 | Tabs, hints in every frame and shadow roots, all hint targets | guide/tabs.md, guide/keys.md |
| M5 | `config.toml`, `config.lua`, `autoconfig.toml`, per-site settings | configuration/files.md |
| M6 | History, bookmarks, quickmarks, sessions | guide/sessions.md |
| M7 | Prompts, downloads, permissions, logins, certificate errors | guide/prompts.md |
| M8 | Ad blocking (lists, hosts files, cosmetic, scriptlets, procedural, frames, your own rules and the `;x` picker), private windows, the Google traffic review | guide/privacy.md |
| M9 | Caret mode, marks, macros, search, `:spawn`, userscripts, Greasemonkey, `:open-editor` | guide/scripts.md |
| M10 | Linux tarball, AppImage, .deb, AUR and Nix recipes, a flake; experimental macOS and Windows packages | guide/installing.md |
| M11 | Opt-in Widevine (VP9/AV1) | guide/pages.md |
| M12 | `config.lua` scripting (keys, commands, hooks), now part of M27 | configuration/lua.md |
| M13 | `riptide://` pages and the UI channel | dev/architecture.md |
| M14 | Pinned tabs, the mouse, favicons | guide/tabs.md |
| M15 | `riptide ':cmd' url` hands over to the running instance (Unix) | guide/terminal.md |
| M16 | `:help` generated from the registries, `:version` | guide/terminal.md |
| M17 | Spell checking with keyboard suggestions, `:spell-install` | guide/pages.md |
| M18 | Versions, changelog, CI on three platforms, one-click and nightly releases (v0.1.0, v0.2.0, v0.3.0) | dev/releasing.md |
| M19 | qutebrowser parity sweep: tabs, status bar, key hints, completion, hints, URLs, downloads, content, input, sessions, tools, config commands | the reference pages |
| M20 | Themes, fonts, `ui.css`, user themes and importers, bars sized by their pages, floating overlay and prompts | configuration/themes.md |
| M21 | The settings page: Settings, Keys, Sites, Plugins and Extensions tabs | configuration/files.md |
| M22 | Call windows with Chrome's screen-share picker, sharing markers, `:share-stop`, `cm` mute, picture-in-picture | guide/tabs.md |
| M23 | Crash recovery (signals, crash loops, kept sessions, `:recover`), crashed-tab pages, crash reports | guide/sessions.md |
| M24 | The documentation website | dev/docs.md |
| M25 | Permission questions belong to their tab, with site, icons and buttons | guide/prompts.md |
| M26 | The e2e harness (`rt-e2e`), unit tests in the CEF layer, linters (clippy lints, cargo-deny, ShellCheck, actionlint, typos, Biome) | dev/testing.md |
| M27 | Lua plugins: sandbox and permissions, git install with a lockfile, one checkout per repository, lazy loading, dependencies, events, keys, commands, floats, panels, pages, `rt.page.*`, `rt.ui.*`, status bar widgets, the Plugins tab with Add/Browse, options and keyring secrets, check/update/sync/restore/clean, a daily update check, `:help` for plugins, `--plugin-test`, the template and riptide-plugins repositories | configuration/plugins.md, configuration/lua.md |
| M28 | Chrome (MV3) extensions: `:extension-install` from the Web Store, the Extensions tab, updates, floating popups, native messaging hosts | guide/extensions.md |

---

## Open work

### Plugins (M27)

- **riptide-plugins `options` branch:** the password plugins' `[[option]]`s and saved passwords. Merge it into `main` once a release with plugin options (0.4) is out, since v0.3.0 rejects unknown manifest keys.
- **A minimum riptide version** in `riptide-plugin.toml` (`riptide = ">=0.4"`), checked before loading, with a clear message instead of a failure, and shown on the Browse list.
- **Events** from the design not built yet: `page_error`, `tab_moved`, `key` (unhandled keys, with a way to swallow one), `command` (before and after), `insert_entered`, `prompt_shown`, `permission_answered`, `theme_changed`, `focus_gained`/`focus_lost`. Per-site keys (`rt.keymap.set(…, { site = … })`).
- **A health report:** a plugin's optional `health()`, run by `:plugins check` (git missing, a tool not found, a required option unset).
- **Plugin pages in floats** (tabs and panels have them).
- **Secrets on Linux without a keyring service:** today saving says it can't; consider an encrypted file as a fallback.
- **Trusted plugins and `rt.secret`:** trusted plugins run with the global `rt`, which has no plugin name, so they can't read secret options.

### Extensions (M28)

- Popups and keyboard commands can't see riptide's tabs: CEF lists only Chrome-style tabs in `chrome.tabs`. Needs a CEF change, or an upstream issue.
- Automatic extension update checks (like plugins' daily check).
- Not checked yet: real autofill with a vault, passkeys, `chrome.commands` shortcuts.

### Ad blocking (M8)

- Scriptlets in frames from another site (separate DevTools targets; needs `Target.setAutoAttach`).
- Automatic filter list updates.
- A tab keeps a host's scriptlets until it closes, even after the site is whitelisted.

### Tabs, windows and sessions

- A tab focus stack (`tab-focus stack-prev`/`stack-next`, `tabs.focus_stack_size`).
- Window size and position aren't saved; every window opens at 1280×800.
- Closing a tab with `d` skips the page's leave-page warning.
- Call windows are restored as ordinary windows.
- Favicons aren't kept in sessions (they're fetched again on load).

### Config

- Watching `config.toml` and `config.lua` for changes (`:config-source` reloads by hand).
- Importing qutebrowser's `config.py` (bindings and settings), beyond themes.

### Calls (M22)

- Check by hand on a real desktop: fullscreen, Chrome's accelerators and context menu in call windows, keys while the picker is open.
- Switching the shared source mid-call; shared tabs highlighted in the tab bar.
- `:media-devices` to pick the camera, microphone and speaker per site; the permission question naming the device.
- VA-API decode and encode, and GPU background blur, on a stock distribution.
- A manual matrix: Meet, Zoom, Teams, Jitsi, Slack huddles, Discord, Whereby — camera, microphone, screen, window and tab sharing, and calls in a background tab.
- `cm` for a call in another window.

### Packaging (M10)

- Publish `riptide-bin` to the AUR (needs the maintainer's account).
- macOS: an app bundle, signing and notarization. Windows: an installer, and named pipes for the remote command (each start is a new instance there).
- Wayland: not tested; check key handling and the desktop portal for screen sharing.

### Smaller gaps

- Widevine: never updated after the first download, not removed when turned off, and protected playback against a real stream isn't verified. **The licensing review is still open** (Google's terms for third-party browsers, GPL-3.0 loading a proprietary CDM).
- Spell checking: a default key binding, and hints for misspelled words.
- `RIPTIDE_FIFO` is read when the userscript exits, not live.
- Greasemonkey: scripts run in the page's main world; `GM_xmlhttpRequest` has no binary responses.
- Downloads from private windows go to the usual folder and list.
- The documentation site doesn't show the version it was built from.
- Lost keys under Xvfb just after `:open -t` finishes loading (about 1 in 5); not seen on a real desktop. Check there and on Wayland.
- The crash tests don't cover a native abort in the browser process.

### Not planned

- Proprietary codecs (H.264/AAC): they need a CEF source build, which works against goal 1 and raises patent questions. Revisit only if VP9/AV1 isn't enough, preferring a documented "build your own CEF" path.
- `window.transparent`, `zoom.text_only`, `zoom.mouse_divider`, `content.local_content_can_access_remote_urls`, `content.persistent_storage`: CEF or Chromium has no way to do them.
- qutebrowser's `qt.*`, `backend`, and Python-only commands.

---

## Chromium update cadence

- Pin the `cef` crate to an exact version.
- `cef-update.yml` opens an "Update CEF to X" issue when crates.io has a newer `cef` crate; upgrades go through a branch with the full test suite.
- Keep CEF-specific code in `rt-cef`, so binding churn doesn't reach the core.

## Testing

Unit tests in every crate, end-to-end tests in `rt-e2e` (a browser per test over a test channel), a short smoke test for real X11 input and the release packages, and linters. [dev/testing.md](book/src/dev/testing.md) has the details. New behaviour gets an e2e test.

## Risks

| Risk | Mitigation |
|---|---|
| `cef` crate API churn and gaps | Pin versions; drop to `cef-dll-sys` for missing pieces; contribute upstream |
| Alloy-style tabs fall behind Chrome style (extensions' `chrome.tabs`, screen sharing) | Chrome-style call windows where needed; watch CEF for Alloy support |
| Wayland support in CEF | X11 first; track CEF's Ozone/Wayland status |
| Large downloads (~150 MB) | Expected for any CEF application; documented |
| DRM: Widevine only for VP9/AV1 | Opt-in Widevine (M11); H.264/AAC not planned |
| Background traffic to Google services | Reviewed in M8; re-check on CEF upgrades |
| Plugins and extensions run other people's code | Sandbox, declared permissions asked for, keyring-only secrets, pinned commits reviewed before updating |

## Open questions

1. **qutebrowser compatibility depth:** import `config.py` bindings and settings? History, bookmarks, quickmarks and themes already import.
2. **Plugin compatibility across releases:** a minimum-version field (above) handles new keys; should riptide-plugins keep a branch per riptide release?

> riptide is GPL-3.0 (`LICENSE`). Dependency licenses must stay GPL-compatible; `cargo-deny` checks them.
