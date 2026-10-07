//! Reports of riptide's own crashes (Rust panics), kept as text files in
//! `<data>/crashes/`. Nothing is sent anywhere; the next start says where
//! the newest report is.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::recovery::utc_stamp;

/// Older reports beyond this many are deleted.
pub const KEEP_REPORTS: usize = 10;

/// Names the report the next start hasn't mentioned yet.
const UNSEEN: &str = ".unseen";

/// What a panic leaves behind.
pub struct Report<'a> {
    /// `riptide 0.1.0 (abc1234, CEF …)`.
    pub version: &'a str,
    pub thread: &'a str,
    pub message: &'a str,
    /// `file:line:column`, if known.
    pub location: Option<&'a str>,
    pub backtrace: &'a str,
}

impl Report<'_> {
    pub fn text(&self) -> String {
        format!(
            "riptide crashed (a Rust panic).\n\n\
             Version:  {}\nOS:       {} {}\nThread:   {}\nLocation: {}\n\n\
             Message:\n{}\n\nBacktrace:\n{}\n",
            self.version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            self.thread,
            self.location.unwrap_or("unknown"),
            self.message,
            self.backtrace.trim_end(),
        )
    }
}

pub struct CrashReports {
    dir: PathBuf,
}

impl CrashReports {
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.into() }
    }

    /// Save `text` as `crash-YYYY-MM-DD-HHMMSS.txt` (UTC), mark it for the
    /// next start, and drop all but the newest [`KEEP_REPORTS`].
    pub fn write(&self, unix_secs: u64, text: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let stamp = utc_stamp(unix_secs);
        let mut n = 1;
        let (path, mut file) = loop {
            let name = match n {
                1 => format!("crash-{stamp}.txt"),
                _ => format!("crash-{stamp}-{n}.txt"),
            };
            let path = self.dir.join(name);
            match std::fs::File::create_new(&path) {
                Ok(file) => break (path, file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 100 => n += 1,
                Err(e) => return Err(e),
            }
        };
        file.write_all(text.as_bytes())?;
        if let Some(name) = path.file_name() {
            std::fs::write(self.dir.join(UNSEEN), name.as_encoded_bytes())?;
        }
        for stale in stale(&self.list(), KEEP_REPORTS) {
            let _ = std::fs::remove_file(self.dir.join(stale));
        }
        Ok(path)
    }

    /// The newest report the last start didn't mention, once: it's then
    /// marked seen.
    pub fn take_unseen(&self) -> Option<PathBuf> {
        let marker = self.dir.join(UNSEEN);
        let name = std::fs::read_to_string(&marker).ok()?;
        let _ = std::fs::remove_file(&marker);
        let path = self.dir.join(name.trim());
        path.is_file().then_some(path)
    }

    /// Report file names, oldest first.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("crash-") && n.ends_with(".txt"))
            .collect();
        names.sort();
        names
    }
}

/// The names beyond the newest `keep`, from a list sorted oldest first.
fn stale(names: &[String], keep: usize) -> Vec<String> {
    names[..names.len().saturating_sub(keep)].to_vec()
}

/// Chromium's own crash dumps (minidumps from Crashpad), as opposed to the
/// reports of riptide's Rust panics above.
pub mod dumps {
    use std::path::{Path, PathBuf};
    use std::time::UNIX_EPOCH;

    /// Older dumps beyond this many are deleted; CEF doesn't limit them on Linux.
    pub const KEEP_DUMPS: usize = 10;

    /// Where Crashpad keeps dumps, under the data directory.
    const DIRS: [&str; 3] = ["pending", "completed", "new"];

    /// Holds the newest dump's time (Unix seconds) the last start saw.
    const SEEN: &str = "crashes/.dumps-seen";

