//! Chrome extensions: reading a Web Store CRX file, its extension id, and
//! what its manifest asks for, in words for the install question.
//!
//! A CRX3 file is `Cr24`, version 3, a protobuf header with the publisher's
//! public keys and the signed extension id, then a zip. The extension id is
//! the first 16 bytes of the key's SHA-256, written with the letters a-p.
//! Signatures aren't checked here: the file comes over HTTPS from Google, and
//! the key must give the id that was asked for.

use sha2::{Digest, Sha256};

/// What `:extension-install` downloads, from the Web Store's update service.
pub fn store_url(id: &str, chrome_major: i32) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={chrome_major}.0\
         &acceptformat=crx3&x=id%3D{id}%26uc"
    )
}

/// An extension id from what was typed: the id itself, or a Web Store page
/// (`https://chromewebstore.google.com/detail/<name>/<id>`).
pub fn parse_id(arg: &str) -> Option<String> {
    let is_id = |s: &str| s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b));
    let arg = arg.trim();
    if is_id(arg) {
        return Some(arg.to_string());
    }
    let path = arg.split(['?', '#']).next()?;
    path.split('/')
        .rev()
        .find(|part| is_id(part))
        .map(str::to_string)
}

/// The extension id of a public key.
pub fn id_from_key(key: &[u8]) -> String {
    id_from_bytes(&Sha256::digest(key)[..16])
}

fn id_from_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [b >> 4, b & 0xf])
        .map(|n| char::from(b'a' + n))
        .collect()
}

/// A CRX file's contents.
#[derive(Debug, PartialEq, Eq)]
pub struct Crx {
    pub id: String,
    /// The DER public key the id comes from; manifest.json's `key` keeps
    /// the id when the extension is loaded from a folder.
    pub public_key: Vec<u8>,
    pub zip: Vec<u8>,
}

/// Read a CRX3 file, checking its key gives its signed id, and `expected`
/// when given.
pub fn parse_crx(bytes: &[u8], expected: Option<&str>) -> Result<Crx, String> {
    if bytes.get(..4) != Some(b"Cr24") {
        return Err("not an extension (CRX) file".into());
    }
    let word = |at: usize| -> Result<usize, String> {
        let b = bytes.get(at..at + 4).ok_or("the CRX file is cut short")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    match word(4)? {
        3 => {}
        2 => return Err("an old CRX2 file, which Chromium no longer installs".into()),
        v => return Err(format!("CRX version {v} isn't supported")),
    }
    let size = word(8)?;
    let header = bytes
        .get(12..12 + size)
        .ok_or("the CRX header is cut short")?;
    let zip = bytes[12 + size..].to_vec();

    let mut keys = Vec::new();
    let mut signed_id = None;
    for (field, value) in fields(header)? {
        match field {
            // sha256_with_rsa, sha256_with_ecdsa: { 1: public_key, 2: signature }
            2 | 3 => {
                for (f, v) in fields(value)? {
                    if f == 1 {
                        keys.push(v.to_vec());
                    }
                }
            }
            // signed_header_data: SignedData { 1: crx_id }
            10000 => {
                for (f, v) in fields(value)? {
                    if f == 1 {
                        signed_id = Some(id_from_bytes(v));
                    }
                }
            }
            _ => {}
        }
    }
    let signed_id = signed_id.ok_or("the CRX file names no extension id")?;
    if let Some(expected) = expected
        && expected != signed_id
    {
        return Err(format!(
            "the file is for extension {signed_id}, not {expected}"
        ));
    }
    let public_key = keys
        .into_iter()
        .find(|k| id_from_key(k) == signed_id)
        .ok_or("no key in the CRX file matches its extension id")?;
    Ok(Crx {
        id: signed_id,
        public_key,
        zip,
    })
}

/// The length-delimited fields of a protobuf message, as (field, bytes).
/// Varint fields are skipped; others aren't expected in a CRX header.
fn fields(mut data: &[u8]) -> Result<Vec<(u64, &[u8])>, String> {
    fn varint(data: &mut &[u8]) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = data.split_first().ok_or("the CRX header is cut short")?;
            *data = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("a malformed CRX header".into())
    }
    let mut out = Vec::new();
    while !data.is_empty() {
        let tag = varint(&mut data)?;
        match tag & 7 {
            0 => {
                varint(&mut data)?;
            }
            2 => {
                let len = usize::try_from(varint(&mut data)?).map_err(|e| e.to_string())?;
                if len > data.len() {
                    return Err("the CRX header is cut short".into());
                }
                let (value, rest) = data.split_at(len);
                out.push((tag >> 3, value));
                data = rest;
            }
            _ => return Err("a malformed CRX header".into()),
        }
    }
    Ok(out)
}

