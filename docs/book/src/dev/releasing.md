# Releasing

1. Set `workspace.package.version` in `Cargo.toml`.
2. Run `./task changelog -- --tag vX.Y.Z` to regenerate `CHANGELOG.md` with git-cliff. `scripts/git-cliff.sh` downloads a pinned, checksum-verified git-cliff if it's not installed.
3. Commit as `chore(release): vX.Y.Z`, tag `vX.Y.Z` and push the tag.

[`.github/workflows/release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/release.yml) checks that the tag matches the version. It builds with `--release`, packs the binary and the CEF runtime into `riptide-X.Y.Z-linux-x86_64.tar.gz` and an AppImage (`./task package` and `./task appimage` do the same locally), and publishes a GitHub release with the notes for that version. In the browser, `:changelog` shows the changelog it was built with. The first start after an update says so in the status bar.

The [documentation site](https://joshzcold.github.io/riptide/) isn't tied to releases: it's rebuilt from `main` on every push.
