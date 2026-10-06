#!/usr/bin/env bash
# Points the AUR and Nix packages at a release: its version and the Linux
# tarball's checksum.
#   scripts/update-packages.sh <version> <riptide-<version>-linux-x86_64.tar.gz>
# release.yml runs it after publishing and commits the result.
set -euo pipefail

version=${1:?usage: scripts/update-packages.sh <version> <tarball>}
tarball=${2:?usage: scripts/update-packages.sh <version> <tarball>}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "update-packages.sh: '$version' isn't a release version like 0.2.0" >&2; exit 1; }
[[ -f $tarball ]] || { echo "update-packages.sh: no $tarball" >&2; exit 1; }

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
pkgbuild=$root/packaging/aur/PKGBUILD
nix=$root/packaging/nix/package.nix

hex=$(sha256sum "$tarball" | cut -d' ' -f1)
# Nix wants the same hash as base64 (SRI).
sri="sha256-$(python3 -c 'import base64, sys; print(base64.b64encode(bytes.fromhex(sys.argv[1])).decode())' "$hex")"

sed -i -E \
    -e "s/^pkgver=.*/pkgver=$version/" \
    -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$hex')/" \
    "$pkgbuild"
sed -i -E \
    -e "s/^(  version \? \")[^\"]*(\",)$/\1$version\2/" \
    -e "s|^(        hash = \")sha256-[^\"]*(\";)$|\1$sri\2|" \
    "$nix"

# Every field must have changed to (or already be) the new values.
if ! grep -qx "pkgver=$version" "$pkgbuild" || ! grep -qx "sha256sums=('$hex')" "$pkgbuild" \
    || ! grep -qF "  version ? \"$version\"," "$nix" || ! grep -qF "hash = \"$sri\";" "$nix"; then
    echo "update-packages.sh: a field wasn't updated; check the patterns" >&2
    exit 1
fi
echo "packages: riptide $version, sha256 $hex"
