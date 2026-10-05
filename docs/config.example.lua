-- Example config.lua for riptide. Loaded after config.toml.
-- Copy to the config directory (run `riptide --paths` to find it).
--
--   c.<setting> = value      set an option (same names as config.toml)
--   rt.get(name)             read an option's current value
--   rt.bind(keys, command [, mode])
--   rt.unbind(keys [, mode])
--   rt.platform              "linux", "macos" or "windows"
--   rt.config_dir, rt.data_dir, rt.version
--   require("name")          loads name.lua or lua/name.lua from the config dir

c.hints.chars = "asdfjkl"

-- One file for every machine.
if rt.platform == "macos" then
  c.hints.uppercase = true
end

-- Build settings with code.
local engines = rt.get("url.searchengines")
engines.rs = "https://docs.rs/releases/search?query={}"
engines.crates = "https://crates.io/search?q={}"
c.url.searchengines = engines

for i, page in ipairs({ "news", "mail" }) do
  rt.bind("g" .. i, "open -t https://example.com/" .. page)
end

rt.bind("<Ctrl-e>", "mode-leave", "insert")

-- Keep machine-specific settings out of version control:
-- pcall(require, "local")
