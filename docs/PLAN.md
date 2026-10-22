# riptide — Project Plan

A keyboard-driven, vim-like browser built on [CEF](https://github.com/chromiumembedded/cef) (modern Chromium) and controlled entirely from Rust.

This file tracks what's left to do. Finished work is one line per milestone below: the [user guide](book/src/introduction.md) documents what it does, the [developer guide](book/src/dev/architecture.md) how it's built and [CEF pitfalls](book/src/dev/cef-pitfalls.md) what it taught us, and `git log -p docs/PLAN.md` keeps the old notes (this file was condensed on 2026-10-09 and tidied on 2026-10-22).

## Goals

1. **Up-to-date browsing.** Track current CEF/Chromium releases closely (the `cef` crate ships `154.x` bindings for Chromium 154).
2. **Rust control plane.** All browser logic — modes, keybindings, commands, config, storage — in Rust. No Python, no Qt.
3. **Keyboard first.** Modes, default bindings and command names that work together, so the whole browser can be driven from the keyboard.

## Non-goals (for now)

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
| M18 | Versions, changelog, CI on three platforms, nightlies, and a release pull request kept up to date with `main` (v0.1.0 to v0.7.0) | dev/releasing.md |
| M19 | Command and setting sweep: tabs, status bar, key hints, completion, hints, URLs, downloads, content, input, sessions, tools, config commands | the reference pages |
| M20 | Themes, fonts, `ui.css`, user themes and importers, bars sized by their pages, floating overlay and prompts | configuration/themes.md |
| M21 | The settings page: Settings, Keys (where each binding came from and what it replaced, `:map`), Sites, Plugins and Extensions tabs | configuration/files.md |
| M22 | Call windows with Chrome's screen-share picker, sharing markers, `:share-stop`, `cm` mute, picture-in-picture | guide/tabs.md |
| M23 | Crash recovery (signals, crash loops, kept sessions, `:recover`), crashed-tab pages, crash reports | guide/sessions.md |
| M24 | The documentation website | dev/docs.md |
| M25 | Permission questions belong to their tab, with site, icons and buttons | guide/prompts.md |
| M26 | The e2e harness (`rt-e2e`), unit tests in the CEF layer, linters (clippy lints, cargo-deny, ShellCheck, actionlint, typos, Biome) | dev/testing.md |
| M27 | Lua plugins: sandbox and permissions, git install with a lockfile, one checkout per repository, lazy loading, dependencies, events (`command_before` and `key` can cancel; both, and `command`, need a permission), keys (per-site too), commands, floats, panels, pages (in tabs, panels and floats), `rt.page.*`, `rt.ui.*`, status bar widgets, the Plugins tab with Add/Browse, options and keyring secrets, health checks, check/update/sync/restore/clean, a daily update check, `:help` for plugins, `--plugin-test`, the template and riptide-plugins repositories | configuration/plugins.md, configuration/lua.md |
| M28 | Chrome (MV3) extensions: `:extension-install` from the Web Store, the Extensions tab, updates, floating popups, native messaging hosts | guide/extensions.md |
| M29 | Splits: `:split`/`:vsplit` and the `s` keys, `<Ctrl-h/j/k/l>`, resizing by keys and by dragging a border, the focused pane's border, layouts in sessions, crash notices and extension popups per pane | guide/tabs.md |

---

## Open work

### Next up (as of 2026-10-22)

1. **Check by hand on a real desktop:** calls (below), and key handling on Wayland.
2. **macOS and Windows packages** (M10), the largest milestone left.

### Extensions (M28)

- Not checked yet: real autofill with a vault, passkeys, `chrome.commands` shortcuts.

### Calls (M22)

- **Check by hand on a real desktop:** fullscreen, Chrome's accelerators and context menu in call windows, keys while the picker is open, `cm` from another window keeping that window in front (the tests have no window manager), VA-API and GPU background blur on a stock distribution, and a matrix of Meet, Zoom, Teams, Jitsi, Slack huddles, Discord and Whereby (camera, microphone, screen, window and tab sharing, calls in a background tab).

### Packaging (M10)

- macOS: an app bundle, signing and notarization. Windows: an installer, and named pipes for the remote command (each start is a new instance there).
- Wayland: not tested; check key handling and the desktop portal for screen sharing.

### Bugs

- Widevine: protected playback against a real stream isn't verified (needs a real desktop and a service that serves VP9/AV1).

### Test reliability

- Lost keys under Xvfb just after `:open -t` finishes loading (about 1 in 5); not seen on a real desktop.
- Under load, CEF can time out setting up a cross-origin iframe's renderer ("Timeout of new browser info response"). The frame then gets no hints until the page is reloaded; the e2e test reloads once.
- Since 2026-10-21 on CI's runners, the smoke test's first hint click after startup is often lost, and so are keys just after `Escape` leaves insert mode; the smoke test sends them again.

### Upkeep

- `deny.toml` ignores RUSTSEC-2026-0370/0371/0372 (zvariant and zbus_names merged into zbus 6). Drop the ignores once `keyring`'s `secret-service` moves to zbus 6.

### Not planned

- Plugins' secrets in a passphrase-encrypted file, to last across restarts on Linux without a keyring service: you'd type riptide's passphrase in place of the vault's, for new crypto code and a passphrase that can't be recovered. They go to the kernel keyring until a restart instead, and a Secret Service (KeePassXC, GNOME Keyring) keeps them for good. Revisit if users ask for it.
- Highlighting the tab a call shares: the picker's Tab list only offers call tabs (CEF lists only Chrome-style tabs), so it's rare, and telling which tab is captured would mean giving sites an id for your tabs (Capture Handle). Revisit if riptide's ordinary tabs become shareable.
- Extensions seeing riptide's tabs in `chrome.tabs`: CEF lists only Chrome-style tabs, so it needs a CEF change (see Risks).
- Reading `RIPTIDE_FIFO` live (it's read when the userscript exits), Greasemonkey scripts in an isolated world, and binary `GM_xmlhttpRequest` responses, until someone needs them.
- Proprietary codecs (H.264/AAC): they need a CEF source build, which works against goal 1 and raises patent questions. Revisit only if VP9/AV1 isn't enough, preferring a documented "build your own CEF" path.

---

## Chromium update cadence

- Pin the `cef` crate to an exact version.
- `cef-update.yml` opens an "Update CEF to X" pull request when crates.io has a newer `cef` crate, and runs the full test suite on it; binding changes are fixed on its branch.
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
| Widevine licensing: Google's terms for third-party browsers, and GPL-3.0 loading a proprietary CDM | Decided 2026-10-22: opt-in, off by default, never bundled; the setting, guide and download message say Google provides it under its own terms; `LICENSE-EXCEPTION` allows loading a CDM. Revisit before the macOS and Windows packages, where signing (VMP) needs Google |
| Background traffic to Google services | Reviewed in M8; re-check on CEF upgrades |
| Plugins and extensions run other people's code | Sandbox, declared permissions asked for, keyring-only secrets, pinned commits reviewed before updating |

## Open questions

None right now. (Plugin compatibility: plugins declare the riptide versions they need, such as `riptide = ">=0.5"`; releases come too often for a riptide-plugins branch per release.)

> riptide is GPL-3.0 (`LICENSE`). Dependency licenses must stay GPL-compatible; `cargo-deny` checks them.
