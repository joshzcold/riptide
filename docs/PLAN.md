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
- [ ] Marks (`m` / `` ` ``) and quickmarks / bookmarks

### Tabs
- [x] `J` / `K` next/prev, `d` close, `u` undo close, `gt` / `gT`, `Alt-<n>`, `Ctrl-Tab` last-focused
- [x] `:tab-move`, `:tab-only`, `:open -t/-b/-r`, popups as tabs (keeping `window.opener`)
- [ ] `:tab-pin`, `:tab-clone`, `:tab-give`, `:tab-take`
- [ ] Multiple windows

### Hints
- [x] `f` / `F` follow (current / new tab), `;b` background, `;y` yank, `;i` / `;I` image, `;o` / `;O` fill, `;h` hover, `;t` inputs, `;r` rapid
- [ ] `;d` download (needs M7)
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
- [ ] Downloads with status-bar progress and prompts
- [ ] Permission prompts (geolocation, camera, notifications)
- [ ] HTTP auth and JS dialogs in the prompt UI
- [ ] Dark mode (Chromium `--force-dark-mode` / blink settings)
- [ ] Widevine DRM, opt-in (M11)
- [ ] Review of background Google service traffic (M8)

### Extensibility
- [ ] Userscripts (spawned processes with `QUTE_URL`, `QUTE_FIFO`, etc.; keep env-var names for compatibility)
- [ ] Greasemonkey-style injected JS
- [ ] `:spawn` external commands
- [ ] `:open-editor` (edit text field in `$EDITOR`)

### Session / state
- [ ] Sessions (save / load / autosave)
- [ ] History with completion
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

### M5 — Config
- `config.toml` (settings, bindings, aliases, per-domain overrides); `:set`, `:bind`, live reload.
- qutebrowser-compatible setting names where they make sense.

### M6 — Storage
- History, bookmarks, quickmarks, sessions.

### M7 — Prompts, downloads, permissions
- Unified prompt UI; download manager; permission and auth dialogs.

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

1. **Config language:** TOML only, or add Lua (`mlua`) for a programmable `config.py`-equivalent?
2. **Platform priority:** Linux-only until M10, or keep macOS/Windows building in CI from the start?
3. **UI overlay tech:** plain HTML/CSS/vanilla TS, or a small framework?
4. **qutebrowser compatibility depth:** import qutebrowser `config.py` bindings, quickmarks, and bookmarks?

> Note: the project is GPL-3.0 (per `LICENSE`). Dependency licenses must stay GPL-compatible (CEF is BSD, adblock-rust is MPL-2.0 — both fine).
