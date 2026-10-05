//! Quickmarks and bookmarks in qutebrowser's plain-text formats, so files can
//! be copied over from `~/.config/qutebrowser/` unchanged:
//!
//! - `quickmarks`: one `name url` per line (the name may contain spaces)
//! - `bookmarks/urls`: one `url title` per line (the title may be empty)

use std::path::{Path, PathBuf};

fn read_lines(path: &Path) -> std::io::Result<Vec<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Write via a temporary file so a crash never truncates the original.
fn write_lines(path: &Path, lines: impl Iterator<Item = String>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = lines.map(|l| l + "\n").collect();
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

fn save_to(path: &Option<PathBuf>, lines: impl Iterator<Item = String>) -> std::io::Result<()> {
    match path {
        Some(path) => write_lines(path, lines),
        None => Err(std::io::Error::other(
            "not saved: the file could not be read at startup, so saving could overwrite it",
        )),
    }
}

#[derive(Debug)]
pub struct Quickmarks {
    /// `None` when the file exists but could not be read; see [`save_to`].
    path: Option<PathBuf>,
    /// Kept in file order, so saving doesn't reshuffle a hand-edited file.
    marks: Vec<(String, String)>,
}

impl Quickmarks {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let marks = read_lines(path)?
            .into_iter()
            .filter_map(|line| {
                let (name, url) = line.rsplit_once(' ')?;
                Some((name.trim().to_string(), url.to_string()))
            })
            .collect();
        Ok(Self {
            path: Some(path.to_path_buf()),
            marks,
        })
    }

    /// An empty, read-only set for when the file could not be read.
    pub fn unsaved() -> Self {
        Self {
            path: None,
            marks: Vec::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.marks.iter().map(|(n, u)| (n.as_str(), u.as_str()))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.marks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }

    pub fn name_for(&self, url: &str) -> Option<&str> {
        self.marks
            .iter()
            .find(|(_, u)| u == url)
            .map(|(n, _)| n.as_str())
    }

    /// Add or replace; returns whether an existing mark was replaced.
    pub fn add(&mut self, name: &str, url: &str) -> std::io::Result<bool> {
        let replaced = match self.marks.iter_mut().find(|(n, _)| n == name) {
            Some(mark) => {
                mark.1 = url.to_string();
                true
            }
            None => {
                self.marks.push((name.to_string(), url.to_string()));
                false
            }
        };
        self.save()?;
        Ok(replaced)
    }

    pub fn remove(&mut self, name: &str) -> std::io::Result<bool> {
        let before = self.marks.len();
        self.marks.retain(|(n, _)| n != name);
        let removed = self.marks.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save(&self) -> std::io::Result<()> {
        save_to(
            &self.path,
            self.marks.iter().map(|(n, u)| format!("{n} {u}")),
        )
    }
}

#[derive(Debug)]
pub struct Bookmarks {
    path: Option<PathBuf>,
    marks: Vec<(String, String)>,
}

impl Bookmarks {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let marks = read_lines(path)?
            .into_iter()
            .map(|line| match line.split_once(' ') {
                Some((url, title)) => (url.to_string(), title.trim().to_string()),
                None => (line, String::new()),
            })
            .collect();
        Ok(Self {
            path: Some(path.to_path_buf()),
            marks,
        })
    }

    /// An empty, read-only set for when the file could not be read.
    pub fn unsaved() -> Self {
        Self {
            path: None,
            marks: Vec::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.marks.iter().map(|(u, t)| (u.as_str(), t.as_str()))
    }

    pub fn contains(&self, url: &str) -> bool {
        self.marks.iter().any(|(u, _)| u == url)
    }

    /// Returns false if the URL was already bookmarked.
    pub fn add(&mut self, url: &str, title: &str) -> std::io::Result<bool> {
        if self.contains(url) {
            return Ok(false);
        }
        self.marks.push((url.to_string(), title.trim().to_string()));
        self.save()?;
        Ok(true)
    }

    pub fn remove(&mut self, url: &str) -> std::io::Result<bool> {
        let before = self.marks.len();
        self.marks.retain(|(u, _)| u != url);
        let removed = self.marks.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn save(&self) -> std::io::Result<()> {
        save_to(
            &self.path,
            self.marks.iter().map(|(u, t)| {
                if t.is_empty() {
                    u.clone()
                } else {
                    format!("{u} {t}")
                }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rt-marks-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_qutebrowser_quickmarks() {
        let dir = TempDir::new("qm");
        let path = dir.0.join("quickmarks");
        std::fs::write(
            &path,
            "rust docs https://doc.rust-lang.org/\nqb https://qutebrowser.org\n\n",
        )
        .unwrap();
        let mut q = Quickmarks::load(&path).unwrap();
        assert_eq!(q.get("rust docs"), Some("https://doc.rust-lang.org/"));
        assert_eq!(q.name_for("https://qutebrowser.org"), Some("qb"));
        assert!(!q.add("gh", "https://github.com").unwrap());
        assert!(q.add("qb", "https://qutebrowser.org/doc/").unwrap());
        assert!(q.remove("rust docs").unwrap());
        assert!(!q.remove("rust docs").unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "qb https://qutebrowser.org/doc/\ngh https://github.com\n"
        );
    }

    #[test]
    fn unreadable_file_is_never_overwritten() {
        let dir = TempDir::new("bad");
        let path = dir.0.join("quickmarks");
        std::fs::write(&path, [0xff, 0xfe, b'\n']).unwrap();
        assert!(Quickmarks::load(&path).is_err());
        let mut q = Quickmarks::unsaved();
        assert!(q.add("x", "https://x.org").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe, b'\n']);
    }

    #[test]
    fn reads_qutebrowser_bookmarks() {
        let dir = TempDir::new("bm");
        let path = dir.0.join("bookmarks").join("urls");
        let mut b = Bookmarks::load(&path).unwrap();
        assert!(b.add("https://x.org/", "X site").unwrap());
        assert!(!b.add("https://x.org/", "again").unwrap());
        assert!(b.add("https://y.org/", "").unwrap());
        let b2 = Bookmarks::load(&path).unwrap();
        assert_eq!(
            b2.iter().collect::<Vec<_>>(),
            [("https://x.org/", "X site"), ("https://y.org/", "")]
        );
        assert!(b.remove("https://x.org/").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "https://y.org/\n");
    }
}
