# Testing

| Command | What it runs |
|---|---|
| `./task test` | Unit tests for `rt-core`, `rt-config` and `rt-storage` (modes, keys, commands, settings, config files, paths for all three platforms, history, marks, sessions); no browser needed |
| `./task smoke` | Starts the real browser on a throwaway Xvfb display, drives it with xdotool, and checks insert mode, key consumption, scrolling and a clean `:quit` |
| `./task lint` | `cargo fmt --check` and `clippy -D warnings` |
| `./task check` | All of the above |

The smoke test uses a temporary profile, so it never touches your browsing data.

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
