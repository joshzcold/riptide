# hackers-browser — Project Plan

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
- A **UI overlay `BrowserView`** (privileged, internal `hb://ui` page) that renders the status bar, command line, completion menu, tab bar, prompts, and messages. Rust drives it via process messages; it never loads remote content.

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
hackers-browser/
├── Cargo.toml                 # workspace
├── crates/
│   ├── hb/                    # binary: main(), CEF init, subprocess dispatch
│   ├── hb-core/               # CEF-free logic: modes, keymap, commands, config (unit-testable)
│   ├── hb-cef/                # CEF integration: App, Client, handlers, Views window, tabs
│   ├── hb-renderer/           # renderer-process handler + JS injection
│   ├── hb-storage/            # history, bookmarks, quickmarks, sessions (SQLite)
│   └── hb-ui/                 # internal UI page (HTML/CSS/TS) embedded via include_dir
├── js/                        # page scripts: hints, scroll, caret, insert detection
└── docs/
```

> **Current state (after M0):** only `hb`, `hb-core` and `hb-cef` exist. Renderer code, UI pages (`hb-cef/ui/`) and page scripts (`hb-cef/js/`) live inside `hb-cef` until they grow enough to split out. `hb-storage` arrives with M6.

**Rule:** `hb-core` has no CEF dependency. Modes, key parsing, command dispatch, and config are tested without a browser. CEF is an adapter that turns events into `hb-core` inputs and executes `hb-core` actions.

### Core loop

```
Key event (OnPreKeyEvent)
  → ModeManager (normal / insert / hint / command / caret / passthrough / prompt)
  → KeyParser (counts, multi-key sequences like `gg`, `;y`)
  → Command (`:scroll down`, `:open -t …`)
  → CommandRegistry dispatch
  → Action on Tab / Window / UI / Storage
```

Today `hb-core::Engine` implements this loop: it takes a `Key` and returns `KeyOutcome { consumed, effects }`. Mode and command-line commands are handled inside the engine. Everything else comes back as `Effect::Run(Command)` for `hb-cef` to carry out. The command table is a hand-written `match` for now.

Commands are registered with a derive macro so each one declares its name, args, flags, count support, and the modes it applies in. This gives `:help`, completion, and argument validation from one source of truth.

---

## Feature parity checklist (qutebrowser → hackers-browser)

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
- [x] `hackers-browser ':cmd' url` talks to the running instance (M15; Unix)
- [x] Spell checking with keyboard-driven suggestions (M17)
- [x] Versioned releases, `CHANGELOG.md`, CI on Linux/macOS/Windows (M18; Linux release artifacts only)

### Extensibility
- [x] Userscripts (spawned processes with `QUTE_URL`, `QUTE_FIFO`, etc.; keep env-var names for compatibility) (M9)
- [x] Greasemonkey-style injected JS (M9)
- [x] `:spawn` external commands (M9)
- [x] `:open-editor` (edit text field in `$EDITOR`) (M9)

### Session / state
- [x] Sessions (save / load / `auto_save.session`, `:wq`)
- [x] History with completion (`:open` + `Tab`)
- [x] Quickmarks and bookmarks in qutebrowser's file formats
- [x] Crash-recovery autosave (`auto_save.interval`, `_autosave` removed on a clean exit) and a history page (`:history`, `hb://history/`) (2026-10-02)
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
| Renderer → browser process message | ✅ `on_focused_node_changed` → `hb.focus` message → auto insert mode on clicking a text field |
| Clean shutdown via `:quit` | ✅ exit code 0, no leftover subprocesses |

Lessons learned:
- A view's preferred size with width or height `0` is treated as unset. The browser view then uses its large default and squeezes the page out of the layout. Always return a non-empty size.
- Dropping the last reference to a `BrowserView` closes its browser **synchronously**, which re-enters our handlers. Shared state is in a UI-thread `RefCell`. Never drop CEF objects, or call CEF methods that fire callbacks, while it is borrowed.
- Loading a `data:` error page from `OnLoadError` adds a history entry, so `back` loops back into the error. Instead, draw the error into Chromium's own error document from `OnLoadEnd`.
- The command line is a Rust-owned buffer. Keys are consumed in `OnPreKeyEvent` and the status bar just displays the text, so UI pages never need keyboard focus.

