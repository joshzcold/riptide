#!/usr/bin/env bash
# The real browser on a throwaway X display, driven with real X11 input
# (xdotool), which the e2e tests in crates/rt-e2e can't do. It also checks
# that a release package works: build-release.yml runs it against the
# unpacked tarball and AppImage (BIN=...). New behaviour gets an e2e test,
# not a step here.
#
# The window title is set to "{mode}::{page title}", so the test can wait for
# the browser's mode before each key instead of guessing with sleeps, and the
# test pages report their state through document.title.
# pass only echoes, so "check && pass || fail" never runs fail after a pass.
# shellcheck disable=SC2015
set -euo pipefail

BIN=${BIN:-target/debug/riptide}
TIMEOUT=${TIMEOUT:-15}
# CI runners are slower, especially on the first page load; scale the pauses
# that give the browser time to react to keys.
SLOW=${SLOW:-${CI:+3}}
SLOW=${SLOW:-1}
nap() { sleep "$(awk "BEGIN { print $1 * $SLOW }")"; }

for tool in Xvfb xdotool; do
    command -v "$tool" >/dev/null || { echo "smoke-test: $tool is required" >&2; exit 1; }
done
[[ -x $BIN ]] || { echo "smoke-test: $BIN not found; run 'task build' first" >&2; exit 1; }

work=$(mktemp -d)
xvfb_pid=""
browser_pid=""
cleanup() {
    # SMOKE_LOG=path keeps the browser log for debugging.
    [[ -n ${SMOKE_LOG:-} && -f $work/browser.log ]] && cp "$work/browser.log" "$SMOKE_LOG"
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

cat >"$work/nav1.html" <<'EOF'
<!doctype html><title>nav1</title><a href="nav2.html">Next »</a>
EOF
cat >"$work/nav2.html" <<'EOF'
<!doctype html><title>nav2</title><link rel="prev" href="nav1.html">
EOF

Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$work/display" 2>/dev/null &
xvfb_pid=$!
for _ in $(seq 50); do [[ -s $work/display ]] && break; sleep 0.1; done
display=$(cat "$work/display")
export DISPLAY=":$display"

failures=0
step() { printf '  %-56s' "$1"; }
pass() { echo "ok"; }
fail() { echo "FAIL ($1)"; failures=$((failures + 1)); }

name() { xdotool getwindowname "$window" 2>/dev/null || true; }

# Waits for a window whose name matches, other than $2. Polls instead of
# `xdotool search --sync`, which aborts with BadWindow when one of
# Chromium's short-lived helper windows disappears mid-search.
find_window() {
    local found
    for _ in $(seq $((TIMEOUT * 10))); do
        found=$(xdotool search --name "$1" 2>/dev/null | grep -vx "${2:-none}" | head -1 || true)
        [[ -n $found ]] && { echo "$found"; return; }
        sleep 0.1
    done
}
page_title() { local n; n=$(name); echo "${n#*::}"; }
mode() { local n; n=$(name); echo "${n%%::*}"; }

# Polls the page title until it matches, failing after TIMEOUT seconds.
expect_title() {
    local want=$1
    for _ in $(seq $((TIMEOUT * 10))); do
        [[ $(page_title) == "$want" ]] && { pass; return; }
        sleep 0.1
    done
    fail "title was '$(name)'"
}

# Matches the first page's title whatever its scroll state.
expect_first_page() {
    for _ in $(seq $((TIMEOUT * 10))); do
        [[ $(page_title) == s=* ]] && { pass; return; }
        sleep 0.1
    done
    fail "title was '$(name)'"
}

# Waits (quietly) for the browser to reach a mode; the next check reports it.
# An optional second argument overrides TIMEOUT.
wait_mode() {
    for _ in $(seq $((${2:-$TIMEOUT} * 10))); do
        [[ $(mode) == "$1" ]] && return 0
        sleep 0.1
    done
    return 1
}

# Runs a command through the command line. ':' is retried, since a key sent
# while a new tab settles can be lost (docs/PLAN.md, M14 gaps).
run() {
    for _ in 1 2 3; do
        xdotool key shift+semicolon
        wait_mode command 2 && break
    done
    xdotool type --delay 5 "$1"
    xdotool key Return
}

# Follows the hint with the given label.
hint() {
    xdotool key f
    wait_mode hint || true
    xdotool key "$1"
}

# Waits for the browser to exit and checks the exit code.
expect_exit() {
    local code=""
    for _ in $(seq $((TIMEOUT * 10))); do
        if ! kill -0 "$browser_pid" 2>/dev/null; then
            wait "$browser_pid" && code=0 || code=$?
            break
        fi
        sleep 0.1
    done
    browser_pid=""
    [[ $code == 0 ]] && pass || fail "exit code '${code:-still running}'"
}

echo "smoke-test on $DISPLAY"
# A private basedir keeps the test away from the real config and profile.
mkdir -p "$work/base/config"
cat >"$work/base/config/config.lua" <<EOF
c.window.title_format = "{mode}::{current_title}"
rt.bind("X", "open -t file://$work/second.html")
EOF
RT_LOG=${RT_LOG:-info} "$BIN" --basedir "$work/base" "file://$work/page.html" >"$work/browser.log" 2>&1 &
browser_pid=$!

step "window opens and loads the page"
window=$(find_window "^normal::ready$")
if [[ -z $window ]]; then
    fail "no window"
    tail -20 "$work/browser.log" >&2
    exit 1
fi
pass
xdotool windowfocus --sync "$window"

# CI enables user namespaces, so it checks that the sandbox really runs.
if [[ -n ${EXPECT_SANDBOX:-} ]]; then
    step "the Chromium sandbox is on"
    grep -q "Chromium sandbox on" "$work/browser.log" && pass || fail "$(grep -o 'Chromium sandbox.*' "$work/browser.log")"
fi
# Keys sent before the page has keyboard focus can be lost, so retry the
# first hint until hint mode shows up.
for _ in 1 2 3 4 5; do
    xdotool key f
    wait_mode hint && break
done

# Focus the field through a hint (a real CEF click), which doesn't depend on
# where the window lands on the display, unlike an xdotool click.
step "focusing a field enters insert mode; typed keys reach it"
xdotool key a
wait_mode insert || true
xdotool type --delay 20 abc
expect_title "s=0 k=3 v=abc c=no"

step "Escape leaves insert mode, 5j scrolls 200px"
xdotool key Escape
wait_mode normal || true
xdotool key 5 j
expect_title "s=200 k=3 v=abc c=no"

step "f + label clicks the button for real"
hint s
title=""
for _ in $(seq $((TIMEOUT * 10))); do
    title=$(page_title)
    [[ $title == *" k=3 v=abc c=trusted" ]] && break
    sleep 0.1
done
[[ $title == *" k=3 v=abc c=trusted" ]] && pass || fail "title was '$title'"

step "a key bound in config.lua opens a tab"
xdotool key shift+x
expect_title "second"
# Under Xvfb, a key sent just as a tab finishes loading is sometimes dropped
# before CEF sees it (docs/PLAN.md, M14 gaps).
nap 0.3

step "K switches back to the first tab"
xdotool key shift+k
expect_first_page

step "d closes the second tab"
xdotool key shift+j d
expect_first_page

step "u restores it"
xdotool key u
expect_title "second"

step ":help :open opens the bundled help page"
run "help -t :open"
expect_title "Riptide help"
run "tab-close"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == second ]] && break; sleep 0.1; done

