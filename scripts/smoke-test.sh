#!/usr/bin/env bash
# End-to-end check of the real browser on a throwaway X display. The window
# title is set to "{mode}::{page title}", so the test can wait for the
# browser's mode before each key instead of guessing with sleeps, and the test
# pages report their state through document.title.
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
    [[ -n ${http_pid:-} ]] && kill "$http_pid" 2>/dev/null || true
    [[ -n ${tls_pid:-} ]] && kill "$tls_pid" 2>/dev/null || true
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
<iframe id="f" src="riptide://ui/statusbar.html"></iframe>
<script>
setTimeout(() => {
  let frame;
  try {
    const d = document.getElementById('f').contentDocument;
    frame = d && d.getElementById('bar') ? 'ui-loaded' : 'empty';
  } catch (e) { frame = 'cross-origin'; }
  document.title = `rt=${typeof window.rt} frame=${frame}`;
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

cat >"$work/search.html" <<'EOF'
<!doctype html><title>search</title>
<div style="height:2000px"></div><p>first needle</p>
<div style="height:2000px"></div><p>second needle</p><div style="height:2000px"></div>
<script>addEventListener('scroll', () => { document.title = 's=' + Math.round(scrollY); });</script>
EOF

cat >"$work/nav1.html" <<'EOF'
<!doctype html><title>nav1</title><a href="nav2.html">Next »</a>
EOF
cat >"$work/nav2.html" <<'EOF'
<!doctype html><title>nav2</title><link rel="prev" href="nav1.html">
EOF

cat >"$work/scheme.html" <<'EOF'
<!doctype html><title>scheme</title>
<script>const q = matchMedia('(prefers-color-scheme: dark)'); const r = () => { document.title = 'dark=' + q.matches; }; r(); q.addEventListener('change', r);</script>
EOF

cat >"$work/private.html" <<'EOF'
<!doctype html><title>private</title>
EOF

cat >"$work/hook.html" <<'EOF'
<!doctype html><title>hook</title>
EOF

cat >"$work/frames.html" <<'EOF'
<!doctype html><title>frames</title>
<div style="height:120px"></div>
<iframe style="width:400px;height:200px" srcdoc="<button style='margin:40px' onclick=&quot;parent.document.title = 'inner clicked ' + event.isTrusted&quot;>inside</button>"></iframe>
EOF

cat >"$work/autofocus.html" <<'EOF'
<!doctype html><title>autofocus</title><input autofocus>
EOF

cat >"$work/links.html" <<'EOF'
<!doctype html><title>links</title>
<p><a href="#home" onclick="document.title='clicked home'">Home</a> <a href="#news" onclick="document.title='clicked news'">News</a>
<a href="#about" onclick="document.title='clicked about'">About us</a></p>
EOF

cat >"$work/follow.html" <<'EOF'
<!doctype html><title>follow</title><p>Some text, then <a href="nav2.html">the target link</a>.</p>
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
cat >"$work/http/geo.html" <<'EOF'
<!doctype html><title>geo</title>
<button onclick="navigator.geolocation.getCurrentPosition(() => { document.title = 'geo=ok'; }, (e) => { document.title = 'geo=' + (e.code === 1 ? 'denied' : 'allowed'); })">ask</button>
EOF
printf '! test list\n/ads/banner.js\n##.ad-banner\n##.late-ad\n127.0.0.1##.local-ad\n' >"$work/filters.txt"
cat >"$work/http/cosmetic.html" <<'EOF'
<!doctype html><title>cosmetic</title>
<div class="ad-banner">ad</div><div class="local-ad">ad</div><div class="content">text</div>
<script>
// An ad that arrives after the page has loaded.
setTimeout(() => { const ad = document.createElement('div'); ad.className = 'late-ad'; document.body.append(ad); }, 1000);
const shown = (c) => { const el = document.querySelector('.' + c); return el ? getComputedStyle(el).display : 'missing'; };
setInterval(() => { document.title = `banner=${shown('ad-banner')} local=${shown('local-ad')} content=${shown('content')} late=${shown('late-ad')}`; }, 200);
</script>
EOF

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

# An HTTPS server with a self-signed certificate, for the certificate prompt.
mkdir -p "$work/tls"
printf '<!doctype html><title>secret page</title>' >"$work/tls/index.html"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$work/tls/key.pem" -out "$work/tls/cert.pem" \
    -days 2 -subj "/CN=localhost" 2>/dev/null
python3 - "$work/tls" "$work/tls-port" 2>/dev/null <<'SERVER' &
import functools, http.server, ssl, sys
d = sys.argv[1]
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=d)
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(d + "/cert.pem", d + "/key.pem")
server.socket = ctx.wrap_socket(server.socket, server_side=True)
open(sys.argv[2], "w").write(str(server.server_port))
server.serve_forever()
SERVER
tls_pid=$!
for _ in $(seq 50); do [[ -s $work/tls-port ]] && break; sleep 0.1; done
http="http://127.0.0.1:$(cat "$work/port")"
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
# :edit-url gets a URL back; text fields get fixed text.
cat >"$work/editor.sh" <<EOF
#!/bin/sh
case "\$1" in
    *url.txt) printf 'file://$work/nav2.html\n' >"\$1" ;;
    *cmd.txt) printf ':open file://$work/second.html\n' >"\$1" ;;
    *) printf 'edited text\n' >"\$1" ;;
