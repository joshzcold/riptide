# Prompts, downloads and permissions

## Prompts

Everything that needs an answer appears in a box floating near the bottom of the page, one at a time. `prompt.position = "center"` puts the box in the middle of the page, `"docked"` puts it in a strip above the status bar, and `prompt.width` sets the box's width. The floating box lists each answer on its own line with its key, and you can click an answer instead of pressing the key:

- JavaScript `alert`, `confirm`, `prompt` and leave-page warnings
- HTTP logins (username, then a hidden password)
- where to save a download
- site permission requests (camera, microphone, location, notifications…)

| Mode | Keys |
|---|---|
| prompt (text) | type, readline keys (`Ctrl-w` deletes one path component), `Return` accepts, `Escape` cancels |
| yesno | `y` / `n`, `Return` (the default), `Escape` cancels |

## Permissions

For permission prompts:
- `y` allows once and `n` (or `Escape`) means "not now".
- `A` always allows and `N` always blocks. These are saved as per-site settings in `autoconfig.toml`, as in qutebrowser, so they survive restarts. That includes camera and microphone. Chromium also remembers `y` for its own permission prompts.

The `content.geolocation`, `content.notifications.enabled`, `content.media.audio_capture`, `content.media.video_capture`, `content.desktop_capture`, `content.mouse_lock` (pointer lock, as games use) and `content.register_protocol_handler` (a site offering to handle `mailto:` links) settings (`ask`, `true` or `false`) answer without asking.

Notifications from sites show on your desktop. `content.notifications.presenter = "messages"` shows them in riptide's status bar instead, starting with the site's origin unless `content.notifications.show_origin = false`.

## Links to other programs

A link riptide can't show, like `mailto:`, `magnet:` or `zoommtg:`, asks before going to your desktop's handler (`xdg-open`). `content.unknown_url_scheme_policy = "allow-all"` hands them over without asking, and `"disallow"` never does.

Untrusted TLS certificates (self-signed, expired, wrong host…) ask before the page loads: `y` loads it once, `A` always loads that site, `N` always blocks it. `content.tls.certificate_errors` (`ask`, `block` or `load-insecurely`) sets the default and can be set per site. `:debug-clear-ssl-errors` forgets the `y` answers given this session.

## Per-site settings

The permission settings above, `content.tls.certificate_errors`, `content.blocking.enabled`, `content.headers.user_agent` and the `content.*` settings listed in [Privacy](privacy.md#cookies-javascript-images-and-the-user-agent) can differ per site. The last matching pattern wins. Patterns are hosts (`example.com`, `*.example.com` for subdomains too), origins (`https://meet.example.com`) or match patterns (`*://*.example.com/app/*`):

```sh
:set -u https://meet.example.com content.media.video_capture true
:set -u *.example.org content.javascript.enabled false
```
```toml
[per_domain."*.example.com"]          # config.toml or autoconfig.toml
"content.blocking.enabled" = false
```
```lua
rt.set("content.geolocation", "false", "*.tracker.example")  -- config.lua
```

## Downloads

Downloads go to `downloads.location.directory`, or the system Downloads folder if that's empty (on Linux, `XDG_DOWNLOAD_DIR` or `~/.config/user-dirs.dirs`). Server-suggested names are reduced to a plain file name, existing files get ` (1)` appended, and typing an existing path asks before overwriting. Set `downloads.location.prompt = false` to skip the question. The status bar shows `↓2 41%` while downloads run.

| Command | |
|---|---|
| `:download [url]` | Download a URL, or the current page |
| `;d` | Hint a link to download |
| `:download-cancel`, `:download-open` | The newest running / finished download, or the one given as a count (`2:download-open`) |
| `:download-retry` | Start the newest failed or cancelled download again |
| `:download-remove [--all]` | Take a download off the list, cancelling it if it's running; `--all` takes every finished one, like `:download-clear` |
| `:download-delete` | Delete the newest finished download's file |
| `:download-clear` | Forget finished downloads |
| `:downloads` | A page listing this session's downloads with their numbers and progress |

In the "Save file to" prompt, `Tab` completes file and directory names, as in a shell, and `Alt-e` picks the folder with `fileselect.folder.command` (see below). `Ctrl-x` opens the file instead of keeping it: it downloads to a temporary folder and opens with `downloads.open_dispatcher` or your desktop's default; `:prompt-open-download zathura` names a program. `Alt-y` copies the download's URL, in any prompt that has one.

`:download-open` uses `downloads.open_dispatcher` if you set one (`{}` is the file, or it goes at the end), and the system's opener (`xdg-open`, `open` or `start`) otherwise.

| Setting | What it does |
|---|---|
| `downloads.location.suggestion` | What the save prompt starts with: folder and name (`both`), the folder (`path`), or the name (`filename`; a bare name saves into the download folder) |
| `downloads.location.remember` | Start the prompt in the folder the last download went to (on by default) |
| `downloads.remove_finished` | Take finished downloads off the list after this many milliseconds (`-1`, the default, keeps them) |

## Choosing files to upload

Upload fields open Chromium's file dialog. To use a terminal file manager instead, set `fileselect.handler = "external"`. Riptide runs the command for the field (`fileselect.single_file.command`, `fileselect.multiple_files.command` or `fileselect.folder.command`) with `{}` replaced by a file to write the chosen paths to, one per line. The defaults run ranger in xterm, as qutebrowser does. For yazi in foot:

```toml
fileselect.handler = "external"
fileselect.single_file.command = ["foot", "yazi", "--chooser-file={}"]
fileselect.multiple_files.command = ["foot", "yazi", "--chooser-file={}"]
```
