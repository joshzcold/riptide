#!/usr/bin/env bash
# Prints the CEF that cef-dll-sys downloaded for the version Cargo.lock pins:
#   scripts/cef-dir.sh <target dir, such as target/debug>
# A restored build cache can hold other versions' downloads too, and starting
# riptide with one of those fails ("unsupported CEF API version").
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# cef-dll-sys's version is <crate version>+<CEF version>.
version=$(sed -n '/^name = "cef-dll-sys"$/{n;s/^version = ".*+\(.*\)"$/\1/p;}' "$root/Cargo.lock")
[[ -n $version ]] || { echo "cef-dir.sh: no cef-dll-sys in Cargo.lock" >&2; exit 1; }
# Without CEF_PATH, cef-dll-sys unpacks CEF into its build directory, marked by archive.json.
while IFS= read -r archive; do
    if grep -Fq "\"cef_binary_$version+" "$archive"; then
        dirname "$archive"
        exit 0
    fi
done < <(find "$1/build" -path '*cef-dll-sys-*/out/*/archive.json')
echo "cef-dir.sh: no CEF $version under $1/build; build riptide first" >&2
exit 1
