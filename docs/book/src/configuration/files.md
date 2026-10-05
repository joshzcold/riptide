# Config files

Run `riptide --paths` to see where config and data live. All config files are optional and load in this order (later wins):

| File | Purpose |
|---|---|
| `autoconfig.toml` | Written by `:set`, `:bind` and `:unbind`. Don't edit it by hand. |
| `config.toml` | Declarative settings and bindings. See the [example](#example-configtoml) below. |
| `config.lua` | The same, as a Lua 5.4 program. See [Lua](lua.md). |

The directories for each platform are listed in [Installing](../guide/installing.md#where-things-live).

Every setting is listed in the [settings reference](../reference/settings.md), and the completion popup lists them as you type `:set `. In the browser:

- `:set hints.chars asdf` changes a setting; `:set hints.uppercase!` toggles one; `:set hints.chars` shows the value.
- `:bind <Ctrl-x> tab-close` adds a binding (`--mode insert` for other modes); `:bind <Ctrl-x>` shows one; `:unbind d` removes one.
- `:config-source` reloads every file. Errors show in the status bar with `file:line`, and the rest of the file still applies.

Per-site values use `[per_domain."<pattern>"]` tables; see [Per-site settings](../guide/prompts.md#per-site-settings).

## Example `config.toml`

```toml
{{#include ../../../config.example.toml}}
```
