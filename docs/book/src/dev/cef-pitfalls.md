# CEF pitfalls

Things CEF, Chromium and the `cef` crate do that cost time to find out. Add to this page when you hit one, next to the area it belongs to.

## Views and windows

- **A preferred size with a 0 width or height counts as unset.** The view then takes its large default and squeezes the page out of the layout. Always return a non-empty size (`window::bar_size`).
- **Dropping the last reference to a `BrowserView` closes its browser synchronously**, which re-enters our handlers. The shell's state is a UI-thread `RefCell`, so never drop CEF objects, or call CEF methods that fire callbacks, while it's borrowed (`shell::with`).
- **The `cef` crate answers 0 for `can_resize`, `can_maximize` and `can_minimize` unless the delegate implements them**, where CEF's own default is 1. Windows then ask for a fixed size and tiling window managers float them. Both window delegates return 1.
- **A Chrome-style `BrowserView` doesn't exist until it's added to a window**, so calling `set_focusable` on it first crashes. A Chrome-style window takes one Chrome-style view, which must be added before any Alloy view (the first view sets the window's profile). Popups must match their opener's style. See `window::create_call` and `tabs::add_call_view`.
- **Overlay views are opaque and only as big as their bounds.** Rounded corners show square ones behind them, and there's no dimming the page behind a box.
- **A hidden view pauses `requestAnimationFrame`.** A page that measures itself before it's shown must do it synchronously (`ui/float.html`).
- **A window doesn't finish closing while any browser in it is open**, overlay views included. Floats and panels are closed in the window's `can_close` (`float::close_window`, `panel::close_window`), or `:quit` hangs.

## Keys and input

- The command line is a Rust-owned buffer; keys are consumed in `OnPreKeyEvent`, so UI pages never need keyboard focus.
- **Chromium ignores input to a page while it shows a JavaScript dialog**, so prompt keys go through the status bar's browser, which gets focus for the duration.
- **A new tab can miss focus requested before its browser existed**; `on_after_created` focuses it again.
- **`send_key_event` to a hidden tab is dropped** (keys go to the window's focused view), and DevTools' `Input.dispatchKeyEvent` reports success without reaching the page in windowed browsers. The first key a page ever gets starts something that drops keys for about 400 ms (`client::send_when_ready`).
- **Under Xvfb, a key sent just as a page finishes loading can be lost** below Views. The e2e harness waits two animation frames after each load.
- **Middle-clicks and Ctrl+clicks come through `OnOpenURLFromTab`**, not popups, and Alloy loads them in the same tab if it isn't handled.
- **Ctrl+wheel over a bar zooms every `riptide://ui` page** (Chromium saves zoom per host). The bar pages refuse it, and `on_load_end` resets a saved zoom for non-tab roles.

## Pages and navigation

- **Loading a `data:` error page from `OnLoadError` adds a history entry**, so `back` loops into the error. Draw into Chromium's own error document from `OnLoadEnd` instead.
- **`BrowserHost::Find` with `find_next = false` never activates or scrolls to a match.** Every call passes `true`; a repeated search first calls `StopFinding`.
- **CEF passes a browser's original `extra_info` to `on_browser_created` again on reload**, which would undo a `:greasemonkey-reload`. Script lists carry a generation number.
- **`Page.addScriptToEvaluateOnNewDocument` silently does nothing without `Page.enable` first** (`adblock::before_navigation`).
- **`Notification` is defined after the context is created**, so the renderer's stand-in takes its place on the first microtask, `DOMContentLoaded` and `load`.
- **A page starting to load clears every status message**, so startup messages wait for the first load (`shell::show_message_after_load`).
- **A stored default for Chromium's `PROTOCOL_HANDLERS` content setting fails a `CHECK`** when a private window's profile inherits it.

## Profiles, preferences and services

- **Chromium names its profile folder `Default` whatever `cache_path` says**, so `cache_path` points there.
- **Services start within 100 ms**, so privacy preferences are written into `Local State` and `Default/Preferences` before CEF starts. `SetPreference` from `on_context_initialized` is too late, and through the `cef` crate it fails silently unless the error out-string is non-empty (an empty `CefString` is passed as NULL).
- **Feature names in `libcef.so` strings carry a `k` prefix** that Chromium strips (`kAimEnabled` → `AimEnabled`).
- **Chrome's login prompt swallows HTTP auth** unless `--disable-chrome-login-prompt` is set (cef#3603).
- **`GetAuthCredentials` runs on the IO thread**; the prompt is posted to the UI thread.
- **Chromium saves permission answers per site**, but camera and microphone requests go through a separate API that isn't saved; riptide keeps its own per-site answers. Chromium also holds non-media permission prompts from hidden tabs until they show.

## The `cef` crate

- **`CefStringList::clone` copies the opaque C struct**, so iterating a clone is always empty; read lists through the C API.
- An empty `CefString` is passed to C as NULL.

## Extensions

- **Manifest V2 is gone** in Chromium 154: MV2 extensions silently don't load.
- **Alloy tabs are invisible to `chrome.tabs.query`**, though messages reach them (`sender.tab` is set). Popups and keyboard commands that act on "the active tab" can't find riptide's tabs.
- **With a `declarativeNetRequest` extension, a page opened during startup may never start loading**; `extensions::after_startup` reloads such tabs.
- **Letting Chromium finish a `.crx` download hands it to its own installer, which deletes the file.** riptide cancels the download and fetches the URL itself.

## Crashes

- **`chrome://crash` with the sandbox on leaves the tab loading** instead of crashing it; the test channel's `CrashTab` aborts the renderer instead.
- **Crashpad reads `crash_reporter.cfg` next to the executable**, and keeps dumps with no size or age limit on Linux (`rt_storage::crash_reports::dumps` prunes them).
- A panic that reaches CEF's C callers panics again ("cannot unwind"); only the first panic on a thread is reported.

## Building and releasing

- **CEF's `libcef.so` carries debug info** (1.4 GB); stripping it gives 260 MB.
- **Inside an AppImage, `chrome-sandbox` can't be setuid**, so the sandbox needs user namespaces there.
- **A tag pushed with a workflow's own token doesn't trigger other workflows**, and GitHub Actions can't bypass a ruleset on a personal repository; see [Releasing](releasing.md).
- **mdBook's smart punctuation turns `--force` into an en dash**, so it's off.

## Testing

- Unix socket paths must fit in about 108 bytes, so the e2e harness uses a short `XDG_RUNTIME_DIR`.
- CEF's zygote leaves the process group; the harness also stops the helpers that carry its `--user-data-dir`.
- `cargo test -p rt-e2e` drives the built `target/debug/riptide` and doesn't rebuild it: run `cargo build` first, or `./task e2e`.
