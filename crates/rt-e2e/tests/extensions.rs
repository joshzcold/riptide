//! Chrome extensions: loaded from `extensions.load` folders, and installed
//! from a CRX file with `:extension-install`.
#![cfg(unix)]

use std::io::Write;

use rt_e2e::Browser;
use sha2::{Digest, Sha256};

/// A throwaway RSA public key (DER); its private half was never kept.
const KEY: &[u8] = include_bytes!("../pages/extensions/test-key.der");

fn id_of(key: &[u8]) -> String {
    Sha256::digest(key)[..16]
        .iter()
        .flat_map(|b| [b >> 4, b & 0xf])
        .map(|n| char::from(b'a' + n))
        .collect()
}

fn field(number: u64, value: &[u8]) -> Vec<u8> {
    fn varint(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                return out.push(byte);
            }
            out.push(byte | 0x80);
        }
    }
    let mut out = Vec::new();
    varint((number << 3) | 2, &mut out);
    varint(value.len() as u64, &mut out);
    out.extend_from_slice(value);
    out
}

/// A CRX3 file holding the files given, signed (in name only) by [`KEY`].
fn crx(files: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    let zip = zip.finish().unwrap().into_inner();
    let proof = [field(1, KEY), field(2, b"signature")].concat();
    let crx_id = &Sha256::digest(KEY)[..16];
    let header = [field(2, &proof), field(10000, &field(1, crx_id))].concat();
    let mut out = b"Cr24".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend(u32::try_from(header.len()).unwrap().to_le_bytes());
    out.extend(header);
    out.extend(zip);
    out
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn extensions_from_folders_run_in_tabs_and_block() {
    // The start page loads although Chromium is still reading the rules.
    let b = Browser::launch()
        .toml("extensions.load = [\"{pages}/extensions/probe\"]\n")
        .start("page.html");
    b.wait_eval("document.documentElement.dataset.probe || ''", "content");
    b.wait_eval("document.documentElement.dataset.probeWorker || ''", "pong");
    b.eval(
        "window.__f = ''; fetch('/blocked-by-probe').then(r => __f = 'loaded ' + r.status, () => __f = 'blocked'); ''",
    );
    b.wait_eval("String(window.__f)", "blocked");

    // Private windows run without them.
    let page = b.url("page.html");
    b.run(&format!("open -p {page}"));
    b.wait_until("the private window loads", |s| {
        s.windows.len() == 2 && s.window().private && s.tab().is_loaded(&page)
    });
    b.wait_painted();
    assert_eq!(
        b.eval("document.documentElement.dataset.probe || 'none'"),
        "none"
    );
    b.run("close");
    b.wait_until("back to one window", |s| s.windows.len() == 1);

    // :extensions lists it, with buttons for its popup and options.
    b.run("extensions");
    b.wait_until("the Extensions tab opens", |s| {
        s.tab().url.ends_with("#extensions") && !s.tab().loading
    });
    b.wait_eval(
        "page.extensions.items.map((x) => `${x.name}|${x.state}|${x.popup}|${x.options}|${x.installed}`).join()",
        "riptide probe|loaded|true|true|false",
    );
    // What works differently from Chrome is on the tab, open the first time.
    b.wait_eval(
        "`${document.querySelector('details.limits').open}|${document.querySelectorAll('.limits li').length}`",
        "true|6",
    );
    let id = b.eval("page.extensions.items[0].id");
    b.eval(&format!(
        "rt.send('extension', JSON.stringify({{ id: '{id}', action: 'popup' }})); ''"
    ));
    let popup = format!("chrome-extension://{id}/popup.html");
    b.wait_until("the popup opens in a tab", |s| s.tab().is_loaded(&popup));

    // :extension-open completes names and opens the options page.
    b.keys(":extension-open rip<Tab>");
    b.wait_until("names complete", |s| {
        s.completion["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["name"] == "riptide probe"))
    });
    b.keys("<Escape>");
    b.run("extension-open riptide probe options");
    let options = format!("chrome-extension://{id}/options.html");
    b.wait_until("the options open", |s| s.tab().is_loaded(&options));

    // A folder's extension isn't deleted; the message points to the setting.
    b.run("extension-remove riptide probe");
    b.wait_until("it points to the setting", |s| {
        s.message()
            .is_some_and(|m| m.contains("take it out of extensions.load"))
    });

    // The tab's button opens Chrome's own page in a call window, and its
    // link to the guide's limits opens in a tab.
    b.run("extensions");
    b.wait_until("the Extensions tab opens", |s| {
        s.tab().url.ends_with("#extensions") && !s.tab().loading
    });
    b.follow_hint("hint", |h| h.text.contains("all the limits"));
    b.wait_until("the guide opens in a tab", |s| {
        s.tabs()
            .iter()
            .any(|t| t.url.ends_with("/guide/extensions.html#limits"))
    });
    b.run("extensions");
    b.wait_until("the Extensions tab opens", |s| {
        s.tab().url.ends_with("#extensions") && !s.tab().loading
    });
    b.eval("rt.send('extension', JSON.stringify({ action: 'chrome' })); ''");
    b.wait_until("Chrome's extensions page opens in a call window", |s| {
        s.windows.len() == 2 && s.window().call && s.tab().url.starts_with("chrome://extensions")
    });
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn extension_install_asks_first_and_loads_after_restart() {
    let b = Browser::launch()
        .toml("messages.timeout = 0\n")
        .start("page.html");
    let manifest = r#"{
  "manifest_version": 3,
  "name": "__MSG_name__",
  "default_locale": "en",
  "version": "2.0",
  "content_scripts": [{ "matches": ["<all_urls>"], "js": ["content.js"], "run_at": "document_start" }],
  "permissions": ["nativeMessaging"],
  "action": { "default_popup": "popup.html" }
}"#;
    let file = b.scratch().join("packed.crx");
    std::fs::write(
        &file,
        crx(&[
            ("manifest.json", manifest),
            (
                "_locales/en/messages.json",
                r#"{ "name": { "message": "Packed probe" } }"#,
            ),
            (
                "content.js",
                "document.documentElement.dataset.packed = 'yes';",
            ),
        ]),
    )
    .unwrap();

    b.run(&format!("extension-install {}", file.display()));
    let s = b.wait_until("it asks first", |s| s.mode == "yesno");
    let message = s.prompt.as_ref().unwrap()["message"].to_string();
    for part in [
        "Packed probe 2.0",
        "read and change everything on every site you visit",
        "talk to programs on your computer",
        "Blocking and anything it does inside pages work. Its popup opens as a tab",
    ] {
        assert!(message.contains(part), "{part:?} in {message}");
    }
    b.keys("y");
    let s = b.wait_until("it offers to restart", |s| {
        s.prompt.as_ref().is_some_and(|p| {
            p["message"]
                .to_string()
                .contains("Installed Packed probe. A restart loads it. Restart now?")
        })
    });
    assert_eq!(s.prompt.unwrap()["title"], "Restart");
    b.keys("n");
    b.wait_until("installed", |s| {
        s.message() == Some("Installed Packed probe; :restart loads it")
    });
    let dir = b.data_dir().join("extensions").join(id_of(KEY));
    let installed = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(
        installed.contains("\"key\""),
        "the store's key keeps its id: {installed}"
    );

    b.run("quit");
    b.wait_exit();
    let page = b.url("page.html");
    b.restart_with(&[&page]);
    b.wait_until("the page loads", |s| s.tab().is_loaded(&page));
    b.wait_eval("document.documentElement.dataset.packed || ''", "yes");

    // The same version again changes nothing; a newer one says what's new.
    b.run(&format!("extension-install {}", file.display()));
    b.wait_until("already installed", |s| {
        s.message() == Some("Packed probe 2.0 is already installed")
    });
    let newer = b.scratch().join("newer.crx");
    std::fs::write(
        &newer,
        crx(&[
            (
                "manifest.json",
                &manifest.replace("\"2.0\"", "\"3.0\"").replace(
                    "[\"nativeMessaging\"]",
                    "[\"nativeMessaging\", \"history\"]",
                ),
            ),
            (
                "_locales/en/messages.json",
                r#"{ "name": { "message": "Packed probe" } }"#,
            ),
        ]),
    )
    .unwrap();
    b.run(&format!("extension-install {}", newer.display()));
    let s = b.wait_until("it asks about the update", |s| s.mode == "yesno");
    let message = s.prompt.as_ref().unwrap()["message"].to_string();
    assert!(
        message.contains("Update Packed probe 2.0 to 3.0?")
            && message.contains("It now also asks to:")
            && message.contains("read and change your history")
            && !message.contains("talk to programs"),
        "only what's new: {message}"
    );
    b.keys("n");
    b.wait_mode("normal");

    b.run("extension-remove packed probe");
    b.wait_until("it offers to restart", |s| s.mode == "yesno");
    b.keys("n");
    b.wait_until("removed", |s| {
        s.message() == Some("Removed Packed probe; it's gone after :restart")
    });
    assert!(!dir.exists());
    b.run("extensions");
    b.wait_until("the Extensions tab opens", |s| {
        s.tab().url.ends_with("#extensions") && !s.tab().loading
    });
    b.wait_eval(
        "`${page.extensions.removed.join()}|${page.extensions.restart}|${page.extensions.items.length}`",
        "Packed probe|true|0",
    );
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn manifest_v2_extensions_are_refused() {
    let b = Browser::launch()
        .toml("messages.timeout = 0\n")
        .start("page.html");
    let file = b.scratch().join("old.crx");
    std::fs::write(
        &file,
        crx(&[(
            "manifest.json",
            r#"{ "manifest_version": 2, "name": "Old", "version": "1" }"#,
        )]),
    )
    .unwrap();
    b.run(&format!("extension-install {}", file.display()));
    b.wait_until("refused", |s| {
        s.message().is_some_and(|m| {
            m.contains("Old is a Manifest V2 extension, which Chromium no longer runs")
        })
    });
    assert!(b.state().prompt.is_none());
}

#[test]
#[ignore = "starts a browser; run with ./task e2e"]
fn a_downloaded_extension_installs_instead_of_saving() {
    let b = Browser::launch()
        .toml("messages.timeout = 0\n")
        .start("page.html");
    // Alone, :extension-install wants a Web Store page.
    b.run("extension-install");
    b.wait_until("it says what to do", |s| {
        s.message()
            .is_some_and(|m| m.contains("Open an extension's page on the Chrome Web Store first"))
    });

    // Named as the Web Store names its downloads.
    let name = format!("{}_1_0.crx", id_of(KEY).to_uppercase());
    let file = b.scratch().join(&name);
    std::fs::write(
        &file,
        crx(&[(
            "manifest.json",
            r#"{ "manifest_version": 3, "name": "Downloaded probe", "version": "1.0" }"#,
        )]),
    )
    .unwrap();
    b.run(&format!("open file://{}", file.display()));
    let s = b.wait_until("it asks to install, not where to save", |s| {
        s.prompt.is_some()
    });
    let prompt = s.prompt.unwrap();
    assert_eq!(prompt["title"], "Install extension", "{prompt}");
    assert!(
        prompt["message"]
            .to_string()
            .contains("Downloaded probe 1.0")
    );
    b.keys("y");
    b.wait_until("it offers to restart", |s| {
        s.prompt.as_ref().is_some_and(|p| p["title"] == "Restart")
    });
    b.keys("n");
    b.wait_until("installed", |s| {
        s.message() == Some("Installed Downloaded probe; :restart loads it")
    });
    assert!(b.data_dir().join("extensions").join(id_of(KEY)).is_dir());
    let leftovers = std::fs::read_dir(b.data_dir().join("extensions/.downloads"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0, "the download is deleted once it's read");
}
