# hackers-browser

Modern browser with vim-like bindings using Rust and CEF.

A keyboard-driven browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate. See [docs/PLAN.md](docs/PLAN.md) for the roadmap.

**Status:** early prototype (milestones 0, 3 and 4: core modes, tabs and hints). One window, Linux/X11 only, Chromium sandbox disabled. Not ready for daily browsing.

## Building

Requirements: Rust 1.88+ (edition 2024). The smoke test also needs `Xvfb` and `xdotool`.

Tasks run through [Task](https://taskfile.dev). The `./task` wrapper uses your installed `task` if there is one. Otherwise it downloads a pinned, checksum-verified release into `.bin/`. Arguments pass straight through.

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
./task                # list all tasks
```

`./task setup` reads the pinned `cef` crate version from `Cargo.lock` and fetches the matching CEF build. It skips the download when that version is already installed, and `build`, `run` and `lint` run it automatically. Set `CEF_PATH` to keep the binaries somewhere else.

The build copies `libcef.so` and Chromium's resources next to the binary. The binary finds them through an `$ORIGIN` rpath.

Logging goes to stderr and uses the `HB_LOG` filter, e.g. `HB_LOG=hb_cef=trace ./task run`. Browser data and `cef.log` live in `~/.local/share/hackers-browser/`.

<details>
<summary>Without Task</summary>

```sh
git clone --depth 1 --branch cef-v154.3.0+154.0.32 https://github.com/tauri-apps/cef-rs /tmp/cef-rs
(cd /tmp/cef-rs && cargo run -p export-cef-dir -- --force "$HOME/.local/share/cef")
export CEF_PATH="$HOME/.local/share/cef"
cargo build && ./target/debug/hackers-browser
```

Without `CEF_PATH`, the `cef-dll-sys` build script downloads the binaries into `target/` instead.
</details>

## Key bindings

| Keys | Command |
|---|---|
| `j` `k` `h` `l` | `scroll down/up/left/right` (accept a count, e.g. `5j`) |
| `gg` / `G` | `scroll-to-perc 0` / `scroll-to-perc` (`50G` = 50%) |
| `0` / `$` | Scroll to the far left / right |
| `Ctrl-d` `Ctrl-u` | Half page down / up |
| `Ctrl-f` `Ctrl-b` | Full page down / up |
| `H` / `L` | `back` / `forward` |
| `r` / `R` | `reload` / `reload -f` |
| `o` / `O` | `:open ` / `:open -t ` (new tab) |
| `go` / `gO` | Edit the current URL, in this tab / a new tab |
| `Ctrl-t` | `open -t` (start page in a new tab) |
| `J` `K`, `gt` `gT` | `tab-next` / `tab-prev` |
| `Alt-1`…`Alt-9`, `g0` `g$` | `tab-focus N` / first / last (a count also works, e.g. `3J`) |
| `Ctrl-Tab`, `Ctrl-^` | `tab-focus last` (previously focused tab) |
| `d`, `Ctrl-w` | `tab-close` |
| `u`, `Ctrl-Shift-t` | `undo` (reopen the last closed tab where it was) |
| `gJ` `gK`, `gm` | `tab-move +` / `-` / to the start (or to the count) |
| `co` | `tab-only` |
| `f` / `F` / `;b` | Hint elements; click / open in a new tab / open in a background tab |
| `;y` / `;h` / `;t` | Hint a link to yank / an element to hover / an input to focus |
| `;i` / `;I` | Hint an image; open it here / in a new tab |
| `;o` / `;O` | Hint a link and put `:open` (or `:open -t`) with its URL on the command line |
| `;r` | Rapid hinting: open several links in background tabs (leave with `Escape`) |
| `yy` / `yt` / `yd` | Yank the URL / title / domain |
| `pp` / `PP` | Open the clipboard contents here / in a new tab |
| `:` | Command line |
| `i` | Insert mode (also entered automatically when a text field gets focus) |
| `Ctrl-v` | Passthrough mode (leave with `Shift-Escape`) |
| `Escape` | Leave insert mode, or clear a pending key sequence |
| `ZQ` `ZZ` `Ctrl-q` | `quit` |

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored, like qutebrowser.

The command line supports readline keys (`Ctrl-a/e/u/k/w/h`, arrows), history (`Up`/`Down`), command chaining with `;;`, and completion of command names.

## Layout

| Crate | Purpose |
|---|---|
| `crates/hb-core` | Modes, key parsing, bindings, commands, command line, URL guessing. No CEF dependency; unit tested. |
| `crates/hb-cef` | CEF integration: window layout, handlers, renderer-process bindings, status bar and completion UI. |
| `crates/hb` | The `hackers-browser` binary. |

## Testing

| Command | What it runs |
|---|---|
| `./task test` | Unit tests for `hb-core` (modes, keys, commands, URLs); no browser needed |
| `./task smoke` | Starts the real browser on a throwaway Xvfb display, drives it with xdotool, and checks insert mode, key consumption, scrolling and a clean `:quit` |
| `./task lint` | `cargo fmt --check` and `clippy -D warnings` |
| `./task check` | All of the above |

The smoke test uses a temporary profile, so it never touches your browsing data.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
