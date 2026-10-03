#!/usr/bin/env bash
# Packs a release build and the CEF runtime it needs into a tarball:
#   scripts/package-linux.sh [version]   -> dist/hackers-browser-<version>-linux-<arch>.tar.gz
# The binary finds libcef.so next to itself through its $ORIGIN rpath.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
build=target/release
version=${1:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"hb","version":"\([^"]*\)".*/\1/p')}
name=hackers-browser-$version-linux-$(uname -m)

[[ -x $build/hackers-browser ]] || { echo "package-linux.sh: run 'task release' first" >&2; exit 1; }

stage=dist/$name
rm -rf "$stage"
mkdir -p "$stage"
# Chromium's runtime files, as the cef-dll-sys build script copies them.
files=(hackers-browser libcef.so libvk_swiftshader.so libvulkan.so.1 vk_swiftshader_icd.json
    chrome-sandbox icudtl.dat v8_context_snapshot.bin resources.pak chrome_100_percent.pak
    chrome_200_percent.pak CREDITS.html)
for f in "${files[@]}"; do
    cp -a "$build/$f" "$stage/"
done
cp -a "$build/locales" "$stage/"
cp LICENSE README.md CHANGELOG.md "$stage/"
# CEF ships libcef.so with debug info (1.4 GB); stripped it is ~260 MB.
strip "$stage/hackers-browser" "$stage/libcef.so" "$stage/libvk_swiftshader.so"

tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
echo "dist/$name.tar.gz"
