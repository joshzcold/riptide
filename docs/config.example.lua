-- Example config.lua for hackers-browser. Loaded after config.toml.
-- Copy to the config directory (run `hackers-browser --paths` to find it).
--
--   c.<setting> = value      set an option (same names as config.toml)
--   hb.get(name)             read an option's current value
--   hb.bind(keys, command [, mode])
--   hb.unbind(keys [, mode])
--   hb.platform              "linux", "macos" or "windows"
--   hb.config_dir, hb.data_dir, hb.version
--   require("name")          loads name.lua or lua/name.lua from the config dir

c.hints.chars = "asdfjkl"

-- One file for every machine.
if hb.platform == "macos" then
  c.hints.uppercase = true
end

-- Build settings with code.
local engines = hb.get("url.searchengines")
engines.rs = "https://docs.rs/releases/search?query={}"
engines.crates = "https://crates.io/search?q={}"
c.url.searchengines = engines

for i, page in ipairs({ "news", "mail" }) do
  hb.bind("g" .. i, "open -t https://example.com/" .. page)
end

hb.bind("<Ctrl-e>", "mode-leave", "insert")

-- Keep machine-specific settings out of version control:
-- pcall(require, "local")
