#!/usr/bin/env bash
# Packs an experimental macOS or Windows release build with its CEF runtime:
#   scripts/package-experimental.sh [version]
#     -> dist/riptide-<version>-macos-<arch>.tar.gz, or on Windows
#        -windows-<arch>.zip and the installer, -windows-<arch>-setup.exe
# macOS can't run the browser yet (no app bundle, M10); Windows is untested by hand.
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
cp LICENSE LICENSE-EXCEPTION README.md CHANGELOG.md "$stage/"
if [[ $os == windows ]]; then
    status="starts, but is untested by hand"
else
    status="does not run yet: it still needs an app bundle"
fi
cat >"$stage/EXPERIMENTAL.txt" <<EOF
This $os build of riptide $version is experimental and $status.
Linux is the only supported platform for now. See https://joshzcold.github.io/riptide/
EOF

if [[ $os == macos ]]; then
    tar -C dist -czf "dist/$name.tar.gz" "$name"
    echo "dist/$name.tar.gz"
else
    (cd dist && 7z a -tzip -bso0 "$name.zip" "$name")
    echo "dist/$name.zip"
    iscc=$(command -v iscc || echo "/c/Program Files (x86)/Inno Setup 6/ISCC.exe")
    [[ -x $iscc ]] || { echo "package-experimental.sh: Inno Setup's iscc isn't installed" >&2; exit 1; }
    "$iscc" -Q "-DVersion=$version" "-DArch=$arch" "-DSource=$(cygpath -w "$root/$stage")" \
        "-DOutput=$(cygpath -w "$root/dist")" "$(cygpath -w "$root/packaging/windows/riptide.iss")"
    echo "dist/$name-setup.exe"
fi
rm -rf "$stage"
