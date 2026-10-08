#!/usr/bin/env bash
# Packs a release build and the CEF runtime it needs into a tarball, and
# with --appimage and --deb also into an AppImage and a Debian package:
#   scripts/package-linux.sh [--appimage] [--deb] [version]
#     -> dist/riptide-<version>-linux-<arch>.tar.gz (.AppImage, riptide_<version>_<arch>.deb)
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
deb=false
while [[ ${1:-} == --* ]]; do
    case $1 in
        --appimage) appimage=true ;;
        --deb) deb=true ;;
        *) echo "package-linux.sh: unknown option $1" >&2; exit 1 ;;
    esac
    shift
done

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
build=target/release
version=${1:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"riptide","version":"\([^"]*\)".*/\1/p')}
name=riptide-$version-linux-$(uname -m)

# crash_reporter.cfg turns on Chromium's crash reporter. With no ServerURL it
# only keeps dumps in the data directory. Keep in step with
# rt_storage::crash_reports::dumps::reporter_config.
crash_config() {
    printf '%s\n' \
        "# Written by riptide: Chromium's crash reporter keeps crash dumps in the" \
        "# data directory and never uploads them (there is no ServerURL)." \
        "[Config]" "ProductName=riptide" "ProductVersion=$version"
}

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
crash_config > "$stage/crash_reporter.cfg"
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

if $deb; then
    case $(uname -m) in
        x86_64) deb_arch=amd64 ;;
        aarch64) deb_arch=arm64 ;;
        *) echo "package-linux.sh: no Debian architecture for $(uname -m)" >&2; exit 1 ;;
    esac
    tree=dist/$name.deb-root
    rm -rf "$tree"
    mkdir -p "$tree/opt" "$tree/usr/bin" "$tree/DEBIAN"
    cp -a "$stage" "$tree/opt/riptide"
    ln -s /opt/riptide/riptide "$tree/usr/bin/riptide"
    install -Dm644 packaging/riptide.desktop "$tree/usr/share/applications/riptide.desktop"
    install -Dm644 packaging/riptide.svg "$tree/usr/share/icons/hicolor/scalable/apps/riptide.svg"
    install -d "$tree/usr/share/doc/riptide"
    cat >"$tree/usr/share/doc/riptide/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: riptide
Source: https://github.com/joshzcold/riptide

Files: *
Copyright: Joshua Cold
License: GPL-3.0-or-later
 On Debian systems, the full text is in /usr/share/common-licenses/GPL-3.
 Chromium's and CEF's licenses are in /opt/riptide/CREDITS.html.
EOF
    # Packaged files must not be group- or world-writable, whatever the umask.
    chmod -R go-w "$tree"
    # The sandbox helper must be setuid root where the kernel doesn't allow
    # unprivileged user namespaces; --root-owner-group makes it root's.
    chmod 4755 "$tree/opt/riptide/chrome-sandbox"

    # Depends: the system libraries the binaries link, as packages of the
    # distribution this runs on (built on Ubuntu 24.04: also Debian 13).
    shlib=$(mktemp -d)
    mkdir -p "$shlib/debian"
    printf 'Source: riptide\n\nPackage: riptide\nArchitecture: any\n' >"$shlib/debian/control"
    depends=$(cd "$shlib" && dpkg-shlibdeps -O -l"$root/$tree/opt/riptide" \
        "$root/$tree/opt/riptide/riptide" "$root/$tree/opt/riptide/libcef.so" \
        "$root/$tree/opt/riptide/libvk_swiftshader.so" 2>/dev/null | sed -n 's/^shlibs:Depends=//p')
    rm -rf "$shlib"
    [[ -n $depends ]] || { echo "package-linux.sh: dpkg-shlibdeps found no dependencies" >&2; exit 1; }
    size=$(du -sk --exclude=DEBIAN "$tree" | cut -f1)
    cat >"$tree/DEBIAN/control" <<EOF
Package: riptide
Version: $version
Architecture: $deb_arch
Maintainer: Joshua Cold <joshzcold@gmail.com>
Installed-Size: $size
Depends: $depends, xdg-utils
Section: web
Priority: optional
Homepage: https://joshzcold.github.io/riptide/
Description: keyboard-driven web browser with vim-like bindings
 Riptide is a browser in the spirit of qutebrowser, built on the Chromium
 Embedded Framework (CEF) and controlled from Rust: modes, hints, a command
 line, and configuration in TOML or Lua.
EOF
    deb_file=dist/riptide_${version}_$deb_arch.deb
    dpkg-deb --root-owner-group -Zxz --build "$tree" "$deb_file" >&2
    rm -rf "$tree"
    echo "$deb_file"
fi
rm -rf "$stage"
