# Testing

| Command | What it runs |
|---|---|
| `./task test` | Unit tests for `rt-core`, `rt-config` and `rt-storage` (modes, keys, commands, settings, config files, paths for all three platforms, history, marks, sessions); no browser needed |
| `./task smoke` | Starts the real browser on a throwaway Xvfb display, drives it with xdotool, and checks insert mode, key consumption, scrolling and a clean `:quit` |
| `./task lint` | `cargo fmt --check`, `clippy -D warnings`, ShellCheck on the scripts, actionlint on the workflows, and cargo-deny (below) |
| `./task check` | All of the above |

The smoke test uses a temporary profile, so it never touches your browsing data.

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
