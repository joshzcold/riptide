//! Split panes: several tabs on screen in one window.
#![cfg(unix)]

use rt_e2e::{Browser, State};

/// Each pane's tab title, top-left first, with the focused one marked `*`.
fn panes(s: &State) -> Vec<String> {
    let w = s.window();
    let mut panes = w.panes.clone();
    panes.sort_by_key(|p| (p.bounds.x, p.bounds.y));
    panes
        .iter()
        .map(|p| {
            let title = p.title.clone().unwrap_or_default();
            if p.focused {
                format!("*{title}")
            } else {
                title
            }
        })
        .collect()
}

/// How many tabs every pane has in all.
fn total(s: &State) -> usize {
    s.window().panes.iter().map(|p| p.tabs.len()).sum()
}

/// `ready` on the left, `second` top right and `dialogs` bottom right (focused).
fn three_panes() -> Browser {
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.run(&format!("split {}", b.url("dialogs.html")));
    b.wait_until("three panes", |s| {
        panes(s) == ["ready", "second", "*dialogs"]
    });
    b
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn vsplit_shows_a_new_tab_beside_the_current_one() {
    let b = Browser::start("page.html");
    let one = b.wait_until("one pane", |s| s.window().panes.len() == 1);
    let full = one.window().panes[0].bounds;
    assert!(
        full.width > 100 && full.height > 100,
        "the first tab has a size: {full:?}"
    );
    b.run(&format!("vsplit {}", b.url("second.html")));
    let s = b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    let mut halves: Vec<_> = s.window().panes.iter().map(|p| p.bounds).collect();
    halves.sort_by_key(|b| b.x);
    let (left, right) = (halves[0], halves[1]);
    // Each pane's page sits below the strip that marks the focused one.
    assert_eq!(
        (left.x, left.y, left.height, right.y, right.height),
        (0, 2, full.height - 2, 2, full.height - 2)
    );
    assert!(
        right.x > left.x + left.width,
        "a gap between them: {halves:?}"
    );
    assert!((left.width - right.width).abs() <= 1, "{halves:?}");
    assert!(
        s.window().panes.iter().all(|p| p.tabs.len() == 1),
        "each pane has a tab of its own"
    );
    // A handle to drag sits over the gap.
    let s = b.wait_until("a handle", |s| s.window().split_handles.len() == 1);
    let handle = s.window().split_handles[0];
    assert!(
        handle.x <= left.x + left.width && handle.x + handle.width >= right.x,
        "{handle:?} {halves:?}"
    );
    assert_eq!(handle.height, full.height);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn ctrl_hjkl_and_sw_move_between_panes() {
    let b = three_panes();
    b.keys("<Ctrl-h>");
    b.wait_until("the left pane", |s| {
        panes(s) == ["*ready", "second", "dialogs"]
    });
    assert_eq!(
        b.state().tab().title,
        "ready",
        "the focused pane's tab is current"
    );
    b.keys("<Ctrl-l>");
    b.wait_until("the upper right pane", |s| {
        panes(s) == ["ready", "*second", "dialogs"]
    });
    b.keys("<Ctrl-j>");
    b.wait_until("the lower right pane", |s| {
        panes(s) == ["ready", "second", "*dialogs"]
    });
    b.keys("<Ctrl-k>");
    b.wait_until("back up", |s| panes(s) == ["ready", "*second", "dialogs"]);
    b.keys("sw");
    b.wait_until("the next pane", |s| {
        panes(s) == ["ready", "second", "*dialogs"]
    });
    b.keys("sw");
    b.wait_until("round to the first", |s| {
        panes(s) == ["*ready", "second", "dialogs"]
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn each_pane_has_tabs_of_its_own() {
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.run(&format!("open -t {}", b.url("dialogs.html")));
    let s = b.wait_until("a second tab on the right", |s| {
        panes(s) == ["ready", "*dialogs"]
    });
    assert_eq!(
        s.tabs().len(),
        2,
        "tab commands see the focused pane's tabs"
    );
    // J and K go through the focused pane's tabs only.
    b.keys("K");
    b.wait_until("back to second", |s| panes(s) == ["ready", "*second"]);
    b.keys("K");
    b.wait_until("round to dialogs", |s| panes(s) == ["ready", "*dialogs"]);
    b.keys("<Ctrl-h>");
    let s = b.wait_until("the left pane", |s| panes(s) == ["*ready", "dialogs"]);
    assert_eq!(s.tabs().len(), 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn tab_move_pane_moves_the_tab_and_the_focus() {
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.run(&format!("open -t {}", b.url("dialogs.html")));
    b.wait_until("dialogs on the right", |s| {
        panes(s) == ["ready", "*dialogs"]
    });
    b.keys("sm");
    let s = b.wait_until("dialogs went left", |s| panes(s) == ["*dialogs", "second"]);
    assert_eq!(s.tabs().len(), 2);
    // Moving a pane's only tab closes the pane.
    b.keys("sL");
    b.wait_until("dialogs went right", |s| panes(s) == ["ready", "*dialogs"]);
    b.keys("<Ctrl-h>");
    b.wait_until("the left pane", |s| panes(s) == ["*ready", "dialogs"]);
    b.run("tab-move-pane right");
    let s = b.wait_until("one pane with every tab", |s| s.window().panes.len() == 1);
    assert_eq!(s.tabs().len(), 3);
    assert_eq!(s.tab().title, "ready");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn sc_and_so_close_panes_but_keep_their_tabs() {
    let b = three_panes();
    b.keys("sc");
    let s = b.wait_until("two panes", |s| s.window().panes.len() == 2);
    assert_eq!(panes(&s), ["ready", "*second"]);
    assert_eq!(
        s.tabs().len(),
        2,
        "the closed pane's tab joins the pane that takes its room"
    );
    assert_eq!(s.window().split_handles.len(), 1);
    b.keys("so");
    let s = b.wait_until("one pane", |s| s.window().panes.len() == 1);
    assert!(s.window().split_handles.is_empty());
    assert_eq!(s.tabs().len(), 3);
    assert_eq!(s.tab().title, "second");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn q_closes_the_focused_pane_and_then_the_window() {
    let b = three_panes();
    b.run("q");
    let s = b.wait_until("two panes", |s| s.window().panes.len() == 2);
    assert_eq!(s.windows.len(), 1, "the window stays open");
    assert_eq!(total(&s), 3, "the pane's tab stays open");
    assert_eq!(panes(&s), ["ready", "*second"]);
    b.run("q");
    b.wait_until("one pane", |s| s.window().panes.len() == 1);
    b.run("q");
    assert!(
        b.wait_exit().success(),
        "the last pane's :q closes the window"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn closing_a_panes_tab_shows_another_or_closes_the_pane() {
    let b = three_panes();
    // The pane's only tab, so the pane closes too.
    b.keys("d");
    let s = b.wait_until("two panes", |s| {
        total(s) == 2 && s.window().panes.len() == 2
    });
    assert_eq!(panes(&s), ["ready", "*second"]);
    b.run(&format!("open -b {}", b.url("dialogs.html")));
    b.wait_until("a second tab in the pane", |s| s.tabs().len() == 2);
    b.keys("d");
    b.wait_until("the pane shows its other tab", |s| {
        panes(s) == ["ready", "*dialogs"] && total(s) == 2
    });
}

fn widths(s: &State) -> Vec<i32> {
    let mut panes = s.window().panes.clone();
    panes.sort_by_key(|p| (p.bounds.x, p.bounds.y));
    panes.iter().map(|p| p.bounds.width).collect()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn s_greater_and_s_minus_resize_the_focused_pane() {
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    let s = b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    let before = widths(&s);
    b.keys("s>");
    b.wait_until("the right pane is wider", |s| {
        let w = widths(s);
        w.len() == 2 && w[1] > before[1] + 50 && w[0] < before[0] - 50
    });
    b.keys("s=");
    b.wait_until("equal again", |s| widths(s) == before);
    b.keys("s-");
    b.wait_until("nothing runs up and down", |s| {
        s.message().is_some_and(|m| m.contains("no neighbour"))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn splits_come_back_with_the_session() {
    let b = Browser::launch()
        .toml(
            "url.start_pages = [\"about:blank\"]\nauto_save.session = true\nsession.lazy_restore = true\n",
        )
        .start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.run(&format!("split {}", b.url("dialogs.html")));
    b.wait_until("three panes", |s| {
        panes(s) == ["ready", "second", "*dialogs"]
    });
    b.keys("s>");
    b.keys("<Ctrl-h>");
    let s = b.wait_until("the left pane, narrower", |s| {
        let w = widths(s);
        panes(s) == ["*ready", "second", "dialogs"] && w[0] + 50 < w[1]
    });
    let saved = widths(&s);
    b.run("wq");
    assert!(b.wait_exit().success());
    let session = std::fs::read_to_string(b.data_dir().join("sessions/default.toml")).unwrap();
    assert!(session.contains("[windows.splits]"), "{session}");

    b.restart();
    b.wait_until("the panes are back", |s| {
        panes(s) == ["*ready", "second", "dialogs"]
    });
    // With lazy restore, tabs load once they're on screen, in any pane.
    let s = b.wait_until("every pane's page loads", |s| {
        s.window()
            .panes
            .iter()
            .all(|p| p.page_url.as_deref().is_some_and(|u| u.starts_with("http")))
    });
    let restored = widths(&s);
    for (a, b) in saved.iter().zip(&restored) {
        assert!((a - b).abs() <= 2, "{saved:?} then {restored:?}");
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn panes_stop_trading_focus_after_tab_switches() {
    // Focusing a tab reports back as a click; stale reports once made two panes take turns for good.
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.run(&format!("open -b {}", b.url("dialogs.html")));
    b.wait_until("two tabs on the right", |s| s.window().tabs.len() == 2);
    for command in [
        "split-focus left",
        "split-focus right",
        "split-focus left",
        "split-focus right",
        "tab-next",
        "tab-next",
        "tab-next",
    ] {
        b.run(command);
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let settled = panes(&b.state());
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(panes(&b.state()), settled, "the panes keep changing");
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn vsplit_m_moves_the_current_tab_into_the_new_pane() {
    let b = Browser::start("page.html");
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("second is current", |s| s.tab().title == "second");
    // The old pane shows the tab used before.
    b.run("vsplit -m");
    let s = b.wait_until("second moved right", |s| panes(s) == ["ready", "*second"]);
    assert_eq!(total(&s), 2);
    // With no other tab, the old pane gets a new one.
    b.run("split -m");
    let s = b.wait_until("second moved down", |s| s.window().panes.len() == 3);
    assert_eq!(total(&s), 3);
    assert_eq!(s.tab().title, "second");
}
