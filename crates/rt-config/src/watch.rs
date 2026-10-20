//! Noticing that the config files changed on disk, for `config.watch`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A file's modification time and size, or `None` while it doesn't exist.
pub type Stamp = Option<(SystemTime, u64)>;

/// Taken just before reading a file, so a change while it's read still counts.
pub fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Watches files by polling. A change counts once two polls in a row see the
/// same thing, so a save in several writes reloads once, after the last.
#[derive(Debug, Default)]
pub struct Watcher {
    files: Vec<PathBuf>,
    seen: Vec<Stamp>,
    pending: Option<Vec<Stamp>>,
}

impl Watcher {
    /// Watch `files` from how they were when they were read.
    pub fn new(files: Vec<(PathBuf, Stamp)>) -> Watcher {
        let (files, seen) = files.into_iter().unzip();
        Watcher {
            files,
            seen,
            pending: None,
        }
    }

    /// Also watch `file`, e.g. a module required after the config loaded.
    pub fn add(&mut self, file: PathBuf, seen: Stamp) {
        if !self.files.contains(&file) {
            self.files.push(file);
            self.seen.push(seen);
            self.pending = None;
        }
    }

    /// The files that changed and have since stayed the same.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        let now: Vec<Stamp> = self.files.iter().map(|f| stamp(f)).collect();
        if now == self.seen {
            self.pending = None;
            return Vec::new();
        }
        if self.pending.as_ref() != Some(&now) {
            self.pending = Some(now);
            return Vec::new();
        }
        let changed = self
            .files
            .iter()
            .zip(self.seen.iter().zip(&now))
            .filter(|(_, (before, after))| before != after)
            .map(|(file, _)| file.clone())
            .collect();
        self.seen = now;
        self.pending = None;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_count_once_they_settle() {
        let dir = std::env::temp_dir().join(format!("rt-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = dir.join("config.toml");
        let lua = dir.join("config.lua");
        std::fs::write(&toml, "a = 1\n").unwrap();
        let mut watcher = Watcher::new(vec![(toml.clone(), stamp(&toml)), (lua.clone(), None)]);
        assert!(watcher.poll().is_empty());
        // A file that appears counts as a change, after one quiet poll.
        std::fs::write(&lua, "-- hi\n").unwrap();
        assert!(watcher.poll().is_empty());
        assert_eq!(watcher.poll(), std::slice::from_ref(&lua));
        assert!(watcher.poll().is_empty());
        // Still being written: wait until two polls agree.
        std::fs::write(&toml, "a = 22\n").unwrap();
        assert!(watcher.poll().is_empty());
        std::fs::write(&toml, "a = 333\n").unwrap();
        assert!(watcher.poll().is_empty());
        assert_eq!(watcher.poll(), std::slice::from_ref(&toml));
        std::fs::remove_file(&lua).unwrap();
        assert!(watcher.poll().is_empty());
        assert_eq!(watcher.poll(), [lua]);
        // A module added later counts from when it was read.
        let module = dir.join("words.lua");
        std::fs::write(&module, "return 1\n").unwrap();
        let read = stamp(&module);
        std::fs::write(&module, "return 22\n").unwrap();
        watcher.add(module.clone(), read);
        assert!(watcher.poll().is_empty());
        assert_eq!(watcher.poll(), [module]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
