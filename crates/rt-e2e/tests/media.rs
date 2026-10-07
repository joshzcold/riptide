//! A tab using the camera, microphone or screen is marked in the tab bar and
//! status bar, and keeps running at full speed in the background.
#![cfg(unix)]

use rt_e2e::Browser;

fn start() -> Browser {
    Browser::launch()
        // Chromium's test camera and microphone; the settings skip the prompt.
        .arg("--use-fake-device-for-media-stream")
        .toml("content.autoplay = true\n[content.media]\naudio_capture = \"true\"\nvideo_capture = \"true\"\n")
        .start("media.html")
}

fn ticks(b: &Browser, tab: usize) -> u64 {
    b.eval_tab(tab, "String(window.__ticks)").parse().unwrap()
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_tab_using_the_camera_and_microphone_is_marked() {
    let b = start();
    b.eval("startCall(); 'started'");
    b.wait_until("the tab bar marks the call", |_| {
        b.eval_bar("tabbar", "document.body.innerText")
            .contains("[A/V] 1:")
    });
    b.wait_until("so does the status bar", |_| {
        b.eval_bar("statusbar", "document.body.innerText")
            .contains("[A/V]")
    });
    b.eval("endCall()");
    b.wait_until("the mark goes when the call ends", |_| {
        !b.eval_bar("tabbar", "document.body.innerText")
            .contains("[A/V]")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_call_in_a_background_tab_keeps_its_pace() {
    let b = start();
    b.eval("startCall(); 'started'");
    b.wait_until("the call is live", |_| {
        b.eval_bar("tabbar", "document.body.innerText")
            .contains("[A/V]")
    });
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("the call tab is in the background", |s| {
        s.window().current_tab == 1 && s.tab().is_loaded(&b.url("second.html"))
    });
    let before = ticks(&b, 0);
    std::thread::sleep(std::time::Duration::from_secs(3));
    let after = ticks(&b, 0);
    eprintln!("background call: {} ticks of 100 ms in 3 s", after - before);
    // A hidden page's timers run about once a second (3 ticks), as in Chrome,
    // unless it plays sound, as a call does with the others' voices.
    assert!(after - before >= 20, "{} ticks in 3 s", after - before);
}
