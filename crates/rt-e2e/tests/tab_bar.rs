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
