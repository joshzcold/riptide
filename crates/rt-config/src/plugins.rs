//! Plugins: what each one asks to be allowed to do (`riptide-plugin.toml`),
//! what you approved, and the lockfile that keeps both.
//!
//! A plugin is a folder (or git repository) with `lua/<name>/init.lua`,
//! optional `plugin/*.lua` run when it loads, and `riptide-plugin.toml`:
//!
//! ```toml
//! name = "reading-list"
//! description = "Save pages to read later"
//! [permissions]
//! spawn = true                      # run programs
//! network = ["api.example.com"]     # rt.fetch to these hosts (later)
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What a plugin may do beyond the safe default (events, keys, commands,
/// timers, messages, its own store, opening URLs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    /// Run programs (`rt.spawn`).
    pub spawn: bool,
    /// Read and write files (Lua's `io` and `os`).
    pub files: bool,
    /// Run any riptide command (`rt.run`), which includes `:spawn`.
    pub commands: bool,
    /// Read and change settings (`rt.get`, `rt.set`, `c`).
    pub settings: bool,
    /// Read and write the clipboard.
    pub clipboard: bool,
    /// See every key pressed.
    pub keys: bool,
    /// Hosts it may fetch from.
    pub network: Vec<String>,
    /// Sites whose pages it may script or style.
    pub pages: Vec<String>,
    /// Hosts its own pages may embed in iframes.
    pub frames: Vec<String>,
}

impl Permissions {
    /// In plain words, one line each, for approving them.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut flag = |on: bool, text: &str| {
            if on {
                out.push(text.to_string());
            }
        };
        flag(self.spawn, "run programs on your computer");
        flag(self.files, "read and write your files");
        flag(
            self.commands,
            "run any riptide command, including running programs",
        );
        flag(self.settings, "read and change your settings");
        flag(self.clipboard, "read and write the clipboard");
        flag(self.keys, "see every key you press");
        let mut list = |hosts: &[String], text: &str| {
            if !hosts.is_empty() {
                out.push(format!("{text}: {}", hosts.join(", ")));
            }
        };
        list(&self.network, "connect to");
        list(&self.pages, "read and change pages on");
        list(&self.frames, "show these sites in its pages");
        out
    }

    /// What `self` asks for that `approved` doesn't cover.
    pub fn beyond(&self, approved: &Permissions) -> Permissions {
        let more = |wanted: &[String], have: &[String]| -> Vec<String> {
            wanted
                .iter()
                .filter(|h| !have.contains(h))
                .cloned()
                .collect()
        };
        Permissions {
            spawn: self.spawn && !approved.spawn,
            files: self.files && !approved.files,
            commands: self.commands && !approved.commands,
            settings: self.settings && !approved.settings,
            clipboard: self.clipboard && !approved.clipboard,
            keys: self.keys && !approved.keys,
            network: more(&self.network, &approved.network),
            pages: more(&self.pages, &approved.pages),
            frames: more(&self.frames, &approved.frames),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Permissions::default()
    }
}

/// `riptide-plugin.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Manifest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Permissions,
}

impl Manifest {
    /// The manifest in `dir`; a plugin without one asks for nothing.
    pub fn read(dir: &Path) -> Result<Manifest, String> {
        let path = dir.join("riptide-plugin.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// What the lockfile keeps for a plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Locked {
    /// Where it came from: a git URL, or empty for a local folder.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub src: String,
    /// The commit it's pinned to.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// The permissions you approved.
    pub approved: Permissions,
}

/// `rt-pack-lock.json` in the config folder, so it can live in dotfiles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lockfile {
    pub plugins: BTreeMap<String, Locked>,
}

impl Lockfile {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("rt-pack-lock.json")
    }

