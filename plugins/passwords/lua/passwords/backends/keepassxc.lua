-- keepassxc-cli on a .kdbx database. Every fill asks for the database
-- password, unless `remember` keeps it in memory for that many seconds. The
-- password goes to keepassxc-cli on its input, never its command line.
local site = require("passwords.site")

local M = {}

-- Lines of `text`, empty ones included, so attributes keep their places.
local function fields(text)
  local out = {}
  for line in ((text or "") .. "\n"):gmatch("([^\n]*)\n") do
    table.insert(out, (line:gsub("\r$", "")))
  end
  return out
end

function M.new(opts)
  if not opts.database then
    error("passwords: the keepassxc backend needs opts.database, the .kdbx file")
  end
  local cmd = opts.command or "keepassxc-cli"
  local remember = tonumber(opts.remember) or 0
  local cached, cached_at
  local backend = {}

  local function argv(sub, ...)
    local out = { cmd, sub, "-q" }
    if opts.keyfile then
      table.insert(out, "-k")
      table.insert(out, site.expand(opts.keyfile))
    end
    table.insert(out, site.expand(opts.database))
    for _, a in ipairs({ ... }) do
      table.insert(out, a)
    end
    return out
  end

  local function with_password(cb)
    if cached and remember > 0 and os.time() - cached_at < remember then
      return cb(cached)
    end
    cached = nil
    rt.ui.input({ prompt = "KeePassXC database password", secret = true }, function(password)
      if not password then
        return cb(nil)
      end
      if remember > 0 then
        cached, cached_at = password, os.time()
      end
      cb(password)
    end)
  end

  function backend.find(host, cb)
    with_password(function(password)
      if not password then
        return cb(nil)
      end
      local input = { stdin = password .. "\n" }
      site.run(argv("search", site.search_term(host)), input, function(out)
        local paths = {}
        for _, path in ipairs(site.lines(out)) do
          if #paths < site.MAX_CANDIDATES then
            table.insert(paths, path)
          end
        end
        site.each(paths, function(path, done)
          site.run(argv("show", "-s", "-a", "Title", "-a", "UserName", "-a", "Password", "-a", "URL", path), input,
            function(shown)
              local f = fields(shown)
              local title, username, pass, url = f[1], f[2], f[3], f[4]
              if not pass or pass == "" or not site.any_matches({ url, title }, host) then
                return done(nil)
              end
              local label = path
              if username ~= "" then
                label = label .. " (" .. username .. ")"
              end
              done({ label = label, login = { username = username ~= "" and username or nil, password = pass } })
            end,
            function() done(nil) end)
        end, function(entries) cb(entries) end)
      end, function(err)
        -- Most likely a wrong password; don't keep it.
        cached = nil
        cb(nil, err)
      end)
    end)
  end

  function backend.get(entry, cb)
    cb(entry.login)
  end

  function backend.forget()
    cached = nil
  end

  return backend
end

return M
