-- Fills the login form on the current site from a password manager's command
-- line tool. Each backend finds the entries saved for the site and reads one.
local site = require("passwords.site")

local M = {}

local BACKENDS = { "pass", "gopass", "rbw", "bw", "keepassxc" }

local backend
local opts = {}

-- Find the logins for the current site, let the user pick one, and fill it.
-- `what` is "login", "username" or "password".
local function fill(what)
  local url = rt.url()
  local host = site.host(url)
  if not host then
    return rt.notify("passwords: this page has no site to fill a login for", "error")
  end
  backend.find(host, function(entries, err)
    if not entries then
      -- No error means the password question was cancelled.
      if err then
        rt.notify("passwords: " .. err, "error")
      end
      return
    end
    if #entries == 0 then
      return rt.notify("passwords: no login saved for " .. host, "warning")
    end
    local function use(entry)
      backend.get(entry, function(login, why)
        if not login then
          return rt.notify("passwords: " .. why, "error")
        end
        rt.page.fill_login({
          host = host,
          username = what ~= "password" and login.username or nil,
          password = what ~= "username" and login.password or nil,
          submit = what == "login" and opts.submit or false,
        })
      end)
    end
    if #entries == 1 then
      return use(entries[1])
    end
    rt.ui.select(entries, {
      prompt = "Login for " .. host,
      format = function(entry) return entry.label end,
    }, function(entry)
      if entry then use(entry) end
    end)
  end)
end

function M.setup(user_opts)
  opts = user_opts or {}
  local name = opts.backend or "pass"
  local known = false
  for _, b in ipairs(BACKENDS) do known = known or b == name end
  if not known then
    error("passwords: backend must be one of " .. table.concat(BACKENDS, ", ") .. ", not " .. tostring(name))
  end
  backend = require("passwords.backends." .. name).new(opts)
  -- Lua can't read $HOME in the sandbox; the shell can, for "~/" in paths.
  rt.spawn({ "sh", "-c", 'printf %s "$HOME"' }, function(r)
    if r.code == 0 and r.stdout ~= "" then site.home = r.stdout end
  end)

  rt.command("password-fill", function() fill("login") end,
    "Fill the username and password saved for this site")
  rt.command("password-fill-username", function() fill("username") end,
    "Fill only the username saved for this site")
  rt.command("password-fill-password", function() fill("password") end,
    "Fill only the password saved for this site")
  rt.command("password-lock", function()
    if backend.forget then
      backend.forget()
      rt.notify("passwords: locked; the next fill asks for the password again")
    else
      rt.notify("passwords: " .. name .. " keeps no session here; its own agent locks it")
    end
  end, "Forget the password manager's unlocked session (bw, keepassxc)")

  local keys = opts.keys
  if keys == nil then
    keys = { login = "<Space>pp", username = "<Space>pu", password = "<Space>pw" }
  end
  if keys then
    local desc = { login = "Fill this site's login", username = "Fill this site's username", password = "Fill this site's password" }
    for what, lhs in pairs(keys) do
      rt.keymap.set("normal", lhs, function() fill(what) end, { desc = desc[what] })
    end
  end
end

return M
