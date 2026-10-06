//! Crash recovery: keeping the tabs a crash left behind under their own name,
//! and not reopening tabs that crash the browser again straight away.

use crate::sessions::Sessions;

/// Saved every `auto_save.interval` and deleted on a clean exit, so finding
/// it at startup means the last run crashed.
pub const AUTOSAVE: &str = "_autosave";
/// A crash's tabs move to `_crashed-<UTC date and time>`, which sorts by age.
pub const CRASHED_PREFIX: &str = "_crashed-";
/// How many crashed sessions are kept.
pub const KEEP_CRASHED: usize = 5;
/// Set while restored tabs are on probation; see [`Recovery::OfferAfterLoop`].
const RECOVERING_MARKER: &str = ".recovering";

/// What startup does with a crash's tabs.
#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No crash.
    Nothing,
    /// Reopen them.
    Restore,
    /// URLs were given on the command line: open those, and say where the
    /// crashed tabs are.
    Offer,
    /// The last start restored tabs and crashed again soon after, so they may
    /// be what crashes it: don't reopen them, say where they are.
    OfferAfterLoop,
}

pub fn recovery(crashed: bool, urls_given: bool, was_recovering: bool) -> Recovery {
    match (crashed, was_recovering, urls_given) {
        (false, _, _) => Recovery::Nothing,
        (true, true, _) => Recovery::OfferAfterLoop,
        (true, false, true) => Recovery::Offer,
        (true, false, false) => Recovery::Restore,
    }
}

/// `_crashed-YYYY-MM-DD-HHMMSS` for a Unix time, in UTC.
pub fn crashed_name(unix_secs: u64) -> String {
    format!("{CRASHED_PREFIX}{}", utc_stamp(unix_secs))
}

/// `YYYY-MM-DD-HHMMSS` for a Unix time, in UTC; sorts by time.
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}-{:02}{:02}{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Year, month and day of a day count since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The crashed sessions beyond the newest `keep`, oldest first.
pub fn stale_crashed(names: &[String], keep: usize) -> Vec<String> {
    let mut crashed: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with(CRASHED_PREFIX))
        .collect();
    crashed.sort();
    let stale = crashed.len().saturating_sub(keep);
    crashed.into_iter().take(stale).cloned().collect()
}

impl Sessions {
    /// After a crash, move the autosave to a `_crashed-…` name so nothing
    /// overwrites it, drop all but the newest [`KEEP_CRASHED`], and return
    /// its name. `None` when the last run exited cleanly.
    pub fn take_crashed(&self, unix_secs: u64) -> Result<Option<String>, String> {
        if !self.exists(AUTOSAVE) {
            return Ok(None);
        }
        let name = crashed_name(unix_secs);
        self.rename(AUTOSAVE, &name)?;
        for stale in stale_crashed(&self.list(), KEEP_CRASHED) {
            let _ = self.delete(&stale);
        }
        Ok(Some(name))
    }

    /// Whether the last start restored crashed tabs and hasn't been running
    /// long enough since to clear [`Sessions::set_recovering`].
    pub fn was_recovering(&self) -> bool {
        self.dir().join(RECOVERING_MARKER).exists()
    }

    pub fn set_recovering(&self, on: bool) {
        let marker = self.dir().join(RECOVERING_MARKER);
        if on {
            let _ = std::fs::create_dir_all(self.dir());
            let _ = std::fs::write(marker, "");
        } else {
            let _ = std::fs::remove_file(marker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::{Session, TabState, WindowState};

    #[test]
    fn startup_restores_offers_or_does_nothing() {
        assert_eq!(recovery(false, false, false), Recovery::Nothing);
        assert_eq!(recovery(false, true, true), Recovery::Nothing);
        assert_eq!(recovery(true, false, false), Recovery::Restore);
        assert_eq!(recovery(true, true, false), Recovery::Offer);
        assert_eq!(recovery(true, false, true), Recovery::OfferAfterLoop);
        assert_eq!(recovery(true, true, true), Recovery::OfferAfterLoop);
    }

    #[test]
    fn crashed_names_are_utc_and_sort_by_age() {
        assert_eq!(crashed_name(0), "_crashed-1970-01-01-000000");
        // 2026-10-06 11:58:03 UTC
        assert_eq!(crashed_name(1_791_287_883), "_crashed-2026-10-06-115803");
        // A leap day.
        assert_eq!(crashed_name(1_709_208_000), "_crashed-2024-02-29-120000");
        assert!(crashed_name(1_791_287_883) < crashed_name(1_791_287_884));
        crate::sessions::validate_name(&crashed_name(1_791_287_883)).unwrap();
    }

    #[test]
    fn only_the_newest_crashed_sessions_are_kept() {
        let names: Vec<String> = [
            "_crashed-2026-10-03-000000",
            "default",
            "_crashed-2026-10-01-000000",
            "_crashed-2026-10-02-000000",
            "_autosave",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            stale_crashed(&names, 2),
            ["_crashed-2026-10-01-000000"].map(String::from)
        );
        assert!(stale_crashed(&names, 5).is_empty());
    }

    fn session(url: &str) -> Session {
        Session {
            windows: vec![WindowState {
                active: 0,
                tabs: vec![TabState {
                    url: url.into(),
                    title: String::new(),
                    pinned: false,
                }],
            }],
        }
    }

    #[test]
    fn take_crashed_moves_the_autosave_aside_and_prunes() {
        let dir = std::env::temp_dir().join(format!("rt-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sessions = Sessions::new(&dir);
        assert_eq!(sessions.take_crashed(1), Ok(None), "no autosave, no crash");

        for day in 1..=KEEP_CRASHED as u64 {
            sessions
                .save(AUTOSAVE, &session("https://old.example/"))
                .unwrap();
            sessions.take_crashed(day * 86_400).unwrap().unwrap();
        }
        sessions
            .save(AUTOSAVE, &session("https://crashed.example/"))
            .unwrap();
        let name = sessions.take_crashed(100 * 86_400).unwrap().unwrap();
        assert!(!sessions.exists(AUTOSAVE));
        assert_eq!(
            sessions.load(&name).unwrap().windows[0].tabs[0].url,
            "https://crashed.example/"
        );
        let kept: Vec<String> = sessions
            .list()
            .into_iter()
            .filter(|n| n.starts_with(CRASHED_PREFIX))
            .collect();
        assert_eq!(kept.len(), KEEP_CRASHED);
        assert!(!kept.contains(&crashed_name(86_400)), "the oldest went");

        assert!(!sessions.was_recovering());
        sessions.set_recovering(true);
        assert!(sessions.was_recovering());
        assert!(!sessions.list().iter().any(|n| n.contains("recovering")));
        sessions.set_recovering(false);
        assert!(!sessions.was_recovering());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
