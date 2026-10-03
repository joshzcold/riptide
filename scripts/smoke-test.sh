#!/usr/bin/env bash
# End-to-end check of the real browser on a throwaway X display. The window
# title is set to "{mode}::{page title}", so the test can wait for the
# browser's mode before each key instead of guessing with sleeps, and the test
# pages report their state through document.title.
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
    # SMOKE_LOG=path keeps the browser log for debugging.
    [[ -n ${SMOKE_LOG:-} && -f $work/browser.log ]] && cp "$work/browser.log" "$SMOKE_LOG"
    [[ -n $browser_pid ]] && kill "$browser_pid" 2>/dev/null || true
    [[ -n $xvfb_pid ]] && kill "$xvfb_pid" 2>/dev/null || true
    [[ -n ${http_pid:-} ]] && kill "$http_pid" 2>/dev/null || true
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

cat >"$work/editor.html" <<'EOF'
<!doctype html><title>editor</title>
<textarea id="t" style="position:fixed;top:0;left:0;width:300px;height:60px"></textarea>
<script>t.addEventListener('input', () => { document.title = 'v=' + t.value; });</script>
EOF

cat >"$work/caret.html" <<'EOF'
<!doctype html><title>caret</title>
<p style="font-size:20px">The quick brown fox jumps over the lazy dog.</p>
<script>document.addEventListener('selectionchange', () => { document.title = 'sel=' + getSelection(); });</script>
EOF

cat >"$work/gm.html" <<'EOF'
<!doctype html><title>gm</title><body>x
<script>document.title = 'start=' + (window.__gmStart || 'no');</script>
EOF

cat >"$work/dialogs.html" <<'EOF'
<!doctype html><title>dialogs</title>
<button onclick="document.title = 'confirm ' + confirm('Sure?')">confirm</button>
<a download="saved.txt" href="data:text/plain,hello">download</a>
EOF

# Content blocking only applies to http(s), so these pages get a local server.
mkdir -p "$work/http/ads"
cat >"$work/http/adblock.html" <<'EOF'
<!doctype html><title>adblock</title>
<script>let blocked = 'no', allowed = 'no';</script>
<script src="ads/banner.js"></script>
<script src="app.js"></script>
<script>document.title = `ads b=${blocked} a=${allowed}`;</script>
EOF
echo "blocked = 'yes';" >"$work/http/ads/banner.js"
echo "allowed = 'yes';" >"$work/http/app.js"
printf '! test list\n/ads/banner.js\n' >"$work/filters.txt"

Xvfb -displayfd 3 -screen 0 1280x900x24 3>"$work/display" 2>/dev/null &
xvfb_pid=$!
for _ in $(seq 50); do [[ -s $work/display ]] && break; sleep 0.1; done

python3 - "$work/http" "$work/port" 2>/dev/null <<'SERVER' &
import functools, http.server, sys
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=sys.argv[1])
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
open(sys.argv[2], "w").write(str(server.server_port))
server.serve_forever()
SERVER
http_pid=$!
for _ in $(seq 50); do [[ -s $work/port ]] && break; sleep 0.1; done
http="http://127.0.0.1:$(cat "$work/port")"
export DISPLAY=":$(cat "$work/display")"

failures=0
step() { printf '  %-56s' "$1"; }
pass() { echo "ok"; }
fail() { echo "FAIL ($1)"; failures=$((failures + 1)); }

name() { xdotool getwindowname "$window" 2>/dev/null || true; }
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

