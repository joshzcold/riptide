# Installing

## Release downloads

Each [GitHub release](https://github.com/joshzcold/riptide/releases) has a Linux x86_64 tarball (`riptide-X.Y.Z-linux-x86_64.tar.gz`) and an AppImage, with `SHA256SUMS`. Both contain the binary and the CEF runtime it needs. Unpack the tarball anywhere and run `riptide`, or make the AppImage executable and run it.

macOS and Windows builds compile and pass their unit tests, but can't run the browser yet; they need the app bundle and installer work that's still planned.

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

## Where things live

Run `riptide --paths` to see the config and data directories.

| Platform | Config directory | Data directory (profile, cookies, cache) |
|---|---|---|
| Linux | `$XDG_CONFIG_HOME/riptide`, default `~/.config/riptide` | `$XDG_DATA_HOME/riptide`, default `~/.local/share/riptide` |
| macOS | `~/.config/riptide` (like Neovim, WezTerm, Zed) | `~/Library/Application Support/riptide` |
| Windows | `%APPDATA%\riptide\config` | `%LOCALAPPDATA%\riptide\data` |

`XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured on every platform. `--basedir DIR` puts everything under `DIR/config` and `DIR/data`, which is handy for a separate profile.
