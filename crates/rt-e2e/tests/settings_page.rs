//! `:settings`: the settings page changes, saves and resets settings.
#![cfg(unix)]

use rt_e2e::Browser;

/// Poll JavaScript in the current tab until it returns `expected`.
fn wait_eval(b: &Browser, code: &str, expected: &str) {
    let start = std::time::Instant::now();
    loop {
        let got = b.eval(code);
        if got == expected {
            return;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{code} is {got:?}, not {expected:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_settings_page_changes_saves_and_resets_settings() {
    let b = Browser::start("page.html");
    // Web pages don't get riptide's message channel.
    assert_eq!(b.eval("typeof rt"), "undefined");
    b.run("settings");
    b.wait_until("the settings page opens", |s| {
        s.tab().url.starts_with("riptide://settings")
    });
    let checkbox = "document.querySelector('[data-key=\"hints.uppercase#bool\"]')";
    wait_eval(&b, &format!("String(!!{checkbox})"), "true");

    b.eval(&format!("{checkbox}.click(), ''"));
    b.wait_until("the change is applied", |s| {
        s.message()
            .is_some_and(|m| m.contains("hints.uppercase = true"))
    });
    let autoconfig = b.config_dir().join("autoconfig.toml");
    let start = std::time::Instant::now();
    while !std::fs::read_to_string(&autoconfig).is_ok_and(|t| t.contains("hints.uppercase")) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "not saved to autoconfig.toml"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // The page shows the new value without a reload.
    wait_eval(&b, &format!("String({checkbox}.checked)"), "true");

    // A refused value is explained next to the setting.
    b.eval("rt.send('set', JSON.stringify({ name: 'prompt.width', value: 5 })), ''");
    wait_eval(
        &b,
        "document.querySelector('[data-name=\"prompt.width\"] .error')?.textContent.includes('outside') ? 'shown' : ''",
        "shown",
    );

    // The page itself refuses what can't be right, before sending it.
    b.eval(
        "(i => { i.value = 'nonsense'; i.dispatchEvent(new Event('change')); return ''; })\
         (document.querySelector('[data-key=\"colors.hints.bg#text\"]'))",
    );
    wait_eval(
        &b,
        "document.querySelector('[data-name=\"colors.hints.bg\"] .error')?.textContent.includes(\"isn't a color\") ? 'shown' : ''",
        "shown",
    );
    // The typed text stays in the field to be fixed.
    wait_eval(
        &b,
        "document.querySelector('[data-key=\"colors.hints.bg#text\"]').value",
        "nonsense",
    );
    b.run("set colors.hints.bg");
    b.wait_until("nothing was sent", |s| {
        s.message()
            .is_some_and(|m| m.trim_end() == "colors.hints.bg =")
    });

    b.eval("document.querySelector('[data-key=\"hints.uppercase#reset\"]').click(), ''");
    wait_eval(&b, &format!("String({checkbox}.checked)"), "false");
    b.run("set hints.uppercase");
    b.wait_until("the reset is applied", |s| {
        s.message()
            .is_some_and(|m| m.contains("hints.uppercase = false"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_keys_tab_binds_unbinds_and_restores_and_warns_about_clashes() {
    let b = Browser::start("page.html");
    // Opened directly (as a restored tab is), not through :settings.
    b.run("open riptide://settings/#keys/normal");
    b.wait_until("the keys tab opens", |s| {
        s.tab().url.starts_with("riptide://settings")
    });
    let field = |key: &str| format!("document.querySelector('[data-key=\"{key}\"]')");
    wait_eval(&b, &format!("String(!!{})", field("keys#newkeys")), "true");

    // A prefix of existing bindings is flagged before binding.
    b.eval(&format!(
        "(i => {{ i.value = 'g'; i.dispatchEvent(new Event('input')); return ''; }})({})",
        field("keys#newkeys")
    ));
    wait_eval(
        &b,
        "document.querySelector('.warn').textContent.includes('Clashes with') ? 'warned' : ''",
        "warned",
    );

    b.eval(&format!(
        "{k}.value = 'gx'; {c}.value = 'tab-close'; {bind}.click(), ''",
        k = field("keys#newkeys"),
        c = field("keys#newcommand"),
        bind = field("keys#bind"),
    ));
    b.run("bind gx");
    b.wait_until("gx is bound", |s| {
        s.message()
            .is_some_and(|m| m.contains("gx is bound to 'tab-close'"))
    });
    let autoconfig = b.config_dir().join("autoconfig.toml");
    assert!(
        std::fs::read_to_string(&autoconfig).is_ok_and(|t| t.contains("tab-close")),
        "not saved"
    );

    // An unknown command is refused with the reason.
    b.eval(&format!(
        "{k}.value = 'gy'; {c}.value = 'no-such-command'; {bind}.click(), ''",
        k = field("keys#newkeys"),
        c = field("keys#newcommand"),
        bind = field("keys#bind"),
    ));
    wait_eval(
        &b,
        "document.querySelector('.banner')?.textContent ? 'shown' : ''",
        "shown",
    );

    // Unbinding a default moves it to "Unbound defaults", where it can be restored.
    b.eval(&format!("{}.click(), ''", field("keys:normal:d#unbind")));
    b.run("bind d");
    b.wait_until("d is unbound", |s| {
        s.message().is_some_and(|m| m.contains("d is unbound"))
    });
    wait_eval(
        &b,
        &format!("String(!!{})", field("keys:normal:d#restore")),
        "true",
    );
    b.eval(&format!("{}.click(), ''", field("keys:normal:d#restore")));
    b.run("bind d");
    b.wait_until("d is back", |s| {
        s.message()
            .is_some_and(|m| m.contains("d is bound to 'tab-close'"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_sites_tab_forgets_saved_answers_and_clears_site_data() {
    let b = Browser::start("page.html");
    let origin = b.url("").trim_end_matches('/').to_string();
    b.eval("document.cookie = 'rt=1; max-age=3600'; localStorage.setItem('rt', '1'), ''");
    assert_eq!(b.eval("document.cookie"), "rt=1");
    b.run("set -u https://meet.example.com content.media.video_capture true");
    b.run("set -u https://meet.example.com content.media.audio_capture true");
    let autoconfig = b.config_dir().join("autoconfig.toml");
    let saved = |b: &Browser| {
        std::fs::read_to_string(b.config_dir().join("autoconfig.toml")).unwrap_or_default()
    };
    let start = std::time::Instant::now();
    while !saved(&b).contains("audio_capture") {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "not saved");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    b.run("open -t riptide://settings/#sites");
    b.wait_until("the sites tab opens", |s| {
        s.tab().url.starts_with("riptide://settings")
    });
    let forget = "document.querySelector('[data-key=\"site:https://meet.example.com:content.media.video_capture#forget\"]')";
    wait_eval(&b, &format!("String(!!{forget})"), "true");
    b.eval(&format!("{forget}.click(), ''"));
    wait_eval(&b, &format!("String(!!{forget})"), "false");
    let start = std::time::Instant::now();
    while saved(&b).contains("video_capture") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "still in {}",
            autoconfig.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // The command does the same.
    b.run("config-unset -u https://meet.example.com content.media.audio_capture");
    let start = std::time::Instant::now();
    while saved(&b).contains("meet.example.com") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the site's table is still there"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Clearing the test page's site takes its cookie and local storage.
    b.eval(&format!(
        "(i => {{ i.value = {origin:?}; document.querySelector('[data-key=\"sites#clear-button\"]').click(); return ''; }})\
         (document.querySelector('[data-key=\"sites#clear\"]'))"
    ));
    b.wait_until("the site is cleared", |s| {
        s.message()
            .is_some_and(|m| m.contains("Cleared cookies and site data"))
    });
    b.run("tab-prev");
    b.wait_until("back on the page", |s| s.tab().url.starts_with("http"));
    b.run("reload");
    let start = std::time::Instant::now();
    loop {
        let left = b.eval("document.cookie + '|' + (localStorage.getItem('rt') ?? '')");
        if left == "|" {
            break;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "left after clearing: {left}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
