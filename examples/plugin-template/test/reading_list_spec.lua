-- Run with: riptide --plugin-test .
-- describe/it/assert/wait come from riptide's test runner; see the plugins
-- page of the riptide docs.
describe("reading-list", function()
  it("saves the page once", function()
    clear_messages()
    keys("<Space>r")
    wait_until(function() return last_message() end)
    assert.matches("^Saved for later", last_message())
    keys("<Space>r")
    wait_until(function() return last_message() == "Already saved" end)
  end)

  it("lists what's saved", function()
    local list = require("reading-list")
    assert.equals(1, #list.urls())
    run("reading-list")
  end)
end)
