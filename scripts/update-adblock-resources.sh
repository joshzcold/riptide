#!/usr/bin/env bash
# Regenerates crates/rt-adblock/resources/ubo.json (scriptlets and $redirect
# files) from uBlock Origin's newest release, or the tag given:
#   scripts/update-adblock-resources.sh [tag]
# Needs curl, unzip and node.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
tag=${1:-$(curl -fsSL https://api.github.com/repos/gorhill/uBlock/releases/latest | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')}
[[ -n $tag ]] || { echo "update-adblock-resources.sh: can't find uBlock Origin's newest release" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL -o "$work/ubo.zip" "https://github.com/gorhill/uBlock/releases/download/$tag/uBlock0_$tag.chromium.zip"
unzip -q "$work/ubo.zip" -d "$work"

out=crates/rt-adblock/resources
mkdir -p "$out"
node scripts/adblock-resources.mjs "$work/uBlock0.chromium" "$out/ubo.json"
echo "$tag" > "$out/ubo.version"
echo "uBlock Origin $tag"
