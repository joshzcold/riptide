#!/usr/bin/env bash
# Runs mdBook (https://rust-lang.github.io/mdBook/), downloading a pinned
# release into .bin/ when it is not installed. Arguments pass straight through.
set -euo pipefail

VERSION=0.5.4

# mdBook publishes no checksums; these were taken from the release assets.
expected_sha512() {
    case $1 in
        x86_64-unknown-linux-gnu) echo c9e0dcafb93a96c8f8cb79465bbd7c87ca88528ee40e0fcf6a8827fe9016095affbb1ee961f4410e292a2248d715750f04ab4f0e6597744822b779508af5db2f ;;
        aarch64-unknown-linux-musl) echo 0b1607ee74371636d87073c76da7ad159eabc787316f52aa61500e70ec56b514bb705985286e9e28e4e51205de442fe5d964135fed97c71d521e5b994829af9d ;;
        x86_64-apple-darwin) echo 0a5f646559a7d098ebe317c9fb14934fdaeec4f3720fe76460ba48c7e1104a88428e52993eddae5fddb1e26c88e005f9f62b5edd18eec1ccf0f5c52c41c8b566 ;;
        aarch64-apple-darwin) echo 43da198b19bbd537f4d7a252ac48b5824db93193bf7253828d291a2cd7148c43040edb7c4017726ce97f88b2765724ab14af55fb08d18ab9a89678c2b5a02c54 ;;
    esac
}

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

if command -v mdbook >/dev/null && [[ $(mdbook --version) == "mdbook v$VERSION" ]]; then
    exec mdbook "$@"
fi

bin=$root/.bin/mdbook-$VERSION
if [[ ! -x $bin ]]; then
    case $(uname -s)-$(uname -m) in
        Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux-aarch64) target=aarch64-unknown-linux-musl ;;
        Darwin-x86_64) target=x86_64-apple-darwin ;;
        Darwin-arm64) target=aarch64-apple-darwin ;;
        *) echo "mdbook.sh: unsupported platform; install mdBook v$VERSION from https://github.com/rust-lang/mdBook" >&2; exit 1 ;;
    esac
    asset=mdbook-v$VERSION-$target.tar.gz
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    echo "mdbook.sh: downloading mdBook v$VERSION ($target)" >&2
    curl -fsSL -o "$tmp/$asset" "https://github.com/rust-lang/mdBook/releases/download/v$VERSION/$asset"
    if command -v sha512sum >/dev/null; then
        actual=$(sha512sum "$tmp/$asset" | cut -d' ' -f1)
    else
        actual=$(shasum -a 512 "$tmp/$asset" | cut -d' ' -f1)
    fi
    if [[ $actual != "$(expected_sha512 "$target")" ]]; then
        echo "mdbook.sh: checksum mismatch for $asset (got $actual)" >&2
        exit 1
    fi
    tar -xzf "$tmp/$asset" -C "$tmp"
    mkdir -p "$root/.bin"
    mv "$tmp/mdbook" "$bin"
    chmod +x "$bin"
fi
exec "$bin" "$@"
