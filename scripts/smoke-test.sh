#!/usr/bin/env bash
# End-to-end check of the real browser on a throwaway X display. The test page
# reports its state through document.title, which we read via the window name.
set -euo pipefail

BIN=${BIN:-target/debug/hackers-browser}
TIMEOUT=${TIMEOUT:-15}

for tool in Xvfb xdotool; do
    command -v "$tool" >/dev/null || { echo "smoke-test: $tool is required" >&2; exit 1; }
done
[[ -x $BIN ]] || { echo "smoke-test: $BIN not found; run 'task build' first" >&2; exit 1; }

work=$(mktemp -d)
xvfb_pid=""
browser_pid=""
cleanup() {
    [[ -n $browser_pid ]] && kill "$browser_pid" 2>/dev/null || true
    [[ -n $xvfb_pid ]] && kill "$xvfb_pid" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT

cat >"$work/page.html" <<'EOF'
<!doctype html><title>ready</title>
<input id="f" style="position:fixed;top:0;left:0;width:300px;height:40px">
<button id="b" style="position:fixed;top:0;left:320px;height:40px" onclick="clicked = event.isTrusted ? 'trusted' : 'synthetic'; report()">b</button>
<div style="height:5000px"></div>
<script>
let keys = 0, clicked = 'no';
const f = document.getElementById('f');
const report = () => { document.title = `s=${Math.round(scrollY)} k=${keys} v=${f.value} c=${clicked}`; };
// Modifier-only presses reach the page by design, so leave them out of the count.
addEventListener('keydown', (e) => { if (!['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) { keys++; report(); } });
addEventListener('scroll', report);
f.addEventListener('input', report);
</script>
EOF

cat >"$work/second.html" <<'EOF'
<!doctype html><title>second</title>
EOF

Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$work/display" 2>/dev/null &
xvfb_pid=$!
for _ in $(seq 50); do [[ -s $work/display ]] && break; sleep 0.1; done
export DISPLAY=":$(cat "$work/display")"

failures=0
step() { printf '  %-44s' "$1"; }
pass() { echo "ok"; }
fail() { echo "FAIL ($1)"; failures=$((failures + 1)); }

# Polls the window title until it matches, failing after TIMEOUT seconds.
expect_title() {
    local want=$1 title=""
    for _ in $(seq $((TIMEOUT * 10))); do
        title=$(xdotool getwindowname "$window" 2>/dev/null || true)
        [[ $title == "$want - hackers-browser" ]] && { pass; return; }
        sleep 0.1
    done
    fail "title was '$title'"
}

echo "smoke-test on $DISPLAY"
# A private data dir keeps the test away from the real profile.
XDG_DATA_HOME=$work/data "$BIN" "file://$work/page.html" >"$work/browser.log" 2>&1 &
browser_pid=$!

step "window opens and loads the page"
window=$(timeout "$TIMEOUT" xdotool search --sync --name "^ready - hackers-browser$" | head -1 || true)
if [[ -z $window ]]; then
    fail "no window"
    tail -20 "$work/browser.log" >&2
    exit 1
fi
pass
xdotool windowfocus --sync "$window"

step "clicking a field enters insert mode"
xdotool mousemove --window "$window" 50 40 click 1
sleep 0.3
xdotool type --delay 20 abc
expect_title "s=0 k=3 v=abc c=no"

step "Escape leaves insert mode, 5j scrolls 200px"
xdotool key Escape 5 j
expect_title "s=200 k=3 v=abc c=no"

step "G scrolls to the bottom without page keys"
xdotool key shift+g
sleep 0.5
title=$(xdotool getwindowname "$window")
[[ $title =~ ^s=([0-9]+)\ k=3\  && ${BASH_REMATCH[1]} -gt 3000 ]] && pass || fail "title was '$title'"

# Matches the first page's title whatever its scroll state.
expect_first_page() {
    local title=""
    for _ in $(seq $((TIMEOUT * 10))); do
        title=$(xdotool getwindowname "$window" 2>/dev/null || true)
        [[ $title == s=*" - hackers-browser" ]] && { pass; return; }
        sleep 0.1
    done
    fail "title was '$title'"
}

step "f + label clicks the button for real"
xdotool key f
sleep 0.5
xdotool key s
title=""
for _ in $(seq $((TIMEOUT * 10))); do
    title=$(xdotool getwindowname "$window")
    [[ $title == *" k=3 v=abc c=trusted - hackers-browser" ]] && break
    sleep 0.1
done
[[ $title == *" k=3 v=abc c=trusted - hackers-browser" ]] && pass || fail "title was '$title'"

step ":open -t opens and focuses a new tab"
xdotool key shift+semicolon
xdotool type --delay 5 "open -t file://$work/second.html"
xdotool key Return
expect_title "second"

step "K switches back to the first tab"
xdotool key shift+k
expect_first_page

step "d closes the second tab"
xdotool key shift+j d
expect_first_page

step "u restores it"
xdotool key u
expect_title "second"

step ":quit with two tabs exits cleanly"
xdotool key shift+semicolon q u i t Return
code=""
for _ in $(seq $((TIMEOUT * 10))); do
    if ! kill -0 "$browser_pid" 2>/dev/null; then
        wait "$browser_pid" && code=0 || code=$?
        break
    fi
    sleep 0.1
done
browser_pid=""
[[ $code == 0 ]] && pass || fail "exit code '${code:-still running}'"

if (( failures > 0 )); then
    echo "smoke-test: $failures check(s) failed; browser log:" >&2
    tail -20 "$work/browser.log" >&2
    exit 1
fi
echo "smoke-test: all checks passed"
