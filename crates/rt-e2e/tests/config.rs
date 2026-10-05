//! Config files and :set.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_binding_in_config_toml_works() {
    let b = Browser::launch()
        .toml("[bindings.normal]\n\"X\" = \"scroll-to-perc 100\"\n")
        .start("page.html");
    b.keys("X");
    b.wait_eval("String(scrollY > 3000)", "true");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn set_persists_to_autoconfig_and_survives_a_restart() {
    let b = Browser::launch()
        .toml("url.start_pages = [\"about:blank\"]\n")
        .start("page.html");
    b.run("set hints.chars xyz");
    let autoconfig = b.config_dir().join("autoconfig.toml");
    let text = std::fs::read_to_string(&autoconfig).unwrap_or_default();
    assert!(text.contains("\"hints.chars\" = \"xyz\""), "{text}");
    b.run("quit");
    b.wait_exit();
    b.restart();
    b.run("set messages.timeout 0");
    b.run("set hints.chars");
    b.wait_until("the value is shown", |s| {
        s.message().is_some_and(|m| m.contains("xyz"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn lua_keys_commands_and_hooks() {
    let b = Browser::launch()
        .lua(
            r#"
c.messages.timeout = 0
rt.bind("Q", function() rt.message("title " .. rt.title()) end)
rt.command("tab-to", function(args) rt.open(args, "tab") end, "Open in a tab")
rt.on("load_finished", function(e)
  if e.url:find("second") then rt.message("hook saw second") end
end)
"#,
        )
        .start("page.html");
    b.keys("Q");
    b.wait_until("the key ran the function", |s| {
        s.message() == Some("title ready")
    });
    b.run(&format!("tab-to {}", b.url("second.html")));
    let s = b.wait_until("the hook ran", |s| s.message() == Some("hook saw second"));
    assert_eq!(s.tabs().len(), 2);
}
