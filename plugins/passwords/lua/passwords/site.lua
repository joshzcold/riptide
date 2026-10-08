-- Which saved logins belong to a site. A login saved for example.com fills on
-- example.com and its subdomains, never on a name that only contains it, such
-- as notexample.com or example.com.evil.net.
local M = {}

-- The host of a URL, lowercased and without user info or port; nil for pages
-- without one (about:blank, riptide:// pages).
function M.host(url)
  local scheme, rest = (url or ""):match("^(%a[%w+.-]*)://([^/?#]*)")
  if not scheme or (scheme ~= "http" and scheme ~= "https") then
    return nil
  end
  local host = rest:gsub("^.*@", ""):gsub(":%d*$", ""):lower()
  if host == "" then
    return nil
  end
  return host
end

-- A saved address or name as a host: a URL's host, or the text itself when it
-- looks like a domain (has a dot and no spaces).
function M.saved_host(text)
  if not text or text == "" then
    return nil
  end
  local from_url = M.host(text)
  if from_url then
    return from_url
  end
  text = text:lower()
  if text:find("^[%w.-]+$") and text:find("%.") then
    return text
  end
  return nil
end

-- Whether a login saved for `saved` belongs on `host`.
function M.matches(saved, host)
  saved = M.saved_host(saved)
  if not saved or not saved:find("%.") then
    return false
  end
  if saved:sub(1, 4) == "www." then
    saved = saved:sub(5)
  end
  return host == saved or host:sub(-(#saved + 1)) == "." .. saved
end

-- Whether any of `texts` (addresses, a name, path parts) belongs on `host`.
function M.any_matches(texts, host)
  for _, text in ipairs(texts) do
    if M.matches(text, host) then
      return true
    end
  end
  return false
end

-- How many of a search's results are read and checked against the site.
M.MAX_CANDIDATES = 20

-- What to search a password manager for: the host's last two parts, so
-- logins saved for the parent domain are found too. Results are checked with
-- `matches` before they're offered.
function M.search_term(host)
  return host:match("([^.]+%.[^.]+)$") or host
end

-- Run `step(item, done)` for each item at once; `finish(results)` gets the
-- non-nil results, in the items' order, once all are done.
function M.each(items, step, finish)
  local results, left = {}, #items
  if left == 0 then
    return finish({})
  end
  for i, item in ipairs(items) do
    step(item, function(result)
      results[i] = result or false
      left = left - 1
      if left == 0 then
        local out = {}
        for j = 1, #items do
          if results[j] then
            table.insert(out, results[j])
          end
        end
        finish(out)
      end
    end)
  end
end

-- The home folder, for "~/" in paths; set once setup has asked the shell.
M.home = nil

-- `path` with a leading "~/" made absolute.
function M.expand(path)
  if path and M.home and path:sub(1, 2) == "~/" then
    return M.home .. path:sub(2)
  end
  return path
end

-- Helpers for running the tools.

-- Lines of `text`, without empty ones.
function M.lines(text)
  local out = {}
  for line in (text or ""):gmatch("[^\r\n]+") do
    table.insert(out, line)
  end
  return out
end

-- Run a tool; `done(stdout)` on success, `failed(message)` otherwise. The
-- message is the tool's first error line, never its output.
function M.run(argv, opts, done, failed)
  rt.spawn(argv, opts or {}, function(r)
    if r.error then
      return failed("couldn't run " .. argv[1] .. ": " .. r.error)
    end
    if r.code ~= 0 then
      local first = M.lines(r.stderr)[1] or ("exited with status " .. tostring(r.code))
      return failed(argv[1] .. ": " .. first)
    end
    done(r.stdout)
  end)
end

return M
