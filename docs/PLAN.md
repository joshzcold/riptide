# riptide — Project Plan

A keyboard-driven, vim-like browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (modern Chromium) and controlled entirely from Rust.

## Goals

1. **Up-to-date browsing.** Track current CEF/Chromium releases closely (the `cef` crate currently ships `154.x` bindings for Chromium 154).
2. **Rust control plane.** All browser logic — modes, keybindings, commands, config, storage — in Rust. No Python, no Qt.
3. **qutebrowser feel.** Same modes, default bindings, command names, and workflow, so existing qutebrowser users feel at home.

## Non-goals (for now)

- Reimplementing qutebrowser's Python `config.py` API verbatim.
- Mobile platforms.
- Building Chromium from source (we consume CEF binary distributions).

---

## Key technical decisions

### 1. Rust bindings: `cef` crate (tauri-apps/cef-rs)

- Crates: `cef` (safe-ish wrappers), `cef-dll-sys` (raw FFI), `export-cef-dir` / `download-cef` (fetch binaries).
- Binaries live in a shared dir (e.g. `~/.local/share/cef`) pointed to by `CEF_PATH`.
- Supports Linux, macOS, Windows on x86_64 and ARM64.
- **Risk:** wrapper API churn tracks CEF majors. Pin exact versions and upgrade deliberately (see "Chromium update cadence").

### 2. Windowing: CEF Views + Alloy runtime style (recommended)

| Option | Pros | Cons |
|---|---|---|
| **CEF Views (Alloy style)** | Native GPU compositing, no frame copies, CEF owns the window, cross-platform | UI chrome must be built from Views widgets or an HTML BrowserView |
| Off-screen rendering (OSR) + winit/wgpu | Total control of the window and drawing | Frame copies or platform-specific shared textures, IME/accessibility/input must be reimplemented, much more code |

**Decision:** CEF Views. The window holds:

- A **content area**: one `BrowserView` per tab; only the active one is visible.
- A **UI overlay `BrowserView`** (privileged, internal `riptide://ui` page) that renders the status bar, command line, completion menu, tab bar, prompts, and messages. Rust drives it via process messages; it never loads remote content.

This mirrors qutebrowser's split (Qt widgets around a web view) while keeping the UI layer simple to style.

### 3. Key handling

- `CefKeyboardHandler::OnPreKeyEvent` sees every key before the page does. The Rust mode manager decides whether to consume it or pass it through.
- **Insert-mode detection:** a small renderer-side script reports focus changes on editable elements (`input`, `textarea`, `contenteditable`) to the browser process. Auto-enter/leave insert mode like qutebrowser's `input.insert_mode.auto_enter`.

### 4. Process model

Single executable. CEF launches subprocesses (renderer, GPU, utility) by re-executing it; `main()` dispatches via `cef::execute_process`.

- **Browser process:** window, tabs, modes, commands, config, storage.
- **Renderer process (Rust `RenderProcessHandler`):** JS bindings for hints, insert-mode detection, caret mode, scrolling helpers. Communicates via `CefProcessMessage`.

### 5. Page interaction

Most qutebrowser features that touch page content are injected JavaScript (it does the same):

- **Scrolling:** `window.scrollBy` / element-aware scrolling.
- **Hints:** JS collects clickable elements, draws labels in a shadow-DOM overlay, reports positions; Rust owns label generation and key matching. Results come back through an eval channel: the browser asks our renderer-process code to evaluate a script, and that Rust code replies with the result. Pages can't forge replies, and no global function is exposed to page scripts.
- **Search (`/`, `n`, `N`):** native `CefBrowserHost::Find`.
- **Caret mode:** JS Selection API.

### 6. Supporting crates

