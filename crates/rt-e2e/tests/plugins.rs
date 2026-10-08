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

/// A git repository with a `versioned` plugin: its first commit says v1,
/// the second v2. Returns both commits.
fn versioned_repo(path: &std::path::Path) -> (String, String) {
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(path)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let module = path.join("lua/versioned/init.lua");
    std::fs::create_dir_all(module.parent().unwrap()).unwrap();
    let write = |version: &str| {
        std::fs::write(
            &module,
            format!(
                "local M = {{}}\nfunction M.setup() rt.command('which-version', function() rt.notify('{version}') end) end\nreturn M\n"
            ),
        )
        .unwrap();
    };
    git(&["init", "--quiet", "--initial-branch=main"]);
    write("v1");
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "v1"]);
    let first = git(&["rev-parse", "HEAD"]);
    write("v2");
    git(&["commit", "--quiet", "-am", "v2"]);
    (first, git(&["rev-parse", "HEAD"]))
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_install_from_git_and_follow_the_lockfile() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-repo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    let (first, second) = versioned_repo(&repo);
    let config = format!(
        "rt.pack.add({{ src = {:?}, name = 'versioned', opts = {{}} }})",
        repo.display().to_string()
    );

    // A fresh install takes the newest commit and records it.
    let b = Browser::launch().lua(&config).start("page.html");
    b.wait_until("installed", |s| {
        s.message()
            .is_some_and(|m| m.contains("Installed plugin versioned"))
    });
    b.run("which-version");
    b.wait_until("the newest version runs", |s| s.message() == Some("v2"));
    let lock = std::fs::read_to_string(b.config_dir().join("rt-pack-lock.json")).unwrap();
    assert!(lock.contains(&second), "{lock}");
    assert!(
        b.data_dir()
            .join("pack/versioned/lua/versioned/init.lua")
            .exists()
    );
    drop(b);

    // A lockfile from elsewhere pins the first commit: that's what's installed.
    let pinned = format!(
        "{{\"plugins\": {{\"versioned\": {{\"src\": {:?}, \"commit\": \"{first}\"}}}}}}",
        repo.display().to_string()
    );
    let b = Browser::launch()
        .lua(&config)
        .file("config/rt-pack-lock.json", &pinned)
        .start("page.html");
    b.wait_until("installed", |s| {
        s.message()
            .is_some_and(|m| m.contains("Installed plugin versioned"))
    });
    b.run("which-version");
    b.wait_until("the pinned version runs", |s| s.message() == Some("v1"));

    // Checking lists the new commit on the Plugins tab; nothing moves until Update.
    b.run("pack-update");
    b.wait_until("the check found v2", |s| {
        s.message()
            .is_some_and(|m| m.contains("versioned has 1 new commit"))
    });
    b.run("plugins");
    let button = r#"document.querySelector('[data-key="plugin:versioned:update#update"]')"#;
    b.wait_eval(&format!("String(!!{button})"), "true");
    assert!(
        b.eval("document.querySelector('.log').innerText")
            .contains("v2")
    );
    b.eval(&format!("{button}.click(), ''"));
    let lock = b.config_dir().join("rt-pack-lock.json");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&lock).unwrap().contains(&second) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the lockfile didn't move"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let start = std::time::Instant::now();
    loop {
        b.run("which-version");
        if b.wait_until("a version", |s| s.message().is_some())
            .message()
            == Some("v2")
        {
            break;
        }
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "the update didn't load");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Remove deletes the installed copy and its lockfile entry.
    b.eval(r#"document.querySelector('[data-key="plugin:versioned:remove#remove"]').click(), ''"#);
    b.wait_until("removed", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("Removed versioned"))
    });
    assert!(!b.data_dir().join("pack/versioned").exists());
    assert!(
        !std::fs::read_to_string(&lock)
            .unwrap()
            .contains("versioned")
    );
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn lazy_plugins_load_on_their_command_key_or_event() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/bycmd/lua/bycmd/init.lua",
            "return { setup = function() rt.command('bycmd-hi', function(args) rt.notify('hi ' .. args) end) end }",
        )
        .file(
            "scratch-plugins/bykey/lua/bykey/init.lua",
            "return { setup = function() rt.keymap.set('normal', 'zx', function() rt.notify('zx pressed') end) end }",
        )
        .file(
            "scratch-plugins/byevent/lua/byevent/init.lua",
            "return { setup = function() rt.on('setting_changed', function(e) rt.notify('saw ' .. e.name) end) end }",
        )
        .lua(
            r#"
local dir = rt.config_dir .. "/../scratch-plugins/"
rt.pack.add({
  { dir = dir .. "bycmd", cmd = "bycmd-hi", opts = {} },
  { dir = dir .. "bykey", keys = "zx", opts = {} },
  { dir = dir .. "byevent", event = "setting_changed", opts = {} },
})
"#,
        )
        .start("page.html");
    b.run("plugins");
    b.wait_eval("page.plugins.map((p) => p.state).join()", "lazy,lazy,lazy");

    // The command that loads it runs once it's loaded, with its arguments.
    b.run("bycmd-hi there");
    b.wait_until("the command ran", |s| s.message() == Some("hi there"));

    // The key that loads it is pressed again for the plugin's own binding.
    b.run("tab-close");
    b.keys("zx");
    b.wait_until("the key ran", |s| s.message() == Some("zx pressed"));

    // The event that loads it reaches its hooks too.
    b.run("set zoom.default 125");
    b.wait_until("the event reached it", |s| {
        s.message() == Some("saw zoom.default")
    });

    let auto = std::fs::read_to_string(b.config_dir().join("autoconfig.toml")).unwrap_or_default();
    assert!(!auto.contains("zx"), "a plugin's binding was saved: {auto}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn builtin_plugins_ship_with_riptide_and_still_ask() {
    let b = Browser::launch()
        .lua(r#"rt.pack.add({ { builtin = "passwords", opts = {} }, { builtin = "no-such-plugin" } })"#)
        .start("page.html");
    let s = b.wait_until("passwords asks", |s| s.mode == "yesno");
    let message = s
        .prompt
        .as_ref()
        .map(|p| p["message"].to_string())
        .unwrap_or_default();
    assert!(
        message.contains("passwords") && message.contains("run programs"),
        "a builtin plugin isn't trusted: {message}"
    );
    b.keys("y");
    b.wait_mode("normal");
    b.run("plugins");
    let version = env!("CARGO_PKG_VERSION");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}|${p.src}|${p.git}|${p.state}`).join(' ')",
        &format!(
            "passwords|builtin (riptide {version})|false|loaded no-such-plugin|builtin (riptide {version})|false|failed"
        ),
    );
    let lock = std::fs::read_to_string(b.config_dir().join("rt-pack-lock.json")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    assert_eq!(
        lock["plugins"]["passwords"]["commit"]
            .as_str()
            .unwrap_or_default(),
        "",
        "no commit for a builtin: {lock}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugin_pages_talk_to_their_own_plugin_only() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/notes/plugin/notes.lua",
            r#"
local panel_page
rt.command("open-notes-panel", function()
  panel_page = rt.ui.page({
    where = "panel",
    side = "right",
    size = 320,
    on_message = function(name, data) rt.notify("panel got " .. name .. " " .. data.n) end,
  })
end)
rt.command("close-notes-panel", function() panel_page:close() end)
rt.command("open-notes", function()
  rt.ui.page({
    path = "index.html",
    on_message = function(name, data, page)
      rt.notify("got " .. name .. " " .. data.n)
      page:send("echo", { text = "pong" })
    end,
  })
end)
"#,
        )
        .file(
            "scratch-plugins/notes/pages/index.html",
            r#"<!doctype html><html><head><title>notes</title>
<script>window.inlineRan = true;</script>
<script src="app.js"></script></head><body>Notes</body></html>"#,
        )
        .file(
            "scratch-plugins/notes/pages/app.js",
            r#"addEventListener("rtmessage", (e) => { document.title = e.detail.name + " " + e.detail.data.text; });
rt.send("hello", JSON.stringify({ n: 1 }));"#,
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/notes" })"#)
        .start("page.html");
    b.run("open-notes");
    b.wait_until("the page's message reached the plugin", |s| {
        s.message() == Some("got hello 1")
    });
    let s = b.wait_until("the plugin's answer reached the page", |s| {
        s.tab().title == "echo pong"
    });
    assert!(
        s.tab()
            .url
            .starts_with("riptide://notes.plugin/index.html?page="),
        "{}",
        s.tab().url
    );
    // Only its own scripts run: the inline one was blocked by the CSP.
    assert_eq!(b.eval("String(window.inlineRan)"), "undefined");

    // The same page in a panel: docked, and talking the same way.
    b.run("open-notes-panel");
    b.wait_until("the panel is laid out", |s| {
        s.panels
            .first()
            .is_some_and(|p| p["width"] == 320 && p["side"] == "right")
    });
    b.wait_until("the panel page's message arrived", |s| {
        s.message() == Some("panel got hello 1")
    });
    b.run("close-notes-panel");
    b.wait_until("closed", |s| s.panels.is_empty());
}
