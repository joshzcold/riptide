#!/usr/bin/env bash
# End-to-end check of the real browser on a throwaway X display. The test page
# reports its state through document.title, which we read via the window name.
set -euo pipefail

BIN=${BIN:-target/debug/hackers-browser}
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

cat >"$work/isolation.html" <<'EOF'
<!doctype html><title>isolation</title>
<iframe id="f" src="hb://ui/statusbar.html"></iframe>
<script>
setTimeout(() => {
  let frame;
  try {
    const d = document.getElementById('f').contentDocument;
    frame = d && d.getElementById('bar') ? 'ui-loaded' : 'empty';
  } catch (e) { frame = 'cross-origin'; }
  document.title = `hb=${typeof window.hb} frame=${frame}`;
}, 1000);
</script>
EOF

cat >"$work/dialogs.html" <<'EOF'
<!doctype html><title>dialogs</title>
<button onclick="document.title = 'confirm ' + confirm('Sure?')">confirm</button>
<a download="saved.txt" href="data:text/plain,hello">download</a>
EOF

Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$work/display" 2>/dev/null &
xvfb_pid=$!
for _ in $(seq 50); do [[ -s $work/display ]] && break; sleep 0.1; done
export DISPLAY=":$(cat "$work/display")"

failures=0
step() { printf '  %-48s' "$1"; }
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
# A private basedir keeps the test away from the real config and profile.
mkdir -p "$work/base/config"
cat >"$work/base/config/config.lua" <<EOF
hb.bind("X", "open -t file://$work/second.html")
c.downloads.location.directory = "$work/dl"
c.downloads.location.prompt = false
EOF
"$BIN" --basedir "$work/base" "file://$work/page.html" >"$work/browser.log" 2>&1 &
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
# Let the first page settle before the first keys (cold caches on CI).
nap 1

# Focus the field through a hint (a real CEF click), which doesn't depend on
# where the window lands on the display, unlike an xdotool click.
step "focusing a field enters insert mode"
xdotool key f
nap 0.5
xdotool key a
nap 0.3
xdotool type --delay 20 abc
expect_title "s=0 k=3 v=abc c=no"

step "Escape leaves insert mode, 5j scrolls 200px"
xdotool key Escape 5 j
expect_title "s=200 k=3 v=abc c=no"

step "G scrolls to the bottom without page keys"
xdotool key shift+g
nap 0.5
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
nap 0.5
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

step "pinned tabs refuse d but close with --force"
xdotool key ctrl+p
nap 0.3
xdotool key d
nap 0.5
title=$(xdotool getwindowname "$window")
xdotool key shift+semicolon
xdotool type --delay 5 "tab-close --force"
xdotool key Return
if [[ $title == "second - hackers-browser" ]]; then expect_first_page; else fail "d closed the pinned tab ('$title')"; fi

step "a key bound in config.lua works"
xdotool key d shift+x
expect_title "second"

step ":set persists to autoconfig.toml"
xdotool key shift+semicolon
xdotool type --delay 5 "set messages.timeout 5000"
xdotool key Return
for _ in $(seq $((TIMEOUT * 10))); do
    grep -q '"messages.timeout" = 5000' "$work/base/config/autoconfig.toml" 2>/dev/null && break
    sleep 0.1
done
grep -q '"messages.timeout" = 5000' "$work/base/config/autoconfig.toml" 2>/dev/null && pass || fail "autoconfig.toml not written"

step ":open completes from history with Tab"
xdotool key shift+semicolon
xdotool type --delay 5 "open -t about:blank"
xdotool key Return
expect_title "about:blank"
xdotool key shift+semicolon
xdotool type --delay 5 "open secon"
nap 0.3
xdotool key Tab Return
expect_title "second"

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

step ":help :open opens the generated help page"
xdotool key shift+semicolon
xdotool type --delay 5 "help :open"
xdotool key Return
expect_title "hackers-browser help"

step "a second invocation hands its arguments to this browser"
code=0
"$BIN" --basedir "$work/base" "file://$work/second.html" ":tab-focus -1" || code=$?
if (( code == 0 )); then expect_title "second"; else fail "second invocation exited with $code"; fi

step "web pages can't see or embed hb:// UI pages"
xdotool key shift+semicolon
xdotool type --delay 5 "open file://$work/isolation.html"
xdotool key Return
expect_title "hb=undefined frame=empty"

step "a JavaScript confirm() is answered with y"
xdotool key shift+semicolon
xdotool type --delay 5 "open file://$work/dialogs.html"
xdotool key Return
expect_title "dialogs"
xdotool key f
nap 0.5
xdotool key a
nap 0.5
xdotool key y
expect_title "confirm true"

step "downloads save to downloads.location.directory"
xdotool key f
nap 0.5
xdotool key s
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/dl/saved.txt ]] && break; sleep 0.1; done
[[ $(cat "$work/dl/saved.txt" 2>/dev/null) == hello ]] && pass || fail "no $work/dl/saved.txt"

step ":wq with several tabs saves and exits cleanly"
xdotool key shift+semicolon
xdotool type --delay 5 "set auto_save.session true"
xdotool key Return
xdotool key shift+semicolon w q Return
expect_exit

step "restarting restores the session"
"$BIN" --basedir "$work/base" >>"$work/browser.log" 2>&1 &
browser_pid=$!
window=$(timeout "$TIMEOUT" xdotool search --sync --name "^dialogs - hackers-browser$" | head -1 || true)
[[ -n $window ]] && pass || fail "no restored window"

step ":quit exits cleanly"
[[ -n $window ]] && xdotool windowfocus --sync "$window"
xdotool key shift+semicolon q u i t Return
expect_exit

if (( failures > 0 )); then
    echo "smoke-test: $failures check(s) failed; browser log:" >&2
    tail -20 "$work/browser.log" >&2
    exit 1
fi
echo "smoke-test: all checks passed"
