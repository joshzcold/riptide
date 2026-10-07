//! `:call-mute` (`cm`) presses the call site's mute key in the call's tab,
//! from another tab.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn cm_presses_the_mute_key_in_the_call_tab_from_another_tab() {
    let b = Browser::launch()
        // Chromium's test microphone; the setting skips the prompt.
        .arg("--use-fake-device-for-media-stream")
        .toml(
            "[content.media]\naudio_capture = \"true\"\n\
             [content.call_mute_keys]\n\"127.0.0.1/call.html\" = \"<Ctrl-d>\"\n",
        )
        .start("call.html");
    b.wait_eval(
        "String(window.__started = window.__started || (startCall(), true))",
        "true",
    );
    b.wait_until("the call uses the microphone", |_| {
        b.eval_bar("tabbar", "document.body.innerText")
            .contains("[A] 1:")
    });
    b.run(&format!("open -t {}", b.url("second.html")));
    b.wait_until("another tab is in front", |s| {
        s.window().current_tab == 1 && s.tab().is_loaded(&b.url("second.html"))
    });

    // The first press waits for the page to take keys; the second goes at once.
    for presses in ["1", "2"] {
        b.keys("cm");
        let start = std::time::Instant::now();
        while b.eval_tab(0, "String(window.__mutes)") != presses {
            assert!(
                start.elapsed() < rt_e2e::TIMEOUT,
                "the call tab got no mute key ({presses})"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        b.wait_until("back on the tab we were on", |s| {
            s.window().current_tab == 1
        });
    }
    let s = b.state();
    assert!(
        s.message()
            .unwrap_or_default()
            .contains("to mute or unmute"),
        "{s:?}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn cm_says_when_there_is_no_call() {
    let b = Browser::start("page.html");
    b.keys("cm");
    b.wait_until("it says so", |s| {
        s.message()
            .is_some_and(|m| m.contains("No tab is using a microphone"))
    });
}