# A userscript and an "editor" for :spawn -u and :open-editor.
mkdir -p "$work/base/config/userscripts"
cat >"$work/base/config/userscripts/us" <<EOF
#!/bin/sh
printf '%s|%s' "\$QUTE_URL" "\$QUTE_MODE" >"$work/us.out"
echo "open -t file://$work/second.html" >>"\$QUTE_FIFO"
EOF
cat >"$work/editor.sh" <<'EOF'
#!/bin/sh
printf 'edited text\n' >"$1"
EOF
chmod +x "$work/base/config/userscripts/us" "$work/editor.sh"
# Greasemonkey scripts for gm.html only: one before the page's scripts, one after.
mkdir -p "$work/base/data/greasemonkey"
cat >"$work/base/data/greasemonkey/start.user.js" <<'EOF'
// ==UserScript==
// @name   Start
// @include file://*/gm.html
// @run-at document-start
// ==/UserScript==
window.__gmStart = GM_info.script.name;
EOF
cat >"$work/base/data/greasemonkey/end.user.js" <<'EOF'
// ==UserScript==
// @include file://*/gm.html
// ==/UserScript==
GM_addStyle('body { color: rgb(1, 2, 3); }');
document.title += ' end=' + getComputedStyle(document.body).color;
EOF

echo "smoke-test on $DISPLAY"
# A private basedir keeps the test away from the real config and profile.
mkdir -p "$work/base/config"
cat >"$work/base/config/config.lua" <<EOF
hb.bind("X", "open -t file://$work/second.html")
c.downloads.location.directory = "$work/dl"
c.downloads.location.prompt = false
c.window.title_format = "{mode}::{current_title}"
c.content.blocking.adblock.lists = { "file://$work/filters.txt" }
c.editor.command = { "$work/editor.sh", "{file}" }
EOF
HB_LOG=${HB_LOG:-info} "$BIN" --basedir "$work/base" "file://$work/page.html" >"$work/browser.log" 2>&1 &
browser_pid=$!

step "window opens and loads the page"
window=$(timeout "$TIMEOUT" xdotool search --sync --name "^normal::ready$" | head -1 || true)
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
step "focusing a field enters insert mode"
xdotool key a
wait_mode insert || true
xdotool type --delay 20 abc
expect_title "s=0 k=3 v=abc c=no"

step "Escape leaves insert mode, 5j scrolls 200px"
xdotool key Escape
wait_mode normal || true
xdotool key 5 j
expect_title "s=200 k=3 v=abc c=no"

step "G scrolls to the bottom without page keys"
xdotool key shift+g
nap 0.5
title=$(page_title)
[[ $title =~ ^s=([0-9]+)\ k=3\  && ${BASH_REMATCH[1]} -gt 3000 ]] && pass || fail "title was '$title'"

# Waits for the page's scroll position to satisfy a test, e.g. "-gt 3000".
expect_scroll() {
    local title=""
    for _ in $(seq $((TIMEOUT * 10))); do
        title=$(page_title)
        [[ $title =~ ^s=([0-9]+)\  ]] && (( BASH_REMATCH[1] $1 )) && { pass; return; }
        sleep 0.1
    done
    fail "title was '$title'"
}

step "\`a sets a mark and 'a jumps back to it"
xdotool key grave a g g
nap 0.3
xdotool key apostrophe a
expect_scroll "> 3000"
step "'' returns to where the jump started"
xdotool key apostrophe apostrophe
expect_scroll "== 0"

step "qa records a macro and @a replays it"
xdotool key q a 5 j q g g
nap 0.3
xdotool key at a
expect_scroll "== 200"

# Matches the first page's title whatever its scroll state.
expect_first_page() {
    for _ in $(seq $((TIMEOUT * 10))); do
        [[ $(page_title) == s=* ]] && { pass; return; }
        sleep 0.1
    done
    fail "title was '$(name)'"
}

step "f + label clicks the button for real"
hint s
title=""
for _ in $(seq $((TIMEOUT * 10))); do
    title=$(page_title)
    [[ $title == *" k=3 v=abc c=trusted" ]] && break
    sleep 0.1
done
[[ $title == *" k=3 v=abc c=trusted" ]] && pass || fail "title was '$title'"

step ":open -t opens and focuses a new tab"
run "open -t file://$work/second.html"
expect_title "second"
# Under Xvfb, a key sent just as a tab opened from the command line finishes
# loading is sometimes dropped before CEF sees it (docs/PLAN.md, M14 gaps).
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