esac
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
rt.bind("X", "open -t file://$work/second.html")
rt.bind("<Ctrl-x>", "cmd-edit --run", "command")
c.downloads.location.directory = "$work/dl"
c.downloads.location.prompt = false
c.window.title_format = "{mode}::{current_title}"
c.content.blocking.adblock.lists = { "file://$work/filters.txt" }
c.editor.command = { "$work/editor.sh", "{file}" }
rt.command("second", function(args) rt.open("file://$work/second.html" .. args, "tab") end, "Open the second page")
rt.bind("gS", function() rt.run("open file://$work/nav1.html") end)
rt.on("load_finished", function(e)
  if e.url:find("hook.html", 1, true) then rt.run("open file://$work/nav2.html") end
end)
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

# Waits for the page's scroll position to satisfy a test, e.g. -gt 3000.
expect_scroll() {
    local title=""
    for _ in $(seq $((TIMEOUT * 10))); do
        title=$(page_title)
        [[ $title =~ ^s=([0-9]+)\  ]] && test "${BASH_REMATCH[1]}" "$1" "$2" && { pass; return; }
        sleep 0.1
    done
    fail "title was '$title'"
}

step "\`a sets a mark and 'a jumps back to it"
xdotool key grave a g g
nap 0.3
xdotool key apostrophe a
expect_scroll -gt 3000
step "'' returns to where the jump started"
xdotool key apostrophe apostrophe
expect_scroll -eq 0

step "qa records a macro and @a replays it"
xdotool key q a 5 j q g g
nap 0.3
xdotool key at a
expect_scroll -eq 200

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
expect_title "Riptide help"

step ":changelog opens the bundled changelog"
run "changelog"
expect_title "riptide changelog"

step ":history lists visited pages"
run "history"
expect_title "History"

step ":history-import reads qutebrowser's history"
python3 - "$work/qb-history.sqlite" <<'QB'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("CREATE TABLE History (url TEXT, title TEXT, atime INTEGER, redirect BOOLEAN)")
db.execute("INSERT INTO History VALUES ('https://imported.example/', 'Imported page', 1700000000, 0)")
db.commit()
QB
run "history-import $work/qb-history.sqlite"
for _ in $(seq $((TIMEOUT * 10))); do
    python3 -c "import sqlite3, sys; c = sqlite3.connect(sys.argv[1]); sys.exit(0 if c.execute(\"select count(*) from completion where url = 'https://imported.example/'\").fetchone()[0] else 1)" "$work/base/data/history.sqlite" && break
    sleep 0.1
done && pass || fail "the visit wasn't imported"

step "a second invocation hands its arguments to this browser"
code=0
"$BIN" --basedir "$work/base" "file://$work/second.html" ":tab-focus -1" || code=$?
if (( code == 0 )); then expect_title "second"; else fail "second invocation exited with $code"; fi

step "web pages can't see or embed riptide:// UI pages"
run "open file://$work/isolation.html"
expect_title "rt=undefined frame=empty"

