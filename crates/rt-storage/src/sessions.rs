//! Sessions: the open tabs, saved as TOML in `<data>/sessions/<name>.toml`.
//! Each tab keeps its page, its back/forward history and how far it was scrolled.

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
    /// The pages before this one, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<PageState>,
    /// The pages after this one, nearest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<PageState>,
    /// How far down the page was scrolled, in CSS pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll: Option<u32>,
}

/// A page in a tab's back/forward history.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    #[serde(default)]
    pub title: String,
}

impl WindowState {
    /// A window's tabs, with `current` the index of the current one. Tabs
    /// without a URL yet (a popup still opening) are left out, and `active`
    /// still points at the same tab. `None` if none are left.
    pub fn from_tabs(tabs: impl IntoIterator<Item = TabState>, current: usize) -> Option<Self> {
        let mut active = 0;
        let mut kept = Vec::new();
        for (i, tab) in tabs.into_iter().enumerate() {
            if tab.url.is_empty() {
                continue;
            }
            if i <= current {
                active = kept.len();
            }
            kept.push(tab);
        }
        (!kept.is_empty()).then_some(Self { active, tabs: kept })
    }
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

    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        let (from, to) = (self.path(from)?, self.path(to)?);
        std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
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

    fn tab((url, title, pinned): (&str, &str, bool)) -> TabState {
        TabState {
            url: url.into(),
            title: title.into(),
            pinned,
            ..Default::default()
        }
    }

    fn page(url: &str) -> PageState {
        PageState {
            url: url.into(),
            title: url.to_uppercase(),
        }
    }

    #[test]
    fn from_tabs_keeps_the_current_tab_when_earlier_ones_are_dropped() {
        let tabs = [
            ("a", "A", true),
            ("", "", false),
            ("c", "C", false),
            ("d", "D", false),
        ];
        let window = WindowState::from_tabs(tabs.map(tab), 2).unwrap();
        let urls: Vec<&str> = window.tabs.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, ["a", "c", "d"]);
        assert_eq!(window.tabs[window.active].url, "c");
        assert!(window.tabs[0].pinned);
    }

    #[test]
    fn from_tabs_picks_the_nearest_earlier_tab_when_the_current_one_has_no_url() {
        let tabs = [("a", "A", false), ("b", "B", false), ("", "", false)];
        let window = WindowState::from_tabs(tabs.map(tab), 2).unwrap();
        assert_eq!(window.tabs[window.active].url, "b");
        assert_eq!(WindowState::from_tabs([("", "", false)].map(tab), 0), None);
        assert_eq!(
            WindowState::from_tabs([("", "", false), ("b", "B", false)].map(tab), 0)
                .unwrap()
                .active,
            0
        );
    }

    #[test]
    fn save_load_list_delete() {
        let dir = std::env::temp_dir().join(format!("rt-sessions-{}", std::process::id()));
        let sessions = Sessions::new(&dir);
        let session = Session {
            windows: vec![WindowState {
                active: 1,
                tabs: vec![
                    TabState {
                        url: "https://a.org/".into(),
                        title: "A".into(),
                        pinned: true,
                        ..Default::default()
                    },
                    TabState {
                        url: "https://b.org/".into(),
                        title: String::new(),
                        pinned: false,
                        back: vec![page("https://a.org/"), page("https://a.org/2")],
                        forward: vec![page("https://c.org/")],
                        scroll: Some(1200),
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
        let tab = &session.windows[0].tabs[0];
        assert!(!tab.pinned);
        assert!(tab.back.is_empty() && tab.forward.is_empty() && tab.scroll.is_none());
    }

    #[test]
    fn a_tab_without_history_saves_only_its_page() {
        let text = toml::to_string(&Session {
            windows: vec![WindowState {
                active: 0,
                tabs: vec![tab(("https://a.org/", "A", false))],
            }],
        })
        .unwrap();
        assert!(
            !text.contains("back") && !text.contains("forward") && !text.contains("scroll"),
            "{text}"
        );
    }

    #[test]
    fn rejects_path_like_names() {
        for bad in ["", "../x", "a/b", ".hidden", "a\\b"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        assert!(validate_name("my session_2.1").is_ok());
    }
}
