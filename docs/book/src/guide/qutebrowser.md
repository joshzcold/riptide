# Moving from qutebrowser

Riptide uses qutebrowser's modes, default bindings, command names and setting names where they make sense, and reads several of its files unchanged.

## Files you can copy

| What | From qutebrowser | To riptide |
|---|---|---|
| Quickmarks | `~/.config/qutebrowser/quickmarks` | `<config>/quickmarks` |
| Bookmarks | `~/.config/qutebrowser/bookmarks/urls` | `<config>/bookmarks/urls` |
| Userscripts | `~/.local/share/qutebrowser/userscripts/` | `<config>/userscripts/` or `<data>/userscripts/` |
| Greasemonkey scripts | `~/.local/share/qutebrowser/greasemonkey/` | `<data>/greasemonkey/` or `<config>/greasemonkey/` |

`riptide --paths` prints `<config>` and `<data>`. If you already have quickmarks in riptide, merge rather than overwrite: both files are one `name url` per line.

History is imported rather than copied: `:history-import` reads qutebrowser's `history.sqlite` from its default data directory (or a path you give). Importing twice adds nothing new.

## Translating `config.py`

Riptide doesn't run `config.py`. Write `config.toml` for plain settings and bindings, or `config.lua` for anything with logic. In `config.lua`, `c` works like qutebrowser's:

| qutebrowser (`config.py`) | riptide (`config.lua`) |
|---|---|
| `c.tabs.new_position.related = "last"` | `c.tabs.new_position.related = "last"` |
| `c.auto_save.session = True` | `c.auto_save.session = true` |
| `c.editor.command = ["kitty", "-e", "nvim", "{}"]` | `c.editor.command = { "kitty", "-e", "nvim", "{file}" }` |
| `config.bind("J", "tab-prev")` | `rt.bind("J", "tab-prev")` |
| `config.bind("<Ctrl-e>", "edit-text", mode="insert")` | `rt.bind("<Ctrl-e>", "edit-text", "insert")` (`open-editor` is the same command) |
| `c.aliases["w"] = "session-save"` | `local a = rt.get("aliases"); a.w = "session-save"; c.aliases = a` |
| `config.set("content.geolocation", True, "https://example.com")` | `rt.set("content.geolocation", "true", "https://example.com")` |
| `if platform.system() == "Darwin":` | `if rt.platform == "macos" then … end` |
| `config.load_autoconfig()` | Not needed: `autoconfig.toml` always loads first. |

Things to check after translating:

- **Unknown commands are rejected** when the config loads, with the file and line in the status bar. That's how you find commands riptide doesn't have yet; `:help` lists the ones it does.
- **Editor placeholders:** qutebrowser's `{}` is `{file}` in `editor.command`, which also has `{line}`, `{column}` and `{column0}`.
- **Settings that don't exist are errors too.** The [settings reference](../reference/settings.md) lists every one.

## `autoconfig.yml`

qutebrowser saves `:set`, `:bind` and per-site permission answers to `autoconfig.yml`. Riptide's equivalent is `autoconfig.toml`, written by `:set`, `:bind`, `:unbind` and the `A`/`N` answers to permission prompts. There's no converter: move the parts you want into `config.toml` or `config.lua`, using [per-site settings](prompts.md#per-site-settings) for the permission lists.