step ":adblock-update blocks requests from the filter list"
run "adblock-update"
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/base/data/adblock/engine.dat ]] && break; sleep 0.1; done
nap 0.5
run "open $http/adblock.html"
expect_title "ads b=no a=yes"

step "element-hiding rules hide ads on the page"
run "open $http/cosmetic.html"
expect_title "banner=none local=none content=block late=none"

step "an 'always' permission answer is saved for the site"
run "open $http/geo.html"
expect_title "geo"
hint a
wait_mode yesno || true
xdotool key shift+a
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == geo=* ]] && break; sleep 0.1; done
geo=$(page_title)
for _ in $(seq $((TIMEOUT * 10))); do grep -q "per_domain.\"$http\"" "$work/base/config/autoconfig.toml" 2>/dev/null && break; sleep 0.1; done
if [[ $geo != geo=allowed && $geo != geo=ok ]]; then
    fail "the page got '$geo'"
elif ! grep -A1 "per_domain.\"$http\"" "$work/base/config/autoconfig.toml" | grep -q '"content.geolocation" = "true"'; then
    fail "autoconfig.toml has no per_domain entry"
else
    pass
fi

step "a userscript gets QUTE_* and runs what it writes to QUTE_FIFO"
run "spawn -u us"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == second ]] && break; sleep 0.1; done
us=$(cat "$work/us.out" 2>/dev/null || true)
if [[ $(page_title) != second ]]; then
    fail "the FIFO command didn't run; title was '$(name)'"
elif [[ $us != "$http/geo.html|command" ]]; then
    fail "QUTE_URL|QUTE_MODE was '$us'"
else
    pass
fi

step "hints can run a userscript or a program on a link"
run "open file://$work/nav1.html"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav1 ]] && break; sleep 0.1; done
rm -f "$work/us.out"
run "hint links userscript us"
wait_mode hint || true
xdotool key a
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/us.out ]] && break; sleep 0.1; done
from_userscript=$(cat "$work/us.out" 2>/dev/null || true)
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == second ]] && break; sleep 0.1; done
run "open file://$work/nav1.html"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav1 ]] && break; sleep 0.1; done
run "hint links spawn sh -c 'echo \"\$1\" > $work/hinted' sh"
wait_mode hint || true
xdotool key a
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/hinted ]] && break; sleep 0.1; done
from_spawn=$(cat "$work/hinted" 2>/dev/null || true)
if [[ $from_userscript != "file://$work/nav2.html|hints" ]]; then
    fail "the userscript got '$from_userscript'"
elif [[ $from_spawn != "file://$work/nav2.html" ]]; then
    fail ":spawn got '$from_spawn'"
else
    pass
fi

step ":spawn -o shows the output in a new tab"
run "spawn -o echo hello from spawn"
expect_title "echo output"
run "tab-close"

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

step "/ finds text in the page, n goes to the next match"
run "open file://$work/search.html"
expect_title "search"
xdotool key slash
wait_mode command || true
xdotool type --delay 20 needle
xdotool key Return
wait_mode normal || true
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) =~ ^s=([0-9]+)$ ]] && (( BASH_REMATCH[1] > 1000 )) && break; sleep 0.1; done
first=$(page_title)
xdotool key n
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) != "$first" ]] && break; sleep 0.1; done
second=$(page_title)
if [[ $first =~ ^s=([0-9]+)$ ]] && (( BASH_REMATCH[1] > 1000 )) && [[ $second =~ ^s=([0-9]+)$ ]] && (( BASH_REMATCH[1] > ${first#s=} )); then
    pass
else
    fail "scroll went from '$first' to '$second'"
fi

step "Return follows the link a search found"
run "open file://$work/follow.html"
expect_title "follow"
xdotool key slash
wait_mode command || true
xdotool type --delay 20 "target"
xdotool key Return
wait_mode normal || true
nap 0.3
xdotool key Return
expect_title "nav2"

step "]] and [[ follow next/prev links, Ctrl-a increments the URL"
run "open file://$work/nav1.html"
expect_title "nav1"
xdotool key bracketright bracketright
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav2 ]] && break; sleep 0.1; done
next=$(page_title)
xdotool key bracketleft bracketleft
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav1 ]] && break; sleep 0.1; done
prev=$(page_title)
nap 0.3
xdotool key ctrl+a
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav2 ]] && break; sleep 0.1; done
incremented=$(page_title)
[[ $next == nav2 && $prev == nav1 && $incremented == nav2 ]] && pass || fail "]] → '$next', [[ → '$prev', Ctrl-a → '$incremented'"

