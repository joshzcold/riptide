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
    b.keys("so");
    let s = b.wait_until("one pane", |s| s.window().panes.len() == 1);
    assert_eq!(s.tabs().len(), 3);
    assert_eq!(s.tab().title, "second");
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