Known gaps carried forward:
- ~~The Chromium sandbox is disabled.~~ Since M8, the sandbox is on whenever Linux allows it (user namespaces, or a setuid `chrome-sandbox`). Otherwise the browser warns and runs without it (`hb_config::sandbox`). CI enables user namespaces and checks that the sandbox runs. The AppArmor profile in the README hasn't been tried on a real Ubuntu machine yet (it needs root). macOS and Windows still run unsandboxed until M10.
- Popups and `:open -t/-b/-w` load in the current tab until M3.
- When an event carries no character, key translation falls back to a US layout. Verify with other layouts.
- Status messages expire after 3 s. Completion covers only command names (M2 extends it).

M1 is essentially complete as a by-product (modes, key parser, scrolling, navigation, `o`, status bar). The M2 command line, history and `;;` chaining also exist. Remaining M1/M2 work: the command registry macro, `Tab` completion selection, and URL/history completion.

### M1 — Minimal vim browser
- `hb-core`: mode manager, key parser, command registry.
- Normal/insert modes, scrolling, back/forward/reload, `o` open.
- Status bar with mode, URL, load progress.

### M2 — Command line & completion
- `:` command line, parser, chaining, history.
- Fuzzy completion framework.

### M3 — Tabs ✅ done 2026-10-02
- Multiple tabs in one window, tab bar, close/undo, tab commands.

Notes: index logic is `hb_core::tabs::TabList` (unit tested). All tab `BrowserView`s share a fill-layout panel, and only the current one is visible. Popups go through CEF's `on_popup_browser_view_created`, so `window.opener` survives. A page's `window.close()` closes only its tab. Gaps: undo restores the URL only (not back/forward history), closing a tab skips `beforeunload` prompts, and there is still a single window.

### M4 — Hints ✅ done 2026-10-02
- JS hint engine, all hint targets, rapid mode.

Notes: labels use qutebrowser's scattered letter algorithm (`hb_core::hints::labels`, unit tested against qutebrowser's output). Clicks are real mouse events sent at the element's centre (`send_mouse_click_event`), so pages see `isTrusted` input and `target=_blank` links become tabs. Gaps: cross-origin iframes are hinted only as a whole (same-origin ones are searched since 2026-10-02); a page can interfere with hints on its own page by redefining `window.__hbHints`; labels for elements that move after the hints are drawn don't follow them.

### M5 — Config ✅ done 2026-10-02
- `config.toml` (settings, bindings, aliases, per-domain overrides); `:set`, `:bind`, live reload.
- qutebrowser-compatible setting names where they make sense.

Notes:
- **Settings registry:** `hb_core::settings`. 15 typed settings with validation; each one has a real effect. Values from TOML, Lua and `:set` all pass through JSON, so validation lives in one place.
- **Sources** (`hb-config`): `autoconfig.toml`, then `config.toml`, then `config.lua`. Each becomes a list of `ConfigOp`s the engine applies.
  - `:set`/`:bind`/`:unbind` persist to `autoconfig.toml`, never to the user's own files. They warn when a config file overrides the value at startup.
  - Bindings to unknown commands are rejected at load time.
- **Lua:** `mlua` with vendored Lua 5.4, so it builds on all three platforms with no system Lua. It provides the `c` proxy, `hb.*` and a config-dir `require` searcher. Errors read `file:line: message`, and changes made before an error still apply. `--lua-types` emits lua-language-server definitions generated from the registry. The checked-in `docs/lua/hb.meta.lua` and `docs/settings.md` are tested for staleness.
- **Paths:** XDG on Linux. On macOS, `~/.config` for config and Application Support for data. On Windows, `%APPDATA%` for config and `%LOCALAPPDATA%` for data. `XDG_*` is honoured everywhere, and `--basedir` overrides all. Unit tests cover all three platforms' rules, but macOS and Windows builds have not been run yet.