step "colors.webpage.preferred_color_scheme applies live"
run "open file://$work/scheme.html"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == dark=* ]] && break; sleep 0.1; done
run "set colors.webpage.preferred_color_scheme dark"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == dark=true ]] && break; sleep 0.1; done
dark=$(page_title)
run "set colors.webpage.preferred_color_scheme light"
expect_title "dark=false"
[[ $dark == dark=true ]] || fail "dark gave '$dark'"

step "an untrusted certificate asks before loading"
run "open https://127.0.0.1:$(cat "$work/tls-port")/"
wait_mode yesno || true
xdotool key y
expect_title "secret page"

step "Lua: a command, a key bound to a function, and a hook"
run "second"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == second ]] && break; sleep 0.1; done
command_page=$(page_title)
nap 0.3
xdotool key g shift+s
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav1 ]] && break; sleep 0.1; done
key_page=$(page_title)
run "open file://$work/hook.html"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == nav2 ]] && break; sleep 0.1; done
hook_page=$(page_title)
[[ $command_page == second && $key_page == nav1 && $hook_page == nav2 ]] && pass ||
    fail ":second → '$command_page', gS → '$key_page', hook → '$hook_page'"

step "+ and . zoom in, = resets, :jseval runs in the page"
run "open file://$work/nav1.html"
expect_title "nav1"
nap 0.3
xdotool key plus
nap 0.2
xdotool key period
nap 0.3
run "jseval document.title = 'z=' + Math.round(devicePixelRatio * 100)"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == z=* ]] && break; sleep 0.1; done
zoomed=$(page_title)
xdotool key equal
nap 0.3
run "jseval document.title = 'z=' + Math.round(devicePixelRatio * 100)"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == z=100 ]] && break; sleep 0.1; done
reset=$(page_title)
[[ $zoomed == z=125 && $reset == z=100 ]] && pass || fail "zoomed '$zoomed', reset '$reset'"

step "tabs.position moves the tab bar to the side, tabs.show hides it"
run "open file://$work/nav1.html"
expect_title "nav1"
# Each reading ends in its own tag, so a stale title isn't mistaken for it.
# It's taken a moment later: typing the command shows a hidden status bar.
size() {
    run "jseval setTimeout(() => document.title = innerWidth + 'x' + innerHeight + ' $1', 300)"
    for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == *" $1" ]] && break; sleep 0.1; done
    local t; t=$(page_title); echo "${t% *}"
}
top=$(size top)
run "set tabs.position left"
nap 0.5
left=$(size left)
run "set tabs.position top"
run "set tabs.show never"
nap 0.5
hidden=$(size hidden)
run "set tabs.show always"
nap 0.5
w=${top%x*}; h=${top#*x}
[[ $left == "$((w - 200))x$((h + 20))" && $hidden == "${w}x$((h + 20))" ]] && pass ||
    fail "top '$top', left '$left', hidden '$hidden'"

step "statusbar.show hides the status bar until a command is typed"
run "set statusbar.show never"
nap 0.5
bare=$(size bare)
xdotool key colon
wait_mode command || true
typing=$(mode)
xdotool key Escape
wait_mode normal || true
run "set statusbar.show always"
[[ $bare == "${w}x$((h + 20))" && $typing == command ]] && pass || fail "hidden '$bare', ':' gave mode '$typing'"

step "Ctrl-d deletes the selected history entry from :open completion"
visits() {
    python3 - "$work/base/data/history.sqlite" <<'PYQ'
import sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(db.execute("SELECT COUNT(*) FROM completion WHERE url LIKE '%/second.html%'").fetchone()[0])
PYQ
}
before=$(visits)
xdotool key o
wait_mode command || true
xdotool type --delay 20 "second.html"
nap 0.5
xdotool key Tab
nap 0.3
xdotool key ctrl+d
nap 0.5
xdotool key Escape
wait_mode normal || true
after=$(visits)
(( before > 0 )) && [[ $after == 0 ]] && pass || fail "entries for second.html: $before before, $after after"

step ":edit-url opens the URL the editor wrote"
run "open file://$work/nav1.html"
expect_title "nav1"
run "edit-url"
expect_title "nav2"

step ":cmd-edit --run runs the command line the editor wrote"
xdotool key colon
wait_mode command || true
xdotool type --delay 20 "open draft"
xdotool key ctrl+x
expect_title "second"

step "a field the page focuses on load doesn't take insert mode unless auto_load is on"
run "open file://$work/autofocus.html"
expect_title "autofocus"
nap 0.5
stayed=$(mode)
run "set input.insert_mode.auto_load true"
run "reload"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(mode) == insert ]] && break; sleep 0.1; done
entered=$(mode)
xdotool key Escape
wait_mode normal || true
run "set input.insert_mode.auto_load false"
[[ $stayed == normal && $entered == insert ]] && pass || fail "default '$stayed', with auto_load '$entered'"

