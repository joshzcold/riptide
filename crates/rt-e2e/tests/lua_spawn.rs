//! `rt.spawn` from config.lua: programs run in the background and their
//! output comes back to a Lua callback.
#![cfg(unix)]

use rt_e2e::Browser;

const CONFIG: &str = r#"
rt.command("upper", function()
  rt.spawn({"tr", "a-z", "A-Z"}, { stdin = "hello" }, function(r)
    rt.message("upper:" .. r.stdout .. ":" .. r.code)
  end)
end)
rt.command("where", function(dir)
  rt.spawn("sh -c 'printf %s:%s \"$(pwd)\" \"$GREETING\"'", { cwd = dir, env = { GREETING = "hi" } }, function(r)
    rt.message("where:" .. r.stdout)
  end)
end)
rt.command("missing", function()
  rt.spawn({"riptide-no-such-program"}, function(r)
    rt.message("missing:" .. tostring(r.code) .. ":" .. tostring(r.error ~= nil))
  end)
end)
"#;

fn wait_message(b: &Browser, command: &str, expected: &str) {
    b.run(command);
    b.wait_until(expected, |s| s.message() == Some(expected));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn lua_spawn_runs_programs_and_calls_back() {
    let b = Browser::launch()
        .lua(CONFIG)
        .toml("messages.timeout = 0\n")
        .start("page.html");
    wait_message(&b, "upper", "upper:HELLO:0");
    let dir = b.scratch();
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    wait_message(
        &b,
        &format!("where {}", dir.display()),
        &format!("where:{}:hi", dir.display()),
    );
    wait_message(&b, "missing", "missing:nil:true");
}
