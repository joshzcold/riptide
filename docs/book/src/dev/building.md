# Building

Requirements: Rust 1.88+ (edition 2024; `rust-toolchain.toml` pins 1.99.1 for now, see below), `cmake` and `ninja` (`./task setup` says how to install them if they're missing). The smoke and end-to-end tests also need `Xvfb` and `xdotool`, so they only run on Linux; on other systems `./task check` skips them and CI runs them.

Tasks run through [Task](https://taskfile.dev). The `./task` wrapper uses your installed `task` if there is one. Otherwise it downloads a pinned, checksum-verified release into `.bin/`. Arguments pass straight through.

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
./task                # list all tasks
```

`./task setup` reads the pinned `cef` crate version from `Cargo.lock` and fetches the matching CEF build. It skips the download when that version is already installed, and `build`, `run` and `lint` run it automatically. Set `CEF_PATH` to keep the binaries somewhere else.

On macOS, CEF needs the browser inside an app bundle. `./task bundle` makes a release build at `dist/riptide.app`, and `./task install` copies it to `/Applications`, keeping the previous app as `riptide.app.bak`. Quit riptide before installing.

On Linux, the build copies `libcef.so` and Chromium's resources next to the binary. The binary finds them through an `$ORIGIN` rpath.

Logging goes to stderr and uses the `RT_LOG` filter, e.g. `RT_LOG=rt_cef=trace ./task run`. Browser data and `cef.log` live in `~/.local/share/riptide/`.

## Without Task

```sh
git clone --depth 1 --branch cef-v156.4.0+156.0.26 https://github.com/tauri-apps/cef-rs /tmp/cef-rs
cp vendor/download-cef/src/lib.rs /tmp/cef-rs/download-cef/src/lib.rs
(cd /tmp/cef-rs && cargo run -p export-cef-dir -- --force "$HOME/.local/share/cef")
export CEF_PATH="$HOME/.local/share/cef"
cargo build && ./target/debug/riptide
```

Without `CEF_PATH`, the `cef-dll-sys` build script downloads the binaries into `target/` instead.

Both downloads go through a patched copy of cef-rs's `download-cef` crate in `vendor/download-cef/`. CEF's build index grew past the 10 MB response limit of the crates.io release, which broke every download. `Cargo.toml` patches the copy in, and the `cp` above (which `./task setup` also does) puts it in the cef-rs checkout. [`vendor/README.md`](https://github.com/joshzcold/riptide/blob/main/vendor/README.md) says when to drop it.

`rust-toolchain.toml` pins Rust 1.99.1, which rustup installs on first use. Rust 1.100 moved build-script output from `build/<crate>-<hash>/out` to `build/<crate>/<hash>/out`. `cef-dll-sys` finds the target directory three levels above its output, so with 1.100 it copies CEF's runtime into `target/<profile>/build` instead of next to `riptide`, which then doesn't start, and `scripts/cef-dir.sh` finds no CEF. Unpin once cef-rs handles the new layout.

## Documentation

`./task docs` builds this book into `docs/book/book/`, and `./task docs-serve` serves it at <http://localhost:3000> with live reload. Both use `scripts/mdbook.sh`, which downloads a pinned, checksum-verified mdBook into `.bin/` if the right version isn't installed. See [Writing documentation](docs.md).
