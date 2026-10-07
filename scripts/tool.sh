#!/usr/bin/env bash
# Runs a pinned lint tool, downloading and checksum-verifying it into .bin/
# the first time. Arguments pass straight through:
#   scripts/tool.sh actionlint|shellcheck|cargo-deny|typos|biome [args...]
# Pinned rather than taken from PATH, so new upstream checks never break CI
# unannounced; bump a version and its checksums here on purpose.
set -euo pipefail

# actionlint runs this ShellCheck on workflow run: blocks too.
shellcheck_version=0.11.0

tool=${1:?usage: scripts/tool.sh actionlint|shellcheck|cargo-deny|typos|biome [args...]}
shift

case $(uname -s)-$(uname -m) in
    Linux-x86_64) platform=linux-x86_64 ;;
    Linux-aarch64) platform=linux-aarch64 ;;
    Darwin-x86_64) platform=macos-x86_64 ;;
    Darwin-arm64) platform=macos-aarch64 ;;
    *) echo "tool.sh: unsupported platform $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

# Sets version, url, sha256 and member (the binary's path inside the archive;
# empty when the download is the binary itself). Checksums are the published
# ones where a project publishes them, otherwise taken from the download.
case $tool in
    actionlint)
        version=1.7.12
        case $platform in
            linux-x86_64) a=linux_amd64 sha256=8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8 ;;
            linux-aarch64) a=linux_arm64 sha256=325e971b6ba9bfa504672e29be93c24981eeb1c07576d730e9f7c8805afff0c6 ;;
            macos-x86_64) a=darwin_amd64 sha256=5b44c3bc2255115c9b69e30efc0fecdf498fdb63c5d58e17084fd5f16324c644 ;;
            macos-aarch64) a=darwin_arm64 sha256=aba9ced2dee8d27fecca3dc7feb1a7f9a52caefa1eb46f3271ea66b6e0e6953f ;;
        esac
        url=https://github.com/rhysd/actionlint/releases/download/v$version/actionlint_${version}_$a.tar.gz
        member=actionlint
        ;;
    shellcheck)
        version=$shellcheck_version
        case $platform in
            linux-x86_64) a=linux.x86_64 sha256=b7af85e41cc99489dcc21d66c6d5f3685138f06d34651e6d34b42ec6d54fe6f6 ;;
            linux-aarch64) a=linux.aarch64 sha256=68a8133197a50beb8803f8d42f9908d1af1c5540d4bb05fdfca8c1fa47decefc ;;
            macos-x86_64) a=darwin.x86_64 sha256=c2c15e08df0e8fbc374c335b230a7ee958c313fa5714817a59aa59f1aa594f51 ;;
            macos-aarch64) a=darwin.aarch64 sha256=339b930feb1ea764467013cc1f72d09cd6b869ebf1013296ba9055ab2ffbd26f ;;
        esac
        url=https://github.com/koalaman/shellcheck/releases/download/v$version/shellcheck-v$version.$a.tar.gz
        member=shellcheck-v$version/shellcheck
        ;;
    cargo-deny)
        version=0.20.2
        case $platform in
            linux-x86_64) a=x86_64-unknown-linux-musl sha256=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f ;;
            linux-aarch64) a=aarch64-unknown-linux-musl sha256=995c82be0defc7a025cae49a2aa2644ce8245c9a3318fc4103907c6a285e8c7d ;;
            macos-x86_64) a=x86_64-apple-darwin sha256=248da7f581724e470071990c088ffc55c811981715f4cbdb258621fb79f8b7a6 ;;
            macos-aarch64) a=aarch64-apple-darwin sha256=fe67d82a10d8597a3549364cb733a3f9cc1bfff9031b7ae46384a9f2a72090c3 ;;
        esac
        url=https://github.com/EmbarkStudios/cargo-deny/releases/download/$version/cargo-deny-$version-$a.tar.gz
        member=cargo-deny-$version-$a/cargo-deny
        ;;
    typos)
        version=1.51.1
        case $platform in
            linux-x86_64) a=x86_64-unknown-linux-musl sha256=93c301fd4120ff076e2cc2a6d6a61cab4a4bdb81899287c0938b452e56d1f454 ;;
            linux-aarch64) a=aarch64-unknown-linux-musl sha256=2ac5e37b3236b3c07dec5cdf1527acf143aec4674a6089b0bf424d021b35e099 ;;
            macos-x86_64) a=x86_64-apple-darwin sha256=f3451079971633a7f769fb958fe06b0d64b5f1dc46df0854aa1ddb9a6f4031d6 ;;
            macos-aarch64) a=aarch64-apple-darwin sha256=316d50a32c3726a80ab39c6d12141b70080498973c5e8a9b068b12cde78b10c1 ;;
        esac
        url=https://github.com/crate-ci/typos/releases/download/v$version/typos-v$version-$a.tar.gz
        member=./typos
        ;;
    biome)
        version=2.5.15
        case $platform in
            linux-x86_64) a=linux-x64-musl sha256=42b6fd3662fc1fc3075a370414478aa3ed82fd43d28430bbfb81ebd02cbde656 ;;
            linux-aarch64) a=linux-arm64-musl sha256=195d4525282d2b877357ba8377545061e342808da37997051153708c7b5326fa ;;
            macos-x86_64) a=darwin-x64 sha256=86fc618563d19f373ac24eb5b8f1e41bd753a506d5df1f198c152ec365141490 ;;
            macos-aarch64) a=darwin-arm64 sha256=3636796d8c78bfcfe0d45946dc93e21e12d27a49eba7401e18af1ea854d25503 ;;
        esac
        url=https://github.com/biomejs/biome/releases/download/%40biomejs%2Fbiome%40$version/biome-$a
        member=
        ;;
    *) echo "tool.sh: unknown tool $tool" >&2; exit 1 ;;
esac

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
bin=$root/.bin/$tool-$version
if [[ ! -x $bin ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo "tool.sh: downloading $tool $version ($platform)" >&2
    curl -fsSL -o "$tmp/archive.tar.gz" "$url"
    if command -v sha256sum >/dev/null; then
        actual=$(sha256sum "$tmp/archive.tar.gz" | cut -d' ' -f1)
    else
        actual=$(shasum -a 256 "$tmp/archive.tar.gz" | cut -d' ' -f1)
    fi
    if [[ $actual != "$sha256" ]]; then
        echo "tool.sh: checksum mismatch for $url (got $actual)" >&2
        exit 1
    fi
    if [[ -n $member ]]; then
        tar -xzf "$tmp/archive.tar.gz" -C "$tmp" "$member"
    else
        member=archive.tar.gz
    fi
    mkdir -p "$root/.bin"
    mv "$tmp/$member" "$bin"
    chmod +x "$bin"
fi
if [[ $tool == actionlint ]]; then
    "$0" shellcheck --version >/dev/null
    exec "$bin" -shellcheck "$root/.bin/shellcheck-$shellcheck_version" "$@"
fi
exec "$bin" "$@"
