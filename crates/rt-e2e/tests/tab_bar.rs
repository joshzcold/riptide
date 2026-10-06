//! How tabs look in the tab bar: widths, title alignment and the indicator.
#![cfg(unix)]

use rt_e2e::Browser;

/// Poll a value in the tab bar until it is `expected`.
fn wait_bar(b: &Browser, code: &str, expected: &str) {
    let start = std::time::Instant::now();
    loop {
        let got = b.eval_bar("tabbar", code);
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

const FIRST_WIDTH: &str = "String(document.querySelector('.tab').getBoundingClientRect().width)";

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_max_and_min_width_size_the_tabs() {
    let b = Browser::start("nav1.html");
    b.run("set tabs.max_width 150");
    wait_bar(&b, FIRST_WIDTH, "150");
    b.run("set tabs.max_width -1");
    b.run("set tabs.min_width 400");
    for _ in 0..4 {
        b.run("open -t nav2.html");
    }
    b.wait_until("five tabs", |s| s.tabs().len() == 5);
    wait_bar(&b, FIRST_WIDTH, "400");
    // The tabs don't fit, so the bar scrolls to show the current one.
    wait_bar(
        &b,
        "String(document.querySelector('#tabs').scrollLeft > 0)",
        "true",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_title_alignment_and_indicator_width() {
    let b = Browser::start("nav1.html");
    b.run("set tabs.title.alignment center");
    wait_bar(
        &b,
        "getComputedStyle(document.querySelector('.title')).textAlign",
        "center",
    );
    b.run("set tabs.indicator.width 7");
    wait_bar(
        &b,
        "String(document.querySelector('.indicator').getBoundingClientRect().width)",
        "7",
    );
}

/// Press on tab `from`, move the pointer to the middle of tab `over` (and a
/// little past it), and report each tab's transform before releasing.
const DRAG: &str = r#"
(() => {
  const bar = document.getElementById('tabs');
  const tabs = [...bar.children];
  const at = (i) => { const r = tabs[i].getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
  const fire = (type, [x, y]) => bar.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
  const [x0, y0] = at(FROM);
  const [x1, y1] = at(OVER);
  fire('pointerdown', [x0, y0]);
  fire('pointermove', [x0 + 10, y0]);
  fire('pointermove', [x1 + 5, y1]);
  const transforms = tabs.map((t) => t.style.transform || 'none').join('|');
  fire('pointerup', [x1 + 5, y1]);
  return transforms;
})()
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_dragging_shows_the_tab_moving_and_drops_it_there() {
    let b = Browser::start("nav1.html");
    for page in ["second.html", "nav2.html"] {
        let url = b.url(page);
        b.run(&format!("open -t {url}"));
        b.wait_until("the tab loads", |s| s.tab().is_loaded(&url));
    }
    wait_bar(&b, "String(document.querySelectorAll('.tab').length)", "3");
    let transforms = b.eval_bar("tabbar", &DRAG.replace("FROM", "0").replace("OVER", "2"));
    let parts: Vec<&str> = transforms.split('|').collect();
    assert!(
        parts[0].starts_with("translateX("),
        "the dragged tab follows: {transforms}"
    );
    assert!(
        parts[1].starts_with("translateX(-"),
        "the others make room: {transforms}"
    );
    assert!(
        parts[2].starts_with("translateX(-"),
        "the others make room: {transforms}"
    );
    let first = b.url("nav1.html");
    b.wait_until("nav1 is now last", |s| s.tabs()[2].url == first);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_unpinned_tabs_can_sit_between_pinned_ones() {
    let b = Browser::start("nav1.html");
    for page in ["second.html", "nav2.html"] {
        let url = b.url(page);
        b.run(&format!("open -t {url}"));
        b.wait_until("the tab loads", |s| s.tab().is_loaded(&url));
    }
    // Pin the first and last tabs; pinning leaves them in place.
    b.run("tab-pin");
    b.run("tab-focus 1");
    b.run("tab-pin");
    let s = b.wait_until("two pinned tabs", |s| {
        s.tabs().iter().filter(|t| t.pinned).count() == 2
    });
    let pins: Vec<bool> = s.tabs().iter().map(|t| t.pinned).collect();
    assert_eq!(pins, [true, false, true]);
    // Dragging the unpinned tab to the end, past a pinned one, works too.
    wait_bar(&b, "String(document.querySelectorAll('.tab').length)", "3");
    b.eval_bar("tabbar", &DRAG.replace("FROM", "1").replace("OVER", "2"));
    let second = b.url("second.html");
    b.wait_until("the unpinned tab is last", |s| {
        s.tabs()[2].url == second && !s.tabs()[2].pinned && s.tabs()[1].pinned
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_a_wide_tab_drags_past_narrow_pinned_ones() {
    let b = Browser::start("nav1.html");
    for page in ["second.html", "nav2.html"] {
        let url = b.url(page);
        b.run(&format!("open -t {url}"));
        b.wait_until("the tab loads", |s| s.tab().is_loaded(&url));
    }
    // The first two tabs pinned and shrunk; the last one is wide.
    for n in [1, 2] {
        b.run(&format!("tab-focus {n}"));
        b.run("tab-pin");
    }
    b.wait_until("two pinned tabs", |s| {
        s.tabs().iter().filter(|t| t.pinned).count() == 2
    });
    wait_bar(
        &b,
        "String(document.querySelectorAll('.tab.shrunk').length)",
        "2",
    );
    // Grab the wide tab near its left edge and move the pointer onto the first tab.
    let widths = b.eval_bar(
        "tabbar",
        r#"(() => {
          const bar = document.getElementById('tabs');
          const tabs = [...bar.children];
          const wide = tabs[2].getBoundingClientRect();
          const first = tabs[0].getBoundingClientRect();
          const y = wide.top + wide.height / 2;
          const fire = (type, x) => bar.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y }));
          fire('pointerdown', wide.left + 4);
          fire('pointermove', wide.left + 14);
          fire('pointermove', first.left + 2);
          const dragged = tabs[2].getBoundingClientRect().width;
          fire('pointerup', first.left + 2);
          return `${wide.width} ${dragged}`;
        })()"#,
    );
    let (before, during) = widths.split_once(' ').unwrap();
    let (before, during): (f64, f64) = (before.parse().unwrap(), during.parse().unwrap());
    assert!(during < before / 2.0, "it shrinks while dragged: {widths}");
    let nav2 = b.url("nav2.html");
    b.wait_until("the wide tab is first", |s| s.tabs()[0].url == nav2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_bar_middle_click_on_empty_space_opens_a_tab() {
    let b = Browser::start("nav1.html");
    wait_bar(&b, "String(document.querySelectorAll('.tab').length)", "1");
    let click = "document.getElementById('tabs').dispatchEvent(new MouseEvent('auxclick', { button: 1, bubbles: true })), ''";
    b.eval_bar("tabbar", click);
    b.wait_until("a new tab", |s| s.tabs().len() == 2);
    b.run("set tabs.close_mouse_button_on_bar close-current");
    b.eval_bar("tabbar", click);
    b.wait_until("the current tab closes", |s| s.tabs().len() == 1);
}
