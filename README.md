# hackers-browser

Modern browser with vim-like bindings using Rust and CEF.

A keyboard-driven browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate. See [docs/PLAN.md](docs/PLAN.md) for the roadmap.

**Status:** early prototype (milestones 0 and 3–7: core modes, tabs, hints, config, storage, prompts and downloads). One window, Linux/X11 only, Chromium sandbox disabled. Not ready for daily browsing.

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

## Configuration

Run `hackers-browser --paths` to see where config and data live. All config files are optional and load in this order (later wins):

| File | Purpose |
|---|---|
| `autoconfig.toml` | Written by `:set`, `:bind` and `:unbind`. Don't edit it by hand. |
| `config.toml` | Declarative settings and bindings. See [docs/config.example.toml](docs/config.example.toml). |
| `config.lua` | The same, as a Lua 5.4 program. See [docs/config.example.lua](docs/config.example.lua). |

| Platform | Config directory | Data directory (profile, cookies, cache) |
|---|---|---|
| Linux | `$XDG_CONFIG_HOME/hackers-browser`, default `~/.config/hackers-browser` | `$XDG_DATA_HOME/hackers-browser`, default `~/.local/share/hackers-browser` |
| macOS | `~/.config/hackers-browser` (like Neovim, WezTerm, Zed) | `~/Library/Application Support/hackers-browser` |
| Windows | `%APPDATA%\hackers-browser\config` | `%LOCALAPPDATA%\hackers-browser\data` |

`XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured on every platform. `--basedir DIR` puts everything under `DIR/config` and `DIR/data`, which is handy for testing or for a separate profile.

Every setting is listed in [docs/settings.md](docs/settings.md), and the completion popup lists them as you type `:set `. In the browser:

- `:set hints.chars asdf` changes a setting; `:set hints.uppercase!` toggles one; `:set hints.chars` shows the value.
- `:bind <Ctrl-x> tab-close` adds a binding (`--mode insert` for other modes); `:bind <Ctrl-x>` shows one; `:unbind d` removes one.
- `:config-source` reloads every file. Errors show in the status bar with `file:line`, and the rest of the file still applies.

### Browsing data

| What | Where | Format |
|---|---|---|
| History | `<data>/history.sqlite` | SQLite; `:history-clear --force` empties it |
| Quickmarks | `<config>/quickmarks` | qutebrowser's: one `name url` per line |
| Bookmarks | `<config>/bookmarks/urls` | qutebrowser's: one `url title` per line |
| Sessions | `<data>/sessions/<name>.toml` | TOML |

Quickmarks and bookmarks use qutebrowser's formats and sit next to the config, so you can keep them in dotfiles or copy yours from `~/.config/qutebrowser/`. Sessions keep each tab's current page; CEF cannot restore a tab's back/forward history.

### Prompts, downloads and permissions

Everything that needs an answer appears above the status bar, one at a time:

- JavaScript `alert`, `confirm`, `prompt` and leave-page warnings
- HTTP logins (username, then a hidden password)
- where to save a download
- site permission requests (camera, microphone, location, notifications…)

| Mode | Keys |
|---|---|
| prompt (text) | type, readline keys (`Ctrl-w` deletes one path component), `Return` accepts, `Escape` cancels |
| yesno | `y` / `n`, `Return` (the default), `Escape` cancels |

For permission prompts, `y` allows and `N` blocks. Chromium saves both per site; `n` and `Escape` mean "not now". Camera and microphone requests use `A`/`N` to remember the answer for the session. The `content.geolocation`, `content.notifications.enabled`, `content.media.audio_capture`, `content.media.video_capture` and `content.desktop_capture` settings (`ask`, `true` or `false`) answer without asking.

Downloads go to `downloads.location.directory`, or the system Downloads folder if that's empty (on Linux, `XDG_DOWNLOAD_DIR` or `~/.config/user-dirs.dirs`). Server-suggested names are reduced to a plain file name, existing files get ` (1)` appended, and typing an existing path asks before overwriting. Set `downloads.location.prompt = false` to skip the question. The status bar shows `↓2 41%` while downloads run.

| Command | |
|---|---|
| `:download [url]` | Download a URL, or the current page |
| `;d` | Hint a link to download |
| `:download-cancel`, `:download-open` | The newest running / finished download, or the one given as a count (`2:download-open`) |
| `:download-clear` | Forget finished downloads |

`:download-open` uses the system's opener (`xdg-open`, `open` or `start`).

### Lua

`config.lua` gets `c` (qutebrowser-style `c.hints.chars = "asdf"`), `hb.set/get/bind/unbind`, `hb.platform` (`linux`, `macos`, `windows`), `hb.config_dir`, and `require()` from the config directory (`name.lua` or `lua/name.lua`). It is a normal Lua with the standard library, trusted like a shell rc file.

For completion and type checking in Neovim, VS Code and other editors using lua-language-server:

```sh
dir="$(hackers-browser --paths | sed -n 's/^config: //p')"
mkdir -p "$dir" && hackers-browser --lua-types > "$dir/hb.meta.lua"
```

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
| `m` | Quickmark this page (type a name, then `Return`) |
| `b` / `B` | Open a quickmark here / in a new tab |
| `M` | Bookmark this page |
| `gb` / `gB` | Open a bookmark here / in a new tab |
| `ZZ`, `:wq` | Save the tabs as the `default` session and quit (`ZQ` quits without saving) |
| `:` | Command line |
| `i` | Insert mode (also entered automatically when a text field gets focus) |
| `Ctrl-v` | Passthrough mode (leave with `Shift-Escape`) |
| `Escape` | Leave insert mode, or clear a pending key sequence |
| `ZQ` `ZZ` `Ctrl-q` | `quit` |

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored, like qutebrowser.

In the command line, `Tab` / `Shift-Tab` cycle through completions. `:open` completes from quickmarks, bookmarks and history (every typed word must match, in any order). `:set`, `:quickmark-load`, `:bookmark-load` and `:session-load` complete their own names. `:session-save [name]`, `:session-load name` and `:session-delete name` manage sessions. With `auto_save.session = true`, the tabs are saved on quit and restored at the next start.

The command line supports readline keys (`Ctrl-a/e/u/k/w/h`, arrows), history (`Up`/`Down`), command chaining with `;;`, and completion of command names.

## Layout

| Crate | Purpose |
|---|---|
| `crates/hb-core` | Modes, key parsing, bindings, commands, command line, URL guessing. No CEF dependency; unit tested. |
| `crates/hb-config` | Config paths per platform, command line, TOML/Lua/autoconfig loading, generated Lua types and settings docs. |
| `crates/hb-storage` | History (SQLite), quickmarks and bookmarks (qutebrowser formats), sessions (TOML). |
| `crates/hb-cef` | CEF integration: window layout, handlers, renderer-process bindings, status bar and completion UI. |
| `crates/hb` | The `hackers-browser` binary. |

## Testing

| Command | What it runs |
|---|---|
| `./task test` | Unit tests for `hb-core`, `hb-config` and `hb-storage` (modes, keys, commands, settings, config files, paths for all three platforms, history, marks, sessions); no browser needed |
| `./task smoke` | Starts the real browser on a throwaway Xvfb display, drives it with xdotool, and checks insert mode, key consumption, scrolling and a clean `:quit` |
| `./task lint` | `cargo fmt --check` and `clippy -D warnings` |
| `./task check` | All of the above |

The smoke test uses a temporary profile, so it never touches your browsing data.

### CI

[`.github/workflows/check.yml`](.github/workflows/check.yml) runs on every push to `main` and every pull request:

| Job | Runs |
|---|---|
| commit messages | `scripts/check-commits.sh` on the new commits |
| linux | `./task lint`, `./task test`, `./task smoke` (Xvfb, cached CEF download) |
| macos, windows | `cargo build`, the unit tests, and `--version`/`--paths` |

macOS and Windows are built and unit-tested but can't run the browser yet; packaging (M10) adds the app bundle and installer they need.

### Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org), which the changelog will be generated from: `feat(tabs): add pinned tabs`, `fix: …`, `docs: …`, `ci: …`. Run `./task hooks` once to check messages locally before CI does.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
