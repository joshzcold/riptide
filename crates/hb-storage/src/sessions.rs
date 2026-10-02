//! Sessions: the open tabs, saved as TOML in `<data>/sessions/<name>.toml`.
//! CEF cannot restore back/forward history, so each tab keeps its current page.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub windows: Vec<WindowState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    /// Index of the focused tab.
    pub active: usize,
    pub tabs: Vec<TabState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabState {
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// Absent in sessions saved before pinned tabs existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
}

pub struct Sessions {
    dir: PathBuf,
}

/// Names become file names, so keep them to one safe path component.
pub fn validate_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Invalid session name {name:?}: use letters, digits, '-', '_', '.' or spaces"
        ))
    }
}

impl Sessions {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    fn path(&self, name: &str) -> Result<PathBuf, String> {
        validate_name(name)?;
        Ok(self.dir.join(format!("{name}.toml")))
    }

    pub fn exists(&self, name: &str) -> bool {
        self.path(name).is_ok_and(|p| p.exists())
    }

    pub fn save(&self, name: &str, session: &Session) -> Result<(), String> {
        let path = self.path(name)?;
        let text = toml::to_string(session).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn load(&self, name: &str) -> Result<Session, String> {
        let path = self.path(name)?;
        let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("Session {name:?} not found"),
            _ => format!("{}: {e}", path.display()),
        })?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn delete(&self, name: &str) -> Result<(), String> {
        let path = self.path(name)?;
        std::fs::remove_file(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("Session {name:?} not found"),
            _ => format!("{}: {e}", path.display()),
        })
    }

    /// Session names, sorted.
    pub fn list(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                name.strip_suffix(".toml").map(String::from)
            })
            .filter(|n| validate_name(n).is_ok())
            .collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_list_delete() {
        let dir = std::env::temp_dir().join(format!("hb-sessions-{}", std::process::id()));
        let sessions = Sessions::new(&dir);
        let session = Session {
            windows: vec![WindowState {
                active: 1,
                tabs: vec![
                    TabState {
                        url: "https://a.org/".into(),
                        title: "A".into(),
                        pinned: true,
                    },
                    TabState {
                        url: "https://b.org/".into(),
                        title: String::new(),
                        pinned: false,
                    },
                ],
            }],
        };
        sessions.save("work", &session).unwrap();
        sessions.save("default", &Session::default()).unwrap();
        assert_eq!(sessions.load("work").unwrap(), session);
        assert_eq!(sessions.list(), ["default", "work"]);
        assert!(sessions.exists("work"));
        sessions.delete("work").unwrap();
        assert!(sessions.load("work").unwrap_err().contains("not found"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn loads_sessions_saved_before_pinning_existed() {
        let old = "[[windows]]\nactive = 0\n[[windows.tabs]]\nurl = \"https://a.org/\"\n";
        let session: Session = toml::from_str(old).unwrap();
        assert!(!session.windows[0].tabs[0].pinned);
    }

    #[test]
    fn rejects_path_like_names() {
        for bad in ["", "../x", "a/b", ".hidden", "a\\b"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_name("my session_2.1").is_ok());
    }
}
