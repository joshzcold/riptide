//! `riptide://edit/<path>`, the built-in editor page: which files it opens and
//! how its URLs map to paths. A page's URL is its file's `file://` URL with
//! `file://` swapped for `riptide://edit`, so `/tmp/a b.rs` is
//! `riptide://edit/tmp/a%20b.rs`.

use std::path::Path;

pub const PREFIX: &str = "riptide://edit/";

/// Files larger than this open with an error instead of their text.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// How much of a file [`looks_like_text`] looks at.
pub const SNIFF_BYTES: usize = 8192;

/// File types Chromium shows better than an editor would, even though they're text.
const RENDERED: &[&str] = &[
    "html", "htm", "xhtml", "shtml", "mhtml", "mht", "svg", "pdf",
];

/// The editor page for `path`: absolute, or `~/…`. `None` for a relative path
/// or a Windows network path.
pub fn url_for_path(path: &str, home: Option<&Path>) -> Option<String> {
    let path = crate::url::local_path(path.trim(), home, cfg!(windows))?;
    edit_url(&crate::url::path_to_file_url(&path, cfg!(windows)))
}

/// The editor page for a `file://` URL on this machine, without its query or fragment.
pub fn edit_url(file_url: &str) -> Option<String> {
    let rest = file_url
        .strip_prefix("file:///")
        .or_else(|| file_url.strip_prefix("file://localhost/"))?;
    let rest = rest.split(['?', '#']).next()?;
    Some(format!("{PREFIX}{rest}"))
}

/// The file an editor page or `file://` URL names, if it's a sound local path.
pub fn path_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix(PREFIX)
        .or_else(|| url.strip_prefix("file:///"))
        .or_else(|| url.strip_prefix("file://localhost/"))?;
    let decoded = percent_decode(rest.split(['?', '#']).next()?)?;
    if decoded.is_empty()
        || decoded.contains('\0')
        || decoded.split(['/', '\\']).any(|part| part == "..")
    {
        return None;
    }
    let bytes = decoded.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/';
    Some(if cfg!(windows) && drive {
        decoded.replace('/', "\\")
    } else {
        format!("/{decoded}")
    })
}

fn percent_decode(text: &str) -> Option<String> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(byte);
        }
    }
    String::from_utf8(out).ok()
}

/// Whether the start of a file reads as text: UTF-8 without NUL bytes. A
/// character cut off at the end of `head` still counts.
pub fn looks_like_text(head: &[u8]) -> bool {
    if head.contains(&0) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && head.len() - e.valid_up_to() < 4,
    }
}

/// Whether `editor.file_urls` should send a `file://` page at `path` to the
/// editor, given the first [`SNIFF_BYTES`] of the file.
pub fn opens_in_editor(path: &str, head: &[u8]) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    !extension.is_some_and(|e| RENDERED.contains(&e.as_str())) && looks_like_text(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_urls_mirror_file_urls() {
        assert_eq!(
            edit_url("file:///tmp/a%20b.rs#L3").as_deref(),
            Some("riptide://edit/tmp/a%20b.rs")
        );
        assert_eq!(
            edit_url("file://localhost/etc/hosts").as_deref(),
            Some("riptide://edit/etc/hosts")
        );
        assert_eq!(edit_url("file://server/share/x"), None);
        assert_eq!(edit_url("https://example.com/x"), None);
    }

    #[cfg(unix)]
    #[test]
    fn paths_round_trip_through_urls() {
        let home = Path::new("/home/u");
        let url = url_for_path("~/notes/a b#1.md", Some(home)).unwrap();
        assert_eq!(url, "riptide://edit/home/u/notes/a%20b%231.md");
        assert_eq!(
            path_from_url(&url).as_deref(),
            Some("/home/u/notes/a b#1.md")
        );
        assert_eq!(url_for_path("notes.md", Some(home)), None);
        assert_eq!(
            path_from_url("file:///etc/hosts?x").as_deref(),
            Some("/etc/hosts")
        );
        assert_eq!(
            path_from_url("riptide://edit/tmp/%E2%9C%93.txt").as_deref(),
            Some("/tmp/✓.txt")
        );
    }

    #[test]
    fn bad_paths_are_refused() {
        for url in [
            "riptide://edit/",
            "riptide://edit/tmp/../etc/passwd",
            "riptide://edit/tmp/%2e%2e/etc/passwd",
            "riptide://edit/tmp/a%00b",
            "riptide://edit/tmp/%zz",
            "riptide://edit/tmp/%ff",
            "riptide://settings/",
        ] {
            assert_eq!(path_from_url(url), None, "{url}");
        }
    }

    #[test]
    fn text_is_utf8_without_nul_bytes() {
        assert!(looks_like_text(b"fn main() {}\n"));
        assert!(looks_like_text(b""));
        assert!(looks_like_text("na\u{ef}ve".as_bytes()));
        // A character cut off by the sniffing window.
        assert!(looks_like_text(&"✓".as_bytes()[..2]));
        assert!(!looks_like_text(b"\x89PNG\r\n\x1a\n\0\0"));
        assert!(!looks_like_text(b"\xff\xfe latin-1 \xe9 here"));
    }

    #[test]
    fn pages_chromium_renders_stay_out_of_the_editor() {
        assert!(opens_in_editor("/x/main.rs", b"fn main() {}"));
        assert!(opens_in_editor("/x/Makefile", b"all:\n"));
        assert!(!opens_in_editor("/x/index.HTML", b"<html>"));
        assert!(!opens_in_editor("/x/logo.svg", b"<svg/>"));
        assert!(!opens_in_editor(
            "/x/photo.jpg",
            b"\xff\xd8\xff\xe0\0\x10JFIF"
        ));
    }
}