Gaps:
- ~~Per-domain settings (with M8). Answering a permission prompt with "always" should save a per-domain setting to `autoconfig.toml`, as qutebrowser does, which also fixes camera and microphone answers being forgotten.~~ Done in M8 (2026-10-02).
- Watching config files for changes (`:config-source` reloads by hand).
- `:config-edit`.
- Importing qutebrowser's `config.py`.

### M6 — Storage ✅ done 2026-10-02
- History, bookmarks, quickmarks, sessions.

Notes:
- **`hb-storage`:**
  - History is SQLite, with a per-visit table and a per-URL table for completion, like qutebrowser.
  - Quickmarks and bookmarks use qutebrowser's text formats, so you can import by copying the files. If an existing file can't be read, the browser treats it as read-only, so it's never overwritten with an empty list.
  - Sessions are TOML, with names restricted to safe file names.
- **Completion:** `hb_core::completion` decides what to offer, and the browser layer supplies the quickmark, bookmark, history and session sources. `Tab`/`Shift-Tab` cycle without re-querying, and the popup shows category headers and scrolls to the selection.
- **Settings:** `auto_save.session` and `completion.web_history.max_items`.

Gaps:
- Sessions restore only each tab's current page; CEF has no API to rebuild back/forward history.
- No crash-recovery autosave, no history page, no private browsing (history is always recorded).
- No import of qutebrowser's `history.sqlite`.

### M7 — Prompts, downloads, permissions ✅ done 2026-10-02
- Unified prompt UI; download manager; permission and auth dialogs.

Notes:
- **Prompts:** `hb_core::prompt` holds a queue answered one at a time in the new `prompt`/`yesno` modes, and `hb-cef/src/prompts.rs` connects each prompt to its CEF callback. JavaScript dialogs are withdrawn when their page navigates, and every prompt for a tab when the tab closes.
- **Lessons learned:**
  - Chromium ignores input to a page while it shows a JavaScript dialog, so prompt keys come through the status bar's browser, which gets focus for the duration.
  - CEF calls `GetAuthCredentials` on the IO thread, so the prompt is posted to the UI thread.
  - Chrome's own login prompt swallows HTTP auth unless `--disable-chrome-login-prompt` is set (cef#3603). CEF always runs Chrome's internals now, even for Alloy-style windows.
  - Chromium saves permission answers per site in the profile. So `y`/`N` map to accept/deny (saved), `n` to dismiss (not saved), and a `content.*=false` setting to ignore, so changing the setting later still works. Camera and microphone requests go through a separate CEF API that isn't saved, so they keep a session memory (`A`/`N`).
- **Downloads:** `hb_config::downloads` finds the platform Downloads folder (XDG on Linux), sanitises suggested names and picks unused names (unit tested).