step "session.lazy_restore loads a background tab only when it's shown"
lazy_visits() {
    python3 - "$work/base/data/history.sqlite" <<'PYQ'
import sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(db.execute("SELECT COUNT(*) FROM visits WHERE url LIKE '%second.html?lazy=1'").fetchone()[0])
PYQ
}
run "tab-only"
run "open file://$work/nav1.html"
expect_title "nav1"
run "open -t file://$work/second.html?lazy=1"
expect_title "second"
run "tab-focus 1"
expect_title "nav1"
run "session-save lazy"
run "set session.lazy_restore true"
run "session-load lazy"
nap 1.5
expect_title "nav1"
before=$(lazy_visits)
run "tab-focus 2"
expect_title "second"
nap 0.5
after=$(lazy_visits)
run "set session.lazy_restore false"
[[ $before == 1 && $after == 2 ]] && pass || fail "visits to the background tab: $before before showing it, $after after"

step ":screenshot saves the tab as a PNG and won't overwrite without --force"
run "screenshot $work/shot.png"
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/shot.png ]] && break; sleep 0.1; done
signature=$(head -c 8 "$work/shot.png" 2>/dev/null | od -An -tx1 | tr -d ' \n')
size=$(stat -c %s "$work/shot.png" 2>/dev/null || echo 0)
run "screenshot $work/shot.png"
nap 0.5
same=$(stat -c %s "$work/shot.png" 2>/dev/null || echo 0)
[[ $signature == 89504e470d0a1a0a && $size -gt 1000 && $same == "$size" ]] && pass ||
    fail "signature '$signature', size $size, after a second try $same"

step ":messages lists this session's messages"
run "messages"
expect_title "Messages"
run "tab-close"

step ":cmd-later, :insert-text and :click-element"
run "open file://$work/editor.html"
expect_title "editor"
run "cmd-later 1500 insert-text hi"
run "jseval t.focus()"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == v=hi ]] && break; sleep 0.1; done
inserted=$(page_title)
xdotool key Escape
wait_mode normal || true
run "open file://$work/links.html"
expect_title "links"
run "click-element css a[href='#news']"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == "clicked news" ]] && break; sleep 0.1; done
clicked=$(page_title)
[[ $inserted == v=hi && $clicked == "clicked news" ]] && pass || fail "inserted '$inserted', clicked '$clicked'"

step "hints reach into same-origin iframes"
run "open file://$work/frames.html"
expect_title "frames"
nap 0.5
hint a
expect_title "inner clicked true"

step "number hints filter by the text typed"
run "set hints.mode number"
run "open file://$work/links.html"
expect_title "links"
xdotool key f
wait_mode hint || true
xdotool type --delay 20 "ou"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == "clicked about" ]] && break; sleep 0.1; done
clicked=$(page_title)
run "set hints.mode letter"
[[ $clicked == "clicked about" ]] && pass || fail "title was '$clicked'"

