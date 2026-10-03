# hackers-browser

Modern browser with vim-like bindings using Rust and CEF.

A keyboard-driven browser in the spirit of [qutebrowser](https://github.com/qutebrowser/qutebrowser), built on [CEF](https://github.com/chromiumembedded/cef) (Chromium 154) through the [`cef`](https://github.com/tauri-apps/cef-rs) crate. See [docs/PLAN.md](docs/PLAN.md) for the roadmap.

**Status:** early prototype (milestones 0 and 3–7: core modes, tabs, hints, config, storage, prompts and downloads). One window, Linux/X11 only. Not ready for daily browsing.

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

### Sandbox

On Linux, Chromium's sandbox needs unprivileged user namespaces or a setuid-root `chrome-sandbox` next to the binary. The browser checks at startup. If neither is available, it runs without the sandbox and logs a warning. The help page (`:help`) shows the result under "Sandbox". `--no-sandbox` turns it off on purpose.

Ubuntu 23.10 and later block user namespaces through AppArmor unless a program has a profile that allows them. Pick one of these fixes:

- **An AppArmor profile (recommended).** It only affects this binary. Save it as `/etc/apparmor.d/hackers-browser`, then load it with `sudo apparmor_parser -r /etc/apparmor.d/hackers-browser`:
  ```
  abi <abi/4.0>,
  include <tunables/global>

  profile hackers-browser /path/to/hackers-browser/target/*/hackers-browser flags=(unconfined) {
    userns,
    include if exists <local/hackers-browser>
  }
  ```
- **Setuid helper:** `sudo chown root:root target/debug/chrome-sandbox && sudo chmod 4755 target/debug/chrome-sandbox`. A rebuild that copies the file again undoes this.
- **System-wide:** `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0` (CI does this). It lowers the hardening for every program.

macOS and Windows builds run without the sandbox for now; it needs the app bundle and installer work in M10.

### Network traffic

Chromium calls Google in the background. hackers-browser turns off the calls that only serve Google and keeps the security updates (`crates/hb-cef/src/privacy.rs`). Measured on a fresh profile left on `about:blank` for 90 seconds, with `--log-net-log`:

| Request | Purpose | Status |
|---|---|---|
| `update.googleapis.com`, `edgedl.me.gvt1.com` | Component updates | Only the components Chromium marks as security data still update: certificate revocation lists (CRLSets) and the subresource filter rules. The ~20 others no longer download, saving ~115 MB per profile. These include Widevine, optimization hints, the on-device suggest model, TTS and the password-strength data. |
| `clients2.google.com/time` | Secure network time, used to explain certificate date errors | kept |
| `redirector.gvt1.com/…/dict` | Spell-check dictionary | only once per language in `spellcheck.languages` (empty by default) |
| `www.google.com/async/folae` | AI Mode eligibility | off (`--disable-features=AimEnabled`) |
| `www.google.com` preconnects | Default search engine warm-up | off (Chrome's default search engine is disabled; hackers-browser has its own `url.searchengines`) |
| `accounts.google.com/ListAccounts` | Google accounts in the cookie jar | **still sent** once at startup. Google sign-in is off, but something still asks for the cookie jar; it carries your google.com cookies if you have any. |

The preferences are written into the profile (`Local State`, `Default/Preferences`) before Chromium starts, since most of these services start within 100 ms. To check for yourself: `hackers-browser --basedir /tmp/t --log-net-log=/tmp/net.json about:blank`, then `grep -o '"url":"[^"]*' /tmp/net.json | sort -u`.

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

### Content blocking

Ads and trackers are blocked at the network level with Adblock Plus filter lists, using Brave's [adblock-rust](https://github.com/brave/adblock-rust). Run `:adblock-update` once to download the lists in `content.blocking.adblock.lists` (EasyList and EasyPrivacy by default; `file://` lists work too). The compiled engine is cached in the data directory and loads in the background at startup.

- `content.blocking.enabled` turns blocking on or off.
- `content.blocking.whitelist` lists hosts where nothing is blocked (subdomains included).
- Top-level pages are never blocked, so a bad rule can't make a site unreachable. Cosmetic (element-hiding) rules aren't applied yet.

### External programs and userscripts

- `:spawn [-v] [-m] [-d] <cmd> [args]` runs a program, with arguments split like a shell would (no shell runs). `{url}` is the current page. `-v` reports success too, `-m` shows the program's output, and `-d` detaches. A non-zero exit is shown as an error. For example, `hb.bind(",m", "spawn -d mpv {url}")`.
- `:spawn -u <name>` runs a **userscript**, compatible with qutebrowser's. It is looked up in `<config>/userscripts/`, then `<data>/userscripts/`, then `PATH`. It gets `QUTE_URL`, `QUTE_TITLE`, `QUTE_SELECTED_TEXT`, `QUTE_HTML`/`QUTE_TEXT` (files with the page's HTML and text), `QUTE_TAB_INDEX`, `QUTE_COUNT`, `QUTE_MODE`, `QUTE_CONFIG_DIR`, `QUTE_DATA_DIR`, `QUTE_DOWNLOAD_DIR` and `QUTE_VERSION`. Commands it writes to `QUTE_FIFO`, one per line, run when it exits.
- `:open-editor`, or `Ctrl-e` in insert mode, edits the focused text field in `editor.command` (default `gvim -f {file} -c "normal {line}G{column0}l"`, as in qutebrowser). The text is written back when the editor exits successfully. For a terminal editor: `c.editor.command = { "foot", "nvim", "+call cursor({line}, {column})", "{file}" }`.

### Spell checking

Off by default. Turn it on with a list of languages, e.g. `c.spellcheck.languages = { "en-US", "de-DE" }` in `config.lua` or `:set spellcheck.languages '["en-US"]'`. Chromium downloads each dictionary once from Google (`redirector.gvt1.com`) and underlines mistakes as you type.

From the keyboard, in a text field:
- `:spell-suggest` lists fixes for the word at the text cursor as completions. `Tab` picks one, `Return` replaces the word, and you're back in insert mode.
- `:spell-add` adds that word to your dictionary.

Nothing is bound by default. For example, `hb.bind("<Ctrl-s>", "spell-suggest", "insert")`. Right-click suggestions work too.

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

### From the terminal

While the browser is running, `hackers-browser` hands its arguments to that instance (per profile, so `--basedir` instances stay separate) and exits:

```sh
hackers-browser https://example.com        # opens per new_instance_open_target (default: new tab)
hackers-browser --target tab-bg notes.html # relative files become file:// URLs
hackers-browser ':tab-focus 1' ':reload'   # arguments starting with ':' run as commands
```

The browser listens on a Unix socket in `$XDG_RUNTIME_DIR/hackers-browser/` (or the data directory), inside a `0700` directory and with `0600` permissions, so only your user can send commands. On Windows each start is a new instance for now.

### Internal pages

The tab bar, status bar and overlay are HTML pages served from the browser itself at `hb://ui/…`. Web pages can't link to, frame or redirect to `hb://` addresses, and only `hb://ui/` pages get the `hb.send()` channel to Rust. The browser accepts only the messages each page is allowed to send.

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
| `co` | `tab-only` (keeps pinned tabs; `:tab-only --force` closes them too) |
| `Ctrl-p` | `tab-pin`: pin or unpin the tab (a count picks one, e.g. `3 Ctrl-p`) |
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
| `` `a `` / `'a` | Set / jump to mark `a`: `a`–`z` remember this page's scroll position, `A`–`Z` also the page itself; `''` returns to where the last jump started |
| `Ctrl-e` (insert mode) | `open-editor`: edit the text field in `editor.command` |
| `F1`, `:help [topic]` | Help: every command, setting (with its current value and where it was set) and key binding, generated from the running browser. `:help :open`, `:help hints.chars`, `:help bindings` jump to an entry; `/` searches. |
| `:version` | Version, git commit, CEF/Chromium versions, paths and loaded config files |
| `ZZ`, `:wq` | Save the tabs as the `default` session and quit (`ZQ` quits without saving) |
| `:` | Command line |
| `i` | Insert mode (also entered automatically when a text field gets focus) |
| `Ctrl-v` | Passthrough mode (leave with `Shift-Escape`) |
| `Escape` | Leave insert mode, or clear a pending key sequence |
| `ZQ` `ZZ` `Ctrl-q` | `quit` |

Pinned tabs stay at the left, shrink to their icon and number (`tabs.pinned.shrink`), and survive `d` and `co` unless you add `--force`. With `tabs.pinned.frozen` (the default), `:open` in a pinned tab opens a new tab instead. Sessions remember which tabs are pinned.

The tab bar shows site icons (`tabs.favicons.show`: `always`, `never` or `pinned`) and works with the mouse: click to select, middle-click to close, scroll to switch (`tabs.mousewheel_switching`), and drag to reorder.

Links that open new windows (`target=_blank`, `window.open`) open as tabs next to the current one, keeping `window.opener`. Closing the last tab is ignored, like qutebrowser.

In the command line, `Tab` / `Shift-Tab` cycle through completions. `:open` completes from quickmarks, bookmarks and history (every typed word must match, in any order). `:set`, `:quickmark-load`, `:bookmark-load` and `:session-load` complete their own names. `:session-save [name]`, `:session-load name` and `:session-delete name` manage sessions. With `auto_save.session = true`, the tabs are saved on quit and restored at the next start.

The command line supports readline keys (`Ctrl-a/e/u/k/w/h`, arrows), history (`Up`/`Down`), command chaining with `;;`, and completion of command names.

## Layout

| Crate | Purpose |
|---|---|
| `crates/hb-core` | Modes, key parsing, bindings, commands, command line, URL guessing. No CEF dependency; unit tested. |
| `crates/hb-config` | Config paths per platform, command line, TOML/Lua/autoconfig loading, the single-instance socket protocol, generated Lua types and settings docs. |
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

### Testing by hand

The [local-testing skill](.claude/skills/local-testing/SKILL.md) describes how to drive the browser on a separate Xvfb display with a scratch `--basedir`, without touching a browser you're running yourself.

### CI

[`.github/workflows/check.yml`](.github/workflows/check.yml) runs on every push to `main` and every pull request:

| Job | Runs |
|---|---|
| commit messages | `scripts/check-commits.sh` on the new commits |
| linux | `./task lint`, `./task test`, `./task smoke` (Xvfb, cached CEF download) |
| macos, windows | `cargo build`, the unit tests, and `--version`/`--paths` |

macOS and Windows are built and unit-tested but can't run the browser yet; packaging (M10) adds the app bundle and installer they need.

### Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org), which [`CHANGELOG.md`](CHANGELOG.md) is generated from (`feat` and `fix` appear in it; `docs`, `test`, `ci` and `chore` don't): `feat(tabs): add pinned tabs`, `fix: …`, `docs: …`, `ci: …`. Run `./task hooks` once to check messages locally before CI does.

### Releases

1. Set `workspace.package.version` in `Cargo.toml`.
2. Run `./task changelog -- --tag vX.Y.Z` to regenerate `CHANGELOG.md` with git-cliff. `scripts/git-cliff.sh` downloads a pinned, checksum-verified git-cliff if it's not installed.
3. Commit as `chore(release): vX.Y.Z`, tag `vX.Y.Z` and push the tag.

[`.github/workflows/release.yml`](.github/workflows/release.yml) checks that the tag matches the version. It builds with `--release`, packs the binary and the CEF runtime into `hackers-browser-X.Y.Z-linux-x86_64.tar.gz` (`./task package` does the same locally), and publishes a GitHub release with the notes for that version. In the browser, `:changelog` shows the changelog it was built with. The first start after an update says so in the status bar.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
