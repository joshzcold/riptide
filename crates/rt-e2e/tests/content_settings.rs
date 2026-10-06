//! Content settings that map onto Chromium's: images and popups.
#![cfg(unix)]

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_block_images_per_site() {
    let b = Browser::start("images.html");
    b.wait_until("the image loads", |s| s.tab().title == "image=8");
    let site = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set -u {site} content.images false"));
    b.run("reload");
    b.wait_until("the image is blocked", |s| s.tab().title == "image=0");
    b.run(&format!("set -u {site} content.images true"));
    b.run("reload");
    b.wait_until("the image loads again", |s| s.tab().title == "image=8");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_popups_need_can_open_tabs_automatically() {
    let b = Browser::start("popup.html");
    b.wait_until("the popup is blocked", |s| s.tab().title == "popup=blocked");
    assert_eq!(b.state().tabs().len(), 1);
    b.run("set content.javascript.can_open_tabs_automatically true");
    b.run("reload");
    b.wait_until("the popup opens", |s| s.tabs().len() == 2);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_settings_leave_private_windows_working() {
    let b = Browser::launch()
        .toml(
            "content.images = false\ncontent.mute = true\n\
             content.javascript.can_open_tabs_automatically = true\n\
             content.javascript.clipboard = \"access-paste\"\n",
        )
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -p {second}"));
    b.wait_until("the private window loads", |s| {
        s.window().private && s.tab().is_loaded(&second)
    });
}

/// The request headers the fixture server saw for `/headers`, lowercased.
fn headers_seen(b: &Browser) -> String {
    b.wait_until("the headers page", |s| {
        s.tab().url.ends_with("/headers") && !s.tab().loading
    });
    b.eval("document.body.innerText")
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_headers_language_dnt_and_custom() {
    let b = Browser::launch()
        .toml(
            "\"content.headers.accept_language\" = \"de-DE,de;q=0.9\"\n\
             [\"content.headers.custom\"]\nX-Riptide = \"yes\"\n",
        )
        .start("page.html");
    b.open("headers");
    let seen = headers_seen(&b);
    assert!(seen.contains("accept-language: de-de,de;q=0.9"), "{seen}");
    assert!(seen.contains("x-riptide: yes"), "{seen}");
    assert!(seen.contains("dnt: 1"), "{seen}");
    assert_eq!(b.eval("navigator.languages[0]"), "de-DE");
    b.run("set content.headers.do_not_track false");
    b.run("reload");
    b.wait_until("reloaded without DNT", |s| !s.tab().loading);
    let seen = headers_seen(&b);
    assert!(!seen.contains("dnt:"), "{seen}");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_headers_referer_same_domain_drops_cross_site_referrers() {
    let b = Browser::start("page.html");
    let go = |b: &Browser, url: String| {
        b.eval(&format!("location.href = {url:?}; ''"));
        headers_seen(b)
    };
    let same = b.url("headers");
    let other = same.replace("127.0.0.1", "localhost");
    let seen = go(&b, same.clone());
    assert!(
        seen.contains("referer: http://127.0.0.1"),
        "same host keeps it: {seen}"
    );
    b.open("page.html");
    let seen = go(&b, other.clone());
    assert!(
        !seen.contains("referer:"),
        "another site doesn't get it: {seen}"
    );
    b.run("set content.headers.referer always");
    b.open("page.html");
    let seen = go(&b, other);
    assert!(
        seen.contains("referer: http://127.0.0.1"),
        "always sends it: {seen}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_proxy_sends_requests_through_the_proxy() {
    let b = Browser::start("page.html");
    // The fixture server doubles as an HTTP proxy, so a host that doesn't
    // exist loads only if the request really goes through it.
    let proxy = b.url("").trim_end_matches('/').to_string();
    b.run(&format!("set content.proxy {proxy}"));
    let url = "http://riptide-proxy-test.invalid/second.html";
    b.run(&format!("open {url}"));
    b.wait_until("the page comes through the proxy", |s| {
        s.tab().url == url && s.tab().title == "second"
    });
    b.run("set content.proxy none");
    b.run("reload");
    b.wait_until("without it the host doesn't resolve", |s| {
        s.tab().title != "second" && !s.tab().loading
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_canvas_reading_false_blocks_reading_pixels_back() {
    const READ: &str = "(() => { const c = document.createElement('canvas'); \
        try { c.getContext('2d').getImageData(0, 0, 1, 1); return 'read'; } catch (e) { return 'blocked'; } })()";
    let b = Browser::start("page.html");
    assert_eq!(b.eval(READ), "read");
    let b = Browser::launch()
        .toml("\"content.canvas_reading\" = false\n")
        .start("page.html");
    assert_eq!(b.eval(READ), "blocked");
}

/// Start gathering WebRTC ICE candidates; `window.__candidates` becomes
/// their count once gathering ends.
const GATHER: &str = "(() => { const pc = new RTCPeerConnection(); let n = 0; \
    pc.onicecandidate = (e) => { if (e.candidate) n++; else window.__candidates = String(n); }; \
    pc.createDataChannel('x'); pc.createOffer().then((o) => pc.setLocalDescription(o)); return ''; })()";

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_webrtc_policy_disable_non_proxied_udp_hides_local_candidates() {
    let b = Browser::start("page.html");
    b.eval(GATHER);
    b.wait_until("gathering ends", |_| {
        b.try_eval("window.__candidates ?? ''")
            .is_ok_and(|n| !n.is_empty())
    });
    let open = b.eval("window.__candidates");
    assert_ne!(open, "0", "the default policy finds local candidates");
    b.run("set content.webrtc_ip_handling_policy disable-non-proxied-udp");
    b.run("reload");
    b.wait_until("reloaded", |s| !s.tab().loading);
    b.eval(GATHER);
    b.wait_until("gathering ends", |_| {
        b.try_eval("window.__candidates ?? ''")
            .is_ok_and(|n| !n.is_empty())
    });
    assert_eq!(b.eval("window.__candidates"), "0");
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_pdf_viewer_false_downloads_pdfs() {
    let b = Browser::launch()
        .toml(
            "downloads.location.directory = \"{scratch}/dl\"\ndownloads.location.prompt = false\n\
             \"content.pdf_viewer\" = false\n",
        )
        .start("page.html");
    b.run(&format!("open {}", b.url("blank.pdf")));
    b.wait_file(&b.scratch().join("dl/blank.pdf"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_javascript_can_close_tabs_gates_window_close() {
    let b = Browser::launch()
        .toml("\"content.javascript.can_close_tabs\" = false\nmessages.timeout = 0\n")
        .start("page.html");
    let second = b.url("second.html");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    b.eval("window.close(); ''");
    b.wait_until("the page is told no", |s| {
        s.message().is_some_and(|m| m.contains("can_close_tabs"))
    });
    assert_eq!(b.state().tabs().len(), 2);
    assert_eq!(b.eval("document.title"), "second", "the tab still works");
    b.keys("d");
    b.wait_until("d still closes it", |s| s.tabs().len() == 1);
    b.run("set content.javascript.can_close_tabs true");
    b.run(&format!("open -t {second}"));
    b.wait_until("two tabs", |s| {
        s.tabs().len() == 2 && s.tab().is_loaded(&second)
    });
    b.eval("window.close(); ''");
    b.wait_until("the page closes its tab", |s| s.tabs().len() == 1);
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_javascript_log_message_levels_show_console_messages() {
    let b = Browser::launch()
        .toml("\"content.javascript.log_message.levels\" = [\"error\"]\nmessages.timeout = 0\n")
        .start("page.html");
    b.eval("console.info('quiet'); console.error('boom'); ''");
    let s = b.wait_until("the error shows", |s| {
        s.message().is_some_and(|m| m.starts_with("JS: boom"))
    });
    assert!(!s.message().unwrap().contains("quiet"));
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_webgl_and_reduced_motion_apply_at_startup() {
    const WEBGL: &str = "String(!!document.createElement('canvas').getContext('webgl'))";
    const REDUCED: &str = "String(matchMedia('(prefers-reduced-motion: reduce)').matches)";
    let b = Browser::start("page.html");
    let webgl_by_default = b.eval(WEBGL);
    assert_eq!(b.eval(REDUCED), "false");
    let b = Browser::launch()
        .toml("\"content.webgl\" = false\n\"content.prefers_reduced_motion\" = true\n")
        .start("page.html");
    assert_eq!(b.eval(REDUCED), "true");
    assert_eq!(
        b.eval(WEBGL),
        "false",
        "WebGL by default: {webgl_by_default}"
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn scrolling_bar_never_hides_page_scrollbars() {
    // Width taken by the page's vertical scrollbar, once the page is taller than the window.
    const BAR: &str = "(() => { document.body.style.height = '5000px'; \
        return String(innerWidth - document.documentElement.clientWidth); })()";
    let b = Browser::start("page.html");
    assert_ne!(b.eval(BAR), "0", "a scrollbar by default");
    b.run("set scrolling.bar never");
    b.run("reload");
    b.wait_until("reloaded", |s| !s.tab().loading);
    let start = std::time::Instant::now();
    while b.eval(BAR) != "0" {
        assert!(
            start.elapsed() < rt_e2e::TIMEOUT,
            "the scrollbar is still there"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
