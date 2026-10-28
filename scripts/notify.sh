#!/usr/bin/env bash
# Sends a push notification through ntfy.sh, for the release and CEF workflows:
#   scripts/notify.sh <title> <message> [link] [tags]
# Needs NTFY_TOPIC and NTFY_ACCESS_TOKEN (repository secrets). Without them it
# sends nothing, and a failed send only warns, so it never fails a job.
set -uo pipefail

title=${1:?usage: scripts/notify.sh <title> <message> [link] [tags]}
message=${2:?usage: scripts/notify.sh <title> <message> [link] [tags]}
link=${3:-}
tags=${4:-}

if [[ -z ${NTFY_TOPIC:-} || -z ${NTFY_ACCESS_TOKEN:-} ]]; then
    echo "notify.sh: NTFY_TOPIC or NTFY_ACCESS_TOKEN isn't set; not sending \"$title\"" >&2
    exit 0
fi
args=(-fsS --retry 3 -H "Authorization: Bearer $NTFY_ACCESS_TOKEN" -H "Title: $title" --data-binary "$message")
[[ -n $link ]] && args+=(-H "Click: $link")
[[ -n $tags ]] && args+=(-H "Tags: $tags")
if curl "${args[@]}" "https://ntfy.sh/$NTFY_TOPIC" >/dev/null; then
    echo "Sent \"$title\""
else
    echo "::warning::couldn't send the notification \"$title\""
fi