step ":open -w opens a window that takes keys, :close closes it"
run "open -w file://$work/nav1.html"
second=$(find_window '^normal::nav1$' "$window")
if [[ -z $second ]]; then
    fail "no second window"
else
    xdotool windowfocus --sync "$second"
    nap 0.3
    xdotool key bracketright bracketright
    for _ in $(seq $((TIMEOUT * 10))); do [[ $(xdotool getwindowname "$second") == *::nav2 ]] && break; sleep 0.1; done
    keys_went=$(xdotool getwindowname "$second")
    first_page=$(page_title)
    run "close"
    for _ in $(seq $((TIMEOUT * 10))); do xdotool getwindowname "$second" >/dev/null 2>&1 || break; sleep 0.1; done
    if [[ $keys_went != *::nav2 ]]; then
        fail "keys didn't reach the second window ('$keys_went')"
    elif [[ $first_page == nav2 ]]; then
        fail "the first window followed the link too"
    elif xdotool getwindowname "$second" >/dev/null 2>&1; then
        fail ":close left the window open"
    else
        pass
    fi
    xdotool windowfocus --sync "$window"
fi

step ":wq with several tabs saves and exits cleanly"
run "set auto_save.session true"
run "wq"
expect_exit

step "restarting restores the session"
"$BIN" --basedir "$work/base" >>"$work/browser.log" 2>&1 &
browser_pid=$!
window=$(find_window "^normal::second$")
[[ -n $window ]] && pass || fail "no restored window"

step ":quit exits cleanly"
[[ -n $window ]] && xdotool windowfocus --sync "$window"
run "quit"
expect_exit

if (( failures > 0 )); then
    echo "smoke-test: $failures check(s) failed; browser log:" >&2
    sed 's/\x1b\[[0-9;]*m//g' "$work/browser.log" | grep -vE "dbus|libva|vaapi|gpu" | tail -30 >&2
    exit 1
fi
echo "smoke-test: all checks passed"