step "pinned tabs refuse d but close with --force"
xdotool key ctrl+p
nap 0.3
xdotool key d
nap 0.5
title=$(page_title)
run "tab-close --force"
if [[ $title == "second" ]]; then expect_first_page; else fail "d closed the pinned tab ('$title')"; fi

step "a key bound in config.lua works"
xdotool key d shift+x
expect_title "second"

step ":set persists to autoconfig.toml"
run "set messages.timeout 5000"
for _ in $(seq $((TIMEOUT * 10))); do
    grep -q '"messages.timeout" = 5000' "$work/base/config/autoconfig.toml" 2>/dev/null && break
    sleep 0.1
done
grep -q '"messages.timeout" = 5000' "$work/base/config/autoconfig.toml" 2>/dev/null && pass || fail "autoconfig.toml not written"

step ":open completes from history with Tab"
run "open -t about:blank"
expect_title "about:blank"
xdotool key shift+semicolon
wait_mode command || true
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
run "help :open"
expect_title "hackers-browser help"

step ":changelog opens the bundled changelog"
run "changelog"
expect_title "hackers-browser changelog"

step "a second invocation hands its arguments to this browser"
code=0
"$BIN" --basedir "$work/base" "file://$work/second.html" ":tab-focus -1" || code=$?
if (( code == 0 )); then expect_title "second"; else fail "second invocation exited with $code"; fi

step "web pages can't see or embed hb:// UI pages"
run "open file://$work/isolation.html"
expect_title "hb=undefined frame=empty"

step ":adblock-update blocks requests from the filter list"
run "adblock-update"
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/base/data/adblock/engine.dat ]] && break; sleep 0.1; done
nap 0.5
run "open $http/adblock.html"
expect_title "ads b=no a=yes"

step "a userscript gets QUTE_* and runs what it writes to QUTE_FIFO"
run "spawn -u us"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == second ]] && break; sleep 0.1; done
us=$(cat "$work/us.out" 2>/dev/null || true)
if [[ $(page_title) != second ]]; then
    fail "the FIFO command didn't run; title was '$(name)'"
elif [[ $us != "$http/adblock.html|command" ]]; then
    fail "QUTE_URL|QUTE_MODE was '$us'"
else
    pass
fi

step "Ctrl-e edits a text field in editor.command"
run "open file://$work/editor.html"
expect_title "editor"
hint a
wait_mode insert || true
xdotool type --delay 20 abc
xdotool key ctrl+e
expect_title "v=edited text"
xdotool key Escape
wait_mode normal || true

step "a macro types the keys it recorded into the page"
xdotool key q z i x y Escape q
expect_title "v=edited textxy"
xdotool key at z
expect_title "v=edited textxyxy"

step "caret mode moves and selects with the keyboard"
run "open file://$work/caret.html"
expect_title "caret"
xdotool key v
wait_mode caret || true
xdotool key w w v e e
expect_title "sel=brown fox"
xdotool key Escape
wait_mode normal || true

step "Greasemonkey scripts run at document-start and -end"
run "open file://$work/gm.html"
expect_title "start=Start end=rgb(1, 2, 3)"

step "a JavaScript confirm() is answered with y"
run "open file://$work/dialogs.html"
expect_title "dialogs"
hint a
wait_mode yesno || true
xdotool key y
expect_title "confirm true"

step "downloads save to downloads.location.directory"
hint s
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/dl/saved.txt ]] && break; sleep 0.1; done
[[ $(cat "$work/dl/saved.txt" 2>/dev/null) == hello ]] && pass || fail "no $work/dl/saved.txt"

step ":wq with several tabs saves and exits cleanly"
run "set auto_save.session true"
run "wq"
expect_exit

step "restarting restores the session"
"$BIN" --basedir "$work/base" >>"$work/browser.log" 2>&1 &
browser_pid=$!
window=$(timeout "$TIMEOUT" xdotool search --sync --name "^normal::dialogs$" | head -1 || true)
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
