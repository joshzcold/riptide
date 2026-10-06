# Themes and colors

`ui.theme` sets the colors of riptide's own parts: the tab bar, the status bar, completion, prompts and hint labels. Web pages keep their own colors (see [dark mode](../guide/pages.md) for those).

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

`:theme nord` switches and saves the choice, like `:set ui.theme nord`; `:theme` alone lists them, and `:theme <Tab>` completes the names. In a config file:

```toml
ui.theme = "gruvbox-dark"
```

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