step "hints.auto_follow never waits for Return; hints.selectors adds a group"
run "set hints.auto_follow never"
run "open file://$work/links.html"
expect_title "links"
nap 0.3
hint a
nap 0.5
waiting=$(page_title)
xdotool key Return
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == clicked* ]] && break; sleep 0.1; done
followed=$(page_title)
run "set hints.auto_follow unique-match"
run "set hints.selectors {\"news\": \"a[href='#news']\"}"
run "jseval document.title = 'links'"
expect_title "links"
run "hint news"
wait_mode hint || true
xdotool key a
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == "clicked news" ]] && break; sleep 0.1; done
group=$(page_title)
[[ $waiting == links && $followed == clicked* && $group == "clicked news" ]] && pass ||
    fail "before Return '$waiting', after '$followed', news group '$group'"

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

step "Tab completes paths in the download prompt"
mkdir -p "$work/dl/subdir"
run "set downloads.location.prompt true"
hint s
wait_mode prompt || true
# The name is "saved (1).txt" now; Ctrl-w stops at spaces as well as slashes.
xdotool key ctrl+w ctrl+w
xdotool type --delay 20 "su"
xdotool key Tab
xdotool type --delay 20 "via-tab.txt"
xdotool key Return
for _ in $(seq $((TIMEOUT * 10))); do [[ -s $work/dl/subdir/via-tab.txt ]] && break; sleep 0.1; done
run "set downloads.location.prompt false"
[[ -s $work/dl/subdir/via-tab.txt ]] && pass || fail "nothing in $work/dl/subdir"

step ":downloads lists this session's downloads"
run "downloads"
expect_title "Downloads"
run "tab-close"

step "T picks a tab by title from completion"
run "open -t file://$work/search.html"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == search || $(page_title) == s=* ]] && break; sleep 0.1; done
run "open -t about:blank"
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == about:blank ]] && break; sleep 0.1; done
xdotool key shift+t
wait_mode command || true
xdotool type --delay 10 "search"
nap 0.3
xdotool key Tab Return
for _ in $(seq $((TIMEOUT * 10))); do [[ $(page_title) == search || $(page_title) == s=* ]] && break; sleep 0.1; done
[[ $(page_title) == search || $(page_title) == s=* ]] && pass || fail "title was '$(name)'"

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

step ":open -p opens a private window that keeps no history"
run "open -p file://$work/private.html"
private=$(find_window '^normal::private$' "$window")
if [[ -z $private ]]; then
    fail "no private window"
else
    xdotool windowfocus --sync "$private"
    nap 0.5
    run "close"
    for _ in $(seq $((TIMEOUT * 10))); do xdotool getwindowname "$private" >/dev/null 2>&1 || break; sleep 0.1; done
    xdotool windowfocus --sync "$window"
    nap 0.3
    if python3 -c "import sqlite3, sys; c = sqlite3.connect(sys.argv[1]); sys.exit(any('private.html' in r[0] for r in c.execute('select url from visits')))" "$work/base/data/history.sqlite"; then
        pass
    else
        fail "the private page is in history"
    fi
fi

step ":wq with several tabs saves and exits cleanly"
run "set auto_save.session true"
run "wq"
expect_exit

step "restarting restores the session"
"$BIN" --basedir "$work/base" >>"$work/browser.log" 2>&1 &
browser_pid=$!
window=$(find_window "^normal::search$")
[[ -n $window ]] && pass || fail "no restored window"

step "confirm_quit asks before quitting with several tabs; n keeps the browser"
[[ -n $window ]] && xdotool windowfocus --sync "$window"
run "set confirm_quit [\"multiple-tabs\"]"
run "quit"
wait_mode yesno || true
asked=$(mode)
xdotool key n
wait_mode normal || true
nap 0.5
kill -0 "$browser_pid" 2>/dev/null && running=yes || running=no
[[ $asked == yesno && $running == yes ]] && pass || fail "mode '$asked', still running: $running"

step ":quit exits cleanly once confirmed"
run "quit"
wait_mode yesno || true
xdotool key y
expect_exit

if (( failures > 0 )); then
    echo "smoke-test: $failures check(s) failed; browser log:" >&2
    sed 's/\x1b\[[0-9;]*m//g' "$work/browser.log" | grep -vE "dbus|libva|vaapi|gpu" | tail -30 >&2
    exit 1
fi
echo "smoke-test: all checks passed"
