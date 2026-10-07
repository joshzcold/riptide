//! Window settings that reach the window manager (read back with xprop),
//! and what the first start after an upgrade shows.
#![cfg(unix)]

use std::process::Command;

use rt_e2e::Browser;

/// An X property of the browser window, as xprop prints it; `args` may
/// start with xprop options such as a format.
fn window_property(b: &Browser, args: &[&str]) -> String {
    let display = format!(":{}", b.display());
    let tree = Command::new("xwininfo")
        .args(["-display", &display, "-root", "-tree"])
        .output()
        .expect("xwininfo is needed for window tests");
    let tree = String::from_utf8_lossy(&tree.stdout).to_string();
    let id = tree
        .lines()
        .find(|l| l.contains("Riptide\":"))
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_else(|| panic!("no Riptide window in\n{tree}"))
        .to_string();
    let output = Command::new("xprop")
        .args(["-display", &display, "-id", &id])
        .args(args)
        .output()
        .expect("xprop is needed for window tests");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// The decorations field of `_MOTIF_WM_HINTS` on the browser window:
/// 1 with a title bar and borders, 0 without.
fn decorations(b: &Browser) -> String {
    let hints = window_property(b, &["_MOTIF_WM_HINTS"]);
    // "_MOTIF_WM_HINTS(_MOTIF_WM_HINTS) = flags, functions, decorations, …"
    hints
        .split_once('=')
        .and_then(|(_, values)| values.split(',').nth(2))
        .unwrap_or_else(|| panic!("unexpected hints: {hints}"))
        .trim()
        .to_string()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_window_has_a_class_for_window_managers_and_pickers() {
    let b = Browser::start("page.html");
    assert_eq!(
        window_property(&b, &["WM_CLASS"]).trim(),
        r#"WM_CLASS(STRING) = "riptide", "Riptide""#
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_window_icon_is_riptides_logo() {
    let b = Browser::start("page.html");
    // `_NET_WM_ICON` is a list of icons: width, height, then ARGB pixels.
    let text = window_property(
        &b,
        &["-notype", "-f", "_NET_WM_ICON", "32c", "_NET_WM_ICON"],
    );
    let values: Vec<u64> = text
        .split_once('=')
        .map(|(_, v)| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
        .unwrap_or_default();
    let mut icons = Vec::new();
    let mut rest = values.as_slice();
    while let [w, h, pixels @ ..] = rest {
        let (w, h) = (*w as usize, *h as usize);
        let Some((icon, after)) = (w * h <= pixels.len()).then(|| pixels.split_at(w * h)) else {
            break;
        };
        icons.push((w, h, icon));
        rest = after;
    }
    let sizes: Vec<_> = icons.iter().map(|(w, h, _)| (*w, *h)).collect();
    let (w, _, pixels) = icons
        .iter()
        .find(|(w, h, _)| (*w, *h) == (128, 128))
        .unwrap_or_else(|| panic!("no 128x128 icon among {sizes:?}"));
    // The logo's dark navy near its top left corner, which Chromium's icon doesn't have.
    let argb = pixels[20 * w + 20];
    let (r, g, bl) = ((argb >> 16) & 0xff, (argb >> 8) & 0xff, argb & 0xff);
    assert!(
        r < 40 && (30..80).contains(&g) && (50..110).contains(&bl),
        "pixel {argb:#x}; icons {sizes:?}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn window_hide_decoration_asks_for_no_title_bar() {
    let b = Browser::start("page.html");
    assert_eq!(decorations(&b), "0x1");
    let b = Browser::launch()
        .toml("window.hide_decoration = true\n")
        .start("page.html");
    assert_eq!(decorations(&b), "0x0");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn changelog_after_upgrade_opens_the_changelog_once() {
    let changelog = |s: &rt_e2e::State| s.tabs().iter().any(|t| t.url == "riptide://changelog/");
    let b = Browser::start("page.html");
    assert!(!changelog(&b.state()), "the first start isn't an upgrade");
    let restart = |b: &Browser| {
        b.run("q");
        assert!(b.wait_exit().success());
        b.restart();
        b.wait_until("the start page", |s| !s.tabs().is_empty());
    };
    // As if the previous start was an older release.
    std::fs::write(b.data_dir().join("last-version"), "0.0.1").unwrap();
    restart(&b);
    b.wait_until("the changelog opens", changelog);
    restart(&b);
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!changelog(&b.state()), "the changelog opened again");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tabs_are_windows_opens_tabs_and_popups_as_windows() {
    let b = Browser::start("page.html");
    let height = |b: &Browser| -> i64 { b.eval("String(innerHeight)").parse().unwrap() };
    let with_bar = height(&b);
    b.run("set tabs.tabs_are_windows true");
    let start = std::time::Instant::now();
    while height(&b) <= with_bar {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the tab bar is still shown"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("the tab is a new window", |s| {
        s.windows.len() == 2 && s.windows.iter().all(|w| w.tabs.len() == 1)
    });
    // d closes the window that is the tab.
    b.keys("d");
    b.wait_until("one window again", |s| s.windows.len() == 1);
    b.run("set content.javascript.can_open_tabs_automatically true");
    b.run(&format!("open {}", b.url("popup.html")));
    b.wait_until("the popup is a new window", |s| {
        s.windows.len() == 2 && s.windows.iter().all(|w| w.tabs.len() == 1)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn prompts_float_near_the_bottom_by_default() {
    let b = Browser::start("page.html");
    let window_width = b.eval("String(innerWidth)");
    b.eval("Notification.requestPermission(); ''");
    b.wait_mode("yesno");
    let overlay = |b: &Browser| {
        b.eval_bar(
            "completion",
            "innerWidth + ' ' + document.body.classList.contains('floating')",
        )
    };
    let start = std::time::Instant::now();
    while overlay(&b) != "640 true" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "not floating: {}",
            overlay(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.run("set prompt.position docked");
    let docked = format!("{window_width} false");
    let start = std::time::Instant::now();
    while overlay(&b) != docked {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "not docked: {}",
            overlay(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn prompt_buttons_answer_with_their_key_and_nothing_else() {
    let b = Browser::start("page.html");
    b.eval(
        "Notification.requestPermission().then((p) => { document.title = 'answered ' + p; }); ''",
    );
    b.wait_mode("yesno");
    let click = |b: &Browser, keys: &str| {
        b.eval_bar(
            "completion",
            &format!(
                "[...document.querySelectorAll('.prompt-option')].find((o) => o.querySelector('kbd').textContent === {keys:?})?.click(), ''"
            ),
        );
    };
    // A key this prompt doesn't offer is ignored, even sent straight from the page.
    b.eval_bar(
        "completion",
        "rt.send('prompt-key', JSON.stringify({ key: 'q' })), ''",
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(b.state().mode, "yesno");
    click(&b, "n");
    b.wait_until("n answered it", |s| {
        s.mode == "normal" && s.tab().title == "answered default"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn prompts_say_what_they_are_about_and_can_be_centered() {
    let b = Browser::launch()
        .toml("\"prompt.position\" = \"center\"\n")
        .start("page.html");
    b.eval("Notification.requestPermission(); ''");
    b.wait_mode("yesno");
    let classes = |b: &Browser| b.eval_bar("completion", "document.body.className");
    let start = std::time::Instant::now();
    while classes(&b) != "floating prompt-permission" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the overlay's classes are {:?}",
            classes(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.keys("<Escape>");
    b.wait_mode("normal");
    b.eval("setTimeout(() => confirm('sure?')); ''");
    b.wait_mode("yesno");
    let start = std::time::Instant::now();
    while classes(&b) != "floating prompt-dialog" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the overlay's classes are {:?}",
            classes(&b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
