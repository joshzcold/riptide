//! Hints inside cross-origin iframes and open shadow roots.
#![cfg(unix)]

use rt_e2e::{Browser, Launch, State};

/// Hint mode with the elements `ready` wants; tries again while frames load.
fn hints_once(b: &Browser, ready: impl Fn(&State) -> bool) -> State {
    let start = std::time::Instant::now();
    let mut reloaded = false;
    loop {
        // Under load, CEF can time out setting up the iframe's renderer
        // ("Timeout of new browser info response"), and only a reload fixes it.
        if !reloaded && start.elapsed() > rt_e2e::TIMEOUT / 3 {
            reloaded = true;
            b.run("reload");
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        b.run("hint");
        let s = b.wait_until("hints are shown", |s| {
            s.mode == "hint" && !s.hints.is_empty()
        });
        if ready(&s) {
            return s;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "hints never covered the frame: {:#?}",
            s.hints
        );
        b.keys("<Escape>");
        b.wait_mode("normal");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_reach_into_cross_origin_iframes_but_not_hidden_ones() {
    let b = Browser::start("xframe.html");
    let start = std::time::Instant::now();
    loop {
        let s = hints_once(&b, |s| s.hints.iter().any(|h| h.text == "inner"));
        let texts: Vec<&str> = s.hints.iter().map(|h| h.text.as_str()).collect();
        assert!(texts.contains(&"outer"), "{texts:?}");
        assert_eq!(
            texts.iter().filter(|t| **t == "inner").count(),
            1,
            "the hidden iframe's button got a hint too: {texts:?}"
        );
        let label = s
            .hints
            .iter()
            .find(|h| h.text == "inner")
            .unwrap()
            .label
            .clone();
        b.keys(&label);
        // A real click, at the button's place in the top page. Under load the
        // frame's own renderer can miss the first one, as a page that hasn't
        // painted does.
        let clicked = std::time::Instant::now();
        while clicked.elapsed() < std::time::Duration::from_secs(3) {
            if b.state().tab().title == "inner clicked true" {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT * 2,
            "the iframe's button was never clicked"
        );
        b.wait_mode("normal");
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn hints_reach_into_open_shadow_roots() {
    let b = Browser::start("shadow.html");
    b.follow_hint("hint", |h| h.text == "shadowed");
    b.wait_until("the shadowed button is clicked", |s| {
        s.tab().title == "shadow clicked true"
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn page_eval_reaches_the_focused_frame_and_names_it() {
    let b = Launch::default()
        .file(
            "scratch-plugins/framed/plugin/framed.lua",
            r#"local frame
rt.command("framed", function()
  rt.page.eval("location.hostname + ' ' + document.activeElement.id", function(v, err, id)
    frame = id
    rt.notify("focused " .. tostring(err or v))
  end, { frame = "focused" })
end)
rt.command("framed-again", function()
  rt.page.eval("document.activeElement.value", function(v, err)
    rt.notify("again " .. tostring(err or v))
  end, { frame = frame })
end)
rt.command("framed-all", function()
  rt.page.eval("location.hostname", function(list)
    local hosts = {}
    for _, f in ipairs(list) do table.insert(hosts, tostring(f.value)) end
    rt.notify("all " .. table.concat(hosts, " "))
  end, { frame = "all" })
end)
rt.command("framed-gone", function()
  rt.page.eval("1", function(v, err) rt.notify("gone " .. tostring(err or v)) end, { frame = "nope" })
end)"#,
        )
        .file(
            "scratch-plugins/framed/riptide-plugin.toml",
            "[permissions]\npages = [\"*\"]\n",
        )
        .lua(r#"rt.pack.add({ dir = rt.config_dir .. "/../scratch-plugins/framed" })"#)
        .start("editframe.html");
    b.wait_until("framed asks", |s| s.mode == "yesno");
    b.keys("y");
    b.wait_mode("normal");
    // The frame's field is the page's only hint; following it focuses it.
    let start = std::time::Instant::now();
    loop {
        let s = hints_once(&b, |s| !s.hints.is_empty());
        b.keys(&s.hints[0].label.clone());
        let clicked = std::time::Instant::now();
        while clicked.elapsed() < std::time::Duration::from_secs(3) && b.state().mode != "insert" {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if b.state().mode == "insert" {
            break;
        }
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT * 2,
            "the frame's field never took focus"
        );
        b.keys("<Escape>");
    }
    b.run("framed");
    b.wait_until("the eval ran in the frame", |s| {
        s.message() == Some("focused localhost field")
    });
    b.run("framed-again");
    b.wait_until("the frame's id reached it again", |s| {
        s.message() == Some("again in the frame")
    });
    b.run("framed-all");
    b.wait_until("every frame answered, the main one first", |s| {
        s.message() == Some("all 127.0.0.1 localhost")
    });
    b.run("framed-gone");
    b.wait_until("an unknown frame is refused", |s| {
        s.message() == Some("gone that frame is gone")
    });
}
