//! `:media-devices` and `content.media.camera`, `.microphone` and `.speaker`:
//! the devices a site gets, switched in a call in progress.
#![cfg(unix)]

use rt_e2e::Browser;
use serde_json::Value;

/// A call on `call-devices.html` with Chromium's fake devices: two cameras
/// (`fake_device_0`, `_1`), and "Fake Audio Input 1/2" and "Output 1/2"
/// besides the defaults.
fn start_call(extra: &str) -> Browser {
    let b = Browser::launch()
        .arg("--use-fake-device-for-media-stream=device-count=2")
        .toml(&format!(
            "[content.media]\naudio_capture = \"true\"\nvideo_capture = \"true\"\n{extra}"
        ))
        .start("call-devices.html");
    b.wait_eval(
        "String(window.__on = window.__on || (call(), true))",
        "true",
    );
    b.wait_until("the call is up", |_| {
        b.eval("String(!!window.__near)") == "true"
    });
    b
}

/// What the call uses once `done` holds for it.
fn wait_in_use(b: &Browser, what: &str, done: impl Fn(&Value) -> bool) -> Value {
    let start = std::time::Instant::now();
    loop {
        b.eval("inUse(), ''");
        std::thread::sleep(std::time::Duration::from_millis(100));
        let now: Value = serde_json::from_str(&b.eval("String(window.__inUse || 'null')")).unwrap();
        if done(&now) {
            return now;
        }
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "{what}: {now}");
    }
}

fn has(value: &Value, key: &str, device: &str) -> bool {
    value[key]
        .as_array()
        .is_some_and(|a| a.iter().any(|d| d == device))
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_site_gets_the_devices_set_for_it() {
    let b = start_call(
        "[per_domain.\"{server}\"]\n\"content.media.camera\" = \"fake_device_1\"\n\
         \"content.media.microphone\" = \"Fake Audio Input 2\"\n\
         \"content.media.speaker\" = \"Fake Audio Output 1\"\n",
    );
    wait_in_use(&b, "the picked devices", |v| {
        ["stream", "sent"]
            .iter()
            .all(|k| has(v, k, "fake_device_1") && has(v, k, "Fake Audio Input 2"))
            && v["speaker"] == "Fake Audio Output 1"
            && v["live"] == true
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_site_keeping_get_user_media_early_still_gets_its_camera() {
    let b = Browser::launch()
        .arg("--use-fake-device-for-media-stream=device-count=2")
        .toml(
            "[content.media]\nvideo_capture = \"true\"\n\
             [per_domain.\"{server}\"]\n\"content.media.camera\" = \"fake_device_1\"\n",
        )
        .start("media-early.html");
    b.wait_eval(
        "String(window.__on = window.__on || (call(), true))",
        "true",
    );
    let label = "String(window.__error || window.__stream?.getVideoTracks()[0]?.label)";
    b.wait_until("the page's camera is the one set for it", |_| {
        b.eval(label) == "fake_device_1"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn media_devices_switches_a_call_in_progress() {
    let b = start_call("");
    wait_in_use(&b, "the defaults", |v| {
        has(v, "sent", "Fake Default Audio Input")
    });

    // Microphone, then the fourth item: "Fake Audio Input 2".
    b.run("media-devices");
    b.wait_until("the kinds", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("Microphone:"))
    });
    b.keys("2");
    b.wait_until("the microphones", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("Fake Audio Input 2"))
    });
    b.keys("4");
    wait_in_use(&b, "the new microphone", |v| {
        has(v, "sent", "Fake Audio Input 2")
            && has(v, "stream", "Fake Audio Input 2")
            && v["live"] == true
    });
    assert!(
        b.wait_file(&b.config_dir().join("autoconfig.toml"))
            .contains("\"content.media.microphone\" = \"Fake Audio Input 2\""),
        "the choice is kept for the site"
    );

    // The site still mutes the track it was given.
    b.eval("__tracks.find((t) => t.kind === 'audio').enabled = false, ''");
    wait_in_use(&b, "muted", |v| {
        v["enabled"]
            .as_array()
            .is_some_and(|e| e.contains(&Value::Bool(false)))
    });

    // Speaker, then "Fake Audio Output 1".
    b.run("media-devices");
    b.wait_until("the kinds", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("Microphone:"))
    });
    b.keys("3");
    b.wait_until("the speakers", |s| {
        s.prompt
            .as_ref()
            .is_some_and(|p| p.to_string().contains("Fake Audio Output 1"))
    });
    b.keys("3");
    wait_in_use(&b, "the new speaker", |v| {
        v["speaker"] == "Fake Audio Output 1"
    });
}
