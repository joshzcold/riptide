#!/usr/bin/env bash
# Packs an experimental macOS or Windows release build with its CEF runtime:
#   scripts/package-experimental.sh [version]
#     -> dist/riptide-<version>-macos-<arch>.tar.gz or -windows-<arch>.zip
# Neither platform can run the browser yet (no app bundle or installer, M10);
# these exist so the builds can be tried and the packaging grows from here.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
build=target/release
version=${1:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"riptide","version":"\([^"]*\)".*/\1/p')}

case $(uname -s) in
    Darwin) os=macos exe=riptide ;;
    MINGW* | MSYS* | CYGWIN*) os=windows exe=riptide.exe ;;
    *) echo "package-experimental.sh: for macOS and Windows; use package-linux.sh on Linux" >&2; exit 1 ;;
esac
case $(uname -m) in
    arm64 | aarch64) arch=aarch64 ;;
    *) arch=x86_64 ;;
esac
name=riptide-$version-$os-$arch

[[ -f $build/$exe ]] || { echo "package-experimental.sh: run 'cargo build --release' first" >&2; exit 1; }
# Without CEF_PATH, cef-dll-sys unpacks CEF into its build directory, marked by archive.json.
if [[ -n ${CEF_PATH:-} && -f $CEF_PATH/archive.json ]]; then
    cef=$CEF_PATH
else
    cef=$(dirname "$(find "$build/build" -path '*cef-dll-sys-*/out/*/archive.json' | head -1)")
fi
[[ -f $cef/archive.json ]] || { echo "package-experimental.sh: can't find the CEF runtime" >&2; exit 1; }

stage=dist/$name
rm -rf "$stage"
mkdir -p "$stage"
cp "$build/$exe" "$stage/"
if [[ $os == macos ]]; then
    # The framework belongs inside an app bundle; until M10 it ships alongside.
    cp -R "$cef/Chromium Embedded Framework.framework" "$stage/"
else
    # The runtime files cef-dll-sys copies next to the binary: CEF's top-level
    # files and locales, minus the build-only ones and CEF's sample launchers.
    find "$cef" -maxdepth 1 -type f ! -name archive.json ! -name 'CMakeLists.txt' ! -name '*.lib' \
        ! -name 'bootstrap*.exe' -exec cp {} "$stage/" \;
    cp -R "$cef/locales" "$stage/"
fi
cp LICENSE README.md CHANGELOG.md "$stage/"
cat >"$stage/EXPERIMENTAL.txt" <<EOF
This $os build of riptide $version is experimental and does not run yet: it
still needs an app bundle (macOS) or installer (Windows). Linux is the only
supported platform for now. See https://joshzcold.github.io/riptide/
EOF

if [[ $os == macos ]]; then
    tar -C dist -czf "dist/$name.tar.gz" "$name"
    echo "dist/$name.tar.gz"
else
    (cd dist && 7z a -tzip -bso0 "$name.zip" "$name")
    echo "dist/$name.zip"
fi
rm -rf "$stage"
