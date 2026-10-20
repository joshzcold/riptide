#!/usr/bin/env bash
# Points the AUR and Nix packages at a release: its version, the Linux
# tarball's checksum, and for the AUR source package the CEF build to fetch.
#   scripts/update-packages.sh <version> <riptide-<version>-linux-x86_64.tar.gz>
# release.yml runs it after publishing and commits the result.
set -euo pipefail

version=${1:?usage: scripts/update-packages.sh <version> <tarball>}
tarball=${2:?usage: scripts/update-packages.sh <version> <tarball>}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "update-packages.sh: '$version' isn't a release version like 0.2.0" >&2; exit 1; }
[[ -f $tarball ]] || { echo "update-packages.sh: no $tarball" >&2; exit 1; }

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
pkgbuild=$root/packaging/aur/riptide-browser-bin/PKGBUILD
source_pkgbuild=$root/packaging/aur/riptide-browser/PKGBUILD
nix=$root/packaging/nix/package.nix

hex=$(sha256sum "$tarball" | cut -d' ' -f1)
# Nix wants the same hash as base64 (SRI).
sri="sha256-$(python3 -c 'import base64, sys; print(base64.b64encode(bytes.fromhex(sys.argv[1])).decode())' "$hex")"

sed -i -E \
    -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$hex')/" \
    "$pkgbuild"
# The CEF build the release's cef crate pins (154.4.0+154.0.33 is CEF 154.0.33),
# and its checksum from the CEF index.
lock=$(git -C "$root" show "v$version:Cargo.lock" 2>/dev/null || cat "$root/Cargo.lock")
cef_version=$(awk '/^name = "cef"$/ { getline; gsub(/version = |"/, ""); print; exit }' <<<"$lock" | cut -d+ -f2)
read -r cef sha1 < <(curl -fsSL https://cef-builds.spotifycdn.com/index.json | python3 -c '
import json, sys
want = sys.argv[1] + "+"
for v in json.load(sys.stdin)["linux64"]["versions"]:
    if v["cef_version"].startswith(want):
        f = next(f for f in v["files"] if f["type"] == "minimal")
        print(v["cef_version"], f["sha1"])
        break
' "$cef_version")
[[ -n ${sha1:-} ]] || { echo "update-packages.sh: CEF $cef_version isn't in the CEF index" >&2; exit 1; }
sed -i -E \
    -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^_cef=.*/_cef=$cef/" \
    -e "/^sha1sums=/,/\)/ s/'[0-9a-f]{40}'/'$sha1'/" \
    "$source_pkgbuild"
sed -i -E \
    -e "s/^(  version \? \")[^\"]*(\",)$/\1$version\2/" \
    -e "s|^(        hash = \")sha256-[^\"]*(\";)$|\1$sri\2|" \
    "$nix"

# Every field must have changed to (or already be) the new values.
if ! grep -qx "pkgver=$version" "$pkgbuild" || ! grep -qx "sha256sums=('$hex')" "$pkgbuild" \
    || ! grep -qx "pkgver=$version" "$source_pkgbuild" || ! grep -qx "_cef=$cef" "$source_pkgbuild" \
    || ! grep -qF "'$sha1')" "$source_pkgbuild" \
    || ! grep -qF "  version ? \"$version\"," "$nix" || ! grep -qF "hash = \"$sri\";" "$nix"; then
    echo "update-packages.sh: a field wasn't updated; check the patterns" >&2
    exit 1
fi
echo "packages: riptide $version, sha256 $hex, CEF $cef"
