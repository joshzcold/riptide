//! Per-site settings and the certificate prompt.
#![cfg(unix)]

use std::process::{Child, Command, Stdio};

use rt_e2e::Browser;

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn per_site_user_agent_and_javascript() {
    let b = Browser::start("ua.html");
    let site = b.url("").trim_end_matches('/').to_string();
    b.wait_until("the page ran", |s| s.tab().title.starts_with("ua=Mozilla"));
    b.run(&format!(
        "set -u {site} content.headers.user_agent RiptideTest/1"
    ));
    b.run("reload");
    b.wait_until("the site's user agent", |s| {
        s.tab().title == "ua=RiptideTest/1"
    });
    b.run(&format!("set -u {site} content.javascript.enabled false"));
    b.run("reload");
    b.wait_until("scripts don't run", |s| {
        s.tab().title == "static" && !s.tab().loading
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn content_cookies_accept_never_refuses_cookies() {
    let b = Browser::start("cookie.html");
    b.wait_until("the cookie is set", |s| s.tab().title == "cookie=a=1");
    b.run("set content.cookies.accept never");
    b.run("reload");
    b.wait_until("no cookie", |s| {
        s.tab().title == "cookie=" && !s.tab().loading
    });
}

/// An HTTPS server with a self-signed certificate, stopped when dropped.
struct TlsServer {
    child: Child,
    port: u16,
}

impl TlsServer {
    fn start(dir: &std::path::Path) -> Self {
        std::fs::write(
            dir.join("index.html"),
            "<!doctype html><title>secret page</title>",
        )
        .unwrap();
        let made = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "2",
                "-subj",
                "/CN=localhost",
            ])
            .arg("-keyout")
            .arg(dir.join("key.pem"))
            .arg("-out")
            .arg(dir.join("cert.pem"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("openssl is needed for the certificate test");
        assert!(made.success());
        let port_file = dir.join("port");
        let child = Command::new("python3")
            .args([
                "-c",
                "import functools, http.server, ssl, sys\n\
                 d = sys.argv[1]\n\
                 h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=d)\n\
                 s = http.server.ThreadingHTTPServer(('127.0.0.1', 0), h)\n\
                 c = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)\n\
                 c.load_cert_chain(d + '/cert.pem', d + '/key.pem')\n\
                 s.socket = c.wrap_socket(s.socket, server_side=True)\n\
                 open(d + '/port', 'w').write(str(s.server_port))\n\
                 s.serve_forever()\n",
            ])
            .arg(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("python3 is needed for the certificate test");
        let start = std::time::Instant::now();
        let port = loop {
            if let Ok(text) = std::fs::read_to_string(&port_file)
                && let Ok(port) = text.trim().parse()
            {
                break port;
            }
            assert!(
                start.elapsed() < rt_e2e::TIMEOUT,
                "the HTTPS server didn't start"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        Self { child, port }
    }
}

impl Drop for TlsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn an_untrusted_certificate_asks_before_loading() {
    let b = Browser::start("page.html");
    let server = TlsServer::start(&b.scratch());
    b.run(&format!("open https://127.0.0.1:{}/", server.port));
    let s = b.wait_mode("yesno");
    assert!(s.prompt.is_some());
    b.keys("y");
    b.wait_until("the page loads after y", |s| s.tab().title == "secret page");
}