/// Standard base64, for manifest.json's `key`.
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// What an extension's manifest says about it, for the install question.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub manifest_version: i64,
    /// What it asks for, in words, most powerful first.
    pub asks: Vec<String>,
    /// Its toolbar popup and options page, as paths inside it.
    pub popup: Option<String>,
    pub options: Option<String>,
}

/// Read manifest.json; `messages` resolves a `__MSG_name__` name from the
/// extension's default locale.
pub fn read_manifest(
    manifest: &serde_json::Value,
    messages: Option<&serde_json::Value>,
) -> Manifest {
    let text = |key: &str| manifest[key].as_str().unwrap_or_default().to_string();
    let localized = |s: String| match s.strip_prefix("__MSG_").and_then(|s| s.strip_suffix("__")) {
        Some(key) => messages
            .and_then(|m| {
                m.as_object()?
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .and_then(|(_, v)| v["message"].as_str().map(str::to_string))
            })
            .unwrap_or(s),
        None => s,
    };
    let list = |key: &str| -> Vec<String> {
        manifest[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let permissions = list("permissions");
    let mut hosts = list("host_permissions");
    if let Some(scripts) = manifest["content_scripts"].as_array() {
        for script in scripts {
            for m in script["matches"].as_array().into_iter().flatten() {
                if let Some(m) = m.as_str() {
                    hosts.push(m.to_string());
                }
            }
        }
    }
    let mut asks = Vec::new();
    let everywhere = hosts.iter().any(|h| {
        matches!(
            h.as_str(),
            "<all_urls>" | "*://*/*" | "http://*/*" | "https://*/*"
        )
    });
    if everywhere {
        asks.push("read and change everything on every site you visit".to_string());
    } else if !hosts.is_empty() {
        let mut sites: Vec<String> = hosts
            .iter()
            .map(|h| {
                h.split_once("://")
                    .map_or(h.as_str(), |(_, rest)| {
                        rest.split('/').next().unwrap_or(rest)
                    })
                    .to_string()
            })
            .collect();
        sites.sort();
        sites.dedup();
        asks.push(format!("read and change pages on {}", sites.join(", ")));
    }
    for (permission, words) in PERMISSION_WORDS {
        if permissions.iter().any(|p| p == permission) {
            asks.push(words.to_string());
        }
    }
    asks.dedup();
    let page = |value: &serde_json::Value| {
        value
            .as_str()
            .filter(|p| !p.is_empty())
            .map(|p| p.trim_start_matches('/').to_string())
    };
    Manifest {
        name: localized(text("name")),
        version: text("version"),
        description: localized(text("description")),
        manifest_version: manifest["manifest_version"].as_i64().unwrap_or(0),
        asks,
        popup: page(&manifest["action"]["default_popup"]),
        options: page(&manifest["options_ui"]["page"]).or_else(|| page(&manifest["options_page"])),
    }
}

/// The id Chromium gives an extension loaded from a folder: from its
/// manifest's `key`, or else from the folder's absolute path.
pub fn folder_id(path: &str, key: Option<&str>) -> String {
    match key.and_then(base64_decode) {
        Some(key) => id_from_key(&key),
        None => id_from_key(path.as_bytes()),
    }
}

/// Standard base64 back to bytes; `None` if it isn't base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let data = bytes
        .strip_suffix(b"==")
        .or_else(|| bytes.strip_suffix(b"="))
        .unwrap_or(&bytes);
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// The Web Store's update check for an installed version.
pub fn update_check_url(id: &str, version: &str, chrome_major: i32) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=updatecheck&prodversion={chrome_major}.0\
         &acceptformat=crx3&x=id%3D{id}%26v%3D{version}%26uc"
    )
}

