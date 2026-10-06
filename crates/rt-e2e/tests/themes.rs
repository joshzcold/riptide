//! Themes: `:theme` and `colors.*` reach the bars' pages.
#![cfg(unix)]

use rt_e2e::Browser;

fn statusbar_bg(b: &Browser) -> String {
    b.eval_bar(
        "statusbar",
        "getComputedStyle(document.body).backgroundColor",
    )
}

fn wait_bg(b: &Browser, expected: &str) {
    let start = std::time::Instant::now();
    while statusbar_bg(b) != expected {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the status bar is {}, not {expected}",
            statusbar_bg(b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn theme_and_color_settings_restyle_the_bars() {
    let b = Browser::start("page.html");
    // riptide's own #061826.
    wait_bg(&b, "rgb(6, 24, 38)");
    b.run("theme nord");
    wait_bg(&b, "rgb(36, 41, 51)");
    b.run("set colors.statusbar.normal.bg #102030");
    wait_bg(&b, "rgb(16, 32, 48)");
    b.run("set colors.statusbar.normal.bg nonsense;");
    b.wait_until("a bad color is refused", |s| {
        s.message().is_some_and(|m| m.contains("isn't a color"))
    });
}

/// Poll `code` until it returns `expected`.
fn wait_for(b: &Browser, what: &str, code: impl Fn(&Browser) -> String, expected: &str) {
    let start = std::time::Instant::now();
    loop {
        let got = code(b);
        if got == expected {
            return;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{what} is {got:?}, not {expected:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn fonts_restyle_the_bars_and_pages() {
    let b = Browser::start("page.html");
    b.run("set fonts.statusbar 11px serif");
    wait_for(
        &b,
        "the status bar font",
        |b| {
            b.eval_bar(
                "statusbar",
                "(s => s.fontSize + ' ' + s.fontFamily)(getComputedStyle(document.body))",
            )
        },
        "11px serif",
    );
    b.run("set fonts.web.size.default 21");
    b.run("reload");
    wait_for(
        &b,
        "the page's text size",
        |b| b.eval("getComputedStyle(document.body).fontSize"),
        "21px",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ui_css_and_user_stylesheets_apply_and_reload() {
    let b = Browser::launch()
        .file(
            "config/ui.css",
            "#bar { background: rgb(1, 2, 3) !important; }",
        )
        .file(
            "config/page.css",
            "body { background: rgb(4, 5, 6) !important; }",
        )
        .toml("\"content.user_stylesheets\" = [\"page.css\"]\n")
        .start("page.html");
    let bar = |b: &Browser| {
        b.eval_bar(
            "statusbar",
            "getComputedStyle(document.getElementById('bar')).backgroundColor",
        )
    };
    let page = |b: &Browser| b.eval("getComputedStyle(document.body).backgroundColor");
    wait_for(&b, "the status bar", bar, "rgb(1, 2, 3)");
    wait_for(&b, "the page", page, "rgb(4, 5, 6)");
    // Edits apply without reloading anything.
    let config = b.config_dir();
    std::fs::write(
        config.join("ui.css"),
        "#bar { background: rgb(7, 8, 9) !important; }",
    )
    .unwrap();
    std::fs::write(
        config.join("page.css"),
        "body { background: rgb(10, 11, 12) !important; }",
    )
    .unwrap();
    wait_for(&b, "the status bar after the edit", bar, "rgb(7, 8, 9)");
    wait_for(&b, "the page after the edit", page, "rgb(10, 11, 12)");
}

/// Wait until `bar`'s page is taller (or shorter) than `than` pixels.
fn wait_height(b: &Browser, bar: &str, what: &str, grow: bool, than: i64) -> i64 {
    let start = std::time::Instant::now();
    loop {
        let height: i64 = b.eval_bar(bar, "String(innerHeight)").parse().unwrap_or(0);
        if (grow && height > than) || (!grow && height > 0 && height <= than) {
            return height;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{what}: the {bar} is {height}px"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn bars_grow_with_their_fonts_and_padding() {
    let b = Browser::start("page.html");
    wait_height(&b, "statusbar", "the default status bar", false, 20);
    b.run("set fonts.statusbar 18pt default_family");
    let tall = wait_height(&b, "statusbar", "a big font", true, 20);
    b.run("set statusbar.padding 10px 4px");
    wait_height(&b, "statusbar", "padding", true, tall);
    b.run("set fonts.tabs.selected 18pt default_family");
    b.run("set fonts.tabs.unselected 18pt default_family");
    wait_height(&b, "tabbar", "a big tab font", true, 20);
    b.run("set statusbar.padding 4em");
    b.wait_until("a bad padding is refused", |s| {
        s.message().is_some_and(|m| m.contains("padding"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn auto_theme_follows_the_light_or_dark_preference() {
    let b = Browser::launch()
        .toml("\"ui.theme\" = \"auto\"\n\"ui.auto_theme.light\" = \"nord\"\n")
        .start("page.html");
    b.run("set colors.webpage.preferred_color_scheme dark");
    // riptide's own #061826.
    wait_bg(&b, "rgb(6, 24, 38)");
    b.run("set colors.webpage.preferred_color_scheme light");
    // nord's #242933.
    wait_bg(&b, "rgb(36, 41, 51)");
    b.run("set ui.auto_theme.light dracula");
    let start = std::time::Instant::now();
    while statusbar_bg(&b) == "rgb(36, 41, 51)" {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "still nord");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn theme_command_previews_the_theme_until_escape() {
    let b = Browser::start("page.html");
    wait_bg(&b, "rgb(6, 24, 38)");
    b.keys(":theme ");
    b.wait_mode("command");
    std::thread::sleep(std::time::Duration::from_millis(200));
    let before = statusbar_bg(&b);
    b.keys("dracula");
    let start = std::time::Instant::now();
    while statusbar_bg(&b) == before {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "no preview: {before}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.keys("<Escape>");
    wait_bg(&b, "rgb(6, 24, 38)");
    b.run("set ui.theme");
    b.wait_until("ui.theme is unchanged", |s| {
        s.message().is_some_and(|m| m.contains("riptide"))
    });
}
