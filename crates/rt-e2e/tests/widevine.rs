//! `content.widevine`: the CDM is deleted when it's off, and updated every
//! `content.widevine_update_interval` days when it's on.
#![cfg(unix)]

use rt_e2e::Browser;

const CDM: &str = "data/WidevineCdm/4.10.2830.0/manifest.json";

/// Whether Chromium's `Local State` has component updates on.
fn component_updates(b: &Browser) -> bool {
    let text = std::fs::read_to_string(b.data_dir().join("Local State")).unwrap();
    let state: serde_json::Value = serde_json::from_str(&text).unwrap();
    state["component_updates"]["component_updates_enabled"] == true
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn turning_widevine_off_deletes_the_cdm() {
    let b = Browser::launch().file(CDM, "{}").start("page.html");
    // Chromium makes the folder again, empty.
    assert!(!b.data_dir().join("WidevineCdm/4.10.2830.0").exists());
    b.wait_until("it says so", |s| {
        s.message()
            .is_some_and(|m| m.contains("Removed the Widevine CDM"))
    });
    assert!(!component_updates(&b));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_widevine_update_turns_component_updates_on_for_one_run() {
    let b = Browser::launch()
        .toml("content.widevine = true\n")
        .file(CDM, "{}")
        // Long ago, so a check is due.
        .file("data/widevine-last-update", "0")
        // Nowhere, so the test doesn't download anything from Google.
        .arg("--component-updater=url-source=http://127.0.0.1:9/")
        .start("page.html");
    assert!(component_updates(&b), "the update run");
    let stamp = std::fs::read_to_string(b.data_dir().join("widevine-last-update")).unwrap();
    assert_ne!(stamp.trim(), "0", "the check is recorded");
    b.run("quit");
    assert!(b.wait_exit().success());
    // Chromium drops this stand-in, which has no library, so put it back.
    let cdm = b.data_dir().join(CDM.trim_start_matches("data/"));
    std::fs::create_dir_all(cdm.parent().unwrap()).unwrap();
    std::fs::write(&cdm, "{}").unwrap();
    b.restart_with(&["--component-updater=url-source=http://127.0.0.1:9/"]);
    assert!(
        !component_updates(&b),
        "not again until the interval passes"
    );
}
