# Config files

Run `riptide --paths` to see where config and data live. All config files are optional and load in this order (later wins):

| File | Purpose |
|---|---|
| `autoconfig.toml` | Written by `:set`, `:bind` and `:unbind`. Don't edit it by hand. |
| `config.toml` | Declarative settings and bindings. See the [example](#example-configtoml) below. |
| `config.lua` | The same, as a Lua 5.4 program. See [Lua](lua.md). |

The directories for each platform are listed in [Installing](../guide/installing.md#where-things-live).

Every setting is listed in the [settings reference](../reference/settings.md), and the completion popup lists them as you type `:set `, with each one's current value. After a name it offers the values: `true`/`false`, a setting's choices, or its current and default values. In the browser:

- `:set hints.chars asdf` changes a setting; `:set hints.uppercase!` toggles one; `:set hints.chars` shows the value.
- `:bind <Ctrl-x> tab-close` adds a binding (`--mode insert` for other modes); `:bind <Ctrl-x>` shows one; `:unbind d` removes one.
- `:config-list-add` and `:config-list-remove` change one item of a list setting, and `:config-dict-add [--replace]` and `:config-dict-remove` one key of a map setting. For example, `:config-dict-add url.searchengines ddg https://duckduckgo.com/?q={}`. Like `:set`, they're saved in `autoconfig.toml`.
- `:config-diff` lists the settings that differ from their defaults, `:config-clear` puts them all back, and `:config-write-toml [--force]` writes your current settings to `config.toml`.
- `:config-edit` opens `config.lua` (or `config.toml`) in `editor.command` and reloads it when you close the editor.
- `:config-source` reloads every file. Errors show in the status bar with `file:line`, and the rest of the file still applies.

A few settings, such as `content.webgl` and `input.spatial_navigation`, only apply when riptide starts; the reference marks them "after a restart". When you change one, the status bar says so, and `:restart` restarts riptide with your tabs.

Per-site values use `[per_domain."<pattern>"]` tables; see [Per-site settings](../guide/prompts.md#per-site-settings).

## Example `config.toml`

```toml
{{#include ../../../config.example.toml}}
```
