# Themes and colors

`ui.theme` sets the colors of riptide's own parts: the tab bar, the status bar, completion, prompts, hint labels, and the `riptide://history` and `riptide://downloads` pages (those pick up a new theme when they next load). Web pages keep their own colors (see [dark mode](../guide/pages.md) for those).

| Theme | Look |
|---|---|
| `riptide` | Dark blue and teal (the default) |
| `riptide-light` | The same, light |
| `gruvbox-dark`, `gruvbox-light` | Gruvbox |
| `catppuccin-mocha`, `catppuccin-latte` | Catppuccin |
| `nord` | Nord |
| `dracula` | Dracula |
| `solarized-dark`, `solarized-light` | Solarized |
| `tokyo-night` | Tokyo Night |

`:theme nord` switches and saves the choice, like `:set ui.theme nord`; `:theme` alone lists them, and `:theme <Tab>` completes the names. While you type or `Tab` through the names, riptide shows each theme as it's named; `Escape` goes back to the one you had. In a config file:

```toml
ui.theme = "gruvbox-dark"
```

`ui.theme = "auto"` follows your desktop's light or dark preference and switches as soon as it changes, using `ui.auto_theme.dark` and `ui.auto_theme.light`:

```toml
ui.theme = "auto"
"ui.auto_theme.dark" = "tokyo-night"
"ui.auto_theme.light" = "solarized-light"
```

If you set `colors.webpage.preferred_color_scheme` to `light` or `dark`, `auto` follows that instead, so riptide matches the pages.

## Your own themes

A file in `themes/` in the config directory adds a theme named after the file. `themes/kanagawa-dragon.toml` makes `kanagawa-dragon`, which `:theme`, `ui.theme` and `ui.auto_theme.*` then accept. Give a palette of base colors, and riptide works out every color from them as it does for the built-in themes (readable text included):

```toml
# ~/.config/riptide/themes/kanagawa-dragon.toml
[palette]
base = "#0a0c0f"      # status bar and headers: the darkest background
surface = "#181616"   # tab bar, completion and prompts
fg = "#c5c9c5"
accent = "#8ba4b0"    # https URLs, the selected completion, key hints
yellow = "#c4b28a"    # hint labels, prompt frames, matches
red = "#c4746e"       # errors
green = "#8a9a7b"     # insert mode
blue = "#658594"      # passthrough mode, pinned tabs

[colors]              # optional: any colors.* setting, without "colors."
"completion.item.selected.bg" = "#2d4f67"
"completion.item.selected.fg" = "#c8c093"
```

| Palette color | Required | If left out |
|---|---|---|
| `base`, `surface`, `fg`, `accent`, `yellow`, `red`, `green`, `blue` | yes | |
| `surface2`, `surface3` (alternating tabs) | no | `surface` mixed with a little `fg` |
| `muted` (descriptions) | no | `fg` mixed toward `surface` |
| `orange` (warnings) | no | between `red` and `yellow` |

Palette colors are `#rrggbb`; `[colors]` takes any CSS color. Theme files are read with the config, so after editing one run `:config-source`. A file with a mistake is skipped with a message saying what's wrong.

In `config.lua`, `rt.theme` defines a theme the same way:

```lua
rt.theme("kanagawa-dragon", {
  palette = {
    base = "#0a0c0f", surface = "#181616", fg = "#c5c9c5", accent = "#8ba4b0",
    yellow = "#c4b28a", red = "#c4746e", green = "#8a9a7b", blue = "#658594",
  },
  colors = { ["completion.item.selected.bg"] = "#2d4f67" },
})
c.ui.theme = "kanagawa-dragon"
```

### Themes from other programs

Two kinds of theme file made for other programs work as they are. Copy one into `themes/`, and its file name becomes the theme's name:

| File | What's read |
|---|---|
| `<name>.yaml` or `.yml`: a [base16](https://github.com/tinted-theming/home) scheme | `base00`…`base0F`, in either the older flat layout or under `palette:` |
| `<name>.py`: a qutebrowser theme | `c.colors.… = …` and `config.set("colors.…", …)` lines, with values as strings, variables (`base00 = "#…"`) or dict entries (`palette['bg']`). No Python is run. |

A qutebrowser theme's status bar, completion, hint, error, insert and passthrough colors become the palette, and every other `colors.*` setting riptide also has is used as it is. Many Neovim and terminal themes ship a base16 file; Kanagawa's is in `extras/base16/`.

## Changing single colors

`colors.*` settings change one color on top of the theme, with qutebrowser's names where there is one. An empty value (the default) uses the theme's. Colors are `#rrggbb`, `#rgb`, `rgb(…)`, `hsl(…)` or a CSS color name:

```toml
ui.theme = "nord"
"colors.statusbar.insert.bg" = "#2e7d32"
"colors.hints.bg" = "#ffd54f"
```

```lua
c.colors.tabs.selected.odd.bg = "#000000"
```

The [settings reference](../reference/settings.md) lists every `colors.*` setting. Every built-in theme keeps text readable: each text color has at least 4.5:1 contrast with its background (WCAG AA), which riptide's tests check.

## Fonts

`fonts.default_family` and `fonts.default_size` set riptide's font, and the per-part settings (`fonts.statusbar`, `fonts.tabs.selected`, `fonts.tabs.unselected`, `fonts.completion.entry`, `fonts.completion.category`, `fonts.prompts`, `fonts.hints`, `fonts.keyhint`) take a CSS `font` in which `default_size` and `default_family` stand for those two, as in qutebrowser:

```toml
"fonts.default_family" = '"JetBrains Mono", monospace'
"fonts.default_size" = "11pt"
"fonts.tabs.selected" = "bold default_size default_family"
```

The tab bar, the status bar and the completion rows grow to fit their fonts. `statusbar.padding` and `tabs.padding` add room around the text, in pixels like CSS `padding`:

```toml
"statusbar.padding" = "4px 8px"
"tabs.padding" = "2px 6px"
```

Padding or font sizes set in `ui.css` resize the bars too.

Pages have their own fonts: `fonts.web.family.standard`, `.fixed`, `.serif` and `.sans_serif` (empty keeps Chromium's) and `fonts.web.size.default`, `.default_fixed` and `.minimum` in pixels. Pages pick them up when they reload.

## Floating command line

`ui.overlay.position = "floating"` turns the command line into a box near the top of the page, like a command palette: what you type shows in the box, with its completions under it, and the key hints appear there too. `ui.overlay.width` sets the box's width in pixels.

```toml
"ui.overlay.position" = "floating"
"ui.overlay.width" = 900
```

The default, `docked`, keeps the list full width above the status bar, as in qutebrowser. Questions have their own setting, `prompt.position`.

## Hint labels

Hint labels take the theme's `colors.hints.*` and `fonts.hints`. `hints.radius` rounds their corners (in pixels, `0` for square) and `hints.padding` sets the room around the letters:

```toml
"hints.radius" = 0
"hints.padding" = "1px 4px"
```

For anything else, put CSS in `hints.css` in the config directory. Labels are `.label` elements and the typed part of a label is `.matched`; the page's own CSS can't reach them, and yours applies only to them:

```css
.label { box-shadow: 0 1px 3px rgb(0 0 0 / 40%); }
.matched { opacity: 0.5; }
```

`hints.css` is read again each time hints are shown.

## Custom CSS

`ui.css` in the config directory is added to riptide's tab bar, status bar and overlay after their own styles, for anything the settings don't cover. The theme's colors are there as CSS variables (`--rt-statusbar-bg`, `--rt-tabs-selected-bg`, … one per `colors.*` setting), so a rule can reuse them:

```css
/* ~/.config/riptide/ui.css */
.tab.selected { border-bottom: 2px solid var(--rt-prompts-border); }
#bar.insert { font-style: italic; }
```

While a question is up, the overlay's `body` has a class for what it's about: `prompt-dialog` (a page's `alert`, `confirm`, `prompt` or leave-page warning), `prompt-permission`, `prompt-login`, `prompt-download`, `prompt-certificate` or `prompt-confirm` (riptide asking before quitting, closing a pinned tab or opening another program). Setting `--prompt-accent` recolors the box's frame and keys:

```css
body.prompt-permission { --prompt-accent: #e06c75; }
body.prompt-download { --prompt-accent: #98c379; }
```

`content.user_stylesheets` lists CSS files for web pages; relative paths are in the config directory, and like other content settings it can be set per site:

```toml
"content.user_stylesheets" = ["readable.css"]

[per_domain."news.example.com"]
"content.user_stylesheets" = ["readable.css", "news.css"]
```

Both are read again within a second of being saved: open pages and bars change without a reload.
