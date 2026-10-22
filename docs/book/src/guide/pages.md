# Pages: dark mode, spell checking, DRM

## Dark mode

- `colors.webpage.preferred_color_scheme` (`auto`, `light` or `dark`) is what pages see in `prefers-color-scheme`. It applies immediately.
- `colors.webpage.darkmode.enabled = true` renders light pages dark with Chromium's automatic dark mode. It applies at once to open tabs, and can be set per site, e.g. `:set -u docs.example.com colors.webpage.darkmode.enabled false` for a site that's already dark.
- `colors.webpage.bg` is the color a new tab shows before its page paints; set it to a dark color such as `#1e1e2e` so loading pages don't flash white.

## Saving and printing

| Command | What it does |
|---|---|
| `:print` | Opens the system print dialog |
| `:print --pdf ~/page.pdf` | Saves the page as a PDF, backgrounds included |
| `:screenshot ~/shot.png` | Saves what the tab shows as an image; `.jpg` and `.webp` work too. It won't replace an existing file without `--force`. |
| `gf`, `:view-source` | Shows the page's source in a new tab |
| `:debug-dump-page ~/page.html` | Saves the page's current HTML, as scripts have changed it |
| `wi`, `:devtools` | Opens or closes the developer tools; `:devtools-focus` brings them to the front |

## Spell checking

Off by default. Turn it on with a list of languages, e.g. `c.spellcheck.languages = { "en-US", "de-DE" }` in `config.lua` or `:set spellcheck.languages ["en-US"]`. Chromium then downloads each dictionary once from Google (`redirector.gvt1.com`) and underlines mistakes as you type.

To keep that request away from Google, install dictionaries yourself first: `:spell-install en-US de-DE`, or `:spell-install` alone to pick from the 47 languages (type a code or a name such as `german`) downloads them from Chromium's own dictionary repository at a fixed version, checks each file against a checksum built into riptide, saves it where Chromium looks, and turns the language on. Chromium then uses that file and fetches nothing.

From the keyboard, in a text field:
- `Alt-s` (`:spell-suggest`) lists fixes for the word at the text cursor as completions. `Tab` picks one, `Return` replaces the word, and you're back in insert mode.
- `:spell-add` adds that word to your dictionary.
- `]s` and `[s` in normal mode (`Escape` from the field keeps the cursor in it) move the text cursor to the next or previous misspelled word, so `i` and `Alt-s` fix it. With none that way, the cursor stays where it was.

Right-click suggestions work too.

## Widevine (DRM)

Off by default. With `c.content.widevine = true` and a restart, Chromium downloads Google's Widevine CDM (about 21 MB) into the data directory, and it loads from the next start. Chromium has no switch for a single component, so component updates are on during that one run, and they go back off once the CDM is installed.

Every `content.widevine_update_interval` days (default 7; 0 never), one start turns component updates on again so Chromium can update the CDM; a newer one loads from the start after. Turning `content.widevine` off deletes the CDM at the next start.

Google provides the Widevine CDM under its own terms. riptide doesn't distribute it or license it to you: turning `content.widevine` on means Chromium downloads it from Google for you, and the status bar says so when the download starts.

Limits: only VP9/AV1 streams work (prebuilt CEF has no H.264/AAC), and Linux Widevine is the software-only level (L3) that services often cap at lower resolutions.
