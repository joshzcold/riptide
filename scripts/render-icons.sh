#!/usr/bin/env bash
# Render packaging/riptide.svg to the PNG files riptide sets as its window icon
# (crates/rt-cef/icons/). Run after changing the logo; needs Inkscape.
set -euo pipefail
cd "$(dirname "$0")/.."
out=crates/rt-cef/icons
mkdir -p "$out"
for size in 32 64 128 256; do
    inkscape packaging/riptide.svg --export-type=png --export-filename="$out/riptide-$size.png" \
        --export-width="$size" --export-height="$size" >/dev/null 2>&1
done
ls -l "$out"
