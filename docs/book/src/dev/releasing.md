# Releasing

Releases are cut from GitHub Actions; nothing needs to run locally.

## Cutting a release

`main` only takes changes through pull requests, so a release is two of them.

1. **The release pull request is always open** while there's something to release. Every push to `main` that adds a `feat`, `fix` or `perf` commit since the last tag (yours or a contributor's) rebuilds `chore(release): vX.Y.Z` on the `release/next` branch from the new `main`: the version in `Cargo.toml` and `Cargo.lock`, and `CHANGELOG.md`. Its description shows the release notes.
   - git-cliff works out the version from the [Conventional Commits](contributing.md#commit-messages) since the last tag. In 0.x, a `feat` or a breaking change (`fix!:`, `feat!:`) bumps the minor version and a `fix` the patch version; 1.0 is set by hand. `docs`, `test`, `ci` and `chore` commits don't open one.
   - Each update replaces the branch, so fix the notes through `cliff.toml` on `main` (for example a `{ sha = "…", skip = true }` parser for a commit to leave out), not by editing the pull request.
2. **Merge the pull request** when you want to release. That's the release: the push to `main` builds, smoke-tests, tags and publishes it.
3. Merge the second pull request, `chore(release): update the AUR and Nix packages for vX.Y.Z`, which points the packages at the new tarball.

**Actions → release → Run workflow** on `main` does step 1 on demand:
- With **Dry run** ticked (the default), it builds and tests everything, then keeps the packages and release notes as a `release-dry-run` artifact for a day, without pushing or publishing. Do this before merging a release with packaging changes.
- With **Version** set, such as `0.3.0` or `0.3.0-rc.1` (anything with a `-` is published as a pre-release), and **Dry run** unticked, it sets the release pull request to that version. The next push to `main` goes back to the worked-out version, so merge it first.

If Actions isn't allowed to create pull requests (**Settings → Actions → General → Allow GitHub Actions to create and approve pull requests**), the run's summary has a link to open each one instead. Pull requests opened by the workflow don't trigger CI; the release build runs the smoke tests after the merge anyway.

[`.github/workflows/release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/release.yml):

| Job | Does |
|---|---|
| plan | **From Actions:** prepares the release pull request, or for a dry run, the version to build. **On every push to `main`:** releases the commit if `Cargo.toml`'s version has no tag yet (a merged release pull request). Otherwise it updates the release pull request, or closes it if nothing since the last tag would be in the changelog. |
| build | Runs [`build-release.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/build-release.yml) on the release commit (below). |
| publish | Writes `SHA256SUMS`, records [build provenance](#verifying-a-download) for every package, and creates the GitHub release, which tags the commit, with that version's section of the changelog. Then [`scripts/update-packages.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/update-packages.sh) points the AUR and Nix packages at the new release, in a pull request. Pre-releases leave the packages alone, and a dry run only prints the change. |

Because a new version in `Cargo.toml` is what triggers a release, change it only through the release pull request.

## What gets built

`build-release.yml` is shared by releases and nightlies:

- **Linux (x86_64):** `cargo build --release`, then [`scripts/package-linux.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/package-linux.sh) packs the stripped binary and CEF runtime into `riptide-<version>-linux-x86_64.tar.gz`, an AppImage and `riptide_<version>_amd64.deb`. The whole [smoke test](testing.md) then runs against the unpacked tarball, the extracted AppImage, and the `.deb` installed with apt (with its setuid sandbox), so a broken package never ships. `./task package`, `./task appimage` and `./task deb` build the same files locally.
  - The `.deb`'s `Depends:` comes from `dpkg-shlibdeps`, so it names the libraries of the distribution it's built on. CI builds on Ubuntu 24.04, which makes it suit Ubuntu 24.04+ and Debian 13+.
- **macOS and Windows (experimental):** [`scripts/package-experimental.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/package-experimental.sh) builds `riptide.app` with [`scripts/bundle-macos.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/bundle-macos.sh) and packs it in a `.dmg` (macOS, on an Apple silicon and an Intel runner), or packs the binary with CEF's runtime files (Windows, `.zip`). The CEF is `CEF_PATH`'s, or else the download in the build directory that [`scripts/cef-dir.sh`](https://github.com/joshzcold/riptide/blob/main/scripts/cef-dir.sh) finds for the version `Cargo.lock` pins, since a restored build cache can also hold older versions. The app holds the framework and five helper apps under `Contents/Frameworks`, which run `riptide-helper` (riptide's renderer code without the browser's, a fifth of the size) and is signed ad hoc (Apple silicon runs only signed code); the release build mounts the `.dmg`, verifies the signatures and runs `--version`. Notarizing it needs an Apple Developer account. On Windows it also builds an installer with Inno Setup from [`packaging/windows/riptide.iss`](https://github.com/joshzcold/riptide/blob/main/packaging/windows/riptide.iss), which the release build installs, starts with `--version` and uninstalls. Both start (CI checks it) but are untested by hand. If either fails to build, the release goes ahead without it.

## Nightly builds

[`.github/workflows/nightly.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/nightly.yml) runs at 07:00 UTC. If `main` changed since the last nightly, it builds it the same way and replaces the rolling [`nightly` pre-release](https://github.com/joshzcold/riptide/releases/tag/nightly), with files named `riptide-nightly-<date>-<commit>-…`. Debian versions must start with a digit, so the `.deb` inside says `<crate version>+nightly<date>.<commit>` (for example `0.3.0+nightly20261020.c7c2679`). That sorts after the last release and before the next one. Running it by hand builds even if nothing changed. It never becomes the "latest" release.

## Verifying a download

Every published package has a `SHA256SUMS` entry and a signed [build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations) that ties it to the workflow run and commit that built it:

```sh
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify riptide-0.2.0-linux-x86_64.tar.gz --repo joshzcold/riptide
```

## After a release

**Packages:**
- The AUR `PKGBUILD`s (`packaging/aur/<package>/`) and the Nix package (`packaging/nix/package.nix`, used by the root `flake.nix`) are updated by the publish job's pull request. If that step fails, run `scripts/update-packages.sh X.Y.Z riptide-X.Y.Z-linux-x86_64.tar.gz` with the released tarball and open a pull request with the result.
- Merging that pull request publishes both to the AUR ([`aur.yml`](https://github.com/joshzcold/riptide/blob/main/.github/workflows/aur.yml)): [`riptide-browser-bin`](https://aur.archlinux.org/packages/riptide-browser-bin), from the release tarball, and [`riptide-browser`](https://aur.archlinux.org/packages/riptide-browser), built from the release tag against the CEF build its `Cargo.lock` pins (`update-packages.sh` looks up that build's checksum in the CEF index). It checks the sources' checksums, builds `riptide-browser` (a package that doesn't build isn't published), writes `.SRCINFO` and pushes over SSH with the `RIPTIDE_AUR_SSH_PRIVATE_KEY` secret, whose public half (`RIPTIDE_AUR_SSH_PUBLIC_KEY`) is on the maintainer's AUR account. Run it by hand (a dry run by default) to check the key or to publish again.
- To check the Nix packages, run `nix build .#riptide` (the release tarball) or `nix build .#riptide-source` (built from the checkout), then `BIN=result/bin/riptide scripts/smoke-test.sh`. `riptide-source` needs no update at a release; it reads the version and dependencies from the checkout.

**In the browser:**

In the browser, `:changelog` shows the changelog it was built with, and the first start after an update says so in the status bar (and opens it, as `changelog_after_upgrade` decides). The [documentation site](https://joshzcold.github.io/riptide/) isn't tied to releases: it's rebuilt from `main` on every push.
