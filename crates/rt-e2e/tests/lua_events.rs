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
rt.on("startup", function() rt.defer(200, function() note("deferred") end) end)
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
    wait_for_line(&b, "deferred");

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

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn keymap_descriptions_show_in_key_hints_and_commands_complete_their_arguments() {
    let b = Browser::launch()
        .lua(
            r#"
rt.keymap.set("normal", "<Space>h", function() rt.notify("hi") end, { desc = "Say hi" })
rt.command("greet", function(args) rt.notify("hello " .. args) end, {
  desc = "Greet someone",
  complete = function() return { "alice", { name = "bob", desc = "a friend" } } end,
})
"#,
        )
        .start("page.html");
    b.keys("<Space>");
    let start = std::time::Instant::now();
    while !b
        .eval_bar("completion", "document.body.innerText")
        .contains("Say hi")
    {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "no description in the key hints"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.keys("h");
    b.wait_until("the function ran", |s| s.message() == Some("hi"));

    b.keys(":greet ");
    let s = b.wait_until("the arguments complete", |s| {
        s.completion["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["name"] == "bob"))
    });
    let names: Vec<&str> = s.completion["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["name"].as_str())
        .collect();
    assert_eq!(names, ["alice", "bob"]);
    b.keys("<Tab><Return>");
    b.wait_until("the command ran with the choice", |s| {
        s.message() == Some("hello alice")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn statusbar_widgets_from_lua_follow_the_page() {
    let b = Browser::launch()
        .lua(
            r#"
c.statusbar.widgets = { "lua:page", "tabs" }
rt.statusbar.widget("page", function() return "on " .. rt.url():match("[^/]*$") end)
"#,
        )
        .start("page.html");
    let text = "document.getElementById('right').innerText";
    let start = std::time::Instant::now();
    while !b.eval_bar("statusbar", text).contains("on page.html") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{}",
            b.eval_bar("statusbar", text)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    b.open("second.html");
    while !b.eval_bar("statusbar", text).contains("on second.html") {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "{}",
            b.eval_bar("statusbar", text)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn floats_show_text_take_their_keys_and_close() {
    let b = Browser::launch()
        .lua(
            r#"
rt.command("show-float", function()
  local f = rt.ui.float({
    title = "Hello",
    lines = { "first line", { { "second ", "muted" }, { "line", "accent" } } },
    keys = {
      u = function(self) self:update({ lines = { "updated" } }) end,
      x = function(self) rt.notify("x pressed"); self:close() end,
    },
    on_close = function() rt.notify("closed") end,
  })
end)
"#,
        )
        .start("page.html");
    b.run("show-float");
    let s = b.wait_until("the float is placed", |s| {
        s.floats.first().is_some_and(|f| f["placed"] == true)
    });
    assert_eq!(s.floats[0]["title"], "Hello");
    assert_eq!(s.floats[0]["text"], "first line\nsecond line");
    assert_eq!(s.floats[0]["source"], "");

    b.keys("u");
    b.wait_until("u updated it", |s| {
        s.floats.first().is_some_and(|f| f["text"] == "updated")
    });
    b.keys("x");
    b.wait_until("x ran and closed it", |s| {
        s.floats.is_empty() && s.message() == Some("x pressed")
    });

    // Escape closes one and runs its on_close.
    b.run("show-float");
    b.wait_until("shown again", |s| s.floats.len() == 1);
    b.keys("<Escape>");
    b.wait_until("Escape closed it", |s| {
        s.floats.is_empty() && s.message() == Some("closed")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn panels_dock_beside_the_page_and_take_keys_when_focused() {
    let b = Browser::launch()
        .lua(
            r#"
rt.command("show-panel", function()
  rt.ui.panel({
    title = "List",
    size = 250,
    lines = { "one", "two", "three" },
    keys = { ["<Return>"] = function(p, line) rt.notify("picked " .. line) end },
  })
end)
"#,
        )
        .start("page.html");
    b.run("show-panel");
    let s = b.wait_until("the panel is laid out", |s| {
        s.panels.first().is_some_and(|p| p["width"] == 250)
    });
    assert_eq!(s.panels[0]["side"], "left");
    assert_eq!(s.panels[0]["focused"], false);

    // Unfocused, j belongs to the page.
    b.keys("j");
    b.run("panel-focus");
    b.wait_until("focused", |s| s.panels[0]["focused"] == true);
    b.keys("j");
    b.wait_until("the cursor moved once", |s| s.panels[0]["cursor"] == 2);
    b.keys("<Return>");
    b.wait_until("Return ran on the cursor's line", |s| {
        s.message() == Some("picked 2")
    });
    b.keys("<Escape>");
    b.wait_until("Escape went back to the page", |s| {
        s.panels[0]["focused"] == false
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn page_eval_css_and_selection_answer_lua() {
    let b = Browser::launch()
        .lua(
            r#"
local function show(prefix) return function(v, err) rt.notify(prefix .. " " .. tostring(err or rt.json.encode(v))) end end
rt.command("t-eval", function() rt.page.eval("({ title: document.title, n: 1 + 1 })", show("eval")) end)
rt.command("t-bad", function() rt.page.eval("nope(", show("bad")) end)
rt.command("t-css", function() rt.page.css("body { color: rgb(1, 2, 3) }") end)
rt.command("t-color", function() rt.page.eval("getComputedStyle(document.body).color", show("color")) end)
rt.command("t-select", function()
  rt.page.eval("(getSelection().selectAllChildren(document.querySelector('h1') || document.body), 0)", function()
    rt.page.selection(show("selection"))
  end)
end)
"#,
        )
        .start("page.html");
    b.run("t-eval");
    b.wait_until("eval answered", |s| {
        s.message() == Some(r#"eval {"n":2,"title":"ready"}"#)
    });
    b.run("t-bad");
    b.wait_until("a syntax error comes back as an error", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("bad ") && !m.contains("null"))
    });
    b.run("t-css");
    b.run("t-color");
    b.wait_until("the stylesheet applied", |s| {
        s.message() == Some(r#"color "rgb(1, 2, 3)""#)
    });
    b.run("t-select");
    b.wait_until("the selection came back", |s| {
        s.message()
            .is_some_and(|m| m.starts_with("selection \"") && m.len() > 12)
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn page_hints_hand_the_picked_element_to_lua() {
    let b = Browser::launch()
        .lua(
            r##"
rt.command("t-hint", function()
  rt.page.hint({ selector = "#f, #b", action = function(el, err)
    rt.notify("picked " .. tostring(err or el.text) .. " " .. tostring(el and el.url))
  end })
end)
"##,
        )
        .start("page.html");
    b.run("t-hint");
    let s = b.wait_until("only the selector's elements are hinted", |s| {
        s.mode == "hint" && s.hints.len() == 2
    });
    let button = s
        .hints
        .iter()
        .find(|h| h.text == "b")
        .expect("the button is hinted");
    b.keys(&button.label);
    b.wait_until("the action got the element", |s| {
        s.message() == Some("picked b nil")
    });
    // The page itself wasn't clicked.
    assert_eq!(b.state().tab().title, "ready");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn commands_added_after_loading_can_run() {
    let b = Browser::launch()
        .lua(
            r#"
-- Added and run in one callback, as a plugin's setup() can.
rt.keymap.set("normal", "zc", function()
  rt.command("late-cmd", function() rt.notify("late ran") end)
  rt.run("late-cmd")
end)
"#,
        )
        .start("page.html");
    b.keys("zc");
    b.wait_until("the new command ran", |s| s.message() == Some("late ran"));
}

/// The events plugins asked for: errors, moves, commands, insert mode,
/// themes, questions and permission answers.
const MORE_EVENTS: &str = r#"
local log = "{scratch}/events"
local function note(text)
  local f = io.open(log, "a")
  f:write(text .. "\n")
  f:close()
end
rt.on("page_error", function(e) note("page_error " .. e.error) end)
rt.on("tab_moved", function(e) note("tab_moved " .. e.from .. " " .. e.to) end)
rt.on("command", function(e)
  if e.name == "message-info" then note("command " .. e.name .. " " .. e.args) end
end)
rt.on("insert_entered", function() note("insert_entered") end)
rt.on("theme_changed", function(e) note("theme_changed " .. e.theme) end)
rt.on("prompt_shown", function(e) note("prompt_shown " .. e.topic) end)
rt.on("permission_answered", function(e) note("permission_answered " .. e.answer .. " " .. e.remembered) end)
"#;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn config_lua_hears_errors_moves_commands_themes_and_questions() {
    let b = Browser::launch().lua(MORE_EVENTS).start("geo.html");
    b.run("message-info hello there");
    wait_for_line(&b, "command message-info hello there");

    b.run("set ui.theme nord");
    wait_for_line(&b, "theme_changed nord");

    b.follow_hint("hint", |h| h.text == "ask");
    b.wait_mode("yesno");
    wait_for_line(&b, "prompt_shown permission");
    b.keys("n");
    wait_for_line(&b, "permission_answered not_now false");

    b.run("set input.insert_mode.auto_load true");
    b.eval("const i = document.createElement('input'); document.body.prepend(i); i.focus(), ''");
    wait_for_line(&b, "insert_entered");
    b.keys("<Escape>");

    b.run(&format!("open -t {}", b.url("page.html")));
    b.wait_until("two tabs", |s| s.tabs().len() == 2);
    b.run("tab-move 1");
    wait_for_line(&b, "tab_moved 2 1");

    // Chromium refuses port 1 without trying it.
    b.run("open http://127.0.0.1:1/");
    wait_for_line(&b, "page_error ERR_UNSAFE_PORT");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn command_before_hooks_cancel_commands() {
    let b = Browser::launch()
        .lua(
            r#"
rt.on("command_before", function(e)
  if e.name == "open" and e.args:find("second") then
    rt.notify("not in " .. rt.mode() .. " mode")
    return true
  end
end)
"#,
        )
        .start("page.html");
    let page = b.url("page.html");
    b.run(&format!("open {}", b.url("second.html")));
    b.wait_until("the hook cancelled it", |s| {
        s.message() == Some("not in normal mode")
    });
    assert!(b.state().tab().is_loaded(&page));
    // Other commands still run.
    let nav1 = b.url("nav1.html");
    b.run(&format!("open {nav1}"));
    b.wait_until("nav1 loads", |s| s.tab().is_loaded(&nav1));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn key_hooks_hear_unbound_keys_and_can_keep_them_from_the_page() {
    let b = Browser::launch()
        .lua(
            r#"
local heard = 0
rt.on("key", function(e)
  heard = heard + 1
  rt.message("unbound " .. e.keys .. " " .. heard)
  return e.keys == "x"
end)
"#,
        )
        // Letters reach the page only with this.
        .toml("messages.timeout = 0\ninput.forward_unbound_keys = \"all\"\n")
        .start("page.html");
    b.eval(
        "window.typed = ''; addEventListener('keydown', (e) => { typed += e.key; document.title = 'typed=' + typed }); 'ok'",
    );
    // Chromium drops the first key a page gets after it loads.
    let start = std::time::Instant::now();
    while !b.eval("document.title").starts_with("typed=") {
        assert!(start.elapsed() < rt_e2e::TIMEOUT, "no key reached the page");
        b.keys("e");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    b.eval("typed = ''; document.title = 'typed='; 'ok'");
    b.keys("e");
    b.wait_eval("document.title", "typed=e");
    let before = b.wait_until("the hook heard it", |s| {
        s.message().is_some_and(|m| m.starts_with("unbound e "))
    });
    let first: usize = before
        .message()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    b.keys("x");
    b.wait_until("the hook heard x", |s| {
        s.message()
            .is_some_and(|m| m == format!("unbound x {}", first + 1))
    });
    // A bound key (j scrolls) isn't one of them.
    b.keys("j");
    b.keys("e");
    b.wait_eval("document.title", "typed=ee");
    b.wait_until("the hook heard e, but not j", |s| {
        s.message()
            .is_some_and(|m| m == format!("unbound e {}", first + 2))
    });
}
