# Releasing

Releases are cut from GitHub Actions; nothing needs to run locally.

## Cutting a release

1. Open **Actions → release → Run workflow** on `main`.
2. Leave **Version** empty to let git-cliff work out the next version from the [Conventional Commits](contributing.md#commit-messages) since the last tag. In 0.x, a `feat` bumps the minor version and anything else the patch version. Or type one, such as `0.3.0` or `0.3.0-rc.1` (anything with a `-` is published as a pre-release).
3. Run it with **Dry run** ticked first (the default). It builds and tests everything, then keeps the packages and release notes as a `release-dry-run` artifact for a day, without pushing or publishing.
4. Run it again with **Dry run** unticked to publish.

[`.github/workflows/release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/release.yml) then:

| Job | Does |
|---|---|
| prepare | Sets `workspace.package.version` in `Cargo.toml`, updates `Cargo.lock`, regenerates `CHANGELOG.md`, commits `chore(release): vX.Y.Z` to `main` and tags it `vX.Y.Z`. The commit and tag are pushed together, so if `main` moved on in the meantime, neither lands and you can run it again. Real releases only run on `main`. |
| build | Runs [`build-release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/build-release.yml) on the tag (below). |
| publish | Writes `SHA256SUMS`, records [build provenance](#verifying-a-download) for every package, and creates the GitHub release with that version's section of the changelog. |

Pushing a `vX.Y.Z` tag by hand still works, as long as `Cargo.toml` already has that version; it skips the prepare step.

## What gets built

`build-release.yml` is shared by releases and nightlies:

- **Linux (x86_64):** `cargo build --release`, then [`scripts/package-linux.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/package-linux.sh) packs the stripped binary and CEF runtime into `riptide-<version>-linux-x86_64.tar.gz` and an AppImage. The whole [smoke test](testing.md) then runs against the unpacked tarball and against the extracted AppImage, so a broken package never ships. `./task package` and `./task appimage` build the same files locally.
- **macOS and Windows (experimental):** [`scripts/package-experimental.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/package-experimental.sh) packs the binary with the CEF framework (macOS, `.tar.gz`) or runtime files (Windows, `.zip`). Neither runs the browser yet: they need the app bundle and installer work in M10. If either fails to build, the release goes ahead without it.

## Nightly builds

[`.github/workflows/nightly.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/nightly.yml) runs at 07:00 UTC. If `main` changed since the last nightly, it builds it the same way and replaces the rolling [`nightly` pre-release](https://github.com/joshzcold/riptide/releases/tag/nightly), with files named `riptide-nightly-<date>-<commit>-…`. Running it by hand builds even if nothing changed. It never becomes the "latest" release.

## Verifying a download

Every published package has a `SHA256SUMS` entry and a signed [build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations) that ties it to the workflow run and commit that built it:

```sh
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify riptide-0.2.0-linux-x86_64.tar.gz --repo joshzcold/riptide
```

## After a release

**Packages:**
- The AUR `PKGBUILD` (`packaging/aur/`) and the Nix package (`packaging/nix/package.nix`, used by the root `flake.nix`) download the release tarball by version. After a release, update their version and checksum:
  - `sha256sums` is the tarball's line in the release's `SHA256SUMS`.
  - The Nix `hash` is the same hash in SRI form: `nix hash convert --hash-algo sha256 --to sri <hex>`.
  - Run `nix build .#riptide`, then `BIN=result/bin/riptide scripts/smoke-test.sh`.

**In the browser:**

In the browser, `:changelog` shows the changelog it was built with, and the first start after an update says so in the status bar (and opens it, as `changelog_after_upgrade` decides). The [documentation site](https://joshzcold.github.io/riptide/) isn't tied to releases: it's rebuilt from `main` on every push.
