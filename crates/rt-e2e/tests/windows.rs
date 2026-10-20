//! Windows and private windows.
#![cfg(unix)]

use rt_e2e::{Bounds, Browser};

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_w_opens_a_window_and_close_closes_it() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -w {second}"));
    let s = b.wait_until("a second window", |s| {
        s.windows.len() == 2 && s.current_window == 1 && s.tab().url == second
    });
    assert!(!s.window().private);
    // Keys go to the new window.
    b.run("set messages.timeout 0");
    b.keys(":close<Return>");
    b.wait_until("one window", |s| s.windows.len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn q_closes_this_window_and_qa_quits() {
    let b = Browser::start("page.html");
    b.run(&format!("open -w {}", b.url("second.html")));
    b.wait_until("a second window", |s| {
        s.windows.len() == 2 && s.current_window == 1
    });
    b.keys(":q<Return>");
    let s = b.wait_until("only the first window is left", |s| s.windows.len() == 1);
    assert_eq!(s.tab().url, b.url("page.html"));
    b.run(&format!("open -w {}", b.url("second.html")));
    b.wait_until("two windows again", |s| s.windows.len() == 2);
    b.keys(":qa<Return>");
    assert!(b.wait_exit().success());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn open_p_opens_a_private_window() {
    let b = Browser::start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -p {second}"));
    let s = b.wait_until("a private window", |s| {
        s.windows.len() == 2 && s.window().private && s.tab().url == second
    });
    assert!(!s.windows[0].private);
}

const PLACED: Bounds = Bounds {
    x: 60,
    y: 40,
    width: 900,
    height: 600,
};
const OTHER: Bounds = Bounds {
    x: 300,
    y: 150,
    width: 700,
    height: 500,
};

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn the_first_window_opens_where_the_last_one_closed() {
    let b = Browser::start("page.html");
    b.set_window_bounds(PLACED);
    b.wait_until("the window moved", |s| s.window().bounds == Some(PLACED));
    b.run("quit");
    assert!(b.wait_exit().success());
    b.restart();
    b.wait_until("the window is back where it was", |s| {
        s.window().bounds == Some(PLACED)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_new_window_takes_the_active_windows_size() {
    let b = Browser::start("page.html");
    b.set_window_bounds(PLACED);
    b.wait_until("the window moved", |s| s.window().bounds == Some(PLACED));
    b.run(&format!("open -w {}", b.url("second.html")));
    b.wait_until("a second window of the same size", |s| {
        s.windows.len() == 2
            && s.current_window == 1
            && s.window()
                .bounds
                .is_some_and(|w| (w.width, w.height) == (PLACED.width, PLACED.height))
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_saved_session_puts_each_window_back() {
    let config = "url.start_pages = [\"about:blank\"]\nauto_save.session = true\n";
    let b = Browser::launch().toml(config).start("page.html");
    b.set_window_bounds(PLACED);
    b.wait_until("the window moved", |s| s.window().bounds == Some(PLACED));
    let second = b.url("second.html");
    b.run(&format!("open -w {second}"));
    b.wait_until("a second window", |s| {
        s.windows.len() == 2 && s.current_window == 1
    });
    b.set_window_bounds(OTHER);
    b.wait_until("the second window moved", |s| {
        s.window().bounds == Some(OTHER)
    });
    b.run("wq");
    assert!(b.wait_exit().success());
    let session = std::fs::read_to_string(b.data_dir().join("sessions/default.toml")).unwrap();
    assert!(session.contains("[windows.geometry]"), "{session}");
    b.restart();
    b.wait_until("both windows are back where they were", |s| {
        let mut bounds: Vec<_> = s.windows.iter().filter_map(|w| w.bounds).collect();
        bounds.sort_by_key(|b| b.x);
        bounds == [PLACED, OTHER]
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_window_saved_on_a_screen_thats_gone_keeps_only_its_size() {
    let saved = "x = 5000\ny = 40\nwidth = 900\nheight = 600\n";
    let b = Browser::launch()
        .file("data/window.toml", saved)
        .start("page.html");
    let s = b.wait_until("the window is open", |s| s.window().bounds.is_some());
    let bounds = s.window().bounds.unwrap();
    assert_eq!((bounds.width, bounds.height), (900, 600));
    assert!(bounds.x < 1280, "{bounds:?}");
}
