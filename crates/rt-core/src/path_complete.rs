//! `Tab` in a file path prompt (where to save a download): complete the last
//! path component like a shell does.

use std::path::{Path, PathBuf};

/// The text after completing its last component, or `None` if nothing
/// matches or nothing would change. `home` expands a leading `~/` for the
/// lookup only; the text keeps the `~`.
pub fn complete(text: &str, home: Option<&Path>) -> Option<String> {
    let (dir, prefix) = split(text);
    let lookup: PathBuf = match (strip_home(dir), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if dir.is_empty() => PathBuf::from("."),
        _ => PathBuf::from(dir),
    };
    let mut matches: Vec<(String, bool)> = std::fs::read_dir(&lookup)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let hidden_ok = prefix.starts_with('.') || !name.starts_with('.');
            (name.starts_with(prefix) && hidden_ok).then(|| (name, e.path().is_dir()))
        })
        .collect();
    matches.sort();
    let (first, _) = matches.first()?;
    let common = matches.iter().fold(first.clone(), |common, (name, _)| {
        common
            .chars()
            .zip(name.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect()
    });
    let mut completed = format!("{dir}{common}");
    if let [(_, true)] = matches.as_slice() {
        completed.push(separator(dir));
    }
    (completed != text).then_some(completed)
}

/// Whether `c` separates folders: `/`, and on Windows `\` too.
pub fn is_separator(c: char) -> bool {
    c == '/' || (cfg!(windows) && c == '\\')
}

/// `text` split after its last separator: the folder and the start of a name.
pub fn split(text: &str) -> (&str, &str) {
    match text.rfind(is_separator) {
        Some(i) => (&text[..=i], &text[i + 1..]),
        None => ("", text),
    }
}

/// What's after a leading `~/` (or `~\` on Windows).
pub fn strip_home(path: &str) -> Option<&str> {
    path.strip_prefix('~')
        .filter(|rest| rest.starts_with(is_separator))
        .map(|rest| &rest[1..])
}

/// The separator to put after a folder in `dir`: the one it already uses.
pub fn separator(dir: &str) -> char {
    dir.chars()
        .last()
        .filter(|c| is_separator(*c))
        .unwrap_or('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_like_a_shell() {
        let dir = std::env::temp_dir().join(format!("rt-path-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Downloads")).unwrap();
        std::fs::create_dir_all(dir.join("Documents")).unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        let base = format!("{}/", dir.display());

        assert_eq!(
            complete(&format!("{base}Dow"), None),
            Some(format!("{base}Downloads/"))
        );
        assert_eq!(
            complete(&format!("{base}Do"), None),
            None,
            "nothing more in common"
        );
        assert_eq!(
            complete(&format!("{base}D"), None),
            Some(format!("{base}Do"))
        );
        assert_eq!(
            complete(&format!("{base}n"), None),
            Some(format!("{base}notes.txt"))
        );
        assert_eq!(
            complete(&format!("{base}.h"), None),
            Some(format!("{base}.hidden"))
        );
        assert_eq!(complete(&format!("{base}zzz"), None), None);
        assert_eq!(
            complete("~/Dow", Some(&dir)),
            Some("~/Downloads/".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn completes_windows_paths() {
        let dir = std::env::temp_dir().join(format!("rt-path-complete-win-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Downloads")).unwrap();
        let base = format!("{}\\", dir.display());
        assert_eq!(
            complete(&format!("{base}Dow"), None),
            Some(format!("{base}Downloads\\"))
        );
        assert_eq!(
            complete("~\\Dow", Some(&dir)),
            Some("~\\Downloads\\".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
