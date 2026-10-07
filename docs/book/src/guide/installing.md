# Installing

## Release downloads

Each [GitHub release](https://github.com/joshzcold/riptide/releases) has three Linux x86_64 downloads, with `SHA256SUMS`. Each contains the binary and the CEF runtime it needs.

| Download | Install |
|---|---|
| `riptide_X.Y.Z_amd64.deb` | Ubuntu 24.04+ and Debian 13+: `sudo apt install ./riptide_X.Y.Z_amd64.deb`. It installs to `/opt/riptide` with `riptide` on your `PATH`, a desktop entry, and apt-managed dependencies. Its setuid sandbox helper works even where user namespaces are blocked. |
| `riptide-X.Y.Z-linux-x86_64.AppImage` | Any distribution: `chmod +x` it and run it. |
| `riptide-X.Y.Z-linux-x86_64.tar.gz` | Unpack it anywhere and run `riptide`. |

To check a download, run `sha256sum -c SHA256SUMS --ignore-missing`, or verify where it was built with `gh attestation verify <file> --repo joshzcold/riptide`.

The [`nightly` pre-release](https://github.com/joshzcold/riptide/releases/tag/nightly) is rebuilt from `main` every night it changes. It has the newest features, and may be broken.

macOS and Windows builds compile and pass their unit tests, but can't run the browser yet; they need the app bundle and installer work that's still planned.

## Nix and Arch Linux

Both install the latest release's Linux tarball.

- **Nix** (flakes): `nix run github:joshzcold/riptide`, or add `github:joshzcold/riptide` as a flake input and use its `packages.x86_64-linux.default`. The package ([`packaging/nix/package.nix`](https://github.com/joshzcold/riptide/blob/main/packaging/nix/package.nix)) patches the binaries for NixOS. The Nix store can't hold a setuid sandbox helper, so it relies on user namespaces for the sandbox, which NixOS allows.
- **Arch Linux:** [`packaging/aur/PKGBUILD`](https://github.com/joshzcold/riptide/blob/main/packaging/aur/PKGBUILD) builds `riptide-bin`. It installs to `/opt/riptide` with `riptide` on your `PATH`, plus a desktop entry and icon. Run `makepkg -si` in that directory. It isn't published to the AUR yet.

To package a tarball you built yourself with Nix:

```sh
./task package
nix-build -E 'with import <nixpkgs> {}; callPackage ./packaging/nix/package.nix {
  tarball = ./dist/riptide-0.1.0-linux-x86_64.tar.gz; }'
```

## Building from source

Requirements: Rust 1.88+ (edition 2024).

Tasks run through [Task](https://taskfile.dev). The `./task` wrapper uses your installed `task` if there is one. Otherwise it downloads a pinned, checksum-verified release into `.bin/`.

```sh
./task setup          # once: download CEF (~1.5 GB) into $CEF_PATH, default ~/.local/share/cef
./task run            # build and launch; or: ./task run -- example.com
```

The [developer guide](../dev/building.md) covers the build in more detail, including building without Task.

## The sandbox

On Linux, Chromium's sandbox needs unprivileged user namespaces or a setuid-root `chrome-sandbox` next to the binary. Riptide checks at startup. If neither is available, it runs without the sandbox and logs a warning. The help page (`:help`) shows the result under "Sandbox". `--no-sandbox` turns it off on purpose.

Ubuntu 23.10 and later block user namespaces through AppArmor unless a program has a profile that allows them. Pick one of these fixes:

- **An AppArmor profile (recommended).** It only affects this binary. Save it as `/etc/apparmor.d/riptide`, then load it with `sudo apparmor_parser -r /etc/apparmor.d/riptide`:
  ```
  abi <abi/4.0>,
  include <tunables/global>

  profile riptide /path/to/riptide/target/*/riptide flags=(unconfined) {
    userns,
    include if exists <local/riptide>
  }
  ```
- **Setuid helper:** `sudo chown root:root target/debug/chrome-sandbox && sudo chmod 4755 target/debug/chrome-sandbox`. A rebuild that copies the file again undoes this.
- **System-wide:** `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0`. It lowers the hardening for every program.

Inside an AppImage, `chrome-sandbox` can't be setuid, so the sandbox needs user namespaces.

macOS and Windows builds run without the sandbox for now; it needs the app bundle and installer work that's still planned.

## Window managers

riptide's windows have the class `riptide` (`WM_CLASS` `riptide`, `Riptide`) for window manager rules and docks, and riptide's logo as their icon. They can be resized, so tiling window managers tile them, call windows included. Before 2026-10-07 they asked for a fixed 1280×800, which made dwm and similar window managers float them.

If another browser's screen-share picker doesn't list riptide's window, check its state with `xprop WM_STATE` and click the window. Programs that list windows skip any that aren't in the `Normal` state. dwm's swallow patch leaves a program started from a terminal in the `Withdrawn` state, because it marks the program's window instead of the terminal's. Either fix the patch (in `swallow()`, mark `p` instead of `c` as withdrawn), or exclude riptide from swallowing with a rule for the class `Riptide`.

## Where things live

Run `riptide --paths` to see the config and data directories.

| Platform | Config directory | Data directory (profile, cookies, cache) |
|---|---|---|
| Linux | `$XDG_CONFIG_HOME/riptide`, default `~/.config/riptide` | `$XDG_DATA_HOME/riptide`, default `~/.local/share/riptide` |
| macOS | `~/.config/riptide` (like Neovim, WezTerm, Zed) | `~/Library/Application Support/riptide` |
| Windows | `%APPDATA%\riptide\config` | `%LOCALAPPDATA%\riptide\data` |

`XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured on every platform. `--basedir DIR` puts everything under `DIR/config` and `DIR/data`, which is handy for a separate profile.
