# Vendored crates

- `download-cef/`: [download-cef](https://crates.io/crates/download-cef) 3.0.0 from [cef-rs](https://github.com/tauri-apps/cef-rs) (Apache-2.0 OR MIT), with a 64 MB limit on CEF's `index.json` instead of ureq's default 10 MB, which the index outgrew in November 2026. `Cargo.toml` patches it in for the `cef-dll-sys` build script, and `./task setup` copies `src/lib.rs` into its `cef-rs` checkout. Drop it once a cef-rs release raises the limit; a `cef` update that moves to another `download-cef` version will fail `./task setup` until then.
