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
- [ ] Hint, Caret, Prompt, Yes/No, Register (marks/macros)

### Navigation
- [x] `o` / `go` open / edit current URL (current tab only)
- [x] `O` / `gO` open in a new tab
- [x] `H` / `L` back/forward, `r` / `R` reload
- [ ] `gu` / `gU` go up URL
- [x] `hjkl`, `gg`, `G`, `0`, `$`, `Ctrl-d/u/f/b` scrolling with counts
- [ ] `/`, `?`, `n`, `N` search
- [ ] `[[` / `]]` prev/next page navigation
- [x] Quickmarks / bookmarks (`m`, `b`, `B`, `M`, `gb`, `gB`)
- [ ] Marks (`` ` ``-style in-page marks; `m` is the quickmark key, as in qutebrowser)

### Tabs
- [x] `J` / `K` next/prev, `d` close, `u` undo close, `gt` / `gT`, `Alt-<n>`, `Ctrl-Tab` last-focused
- [x] `:tab-move`, `:tab-only`, `:open -t/-b/-r`, popups as tabs (keeping `window.opener`)
- [ ] `:tab-pin` and pinned tabs (M14), `:tab-clone`, `:tab-give`, `:tab-take`
- [ ] Mouse: click/middle-click/wheel/drag in the tab bar, middle-click links (M14)
- [ ] Favicons in the tab bar and completion (M14)
- [ ] Multiple windows

### Hints
- [x] `f` / `F` follow (current / new tab), `;b` background, `;y` yank, `;i` / `;I` image, `;o` / `;O` fill, `;h` hover, `;t` inputs, `;r` rapid
- [x] `;d` download
- [ ] Number hint mode, configurable chars (M5), hints inside iframes

### Command line
- [ ] `:` command entry with history, fuzzy completion (commands, URLs, history, bookmarks, tabs, settings)
- [ ] Command chaining (`;;`), aliases, `:bind` / `:unbind`, `:set`

### Yank / paste
- [x] `yy`, `yt`, `yd`, `pp`, `PP` (`{clipboard}` is substituted per command, so pasted text can't add `;;` commands)
- [ ] Primary selection (`yY`, `pP`)

### Content
- [ ] Ad blocking (EasyList / uBlock lists) and host blocking
- [ ] Per-domain settings (JS, cookies, images, notifications)
- [x] Downloads with status-bar progress and prompts
- [x] Permission prompts (geolocation, camera, notifications)
- [x] HTTP auth and JS dialogs in the prompt UI
- [ ] Download bar listing each download; path completion in the save prompt
- [ ] TLS certificate errors: Chromium blocks them; no "proceed anyway" prompt yet
- [ ] Dark mode (Chromium `--force-dark-mode` / blink settings)
- [ ] Widevine DRM, opt-in (M11)
- [ ] Review of background Google service traffic (M8)

### Help and tooling
- [ ] `:help` pages generated from the live commands, settings and bindings; `:version` (M16)
- [ ] `hackers-browser ':cmd' url` talks to the running instance (M15)
- [ ] Spell checking with keyboard-driven suggestions (M17)
- [ ] Versioned releases, `CHANGELOG.md`, CI on Linux/macOS/Windows (M18)

### Extensibility
- [ ] Userscripts (spawned processes with `QUTE_URL`, `QUTE_FIFO`, etc.; keep env-var names for compatibility)
- [ ] Greasemonkey-style injected JS
- [ ] `:spawn` external commands
- [ ] `:open-editor` (edit text field in `$EDITOR`)

### Session / state
- [x] Sessions (save / load / `auto_save.session`, `:wq`)
- [x] History with completion (`:open` + `Tab`)
- [x] Quickmarks and bookmarks in qutebrowser's file formats
- [ ] Crash-recovery autosave, history page (`qute://history`-like), importing qutebrowser's history.sqlite
- [ ] Private windows (separate `CefRequestContext`)

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
- **The Chromium sandbox is disabled** (`no_sandbox`). On Linux, the `sandbox` feature needs a SUID-root `chrome-sandbox` or unprivileged user namespaces. Ubuntu 24.04's AppArmor restricts the latter. Resolve before anyone uses this for real browsing (target: M8 or earlier).
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

Notes: labels use qutebrowser's scattered letter algorithm (`hb_core::hints::labels`, unit tested against qutebrowser's output). Clicks are real mouse events sent at the element's centre (`send_mouse_click_event`), so pages see `isTrusted` input and `target=_blank` links become tabs. Gaps: iframes aren't hinted; a page can interfere with hints on its own page by redefining `window.__hbHints`; labels for elements that move after the hints are drawn don't follow them.

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
- Per-domain settings (with M8).
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
- Closing a tab with `d` skips leave-page warnings.
- File-upload dialogs (`<input type=file>`) use CEF's default and are untested.
- TLS errors have no override.

### M8 — Content blocking & privacy
- adblock-rust via `OnBeforeResourceLoad`; filter-list updates (`:adblock-update`); per-domain settings; private windows.
- **Review background Google service traffic.** CEF's component updater is on by default. On first run it already downloads, without asking: Widevine (21 MB), Safe Browsing lists, optimization hints, Variations (Google's feature-config download), certificate revocation lists (CRLSets) and more. These end up as directories under `~/.local/share/hackers-browser/`.
  - List every Google endpoint the browser contacts (watch a fresh profile's network traffic) and what each one provides.
  - Decide keep/disable/opt-in per item. Keep security updates (CRLSets, certificate/PKI metadata); disable or make opt-in anything that only serves Google (optimization hints, Variations).
  - Find a per-component switch. `--disable-component-update` turns off everything, including CRLSets, which we should not lose.
  - Document the result in the README.

### M9 — Power features
- Caret mode, marks, macros, userscripts, greasemonkey, `:open-editor`, search engines.

### M10 — Packaging
- Linux tarball / AppImage / AUR / Nix; then macOS app bundle (`bundle-cef-app`) and Windows.

### M12 — Lua scripting
Builds on the M5 Lua config API.
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

Not done: help for Lua-defined commands (M12).

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

### M17 — Spell checking
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
- **CI (GitHub Actions)** ✅ `check.yml` added 2026-10-02 (commit messages, Linux lint/tests/smoke, macOS and Windows build + unit tests). Still to do: `release.yml`.
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

### M11 — Widevine DRM (opt-in)
Depends on M5 (settings) and the M8 component review.

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
