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
    std::fs::remove_dir_all(&repo).unwrap();
}
