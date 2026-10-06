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