    pub fn load(config_dir: &Path) -> Result<Lockfile, String> {
        let path = Self::path(config_dir);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Lockfile::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<(), String> {
        let path = Self::path(config_dir);
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// A plugin name: what `require` and the folder use.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A plugin's name from its git URL or folder: the last part, without
/// `.git`, a `riptide-` prefix or a `.nvim`-style suffix.
pub fn name_from(src: &str) -> String {
    let last = src
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(src);
    let last = last.trim_end_matches(".git");
    let last = last.strip_prefix("riptide-").unwrap_or(last);
    last.rsplit_once('.')
        .map_or(last, |(name, _)| name)
        .to_string()
}

/// Installing and pinning plugins with git, run as a program. These block,
/// so the browser calls them off its UI thread.
pub mod git {
    use std::path::Path;
    use std::process::Command;

    fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new("git");
        command
            .args(args)
            // Never wait for a password: a private repository fails instead.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "");
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        let output = command.output().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "git isn't installed".to_string(),
            _ => format!("git: {e}"),
        })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(error
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("git failed")
                .trim()
                .to_string())
        }
    }

    fn check_rev(rev: &str) -> Result<(), String> {
        if rev.is_empty() || rev.starts_with('-') || rev.contains(char::is_whitespace) {
            return Err(format!("{rev:?} isn't a version (a tag, branch or commit)"));
        }
        Ok(())
    }

    /// The commit checked out in `dir`.
    pub fn head(dir: &Path) -> Result<String, String> {
        git(Some(dir), &["rev-parse", "HEAD"])
    }

    /// Check out `rev` (a tag, branch or commit) in `dir`, fetching if it
    /// isn't there yet; returns the commit.
    pub fn checkout(dir: &Path, rev: &str) -> Result<String, String> {
        check_rev(rev)?;
        let resolve = |rev: &str| {
            git(
                Some(dir),
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{rev}^{{commit}}"),
                ],
            )
        };
        let commit = match resolve(rev).or_else(|_| resolve(&format!("origin/{rev}"))) {
            Ok(commit) => commit,
            Err(_) => {
                git(Some(dir), &["fetch", "--quiet", "--tags", "origin"])?;
                resolve(rev)
                    .or_else(|_| resolve(&format!("origin/{rev}")))
                    .map_err(|_| format!("no version {rev:?} in the repository"))?
            }
        };
        git(Some(dir), &["checkout", "--quiet", "--detach", &commit])?;
        Ok(commit)
    }

    /// Clone `src` into `dir` and check out `rev`, or the default branch;
    /// returns the commit. A failed clone leaves nothing behind.
    pub fn install(src: &str, dir: &Path, rev: Option<&str>) -> Result<String, String> {
        if src.starts_with('-') {
            return Err(format!("{src:?} isn't a git URL"));
        }
        let parent = dir.parent().ok_or("no folder to install into")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let partial = dir.with_extension("part");
        let _ = std::fs::remove_dir_all(&partial);
        let partial_text = partial.to_string_lossy().into_owned();
        let cloned =
            git(None, &["clone", "--quiet", "--", src, &partial_text]).and_then(|_| match rev {
                Some(rev) => checkout(&partial, rev),
                None => head(&partial),
            });
        match cloned {
            Ok(commit) => {
                std::fs::rename(&partial, dir).map_err(|e| e.to_string())?;
                Ok(commit)
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&partial);
                Err(e)
            }
        }
    }

    /// The commits after `from` up to `to`, newest first, as one-line logs.
    pub fn log(dir: &Path, from: &str, to: &str) -> Result<Vec<String>, String> {
        check_rev(from)?;
        check_rev(to)?;
        let range = format!("{from}..{to}");
        Ok(
            git(Some(dir), &["log", "--oneline", "--no-decorate", &range])?
                .lines()
                .map(str::to_string)
                .collect(),
        )
    }

    /// Fetch, and return the default branch's newest commit.
    pub fn fetch_latest(dir: &Path) -> Result<String, String> {
        git(Some(dir), &["fetch", "--quiet", "--tags", "origin"])?;
        git(Some(dir), &["rev-parse", "--verify", "origin/HEAD"])
            .or_else(|_| git(Some(dir), &["rev-parse", "--verify", "FETCH_HEAD"]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_urls_and_folders() {
        assert_eq!(
            name_from("https://github.com/me/riptide-reading-list.git"),
            "reading-list"
        );
        assert_eq!(name_from("git@github.com:me/tabtree.nvim"), "tabtree");
        assert_eq!(name_from("/home/me/code/notes/"), "notes");
        assert!(valid_name("reading-list"));
        assert!(!valid_name("../x"));
    }

    #[test]
    fn manifests_describe_and_compare_permissions() {
        let dir = std::env::temp_dir().join(format!("rt-plugin-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Manifest::read(&dir).unwrap(), Manifest::default());
        std::fs::write(
            dir.join("riptide-plugin.toml"),
            "name = \"x\"\n[permissions]\nspawn = true\nnetwork = [\"a.example\", \"b.example\"]\n",
        )
        .unwrap();
        let wanted = Manifest::read(&dir).unwrap().permissions;
        assert_eq!(
            wanted.describe(),
            [
                "run programs on your computer",
                "connect to: a.example, b.example"
            ]
        );
        let approved = Permissions {
            spawn: true,
            network: vec!["a.example".into()],
            ..Permissions::default()
        };
        assert_eq!(
            wanted.beyond(&approved),
            Permissions {
                network: vec!["b.example".into()],
                ..Permissions::default()
            }
        );
        assert!(wanted.beyond(&wanted).is_empty());
        std::fs::write(
            dir.join("riptide-plugin.toml"),
            "[permissions]\nroot = true\n",
        )
        .unwrap();
        assert!(Manifest::read(&dir).unwrap_err().contains("root"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A repository with two commits, the first tagged v1.
    fn repo(path: &Path) -> (String, String) {
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(path)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        std::fs::create_dir_all(path.join("lua/demo")).unwrap();
        run(&["init", "--quiet", "--initial-branch=main"]);
        std::fs::write(path.join("lua/demo/init.lua"), "return 1").unwrap();
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "one"]);
        run(&["tag", "v1"]);
        let first = run(&["rev-parse", "HEAD"]);
        std::fs::write(path.join("lua/demo/init.lua"), "return 2").unwrap();
        run(&["commit", "--quiet", "-am", "two"]);
        (first, run(&["rev-parse", "HEAD"]))
    }

    #[test]
    fn git_installs_pins_and_moves_plugins() {
        let base = std::env::temp_dir().join(format!("rt-plugin-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let source = base.join("source");
        let (first, second) = repo(&source);
        let src = source.to_string_lossy().into_owned();

        let pinned = base.join("pack/pinned");
        assert_eq!(git::install(&src, &pinned, Some("v1")).unwrap(), first);
        assert_eq!(
            std::fs::read_to_string(pinned.join("lua/demo/init.lua")).unwrap(),
            "return 1"
        );
        assert_eq!(git::checkout(&pinned, "main").unwrap(), second);
        assert_eq!(git::head(&pinned).unwrap(), second);
        assert_eq!(git::log(&pinned, &first, &second).unwrap().len(), 1);
        assert_eq!(git::fetch_latest(&pinned).unwrap(), second);

        let latest = base.join("pack/latest");
        assert_eq!(git::install(&src, &latest, None).unwrap(), second);
        // Bad input fails cleanly and leaves nothing behind.
        let missing = base.join("pack/missing");
        assert!(
            git::install(&src, &missing, Some("v9"))
                .unwrap_err()
                .contains("v9")
        );
        assert!(!missing.exists() && !missing.with_extension("part").exists());
        assert!(git::install("--upload-pack=x", &missing, None).is_err());
        assert!(git::checkout(&pinned, "--orphan").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_lockfile_round_trips() {
        let dir = std::env::temp_dir().join(format!("rt-plugin-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Lockfile::load(&dir).unwrap(), Lockfile::default());
        let mut lock = Lockfile::default();
        lock.plugins.insert(
            "notes".into(),
            Locked {
                src: "https://example.com/notes.git".into(),
                commit: "abc123".into(),
                approved: Permissions {
                    files: true,
                    ..Permissions::default()
                },
            },
        );
        lock.save(&dir).unwrap();
        assert_eq!(Lockfile::load(&dir).unwrap(), lock);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
