//! Plugins from `rt.pack.add`: loaded in a sandbox once their permissions
//! are approved.
#![cfg(unix)]

use rt_e2e::Browser;

const GREETER: &str = r#"
local M = {}
function M.setup(opts)
  rt.command("greeter-hello", function() rt.notify("hello " .. opts.who) end)
end
return M
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_load_after_their_permissions_are_approved() {
    let b = Browser::launch()
        .file("scratch-plugins/greeter/lua/greeter/init.lua", GREETER)
        .file(
            "scratch-plugins/greeter/riptide-plugin.toml",
            "description = \"Says hello\"\n[permissions]\nspawn = true\n",
        )
        .file(
            "scratch-plugins/greedy/plugin/greedy.lua",
            "rt.command('greedy-x', function() end)",
        )
        .file(
            "scratch-plugins/greedy/riptide-plugin.toml",
            "[permissions]\nfiles = true\n",
        )
        .lua(
            r#"
rt.pack.add({
  { dir = rt.config_dir .. "/../scratch-plugins/greeter", opts = { who = "world" } },
  { dir = rt.config_dir .. "/../scratch-plugins/greedy" },
})
"#,
        )
        .start("page.html");
    // One question at a time: greeter first.
    let s = b.wait_until("greeter asks", |s| s.mode == "yesno");
    let message = s
        .prompt
        .as_ref()
        .map(|p| p["message"].to_string())
        .unwrap_or_default();
    assert!(
        message.contains("greeter") && message.contains("run programs"),
        "{message}"
    );
    b.keys("y");
    let s = b.wait_until("greedy asks", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p["message"].to_string().contains("greedy"))
    });
    assert!(
        s.prompt.unwrap()["message"]
            .to_string()
            .contains("read and write your files")
    );
    b.keys("n");
    b.wait_mode("normal");

    b.run("greeter-hello");
    b.wait_until("the plugin's command ran", |s| {
        s.message() == Some("hello world")
    });
    b.run("greedy-x");
    b.wait_until("the refused plugin didn't load", |s| {
        s.message().is_some_and(|m| m.contains("greedy-x"))
    });
    let lock = std::fs::read_to_string(b.config_dir().join("rt-pack-lock.json")).unwrap();
    assert!(
        lock.contains("greeter") && lock.contains("\"spawn\": true"),
        "{lock}"
    );
    assert!(!lock.contains("greedy"), "{lock}");
}
