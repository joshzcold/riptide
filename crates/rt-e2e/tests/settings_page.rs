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
