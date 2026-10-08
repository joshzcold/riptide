-- rbw, the unofficial Bitwarden client, unlocked by its own agent. Its search
-- matches anywhere in an entry, so each result's saved addresses (or its name)
-- must belong to the site.
local site = require("passwords.site")

local M = {}

function M.new(opts)
  local cmd = opts.command or "rbw"
  local backend = {}

  function backend.find(host, cb)
    site.run({ cmd, "search", "--fields", "id,name,user", site.search_term(host) }, nil, function(out)
      local found = {}
      for _, line in ipairs(site.lines(out)) do
        local id, name, user = line:match("^([^\t]*)\t([^\t]*)\t?(.*)$")
        if id and id ~= "" and #found < site.MAX_CANDIDATES then
          table.insert(found, { id = id, name = name, user = user })
        end
      end
      site.each(found, function(item, done)
        site.run({ cmd, "get", "--raw", item.id }, nil, function(raw)
          local ok, cipher = pcall(rt.json.decode, raw)
          local data = ok and type(cipher) == "table" and cipher.data or nil
          if type(data) ~= "table" or not data.password then
            return done(nil)
          end
          local saved = { item.name }
          for _, uri in ipairs(data.uris or {}) do
            table.insert(saved, type(uri) == "table" and uri.uri or uri)
          end
          if not site.any_matches(saved, host) then
            return done(nil)
          end
          local label = item.name
          if data.username and data.username ~= "" then
            label = label .. " (" .. data.username .. ")"
          end
          done({ id = item.id, label = label, login = { username = data.username, password = data.password } })
        end, function() done(nil) end)
      end, function(entries)
        cb(entries)
      end)
    end, function(err) cb(nil, err) end)
  end

  function backend.get(entry, cb)
    cb(entry.login)
  end

  return backend
end

return M
