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
    // The repository's checkout: one per repository, named after it.
    let checkout = |b: &Browser| -> Option<std::path::PathBuf> {
        std::fs::read_dir(b.data_dir().join("pack"))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("rt-e2e-plugin-repo"))
            })
    };
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
        checkout(&b)
            .unwrap()
            .join("lua/versioned/init.lua")
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
    b.run("pack-check");
    b.wait_until("the check found v2", |s| {
        s.message()
            .is_some_and(|m| m.contains("versioned: 1 new commit"))
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

    let version = |b: &Browser| -> Option<String> {
        b.run("which-version");
        b.wait_until("a version", |s| s.message().is_some())
            .message()
            .map(str::to_string)
    };
    let wait_version = |b: &Browser, wanted: &str| {
        let start = std::time::Instant::now();
        while version(b).as_deref() != Some(wanted) {
            assert!(start.elapsed() < rt_e2e::TIMEOUT, "never on {wanted}");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };

    // Restore puts it back on the lockfile's commit, e.g. after a dotfiles pull.
    let text = std::fs::read_to_string(&lock)
        .unwrap()
        .replace(&second, &first);
    std::fs::write(&lock, text).unwrap();
    b.run("pack-restore");
    wait_version(&b, "v1");

    // :pack-update moves it straight to the newest commit and records it.
    b.run("pack-update");
    wait_version(&b, "v2");
    assert!(std::fs::read_to_string(&lock).unwrap().contains(&second));

    // Remove deletes the installed copy and its lockfile entry.
    b.eval(r#"document.querySelector('[data-key="plugin:versioned:remove#remove"]').click(), ''"#);
    b.wait_until("removed", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("Removed versioned"))
    });
    assert!(checkout(&b).is_none());
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
local float_page
rt.command("open-notes-float", function()
  float_page = rt.ui.page({
    where = "float",
    title = "Notes",
    width = 300,
    height = 200,
    on_message = function(name, data) rt.notify("float got " .. name .. " " .. data.n) end,
  })
end)
rt.command("close-notes-float", function() float_page:close() end)
rt.command("open-notes-over", function()
  float_page = rt.ui.page({
    where = "float",
    over = { x = 20, y = 60, width = 500, height = 260 },
    keys = "all",
    on_message = function(name) rt.notify("over got " .. name) end,
  })
end)
rt.command("open-notes-offset", function()
  float_page = rt.ui.page({
    where = "float",
    over = { x = 20, y = 60, width = 500, height = 260 },
    offset = { x = 100, y = 30 },
    on_message = function(name, _, page)
      if name == "hello" then page:resize(420, 200) end
      rt.notify("offset got " .. name)
    end,
  })
end)
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

    // In a float: over the page at its size, named after the plugin.
    b.run("open-notes-float");
    let s = b.wait_until("the float page's message arrived", |s| {
        s.message() == Some("float got hello 1")
    });
    assert_eq!(s.popup["title"], "Notes · plugin notes");
    assert_eq!(s.popup["size"], serde_json::json!([300, 200]));
    assert!(
        s.popup["url"]
            .as_str()
            .is_some_and(|u| u.starts_with("riptide://notes.plugin/index.html?page=")),
        "{}",
        s.popup
    );
    b.run("close-notes-float");
    b.wait_until("the float closed", |s| s.popup.is_null());

    // Over part of the page, with its bar just above, and Escape is the
    // page's: the float takes every key.
    b.run("open-notes-over");
    let s = b.wait_until("the float page loaded", |s| {
        s.message() == Some("over got hello")
    });
    assert_eq!(s.popup["all_keys"], true);
    let bounds = &s.popup["bounds"];
    assert_eq!(
        (&bounds[0], &bounds[2], &bounds[3]),
        (&20.into(), &500.into(), &260.into())
    );
    let top = bounds[1].as_i64().unwrap();
    assert!(
        top > 60,
        "below the tab bar, at the page's y = 60: {bounds}"
    );
    b.keys("<Escape>");
    assert!(!b.state().popup.is_null(), "Escape closed the float");
    b.run("close-notes-float");
    b.wait_until("the float closed", |s| s.popup.is_null());

    // Moved from there by `offset`, and resized by its plugin.
    b.run("open-notes-offset");
    b.wait_until("the float page loaded", |s| {
        s.message() == Some("offset got hello")
    });
    let s = b.wait_until("the float was resized", |s| {
        s.popup["bounds"][2] == 420 && s.popup["bounds"][3] == 200
    });
    assert_eq!(s.popup["bounds"][0], 120, "{}", s.popup);
    assert_eq!(s.popup["bounds"][1].as_i64(), Some(top + 30), "{}", s.popup);
    b.run("close-notes-float");
    b.wait_until("the float closed", |s| s.popup.is_null());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn page_eval_is_refused_on_sites_outside_the_pages_permission() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/peek/plugin/peek.lua",
            r#"rt.command("peek", function()
  rt.page.eval("document.title", function(v, err) rt.notify("peek " .. tostring(err or v)) end)
end)"#,
        )
        .file(
            "scratch-plugins/peek/riptide-plugin.toml",
            "[permissions]\npages = [\"*.example.org\"]\n",
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/peek" })"#)
        .start("page.html");
    b.wait_until("peek asks", |s| s.mode == "yesno");
    b.keys("y");
    b.wait_mode("normal");
    b.run("peek");
    b.wait_until("the callback heard why", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("peek may not act on 127.0.0.1"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn help_covers_the_lua_api_and_each_plugins_readme() {
    let b = Browser::launch()
        .file("scratch-plugins/helpful/plugin/helpful.lua", "")
        .file(
            "scratch-plugins/helpful/riptide-plugin.toml",
            "description = \"Helps out\"\n",
        )
        .file(
            "scratch-plugins/helpful/README.md",
            "# helpful\n\n<script>window.readmeRan = 1</script> Use :helpful-go.",
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/helpful" })"#)
        .start("page.html");
    b.run("help rt.ui.float");
    b.wait_until("the help page is at rt.ui.float", |s| {
        s.tab().url == "riptide://help/#lua-rt.ui.float"
    });
    b.wait_eval(
        "document.getElementById('lua-rt.ui.float')?.textContent.includes('box of text') ? 'yes' : 'no'",
        "yes",
    );
    b.run("help helpful");
    b.wait_until("the help page is at the plugin", |s| {
        s.tab().url == "riptide://help/#plugin-helpful"
    });
    b.wait_eval(
        "document.getElementById('plugin-helpful')?.textContent.includes('Use :helpful-go.') ? 'yes' : 'no'",
        "yes",
    );
    // The README is text, never markup.
    assert_eq!(b.eval("String(window.readmeRan)"), "undefined");

    b.keys(":help rt.ui.fl");
    b.wait_until("Lua functions complete", |s| {
        s.completion["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["name"] == "rt.ui.float"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugin_test_runs_the_templates_specs_and_exits_with_the_result() {
    // A browser only for its display; the test runner starts its own.
    let b = Browser::start("page.html");
    let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/reading-list");
    let out = std::process::Command::new(rt_e2e::binary())
        .arg("--plugin-test")
        .arg(&template)
        .env("DISPLAY", format!(":{}", b.display()))
        .env("XDG_RUNTIME_DIR", b.scratch().join("plugin-test-run"))
        .env("DBUS_SESSION_BUS_ADDRESS", "disabled:")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("1..2\n"), "{stdout}");
    assert!(stdout.contains("# 2 tests, 0 failed"), "{stdout}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_install_from_a_folder_of_a_repository_of_several() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-monorepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    for (name, word) in [("first", "one"), ("second", "two")] {
        let script = repo.join(format!("{name}/plugin/{name}.lua"));
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(
            &script,
            format!("rt.command('{name}-say', function() rt.notify('{word}') end)"),
        )
        .unwrap();
    }
    // second needs first, which riptide adds from the same repository.
    std::fs::write(
        repo.join("second/riptide-plugin.toml"),
        "dependencies = [\"first\"]\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch=main"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "two plugins"]);
    let src = repo.display().to_string();
    let b = Browser::launch()
        .lua(&format!("rt.pack.add({{ {src:?}, subdir = 'second' }})"))
        .start("page.html");
    b.wait_until("both installed", |s| {
        s.message().is_some_and(|m| m.contains("Installed plugin"))
    });
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).sort().join(' ')",
        "first:loaded second:loaded",
    );
    b.run("second-say");
    b.wait_until("the second plugin runs", |s| s.message() == Some("two"));
    b.run("first-say");
    b.wait_until("the first plugin runs", |s| s.message() == Some("one"));

    // One checkout for the repository, one commit for both in the lockfile.
    let pack = b.data_dir().join("pack");
    let checkouts = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&pack)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(checkouts().len(), 1, "{:?}", checkouts());

    // A new commit shows on both after Check all; Update all moves them together.
    std::fs::write(
        repo.join("first/plugin/first.lua"),
        "rt.command('first-say', function() rt.notify('one, newer') end)",
    )
    .unwrap();
    git(&["commit", "--quiet", "-am", "first says more"]);
    b.run("plugins");
    b.wait_eval(
        "String(!!document.querySelector('[data-key=\"plugins:check-all#check-all\"]'))",
        "true",
    );
    b.eval("document.querySelector('[data-key=\"plugins:check-all#check-all\"]').click(), ''");
    b.wait_eval(
        "page.plugins.filter((p) => p.update).map((p) => p.name).sort().join(' ')",
        "first second",
    );
    b.eval("document.querySelector('[data-key=\"plugins:update-all#update-all\"]').click(), ''");
    let start = std::time::Instant::now();
    loop {
        b.run("first-say");
        if b.wait_until("an answer", |s| s.message().is_some())
            .message()
            == Some("one, newer")
        {
            break;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "Update all didn't load the new commit"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let lock = std::fs::read_to_string(b.config_dir().join("rt-pack-lock.json")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&lock).unwrap();
    assert_eq!(
        lock["plugins"]["first"]["commit"],
        lock["plugins"]["second"]["commit"]
    );

    // Clean removes checkouts config.lua doesn't use, such as an old one.
    std::fs::create_dir_all(pack.join("stale-0000")).unwrap();
    b.run("pack-clean");
    b.wait_until("cleaned", |s| {
        s.message().is_some_and(|m| m.contains("stale-0000"))
    });
    assert_eq!(checkouts().len(), 1, "{:?}", checkouts());

    // Sync installs what's missing: here the checkout itself.
    let checkout = pack.join(&checkouts()[0]);
    std::fs::remove_dir_all(&checkout).unwrap();
    b.run("pack-sync");
    let start = std::time::Instant::now();
    while !checkout.join("first/plugin/first.lua").exists() {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "sync didn't install it again"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).sort().join(' ')",
        "first:loaded second:loaded",
    );
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_plugins_tab_lists_the_keys_each_plugin_bound() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/keyed/plugin/keyed.lua",
            r#"rt.keymap.set("normal", "<Space>kk", function() rt.notify("kk") end, { desc = "Say kk" })
rt.keymap.set("insert", "<Ctrl-k>", function() end)"#,
        )
        .lua(r#"rt.pack.add({ { dir = rt.config_dir .. "/../scratch-plugins/keyed" } })"#)
        .start("page.html");
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).join(' ')",
        "keyed:loaded",
    );
    b.wait_eval(
        "page.plugins[0].keys.map((k) => `${k.mode} ${k.keys} ${k.desc}`).join('; ')",
        "normal <Space>kk Say kk; insert <Ctrl-k> ",
    );
    // Drawn on the plugin's card, with the keys as chips.
    b.wait_eval(
        "String([...document.querySelectorAll('.desc kbd')].some((k) => k.textContent === '<Space>'))",
        "true",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn pack_health_shows_what_keeps_each_plugin_from_working() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/fine/lua/fine/init.lua",
            "return { setup = function() end, health = function(report) report.ok('all set') end }",
        )
        .file(
            "scratch-plugins/needy/riptide-plugin.toml",
            "[[option]]\nname = \"vault\"\nrequired = true\n",
        )
        .file(
            "scratch-plugins/needy/lua/needy/init.lua",
            "return { setup = function() end, health = function(report) report.warn('no vault tool') end }",
        )
        .lua(
            r#"
local dir = rt.config_dir .. "/../scratch-plugins/"
rt.pack.add({ { dir = dir .. "fine", opts = {} }, { dir = dir .. "needy", opts = {} } })
"#,
        )
        .start("page.html");
    b.run("pack-health");
    b.wait_until("the health check says which plugin has problems", |s| {
        s.message() == Some("Health: problems with needy; see :plugins")
    });
    // It opened the Plugins tab, with each plugin's findings.
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}: ${(p.health || []).map((h) => `${h.level} ${h.text}`).join(', ')}`).join('; ')",
        "fine: ok all set; needy: error option vault is required and not set, warn no vault tool",
    );
    b.wait_eval(
        "String(document.querySelectorAll('.health .error').length)",
        "1",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_bring_their_dependencies_which_load_first() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/core/lua/core/init.lua",
            "local M = { names = {} }\nfunction M.register(name) table.insert(M.names, name) end\nreturn M",
        )
        .file(
            "scratch-plugins/core/plugin/core.lua",
            "rt.command('core-list', function() rt.notify('backends: ' .. table.concat(require('core').names, ',')) end)",
        )
        .file(
            "scratch-plugins/backend/riptide-plugin.toml",
            "dependencies = [\"core\"]\n",
        )
        .file(
            "scratch-plugins/backend/plugin/backend.lua",
            "require('core').register('one')",
        )
        .file(
            "scratch-plugins/orphan/riptide-plugin.toml",
            "dependencies = [\"nowhere\"]\n",
        )
        .file("scratch-plugins/orphan/plugin/orphan.lua", "")
        .lua(
            r#"
local dir = rt.config_dir .. "/../scratch-plugins/"
-- Only the backend: its dependency comes from the folder beside it.
rt.pack.add({ { dir = dir .. "backend" }, { dir = dir .. "orphan" } })
"#,
        )
        .start("page.html");
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).sort().join(' ')",
        "backend:loaded core:loaded orphan:failed",
    );
    b.run("core-list");
    b.wait_until("the backend registered with the core", |s| {
        s.message() == Some("backends: one")
    });
    assert!(
        b.eval("page.plugins.find((p) => p.name === 'orphan').error")
            .contains("nowhere")
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_are_checked_for_updates_in_the_background() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    let (first, _) = versioned_repo(&repo);
    let src = repo.display().to_string();
    let b = Browser::launch()
        .lua(&format!(
            "c.plugins.check_interval = 1\nrt.pack.add({{ src = {src:?}, name = 'versioned', opts = {{}} }})"
        ))
        // Pinned to the first commit, and last checked long ago.
        .file(
            "config/rt-pack-lock.json",
            &format!("{{\"plugins\": {{\"versioned\": {{\"src\": {src:?}, \"commit\": \"{first}\"}}}}}}"),
        )
        .file("data/pack-last-check", "1")
        .start("page.html");
    b.wait_until("the background check found the update", |s| {
        s.message() == Some("Plugin updates for versioned; review them on :plugins")
    });
    // Nothing moved; the check time was recorded.
    b.run("which-version");
    b.wait_until("still the pinned version", |s| s.message() == Some("v1"));
    let checked = std::fs::read_to_string(b.data_dir().join("pack-last-check")).unwrap();
    assert!(checked.trim().parse::<u64>().unwrap() > 1);

    // The next check finds the same update and says nothing: it was mentioned.
    b.run("quit");
    b.wait_exit();
    std::fs::write(b.data_dir().join("pack-last-check"), "1").unwrap();
    b.restart();
    let start = std::time::Instant::now();
    while std::fs::read_to_string(b.data_dir().join("pack-last-check"))
        .unwrap()
        .trim()
        == "1"
    {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the second check didn't run"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let s = b.state();
    assert_ne!(
        s.message(),
        Some("Plugin updates for versioned; review them on :plugins")
    );
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_are_added_from_the_catalog_and_kept_in_plugins_toml() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-catalog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    for (name, word) in [("alpha", "a"), ("beta", "b")] {
        std::fs::create_dir_all(repo.join(format!("{name}/plugin"))).unwrap();
        std::fs::write(
            repo.join(format!("{name}/riptide-plugin.toml")),
            format!("description = \"Says {word}\"\n"),
        )
        .unwrap();
        std::fs::write(
            repo.join(format!("{name}/plugin/{name}.lua")),
            format!("rt.command('{name}-say', function() rt.notify('{word}') end)"),
        )
        .unwrap();
    }
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch=main"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "two plugins"]);
    let url = format!("file://{}", repo.display());
    let b = Browser::launch()
        .lua(&format!("c.plugins.catalog = {url:?}"))
        .start("page.html");

    // Browse lists the catalog; Add puts the plugin in plugins.toml and loads it.
    b.run("plugins");
    let click = |key: &str| {
        let query = format!("document.querySelector('[data-key=\"{key}\"]')");
        b.wait_eval(&format!("String(!!{query} && !{query}.disabled)"), "true");
        b.eval(&format!("{query}.click(), ''"));
    };
    click("plugins:browse#browse");
    b.wait_eval(
        "(page.catalog?.plugins || []).map((p) => p.name).join(' ')",
        "alpha beta",
    );
    click("catalog:alpha#add");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}:${p.added}`).join(' ')",
        "alpha:loaded:true",
    );
    b.run("alpha-say");
    b.wait_until("alpha runs", |s| s.message() == Some("a"));
    let toml = || std::fs::read_to_string(b.config_dir().join("plugins.toml")).unwrap_or_default();
    assert!(toml().contains("subdir = \"alpha\""), "{}", toml());

    // :pack-add does the same from the command line.
    b.run(&format!("pack-add {url} beta"));
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).sort().join(' ')",
        "alpha:loaded beta:loaded",
    );

    // Remove takes it out of plugins.toml, so it doesn't come back.
    click("plugin:alpha:remove#remove");
    b.wait_eval("page.plugins.map((p) => p.name).join(' ')", "beta");
    assert!(!toml().contains("alpha"), "{}", toml());
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugin_options_are_edited_on_the_tab_and_secrets_kept_out_of_files() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-options-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("vault/lua/vault")).unwrap();
    std::fs::write(
        repo.join("vault/riptide-plugin.toml"),
        r#"description = "Talks to a vault"

[[option]]
name = "server"
description = "Where it is"
required = true

[[option]]
name = "port"
type = "number"
default = 443

[[option]]
name = "mode"
type = "choice"
choices = ["fast", "safe"]

[[option]]
name = "token"
type = "secret"
description = "Your API token"
"#,
    )
    .unwrap();
    std::fs::write(
        repo.join("vault/lua/vault/init.lua"),
        r#"local M = {}
function M.setup(opts)
  rt.command("vault-show", function()
    rt.notify("server " .. tostring(opts.server) .. ":" .. tostring(opts.port) .. " " .. tostring(opts.mode))
  end)
  rt.command("vault-token", function()
    rt.secret.get("token", function(v, err) rt.notify("token " .. tostring(err or v)) end)
  end)
  rt.command("vault-other", function()
    rt.secret.get("server", function(v, err) rt.notify("other " .. tostring(err or v)) end)
  end)
end
return M"#,
    )
    .unwrap();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch=main"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "vault"]);
    let url = format!("file://{}", repo.display());
    let b = Browser::start("page.html");
    b.run(&format!("pack-add {url} vault"));
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).join(' ')",
        "vault:loaded",
    );

    // Options: typed in the form, saved to plugins.toml, given to setup().
    let set = |key: &str, js: &str| {
        b.eval(&format!(
            "(() => {{ const e = document.querySelector('[data-key=\"{key}\"]'); {js}; return ''; }})()"
        ));
    };
    set("option:vault:server#input", "e.value = 'vault.example.com'");
    set("option:vault:mode#input", "e.value = 'safe'");
    set("options:vault#save", "e.click()");
    let toml = || std::fs::read_to_string(b.config_dir().join("plugins.toml")).unwrap();
    let start = std::time::Instant::now();
    while !toml().contains("vault.example.com") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "options weren't saved: {}",
            toml()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let say = |command: &str, wanted: &str| {
        let start = std::time::Instant::now();
        loop {
            b.run(command);
            if b.wait_until("an answer", |s| s.message().is_some())
                .message()
                == Some(wanted)
            {
                break;
            }
            assert!(
                start.elapsed() < rt_e2e::TIMEOUT,
                "{command} never said {wanted:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };
    say("vault-show", "server vault.example.com:nil safe");

    // A secret goes to the keyring (here the test's memory store), and only
    // the plugin's own declared secret can be read.
    say("vault-token", "token nil");
    b.run("plugins");
    b.wait_eval(
        "String(!!document.querySelector('[data-key=\"option:vault:token#input\"]'))",
        "true",
    );
    set("option:vault:token#input", "e.value = 's3cret-token'");
    set("option:vault:token#button", "e.click()");
    b.wait_eval(
        "String(page.plugins[0].options.find((o) => o.name === 'token').set)",
        "true",
    );
    say("vault-token", "token s3cret-token");
    say(
        "vault-other",
        "other server isn't one of vault's secret options",
    );

    // Nowhere on disk: config, data, plugins.toml, the lockfile.
    let mut leaks = Vec::new();
    let mut stack = vec![b.config_dir().parent().unwrap().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read(&path)
                .is_ok_and(|bytes| bytes.windows(12).any(|w| w == b"s3cret-token"))
            {
                leaks.push(path);
            }
        }
    }
    assert!(leaks.is_empty(), "the secret was written to {leaks:?}");

    set("option:vault:token#clear", "e.click()");
    b.wait_eval(
        "String(page.plugins[0].options.find((o) => o.name === 'token').set)",
        "false",
    );
    say("vault-token", "token nil");
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_for_a_newer_riptide_are_refused_with_a_reason() {
    let repo = std::env::temp_dir().join(format!("rt-e2e-plugin-versions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    for (name, wanted) in [("today", ">=0.1"), ("someday", ">=99")] {
        let script = repo.join(format!("{name}/plugin/{name}.lua"));
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "").unwrap();
        std::fs::write(
            repo.join(format!("{name}/riptide-plugin.toml")),
            format!("riptide = {wanted:?}\n"),
        )
        .unwrap();
    }
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch=main"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "two plugins"]);
    let dir = repo.display().to_string();
    let b = Browser::launch()
        .lua(&format!(
            "c.plugins.catalog = {:?}\nrt.pack.add({{ {{ dir = {:?} }}, {{ dir = {:?} }} }})",
            format!("file://{dir}"),
            format!("{dir}/today"),
            format!("{dir}/someday")
        ))
        .start("page.html");
    b.run("plugins");
    b.wait_eval(
        "page.plugins.map((p) => `${p.name}:${p.state}`).sort().join(' ')",
        "someday:failed today:loaded",
    );
    let error = b.eval("page.plugins.find((p) => p.name === 'someday').error");
    assert!(error.contains("needs riptide >=99"), "{error}");
    // Browse lists both, and won't add the one this riptide can't run.
    b.eval(r#"document.querySelector('[data-key="plugins:browse#browse"]').click(), ''"#);
    b.wait_eval(
        "(page.catalog?.plugins || []).map((p) => `${p.folder}:${!!p.unsupported}`).join(' ')",
        "someday:true today:false",
    );
    b.wait_eval(
        r#"String(document.querySelector('[data-key="catalog:someday#add"]')?.disabled)"#,
        "true",
    );
    std::fs::remove_dir_all(&repo).unwrap();
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_with_the_history_permission_read_history_bookmarks_and_quickmarks() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/finder/plugin/finder.lua",
            r#"rt.command("finder", function()
  rt.history("nav1", function(entries, err)
    local found = entries and entries[1] and entries[1].url:match("nav1%.html") and "nav1" or tostring(err)
    rt.bookmarks(function(marks)
      rt.quickmarks(function(quick)
        rt.notify("finder " .. found .. " " .. #marks .. " " .. type(quick) .. " " .. type(rt.set_quickmarks))
      end)
    end)
  end)
end)
rt.command("finder-first", function() rt.select_tab(1) end)"#,
        )
        .file(
            "scratch-plugins/finder/riptide-plugin.toml",
            "[permissions]\nhistory = true\n",
        )
        .file(
            "scratch-plugins/nosy/plugin/nosy.lua",
            r#"rt.command("nosy", function() rt.notify("nosy " .. type(rt.history)) end)"#,
        )
        .lua(
            r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/finder" })
rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/nosy" })"#,
        )
        .start("nav1.html");
    b.wait_until("finder asks", |s| s.mode == "yesno");
    b.keys("y");
    b.wait_mode("normal");
    b.run("bookmark-add");
    b.run(&format!("open -t {}", b.url("links.html")));
    b.wait_until("the second tab", |s| s.tab().url.ends_with("links.html"));

    b.run("finder");
    b.wait_until("the lists arrived", |s| {
        // It may read, but not change: that's the marks permission.
        s.message() == Some("finder nav1 1 table nil")
    });
    b.run("finder-first");
    b.wait_until("the first tab is shown", |s| {
        s.tab().url.ends_with("nav1.html")
    });

    // Without the permission, it has no rt.history.
    b.run("nosy");
    b.wait_until("nosy can't read history", |s| {
        s.message() == Some("nosy nil")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_move_and_close_tabs_and_replace_quickmarks() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/tidy/plugin/tidy.lua",
            r#"rt.command("tidy-marks", function()
  rt.set_quickmarks({ { name = "one", url = "https://one.example/" }, { name = "two", url = "https://two.example/" } }, function(ok)
    rt.quickmarks(function(marks) rt.notify("marks " .. tostring(ok) .. " " .. marks[1].name .. " " .. marks[2].name) end)
  end)
end)
rt.command("tidy-float", function()
  rt.ui.page({ where = "float", on_message = function(name) rt.notify("float " .. name) end })
end)
rt.command("tidy-move", function() rt.move_tab(1, 2) end)
rt.command("tidy-close", function() rt.close_tab(1) end)"#,
        )
        .file("scratch-plugins/tidy/pages/index.html", r#"<script src="app.js"></script>"#)
        .file(
            "scratch-plugins/tidy/pages/app.js",
            r#"rt.send("hello", JSON.stringify({}));"#,
        )
        .file(
            "scratch-plugins/tidy/riptide-plugin.toml",
            "[permissions]\nhistory = true\nmarks = true\n",
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/tidy" })"#)
        .start("nav1.html");
    b.wait_until("tidy asks", |s| s.mode == "yesno");
    b.keys("y");
    b.wait_mode("normal");

    b.run("tidy-marks");
    b.wait_until("the quickmarks were replaced", |s| {
        s.message() == Some("marks true one two")
    });

    b.run(&format!("open -t {}", b.url("links.html")));
    // Loaded, not just known by its URL, as the harness notes.
    b.wait_until("the second tab", |s| {
        s.tab().url.ends_with("links.html") && !s.tab().loading && s.tab().title == "links"
    });
    b.run("tidy-float");
    b.wait_until("the float page loaded", |s| {
        s.message() == Some("float hello")
    });
    // links.html is the current tab, second; moving nav1 after it leaves the
    // current tab shown, and the float with it.
    b.run("tidy-move");
    let s = b.wait_until("nav1 moved last", |s| {
        s.windows[0]
            .tabs
            .last()
            .is_some_and(|t| t.url.ends_with("nav1.html"))
    });
    assert!(!s.popup.is_null(), "moving tabs closed the float");
    b.run("tidy-close");
    b.wait_until("one tab left", |s| s.windows[0].tabs.len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn plugins_enter_caret_mode_but_not_riptides_own_modes() {
    let b = Browser::launch()
        .file(
            "scratch-plugins/moder/plugin/moder.lua",
            r#"rt.command("moder-caret", function() rt.enter_mode("caret") end)
rt.command("moder-prompt", function()
  local ok = pcall(rt.enter_mode, "prompt")
  rt.notify("prompt " .. tostring(ok))
end)"#,
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/moder" })"#)
        .start("caret.html");
    b.run("moder-caret");
    b.wait_mode("caret");
    b.run("mode-leave");
    b.wait_mode("normal");
    b.run("moder-prompt");
    b.wait_until("prompt mode is refused", |s| {
        s.message() == Some("prompt false")
    });
}