Gaps:
- No per-download bar.
- No path completion in the save prompt.
- No command to reset per-site permissions (Chromium's saved answers).
- ~~Camera and microphone answers are forgotten on restart, so video-call sites ask every session.~~ `A`/`N` now save a per-site setting (M8).
- Closing a tab with `d` skips leave-page warnings.
- File-upload dialogs (`<input type=file>`) use CEF's default and are untested.
- TLS errors have no override.

### M8 — Content blocking & privacy
- ✅ **adblock-rust via `OnBeforeResourceLoad`** (2026-10-02). The new `hb-adblock` crate (CEF-free, unit tested) uses `adblock` 0.13 without its `single-thread` feature, so the engine is `Send + Sync` for CEF's IO thread.
  - EasyList and EasyPrivacy (135k rules) compile in ~56 ms (release) into a ~6 MB cache that loads in ~18 ms. A check takes ~1.5 µs.
  - `:adblock-update` downloads through `CefURLRequest`, so there's no HTTP client dependency, and `file://` lists work. It compiles on a worker thread.
  - Settings: `content.blocking.enabled`, `content.blocking.adblock.lists` and `content.blocking.whitelist`, all with qutebrowser's names.
  - Top-level navigations are never blocked. The smoke test serves a page on 127.0.0.1 and checks that a listed script is cancelled while another loads.
  - ✅ Cosmetic filtering (2026-10-02): after `on_load_end` the main frame gets a `<style>` with `url_cosmetic_resources` hide selectors (one rule per selector, so an invalid one can't void the rest). Unless the site has `generichide`, it also gets `hidden_class_id_selectors` for the page's classes and ids. Covered by unit tests and a smoke step.
  - Not done: re-checking content added after load (MutationObserver), subframes, procedural filters, scriptlets and `$redirect` resources, a blocked count in the status bar, automatic list updates, and qutebrowser's hosts-file method.
- ✅ **Per-domain settings** (2026-10-02):
  - `Settings` keeps `(pattern, name, value)` overrides for an allowlist (`settings::PER_DOMAIN`: the `content.*` permission settings and `content.blocking.enabled`). `get_for(name, url)` returns the last matching one.
  - `hb_core::url::pattern_matches` handles hosts, `*.` subdomains, origins with ports, and Chrome match patterns. It's shared with Greasemonkey.
  - `ConfigOp::SetFor` comes from `:set -u <pattern>`, `[per_domain."<pattern>"]` in TOML (autoconfig writes it the same way) and `hb.set(name, value, pattern)` in Lua.
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
  - Changes are in `hb-cef/src/privacy.rs`: `component_updates.component_updates_enabled = false` (Chromium still updates the components it exempts as security data, as with the `ComponentUpdatesEnabled` policy), spell-check dictionaries off, sign-in off, Chrome's default search engine off, and `--disable-features=AimEnabled,PreconnectToSearch,SearchEnginePreconnect2`, merged with any `--disable-features` the user passes.
  - Prefs are written into `Local State` and `Default/Preferences` before CEF starts, because these services start within 100 ms. `CefPreferenceManager::SetPreference` from `on_context_initialized` is too late. Through cef-rs it also fails silently unless the `error` out-string is non-empty, since an empty `CefString` is passed as NULL.
  - Feature names in `libcef.so` strings carry a `k` prefix that Chromium strips at runtime (`kAimEnabled` → `AimEnabled`). Class names such as `AimEligibilityService` are not features.
  - The profile lives in `data/Default`: Chromium uses that name whatever `cache_path` says, so `cache_path` now points there.
  - Left: `accounts.google.com/ListAccounts` (`gaia_auth_list_accounts`) still runs once at startup. Some startup service asks `GaiaCookieManagerService` for the cookie jar, which isn't found yet.
  - **Widevine is no longer downloaded**, since it updates through the component updater. M11 must find a way to update only the Widevine component when the user opts in.
- Original item: Review background Google service traffic (component updater, Safe Browsing, optimization hints, Variations, CRLSets). Keep security updates; disable or make opt-in what only serves Google. Document in the README.

### M9 — Power features
- Caret mode, marks, macros, userscripts, greasemonkey, `:open-editor`, search engines.
- ✅ **`:spawn`, userscripts and `:open-editor`** (2026-10-02):
  - `hb_core::shell_words` splits arguments like a POSIX shell, with no shell involved.
  - `hb_config::userscripts::resolve` searches config, then data, then `PATH`.
  - `hb-cef/src/spawn.rs` runs programs on a worker thread and reports back through a UI task. Userscripts get qutebrowser's `QUTE_*` environment. `QUTE_HTML`, `QUTE_TEXT` and `QUTE_FIFO` live in a private 0700 temp directory that is removed afterwards.
  - `editor.command` is validated to contain `{file}`. `js/editor.js` remembers the field and writes the text back with `input`/`change` events. It's bound to `Ctrl-e` in insert mode.
  - Smoke steps cover a userscript (environment plus a FIFO command) and Ctrl-e with a scripted editor.
  - Gaps:
    - `QUTE_FIFO` is a regular file read when the script exits, not a live FIFO, so long-running scripts' commands are delayed until they exit.
    - Not yet: `-o` (output in a tab), `QUTE_USER_AGENT`, hint-mode userscripts (`QUTE_MODE=hints`) and `:spawn` from hints.
    - Password fields are skipped by `:open-editor`.
    - The remote socket can run `:spawn`; it is limited to the same user (M15).
- ✅ **Marks** (2026-10-02): `` ` `` and `'` enter the `set_mark` and `jump_mark` modes (qutebrowser's names), and the next key names the mark (`Command::Mark`, unit tested). `hb-cef/src/marks.rs` reads and sets `scrollX`/`scrollY` through the eval channel. Uppercase marks also reopen their page and scroll once `on_load_end` fires. `''` goes back to where the last jump started. Marks last for the session. A smoke step covers `` `a ``, `gg`, `'a` and `''`.
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
  - Not done: paragraph/block moves (`{`, `}`), the primary selection (`Y`), and following a selected link with `Return`.
- ✅ **Greasemonkey** (2026-10-02):
  - `hb_config::greasemonkey` parses the metadata block and matches URLs (Chrome match patterns plus `@include`/`@exclude` globs), with unit tests.
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
  - `hb-cef/src/search.rs` uses `BrowserHost::Find` and a `FindHandler`, which reports "Match i of n" or "not found".
  - In this CEF build `find_next = false` never activates or scrolls to a match. Every call passes `true`; repeating the same text first calls `StopFinding(clear_selection)`, so it starts again from the top.
  - `search.ignore_case` is `smart`, `always` or `never`. Chromium always wraps around.
  - A smoke step checks that `/needle` scrolls to the first match and `n` to the second.
- ✅ **`:navigate`** (2026-10-02): `up`, `increment` and `decrement` are pure URL functions in `hb_core::url` (unit tested: query and fragment first, leading zeros kept, the host never touched). `prev`/`next` use `js/navigate.js`: `rel` links first, then link text matching qutebrowser's default `hints.prev_regexes`/`hints.next_regexes`. These are bound to `gu gU [[ ]] {{ }} Ctrl-a Ctrl-x` as in qutebrowser and covered by a smoke step. The regexes aren't settings yet.

### M10 — Packaging
- Linux tarball / AppImage / AUR / Nix; then macOS app bundle (`bundle-cef-app`) and Windows.
- ✅ **Tarball and AppImage** (2026-10-02): `scripts/package-linux.sh [--appimage]` stages the stripped binary and CEF runtime. The AppImage adds `packaging/hackers-browser.{desktop,svg}` and an `AppRun`, and is built by appimagetool 1.9.1 (pinned and checksum-verified; it fetches its runtime itself). `./task appimage` builds both locally, and `release.yml` publishes both.
  - Tested locally: the 146 MB AppImage starts, opens a window under Xvfb, quits cleanly and unmounts.
  - Inside an AppImage `chrome-sandbox` can't be setuid, so the sandbox needs user namespaces (see the README).
  - Not done: AUR, Nix, the macOS app bundle and Windows packaging (none can be tested on this machine).

### M12 — Lua scripting ✅ mostly done 2026-10-02
Builds on the M5 Lua config API.

Result:
- The VM that ran `config.lua` is kept in a UI-thread `thread_local` (`hb_config::lua`, since `mlua::Lua` isn't `Send`). Callbacks get a `Context` (URL, title, mode, count) and return `Action`s (`Run(line)`, `Message`), which `hb-cef/src/lua.rs` carries out outside any shell borrow. A depth limit stops hooks that trigger each other.
- `hb.bind(keys, function)` stores the function and binds `lua-call <id>`.
- `hb.command(name, fn, description)` defines commands. The engine parses them as `Command::User`, accepts bindings to them and completes them next to built-ins; built-in names are refused.
- `hb.on` supports `load_finished`, `url_changed`, `tab_opened` and `mode_changed`.
- In callbacks, `hb.set` becomes a `:set`.
- Unit tests in hb-config drive the VM without CEF (bindings, commands, hooks, errors with `config.lua:line`). A smoke step uses all three entry points.
- Lua commands appear on the help page, after the built-in ones.
- Not done: a sandbox for third-party scripts (only the user's own config runs), a richer API (tabs list, settings watchers), and Lua userscripts.

Original plan:
- Bind keys to Lua functions: `hb.bind("<Ctrl-g>", function() ... end)`.
- Lua-defined commands: `hb.command("name", fn)`, with completion.
- Event hooks: `hb.on("load_finished", fn)`, `hb.on("tab_opened", fn)`, mode changes.
- A small runtime API: current tab URL and title, open URLs, run commands, show messages.
- Userscripts written in Lua, alongside qutebrowser-compatible external userscripts (M9).
- Decide on a sandbox for third-party scripts (e.g. no `io`/`os` unless allowed). The user's own `config.lua` stays fully trusted.

### M13 — Internal pages and a UI channel (foundation for M14 and M16) ✅ done 2026-10-02

Result:
- **Scheme:** `hb://` is registered as standard + secure + display-isolated and served from embedded files by `hb-cef/src/scheme.rs`. Responses carry a strict CSP (inline code only, no network), `nosniff` and `no-store`.
- **UI pages:** the tab bar, status bar and overlay moved from `data:` URLs to `hb://ui/…`.
- **Channel:** `hb.send(name, json)` exists only in `hb://ui/` frames. The browser re-checks the sending frame's URL itself, and `hb_core::ui_message` validates each message against a per-page allowlist (unit tested).
- **First message:** clicking a tab in the tab bar selects it.
- **Isolation, as verified:**
  - Web pages see no `window.hb`.
  - An `hb://` iframe stays empty, and an `hb://` link does nothing; Chromium's display isolation refuses both.
  - `:open hb://ui/…` and redirects to `hb://` are blocked by `OnBeforeBrowse`.
  - A smoke test covers the first two.

Original plan:
- **`hb://` scheme:** register it with `CefSchemeRegistrar::AddCustomScheme` and serve it from embedded files through a `CefSchemeHandlerFactory`. Move the tab bar, status bar and overlay pages off `data:` URLs onto `hb://ui/...`.
- **UI → Rust messages:** in the renderer's `OnContextCreated`, add a `window.hb.send(name, json)` function **only for frames whose URL is `hb://`**. Web pages never see it.
  - The browser process accepts these messages only from our UI browsers, and still validates every field. This is the reverse of the eval channel, needed for clicks in the tab bar and for links on the help page.
- Opening `hb://` from a web page (link, redirect, `window.open`) is blocked in `OnBeforeBrowse`. Only the user (`:open hb://help`) or the browser itself can open it.

### M14 — Tabs: pinned, mouse, favicons ✅ done 2026-10-02

Result:
- **Pinned tabs:** `TabList` keeps them first (unit tested: pinning, moves, inserts and removals stay outside or inside the block as they should). Added `:tab-pin` / `Ctrl-p`, `--force` for `tab-close`/`tab-only`, the `tabs.pinned.frozen` and `tabs.pinned.shrink` settings, and pin state in sessions (older session files still load).
- **Mouse in the tab bar:** click, middle-click to close (pinned tabs refuse), wheel (`tabs.mousewheel_switching`) and drag to reorder, all as allowlisted `hb.send` messages. Drag uses pointer events rather than HTML5 drag and drop, so it never involves the OS or other applications.
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
- **Code:** `hb_config::remote` (protocol, socket paths, client, server, argument handling; unit tested over a real socket) and `hb-cef/src/remote.rs`.
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
- **Behaviour:** `hackers-browser example.com` or `hackers-browser ':open -t example.com' ':tab-focus 1'`, run while the browser is already open, sends the URLs and commands to that instance and exits, like qutebrowser.
  - A URL opens per `new_instance_open_target` (`tab`, `tab-bg`, `window`).
  - An argument starting with `:` runs as a command.
  - `--target` overrides the open target for one call.
- **Transport:** a local socket per profile.
  - On Unix, `$XDG_RUNTIME_DIR/hackers-browser/<hash of basedir>.sock`, or the data dir if `XDG_RUNTIME_DIR` isn't set. The directory is `0700` and the socket `0600`, and the server checks the peer's user id (`SO_PEERCRED` / `getpeereid`).
  - On Windows, a named pipe restricted to the current user.
  - The [`interprocess`](https://crates.io/crates/interprocess) crate covers both. The protocol is versioned JSON lines (`{"version":1,"args":[…],"cwd":"…","target":…}`).
- **Startup order:** check for a running instance before CEF initialises, because Chromium's profile lock would otherwise refuse the second process. Remove a stale socket left behind by a crash.
- **Security:** anyone who can write to the socket can run commands, including `:spawn` once that exists (M9). Only the same user may connect, and nothing listens on the network.
- The same channel later serves userscripts' `QUTE_FIFO`-style command input (M9 and M12).
- **Tests:** protocol and argument handling in a CEF-free crate. A smoke step sends `:open -t` to the running test browser.

### M16 — Help pages ✅ done 2026-10-02

Result:
- **Data:** `hb_core::help::build` assembles commands (with the keys bound to them, including `cmd-set-text` prefills), settings (current value, default, type, and the file that set it) and per-mode bindings (with changed and removed ones marked) from the live registries. Unit tested.
- **Page:** `hb-cef/src/help.rs` fills `ui/help.html` with that JSON and rebuilds it after every config load and `:set`/`:bind`. The scheme handler serves it from a shared `RwLock`.
- **Look:** light/dark themes, sticky search (`/`), side navigation and anchors (`:help :open`, `:help hints.chars`, `:help bindings`, `:version`). Built with `textContent` only.
- **Commands:** `:help [-t] [topic]`, `:version` and `F1`. `--version` now prints the git commit and CEF/Chromium versions (from `hb-cef/build.rs`).
- A smoke step opens `:help :open`.

Done: Lua-defined commands are listed too (M12).

Original plan:
- **`:help [topic]`** opens `hb://help`, a set of pages generated from the live registries, so it is always current:
  - **commands:** name, arguments and description from `COMMANDS`, with any `config.lua`-defined commands added once M12 exists
  - **settings:** type, default, *current value* and where it was set (default, `config.toml`, `config.lua` or `:set`)
  - **key bindings:** per mode, including the user's bindings, with changes from the defaults marked
  - plus pages on modes, hints, the config files and their paths, and the Lua API
  - `:help :open` and `:help hints.chars` jump straight to an entry.
- **Look:** clean and readable in light and dark themes (following `prefers-color-scheme`), keyboard-first.
  - `f` hints and `/` search work normally.
  - A search box filters commands and settings as you type, and the page works without a mouse.
- `:version` (`hb://version`) shows the version, git commit, CEF/Chromium version, the paths from `--paths`, and the loaded config files.
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
  - Also offer `hackers-browser --install-dictionary en-US`, like qutebrowser's `dictcli`, which fetches from the Chromium dictionary repository and verifies a pinned checksum.
  - Document both and let the user choose.
- **Fixing words from the keyboard:**
  - `:spell-suggest` puts the suggestions for the misspelled word under the cursor in the completion popup (`Tab` to pick, `Return` to replace). It uses `CefContextMenuParams::GetDictionarySuggestions` / `BrowserHost::ReplaceMisspelling`, or a renderer query if the context-menu path needs a right-click.
  - `:spell-add` (`AddWordToDictionary`) adds the word to your dictionary.
  - Right-click suggestions come for free through CEF's context menu.
- Open question: should spell checking stay off by default (privacy-friendly) or follow the system locale?

### M18 — Versioning, changelog and CI
- **One version for the whole workspace** (`workspace.package.version`), following semver. Stay on 0.x until the plan's core is done.
- **The binary reports what it is:**
  - `--version` prints e.g. `hackers-browser 0.4.0 (abc1234, CEF 154.0.32, Chromium 154.0.8037.58)`.
  - The git commit comes from a `build.rs` (`git describe --always --dirty`), falling back to "unknown" in source tarballs.
  - CEF and Chromium versions come from `cef::sys` constants.
  - `:version` shows the same (M16).
- **Changelog:** `CHANGELOG.md` generated by [git-cliff](https://git-cliff.org) from commit messages; cef-rs uses the same setup.
  - Releases are tagged `vX.Y.Z` and the release notes come from the changelog.
  - In the browser, `:changelog` opens the bundled `CHANGELOG.md` (`hb://changelog`), and the first start after an upgrade shows "Updated to 0.5.0. :changelog for details" in the status bar.
- **CI (GitHub Actions)** ✅ `check.yml` added 2026-10-02 (commit messages, Linux lint/tests/smoke, macOS and Windows build + unit tests).
- ✅ **Done 2026-10-02:**
  - `cliff.toml` and `CHANGELOG.md`. Pre-Conventional "Add …/Scaffold …" commits are sorted under Features.
  - `scripts/git-cliff.sh` downloads a pinned, checksum-verified git-cliff, like `./task`. `./task changelog` regenerates the changelog.
  - `release.yml` runs on a `v*` tag: it checks the version, builds `--release`, and publishes `scripts/package-linux.sh`'s tarball with `SHA256SUMS` and `git-cliff --latest` notes. The libraries are stripped: CEF's `libcef.so` has debug info and goes from 1.4 GB to 260 MB, giving a 156 MB tarball.
  - `:changelog [-t]` serves the bundled changelog at `hb://changelog/` through `hb_core::changelog::to_html`, a minimal, escaping Markdown renderer that is unit tested. The "Updated to X" notice compares `<data>/last-version`.
  - No release has been tagged yet; that's the maintainer's call. Running `release.yml` by hand is a dry run that keeps the files as a one-day artifact. Run 37088697211 (2026-10-02) built the tarball, the AppImage, `SHA256SUMS` and the notes (301 MB in total) with publishing skipped.
  - Not done: macOS and Windows release artifacts (M10).
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

### Deferred — proprietary codecs (H.264 / AAC)
Not scheduled. H.264/AAC require building CEF/Chromium from source with `proprietary_codecs=true` and `ffmpeg_branding="Chrome"`. That means hours and a lot of disk space per release, and it works against goal 1 (tracking Chromium quickly). Distributing such builds also raises patent-licensing questions. Revisit only if VP9/AV1 Widevine proves insufficient; if so, prefer a documented "build your own CEF" path over shipping these binaries.

---

## Chromium update cadence

- Pin `cef` crate to an exact version (e.g. `=154.3.0`).
- CI job checks for new CEF releases weekly; upgrades go through a branch with smoke tests.
- Keep CEF-specific code isolated in `hb-cef` so binding churn doesn't leak into core logic.

## Testing strategy

- **Unit tests** in `hb-core`: key parsing, mode transitions, command parsing, config merging (the bulk of logic).
- **JS tests** for the hint / insert-detection scripts (headless).
- **Integration tests**: launch the browser against a local test server and drive it through a debug control channel (e.g. `--remote-debugging-port` + CDP) to assert behavior.

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

> Note: the project is GPL-3.0 (per `LICENSE`). Dependency licenses must stay GPL-compatible (CEF is BSD, adblock-rust is MPL-2.0 — both fine).
