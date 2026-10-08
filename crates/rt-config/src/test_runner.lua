---@diagnostic disable: undefined-global, undefined-field, duplicate-set-field
-- `riptide --plugin-test`: the config.lua of a throwaway profile. It loads the
-- plugin, runs its test/*_spec.lua files and prints TAP; `plugin_test` (set
-- above by riptide) names the plugin's folder, its specs and the time limit.
-- Each `it` runs in a coroutine, so wait() lets the browser work meanwhile.

c.confirm_quit = { "never" }

local function say(line)
  io.stdout:write(line, "\n")
  io.stdout:flush()
end

-- Messages, the plugin's among them: it copies rt when it loads, after this.
local messages = {}
local notify = rt.notify
rt.notify = function(text, level)
  messages[#messages + 1] = tostring(text)
  return notify(text, level)
end
rt.message = rt.notify

local tests, names, setups = {}, {}, {}

local function describe(name, fn)
  local outer = #setups
  names[#names + 1] = name
  fn()
  names[#names] = nil
  for i = #setups, outer + 1, -1 do setups[i] = nil end
end

local function it(name, fn)
  local full = #names > 0 and (table.concat(names, " ") .. " " .. name) or name
  tests[#tests + 1] = { name = full, fn = fn, before = { table.unpack(setups) } }
end

local function before_each(fn) setups[#setups + 1] = fn end

local function show(value)
  if type(value) == "string" then return ("%q"):format(value) end
  if type(value) == "table" then
    local ok, json = pcall(rt.json.encode, value)
    if ok then return json end
  end
  return tostring(value)
end

local function fail(text) error(text, 3) end

local function same(a, b)
  if type(a) ~= type(b) then return false end
  if type(a) ~= "table" then return a == b end
  for k, v in pairs(a) do
    if not same(v, b[k]) then return false end
  end
  for k in pairs(b) do
    if a[k] == nil then return false end
  end
  return true
end

local function label(text) return text and (text .. ": ") or "" end

local check = {
  equals = function(expected, actual, text)
    if expected ~= actual then fail(label(text) .. "expected " .. show(expected) .. ", got " .. show(actual)) end
  end,
  same = function(expected, actual, text)
    if not same(expected, actual) then fail(label(text) .. "expected " .. show(expected) .. ", got " .. show(actual)) end
  end,
  truthy = function(value, text)
    if not value then fail(label(text) .. "expected a true value, got " .. show(value)) end
  end,
  falsy = function(value, text)
    if value then fail(label(text) .. "expected a false value, got " .. show(value)) end
  end,
  matches = function(pattern, text, what)
    if type(text) ~= "string" or not text:find(pattern) then
      fail(label(what) .. show(text) .. " doesn't match " .. show(pattern))
    end
  end,
  has_error = function(fn, pattern)
    local ok, err = pcall(fn)
    if ok then fail("expected an error") end
    if pattern and not tostring(err):find(pattern) then fail("the error " .. show(tostring(err)) .. " doesn't match " .. show(pattern)) end
  end,
}
setmetatable(check, {
  __call = function(_, value, text)
    if not value then fail(text or "assertion failed") end
    return value
  end,
})

-- The test running now; waits from an earlier one resume nothing.
local current
local function resume_if_current(co)
  if co == current and coroutine.status(co) == "suspended" then
    local ok, err = coroutine.resume(co)
    if not ok then current = nil; return false, err end
  end
  return true
end
local after_resume

local function wake(co)
  local ok, err = resume_if_current(co)
  after_resume(co, ok, err)
end

local function wait(ms)
  local co = coroutine.running()
  rt.defer(ms or 0, function() wake(co) end)
  coroutine.yield()
end

local function wait_until(fn, timeout)
  timeout = timeout or 5000
  for _ = 0, timeout, 50 do
    local value = fn()
    if value then return value end
    wait(50)
  end
  fail("waited " .. timeout .. " ms in vain")
end

local function wait_for(event, opts)
  opts = opts or {}
  local timeout = opts.timeout or 5000
  local co = coroutine.running()
  local fired
  local id = rt.on(event, { pattern = opts.pattern, once = true }, function(e)
    fired = e
    wake(co)
  end)
  local timer = rt.defer(timeout, function() wake(co) end)
  coroutine.yield()
  timer:stop()
  if not fired then
    rt.off(id)
    fail("no " .. event .. " event within " .. timeout .. " ms")
  end
  return fired
end

local env = setmetatable({
  describe = describe,
  it = it,
  before_each = before_each,
  assert = check,
  wait = wait,
  wait_until = wait_until,
  wait_for = wait_for,
  -- Press riptide keys, or run a command; both happen once the test waits.
  keys = function(keys) rt._keys(keys); wait(0) end,
  run = function(line) rt.run(line); wait(0) end,
  -- A file in the plugin's test/ folder, as a web page.
  page = function(path) return "http://plugin-test.localhost/" .. path end,
  messages = function() return messages end,
  last_message = function() return messages[#messages] end,
  clear_messages = function() messages = {} end,
}, { __index = _G })

local custom = plugin_test.dir .. "/test/config.lua"
local file = io.open(custom)
if file then
  file:close()
  dofile(custom)
else
  rt.pack.add({ dir = plugin_test.dir, name = plugin_test.name, opts = {} })
end

local loading_failed = {}
for _, spec in ipairs(plugin_test.specs) do
  -- Named from the plugin's folder, so errors read test/x_spec.lua:3.
  local file = assert(io.open(spec))
  local text = file:read("a")
  file:close()
  local chunk, err = load(text, "@" .. spec:sub(#plugin_test.dir + 2), "t", env)
  if chunk then
    local ok, run_err = pcall(chunk)
    if not ok then loading_failed[#loading_failed + 1] = run_err end
  else
    loading_failed[#loading_failed + 1] = err
  end
end

local index, failed = 0, 0
local run_next

local function finish(test, ok, err)
  if test.done then return end
  test.done = true
  if test.timer then test.timer:stop() end
  current = nil
  if ok then
    say(("ok %d - %s"):format(index, test.name))
  else
    failed = failed + 1
    say(("not ok %d - %s"):format(index, test.name))
    for line in tostring(err):gmatch("[^\n]+") do say("# " .. line) end
  end
  rt.defer(0, run_next)
end

after_resume = function(co, ok, err)
  local test = tests[index]
  if not test or test.co ~= co then return end
  if not ok then
    finish(test, false, err)
  elseif coroutine.status(co) == "dead" then
    finish(test, true)
  end
end

run_next = function()
  index = index + 1
  local test = tests[index]
  if not test then
    say(("# %d tests, %d failed"):format(#tests, failed))
    return rt._exit(failed > 0 and 1 or 0)
  end
  test.co = coroutine.create(function()
    for _, before in ipairs(test.before) do before() end
    test.fn()
  end)
  test.timer = rt.defer(plugin_test.timeout, function()
    finish(test, false, "timed out after " .. plugin_test.timeout .. " ms")
  end)
  current = test.co
  wake(test.co)
end

rt.on("startup", function()
  say("TAP version 13")
  if #loading_failed > 0 then
    for _, err in ipairs(loading_failed) do say("# " .. tostring(err)) end
    say("Bail out! a spec file didn't load")
    return rt._exit(1)
  end
  say("1.." .. #tests)
  -- Give the plugin and the first page a moment to settle.
  rt.defer(300, run_next)
end)
