//! Input settings: forgetting half-typed keys, and a mode per site.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_partial_timeout_forgets_a_half_typed_chain() {
    let b = Browser::launch()
        .toml("\"input.partial_timeout\" = 300\n")
        .start("page.html");
    b.keys("g");
    b.wait_until("g is waiting", |s| s.status["keystring"] == "g");
    b.wait_until("it's forgotten", |s| s.status["keystring"] == "");
    assert_eq!(b.state().mode, "normal");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn input_mode_override_enters_the_sites_mode_on_load() {
    let b = Browser::start("page.html");
    let site = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set -u {site} input.mode_override passthrough"));
    b.open("second.html");
    b.wait_mode("passthrough");
}
