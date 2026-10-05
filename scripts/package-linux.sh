#!/usr/bin/env bash
# Packs a release build and the CEF runtime it needs into a tarball, and
# with --appimage also into an AppImage:
#   scripts/package-linux.sh [--appimage] [version]
#     -> dist/riptide-<version>-linux-<arch>.tar.gz (and .AppImage)
# The binary finds libcef.so next to itself through its $ORIGIN rpath.
set -euo pipefail

APPIMAGETOOL_VERSION=1.9.1
appimagetool_sha256() {
    case $1 in
        x86_64) echo ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 ;;
        aarch64) echo f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158 ;;
    esac
}

appimage=false
if [[ ${1:-} == --appimage ]]; then
    appimage=true
    shift
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
build=target/release
version=${1:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"riptide","version":"\([^"]*\)".*/\1/p')}
name=riptide-$version-linux-$(uname -m)

[[ -x $build/riptide ]] || { echo "package-linux.sh: run 'task release' first" >&2; exit 1; }

stage=dist/$name
rm -rf "$stage"
mkdir -p "$stage"
# Chromium's runtime files, as the cef-dll-sys build script copies them.
files=(riptide libcef.so libvk_swiftshader.so libvulkan.so.1 vk_swiftshader_icd.json
    chrome-sandbox icudtl.dat v8_context_snapshot.bin resources.pak chrome_100_percent.pak
    chrome_200_percent.pak CREDITS.html)
for f in "${files[@]}"; do
    cp -a "$build/$f" "$stage/"
done
cp -a "$build/locales" "$stage/"
cp LICENSE README.md CHANGELOG.md packaging/riptide.desktop packaging/riptide.svg "$stage/"
# CEF ships libcef.so with debug info (1.4 GB); stripped it is ~260 MB.
strip "$stage/riptide" "$stage/libcef.so" "$stage/libvk_swiftshader.so"

tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "dist/$name.tar.gz"

if $appimage; then
    arch=$(uname -m)
    tool=$root/.bin/appimagetool-$APPIMAGETOOL_VERSION-$arch
    if [[ ! -x $tool ]]; then
        mkdir -p "$root/.bin"
        curl -fsSL -o "$tool.part" \
            "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-$arch.AppImage"
        actual=$(sha256sum "$tool.part" | cut -d' ' -f1)
        if [[ $actual != "$(appimagetool_sha256 "$arch")" ]]; then
            echo "package-linux.sh: appimagetool checksum mismatch (got $actual)" >&2
            rm -f "$tool.part"
            exit 1
        fi
        mv "$tool.part" "$tool"
        chmod +x "$tool"
    fi
    appdir=dist/$name.AppDir
    rm -rf "$appdir"
    cp -a "$stage" "$appdir"
    cp packaging/riptide.desktop packaging/riptide.svg "$appdir/"
    cat >"$appdir/AppRun" <<'APPRUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/riptide" "$@"
APPRUN
    chmod +x "$appdir/AppRun"
    # Works without FUSE too (e.g. in containers).
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$arch "$tool" --no-appstream "$appdir" "dist/$name.AppImage" >&2
    rm -rf "$appdir"
    echo "dist/$name.AppImage"
fi
rm -rf "$stage"
