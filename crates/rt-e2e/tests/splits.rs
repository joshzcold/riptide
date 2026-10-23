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
            let title = &w.tabs[p.tab].title;
            if p.focused {
                format!("*{title}")
            } else {
                title.clone()
            }
        })
        .collect()
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
    assert_eq!(
        (left.x, left.height, right.height),
        (0, full.height, full.height)
    );
    assert!(
        right.x > left.x + left.width,
        "a gap between them: {halves:?}"
    );
    assert!((left.width - right.width).abs() <= 1, "{halves:?}");
    assert_eq!(s.tabs().len(), 2);
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
fn picking_a_tab_another_pane_shows_focuses_that_pane() {
    let b = Browser::start("page.html");
    b.run(&format!("vsplit {}", b.url("second.html")));
    b.wait_until("two panes", |s| panes(s) == ["ready", "*second"]);
    b.keys("K");
    b.wait_until("the left pane", |s| panes(s) == ["*ready", "second"]);
    b.keys("J");
    b.wait_until("the right pane", |s| panes(s) == ["ready", "*second"]);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn sc_and_so_close_panes_but_keep_their_tabs() {
    let b = three_panes();
    b.keys("sc");
    let s = b.wait_until("two panes", |s| s.window().panes.len() == 2);
    assert_eq!(s.tabs().len(), 3, "the pane's tab stays open");
    assert_eq!(panes(&s), ["ready", "*second"]);
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
    assert_eq!(s.tabs().len(), 3, "the pane's tab stays open");
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
    // Three tabs on screen, so the pane has nothing left to show.
    b.keys("d");
    let s = b.wait_until("two panes", |s| {
        s.tabs().len() == 2 && s.window().panes.len() == 2
    });
    assert!(
        panes(&s).iter().any(|p| p.starts_with('*')),
        "{:?}",
        panes(&s)
    );
    b.run(&format!("open -b {}", b.url("page.html?more")));
    b.wait_until("three tabs", |s| s.tabs().len() == 3);
    b.keys("d");
    b.wait_until("the pane shows the tab that wasn't on screen", |s| {
        s.tabs().len() == 2 && s.window().panes.len() == 2
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
        panes(s) == ["*ready", "second", "dialogs"] && w[0] < w[1]
    });
    let saved = widths(&s);
    b.run("wq");
    assert!(b.wait_exit().success());
    let session = std::fs::read_to_string(b.data_dir().join("sessions/default.toml")).unwrap();
    assert!(session.contains("[windows.splits]"), "{session}");

    b.restart();
    let s = b.wait_until("the panes are back", |s| {
        panes(s) == ["*ready", "second", "dialogs"]
    });
    // With lazy restore, tabs load once they're on screen, in any pane.
    for pane in &s.window().panes {
        let url = s.window().tabs[pane.tab].url.clone();
        let start = std::time::Instant::now();
        while b.eval_tab(pane.tab, "location.href") != url {
            assert!(start.elapsed() < rt_e2e::TIMEOUT, "{url} didn't load");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let restored = widths(&s);
    for (a, b) in saved.iter().zip(&restored) {
        assert!((a - b).abs() <= 2, "{saved:?} then {restored:?}");
    }
}
