# reading-list

A riptide plugin template: save pages to read later with `<Space>r`, and
`:reading-list` shows them.

```lua
-- config.lua
rt.pack.add({ "https://github.com/you/reading-list", opts = { key = "<Space>r" } })
```

## Layout

| Path | |
|---|---|
| `riptide-plugin.toml` | name, description, permissions |
| `lua/reading-list/init.lua` | the module; `setup(opts)` runs when it loads |
| `test/*_spec.lua` | tests, run by `riptide --plugin-test .` |
| `.github/workflows/test.yml` | runs the tests on every push |

`riptide --lua-types > rt.meta.lua` gives lua-language-server the API.