| Need | Crate |
|---|---|
| Content blocking | `adblock` (Brave's adblock-rust; MPL-2.0, GPL-compatible) |
| History / bookmarks / quickmarks | `rusqlite` |
| Fuzzy completion | `nucleo` |
| Config | `serde` + `toml`, Lua (`mlua`) later |
| Command-line parsing | Custom parser (qutebrowser syntax: `:cmd --flag arg;; cmd2`) |
| Logging | `tracing` |
| Paths | `directories` (XDG) |
| Errors | `thiserror` / `anyhow` |

---

## Architecture

```
riptide/
├── Cargo.toml                 # workspace
├── crates/
│   ├── riptide/               # binary: main(), CEF init, subprocess dispatch
│   ├── rt-core/               # CEF-free logic: modes, keymap, commands, config (unit-testable)
│   ├── rt-cef/                # CEF integration: App, Client, handlers, Views window, tabs
│   ├── rt-renderer/           # renderer-process handler + JS injection
│   ├── rt-storage/            # history, bookmarks, quickmarks, sessions (SQLite)
│   └── rt-ui/                 # internal UI page (HTML/CSS/TS) embedded via include_dir
├── js/                        # page scripts: hints, scroll, caret, insert detection
└── docs/
```

> **Current state (after M0):** only `riptide`, `rt-core` and `rt-cef` exist. Renderer code, UI pages (`rt-cef/ui/`) and page scripts (`rt-cef/js/`) live inside `rt-cef` until they grow enough to split out. `rt-storage` arrives with M6.

**Rule:** `rt-core` has no CEF dependency. Modes, key parsing, command dispatch, and config are tested without a browser. CEF is an adapter that turns events into `rt-core` inputs and executes `rt-core` actions.

### Core loop

```
Key event (OnPreKeyEvent)
  → ModeManager (normal / insert / hint / command / caret / passthrough / prompt)
  → KeyParser (counts, multi-key sequences like `gg`, `;y`)
  → Command (`:scroll down`, `:open -t …`)
  → CommandRegistry dispatch
  → Action on Tab / Window / UI / Storage
```

Today `rt-core::Engine` implements this loop: it takes a `Key` and returns `KeyOutcome { consumed, effects }`. Mode and command-line commands are handled inside the engine. Everything else comes back as `Effect::Run(Command)` for `rt-cef` to carry out. The command table is a hand-written `match` for now.

Commands are registered with a derive macro so each one declares its name, args, flags, count support, and the modes it applies in. This gives `:help`, completion, and argument validation from one source of truth.

---

## Feature parity checklist (qutebrowser → riptide)

### Modes
- [x] Normal, Insert (with auto-enter/leave on focus), Command, Passthrough
- [x] Hint, Caret, Prompt, Yes/No, Register (marks/macros)

### Navigation
- [x] `o` / `go` open / edit current URL (current tab only)
- [x] `O` / `gO` open in a new tab
- [x] `H` / `L` back/forward, `r` / `R` reload
- [x] `gu` / `gU` go up URL (M9)
- [x] `hjkl`, `gg`, `G`, `0`, `$`, `Ctrl-d/u/f/b` scrolling with counts
- [x] `/`, `?`, `n`, `N` search (M9)
- [x] `[[` / `]]` prev/next page navigation, `Ctrl-a`/`Ctrl-x` (M9)
- [x] Quickmarks / bookmarks (`m`, `b`, `B`, `M`, `gb`, `gB`)
- [x] Marks (`` ` ``-style in-page marks; `m` is the quickmark key, as in qutebrowser) (M9)

### Tabs
- [x] `J` / `K` next/prev, `d` close, `u` undo close, `gt` / `gT`, `Alt-<n>`, `Ctrl-Tab` last-focused
- [x] `:tab-move`, `:tab-only`, `:open -t/-b/-r`, popups as tabs (keeping `window.opener`)
- [x] `:tab-pin` and pinned tabs (M14)
- [x] `:tab-clone`, `:tab-give`, `:tab-take` (2026-10-02; the page is reopened, so its back/forward history doesn't move)
- [x] Mouse: click/middle-click/wheel/drag in the tab bar, middle-click links (M14)
- [x] Favicons in the tab bar (M14); not yet in completion
- [x] Multiple windows (2026-10-02)
- [x] Each tab keeps its own insert/normal mode: switching back restores it (qutebrowser's `tabs.mode_on_change = restore`). Each tab stores the mode it was left in; with `restore`, switching back brings insert or passthrough mode back (2026-10-02). Hints, the caret and the like end on any switch; the command line and prompts stay.
- [ ] **Closing a pinned tab asks instead of refusing.** Today `d`, `Ctrl-w`, `:tab-close` and a middle-click on a pinned tab only show "Tab is pinned! Use :tab-close --force to close it" (`tabs::close_unless_pinned`).
  - Ask "Close pinned tab <title>?" as a yes/no prompt, defaulting to no so a stray `Return` keeps the tab; `y` closes it.
  - `--force` still closes without asking. `tabs.pinned.close` (`ask`, the default; `refuse`, today's behaviour and qutebrowser's; or `close`) chooses what happens.
  - `:tab-only` keeps pinned tabs as it does now, without asking about each one.
  - `u` should reopen the tab pinned: today `closed` remembers only the index and URL.
  - **Tests:** a smoke step: `d` on a pinned tab asks, `n` keeps it, `d` then `y` closes it, and `u` brings it back pinned.

### Hints
- [x] `f` / `F` follow (current / new tab), `;b` background, `;y` yank, `;i` / `;I` image, `;o` / `;O` fill, `;h` hover, `;t` inputs, `;r` rapid
- [x] `;d` download
- [x] Configurable chars (M5), hints inside same-origin iframes (2026-10-02; cross-origin iframes are hinted as a whole)
- [x] Number hint mode (`hints.mode = number`: digit labels, letters filter by element text, a unique match is followed) (2026-10-02)

### Command line
- [x] `:` command entry with history, completion (commands, URLs, history, bookmarks, settings, open tabs via `:tab-select`/`T`)
- [x] Command chaining (`;;`), aliases, `:bind` / `:unbind`, `:set`

### Yank / paste
- [x] `yy`, `yt`, `yd`, `pp`, `PP` (`{clipboard}` is substituted per command, so pasted text can't add `;;` commands)
- [x] Primary selection (`yY`, `pP`; X11 via arboard, the clipboard elsewhere) (2026-10-02)

### Content
- [x] Ad blocking (EasyList / uBlock lists) (M8; host-file blocking not yet)
- [x] Per-domain settings for permissions and content blocking (M8; JS, cookies and images still to come)
- [x] Downloads with status-bar progress and prompts
- [x] Permission prompts (geolocation, camera, notifications)
- [x] HTTP auth and JS dialogs in the prompt UI
- [x] Download list (`:downloads`, a page that refreshes while downloads run) and `Tab` path completion in the save prompt (2026-10-02)
- [x] TLS certificate errors: `OnCertificateError` asks per `content.tls.certificate_errors` (`ask`/`block`/`load-insecurely`, per-site). `A`/`N` save the answer for the origin. A smoke step uses a self-signed HTTPS server (2026-10-02).
- [x] Dark mode: `colors.webpage.preferred_color_scheme` (live, `SetChromeColorScheme`) and `colors.webpage.darkmode.enabled` (startup `--blink-settings=forceDarkModeEnabled=true`)
- [x] Widevine DRM, opt-in (M11)
- [x] Review of background Google service traffic (M8; one `ListAccounts` call left)

### Help and tooling
- [x] `:help` pages generated from the live commands, settings and bindings; `:version` (M16)
- [x] `riptide ':cmd' url` talks to the running instance (M15; Unix)
- [x] Spell checking with keyboard-driven suggestions (M17)
- [x] Versioned releases, `CHANGELOG.md`, CI on Linux/macOS/Windows (M18; Linux release artifacts only)
- [x] Documentation website at https://joshzcold.github.io/riptide/ (mdBook, `docs/book/`), with agent skills for user and developer docs (M24)

### Extensibility
- [x] Userscripts (spawned processes with `QUTE_URL`, `QUTE_FIFO`, etc.; keep env-var names for compatibility) (M9)
- [x] Greasemonkey-style injected JS (M9)
- [x] `:spawn` external commands (M9)
- [x] `:open-editor` (edit text field in `$EDITOR`) (M9)

### Session / state
- [x] Sessions (save / load / `auto_save.session`, `:wq`)
- [x] History with completion (`:open` + `Tab`)
- [x] Quickmarks and bookmarks in qutebrowser's file formats
- [x] Crash-recovery autosave (`auto_save.interval`, `_autosave` removed on a clean exit) and a history page (`:history`, `riptide://history/`) (2026-10-02)
- [ ] Recovery that survives signals, repeat crashes and crash loops; crashed-tab pages; crash reports by email or GitHub issue (M23)
- [x] Importing qutebrowser's history.sqlite (`:history-import`, read-only, skips redirects and duplicates) (2026-10-02)
- [x] Private windows (separate `CefRequestContext`) (2026-10-02)

---

## Milestones

### M0 — Spike (validate the stack) ✅ done 2026-10-02
- Workspace scaffolding; fetch CEF via `export-cef-dir`.
- Port `cefsimple` to a Views window that loads a URL on Linux/X11.
- Prove: `OnPreKeyEvent` can consume keys; two `BrowserView`s (content + UI overlay) layer correctly; a renderer-process message round-trips.
- **Exit criteria:** confirm Views + HTML overlay works, or fall back to OSR before writing more code.

**Result: CEF Views + HTML UI works; no OSR fallback needed.** Tested on CEF 154.0.32 / Chromium 154.0.8037.58 under Xvfb, driving the browser with xdotool:

| Check | Result |
|---|---|
| `OnPreKeyEvent` consumes keys | ✅ `5j`/`3j` scrolled exactly 320px; the page received none of the keys |
| Status bar `BrowserView` in a box layout | ✅ fixed 20px bar below a flexible page view |
| Completion overlay (`add_overlay_view`, custom docking) | ✅ floats above the status bar while typing `:scr` |
| Renderer → browser process message | ✅ `on_focused_node_changed` → `rt.focus` message → auto insert mode on clicking a text field |
| Clean shutdown via `:quit` | ✅ exit code 0, no leftover subprocesses |

Lessons learned:
- A view's preferred size with width or height `0` is treated as unset. The browser view then uses its large default and squeezes the page out of the layout. Always return a non-empty size.
- Dropping the last reference to a `BrowserView` closes its browser **synchronously**, which re-enters our handlers. Shared state is in a UI-thread `RefCell`. Never drop CEF objects, or call CEF methods that fire callbacks, while it is borrowed.
- Loading a `data:` error page from `OnLoadError` adds a history entry, so `back` loops back into the error. Instead, draw the error into Chromium's own error document from `OnLoadEnd`.
- The command line is a Rust-owned buffer. Keys are consumed in `OnPreKeyEvent` and the status bar just displays the text, so UI pages never need keyboard focus.

Known gaps carried forward:
- ~~The Chromium sandbox is disabled.~~ Since M8, the sandbox is on whenever Linux allows it (user namespaces, or a setuid `chrome-sandbox`). Otherwise the browser warns and runs without it (`rt_config::sandbox`). CI enables user namespaces and checks that the sandbox runs. The AppArmor profile in the README hasn't been tried on a real Ubuntu machine yet (it needs root). macOS and Windows still run unsandboxed until M10.
- Popups and `:open -t/-b/-w` load in the current tab until M3.
- When an event carries no character, key translation falls back to a US layout. Verify with other layouts.
- Status messages expire after 3 s. Completion covers only command names (M2 extends it).

M1 is essentially complete as a by-product (modes, key parser, scrolling, navigation, `o`, status bar). The M2 command line, history and `;;` chaining also exist. Remaining M1/M2 work: the command registry macro, `Tab` completion selection, and URL/history completion.

### M1 — Minimal vim browser
- `rt-core`: mode manager, key parser, command registry.
- Normal/insert modes, scrolling, back/forward/reload, `o` open.
- Status bar with mode, URL, load progress.

### M2 — Command line & completion
- `:` command line, parser, chaining, history.
- Fuzzy completion framework.

### M3 — Tabs ✅ done 2026-10-02
- Multiple tabs in one window, tab bar, close/undo, tab commands.

Notes: index logic is `rt_core::tabs::TabList` (unit tested). All tab `BrowserView`s share a fill-layout panel, and only the current one is visible. Popups go through CEF's `on_popup_browser_view_created`, so `window.opener` survives. A page's `window.close()` closes only its tab. Gaps: undo restores the URL only (not back/forward history), closing a tab skips `beforeunload` prompts, and there is still a single window.

### M4 — Hints ✅ done 2026-10-02
- JS hint engine, all hint targets, rapid mode.

Notes: labels use qutebrowser's scattered letter algorithm (`rt_core::hints::labels`, unit tested against qutebrowser's output). Clicks are real mouse events sent at the element's centre (`send_mouse_click_event`), so pages see `isTrusted` input and `target=_blank` links become tabs. Gaps: cross-origin iframes are hinted only as a whole (same-origin ones are searched since 2026-10-02); a page can interfere with hints on its own page by redefining `window.__rtHints`; labels for elements that move after the hints are drawn don't follow them.

### M5 — Config ✅ done 2026-10-02
- `config.toml` (settings, bindings, aliases, per-domain overrides); `:set`, `:bind`, live reload.
- qutebrowser-compatible setting names where they make sense.

Notes:
- **Settings registry:** `rt_core::settings`. 15 typed settings with validation; each one has a real effect. Values from TOML, Lua and `:set` all pass through JSON, so validation lives in one place.
- **Sources** (`rt-config`): `autoconfig.toml`, then `config.toml`, then `config.lua`. Each becomes a list of `ConfigOp`s the engine applies.
  - `:set`/`:bind`/`:unbind` persist to `autoconfig.toml`, never to the user's own files. They warn when a config file overrides the value at startup.
  - Bindings to unknown commands are rejected at load time.
- **Lua:** `mlua` with vendored Lua 5.4, so it builds on all three platforms with no system Lua. It provides the `c` proxy, `rt.*` and a config-dir `require` searcher. Errors read `file:line: message`, and changes made before an error still apply. `--lua-types` emits lua-language-server definitions generated from the registry. The checked-in `docs/lua/rt.meta.lua` and `docs/settings.md` are tested for staleness.
- **Paths:** XDG on Linux. On macOS, `~/.config` for config and Application Support for data. On Windows, `%APPDATA%` for config and `%LOCALAPPDATA%` for data. `XDG_*` is honoured everywhere, and `--basedir` overrides all. Unit tests cover all three platforms' rules, but macOS and Windows builds have not been run yet.

Gaps:
- ~~Per-domain settings (with M8). Answering a permission prompt with "always" should save a per-domain setting to `autoconfig.toml`, as qutebrowser does, which also fixes camera and microphone answers being forgotten.~~ Done in M8 (2026-10-02).
- Watching config files for changes (`:config-source` reloads by hand).
- `:config-edit`.
- Importing qutebrowser's `config.py`.

### M6 — Storage ✅ done 2026-10-02
- History, bookmarks, quickmarks, sessions.

Notes:
- **`rt-storage`:**
  - History is SQLite, with a per-visit table and a per-URL table for completion, like qutebrowser.
  - Quickmarks and bookmarks use qutebrowser's text formats, so you can import by copying the files. If an existing file can't be read, the browser treats it as read-only, so it's never overwritten with an empty list.
  - Sessions are TOML, with names restricted to safe file names.
- **Completion:** `rt_core::completion` decides what to offer, and the browser layer supplies the quickmark, bookmark, history and session sources. `Tab`/`Shift-Tab` cycle without re-querying, and the popup shows category headers and scrolls to the selection.
- **Settings:** `auto_save.session` and `completion.web_history.max_items`.

Gaps:
- Sessions restore only each tab's current page; CEF has no API to rebuild back/forward history.
- No crash-recovery autosave, no history page, no private browsing (history is always recorded).
- No import of qutebrowser's `history.sqlite`.

### M7 — Prompts, downloads, permissions ✅ done 2026-10-02
- Unified prompt UI; download manager; permission and auth dialogs.

Notes:
- **Prompts:** `rt_core::prompt` holds a queue answered one at a time in the new `prompt`/`yesno` modes, and `rt-cef/src/prompts.rs` connects each prompt to its CEF callback. JavaScript dialogs are withdrawn when their page navigates, and every prompt for a tab when the tab closes.
- **Lessons learned:**
  - Chromium ignores input to a page while it shows a JavaScript dialog, so prompt keys come through the status bar's browser, which gets focus for the duration.
  - CEF calls `GetAuthCredentials` on the IO thread, so the prompt is posted to the UI thread.
  - Chrome's own login prompt swallows HTTP auth unless `--disable-chrome-login-prompt` is set (cef#3603). CEF always runs Chrome's internals now, even for Alloy-style windows.
  - Chromium saves permission answers per site in the profile. So `y`/`N` map to accept/deny (saved), `n` to dismiss (not saved), and a `content.*=false` setting to ignore, so changing the setting later still works. Camera and microphone requests go through a separate CEF API that isn't saved, so they keep a session memory (`A`/`N`).
- **Downloads:** `rt_config::downloads` finds the platform Downloads folder (XDG on Linux), sanitises suggested names and picks unused names (unit tested).

Gaps:
- No per-download bar.
- No path completion in the save prompt.
- No command to reset per-site permissions (Chromium's saved answers).
- ~~Camera and microphone answers are forgotten on restart, so video-call sites ask every session.~~ `A`/`N` now save a per-site setting (M8).
- Closing a tab with `d` skips leave-page warnings.
- File-upload dialogs (`<input type=file>`) use CEF's default and are untested.
- TLS errors have no override.

### M8 — Content blocking & privacy
- ✅ **adblock-rust via `OnBeforeResourceLoad`** (2026-10-02). The new `rt-adblock` crate (CEF-free, unit tested) uses `adblock` 0.13 without its `single-thread` feature, so the engine is `Send + Sync` for CEF's IO thread.
  - EasyList and EasyPrivacy (135k rules) compile in ~56 ms (release) into a ~6 MB cache that loads in ~18 ms. A check takes ~1.5 µs.
  - `:adblock-update` downloads through `CefURLRequest`, so there's no HTTP client dependency, and `file://` lists work. It compiles on a worker thread.
  - Settings: `content.blocking.enabled`, `content.blocking.adblock.lists` and `content.blocking.whitelist`, all with qutebrowser's names.
  - Top-level navigations are never blocked. The smoke test serves a page on 127.0.0.1 and checks that a listed script is cancelled while another loads.
  - ✅ Cosmetic filtering (2026-10-02): after `on_load_end` the main frame gets a `<style>` with `url_cosmetic_resources` hide selectors (one rule per selector, so an invalid one can't void the rest). Unless the site has `generichide`, it also gets `hidden_class_id_selectors` for the page's classes and ids. Covered by unit tests and a smoke step.
  - Content added later is re-checked 2 s and 6 s after the load; the style skips rules it already has.
  - Not done: continuous re-checking (a MutationObserver would need a page-to-browser channel for web pages), subframes, procedural filters, scriptlets and `$redirect` resources, a blocked count in the status bar, automatic list updates, and qutebrowser's hosts-file method.
- ✅ **Per-domain settings** (2026-10-02):
  - `Settings` keeps `(pattern, name, value)` overrides for an allowlist (`settings::PER_DOMAIN`: the `content.*` permission settings and `content.blocking.enabled`). `get_for(name, url)` returns the last matching one.
  - `rt_core::url::pattern_matches` handles hosts, `*.` subdomains, origins with ports, and Chrome match patterns. It's shared with Greasemonkey.
  - `ConfigOp::SetFor` comes from `:set -u <pattern>`, `[per_domain."<pattern>"]` in TOML (autoconfig writes it the same way) and `rt.set(name, value, pattern)` in Lua.
  - `permissions::decide` uses the requesting origin's value, and content blocking checks the page's.
  - Permission answers `A`/`N` save a per-site setting, which fixes the user's note about camera and microphone answers being forgotten.
  - Unit tests cover each layer. A smoke step answers `A` to a geolocation request and checks `autoconfig.toml`. By hand: after deleting Chromium's data, the saved answer still allows without asking.
  - Gaps:
    - ~~Chromium also remembers `y` per site for permission prompts, and that memory wins over a later per-site `false`.~~ Exact-origin per-site values (what `A`/`N` save, or `:set -u https://host`) are now also written into Chromium's content settings (`SetContentSetting`), so a later `false` blocks. Tested by hand: `y`, then `:set -u <origin> content.geolocation false`, reload, and the site is denied without a prompt. Wildcard patterns still only apply when Chromium asks.
    - There are no per-site JavaScript, cookie or image settings yet.
- ✅ **Multiple and private windows** (2026-10-02):
  - `shell::WindowState` holds a window's views, tabs, closed-tab list and redraw caches. `Shell` keeps a never-empty list and derefs to the active one, so the single-window code kept working.
  - `with_tab` finds a browser in any window and makes that window the shell's for the closure. A key or a tab-bar click makes its window active, as does `on_window_activation_changed`.
  - `refresh_ui` redraws every window. Only the active one shows the mode, command line, messages and overlay.
  - `:open -w` and `-p` open windows, `:close` closes the current one, and `:quit` saves the session once and closes all. Sessions keep every non-private window, active first, and restore opens the others.
  - Private windows share one `CefRequestContext` created with an empty cache path (in memory). Popups inherit it. History isn't recorded, and the status bar is gray.
  - Smoke steps cover a second window taking keys (`]]` there doesn't affect the first), `:close`, and a private page staying out of history.
  - Gaps:
    - `:tab-give`/`:tab-take` and moving tabs between windows.
    - Each window opens at 1280×800 at the origin, and window geometry isn't saved.
    - The `{private}` title field.
    - Downloads from private windows still go to the downloads directory and list.
- ✅ **Background Google traffic reviewed** (2026-10-02). Method: a fresh profile on `about:blank` for 90 s with `--log-net-log`. Each request was mapped to its Chromium source through its traffic-annotation hash (`hash(id) = fold(c, h*31 + c) mod 138003713` over `tools/traffic_annotation/summary/annotations.xml`). Results are in the README's "Network traffic" table. Before: 8 Google hosts and 122 MB downloaded. After: CRLSets, subresource filter rules, network time and one `ListAccounts`, 5.8 MB.
  - Changes are in `rt-cef/src/privacy.rs`: `component_updates.component_updates_enabled = false` (Chromium still updates the components it exempts as security data, as with the `ComponentUpdatesEnabled` policy), spell-check dictionaries off, sign-in off, Chrome's default search engine off, and `--disable-features=AimEnabled,PreconnectToSearch,SearchEnginePreconnect2`, merged with any `--disable-features` the user passes.
  - Prefs are written into `Local State` and `Default/Preferences` before CEF starts, because these services start within 100 ms. `CefPreferenceManager::SetPreference` from `on_context_initialized` is too late. Through cef-rs it also fails silently unless the `error` out-string is non-empty, since an empty `CefString` is passed as NULL.
  - Feature names in `libcef.so` strings carry a `k` prefix that Chromium strips at runtime (`kAimEnabled` → `AimEnabled`). Class names such as `AimEligibilityService` are not features.
  - The profile lives in `data/Default`: Chromium uses that name whatever `cache_path` says, so `cache_path` now points there.
  - Left: `accounts.google.com/ListAccounts` (`gaia_auth_list_accounts`) still runs once at startup. Some startup service asks `GaiaCookieManagerService` for the cookie jar, which isn't found yet.
  - **Widevine is no longer downloaded**, since it updates through the component updater. M11 must find a way to update only the Widevine component when the user opts in.
- Original item: Review background Google service traffic (component updater, Safe Browsing, optimization hints, Variations, CRLSets). Keep security updates; disable or make opt-in what only serves Google. Document in the README.

### M9 — Power features
- Caret mode, marks, macros, userscripts, greasemonkey, `:open-editor`, search engines.
- ✅ **`:spawn`, userscripts and `:open-editor`** (2026-10-02):
  - `rt_core::shell_words` splits arguments like a POSIX shell, with no shell involved.
  - `rt_config::userscripts::resolve` searches config, then data, then `PATH`.
  - `rt-cef/src/spawn.rs` runs programs on a worker thread and reports back through a UI task. Userscripts get qutebrowser's `QUTE_*` environment. `QUTE_HTML`, `QUTE_TEXT` and `QUTE_FIFO` live in a private 0700 temp directory that is removed afterwards.
  - `editor.command` is validated to contain `{file}`. `js/editor.js` remembers the field and writes the text back with `input`/`change` events. It's bound to `Ctrl-e` in insert mode.
  - Smoke steps cover a userscript (environment plus a FIFO command) and Ctrl-e with a scripted editor.
  - Gaps:
    - `QUTE_FIFO` is a regular file read when the script exits, not a live FIFO, so long-running scripts' commands are delayed until they exit.
    - `-o` shows the output in a new tab (`riptide://process/`).
    - Hints can run programs and userscripts: `:hint links spawn …` with `{hint-url}` shell-quoted, and `:hint links userscript …` with `QUTE_MODE=hints`.
    - `QUTE_USER_AGENT` comes from the page's `navigator.userAgent`.
    - Password fields are skipped by `:open-editor`.
    - The remote socket can run `:spawn`; it is limited to the same user (M15).
- ✅ **Marks** (2026-10-02): `` ` `` and `'` enter the `set_mark` and `jump_mark` modes (qutebrowser's names), and the next key names the mark (`Command::Mark`, unit tested). `rt-cef/src/marks.rs` reads and sets `scrollX`/`scrollY` through the eval channel. Uppercase marks also reopen their page and scroll once `on_load_end` fires. `''` goes back to where the last jump started. Marks last for the session. A smoke step covers `` `a ``, `gg`, `'a` and `''`.
- ✅ **Macros** (2026-10-02):
  - `q` and `@` enter `record_macro`/`run_macro` (qutebrowser's names). `:macro-record [r]` and `:macro-run [r]` work too, `@@` repeats the last macro, and a count repeats it.
  - The engine records every key in any mode, then drops the keys that stopped the recording: the binding, or the `:macro-record` command line.
  - Replay feeds the keys back through the engine. Keys it doesn't consume become `Effect::PassKey`, which `client::send_to_page` turns into RAWKEYDOWN/CHAR/KEYUP using `vk::to_raw` (round-trip tested). A flag keeps those events from going through the engine a second time. Nesting stops at 10.
  - The status bar shows `recording @r`. Smoke steps cover a scroll macro and a macro that types into a field.
  - Gaps:
    - Replay doesn't wait for asynchronous results (a hint's page reply, a new tab loading), so keys after them can run too early, much as in qutebrowser.
    - Registers last for the session.
    - Characters outside a US layout are replayed as text without a key code.
- ✅ **Caret mode** (2026-10-02):
  - `v`/`V` enter `caret` mode with qutebrowser's bindings and command names (`move-to-next-word`, `selection-toggle [--line]`, `selection-reverse`, `yank selection`). It shares the normal-mode binding logic (counts, `gg`); unbound keys stay out of the page.
  - `js/caret.js` starts at the first text on screen (`caretPositionFromPoint`), moves with `Selection.modify`, draws its own caret (Chromium only draws one in editable text) and scrolls to keep it visible. Leaving clears the selection.
  - A smoke step selects "brown fox" with `w w v e e`.
  - `Y` yanks the selection to the primary selection.
  - `{`/`}` move by paragraph. `Return` in normal mode follows the link around a search match (`selection-follow`, which ends the find session so the match becomes the selection) or the focused link, and otherwise passes `Return` to the page.
- ✅ **Greasemonkey** (2026-10-02):
  - `rt_config::greasemonkey` parses the metadata block and matches URLs (Chrome match patterns plus `@include`/`@exclude` globs), with unit tests.
  - The browser passes the scripts to each tab's renderer in `extra_info`. `:greasemonkey-reload` sends them as a process message.
  - The renderer runs them from `on_context_created`: `document-start` runs before the page's scripts, `document-end` waits for `DOMContentLoaded`, and `document-idle` waits for `load`.
  - Lists carry a generation number: CEF passes a browser's original `extra_info` to `on_browser_created` again on reload, which would otherwise undo a reload.
  - A smoke step checks start before the page's script, end after it, and `GM_addStyle`.
  - Gaps:
    - Scripts run in the page's main world.
    - Only `GM_info`, `GM_addStyle` and `unsafeWindow` exist; no `GM_setValue` or `GM_xmlhttpRequest`.
    - Popups opened by pages don't get `extra_info`. They usually share their opener's renderer, which already has the scripts.
    - A tab that moves to a new renderer process after `:greasemonkey-reload` gets the older list until it's reopened.
- ✅ **Page search** (2026-10-02):
  - `/` and `?` put the prefix on the command line. The engine turns `/text` into `Command::Search` on `Return`, and into incremental searches while typing (`search.incremental`); Escape clears the highlights.
  - `n`/`N` are `:search-next`/`:search-prev` and take a count.
  - `rt-cef/src/search.rs` uses `BrowserHost::Find` and a `FindHandler`, which reports "Match i of n" or "not found".
  - In this CEF build `find_next = false` never activates or scrolls to a match. Every call passes `true`; repeating the same text first calls `StopFinding(clear_selection)`, so it starts again from the top.
  - `search.ignore_case` is `smart`, `always` or `never`. Chromium always wraps around.
  - A smoke step checks that `/needle` scrolls to the first match and `n` to the second.
- ✅ **`:navigate`** (2026-10-02): `up`, `increment` and `decrement` are pure URL functions in `rt_core::url` (unit tested: query and fragment first, leading zeros kept, the host never touched). `prev`/`next` use `js/navigate.js`: `rel` links first, then link text matching qutebrowser's default `hints.prev_regexes`/`hints.next_regexes`. These are bound to `gu gU [[ ]] {{ }} Ctrl-a Ctrl-x` as in qutebrowser and covered by a smoke step. The regexes aren't settings yet.

### M10 — Packaging
- Linux tarball / AppImage / AUR / Nix; then macOS app bundle (`bundle-cef-app`) and Windows.
- ✅ **Tarball and AppImage** (2026-10-02): `scripts/package-linux.sh [--appimage]` stages the stripped binary and CEF runtime. The AppImage adds `packaging/riptide.{desktop,svg}` and an `AppRun`, and is built by appimagetool 1.9.1 (pinned and checksum-verified; it fetches its runtime itself). `./task appimage` builds both locally, and `release.yml` publishes both.
  - Tested locally: the 146 MB AppImage starts, opens a window under Xvfb, quits cleanly and unmounts.
  - Inside an AppImage `chrome-sandbox` can't be setuid, so the sandbox needs user namespaces (see the README).
  - Not done: AUR, Nix, the macOS app bundle and Windows packaging (none can be tested on this machine).

### M12 — Lua scripting ✅ mostly done 2026-10-02
Builds on the M5 Lua config API.

Result:
- The VM that ran `config.lua` is kept in a UI-thread `thread_local` (`rt_config::lua`, since `mlua::Lua` isn't `Send`). Callbacks get a `Context` (URL, title, mode, count) and return `Action`s (`Run(line)`, `Message`), which `rt-cef/src/lua.rs` carries out outside any shell borrow. A depth limit stops hooks that trigger each other.
- `rt.bind(keys, function)` stores the function and binds `lua-call <id>`.
- `rt.command(name, fn, description)` defines commands. The engine parses them as `Command::User`, accepts bindings to them and completes them next to built-ins; built-in names are refused.
- `rt.on` supports `load_finished`, `url_changed`, `tab_opened` and `mode_changed`.
- In callbacks, `rt.set` becomes a `:set`.
- Unit tests in rt-config drive the VM without CEF (bindings, commands, hooks, errors with `config.lua:line`). A smoke step uses all three entry points.
- Lua commands appear on the help page, after the built-in ones.
- Not done: a sandbox for third-party scripts (only the user's own config runs), settings watchers, and Lua userscripts. `rt.tabs()` lists the window's tabs.

Original plan:
- Bind keys to Lua functions: `rt.bind("<Ctrl-g>", function() ... end)`.
- Lua-defined commands: `rt.command("name", fn)`, with completion.
- Event hooks: `rt.on("load_finished", fn)`, `rt.on("tab_opened", fn)`, mode changes.
- A small runtime API: current tab URL and title, open URLs, run commands, show messages.
- Userscripts written in Lua, alongside qutebrowser-compatible external userscripts (M9).
- Decide on a sandbox for third-party scripts (e.g. no `io`/`os` unless allowed). The user's own `config.lua` stays fully trusted.

### M13 — Internal pages and a UI channel (foundation for M14 and M16) ✅ done 2026-10-02

Result:
- **Scheme:** `riptide://` is registered as standard + secure + display-isolated and served from embedded files by `rt-cef/src/scheme.rs`. Responses carry a strict CSP (inline code only, no network), `nosniff` and `no-store`.
- **UI pages:** the tab bar, status bar and overlay moved from `data:` URLs to `riptide://ui/…`.
- **Channel:** `rt.send(name, json)` exists only in `riptide://ui/` frames. The browser re-checks the sending frame's URL itself, and `rt_core::ui_message` validates each message against a per-page allowlist (unit tested).
- **First message:** clicking a tab in the tab bar selects it.
- **Isolation, as verified:**
  - Web pages see no `window.rt`.
  - An `riptide://` iframe stays empty, and an `riptide://` link does nothing; Chromium's display isolation refuses both.
  - `:open riptide://ui/…` and redirects to `riptide://` are blocked by `OnBeforeBrowse`.
  - A smoke test covers the first two.

Original plan:
- **`riptide://` scheme:** register it with `CefSchemeRegistrar::AddCustomScheme` and serve it from embedded files through a `CefSchemeHandlerFactory`. Move the tab bar, status bar and overlay pages off `data:` URLs onto `riptide://ui/...`.
- **UI → Rust messages:** in the renderer's `OnContextCreated`, add a `window.rt.send(name, json)` function **only for frames whose URL is `riptide://`**. Web pages never see it.
  - The browser process accepts these messages only from our UI browsers, and still validates every field. This is the reverse of the eval channel, needed for clicks in the tab bar and for links on the help page.
- Opening `riptide://` from a web page (link, redirect, `window.open`) is blocked in `OnBeforeBrowse`. Only the user (`:open riptide://help`) or the browser itself can open it.

### M14 — Tabs: pinned, mouse, favicons ✅ done 2026-10-02

Result:
- **Pinned tabs:** `TabList` keeps them first (unit tested: pinning, moves, inserts and removals stay outside or inside the block as they should). Added `:tab-pin` / `Ctrl-p`, `--force` for `tab-close`/`tab-only`, the `tabs.pinned.frozen` and `tabs.pinned.shrink` settings, and pin state in sessions (older session files still load).
- **Mouse in the tab bar:** click, middle-click to close (pinned tabs refuse), wheel (`tabs.mousewheel_switching`) and drag to reorder, all as allowlisted `rt.send` messages. Drag uses pointer events rather than HTML5 drag and drop, so it never involves the OS or other applications.
- **Favicons:** from `OnFaviconURLChange` and `DownloadImage` (32 px, at most 64 KB as PNG), shown per `tabs.favicons.show`.
- **Bugs found on the way:**
  - The `cef` crate's `CefStringList::clone` copies the opaque C struct, so iterating a clone is always empty; we read the list through the C API instead. Worth reporting upstream.
  - A new tab could miss keyboard focus requested before its browser existed (`on_after_created` now re-focuses it).

Not done:
- Favicons in `:open` completion and cached in history.
- Saving favicons in sessions (they're re-fetched when the page loads).
- Middle-clicking links in pages to open background tabs works through Chromium's popup handling but isn't tested yet.
- Mouse back/forward buttons.
- **Lost keys after `:open -t` (seen under Xvfb only):** a key sent by `xdotool` just as a tab opened from the command line finishes loading is dropped about 1 time in 5. It never reaches `OnPreKeyEvent`, `CefWindowDelegate::OnKeyEvent` or a high-priority window accelerator, so it's lost below Views (X11/aura). Tabs opened through the remote command don't lose keys (0/16), nor do keys sent 0.3 s after the load (0/12). It hasn't been seen with a real keyboard and window manager yet. The smoke test pauses 0.3 s after that step; check again on a real desktop and with Wayland (M10).

Original plan:
- **Pinned tabs** (qutebrowser's `:tab-pin`, `Ctrl-p`):
  - Pinned tabs sit at the left and shrink to favicon + number.
  - `tab-close` and `tab-only` skip them unless given `--force`; `tab-move` can't push an unpinned tab in among them.
  - `tabs.pinned.frozen` (default true) makes `:open` in a pinned tab open a new tab instead. `tabs.pinned.shrink` controls the shrinking.
  - Pin state is saved in sessions: add `pinned = true` to `TabState`, with `#[serde(default)]` so old session files still load.
  - The core logic goes in `TabList` (pinned count, move/close rules) with unit tests.
- **Mouse support:**
  - Tab bar (via the M13 channel): left-click switches, middle-click closes, wheel cycles, and dragging reorders (using `TabList::move_current` semantics).
  - Pages: middle-click on a link opens a background tab. Chromium already reports this as `NEW_BACKGROUND_TAB` to `on_before_popup`; verify it and honour `tabs.background`.
  - Mouse back/forward buttons map to `back`/`forward`.
  - Setting `tabs.mousewheel_switching` (default true).
- **Favicons:**
  - `DisplayHandler::OnFaviconURLChange` supplies the icon URLs, and `BrowserHost::DownloadImage(url, is_favicon=true, max_size=32)` fetches the image. `CefImage::GetAsPNG` converts it to a `data:image/png` URL for the tab bar.
  - Cached per URL for the session. Also shown in `:open` completion, cached in `history.sqlite`, and saved in sessions.
  - Setting `tabs.favicons.show` (`always`, `never`, `pinned`).
  - Fetching icons is a network request the page asked for anyway, so it adds no new tracking. Size and format are capped, and only images are accepted.

### M15 — Commands from the terminal (single instance + IPC) ✅ done 2026-10-02 (Unix)

Result:
- **Code:** `rt_config::remote` (protocol, socket paths, client, server, argument handling; unit tested over a real socket) and `rt-cef/src/remote.rs`.
- **Startup:** before CEF starts, the browser tries the profile's socket. If an instance answers, it hands over its arguments (URLs per `new_instance_open_target` or `--target`, `:commands` run in order) and exits. Otherwise it binds the socket and serves it once CEF is up, posting each request to the UI thread and raising the window. A first instance also runs `:commands` given on its command line.
- **Security:**
  - Socket in a `0700` directory, mode `0600`, one per data directory.
  - A stale socket from a crash is replaced; a live one is never taken over.
  - Requests are versioned JSON lines, capped at 1 MB.
- **Tests:** a smoke step sends a URL and a command from a second invocation.

Not done:
- Windows named pipes (each start is a new instance there).
- Errors from remote commands aren't sent back to the caller (it's one-way, as in qutebrowser).
- Chromium exits on SIGTERM without our cleanup, leaving a stale socket that the next start replaces.

Original plan:
- **Behaviour:** `riptide example.com` or `riptide ':open -t example.com' ':tab-focus 1'`, run while the browser is already open, sends the URLs and commands to that instance and exits, like qutebrowser.
  - A URL opens per `new_instance_open_target` (`tab`, `tab-bg`, `window`).
  - An argument starting with `:` runs as a command.
  - `--target` overrides the open target for one call.
- **Transport:** a local socket per profile.
  - On Unix, `$XDG_RUNTIME_DIR/riptide/<hash of basedir>.sock`, or the data dir if `XDG_RUNTIME_DIR` isn't set. The directory is `0700` and the socket `0600`, and the server checks the peer's user id (`SO_PEERCRED` / `getpeereid`).
  - On Windows, a named pipe restricted to the current user.
  - The [`interprocess`](https://crates.io/crates/interprocess) crate covers both. The protocol is versioned JSON lines (`{"version":1,"args":[…],"cwd":"…","target":…}`).
- **Startup order:** check for a running instance before CEF initialises, because Chromium's profile lock would otherwise refuse the second process. Remove a stale socket left behind by a crash.
- **Security:** anyone who can write to the socket can run commands, including `:spawn` once that exists (M9). Only the same user may connect, and nothing listens on the network.
- The same channel later serves userscripts' `QUTE_FIFO`-style command input (M9 and M12).
- **Tests:** protocol and argument handling in a CEF-free crate. A smoke step sends `:open -t` to the running test browser.

### M16 — Help pages ✅ done 2026-10-02

Result:
- **Data:** `rt_core::help::build` assembles commands (with the keys bound to them, including `cmd-set-text` prefills), settings (current value, default, type, and the file that set it) and per-mode bindings (with changed and removed ones marked) from the live registries. Unit tested.
- **Page:** `rt-cef/src/help.rs` fills `ui/help.html` with that JSON and rebuilds it after every config load and `:set`/`:bind`. The scheme handler serves it from a shared `RwLock`.
- **Look:** light/dark themes, sticky search (`/`), side navigation and anchors (`:help :open`, `:help hints.chars`, `:help bindings`, `:version`). Built with `textContent` only.
- **Commands:** `:help [-t] [topic]`, `:version` and `F1`. `--version` now prints the git commit and CEF/Chromium versions (from `rt-cef/build.rs`).
- A smoke step opens `:help :open`.

Done: Lua-defined commands are listed too (M12).

Original plan:
- **`:help [topic]`** opens `riptide://help`, a set of pages generated from the live registries, so it is always current:
  - **commands:** name, arguments and description from `COMMANDS`, with any `config.lua`-defined commands added once M12 exists
  - **settings:** type, default, *current value* and where it was set (default, `config.toml`, `config.lua` or `:set`)
  - **key bindings:** per mode, including the user's bindings, with changes from the defaults marked
  - plus pages on modes, hints, the config files and their paths, and the Lua API
  - `:help :open` and `:help hints.chars` jump straight to an entry.
- **Look:** clean and readable in light and dark themes (following `prefers-color-scheme`), keyboard-first.
  - `f` hints and `/` search work normally.
  - A search box filters commands and settings as you type, and the page works without a mouse.
- `:version` (`riptide://version`) shows the version, git commit, CEF/Chromium version, the paths from `--paths`, and the loaded config files.
- Bindings: `F1` and `:help`, as in qutebrowser.
- **Tests:** the generated pages render without errors (a smoke step opens `:help` and checks the title). A unit test checks every command and setting appears.

### M17 — Spell checking ✅ done 2026-10-02 (except the offline dictionary installer)

Result:
- **Setting:** `spellcheck.languages` (list, default empty, so spell checking is off). Changes apply live through `RequestContext::SetPreference` (`browser.enable_spellchecking`, `spellcheck.dictionaries`), called outside the shell borrow. `privacy.rs` still writes spell checking off before startup, and the setting turns it on again.
- **Keyboard:** `:spell-suggest` runs `js/spell.js` to find the word at the text cursor (a mirror element for inputs and textareas, the selection range for contenteditable), then right-clicks its centre. `OnBeforeContextMenu` takes `GetMisspelledWord`/`GetDictionarySuggestions`, clears the menu so it never shows, and opens `:spell-replace ` with the suggestions as completions (`CompletionKind::Spelling`). `:spell-replace` calls `ReplaceMisspelling` and goes back to insert mode; `:spell-add` calls `AddWordToDictionary`.
- Tested by hand under Xvfb: "teh" → the, eh, tech, tee, tea; replacing it, a word with no suggestions, and `:spell-add`. There's no smoke step because the dictionary comes from Google's servers.
- Not done: `--install-dictionary` (offline `.bdic` with pinned checksums), a default key binding, and hints for misspelled words.

Original plan:
- **Chromium's spell checker is built into CEF.** Turn it on per profile with `RequestContext::SetPreference("browser.enable_spellchecking", true)` and `spellcheck.dictionaries = [...]`. Keep `spellcheck.use_spelling_service = false` so typed text is never sent to Google.
- **Settings:** `spellcheck.languages` (list, default empty, so spell checking is off). Applied live when changed.
- **Dictionaries:**
  - Chromium downloads `.bdic` files from `redirector.gvt1.com` the first time a language is enabled. That is part of the M8 Google traffic review.
  - Also offer `riptide --install-dictionary en-US`, like qutebrowser's `dictcli`, which fetches from the Chromium dictionary repository and verifies a pinned checksum.
  - Document both and let the user choose.
- **Fixing words from the keyboard:**
  - `:spell-suggest` puts the suggestions for the misspelled word under the cursor in the completion popup (`Tab` to pick, `Return` to replace). It uses `CefContextMenuParams::GetDictionarySuggestions` / `BrowserHost::ReplaceMisspelling`, or a renderer query if the context-menu path needs a right-click.
  - `:spell-add` (`AddWordToDictionary`) adds the word to your dictionary.
  - Right-click suggestions come for free through CEF's context menu.
- Open question: should spell checking stay off by default (privacy-friendly) or follow the system locale?

### M18 — Versioning, changelog and CI
- **One version for the whole workspace** (`workspace.package.version`), following semver. Stay on 0.x until the plan's core is done.
- **The binary reports what it is:**
  - `--version` prints e.g. `riptide 0.4.0 (abc1234, CEF 154.0.32, Chromium 154.0.8037.58)`.
  - The git commit comes from a `build.rs` (`git describe --always --dirty`), falling back to "unknown" in source tarballs.
  - CEF and Chromium versions come from `cef::sys` constants.
  - `:version` shows the same (M16).
- **Changelog:** `CHANGELOG.md` generated by [git-cliff](https://git-cliff.org) from commit messages; cef-rs uses the same setup.
  - Releases are tagged `vX.Y.Z` and the release notes come from the changelog.
  - In the browser, `:changelog` opens the bundled `CHANGELOG.md` (`riptide://changelog`), and the first start after an upgrade shows "Updated to 0.5.0. :changelog for details" in the status bar.
- **CI (GitHub Actions)** ✅ `check.yml` added 2026-10-02 (commit messages, Linux lint/tests/smoke, macOS and Windows build + unit tests).
- ✅ **Done 2026-10-02:**
  - `cliff.toml` and `CHANGELOG.md`. Pre-Conventional "Add …/Scaffold …" commits are sorted under Features.
  - `scripts/git-cliff.sh` downloads a pinned, checksum-verified git-cliff, like `./task`. `./task changelog` regenerates the changelog.
  - `release.yml` runs on a `v*` tag: it checks the version, builds `--release`, and publishes `scripts/package-linux.sh`'s tarball with `SHA256SUMS` and `git-cliff --latest` notes. The libraries are stripped: CEF's `libcef.so` has debug info and goes from 1.4 GB to 260 MB, giving a 156 MB tarball.
  - `:changelog [-t]` serves the bundled changelog at `riptide://changelog/` through `rt_core::changelog::to_html`, a minimal, escaping Markdown renderer that is unit tested. The "Updated to X" notice compares `<data>/last-version`.
  - No release has been tagged yet; that's the maintainer's call. Running `release.yml` by hand is a dry run that keeps the files as a one-day artifact. Run 37088697211 (2026-10-02) built the tarball, the AppImage, `SHA256SUMS` and the notes (301 MB in total) with publishing skipped.
  - ~~Not done: macOS and Windows release artifacts (M10).~~ Experimental packages since 2026-10-05 (below).
- ✅ **Release automation** (2026-10-05):
  - **One-click releases:** `release.yml` is started from Actions. It works out the version with `git-cliff --bumped-version` (`[bump] initial_tag = "v0.1.0"` in `cliff.toml`), or takes one you type. It then commits `chore(release): vX.Y.Z` with `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`, and pushes the commit and the tag together (`--atomic`). Build and publish follow in the same run, because a tag pushed with the workflow's own token doesn't trigger other workflows. Dry run is the default; pushing a tag by hand still works.
  - **Shared builds:** `build-release.yml` (`workflow_call`) builds for releases and nightlies alike. On Linux it runs the full smoke test against the unpacked tarball and the extracted AppImage before anything is published. macOS and Windows build experimental packages (`scripts/package-experimental.sh`, with `continue-on-error`).
  - **Nightlies:** `nightly.yml` runs at 07:00 UTC when `main` has changed, and replaces the rolling `nightly` pre-release.
  - **Provenance:** releases and nightlies get `SHA256SUMS` and `actions/attest-build-provenance` attestations (`gh attestation verify`).
  - Not done: macOS signing and notarization, and a Windows installer (M10). Packages for AUR and Nix. A weekly check for new CEF releases.
- **Commit messages:** Conventional Commits, decided 2026-10-02 and checked by `scripts/check-commits.sh` in CI and in the optional `./task hooks` git hook.
- Original CI plan:
  - `check.yml` on every push/PR runs `./task lint test smoke` on Ubuntu, with Xvfb and the CEF download cached by version.
  - macOS and Windows jobs build and run the unit tests (`--paths` checks), which verifies the cross-platform path code early.
  - `release.yml` on a `v*` tag builds release binaries, attaches them to a GitHub release, and publishes the changelog section. This feeds M10's packaging later.
  - Optionally use [release-plz](https://release-plz.dev) to open "release vX.Y.Z" PRs that bump the version and changelog automatically.

**Suggested order:**
1. M18's CI part first, because it catches regressions on all three platforms from now on.
2. M13, since M14 and M16 depend on it.
3. M14.
4. M16.
5. M15.
6. M17, after M8's Google traffic review.

The rest of M18 (releases) can land whenever the first release is cut.

### M11 — Widevine DRM (opt-in) ✅ mostly done 2026-10-02
Depends on M5 (settings) and the M8 component review.

Result:
- `content.widevine` (off by default) is read before CEF starts, like dark mode. When it's on and `<data>/WidevineCdm/<version>/manifest.json` is missing, `privacy::seed_prefs` turns component updates on for that run, since Chromium has no per-component switch.
- A watcher thread reports "Widevine downloaded; restart to enable it". At the next start updates are off again, and Chromium still registers the installed CDM from disk.
- Tested 2026-10-02 in a scratch profile: run 1 downloaded 4.10.3050.0, and run 2's `requestMediaKeySystemAccess('com.widevine.alpha', vp9)` resolved, with `component_updates_enabled` back to false.
- Gaps:
  - The CDM is never updated afterwards (delete `<data>/WidevineCdm` to fetch it again).
  - Turning the setting off doesn't remove a downloaded CDM.
  - Protected playback against a real stream isn't verified.
  - ~~Startup messages share the one status-bar slot.~~ Messages now stack: the newest is in the status bar and up to four older ones show above it, each expiring on its own.
  - The licensing review below is still open.

Tested 2026-10-02 on CEF 154 / Linux with the stock (Spotify CDN) build and no code changes:

| Check | Result |
|---|---|
| VP9 / AV1 / Opus | ✅ supported |
| H.264 / AAC | ❌ not in prebuilt CEF builds |
| Widevine at first launch | ❌ not available |
| ~1 min later | CEF's component updater downloaded Widevine 4.10.3050.0 into the profile |
| Widevine after a restart | ✅ `com.widevine.alpha` available with VP9 |

Actual protected playback is not yet verified.

`CefRegisterWidevineCdm` no longer exists ([cef#3149](https://github.com/chromiumembedded/cef/issues/3149)). There is nothing to bundle or register: CEF fetches the CDM from Google itself.

Work:
- A `content.widevine` setting, **off by default**. When off, Widevine is neither downloaded nor loaded; when on, the component updater fetches it.
- After the first download, show "Widevine downloaded; restart to enable" (on Linux the CDM loads only at the next launch).
- A test page or smoke check that reports EME support, plus a manual playback test against a public Widevine demo stream.
- Document the limits:
  - Only VP9/AV1 content works (no H.264/AAC).
  - Linux Widevine is the software-only level (L3), which services often limit to lower resolutions.
  - Windows/macOS builds would need VMP signing for many services.
- **Before release, a qualified reviewer must check the licensing:** Google's Widevine terms for third-party browsers, and a GPL-3.0 application loading a proprietary CDM at runtime.

### M19 — qutebrowser parity sweep

On 2026-10-05 I compared qutebrowser's own lists with ours: its 172 commands (`doc/help/commands.asciidoc`) and 354 settings (`doc/help/settings.asciidoc`). Riptide parses 127 commands. Most of what's missing is below, grouped by what users notice. **Tier 1** items come first because they matter in daily use.

- **Tabs** (tier 1: position and visibility):
  - ✅ (2026-10-05) `tabs.position` (top, bottom, left, right) with `tabs.width` for vertical tabs, and `tabs.show` (always, never, multiple, switching) with `tabs.show_switching_delay`. All of them apply live.
    - Layout: the window is a column (tab bar when top or bottom, a row, the status bar), and the row holds the page area plus a left or right tab bar. `window::place_tabbar` moves the tab bar's view between them.
    - Its preferred size is kept outside the shell, because CEF asks for sizes during layout.
    - The visibility rule (`rt_core::tabs::bar_visible`) is unit tested, and a smoke step checks the page's size with the bar on the left and hidden.
  - ✅ (2026-10-05) `tabs.title.format` and `format_pinned` (`{index}`, `{aligned_index}`, `{current_title}`, `{current_url}`, `{host}`, `{perc}`, `{audio}`, `{private}`), formatted in `rt_core::title::tab_label`, and `tabs.tooltips`. Still to do: `tabs.title.alignment`, `min_width`, `max_width`.
  - ✅ (2026-10-05) `tabs.select_on_remove` (next, prev, last-used), `tabs.wrap`, `tabs.undo_stack_size`. `tabs.background` isn't needed: Chromium's link dispositions already open middle-clicks in the background. Still to do: a focus stack (`tab-focus stack-prev/stack-next`, `tabs.focus_stack_size`).
  - Middle-click to close (`tabs.close_mouse_button`, `close_mouse_button_on_bar`), `tabs.tooltips`, `tabs.tabs_are_windows`.
- **Status bar** (tier 1):
  - ✅ (2026-10-05) `statusbar.position` (top, bottom) and `statusbar.show`, where `never` still shows the bar while a command is typed or a prompt answered, and `in-mode` also outside normal mode and while a message is up. Both bars are arranged by `window::arrange_bars`, and the completion overlay opens on the page's side of the status bar.
  - ✅ (2026-10-05) `statusbar.widgets` with qutebrowser's names (keypress, search_match, url, scroll, scroll_raw, history, tabs, progress, `clock[:format]`, `text:…`) plus downloads, muted and zoom. Unknown names are rejected.
    - `scroll` reads the current tab's position twice a second while the widget is shown, rather than giving pages a hook that would let them detect the browser.
    - The smoke test can't read the status bar's text, so the widgets were checked by screenshot.
  - `statusbar.padding` waits for M20's measured bar sizes.
- ✅ **Key hints** (tier 1, 2026-10-05): after `keyhint.delay`, the overlay lists what a pending key chain can still become, minus `keyhint.blacklist` globs.
  - The entries are laid out in columns, letters first.
  - `Engine::keyhints` and `Keymap::continuations` are unit tested; the layout was checked by screenshot.
- **Completion:**
  - ✅ Tier 1 (2026-10-05): `completion-item-del` (`Ctrl-d`: delete a history, quickmark, bookmark or session entry, or close a tab) and `completion-item-yank` (`Ctrl-c`, `Ctrl-Shift-c` for the primary selection). A smoke step deletes a history entry and checks `history.sqlite`.
  - `completion.height`, `shrink`, `show`, `open_categories` (order and which ones), `web_history.exclude`, `timestamp_format`, `min_chars`, `delay`, `use_best_match`, `quick`, and `cmd_history_max_items`.
  - Command-line commands: ✅ `cmd-edit` (2026-10-05); still to do: `cmd-repeat`, `cmd-repeat-last`, `cmd-run-with-count`.
- **Hints:**
  - ✅ Tier 1 (2026-10-05):
    - `hints.auto_follow` (always, unique-match, full-match, never), with `hint-follow` on `Return`, and `hints.auto_follow_timeout`.
    - `hints.selectors`: user groups are merged over the built-in all, links, images, media and inputs.
    - Rapid mode (`hint --rapid`) was already done.
  - `hints.min_chars`, `hints.scatter`, `hints.dictionary` (word hints), `hints.leave_on_load`, `hints.hide_unmatched_rapid_hints`, `hint-follow`, and `hints.next_regexes`/`prev_regexes` as real settings.
- **URLs:**
  - ✅ Tier 1 (2026-10-05): `edit-url` (edit the URL in the editor) and `url.yank_ignored_parameters` (drop `utm_*`, `ref`, `fbclid` and `gclid` when yanking).
  - `url.auto_search` (naive, dns, schemeless, never), `url.open_base_url`, `url.incdec_segments`, `new_instance_open_target_window`.
- **Downloads and files:**
  - ✅ Tier 1 (2026-10-05):
    - `download-retry`, `download-remove [--all]` and `download-delete`.
    - The external file picker: `fileselect.handler = external` with `fileselect.{single_file,multiple_files,folder}.command`. CEF's `DialogHandler` hands upload fields to the program, and the callback waits on the UI thread while it runs.
    - Smoke steps cover `download-delete` and the picker. `download-retry` isn't smoke tested (no failing download to retry).
    - ✅ `prompt-fileselect-external` (`Alt-e` in a file prompt) fills the prompt with the folder `fileselect.folder.command` picks; smoke tested.
  - `downloads.remove_finished`, `downloads.position`, `downloads.location.remember` and `suggestion`, `downloads.open_dispatcher`, `downloads.prevent_mixed_content`, `prompt-open-download`, `prompt-yank`, `prompt-item-focus`.
- **Content** (most are Chromium prefs or switches; per-site where qutebrowser allows it):
  - ✅ Tier 1 (2026-10-05): `content.javascript.enabled` (per site), `content.cookies.accept` (all, no-3rdparty, no-unknown-3rdparty, never) and `content.cookies.store`, and `content.headers.user_agent` (per site).
    - JavaScript and cookies are Chromium content settings. Per-site values are resolved just before each navigation (`on_before_browse`) and set for that origin, so glob patterns work despite CEF taking exact URLs.
    - The user agent is set per tab with the DevTools `Emulation.setUserAgentOverride`, which covers requests and `navigator.userAgent`.
    - A smoke step covers all three.
  - Other headers: `accept_language`, `do_not_track`, `referer`, `custom`.
  - Media and images: `content.images`, `content.autoplay`, `content.webgl`, `content.mute`.
  - Privacy and network: `content.proxy` (system, none, URL, PAC) and `proxy_dns_requests`, `content.webrtc_ip_handling_policy`, `content.canvas_reading`, `content.dns_prefetch`, `content.cache.size`.
  - JavaScript permissions: `content.javascript.clipboard`, `can_open_tabs_automatically`, `can_close_tabs`.
  - Notifications and logging: `content.notifications.presenter` and `show_origin`, `content.javascript.log` (into `:messages`).
  - Everything else: the PDF viewer (`content.pdfjs`), `content.prefers_reduced_motion`, `content.register_protocol_handler`, `content.unknown_url_scheme_policy`, `content.local_content_can_access_*`, `content.persistent_storage`, `content.mouse_lock`.
  - `content.user_stylesheets` is part of M20.
- **Input:**
  - ✅ Tier 1 (2026-10-05):
    - `bindings.key_mappings` (qutebrowser's defaults, applied in every mode before bindings).
    - `input.insert_mode.auto_load`: the renderer now reports `navigator.userActivation` with each focus change, so a field the page focuses by itself no longer takes insert mode unless this is on.
    - The mouse's back and forward buttons already work in Alloy windows (checked with X11 buttons 8 and 9). A setting to turn them off would need a page hook, so it isn't planned.
  - `input.partial_timeout`, `input.mouse.rocker_gestures`, `input.spatial_navigation`, `input.media_keys`, `input.match_counts`, `input.mode_override`.
- **Scrolling, search, zoom:**
  - ✅ (2026-10-05) `scrolling.smooth`, `search.wrap` and `search.wrap_messages`, and `zoom.levels`. Chromium always wraps searches, so `search.wrap = false` steps back when a result wraps.
  - Still to do: `scrolling.bar`.
  - Not possible: `zoom.text_only` (Chromium has no text-only zoom) and `zoom.mouse_divider` (Ctrl+wheel zoom is Chromium's own).
- **Sessions and window:**
  - ✅ Tier 1 (2026-10-05): `session.lazy_restore` (background tabs keep their URL and title and load when first shown) and `confirm_quit` (always, multiple-tabs, downloads, never; asked on `:quit` and when closing the last window), each with a smoke step.
  - `session.default_name`, `:save`, `window.hide_decoration`, `window.transparent`, `changelog_after_upgrade`.
- **Tools:**
  - ✅ Tier 1 (2026-10-05): `:screenshot [--force] file`, through the DevTools protocol (`send_dev_tools_message` with `Page.captureScreenshot` and a `DevToolsMessageObserver`), which needs no Views API or DevTools window. The format follows the file extension (png, jpeg, webp). A smoke step checks the PNG signature.
  - `:restart` (save the session, then re-exec), `:report`, `debug-keytester`, `debug-dump-page`, `debug-log-filter`, `debug-clear-ssl-errors`, `devtools-focus`, `selection-drop`.
  - Bookmarks and quickmarks: `bookmark-list`, `quickmark-save`, `bookmarks-reload`, `quickmarks-reload`.
  - Missing readline and caret commands: `rl-backward-word`, `rl-forward-word`, `rl-kill-word`, `rl-backward-kill-word`, `rl-yank`, and the `move-to-*-block` set.
- **Userscripts, editing and Greasemonkey** (already there since M9: `:spawn -u/-v/-o/-m/-d`, hint targets `spawn` and `userscript`, 15 of qutebrowser's 17 `QUTE_*` variables with `QUTE_FIFO`, `:open-editor`, and Greasemonkey `@match`/`@include`/`@exclude`/`@run-at`/`@noframes` with `GM_addStyle` and `GM_info`):
  - ✅ **Environment** (2026-10-05): `QUTE_CURRENT_URL` (the page's URL when `QUTE_URL` is a hinted link), `QUTE_SELECTED_HTML`, and `RIPTIDE_*` copies of every variable.
  - ✅ **qutebrowser's bundled userscripts** (tier 1, 2026-10-05):
    - Checked the variables and `QUTE_FIFO` commands of 18 of them against riptide, which turned up two fixes:
      - One-word arguments are now unquoted as in qutebrowser (`message-info 'text'`, `fake-key \a`, `fake-key " "`), which the password scripts rely on.
      - `jseval` takes `-q`, `-w <world>` and `-f`.
    - `format_json` was run unchanged and works.
    - The password scripts weren't run against a real store.
    - `ripbang` needs `config-dict-add` (still to do).
  - **Editor:**
    - ✅ Tier 1 (2026-10-05):
      - `edit-text` as qutebrowser's current name for `:open-editor`, which stays as an alias.
      - `edit-url` (with `--tab`, `--bg`, `--window`, `--private`, `--related`).
      - `cmd-edit` (edit the command line; `--run` runs it on save).
      - All three share one editor helper, and each has a smoke step.
    - `editor.encoding`, `editor.remove_file`.
  - **Greasemonkey:**
    - ✅ Tier 1 (2026-10-05): `@require`, fetched once into `<data>/greasemonkey-data/requires/` and keyed by the URL. Also `GM_setValue`, `getValue`, `deleteValue` and `listValues`, with promise versions under `GM.*`, stored per script in `<data>/greasemonkey-data/values/`.
      - The setter is a native function passed to the script as an argument, never through a global, so pages can't reach it.
      - Value updates carry the browser's script generation, so a reload's original `extra_info` doesn't bring back old values.
      - A smoke step covers both.
    - `GM_xmlhttpRequest` through the browser process (cross-origin, so only for scripts that `@grant` it, and only to `@connect` hosts), `GM_openInTab`, the promise-based `GM.*` API, and `unsafeWindow`.
  - **Lua:** `rt.spawn(argv, opts)` with a callback for the output, so `config.lua` can do what a userscript does without a separate file.
- ✅ **Config commands** (2026-10-05): `config-list-add` and `remove`, `config-dict-add [--replace]` and `remove`, `config-clear`, `config-diff` (a `riptide://config-diff/` page), `config-edit` (opens `config.lua` or `config.toml`, then reloads it), and `config-write-toml` (round-trip tested).
- **Not planned:** `qt.*` (Qt only), `backend`, and the Python-only commands (`debug-pyeval`, `debug-all-objects`, `config-write-py`).
- **Done when:** each item has a unit test where the logic is CEF-free and a smoke step where it's visible. The tiers set the order, and each group is its own commit.

### M20 — Theming

Today the bar colors are hardcoded CSS variables in each UI page; the Riptide palette landed on 2026-10-05.

- **One token set:** named tokens (bg, fg, accent, selected, insert, passthrough, private, warning, error, https, http, hint fg/bg/border, prompt, completion, downloads, keyhint) feed every UI page as CSS variables. They're sent over the UI channel (M13), so a change applies live without reloading anything.
- **Settings:**
  - `colors.*` uses qutebrowser's names where they map (`colors.statusbar.insert.bg`, `colors.tabs.selected.odd.bg`, `colors.hints.bg`, `colors.completion.item.selected.bg`, `colors.messages.error.bg`, `colors.webpage.bg`…) and overrides the theme's tokens one by one.
  - `fonts.*`: `fonts.default_family`, `fonts.default_size`, per-widget fonts, and the page fonts `fonts.web.family.*` and `fonts.web.size.*` (Chromium's `webkit.webprefs.fonts` prefs).
- **Built-in themes:**
  - `riptide` (dark, the default) and `riptide-light`, plus ports of gruvbox (dark and light), catppuccin (mocha and latte), nord, dracula, solarized (dark and light) and tokyo night.
  - `:theme <name>` with completion and a live preview while picking.
  - `ui.theme`, where `auto` follows the system's light or dark preference with a configurable pair.
- **User themes:** `themes/<name>.toml` in the config directory (tokens plus optional overrides), or `rt.theme{…}` in Lua.
- **Importers:** base16 scheme YAML, and simple qutebrowser theme files (only `c.colors.… = "…"` lines are read; no Python is run).
- **Custom CSS:**
  - `ui.css` in the config directory is added to every UI page, for anything tokens can't do (tab shape, padding, fonts).
  - `content.user_stylesheets` applies files to web pages, and can be set per site with patterns.
  - Both reload when the file changes.
- **Pages:**
  - `colors.webpage.bg` sets the background shown while a page loads, so dark themes don't flash white.
  - The dark-mode setting becomes live and per site; today it's startup-only.
- **Size, shape and position.** Today each bar is a separate CEF view whose height is a Rust constant (`TABBAR_HEIGHT`, `STATUSBAR_HEIGHT`). The overlay that holds completion and prompts is sized as rows × a fixed row height. CSS alone can't resize any of them, so:
  - **Pages report their size:** each UI page measures its content (fonts, padding, rows) and reports its height or width over the UI channel, and Rust lays the views out to match. Fonts, padding and `ui.css` then change real sizes, with no clipping.
  - **Placement:**
    - The tab bar goes top, bottom, left or right (`tabs.position`), with `tabs.width` for vertical tabs; the status bar goes top or bottom (`statusbar.position`); each can be hidden (`tabs.show`, `statusbar.show`).
    - The overlay is either `docked` (full width above the status bar, as now and in qutebrowser) or `floating` (a centered box like a command palette). `ui.overlay.position`, `ui.overlay.width` and `completion.height` (rows or a percent) control it.
  - **Shape:** padding, radius, borders and shadows come from tokens (`tabs.padding`, `tabs.indicator.width`, `statusbar.padding`, `hints.radius`, `hints.border`, `prompt.radius`) and from `ui.css` for anything beyond them. Tab-bar choices like tab width, min/max, title alignment and elision are settings (M19).
  - **Hints:** labels are drawn in the page inside a shadow DOM. They take the theme tokens, `fonts.hints`, `hints.uppercase` and `hints.radius`, plus a `hints.css` file in the config directory for full control. The shadow DOM keeps the page's CSS out and our CSS in.
  - **Window:** `window.hide_decoration` and `window.transparent` (M19).
- **Dialogs and prompts.** JavaScript `alert`/`confirm`/`prompt`, leave-page warnings, HTTP logins, permission requests, certificate warnings and download prompts all use the prompt area in the overlay today (M7).
  - **Style:** `colors.prompts.*` (fg, bg, border, selected), `fonts.prompts`, `prompt.radius`, and `ui.css`. Each kind of prompt gets a CSS class (`.prompt.alert`, `.prompt.auth`, `.prompt.permission`, `.prompt.certificate`, `.prompt.download`), so a theme can, for example, make certificate warnings red and loud.
  - **Placement:** `prompt.position`, either `docked` (above the status bar, the default) or `center` (a modal box over the page, dimming it). `prompt.width` sets the width.
  - **Content:** the asking site's origin is always shown and can't be styled away, because it's how users spot a spoofed dialog. A theme can reorder or restyle the key hints (`y: yes…`) but not hide the origin.
  - **Mouse:** optional buttons for each answer (`prompt.buttons`), off by default, for mouse users and touch screens.
- **Tests:**
  - Unit tests that every built-in theme defines every token with valid colors and passes a WCAG AA contrast check for its text pairs.
  - A smoke step switches themes and reads a CSS variable from the status bar.
  - Smoke steps for layout: a bigger font grows the tab bar's view to fit, `tabs.position left` puts the tab bar beside the page, and `prompt.position center` centers a JavaScript `confirm()`. Screenshots are compared against the expected layout boxes, not pixels.

### M21 — Interactive settings

- **`riptide://settings` (`:settings`):** every setting grouped by section, with a search box (`/` focuses it). Each one shows its type, default, current value, where the value came from (default, `config.toml`, `config.lua`, `autoconfig.toml` or `:set`), and any per-site overrides.
- **Editors by kind:** a toggle for true/false, a select for enums, a number field with its bounds, text, lists (add, remove, reorder), maps (key/value rows), and colors with a swatch and picker (shared with M20).
- **Saving:**
  - A change applies at once and is saved to `autoconfig.toml` through the UI-only channel.
  - The Rust side validates it with the same parser as `:set`, and only riptide:// UI pages may send these messages.
  - A setting that a config file also sets is marked "your config file sets this and wins at startup".
  - Each setting has a reset button (`:config-unset`) and per-site override rows (pattern plus value).
- **Keyboard first:** hints, `j`/`k`, `Return` to edit, `Escape` to leave a field.
- **Keys tab:** bindings per mode with search. Rebind by pressing the new keys, unbind, and see conflicts with prefixes before they happen.
- **Sites tab:** saved permission answers (camera, microphone, location, notifications, screen sharing) and certificate exceptions, each with revoke, and clearing cookies and site data per site.
- **Command line:** `:set` completes values (enum choices, true/false, the current value prefilled), and `:set name?` shows the setting's help.
- **Tests:** unit tests for the settings data model and for validating messages; a smoke step toggles a setting on the page and checks it's applied and saved.

### M22 — Conference calls and screen sharing

**Measured on 2026-10-05** (CEF 154, Alloy style, Xvfb, `http://127.0.0.1` test page):

| Request | Result |
|---|---|
| `getDisplayMedia({video, audio})` | Our prompt asks "capture your desktop audio and capture your screen". Yes shares the **whole screen** (track label `Screen`, 1280×800) plus system audio. There's no picker and `displaySurface` is undefined. |
| `getUserMedia` with `chromeMediaSource: 'desktop'`, `chromeMediaSourceId: 'window:<xid>:0'` | Whole screen; Alloy ignores the requested window. |
| …`'screen:0:0'` | `NotReadableError: Could not start video source` |
| …`chromeMediaSource: 'tab'` | `AbortError: Error starting tab capture` |
| Chrome-style tab `BrowserView` in our Alloy window | Refused: "Cannot add Chrome style BrowserView to Alloy style Window" |
| Chrome-style window with all views Chrome-style (naive switch) | Exits at startup with no error in either log. Not investigated. |

So with Alloy, sharing one tab or one window isn't possible from the page side. Options, in order:

1. **Chrome style for everything** (spike first, about a day). Chrome style brings Chrome's own picker (tab, window, screen), tab capture with "share this tab instead", tab audio, and the "sharing" bar.
   - This reopens decision #2. Check what breaks: key handling (`on_pre_key_event`), popups as tabs, the UI pages, DevTools (simpler: it's Chrome style already), and Chrome UI we'd have to hide.
   - Also find why the naive switch exits.
2. **The desktop portal on Wayland.** Chromium's PipeWire capturer hands screen requests to xdg-desktop-portal, whose dialog picks a window or screen (not a tab). Check whether Alloy goes through it. On X11 there is no portal. Wayland can't be tested here yet (no compositor).
3. **A small CEF patch** that lets `on_request_media_access_permission` return a chosen `DesktopMediaID`, plus a picker of our own. It means building CEF ourselves, so only if 1 fails, and it's worth proposing upstream.

Whatever path wins:

- **Picker UX:**
  - A keyboard picker in the prompt area: tabs (title and favicon), windows, screens, thumbnails if available, and a "share audio" toggle.
  - A sharing marker in the status bar and on the tab, `:share-stop`, and switching the shared source mid-call.
  - Shared tabs stay highlighted in the tab bar.
- **Calls keep running in the background:** check that a tab with live capture or WebRTC isn't throttled when hidden (timers, rendering), and exempt it if it is.
- **Devices:**
  - `:media-devices` picks the default camera, microphone and speaker, saved per site.
  - The permission prompt names the device.
  - Check echo cancellation and noise suppression with PipeWire and PulseAudio.
- **Performance:** VA-API video decode and encode (this machine logs `vaInitialize failed` from mixed Nix and system Mesa libraries; check on a stock distro), and GPU use for background blur (WebGL; Xvfb blocklists it, real GPUs don't).
- **While browsing other tabs:**
  - Picture-in-picture (`:pip`, plus the Document Picture-in-Picture API if CEF supports it).
  - A mute toggle that reaches the call tab from any tab, using the site's own shortcut sent through `fake-key` to that tab.
  - Desktop notifications through `content.notifications.presenter` (libnotify).
- **Indicators:** camera, microphone and screen in use, per tab, from our permission grants and the tracks' lifetimes, since Chrome's capture indicator isn't in Alloy.
- **Tests:**
  - A smoke step with a local WebRTC loopback page and fake devices (`--use-fake-device-for-media-stream`; test builds only).
  - Once a picker exists, a check of `displaySurface` for each choice.
  - A manual matrix before calling it done: Google Meet, Zoom (web), Microsoft Teams, Jitsi, Slack huddles, Discord and Whereby, each tested for camera, microphone, screen, window and tab share, and with the call in a background tab. Check each site's browser detection with our user agent.

### M23 — Crash recovery and crash reports

**Today:** every `auto_save.interval` ms the open tabs are saved to the `_autosave` session. That file is deleted once `run_message_loop` returns, so if it's there at startup, the last run crashed. The tabs are then restored, or, if URLs were given on the command line, the user is pointed to `:session-load _autosave`. A tab saves only its URL, its title and whether it's pinned.

- **Recovering open tabs:**
  - ~~**Signals count as clean exits.**~~ Checked 2026-10-05 with the e2e test `tabs_come_back_after_sigterm`. After SIGTERM, riptide exits with status 0 but keeps `_autosave.toml`, and the next start restores the tabs. SIGKILL works the same way (`tabs_come_back_after_a_crash`). SIGINT and logout aren't tested yet.
  - **The "Restored the tabs open before the crash" message is hidden** almost at once by the "Content blocking has no filter lists yet" notice, so it's easy to miss that recovery happened.
  - **The recovery copy gets overwritten:** after a crash, starting with URLs on the command line leaves the old tabs in `_autosave`, but the next autosave tick replaces them. At startup, rename the crashed autosave to a timestamped session (e.g. `_crashed-2026-10-05T10-34`), keep the last few, and list them in `:session-load` completion.
  - **Crash loops:** if the restored tabs crash the browser again shortly after startup, don't restore them automatically the next time. Instead, show the crashed tabs on a `riptide://recover/` page where the user picks which ones to reopen, the way Firefox's "Restore Session" page works.
  - **More state per tab:** save each tab's back/forward history and scroll position, so restoring doesn't drop where you were.
  - **Crashed tabs:** there's no `on_render_process_terminated` handler, so a crashed or killed renderer leaves a dead tab. Show an error page in the tab with "reload" (`r`) and log the reason.
- **Crash reports** (complements `:report` in M19):
  - **Capture:**
    - A Rust panic hook writes the message, backtrace and version (M18's `--version` string) to `<data>/crashes/<timestamp>.txt`.
    - For native crashes in CEF, enable Crashpad to write minidumps locally with uploads off, and record the dump's path.
  - **Offer after the crash:** at the next startup, the status bar says "riptide crashed last time: :crash-report". That command opens `riptide://crash/`, which shows the report and lets the user edit it, then send it one of two ways:
    - **Email:** a `mailto:` link with the subject and body filled in. The address comes from a `crash_report.email` setting; with no address, the email button is hidden.
    - **GitHub issue:** `https://github.com/joshzcold/riptide/issues/new?title=…&body=…`, filled in. Truncate the log so the URL stays under GitHub's ~8 KB limit, and tell the user to attach the full log or minidump by hand.
  - **Privacy:** nothing is ever sent automatically. Reports leave out tab URLs and titles by default, and include them only if a checkbox is ticked. Log lines are shown before sending, because they can contain URLs.
  - **Tests:**
    - A test-build-only `:debug-crash` command (`panic`, `abort`, `renderer`) triggers each kind of crash.
    - A smoke step checks that the next start offers the report and restores the tabs.

### M24 — Documentation website ✅ mostly done 2026-10-05

Result:
- **Book:** `docs/book/` (mdBook 0.5.4 through `scripts/mdbook.sh`, pinned and checksum-verified). It has a user guide (10 pages, including moving from qutebrowser), configuration, reference and developer guide parts, mostly moved out of the README. The README is now a 53-line landing page.
- **Generated reference:** `rt_config::reference` writes `reference/commands.md` and `reference/bindings.md` from `rt_core::help::build` with the default config, and a unit test checks they're current (`UPDATE_LUA_TYPES=1 cargo test -p rt-config` regenerates them with the Lua types). The settings, Lua API, changelog and example-config pages include `docs/settings.md`, `docs/lua/rt.meta.lua`, `CHANGELOG.md` and `docs/config.example.*` directly.
- **Tasks:** `./task docs` builds and `./task docs-serve` previews.
- **Published** at <https://joshzcold.github.io/riptide/> once the repository went public (Pages source: GitHub Actions).
- **CI:**
  - A `docs` job in `check.yml` builds the book and checks internal links and anchors with lychee (offline).
  - `docs.yml` deploys to GitHub Pages on pushes to `main` and checks external links weekly.
- **Agents:** `.claude/skills/docs-user` and `docs-dev` hold the checklists, `docs/book/src/dev/docs.md` holds the rules, and `AGENTS.md` points to all three.
- **Lessons:**
  - mdBook's smart punctuation turns `--force` into an en dash, so it's off.
  - `<text>` placeholders in command descriptions are escaped, or they render as HTML tags.
  - lychee can't resolve the 404 page's root-relative links offline, so that page is excluded.

Left:
- **Version on the site:** the plan called for showing the version the site was built from; it doesn't yet.
- **Links from `:help`:** `riptide://help` doesn't link to the guide pages yet.

Original plan:

**Today:**
- The user documentation is the 386-line README (building, configuration, key bindings, testing, releases).
- Reference material is generated from the live registries: `docs/settings.md`, `docs/lua/rt.meta.lua` and the in-browser `riptide://help`.
- `docs/PLAN.md` is the only developer document.
- There's no website.

- **Framework: [mdBook](https://rust-lang.github.io/mdBook/)**, which the Rust Book, Cargo and the rustc dev guide all use.
  - Plain Markdown and a single static binary, with no Node toolchain.
  - Search, light and dark themes, and an "edit this page" link are built in.
  - Zola is the alternative if we later want a separate marketing-style landing page; it isn't needed for docs.
  - Pin and checksum the mdBook binary in `scripts/mdbook.sh`, as `scripts/git-cliff.sh` does, so `./task docs` needs nothing installed.
- **Layout** (`docs/book/`, one book in two parts):
  - **User guide:**
    - Installing (tarball, AppImage, sandbox setup), first start, and moving over from qutebrowser (quickmarks, bookmarks, history import, translating `config.py` to `config.lua`)
    - Modes, hints, tabs, sessions and crash recovery, downloads and permissions, userscripts and Greasemonkey, spell checking, Widevine, privacy and network traffic
    - **Configuration:** the config files and their paths, then TOML, then Lua
  - **Reference** (generated, never hand-edited):
    - Every command, setting and default binding, and the Lua API
    - All built from the same data as `riptide://help` (`rt_core::help::build`) and checked for staleness the way `docs/settings.md` is today
    - The changelog is included from `CHANGELOG.md`.
  - **Developer guide:**
    - Building, crate layout and architecture, the CEF threading rules, and the UI channel
    - Testing, including the local-testing rules
    - Commit conventions, releasing, and updating CEF
  - **The README shrinks** to a summary, a quick start and links to the site. PLAN.md stays in the repo and isn't published.
- **GitHub Pages:**
  - A `docs.yml` workflow builds the book on pushes to `main` and deploys it with `actions/upload-pages-artifact` and `actions/deploy-pages`, to `https://joshzcold.github.io/riptide/`.
  - `check.yml` builds the book on every PR, so broken docs fail CI. It also checks links with [lychee](https://github.com/lycheeverse/lychee), external links weekly rather than per PR.
  - The release workflow is unchanged; the site tracks `main` and shows the version it was built from. Versioned docs can wait until there's a 1.0.
- **`:help` and the site share text:** command and setting descriptions stay in the registries (one source). `riptide://help` links to the matching website page for longer guides.
- **Agent skills** (`.claude/skills/`, same format as `local-testing`), plus an `AGENTS.md` pointing to them for agents that don't read Claude skills:
  - **`docs-user`:** when a change is visible to users (a new command, setting, binding, mode or behaviour):
    - Update the matching user-guide page.
    - Regenerate the reference rather than editing it.
    - Add a qutebrowser note when behaviour differs.
    - Style: second person, task-first, one example per feature, plain language.
  - **`docs-dev`:** when a change affects how riptide is built, structured or tested (a new crate, a threading rule, a CEF pitfall, a test step):
    - Update the developer guide.
    - Record lessons learned there instead of only in PLAN.md.
    - Style: explain why, with file paths and links to code.
  - Both skills say how to build and preview (`./task docs`, `./task docs-serve`), that generated pages are never hand-edited, and that `./task check` must pass.
  - Both list what doesn't need docs: refactors, internal-only fixes and test-only changes.
- **Tests:**
  - `./task check` builds the book and fails on stale generated pages or broken internal links.
  - Every command and setting has a reference entry; this is the existing unit test, extended.

### M25 — Permission requests as a floating card

**Today:** a site's request for the camera, microphone, location, notifications or screen capture, and certificate warnings, take the same docked one-line prompt above the status bar as `confirm()` and download paths (M7). It's easy to miss, especially when the status bar is hidden (`statusbar.show`), it doesn't say clearly what is being asked for, and it can only be answered from the keyboard.

- **A floating card** over the top of the page, below the tab bar, the way Chrome and Firefox place theirs:
  - The site's origin, in large text, with its favicon; it can't be hidden or restyled away (see M20).
  - What it wants, as an icon and a word for each: camera, microphone, location, notifications, screen.
  - The answers as keys and buttons: `y` Allow once, `A` Always allow, `n` Not now, `N` Always block. Buttons work with the mouse, unlike the docked prompt.
  - A short line about what "always" means, e.g. "saved for meet.example.com in autoconfig.toml".
- **Behaviour:**
  - The card belongs to its tab. Switching tabs hides it, and coming back shows it again. A tab with a waiting request gets a badge in the tab bar, so it can't be forgotten.
  - Keys go to the card while it's shown in the current tab (the existing `yesno` mode), and `Escape` means "not now", as today.
  - Several requests from one page are combined into one card ("camera and microphone"), as they are now.
  - Other prompts (JavaScript dialogs, logins, download paths) stay docked, or follow M20's `prompt.position`.
- **Building it:**
  - The card is a `riptide://ui/permission.html` page in its own overlay view, placed by `window::arrange_bars`. The view grows to fit its content, as M20 plans for every UI page.
  - It talks to Rust over the UI channel (M13): only `answer(id, choice)` is allowed, and the browser checks the id against the waiting prompt.
  - `rt_core::prompt` gains a `Permission { origin, features }` kind, so the UI can draw icons instead of parsing a message.
- **Afterwards:**
  - A `riptide://permissions` page lists the saved answers per site and can revoke them. This also closes M7's "no command to reset per-site permissions" gap.
  - Pairs with M22's indicators for camera, microphone and screen in use.
- **Tests:**
  - Unit tests for the new prompt kind and for combining features.
  - A smoke step: a `getUserMedia` request shows the card; `y` grants it, and a page script sees the stream start; switching tabs and back keeps the card; `N` saves a block in `autoconfig.toml`.

### M26 — Testing and linting

**Today (2026-10-05):**
- **Unit tests:** about 200 run with `./task test`: 141 in `rt-core`, 46 in `rt-config`, 13 in `rt-storage` and 6 in `rt-adblock`. They cover keys, modes, commands, settings, config files, paths, storage and filter lists. `rt-cef` is the largest crate (about 16,500 lines) and has 1 test. `riptide` has none.
- **End to end:** `scripts/smoke-test.sh` is one 875-line bash script of 59 steps. It drives the real browser on Xvfb with xdotool and reads state back through the window title (`{mode}::{page title}`). It runs on every push and, since the release workflow, against the packaged tarball and AppImage. It covers a lot, but it runs in order, so one failure hides the rest. Its waits are tuned by hand (`SLOW`), it's hard to run one step on its own, and assertions are limited to what fits in a title.
- **Linting:** `cargo fmt --check` and `clippy --all-targets -D warnings` run in CI, and the commit-message check. Nothing checks the JavaScript (`crates/rt-cef/js/`, 6 files), the UI pages (`crates/rt-cef/ui/`, 7 files), the shell scripts or the workflows. Dependency licenses and security advisories aren't checked either.

#### End-to-end options

| Option | How it drives the browser | Strengths | Weaknesses |
|---|---|---|---|
| **A. Keep growing the bash smoke test** | xdotool keys on Xvfb, state from the window title | Real X11 input through the whole stack. Already works and runs on packages. | Serial, timing-tuned and flaky by nature. Hard to assert anything rich or to run one test. Bash doesn't scale. |
| **B. Chrome DevTools Protocol** (`--remote-debugging-port`, from Rust with `chromiumoxide` or Python with Playwright's `connect_over_cdp`) | CDP input events and DOM queries | Rich page assertions, screenshots, network interception, an ecosystem we don't have to build. | CDP input goes to the page and probably skips CEF's `OnPreKeyEvent`, so it doesn't test our key handling (needs a spike). It can't see our UI views as one browser. The debugging port has to stay off in normal builds. |
| **C. Our own test channel** (recommended backbone) | The M15 command socket, extended with test-only requests: send keys into the same path as `OnPreKeyEvent`, run commands, and query state as JSON (mode, tabs, URLs, titles, status bar text, completion, prompts, messages) | Deterministic: no xdotool timing, and waits poll real state. Tests are Rust (`cargo test`), each in its own browser and display, so they run in parallel and one at a time. Assertions are as rich as the state we expose. | Keys enter just above X11, so the real input layer needs a few xdotool tests of its own. The test-only requests must not exist in release builds. |
| **D. UI pages and page scripts outside CEF** | Playwright or headless Chrome on `ui/*.html` and `js/*.js` with fixture pages and a fake `rt.send` | Fast, precise tests for hint collection, caret movement and tab bar rendering. | Another toolchain (Node or Python plus a browser download). Doesn't test the CEF integration. |
| **E. Screenshots** | `Page.captureScreenshot` (the planned `:screenshot`) compared against expected layout boxes | Catches layout breakage (M20 theming, vertical tabs, prompt placement). | Pixel comparisons are brittle; only layout boxes are stable enough to assert. |

**Recommendation:** C as the backbone, with A cut down to a thin "real input" suite, and E added for layout once M20 lands. Page scripts are tested through C with fixture pages, which avoids D's extra toolchain; revisit D if JS logic grows. B is worth a one-day spike in case CDP input does reach `OnPreKeyEvent`, because it would then give DOM assertions for free.

#### Plan

- **Test channel (C):**
  - Behind a `test-control` cargo feature, which release builds (`build-release.yml`) don't enable. A unit test checks the release build refuses the requests.
  - Socket requests: `keys` (a key string such as `5j` or `<Ctrl-w>`, fed to the engine as `OnPreKeyEvent` would), `run` (a command line), `state` (JSON for mode, windows, tabs, URLs, titles, status bar, completion, prompts and messages), `eval` (JavaScript in a tab, through the existing eval channel), and `wait` (until a condition on `state` holds, with a timeout).
- **Harness:** a new `crates/rt-e2e` test crate.
  - A `Browser` fixture that starts `riptide --basedir <tempdir>` on its own Xvfb display (`-displayfd`, as the smoke test does). It serves fixture pages from a local HTTP server, connects to the socket, and kills only its own processes on drop (see the local-testing skill).
  - Helpers that poll: `b.keys("f")`, `b.wait_mode("hint")`, `b.state().tabs`.
  - Fixture pages live in `crates/rt-e2e/pages/`.
  - `./task e2e` runs the suite, and CI runs it on Linux in a new job. On failure, the browser log and a screenshot are uploaded as an artifact.
- **Migration:** move smoke steps into `rt-e2e` area by area: modes and keys, tabs, hints, command line and completion, prompts and permissions, downloads, sessions and crash recovery, config and Lua, private windows, adblock. Each moved area is deleted from the bash script.
- **What stays in the smoke test:** about 10 steps of real input (typing into a field, `Escape`, a mouse click, the tab bar's mouse handling). It's still the test that `build-release.yml` runs against the packages.
- **New coverage the channel makes possible:** M23's crash recovery (kill -9 the browser and check the tabs come back), the pinned-tab prompt, M25's permission card, per-tab modes, and multiple windows.
- **Later:** run the same suite on macOS and Windows once M10 makes them run.

- ✅ **Started (2026-10-05):**
  - **Test channel:** `TestRequest` (`keys`, `run`, `state`, `eval`) in `rt_config::remote`, answered by `rt-cef/src/test_control.rs` in debug builds or with `--features test-control`.
    - Keys go through `client::press`, which feeds `handle_key_event` exactly as `OnPreKeyEvent` does, then sends the key to the page if the engine doesn't use it.
    - The socket thread waits on a channel for the UI thread's answer.
    - There's no server-side `wait` request: the harness polls `state` instead.
  - **Harness:** `crates/rt-e2e` gives each test its own Xvfb (`-displayfd`), scratch `--basedir`, short `XDG_RUNTIME_DIR` (Unix socket paths must fit in ~108 bytes) and fixture HTTP server. It stops its process group and then the CEF helpers that carry its `--user-data-dir`, because the zygote leaves the group.
  - **Tests:** 51 so far.
    - Areas: keys, modes, marks, macros, search, `]]`/`[[`, zoom, tabs, hints (including iframes and number hints), the command line and completion, prompts, `config.toml`/`config.lua`/`autoconfig.toml`, windows, private windows, sessions, and recovery after SIGKILL and SIGTERM.
    - Since 2026-10-05, also: history completion and `:history-import`, the internal pages and their isolation from web pages, ad blocking (network and element hiding), permission prompts saved per site (allow and block), userscripts and their FIFO, hints running userscripts and programs, `:spawn -o`, the editor, Greasemonkey (`@run-at`, `GM_addStyle`, `GM_setValue`, `@require`), downloads (directory, path completion in the prompt, `:download-delete`), caret mode, and a second `riptide` invocation handing over its arguments.
    - These were written in a separate git worktree (`.claude/worktrees/`), so test runs never build or start the main checkout's work in progress.
    - They're `#[ignore]`d so a workspace-wide `cargo test` never starts browsers. `./task e2e` and CI run them; each test binary takes about a second.
    - `state` lists hint labels with each element's text and URL, so `follow_hint` can click anything.
    - The harness can restart a profile, crash it (SIGKILL) or terminate it (SIGTERM).
  - **Found while writing them:**
    - ✅ A tab closed before its first page committed had an empty `url`, so `u` couldn't reopen it. `tabs::open` now records the URL it was asked for. Test: `u_reopens_a_tab_closed_before_its_page_loaded`.
    - Under load, keys sent to a page that has loaded but not painted are dropped, even though it reports focus and visibility. This is probably the "lost keys" gap in M14. The harness waits for two animation frames after each load.
  - **Next:**
    - Still only in the smoke test: the certificate prompt (needs an HTTPS fixture server), per-site user agents and JavaScript, `:screenshot`, upload fields with `fileselect.handler`, Alt-e in the download prompt, lazy session restore, `tabs.position`/`statusbar.show` layout, `confirm_quit`, and the editor round trips for `:edit-url` and `:cmd-edit`.
    - Delete the ported steps from `scripts/smoke-test.sh` once nobody else is editing it.

#### Unit tests for the CEF layer

`rt-cef` mixes logic that has nothing to do with CEF into its handlers. Move that logic into pure functions, in `rt-core` where it isn't CEF-specific, and test it:
- Bar layout (`window::arrange_bars`: positions for every `tabs.position` and `statusbar.position`).
- UI channel validation: which page may send which message.
- Permission decisions and the remembered answers.
- Session conversion (tabs to `Session` and back, pinned state).
- Download naming and the prompt flow.
- Favicon caching rules.
- The renderer's message parsing.

Each move adds tests, and the rule from `rt-core` applies: anything that can be decided without CEF is tested without CEF. Coverage is measured with `cargo llvm-cov`, reported in CI as a summary but not used as a gate.

#### Linting

| Tool | Checks | Notes |
|---|---|---|
| `cargo fmt`, `clippy -D warnings` | Rust | Already in CI. Add `[workspace.lints]` in `Cargo.toml` so every crate shares one set, e.g. `unsafe_op_in_unsafe_fn`, `clippy::dbg_macro`, `clippy::todo`, `clippy::unwrap_used` in non-test code of `rt-core`. |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | Dependency licenses (all must be GPL-3.0-compatible), RustSec advisories, banned and duplicate crates, sources | Most valuable addition: riptide is GPL-3.0 and ships its dependencies. |
| [Biome](https://biomejs.dev) | Lints and formats `js/*.js` and the scripts in `ui/*.html` | A single pinned binary with no Node project, the same pattern as mdBook and git-cliff. |
| [ShellCheck](https://www.shellcheck.net) | `scripts/*.sh`, `task` | The smoke test and the packaging scripts are bash. |
| [actionlint](https://github.com/rhysd/actionlint) | `.github/workflows/*.yml`, including ShellCheck on `run:` blocks | It already found one problem in the release workflow while that was being written. |
| [typos](https://github.com/crate-ci/typos) | Spelling in code, docs and commit messages | Cheap, and has few false positives. |
| lychee | Links in the docs | Already in CI (M24). |

- Every tool is pinned and checksum-verified in `scripts/` (or `cargo install --locked` with a cached binary). `./task lint` runs all of them, and so does CI.
- `./task hooks` gains an optional pre-push hook that runs `./task lint`.
- Order: actionlint, ShellCheck and cargo-deny first, since they're quick wins with real findings. Then `[workspace.lints]`, then Biome, fixing what each one finds in the same change.
- ✅ **Quick wins done (2026-10-05):** `scripts/tool.sh` pins actionlint 1.7.12, ShellCheck 0.11.0 and cargo-deny 0.20.2 with checksums, and `./task lint` (so CI too) runs all three.
  - **ShellCheck** found a `(( … $1 ))` test in `expect_scroll` that it couldn't parse (now `test "$value" -gt 3000`) and an `export` that hid a failed `cat`. Its 20 notes about `check && pass || fail` are disabled for the smoke test, with a reason: `pass` only echoes.
  - **actionlint** is clean. It had already caught a glob used as a command while the release workflow was written.
  - **cargo-deny:** `deny.toml` allows only GPL-3.0-compatible licenses, and all 239 crates in `Cargo.lock` pass. RustSec advisories and sources are clean. Duplicate versions only warn. The crates are now `publish = false`, which lets the path dependencies between them pass the wildcard check.

#### Order of work
1. Linting quick wins (above).
2. The test channel and `rt-e2e` with a handful of ported steps, to prove the design.
3. Move `rt-cef` logic into tested functions, starting with layout and the UI channel, alongside feature work.
4. Port the rest of the smoke test area by area, then cut it down to the real-input suite.
5. Screenshots for layout (with M20), and the CDP spike.

### Deferred — proprietary codecs (H.264 / AAC)
Not scheduled. H.264/AAC require building CEF/Chromium from source with `proprietary_codecs=true` and `ffmpeg_branding="Chrome"`. That means hours and a lot of disk space per release, and it works against goal 1 (tracking Chromium quickly). Distributing such builds also raises patent-licensing questions. Revisit only if VP9/AV1 Widevine proves insufficient; if so, prefer a documented "build your own CEF" path over shipping these binaries.

---

## Chromium update cadence

- Pin `cef` crate to an exact version (e.g. `=154.3.0`).
- CI job checks for new CEF releases weekly; upgrades go through a branch with smoke tests.
- Keep CEF-specific code isolated in `rt-cef` so binding churn doesn't leak into core logic.

## Testing strategy

- **Unit tests** in `rt-core`: key parsing, mode transitions, command parsing, config merging (the bulk of logic).
- **JS tests** for the hint / insert-detection scripts (headless).
- **Integration tests**: launch the browser against a local test server and drive it through a debug control channel (e.g. `--remote-debugging-port` + CDP) to assert behavior.
- **Status and plan (2026-10-05):** see M26. Unit tests cover `rt-core`, `rt-config`, `rt-storage` and `rt-adblock`; the end-to-end suite is `scripts/smoke-test.sh` (59 steps); M26 plans a Rust e2e harness on a test-only control channel, unit tests for the CEF layer, and more linters.

## Risks

| Risk | Mitigation |
|---|---|
| `cef` crate API churn / gaps | Pin versions; drop to `cef-dll-sys` for missing pieces; contribute upstream |
| Views overlay limitations (transparency, z-order) | Validated in M0; OSR is the fallback |
| Wayland support in CEF | Start on X11 (current session); track CEF's Ozone/Wayland status |
| Large binary distribution (~200MB+) | Expected for any CEF app; document clearly |
| DRM: stock CEF has Widevine (via component updater) but no H.264/AAC | Opt-in Widevine in M11 for VP9/AV1 content; H.264/AAC deferred (needs a source build) |
| Silent background traffic to Google services | Review and per-component decisions in M8 |

## Open questions

1. ~~**Config language**~~: decided in M5. Both are supported: TOML for declarative config, Lua 5.4 for programmable config (M12 extends it to scripting).
2. **Platform priority:** Linux-only until M10, or keep macOS/Windows building in CI from the start?
3. **UI overlay tech:** plain HTML/CSS/vanilla TS, or a small framework?
4. **qutebrowser compatibility depth:** import qutebrowser `config.py` bindings, quickmarks, and bookmarks?
5. **Alloy or Chrome style** (reopened by M22): Alloy can only share the whole screen. Chrome style would bring Chrome's tab/window/screen picker, at the cost of revisiting decision #2. The M22 spike decides.

> Note: the project is GPL-3.0 (per `LICENSE`). Dependency licenses must stay GPL-compatible (CEF is BSD, adblock-rust is MPL-2.0 — both fine).
