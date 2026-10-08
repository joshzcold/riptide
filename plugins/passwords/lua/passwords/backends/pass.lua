-- pass and gopass: an entry belongs to a site when a part of its path is the
-- site, e.g. websites/example.com/alice. The password is the first line; the
-- username is a "login:", "user:", "username:" or "email:" line, or else the
-- path part after the site.
local site = require("passwords.site")

local M = {}

local USER_KEYS = { login = true, user = true, username = true, email = true }

local function parts_of(entry)
  local parts = {}
  for part in entry:gmatch("[^/]+") do
    table.insert(parts, part)
  end
  return parts
end

local function username_from(rest, entry)
  for _, line in ipairs(site.lines(rest)) do
    local key, value = line:match("^%s*([%a_-]+)%s*:%s*(.-)%s*$")
    if key and value ~= "" and USER_KEYS[key:lower()] then
      return value
    end
  end
  local parts = parts_of(entry)
  if #parts >= 2 and site.saved_host(parts[#parts - 1]) then
    return parts[#parts]
  end
  return nil
end

-- The store's entries; pass keeps them as .gpg files.
local LIST_PASS = [[
d="${RT_STORE:-${PASSWORD_STORE_DIR:-$HOME/.password-store}}"
case $d in "~/"*) d="$HOME/${d#\~/}" ;; esac
cd "$d" && find -L . -name '*.gpg' -print
]]

function M.new(opts)
  local gopass = opts.backend == "gopass"
  local cmd = opts.command or (gopass and "gopass" or "pass")
  local backend = {}

  function backend.find(host, cb)
    local argv, run_opts
    if gopass then
      argv = { cmd, "ls", "--flat" }
    else
      argv = { "sh", "-c", LIST_PASS }
      run_opts = { env = { RT_STORE = opts.store or "" } }
    end
    site.run(argv, run_opts, function(out)
      local entries = {}
      for _, line in ipairs(site.lines(out)) do
        local entry = line:gsub("^%./", ""):gsub("%.gpg$", "")
        if site.any_matches(parts_of(entry), host) then
          table.insert(entries, { id = entry, label = entry })
        end
      end
      table.sort(entries, function(a, b) return a.id < b.id end)
      cb(entries)
    end, function(err) cb(nil, err) end)
  end

  function backend.get(entry, cb)
    site.run({ cmd, "show", entry.id }, nil, function(out)
      local password, rest = out:match("^([^\r\n]*)\r?\n?(.*)$")
      if not password or password == "" then
        return cb(nil, entry.id .. " has no password on its first line")
      end
      cb({ username = username_from(rest, entry.id), password = password })
    end, function(err) cb(nil, err) end)
  end

  return backend
end

return M
