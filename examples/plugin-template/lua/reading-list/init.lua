-- require("reading-list"): the plugin's module. riptide calls setup(opts)
-- once it loads, with the opts from rt.pack.add.
local M = {}

local list = rt.store()

function M.urls()
  return list.get("urls") or {}
end

function M.add(url)
  local urls = M.urls()
  for _, saved in ipairs(urls) do
    if saved == url then return false end
  end
  table.insert(urls, url)
  list.set("urls", urls)
  return true
end

function M.setup(opts)
  opts = opts or {}
  rt.keymap.set("normal", opts.key or "<Space>r", function()
    if M.add(rt.url()) then
      rt.notify("Saved for later (" .. #M.urls() .. ")")
    else
      rt.notify("Already saved")
    end
  end, { desc = "Read later" })
  rt.command("reading-list", function()
    local lines = {}
    for i, url in ipairs(M.urls()) do
      lines[#lines + 1] = { { tostring(i) .. " ", "key" }, { url, "url" } }
    end
    if #lines == 0 then lines = { { { "Nothing saved yet", "muted" } } } end
    rt.ui.float({ title = "Reading list", lines = lines, keys = { q = function(f) f:close() end } })
  end, { desc = "Show the reading list" })
end

return M
