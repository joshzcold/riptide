//! Where downloads go and what they're called.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::paths::Platform;

/// The system Downloads folder: `XDG_DOWNLOAD_DIR` (or `user-dirs.dirs`) on
/// Linux, `~/Downloads` on macOS and Windows.
pub fn system_dir(platform: Platform, env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |key: &str| env(key).map(PathBuf::from).filter(|p| p.is_absolute());
    let home_key = if platform == Platform::Windows {
        "USERPROFILE"
    } else {
        "HOME"
    };
    let home = absolute(home_key)?;
    if platform == Platform::Linux {
        if let Some(dir) = absolute("XDG_DOWNLOAD_DIR") {
            return Some(dir);
        }
        let config = absolute("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs"))
            && let Some(dir) = parse_user_dirs(&text, &home)
        {
            return Some(dir);
        }
    }
    Some(home.join("Downloads"))
}

pub fn current_system_dir() -> Option<PathBuf> {
    system_dir(Platform::current(), &|key| std::env::var_os(key))
}

/// Read `XDG_DOWNLOAD_DIR="$HOME/Downloads"` from xdg-user-dirs' file.
fn parse_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("XDG_DOWNLOAD_DIR="))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    path.is_absolute().then_some(path)
}

/// Reduce a server-suggested name to a single safe file name, so names like
/// `../../.bashrc` or `a/b` can't escape the downloads folder.
pub fn sanitize_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned.to_string()
    }
}

/// `dir/name`, or `dir/name (1).ext` and so on if that already exists.
pub fn unique_path(dir: &Path, name: &str, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let candidate = dir.join(name);
    if !exists(&candidate) {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !exists(p))
        .expect("an unused name")
}

/// Expand a leading `~` in a path typed at a prompt.
pub fn expand_home(input: &str, home: Option<&Path>) -> PathBuf {
    match (input.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.into()))
            .collect();
        move |k| vars.get(k).cloned()
    }

    // Unix-style absolute paths aren't absolute on Windows.
    #[cfg(not(windows))]
    #[test]
    fn platform_download_dirs() {
        let e = env(&[
            ("HOME", "/nonexistent-home"),
            ("XDG_CONFIG_HOME", "/nonexistent-cfg"),
        ]);
        assert_eq!(
            system_dir(Platform::Linux, &e),
            Some(PathBuf::from("/nonexistent-home/Downloads"))
        );
        let e = env(&[("HOME", "/h"), ("XDG_DOWNLOAD_DIR", "/data/dl")]);
        assert_eq!(
            system_dir(Platform::Linux, &e),
            Some(PathBuf::from("/data/dl"))
        );
        let e = env(&[("HOME", "/Users/u")]);
        assert_eq!(
            system_dir(Platform::MacOs, &e),
            Some(PathBuf::from("/Users/u/Downloads"))
        );
        assert_eq!(system_dir(Platform::Linux, &env(&[])), None);
    }

    // Unix-style absolute paths aren't absolute on Windows.
    #[cfg(not(windows))]
    #[test]
    fn reads_xdg_user_dirs() {
        let home = Path::new("/home/u");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n";
        assert_eq!(
            parse_user_dirs(text, home),
            Some(PathBuf::from("/home/u/Téléchargements"))
        );
        assert_eq!(
            parse_user_dirs("XDG_DOWNLOAD_DIR=\"/srv/dl\"", home),
            Some(PathBuf::from("/srv/dl"))
        );
        assert_eq!(parse_user_dirs("XDG_DOWNLOAD_DIR=\"relative\"", home), None);
    }

    #[test]
    fn sanitizes_suggested_names() {
        assert_eq!(sanitize_name("report.pdf"), "report.pdf");
        assert_eq!(sanitize_name("../../.bashrc"), "bashrc");
        assert_eq!(sanitize_name("C:\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(sanitize_name("a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(sanitize_name("line\nbreak"), "line_break");
        assert_eq!(sanitize_name(""), "download");
        assert_eq!(sanitize_name("..."), "download");
    }

    #[test]
    fn unique_names() {
        let taken: HashSet<PathBuf> = ["/d/a.pdf", "/d/a (1).pdf", "/d/noext"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let exists = |p: &Path| taken.contains(p);
        assert_eq!(
            unique_path(Path::new("/d"), "b.pdf", &exists),
            PathBuf::from("/d/b.pdf")
        );
        assert_eq!(
            unique_path(Path::new("/d"), "a.pdf", &exists),
            PathBuf::from("/d/a (2).pdf")
        );
        assert_eq!(
            unique_path(Path::new("/d"), "noext", &exists),
            PathBuf::from("/d/noext (1)")
        );
    }

    #[test]
    fn expands_home() {
        let home = Some(Path::new("/home/u"));
        assert_eq!(expand_home("~/x.pdf", home), PathBuf::from("/home/u/x.pdf"));
        assert_eq!(expand_home("~", home), PathBuf::from("/home/u"));
        assert_eq!(expand_home("~bob/x", home), PathBuf::from("~bob/x"));
        assert_eq!(expand_home("/abs", home), PathBuf::from("/abs"));
    }
}
