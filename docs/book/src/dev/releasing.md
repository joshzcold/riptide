# Releasing

Releases are cut from GitHub Actions; nothing needs to run locally.

## Cutting a release

`main` only takes changes through pull requests, so a release is two of them.

1. Open **Actions → release → Run workflow** on `main`.
2. Leave **Version** empty to let git-cliff work out the next version from the [Conventional Commits](contributing.md#commit-messages) since the last tag. In 0.x, a `feat` bumps the minor version and anything else the patch version. Or type one, such as `0.3.0` or `0.3.0-rc.1` (anything with a `-` is published as a pre-release).
3. Run it with **Dry run** ticked first (the default). It builds and tests everything, then keeps the packages and release notes as a `release-dry-run` artifact for a day, without pushing or publishing.
4. Run it again with **Dry run** unticked. It commits `chore(release): vX.Y.Z` (the version in `Cargo.toml` and `Cargo.lock`, and `CHANGELOG.md`) to a `release/vX.Y.Z` branch and opens a pull request.
5. **Merge the pull request.** That's the release: the push to `main` builds, smoke-tests, tags and publishes it.
6. Merge the second pull request, `chore(release): update the AUR and Nix packages for vX.Y.Z`, which points the packages at the new tarball.

If Actions isn't allowed to create pull requests (**Settings → Actions → General → Allow GitHub Actions to create and approve pull requests**), the run's summary has a link to open each one instead. Pull requests opened by the workflow don't trigger CI; the release build runs the smoke tests after the merge anyway.

[`.github/workflows/release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/release.yml):

| Job | Does |
|---|---|
| plan | **From Actions:** prepares the release pull request, or for a dry run, the version to build. **On every push to `main`:** releases the commit if `Cargo.toml`'s version has no tag yet (a merged release pull request), and otherwise does nothing. |
| build | Runs [`build-release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/build-release.yml) on the release commit (below). |
| publish | Writes `SHA256SUMS`, records [build provenance](#verifying-a-download) for every package, and creates the GitHub release, which tags the commit, with that version's section of the changelog. Then [`scripts/update-packages.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/update-packages.sh) points the AUR and Nix packages at the new tarball, in a pull request. Pre-releases leave the packages alone, and a dry run only prints the change. |

Because a new version in `Cargo.toml` is what triggers a release, change it only through the release pull request.

## What gets built

`build-release.yml` is shared by releases and nightlies:

- **Linux (x86_64):** `cargo build --release`, then [`scripts/package-linux.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/package-linux.sh) packs the stripped binary and CEF runtime into `riptide-<version>-linux-x86_64.tar.gz`, an AppImage and `riptide_<version>_amd64.deb`. The whole [smoke test](testing.md) then runs against the unpacked tarball, the extracted AppImage, and the `.deb` installed with apt (with its setuid sandbox), so a broken package never ships. `./task package`, `./task appimage` and `./task deb` build the same files locally.
  - The `.deb`'s `Depends:` comes from `dpkg-shlibdeps`, so it names the libraries of the distribution it's built on. CI builds on Ubuntu 24.04, which makes it suit Ubuntu 24.04+ and Debian 13+.
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
- The AUR `PKGBUILD` (`packaging/aur/`) and the Nix package (`packaging/nix/package.nix`, used by the root `flake.nix`) are updated by the publish job's pull request. If that step fails, run `scripts/update-packages.sh X.Y.Z riptide-X.Y.Z-linux-x86_64.tar.gz` with the released tarball and open a pull request with the result.
- To check the Nix package, run `nix build .#riptide`, then `BIN=result/bin/riptide scripts/smoke-test.sh`.

**In the browser:**

In the browser, `:changelog` shows the changelog it was built with, and the first start after an update says so in the status bar (and opens it, as `changelog_after_upgrade` decides). The [documentation site](https://joshzcold.github.io/riptide/) isn't tied to releases: it's rebuilt from `main` on every push.
