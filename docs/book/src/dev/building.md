# Building

Requirements: Rust 1.88+ (edition 2024). The smoke test also needs `Xvfb` and `xdotool`.

Tasks run through [Task](https://taskfile.dev). The `./task` wrapper uses your installed `task` if there is one. Otherwise it downloads a pinned, checksum-verified release into `.bin/`. Arguments pass straight through.

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
./task                # list all tasks
```

`./task setup` reads the pinned `cef` crate version from `Cargo.lock` and fetches the matching CEF build. It skips the download when that version is already installed, and `build`, `run` and `lint` run it automatically. Set `CEF_PATH` to keep the binaries somewhere else.

The build copies `libcef.so` and Chromium's resources next to the binary. The binary finds them through an `$ORIGIN` rpath.

Logging goes to stderr and uses the `RT_LOG` filter, e.g. `RT_LOG=rt_cef=trace ./task run`. Browser data and `cef.log` live in `~/.local/share/riptide/`.

## Without Task

```sh
git clone --depth 1 --branch cef-v154.3.0+154.0.32 https://github.com/tauri-apps/cef-rs /tmp/cef-rs
(cd /tmp/cef-rs && cargo run -p export-cef-dir -- --force "$HOME/.local/share/cef")
export CEF_PATH="$HOME/.local/share/cef"
cargo build && ./target/debug/riptide
```

Without `CEF_PATH`, the `cef-dll-sys` build script downloads the binaries into `target/` instead.

## Documentation

`./task docs` builds this book into `docs/book/book/`, and `./task docs-serve` serves it at <http://localhost:3000> with live reload. Both use `scripts/mdbook.sh`, which downloads a pinned, checksum-verified mdBook into `.bin/` if the right version isn't installed. See [Writing documentation](docs.md).
