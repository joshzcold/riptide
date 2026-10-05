---
name: local-testing
description: Use when running, driving or screenshotting riptide to test a change (smoke test, Xvfb, xdotool, test pages, logs). Covers how to test without touching the user's own running browser, profile, sockets or display, plus the timing and input pitfalls of driving CEF from scripts.
---

# Testing riptide locally without disturbing the user

The user runs riptide for real on this machine. Every test must stay in its own profile, its own X display and its own processes.

## Rules

1. **Never `pkill -x riptide`, `killall riptide` or a bare `pkill -x Xvfb`.** They kill the user's browser and display. Stop only what you started:
   - by the PID you captured (`… & pid=$!` … `kill "$pid"`), or
   - by profile: `pkill -f -- '--basedir /path/you/used'`. CEF subprocesses carry `--user-data-dir=<basedir>/data`, so `pkill -f -- '--user-data-dir=<basedir>/data'` gets them too.
   - Beware: `pkill -f <pattern>` also matches your own shell when the pattern appears in the command line. Anchor it (`'^python3 .*server.py'`) or kill by PID.
2. **Always pass `--basedir <scratchpad dir>`.** Without it the test uses the user's config, history and cookies. Since single-instance support (M15) it would also hand its arguments to the user's running browser instead of starting a test instance.
3. **Use your own display:** `Xvfb :99` (check with `pgrep -f -- 'Xvfb :99'` first), and set `DISPLAY=:99` for every command. Never use `:0`, the user's screen. The smoke test picks a free display by itself.
4. **Remote-command sockets** live in `$XDG_RUNTIME_DIR/riptide/<hash>.sock`, one per data dir. Only delete a socket that nothing is listening on; a live one may be the user's browser.
5. Keep test pages, profiles and downloads in the session scratchpad, never in the repo or `$HOME`.

## The automated smoke test (safe by design)

```sh
./task smoke                 # build, then scripts/smoke-test.sh
CI=true ./scripts/smoke-test.sh   # same pauses as CI (3x slower)
```

It starts its own Xvfb (`-displayfd`), uses a temporary `--basedir`, kills only its own PID, and deletes everything afterwards. Prefer adding a step to it over ad-hoc manual checks when the behaviour should stay tested. `./task check` runs lint, unit tests and smoke.

## A manual session

```sh
SP=<scratchpad>; B=$SP/test-profile; mkdir -p "$B"
export DISPLAY=:99
pgrep -f -- 'Xvfb :99' >/dev/null || { Xvfb :99 -screen 0 1280x860x24 >/dev/null 2>&1 & }
RT_LOG=rt_cef=trace,info ./target/debug/riptide --basedir "$B" file://$SP/page.html >"$SP/run.log" 2>&1 &
pid=$!
sleep 4

# With c.window.title_format = "{mode}::{current_title}" in the test profile's
# config.lua, the window name says the mode and page title (what the smoke test does).
W=$(for w in $(xdotool search --name riptide); do
      [[ $(xdotool getwindowname "$w") == *" - Riptide" ]] && echo "$w"; done | head -1)
xdotool windowfocus --sync "$W"
xdotool key f; sleep 0.5; xdotool key a          # e.g. follow the first hint
xdotool getwindowname "$W"                        # page title + " - Riptide"
import -window root "$SP/shot.png"                # screenshot, then crop:
convert "$SP/shot.png" -crop 1280x20+0+0   "$SP/tabbar.png"
convert "$SP/shot.png" -crop 1280x22+0+779 "$SP/statusbar.png"

kill "$pid"                                       # only yours
```

The window is 1280×800: the tab bar is at y 0–20, the page at y 20–780 and the status bar at y 780–800, with the overlay (completion and prompts) just above the status bar.

Useful checks:
- **Page state:** have the test page write it into `document.title` (e.g. `s=${scrollY} k=${keys}`) and read it with `xdotool getwindowname`. This needs no screenshots and works in loops.
- **Logs:** `RT_LOG=rt_cef=trace` logs every key, with `consumed` and `editable`. Mode changes log at debug. Strip colours before grepping: `sed 's/\x1b\[[0-9;]*m//g' run.log | grep 'mode changed'`.
- **Remote commands:** `./target/debug/riptide --basedir "$B" ':open -t x'` drives the running test instance, using the same `--basedir`.
- **Pages over HTTP** (auth, favicons, downloads, permissions): `python3 -m http.server 8766 --bind 127.0.0.1 --directory "$SP/pages" &`. Use a small custom server for 401s or `Content-Disposition`. Bind to 127.0.0.1 only, and stop it by PID.

## Pitfalls seen so far

- **`xdotool search --sync` can abort with `BadWindow`** when one of Chromium's short-lived helper windows disappears mid-search. Poll `xdotool search --name … 2>/dev/null` in a loop instead (the smoke test's `find_window`).
- **A `pkill -f` pattern that appears in your own command line kills your own shell.** Kill by PID wherever you can.
- **Keys sent right as a tab opened from a typed `:open -t` finishes loading can be lost** under Xvfb, before CEF sees them (see the M14 notes in docs/PLAN.md). Pause 0.3 s after such a step, or retry the first key until the mode changes (the smoke test's `run` retries `:`).
- **Windows stack at the origin without a window manager.** Focus the one you want with `xdotool windowfocus --sync`; `import -window` of a covered window may fail.
- **Driving the browser without keys:** `./target/debug/riptide --basedir "$B" ':some-command'` runs a command in the running test instance (insert mode included), which avoids key-timing problems entirely.
- **Type only after the browser is ready for it.** Keys sent before a prompt, hint labels or a new tab is ready land in normal mode, and the stray letters run commands (`m` = quickmark, `b` = `:quickmark-load`, `o` = `:open`). Wait for a title change or sleep generously, more on CI.
- **Poll, don't sleep once.** Wait for a file, title or process with a loop and a timeout. One-shot `sleep 0.5; check` steps are flaky.
- **Prefer hint clicks to `xdotool click` for page content.** On CI the xdotool click missed. A hint sends a real CEF mouse event that doesn't depend on window placement. Tab-bar mouse tests via xdotool work locally only.
- **JavaScript dialogs block input to the page.** Prompt keys go through the status bar's browser instead, so test prompts with keys, not page clicks.
- **HTML5 drag and drop can't be driven by xdotool;** the tab bar uses pointer events, which can.
- **`file://` vs `http://`:** use HTTP for anything origin-dependent (auth, favicons, permissions).
- **Chromium remembers permission answers per site** in the profile. Start from a fresh `--basedir` to test a prompt again.
- **Heredocs:** a test page containing a line `EOF` ends an outer `<<'EOF'`. Use a distinct delimiter (`<<'HTML'`, `<<'PYEOF'`).
- **A display number may already be taken by your own earlier Xvfb.** A second `Xvfb :99` fails quietly; read the PID from `/tmp/.X99-lock` and reuse or kill that one, or let the smoke test pick a free display with `-displayfd`.
- **If the test instance dies, the next `':command'` starts a new one** (the socket is gone) instead of failing. Check the original PID is still alive after each command.
- **Start a long-lived test instance as a tracked background task** (`exec … --basedir "$B" … > log 2>&1`), not with `( … & )` in a subshell, which can leave it without its output or kill it with the shell.
- **macOS and Windows** builds can't launch the browser yet (no app bundle or installer). CI only builds and unit-tests them, plus `--version` and `--paths`.
