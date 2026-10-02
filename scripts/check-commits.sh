#!/usr/bin/env bash
# Checks commit subjects follow Conventional Commits, which git-cliff turns
# into CHANGELOG.md. Usage:
#   scripts/check-commits.sh <rev-range>     # e.g. origin/main..HEAD (CI)
#   scripts/check-commits.sh --file <path>   # a commit message (git hook)
set -euo pipefail

TYPES="feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert"
PATTERN="^($TYPES)(\([a-z0-9._/-]+\))?!?: [^ ].*"

check() {
    local subject=$1 where=$2
    # Merge and fixup commits are made by tools, not written by hand.
    [[ $subject =~ ^(Merge|fixup!|squash!) ]] && return 0
    if [[ ! $subject =~ $PATTERN ]]; then
        echo "✗ $where: '$subject'" >&2
        echo "  expected '<type>[(scope)][!]: <description>' with type one of: ${TYPES//|/, }" >&2
        return 1
    fi
}

if [[ ${1:-} == --file ]]; then
    check "$(grep -v '^#' "$2" | head -1)" "commit message"
    exit
fi

range=${1:?usage: check-commits.sh <rev-range> | --file <path>}
failed=0
while IFS=' ' read -r sha subject; do
    check "$subject" "${sha:0:8}" || failed=1
done < <(git log --format='%H %s' "$range")
if (( failed )); then
    echo "See https://www.conventionalcommits.org; e.g. 'feat(tabs): add pinned tabs'." >&2
    exit 1
fi
echo "commit messages ok"
