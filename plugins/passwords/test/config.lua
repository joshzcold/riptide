-- The specs start with pass and a store of empty entries in test/store;
-- each spec switches backend with use().
rt.pack.add({
  dir = plugin_test.dir,
  name = "passwords",
  opts = {
    backend = "pass",
    command = plugin_test.dir .. "/test/bin/fake-pass",
    store = plugin_test.dir .. "/test/store",
  },
})
