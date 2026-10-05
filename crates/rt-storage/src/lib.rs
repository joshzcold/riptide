//! Persistent browsing state: history (SQLite), quickmarks and bookmarks
//! (qutebrowser-compatible text files) and sessions (TOML).

pub mod history;
pub mod marks;
pub mod sessions;

use std::path::Path;

pub use history::{History, HistoryEntry};
pub use marks::{Bookmarks, Quickmarks};
pub use sessions::{Session, Sessions, TabState, WindowState};

pub struct Storage {
    /// `None` if the database could not be opened; browsing still works.
    pub history: Option<History>,
    pub quickmarks: Quickmarks,
    pub bookmarks: Bookmarks,
    pub sessions: Sessions,
}

impl Storage {
    /// Quickmarks and bookmarks live with the config (like qutebrowser), so
    /// they can be kept in dotfiles; history and sessions are data.
    pub fn open(config_dir: &Path, data_dir: &Path) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let history = match History::open(&data_dir.join("history.sqlite")) {
            Ok(h) => Some(h),
            Err(e) => {
                errors.push(format!("history: {e}"));
                None
            }
        };
        let quickmarks_path = config_dir.join("quickmarks");
        let quickmarks = Quickmarks::load(&quickmarks_path).unwrap_or_else(|e| {
            errors.push(format!("{}: {e}", quickmarks_path.display()));
            Quickmarks::unsaved()
        });
        let bookmarks_path = config_dir.join("bookmarks").join("urls");
        let bookmarks = Bookmarks::load(&bookmarks_path).unwrap_or_else(|e| {
            errors.push(format!("{}: {e}", bookmarks_path.display()));
            Bookmarks::unsaved()
        });
        let storage = Self {
            history,
            quickmarks,
            bookmarks,
            sessions: Sessions::new(&data_dir.join("sessions")),
        };
        (storage, errors)
    }

    /// Read the quickmarks and bookmarks files again, after editing them by hand.
    pub fn reload_marks(&mut self, config_dir: &Path) -> Vec<String> {
        let mut errors = Vec::new();
        let quickmarks_path = config_dir.join("quickmarks");
        match Quickmarks::load(&quickmarks_path) {
            Ok(q) => self.quickmarks = q,
            Err(e) => errors.push(format!("{}: {e}", quickmarks_path.display())),
        }
        let bookmarks_path = config_dir.join("bookmarks").join("urls");
        match Bookmarks::load(&bookmarks_path) {
            Ok(b) => self.bookmarks = b,
            Err(e) => errors.push(format!("{}: {e}", bookmarks_path.display())),
        }
        errors
    }
}

/// qutebrowser-style matching: every word appears somewhere in the fields,
/// in any order, ignoring case.
pub fn matches(pattern: &str, fields: &[&str]) -> bool {
    let haystack = fields.join(" ").to_lowercase();
    pattern
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_is_word_based() {
        assert!(matches("", &["anything"]));
        assert!(matches(
            "rust DOCS",
            &["https://doc.rust-lang.org", "Rust Docs"]
        ));
        assert!(!matches(
            "rust python",
            &["https://doc.rust-lang.org", "Rust Docs"]
        ));
    }
}
