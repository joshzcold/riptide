#!/usr/bin/env bash
# Runs git-cliff (https://git-cliff.org), downloading a pinned release into
# .bin/ when it is not installed. Arguments pass straight through.
set -euo pipefail

VERSION=2.14.2

# From the release's .sha512 files; plain case so macOS bash 3.2 works.
expected_sha512() {
    case $1 in
        x86_64-unknown-linux-gnu) echo 26d1f7c8ea2400f2ccb0b2e7f321635f600b583bf4cd1f7afaa5e8c24068ad1088a2504e0a4d9f4333d4bb20a80329327d9462634343ee3af91e1a7fbdc6f18a ;;
        aarch64-unknown-linux-gnu) echo fd27768ea76fea8c3ba3f3247fef2f6604988b629f15e48bf40be5eaa08b8fef2e4087cc9c8af16bc522bc19cc369611fa7692101b513f63469ad58522708600 ;;
        x86_64-apple-darwin) echo 4d32c50a010cbff65e46f1f36347c2dd1a20f17efea7b77115d3166146ad467dc803cd07b839fc35e0a6639aa8d1af9eab7cf8f7984bcd819513f46a396ff801 ;;
        aarch64-apple-darwin) echo 6ec30660233fcd73b92e9d888965945b6435d6fcd7120df8d7c0799faa46cf06d563d49de721ccf4387e6d61bdc5b37c8eb73273c7d9a0883b90cd9857c6ad9f ;;
    esac
}

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if command -v git-cliff >/dev/null; then
    exec git-cliff "$@"
fi

bin=$root/.bin/git-cliff-$VERSION
if [[ ! -x $bin ]]; then
    case $(uname -s)-$(uname -m) in
        Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
        Darwin-x86_64) target=x86_64-apple-darwin ;;
        Darwin-arm64) target=aarch64-apple-darwin ;;
        *) echo "git-cliff.sh: unsupported platform; install git-cliff from https://git-cliff.org" >&2; exit 1 ;;
    esac
    asset=git-cliff-$VERSION-$target.tar.gz
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    echo "git-cliff.sh: downloading git-cliff v$VERSION ($target)" >&2
    curl -fsSL -o "$tmp/$asset" "https://github.com/orhun/git-cliff/releases/download/v$VERSION/$asset"
    if command -v sha512sum >/dev/null; then
        actual=$(sha512sum "$tmp/$asset" | cut -d' ' -f1)
    else
        actual=$(shasum -a 512 "$tmp/$asset" | cut -d' ' -f1)
    fi
    if [[ $actual != "$(expected_sha512 "$target")" ]]; then
        echo "git-cliff.sh: checksum mismatch for $asset (got $actual)" >&2
        exit 1
    fi
    tar -xzf "$tmp/$asset" -C "$tmp"
    mkdir -p "$root/.bin"
    mv "$tmp/git-cliff-$VERSION/git-cliff" "$bin"
    chmod +x "$bin"
fi
exec "$bin" "$@"
