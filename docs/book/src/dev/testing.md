# Testing

| Command | What it runs |
|---|---|
| `./task test` | Unit tests for `rt-core`, `rt-config` and `rt-storage` (modes, keys, commands, settings, config files, paths for all three platforms, history, marks, sessions); no browser needed |
| `./task e2e` | End-to-end tests in [`crates/rt-e2e`](#end-to-end-tests): real browsers, each on its own Xvfb display, driven through the test channel. `./task e2e -- tabs` runs only the tests whose names contain `tabs`. |
| `./task smoke` | Starts the real browser on a throwaway Xvfb display, drives it with xdotool, and checks insert mode, key consumption, scrolling and a clean `:quit` |
| `./task lint` | `cargo fmt --check`, `clippy -D warnings`, ShellCheck on the scripts, actionlint on the workflows, and cargo-deny (below) |
| `./task check` | All of the above |

The e2e and smoke tests use temporary profiles, so they never touch your browsing data or your browser.

## End-to-end tests

Debug builds (and release builds with `--features test-control`) answer test requests on the [command socket](https://github.com/joshzcold/riptide/blob/main/crates/rt-config/src/remote.rs). Release builds refuse them. There are four requests:

| Request | Does |
|---|---|
| `keys` | Presses keys such as `5j`, `<Escape>` or `:open x<Return>`. They go through the same engine path as typed keys, and to the page when the engine doesn't use them. |
| `run` | Runs a command line, as `:` would. |
| `state` | Returns JSON with the mode, windows, tabs (URL, title, pinned, loading, mode), the status bar, completion and the prompt. |
| `eval` | Runs JavaScript in a tab and returns its string result. |

`crates/rt-e2e` wraps them in a `Browser` that starts riptide with a scratch `--basedir`, its own Xvfb display, runtime directory and command socket, and a local HTTP server for the fixture pages in `crates/rt-e2e/pages/`. It stops only its own processes when the test ends. When a test fails, the end of the browser log is printed and the profile is kept in `/tmp/rt-e2e-*` for a look.

```rust
#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn d_closes_the_tab() {
    let b = Browser::start("page.html");
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("the second tab loads", |s| s.tabs().len() == 2 && s.tab().title == "second");
    b.keys("d");
    b.wait_until("one tab is left", |s| s.tabs().len() == 1);
}
```

- **Wait, don't sleep:** `wait_until`, `wait_mode` and `wait_eval` poll until the state matches, and fail with the last state after 15 seconds. Use `s.tab().is_loaded(&url)` to wait for a page: a new tab knows its URL before it starts loading, so checking the URL alone isn't enough.
- **Start options:** `Browser::launch().toml("…").lua("…").start("page.html")` writes `config.toml` and `config.lua` into the profile first.
- **Restarts:** `config_dir()` and `data_dir()` give the profile's paths. After `:quit` and `wait_exit()`, a `crash()` (SIGKILL) or a `terminate()` (SIGTERM), `restart()` starts the browser again on the same profile, for testing sessions and crash recovery.
- **Hints:** `follow_hint("hint links tab", |h| h.url.as_deref() == Some(&url))` starts hints and presses the label of the element you pick by its text or link. `state().hints` lists the labels on screen.
- **Painting:** `start()` and `open()` wait until the page has drawn a frame. Under load Chromium drops keys sent to a page that has loaded but not painted, so call `wait_painted()` after navigating some other way.
- **Every test is `#[ignore]`d,** so a plain `cargo test` never starts browsers. `./task e2e` runs them with `--ignored`, two at a time (`E2E_THREADS` changes that).
- **Clicks that should count as the user's go through hints** (`follow_hint`). Insert mode ignores a script's `focus()`, so pages can't switch it on, and that includes test scripts.
- **The smoke test stays** for what needs real X11 input (xdotool) and for checking the release packages. New behaviour gets an e2e test.

## Linters

`./task lint` runs pinned versions of each tool through [`scripts/tool.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/tool.sh), which downloads them into `.bin/` and checks their checksums. That way a new upstream release never adds warnings to CI without warning. To upgrade one, change its version and checksums there.

| Tool | Checks | Config |
|---|---|---|
| rustfmt, clippy | Rust formatting and lints, with warnings as errors | — |
| [ShellCheck](https://www.shellcheck.net) | `scripts/*.sh` and `task` | `# shellcheck disable=…` comments, each with its reason |
| [actionlint](https://github.com/rhysd/actionlint) | `.github/workflows/*.yml`, including ShellCheck on their `run:` blocks | — |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | Dependency licenses (each must be GPL-3.0-compatible), RustSec security advisories, yanked crates, and where crates come from | [`deny.toml`](https://github.com/joshzcold/riptide/blob/main/deny.toml) |

A dependency with a license that isn't in `deny.toml` fails the check. Add the license only after checking that it's compatible with GPL-3.0. A new security advisory fails CI until the crate is updated, or until the advisory is listed in `deny.toml`'s `ignore`, with a reason.

## Testing by hand

The [local-testing skill](https://github.com/joshzcold/riptide/blob/main/.claude/skills/local-testing/SKILL.md) describes how to drive the browser on a separate Xvfb display with a scratch `--basedir`, without touching a browser you're running yourself.

## CI

[`.github/workflows/check.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/check.yml) runs on every push to `main` and every pull request:

| Job | Runs |
|---|---|
| commit messages | `scripts/check-commits.sh` on the new commits |
| linux | `./task lint`, `./task test`, `./task smoke` (Xvfb, cached CEF download) |
| macos, windows | `cargo build`, the unit tests, and `--version`/`--paths` |
| docs | builds this book and checks its internal links and anchors with [lychee](https://github.com/lycheeverse/lychee) |

macOS and Windows are built and unit-tested but can't run the browser yet; packaging (M10) adds the app bundle and installer they need.

[`.github/workflows/docs.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/docs.yml) publishes the book to GitHub Pages on every push to `main`, and checks its external links weekly.
