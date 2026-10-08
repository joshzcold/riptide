# Architecture

Riptide is one Rust workspace around CEF's prebuilt Chromium. The [project plan](https://github.com/joshzcold/riptide/blob/main/docs/PLAN.md) records the reasoning behind each decision and the lessons learned per milestone; this page is the map.

## Crates

| Crate | Purpose |
|---|---|
| `crates/riptide` | The `riptide` binary: `main()`, which hands subprocesses to CEF and starts the browser. |
| `crates/rt-core` | Modes, key parsing, bindings, commands, the command line, settings, URL guessing and the help data. **No CEF dependency**; unit tested. |
| `crates/rt-config` | Config paths per platform, the command line, TOML/Lua/autoconfig loading, the single-instance socket protocol, and the generated Lua types, settings docs and reference pages. |
| `crates/rt-storage` | History (SQLite), quickmarks and bookmarks (qutebrowser formats), sessions (TOML). |
| `crates/rt-adblock` | Ad and tracker blocking with Adblock Plus filter lists (Brave's adblock-rust). `resources/ubo.json` holds uBlock Origin's scriptlets and `$redirect` stand-ins; refresh it with `scripts/update-adblock-resources.sh` (needs Node) when uBlock Origin releases. |
| `crates/rt-cef` | CEF integration: window layout, handlers, renderer-process bindings, the `riptide://` pages and the status bar and completion UI. |

The rule that keeps this testable: `rt-core` never depends on CEF. CEF is an adapter that turns events into `rt-core` inputs and carries out the actions `rt-core` returns.

## The core loop

```
Key event (OnPreKeyEvent, before the page sees it)
  → rt_core::Engine: mode, counts, multi-key sequences (gg, ;y)
  → a Command (:scroll down, :open -t …)
  → handled inside the engine (modes, command line), or
    returned as Effect::Run { command, count } for rt-cef to carry out on a tab, window, UI or storage
```

`Engine` takes a `Key` and returns `KeyOutcome { consumed, effects }`, so every binding and mode transition is unit tested without a browser.

## Processes

There is a single executable. CEF launches its renderer, GPU and utility processes by running it again, and `main()` dispatches them through `cef::execute_process`.

- **Browser process:** windows, tabs, modes, commands, config and storage.
- **Renderer processes:** a Rust `RenderProcessHandler` that runs our page scripts (`crates/rt-cef/js/`: hints, scrolling, caret mode, navigation, spell checking, the editor) and reports results back through process messages. Pages can't forge those replies, and no global function is exposed to page scripts.

## The window

Each window uses CEF Views in the Alloy runtime style. Every tab is its own `BrowserView`, and only the current one is visible. The tab bar, status bar, completion, prompts and messages are HTML pages (`crates/rt-cef/ui/`) served by the browser itself at `riptide://ui/…`, in their own views. Only `riptide://ui/` pages get the `rt.send()` channel to Rust, and the browser accepts only the messages each page is allowed to send. Web pages can't link to, frame or redirect to `riptide://` addresses.

## Where to look

- A new command: `crates/rt-core/src/command.rs` (`COMMANDS` and the parser), then its `Effect` handling in `crates/rt-cef/src/shell.rs`.
- A new setting: `crates/rt-core/src/settings.rs`. Types, docs and `:help` pick it up from the registry.
- A default binding: `crates/rt-core/src/keymap.rs`.
- Page-side behaviour: `crates/rt-cef/js/` and its Rust side in `crates/rt-cef/src/`.
