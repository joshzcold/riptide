-- Run with: riptide --plugin-test .
-- Fake password tools in test/bin stand in for the real ones, and
-- test/login.html is the site, at http://plugin-test.localhost/.

local bin = plugin_test.dir .. "/test/bin/"

-- Set the plugin up again with another backend.
local function use(opts)
  require("passwords").setup(opts)
end

-- A value from the page, through the page's own JavaScript.
local function js(code)
  local value, done
  rt.page.eval(code, function(v)
    value, done = v, true
  end)
  wait_until(function() return done end)
  return value
end

local function field(id)
  return js("document.getElementById('" .. id .. "').value")
end

local function wait_field(id, expected)
  wait_until(function() return field(id) == expected end)
end

local function open_login()
  run("open " .. page("login.html"))
  wait_for("load_finished", { pattern = "plugin-test.localhost" })
end

describe("passwords", function()
  before_each(function()
    clear_messages()
    open_login()
  end)

  it("offers the site's pass entries and fills the one picked", function()
    use({ backend = "pass", command = bin .. "fake-pass", store = plugin_test.dir .. "/test/store" })
    run("password-fill")
    -- Two entries belong here; the lookalike sites' don't. The picker's keys
    -- follow the entries' order.
    wait(500)
    keys("1")
    wait_field("pass", "pw-alice")
    assert.equals("alice@example.com", field("user"))
    assert.equals("", field("search"), "the search box outside the form is left alone")

    -- The username alone, from the path after the site.
    run("password-fill-username")
    wait(500)
    keys("2")
    wait_field("user", "bob")
    assert.equals("pw-alice", field("pass"), "only the username changed")
  end)

  it("skips rbw entries for lookalike sites, and submits", function()
    use({ backend = "rbw", command = bin .. "fake-rbw", submit = true })
    run("password-fill")
    -- One login belongs here, so there's nothing to pick.
    wait_until(function() return js("window.__submitted") == "ann/pw-ann" end)
  end)

  it("asks for the Bitwarden master password once", function()
    use({ backend = "bw", command = bin .. "fake-bw" })
    run("password-fill")
    wait(300)
    keys("correct<Return>")
    wait_field("pass", "pw-bea")
    -- The session is kept: no question the second time.
    js("(() => { document.getElementById('pass').value = ''; return 1; })()")
    run("password-fill-password")
    wait_field("pass", "pw-bea")
    -- :password-lock forgets it. A wrong password unlocks nothing, so the
    -- next fill asks again.
    run("password-lock")
    js("(() => { document.getElementById('pass').value = ''; return 1; })()")
    run("password-fill")
    wait(300)
    keys("wrong<Return>")
    wait(1000)
    assert.equals("", field("pass"))
    run("password-fill")
    wait(300)
    keys("correct<Return>")
    wait_field("pass", "pw-bea")
  end)

  it("reads a KeePassXC database with its password", function()
    use({ backend = "keepassxc", command = bin .. "fake-keepassxc-cli", database = "~/x.kdbx" })
    wait(200) -- for $HOME, which the plugin asks the shell for
    run("password-fill")
    wait(300)
    keys("dbpass<Return>")
    wait_field("pass", "pw-carl")
    assert.equals("carl", field("user"))
  end)

  it("fills nothing once the tab has moved to another site", function()
    use({ backend = "rbw", command = bin .. "fake-rbw-slow" })
    run("password-fill")
    -- While rbw is still answering, the tab goes to another site with a
    -- login form; the login isn't filled there.
    run("open data:text/html,<form><input id=user><input id=pass type=password></form>")
    wait(3500)
    assert.equals("", field("pass"))
  end)
end)