/// The newer version an update check found, if any: its answer has
/// `<updatecheck status="ok" version="…"/>`, or `status="noupdate"`.
pub fn update_check_version(xml: &str) -> Option<String> {
    let tag = &xml[xml.find("<updatecheck")?..];
    let tag = &tag[..tag.find('>')?];
    let attr = |name: &str| {
        let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
        let rest = &tag[start..];
        Some(rest[..rest.find('"')?].to_string())
    };
    (attr("status")? == "ok").then(|| attr("version")).flatten()
}

/// Whether version `a` is newer than `b`, comparing dotted numbers.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

/// Permissions worth naming, in Chrome's own words where it has them.
const PERMISSION_WORDS: &[(&str, &str)] = &[
    ("debugger", "use the debugger on your pages"),
    ("nativeMessaging", "talk to programs on your computer"),
    ("proxy", "change your proxy settings"),
    ("webRequest", "see your network traffic"),
    ("declarativeNetRequest", "block page content"),
    ("history", "read and change your history"),
    ("bookmarks", "read and change your bookmarks"),
    ("cookies", "read and change your cookies"),
    ("tabs", "read your open tabs' addresses"),
    ("clipboardRead", "read your clipboard"),
    ("clipboardWrite", "change your clipboard"),
    ("downloads", "manage your downloads"),
    ("management", "manage your other extensions"),
    ("privacy", "change your privacy settings"),
    ("geolocation", "know your location"),
    ("notifications", "show notifications"),
    ("webAuthenticationProxy", "answer passkey requests"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a protobuf length-delimited field.
    fn field(number: u64, value: &[u8]) -> Vec<u8> {
        fn varint(mut n: u64, out: &mut Vec<u8>) {
            loop {
                let byte = (n & 0x7f) as u8;
                n >>= 7;
                if n == 0 {
                    out.push(byte);
                    return;
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

    /// A CRX3 file whose header has `keys` and signs `id_key`'s id.
    pub(crate) fn crx(keys: &[&[u8]], id_key: &[u8], zip: &[u8]) -> Vec<u8> {
        let mut header = Vec::new();
        for key in keys {
            header.extend(field(2, &[field(1, key), field(2, b"sig")].concat()));
        }
        let crx_id = &Sha256::digest(id_key)[..16];
        header.extend(field(10000, &field(1, crx_id)));
        let mut out = b"Cr24".to_vec();
        out.extend(3u32.to_le_bytes());
        out.extend((header.len() as u32).to_le_bytes());
        out.extend(header);
        out.extend_from_slice(zip);
        out
    }

    #[test]
    fn ids_come_from_keys_and_store_pages() {
        assert_eq!(id_from_key(b"key"), id_from_key(b"key"));
        assert!(
            id_from_key(b"key")
                .bytes()
                .all(|b| (b'a'..=b'p').contains(&b))
        );
        assert_eq!(id_from_key(b"key").len(), 32);
        let id = "ddkjiahejlhfcafbddmgiahcphecmpfh";
        assert_eq!(parse_id(id).as_deref(), Some(id));
        assert_eq!(
            parse_id(&format!(
                "https://chromewebstore.google.com/detail/ublock-origin-lite/{id}?hl=en"
            ))
            .as_deref(),
            Some(id)
        );
        assert_eq!(parse_id("ublock"), None);
        assert_eq!(parse_id("https://example.com/qqqq"), None);
        assert!(
            store_url(id, 154).contains("prodversion=154.0") && store_url(id, 154).contains(id)
        );
    }

    #[test]
    fn a_crx_gives_the_key_matching_its_signed_id_and_its_zip() {
        let file = crx(&[b"other", b"mine"], b"mine", b"PK zip");
        let parsed = parse_crx(&file, Some(&id_from_key(b"mine"))).unwrap();
        assert_eq!(parsed.public_key, b"mine");
        assert_eq!(parsed.zip, b"PK zip");
        assert_eq!(parsed.id, id_from_key(b"mine"));
        // Another id than the one asked for.
        let err = parse_crx(&file, Some(&id_from_key(b"other"))).unwrap_err();
        assert!(err.contains("not"), "{err}");
        // A signed id no key gives.
        assert!(parse_crx(&crx(&[b"other"], b"mine", b""), None).is_err());
        assert!(
            parse_crx(b"PK\x03\x04", None)
                .unwrap_err()
                .contains("not an extension")
        );
        assert!(parse_crx(&file[..20], None).is_err());
    }

    #[test]
    fn folder_ids_come_from_the_key_or_the_path() {
        let key = base64(b"some key");
        assert_eq!(base64_decode(&key).as_deref(), Some(&b"some key"[..]));
        assert_eq!(folder_id("/x", Some(&key)), id_from_key(b"some key"));
        assert_eq!(folder_id("/home/a/ext", None), id_from_key(b"/home/a/ext"));
        assert_eq!(base64_decode("Zm9v!"), None);
        for text in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            assert_eq!(
                base64_decode(&base64(text.as_bytes())).unwrap(),
                text.as_bytes()
            );
        }
    }

    #[test]
    fn update_checks_find_newer_versions() {
        let ok = r#"<?xml version="1.0"?><gupdate><app appid="x"><updatecheck codebase="https://x/y.crx" status="ok" version="2026.10.8"/></app></gupdate>"#;
        assert_eq!(update_check_version(ok).as_deref(), Some("2026.10.8"));
        // As the Web Store answers (shortened).
        let real = r#"<gupdate protocol="2.0"><app appid="d" status="ok"><updatecheck _esbAllowlist="true" codebase="https://x/D.crx" fp="1.6e" hash_sha256="ab" protected="0" size="9672935" status="ok" version="2026.1006.1931"/></app></gupdate>"#;
        assert_eq!(
            update_check_version(real).as_deref(),
            Some("2026.1006.1931")
        );
        let none = r#"<gupdate><app appid="x"><updatecheck status="noupdate"/></app></gupdate>"#;
        assert_eq!(update_check_version(none), None);
        assert!(update_check_url("x", "1.2", 154).contains("v%3D1.2"));
        assert!(newer("1.10", "1.9"));
        assert!(newer("2026.1007.1", "2026.1006.1931"));
        assert!(!newer("1.0", "1.0"));
        assert!(!newer("1.0", "1.0.1"));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn manifests_say_what_they_ask_for() {
        let manifest = serde_json::json!({
            "name": "__MSG_extName__",
            "version": "1.2",
            "manifest_version": 3,
            "permissions": ["storage", "nativeMessaging", "declarativeNetRequest"],
            "host_permissions": ["<all_urls>"],
        });
        let messages = serde_json::json!({ "extName": { "message": "Locker" } });
        let m = read_manifest(&manifest, Some(&messages));
        assert_eq!(m.name, "Locker");
        assert_eq!((&m.popup, &m.options), (&None, &None));
        let pages = serde_json::json!({ "action": { "default_popup": "popup/index.html" }, "options_page": "options.html" });
        let with_pages = read_manifest(&pages, None);
        assert_eq!(with_pages.popup.as_deref(), Some("popup/index.html"));
        assert_eq!(with_pages.options.as_deref(), Some("options.html"));
        let ui = serde_json::json!({ "options_ui": { "page": "/settings.html" }, "options_page": "old.html" });
        assert_eq!(
            read_manifest(&ui, None).options.as_deref(),
            Some("settings.html")
        );
        assert_eq!(m.manifest_version, 3);
        assert_eq!(
            m.asks,
            [
                "read and change everything on every site you visit",
                "talk to programs on your computer",
                "block page content"
            ]
        );
        let some = serde_json::json!({
            "name": "Mail helper",
            "manifest_version": 3,
            "content_scripts": [{ "matches": ["https://mail.example.com/*", "https://*.example.org/*"] }],
        });
        assert_eq!(
            read_manifest(&some, None).asks,
            ["read and change pages on *.example.org, mail.example.com"]
        );
    }
}
