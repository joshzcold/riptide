//! `rt.on`: config.lua hears the browser's events, with patterns, groups
//! and once.
#![cfg(unix)]

use rt_e2e::Browser;

/// Each event appends a line to `{scratch}/events`.
const CONFIG: &str = r#"
local log = "{scratch}/events"
local function note(text)
  local f = io.open(log, "a")
  f:write(text .. "\n")
  f:close()
end
local g = rt.group("test", { clear = true })
for _, event in ipairs({ "startup", "tab_opened", "tab_closed", "window_opened", "window_closed" }) do
  rt.on(event, { group = g }, function() note(event) end)
end
rt.on("tab_selected", { group = g }, function(e) note("tab_selected " .. e.index) end)
rt.on("title_changed", { pattern = "*://127.0.0.1/*second.html" }, function(e) note("title " .. e.title) end)
rt.on("load_started", { once = true }, function() note("first load_started") end)
rt.on("setting_changed", function(e) note("setting " .. e.name .. "=" .. e.value) end)
"#;

fn events(b: &Browser) -> String {
    std::fs::read_to_string(b.scratch().join("events")).unwrap_or_default()
}

fn wait_for_line(b: &Browser, line: &str) {
    let start = std::time::Instant::now();
    while !events(b).lines().any(|l| l == line) {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "no {line:?} in:\n{}",
            events(b)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn config_lua_hears_events_with_patterns_groups_and_once() {
    let b = Browser::launch().lua(CONFIG).start("page.html");
    wait_for_line(&b, "startup");
    wait_for_line(&b, "window_opened");
    wait_for_line(&b, "first load_started");

    b.run(&format!("open -t {}", b.url("second.html")));
    wait_for_line(&b, "tab_opened");
    wait_for_line(&b, "title second");
    wait_for_line(&b, "tab_selected 2");
    b.run("tab-prev");
    wait_for_line(&b, "tab_selected 1");
    b.run("tab-close");
    wait_for_line(&b, "tab_closed");

    b.run("set zoom.default 125");
    wait_for_line(&b, "setting zoom.default=125");

    b.run("open -w about:blank");
    b.wait_until("a second window", |s| s.windows.len() == 2);
    b.run("close");
    wait_for_line(&b, "window_closed");

    let log = events(&b);
    // `once`: only the first load.
    assert_eq!(log.matches("first load_started").count(), 1, "{log}");
    // The pattern kept page.html's title out.
    assert!(!log.contains("title ready"), "{log}");
}
