# Pages: dark mode, spell checking, DRM

## Dark mode

- `colors.webpage.preferred_color_scheme` (`auto`, `light` or `dark`) is what pages see in `prefers-color-scheme`. It applies immediately.
- `colors.webpage.darkmode.enabled = true` renders light pages dark with Chromium's automatic dark mode. It takes effect after a restart: it's a Chromium switch, so `config.toml`/`config.lua` are read before Chromium starts.

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

Off by default. Turn it on with a list of languages, e.g. `c.spellcheck.languages = { "en-US", "de-DE" }` in `config.lua` or `:set spellcheck.languages '["en-US"]'`. Chromium downloads each dictionary once from Google (`redirector.gvt1.com`) and underlines mistakes as you type.

From the keyboard, in a text field:
- `:spell-suggest` lists fixes for the word at the text cursor as completions. `Tab` picks one, `Return` replaces the word, and you're back in insert mode.
- `:spell-add` adds that word to your dictionary.

Nothing is bound by default. For example, `rt.bind("<Ctrl-s>", "spell-suggest", "insert")`. Right-click suggestions work too.

## Widevine (DRM)

Off by default. With `c.content.widevine = true` and a restart, Chromium downloads Google's Widevine CDM (about 21 MB) into the data directory, and it loads from the next start. Chromium has no switch for a single component, so component updates are on during that one run. Once the CDM is installed they go back off, and the CDM itself isn't updated. To update or remove it, delete `<data>/WidevineCdm`.

Limits: only VP9/AV1 streams work (prebuilt CEF has no H.264/AAC), Linux Widevine is the software-only level (L3) that services often cap at lower resolutions, and this hasn't been checked against Google's Widevine terms for third-party browsers.
