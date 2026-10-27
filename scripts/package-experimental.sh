#!/usr/bin/env bash
# Packs an experimental macOS or Windows release build with its CEF runtime:
#   scripts/package-experimental.sh [version]
#     -> dist/riptide-<version>-macos-<arch>.dmg (riptide.app), or on Windows
#        -windows-<arch>.zip and the installer, -windows-<arch>-setup.exe
# Both start on CI but are untested by hand; the macOS app is ad-hoc signed.
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
if [[ -n ${CEF_PATH:-} && -f $CEF_PATH/archive.json ]]; then
    cef=$CEF_PATH
else
    cef=$(scripts/cef-dir.sh "$build")
fi

stage=dist/$name
rm -rf "$stage"
mkdir -p "$stage"
if [[ $os == macos ]]; then
    scripts/bundle-macos.sh "$build/$exe" "$cef" "$stage" "$version" >/dev/null
    # Drag riptide.app onto Applications to install it.
    ln -s /Applications "$stage/Applications"
else
    cp "$build/$exe" "$stage/"
    # The runtime files cef-dll-sys copies next to the binary: CEF's top-level
    # files and locales, minus the build-only ones and CEF's sample launchers.
    find "$cef" -maxdepth 1 -type f ! -name archive.json ! -name 'CMakeLists.txt' ! -name '*.lib' \
        ! -name 'bootstrap*.exe' -exec cp {} "$stage/" \;
    cp -R "$cef/locales" "$stage/"
fi
cp LICENSE LICENSE-EXCEPTION README.md CHANGELOG.md "$stage/"
status="starts, but is untested by hand"
cat >"$stage/EXPERIMENTAL.txt" <<EOF
This $os build of riptide $version is experimental and $status.
Linux is the only supported platform for now. See https://joshzcold.github.io/riptide/
EOF

if [[ $os == macos ]]; then
    hdiutil create -quiet -volname "riptide $version" -srcfolder "$stage" -ov -format UDZO "dist/$name.dmg"
    echo "dist/$name.dmg"
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
