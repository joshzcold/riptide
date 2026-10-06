#!/usr/bin/env bash
# Opens a pull request from <branch> into main, for the release workflow:
#   scripts/open-pr.sh <branch> <title> <body>
# When Actions may not create pull requests (a repository setting), it adds a
# link to open one by hand to the job summary instead, and still succeeds.
set -euo pipefail

branch=${1:?usage: scripts/open-pr.sh <branch> <title> <body>}
title=${2:?usage: scripts/open-pr.sh <branch> <title> <body>}
body=${3:?usage: scripts/open-pr.sh <branch> <title> <body>}
repo=${GITHUB_REPOSITORY:?run from a workflow}

if url=$(gh pr create --base main --head "$branch" --title "$title" --body "$body" 2>&1); then
    echo "Opened $url"
    summary="Opened [$title]($url)."
else
    echo "::warning::couldn't open the pull request ($url); open it by hand"
    link="https://github.com/$repo/compare/main...$branch?expand=1"
    summary="**Open a pull request for [$branch]($link)** to continue: $title."
fi
echo "$summary" >>"${GITHUB_STEP_SUMMARY:-/dev/stdout}"