    /// What turns Crashpad on, written next to the executable. With no
    /// `ServerURL`, dumps are only kept on disk.
    pub fn reporter_config(version: &str) -> String {
        format!(
            "# Written by riptide: Chromium's crash reporter keeps crash dumps in the\n\
             # data directory and never uploads them (there is no ServerURL).\n\
             [Config]\nProductName=riptide\nProductVersion={version}\n"
        )
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Dump {
        pub path: PathBuf,
        /// Unix seconds.
        pub modified: u64,
        pub size: u64,
    }

    /// The dumps, newest first.
    pub fn list(data_dir: &Path) -> Vec<Dump> {
        let mut dumps: Vec<Dump> = DIRS
            .iter()
            .flat_map(|dir| {
                std::fs::read_dir(data_dir.join(dir))
                    .into_iter()
                    .flatten()
                    .flatten()
            })
            .filter(|e| e.path().extension().is_some_and(|x| x == "dmp"))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let modified = meta
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                Some(Dump {
                    path: e.path(),
                    modified,
                    size: meta.len(),
                })
            })
            .collect();
        dumps.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.path.cmp(&b.path))
        });
        dumps
    }

    /// Delete all but the newest `keep` dumps.
    pub fn prune(data_dir: &Path, keep: usize) {
        for dump in list(data_dir).into_iter().skip(keep) {
            let _ = std::fs::remove_file(dump.path);
        }
    }

    /// How many dumps are newer than the last start saw, and remember the
    /// newest, so each crash is mentioned once.
    pub fn take_new(data_dir: &Path) -> usize {
        let seen: u64 = std::fs::read_to_string(data_dir.join(SEEN))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let dumps = list(data_dir);
        let new = dumps.iter().filter(|d| d.modified > seen).count();
        if let Some(newest) = dumps.first().filter(|d| d.modified > seen) {
            let marker = data_dir.join(SEEN);
            if let Some(dir) = marker.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(marker, newest.modified.to_string());
        }
        new
    }

    /// `SystemTime` for tests.
    #[cfg(test)]
    pub(super) fn at(secs: u64) -> std::time::SystemTime {
        UNIX_EPOCH + std::time::Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_are_listed_newest_first_pruned_and_mentioned_once() {
        let dir = temp("dumps");
        for (sub, name, secs) in [
            ("pending", "a.dmp", 100),
            ("completed", "b.dmp", 300),
            ("pending", "c.dmp", 200),
            ("pending", "notes.txt", 400),
        ] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            let file = std::fs::File::create(dir.join(sub).join(name)).unwrap();
            file.set_modified(dumps::at(secs)).unwrap();
        }
        let names = |d: &[dumps::Dump]| {
            d.iter()
                .map(|d| d.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&dumps::list(&dir)), ["b.dmp", "c.dmp", "a.dmp"]);
        assert_eq!(dumps::take_new(&dir), 3);
        assert_eq!(dumps::take_new(&dir), 0, "each dump is mentioned once");
        dumps::prune(&dir, 2);
        assert_eq!(names(&dumps::list(&dir)), ["b.dmp", "c.dmp"]);
        assert!(dumps::reporter_config("0.2.0").contains("ProductVersion=0.2.0"));
        assert!(
            !dumps::reporter_config("0.2.0").contains("ServerURL="),
            "nothing is uploaded"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rt-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_report_says_what_and_where() {
        let text = Report {
            version: "riptide 0.1.0 (abc1234, CEF 1)",
            thread: "main",
            message: "index out of bounds",
            location: Some("crates/rt-cef/src/tabs.rs:10:5"),
            backtrace: "   0: riptide::main\n",
        }
        .text();
        for part in [
            "Version:  riptide 0.1.0 (abc1234, CEF 1)",
            "Thread:   main",
            "Location: crates/rt-cef/src/tabs.rs:10:5",
            "Message:\nindex out of bounds",
            "Backtrace:\n   0: riptide::main\n",
        ] {
            assert!(text.contains(part), "{part:?} in {text}");
        }
    }

    #[test]
    fn the_next_start_hears_about_the_newest_report_once() {
        let dir = temp("unseen");
        let reports = CrashReports::new(&dir);
        assert_eq!(reports.take_unseen(), None);
        reports.write(86_400, "first").unwrap();
        let second = reports.write(86_400, "second").unwrap();
        assert!(
            second.ends_with("crash-1970-01-02-000000-2.txt"),
            "{second:?}"
        );
        assert_eq!(reports.take_unseen(), Some(second.clone()));
        assert_eq!(reports.take_unseen(), None);
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_newest_reports_are_kept() {
        let dir = temp("prune");
        let reports = CrashReports::new(&dir);
        for day in 1..=KEEP_REPORTS as u64 + 3 {
            reports.write(day * 86_400, "x").unwrap();
        }
        let kept = reports.list();
        assert_eq!(kept.len(), KEEP_REPORTS);
        assert_eq!(kept[0], "crash-1970-01-05-000000.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
