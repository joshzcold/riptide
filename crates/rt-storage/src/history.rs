//! Browsing history in SQLite: every visit, plus one row per URL for fast
//! completion (the same split qutebrowser uses).

use std::path::Path;

use rusqlite::{Connection, OpenFlags, params, params_from_iter};

const SCHEMA_VERSION: i32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    /// Seconds since the Unix epoch.
    pub last_visit: i64,
}

pub struct History {
    conn: Connection,
}

/// URLs that aren't worth remembering.
pub fn is_recordable(url: &str) -> bool {
    ![
        "data:",
        "about:",
        "chrome:",
        "chrome-error:",
        "devtools:",
        "javascript:",
        "blob:",
    ]
    .iter()
    .any(|scheme| url.starts_with(scheme))
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS visits (
                     url TEXT NOT NULL,
                     title TEXT NOT NULL,
                     atime INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS visits_url ON visits (url);
                 CREATE TABLE IF NOT EXISTS completion (
                     url TEXT PRIMARY KEY,
                     title TEXT NOT NULL,
                     last_visit INTEGER NOT NULL,
                     visits INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS completion_last_visit ON completion (last_visit);
                 PRAGMA user_version = 1;",
            )?;
        }
        if version < 2 {
            // Each site's icon, for completion.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS favicons (
                     host TEXT PRIMARY KEY,
                     icon TEXT NOT NULL,
                     updated INTEGER NOT NULL
                 );
                 PRAGMA user_version = 2;",
            )?;
        }
        Ok(Self { conn })
    }

    pub fn add_visit(&self, url: &str, title: &str, now: i64) -> rusqlite::Result<()> {
        if !is_recordable(url) {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO visits (url, title, atime) VALUES (?1, ?2, ?3)",
            params![url, title, now],
        )?;
        self.conn.execute(
            "INSERT INTO completion (url, title, last_visit, visits) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT (url) DO UPDATE SET
                 title = CASE WHEN excluded.title = '' THEN title ELSE excluded.title END,
                 last_visit = excluded.last_visit,
                 visits = visits + 1",
            params![url, title, now],
        )?;
        Ok(())
    }

    /// Import qutebrowser's `history.sqlite`: every visit that wasn't a
    /// redirect. Visits already here (same URL and time) are skipped, so
    /// importing twice is harmless. Returns how many were added.
    pub fn import_qutebrowser(&mut self, path: &Path) -> rusqlite::Result<usize> {
        let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut rows = source
            .prepare("SELECT url, title, atime FROM History WHERE NOT redirect ORDER BY atime")?;
        let visits = rows
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let tx = self.conn.transaction()?;
        let mut added = 0;
        for (url, title, atime) in visits {
            if !is_recordable(&url) {
                continue;
            }
            let inserted = tx.execute(
                "INSERT INTO visits (url, title, atime)
                 SELECT ?1, ?2, ?3 WHERE NOT EXISTS (SELECT 1 FROM visits WHERE url = ?1 AND atime = ?3)",
                params![url, title, atime],
            )?;
            if inserted == 0 {
                continue;
            }
            added += 1;
            tx.execute(
                "INSERT INTO completion (url, title, last_visit, visits) VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT (url) DO UPDATE SET
                     title = CASE WHEN excluded.title = '' THEN title ELSE excluded.title END,
                     last_visit = MAX(last_visit, excluded.last_visit),
                     visits = visits + 1",
                params![url, title, atime],
            )?;
        }
        tx.commit()?;
        Ok(added)
    }

    /// Titles often arrive after the load finishes.
    pub fn set_title(&self, url: &str, title: &str) -> rusqlite::Result<()> {
        if title.is_empty() {
            return Ok(());
        }
        self.conn.execute(
            "UPDATE completion SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    /// Entries whose URL or title contains every word, case-insensitively,
    /// most recent first.
    pub fn search(&self, pattern: &str, limit: usize) -> rusqlite::Result<Vec<HistoryEntry>> {
        let words: Vec<String> = pattern
            .split_whitespace()
            .map(|w| format!("%{}%", escape_like(w)))
            .collect();
        let mut sql = String::from("SELECT url, title, last_visit FROM completion WHERE 1");
        for i in 1..=words.len() {
            sql.push_str(&format!(
                " AND (url LIKE ?{i} ESCAPE '\\' OR title LIKE ?{i} ESCAPE '\\')"
            ));
        }
        sql.push_str(&format!(" ORDER BY last_visit DESC LIMIT {limit}"));
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params_from_iter(words.iter()), |row| {
            Ok(HistoryEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                last_visit: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "DELETE FROM visits; DELETE FROM completion; DELETE FROM favicons; VACUUM;",
        )
    }

    /// Remember `host`'s icon (a `data:` URL), replacing an older one.
    pub fn set_favicon(&self, host: &str, icon: &str, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO favicons (host, icon, updated) VALUES (?1, ?2, ?3)
             ON CONFLICT (host) DO UPDATE SET icon = excluded.icon, updated = excluded.updated",
            params![host, icon, now],
        )?;
        Ok(())
    }

    /// Every remembered icon, by host.
    pub fn favicons(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let mut rows = self.conn.prepare("SELECT host, icon FROM favicons")?;
        let icons = rows.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        icons.collect()
    }

    /// Forget every visit to `url`. Returns whether there were any.
    pub fn delete_url(&self, url: &str) -> rusqlite::Result<bool> {
        let visits = self
            .conn
            .execute("DELETE FROM visits WHERE url = ?1", [url])?;
        let entries = self
            .conn
            .execute("DELETE FROM completion WHERE url = ?1", [url])?;
        Ok(visits + entries > 0)
    }

    pub fn visit_count(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM visits", [], |row| row.get(0))
    }
}

fn escape_like(word: &str) -> String {
    word.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicons_are_kept_per_host_and_cleared_with_history() {
        let h = History::open_in_memory().unwrap();
        h.set_favicon("example.com", "data:image/png;base64,AAAA", 1)
            .unwrap();
        h.set_favicon("example.com", "data:image/png;base64,BBBB", 2)
            .unwrap();
        h.set_favicon("other.org", "data:image/png;base64,CCCC", 2)
            .unwrap();
        let mut icons = h.favicons().unwrap();
        icons.sort();
        assert_eq!(
            icons,
            [
                (
                    "example.com".to_string(),
                    "data:image/png;base64,BBBB".to_string()
                ),
                (
                    "other.org".to_string(),
                    "data:image/png;base64,CCCC".to_string()
                ),
            ]
        );
        h.clear().unwrap();
        assert!(h.favicons().unwrap().is_empty());
    }

    #[test]
    fn a_version_1_database_gets_the_favicons_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE visits (url TEXT NOT NULL, title TEXT NOT NULL, atime INTEGER NOT NULL);
             CREATE TABLE completion (url TEXT PRIMARY KEY, title TEXT NOT NULL, last_visit INTEGER NOT NULL, visits INTEGER NOT NULL);
             PRAGMA user_version = 1;",
        )
        .unwrap();
        let h = History::init(conn).unwrap();
        h.set_favicon("a.example", "data:image/png;base64,AAAA", 1)
            .unwrap();
        assert_eq!(h.favicons().unwrap().len(), 1);
    }

    #[test]
    fn imports_qutebrowser_history_once() {
        let path =
            std::env::temp_dir().join(format!("rt-qb-history-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let qb = Connection::open(&path).unwrap();
            qb.execute_batch(
                "CREATE TABLE History (url TEXT, title TEXT, atime INTEGER, redirect BOOLEAN);
                 INSERT INTO History VALUES ('https://rust-lang.org/', 'Rust', 100, 0);
                 INSERT INTO History VALUES ('https://rust-lang.org/', 'Rust', 200, 0);
                 INSERT INTO History VALUES ('https://t.co/x', '', 150, 1);
                 INSERT INTO History VALUES ('about:blank', '', 160, 0);",
            )
            .unwrap();
        }
        let mut history = History::open_in_memory().unwrap();
        assert_eq!(history.import_qutebrowser(&path).unwrap(), 2);
        assert_eq!(
            history.import_qutebrowser(&path).unwrap(),
            0,
            "a second import adds nothing"
        );
        let found = history.search("rust", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].title.as_str(), found[0].last_visit),
            ("Rust", 200)
        );
        assert!(
            history.search("t.co", 10).unwrap().is_empty(),
            "redirects are skipped"
        );
        std::fs::remove_file(&path).unwrap();
    }

    fn urls(entries: &[HistoryEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.url.as_str()).collect()
    }

    #[test]
    fn records_and_searches_recent_first() {
        let h = History::open_in_memory().unwrap();
        h.add_visit("https://rust-lang.org/", "Rust", 10).unwrap();
        h.add_visit("https://docs.rs/serde", "serde - Rust", 20)
            .unwrap();
        h.add_visit("https://example.com/", "Example", 30).unwrap();
        h.add_visit("https://rust-lang.org/", "", 40).unwrap();

        let all = h.search("", 10).unwrap();
        assert_eq!(
            urls(&all),
            [
                "https://rust-lang.org/",
                "https://example.com/",
                "https://docs.rs/serde"
            ]
        );
        // An empty title on a later visit keeps the earlier one.
        assert_eq!(all[0].title, "Rust");
        assert_eq!(h.visit_count().unwrap(), 4);
    }

    #[test]
    fn delete_url_forgets_one_page() {
        let h = History::open_in_memory().unwrap();
        h.add_visit("https://a.org/", "A", 1).unwrap();
        h.add_visit("https://a.org/", "A", 2).unwrap();
        h.add_visit("https://b.org/", "B", 3).unwrap();
        assert!(h.delete_url("https://a.org/").unwrap());
        assert_eq!(urls(&h.search("", 10).unwrap()), ["https://b.org/"]);
        assert_eq!(h.visit_count().unwrap(), 1);
        assert!(!h.delete_url("https://a.org/").unwrap());
    }

    #[test]
    fn every_word_must_match_url_or_title() {
        let h = History::open_in_memory().unwrap();
        h.add_visit("https://docs.rs/serde", "serde - Rust", 1)
            .unwrap();
        h.add_visit("https://serde.rs/", "Serde overview", 2)
            .unwrap();
        assert_eq!(
            urls(&h.search("RUST serde", 10).unwrap()),
            ["https://docs.rs/serde"]
        );
        assert_eq!(
            urls(&h.search("overview serde", 10).unwrap()),
            ["https://serde.rs/"]
        );
        assert!(h.search("serde python", 10).unwrap().is_empty());
        assert_eq!(h.search("serde", 1).unwrap().len(), 1);
    }

    #[test]
    fn like_wildcards_are_literal() {
        let h = History::open_in_memory().unwrap();
        h.add_visit("https://a.org/100%", "", 1).unwrap();
        h.add_visit("https://a.org/1000", "", 2).unwrap();
        assert_eq!(urls(&h.search("100%", 10).unwrap()), ["https://a.org/100%"]);
        assert!(h.search("a_org", 10).unwrap().is_empty());
    }

    #[test]
    fn skips_internal_urls_and_updates_titles() {
        let h = History::open_in_memory().unwrap();
        h.add_visit("about:blank", "", 1).unwrap();
        h.add_visit("data:text/html,x", "", 1).unwrap();
        h.add_visit("https://x.org/", "", 2).unwrap();
        h.set_title("https://x.org/", "X").unwrap();
        let all = h.search("", 10).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "X");
        h.clear().unwrap();
        assert!(h.search("", 10).unwrap().is_empty());
    }
}
