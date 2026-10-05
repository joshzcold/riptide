//! Where configuration and browser data live on each platform.
//!
//! | | Config | Data |
//! |---|---|---|
//! | Linux | `$XDG_CONFIG_HOME` or `~/.config` | `$XDG_DATA_HOME` or `~/.local/share` |
//! | macOS | `$XDG_CONFIG_HOME` or `~/.config` | `$XDG_DATA_HOME` or `~/Library/Application Support` |
//! | Windows | `$XDG_CONFIG_HOME` or `%APPDATA%\…\config` | `$XDG_DATA_HOME` or `%LOCALAPPDATA%\…\data` |
//!
//! Every directory is suffixed with `riptide`. macOS follows the
//! developer-tool convention (Neovim, WezTerm, Zed) of a `~/.config` dotfile
//! directory, and keeps the large Chromium profile in Application Support.
//! `--basedir DIR` puts everything under `DIR/config` and `DIR/data`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APP: &str = "riptide";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOs => "macos",
            Platform::Windows => "windows",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn resolve(basedir: Option<&Path>) -> Result<Self, String> {
        Self::resolve_with(Platform::current(), basedir, &|key| std::env::var_os(key))
    }

    /// `env` is injectable so every platform's rules are testable anywhere.
    pub fn resolve_with(
        platform: Platform,
        basedir: Option<&Path>,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Self, String> {
        if let Some(base) = basedir {
            return Ok(Self {
                config_dir: base.join("config"),
                data_dir: base.join("data"),
            });
        }
        // The XDG spec says relative values must be ignored.
        let absolute = |key: &str| env(key).map(PathBuf::from).filter(|p| p.is_absolute());
        let home = || {
            let key = if platform == Platform::Windows {
                "USERPROFILE"
            } else {
                "HOME"
            };
            absolute(key).ok_or_else(|| format!("${key} is not set; pass --basedir"))
        };

        let config_dir = match absolute("XDG_CONFIG_HOME") {
            Some(dir) => dir.join(APP),
            None => match platform {
                Platform::Linux | Platform::MacOs => home()?.join(".config").join(APP),
                Platform::Windows => match absolute("APPDATA") {
                    Some(dir) => dir.join(APP).join("config"),
                    None => home()?
                        .join("AppData")
                        .join("Roaming")
                        .join(APP)
                        .join("config"),
                },
            },
        };
        let data_dir = match absolute("XDG_DATA_HOME") {
            Some(dir) => dir.join(APP),
            None => match platform {
                Platform::Linux => home()?.join(".local").join("share").join(APP),
                Platform::MacOs => home()?
                    .join("Library")
                    .join("Application Support")
                    .join(APP),
                Platform::Windows => match absolute("LOCALAPPDATA") {
                    Some(dir) => dir.join(APP).join("data"),
                    None => home()?.join("AppData").join("Local").join(APP).join("data"),
                },
            },
        };
        Ok(Self {
            config_dir,
            data_dir,
        })
    }

    pub fn config_toml(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn config_lua(&self) -> PathBuf {
        self.config_dir.join("config.lua")
    }

    /// Written by `:set`/`:bind`; never edited by hand.
    pub fn autoconfig(&self) -> PathBuf {
        self.config_dir.join("autoconfig.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn resolve(platform: Platform, vars: &[(&str, &str)]) -> Result<Paths, String> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        Paths::resolve_with(platform, None, &|k| vars.get(k).cloned())
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    // Unix-style absolute paths aren't absolute on Windows.
    #[cfg(not(windows))]
    #[test]
    fn linux_defaults_and_xdg() {
        let paths = resolve(Platform::Linux, &[("HOME", "/home/u")]).unwrap();
        assert_eq!(paths.config_dir, p("/home/u/.config/riptide"));
        assert_eq!(paths.data_dir, p("/home/u/.local/share/riptide"));
        let paths = resolve(
            Platform::Linux,
            &[
                ("HOME", "/home/u"),
                ("XDG_CONFIG_HOME", "/cfg"),
                ("XDG_DATA_HOME", "/dat"),
            ],
        )
        .unwrap();
        assert_eq!(paths.config_dir, p("/cfg/riptide"));
        assert_eq!(paths.data_dir, p("/dat/riptide"));
    }

    // Unix-style absolute paths aren't absolute on Windows.
    #[cfg(not(windows))]
    #[test]
    fn relative_xdg_values_are_ignored() {
        let paths = resolve(
            Platform::Linux,
            &[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "rel")],
        )
        .unwrap();
        assert_eq!(paths.config_dir, p("/home/u/.config/riptide"));
    }

    // Unix-style absolute paths aren't absolute on Windows.
    #[cfg(not(windows))]
    #[test]
    fn macos_uses_dotconfig_and_application_support() {
        let paths = resolve(Platform::MacOs, &[("HOME", "/Users/u")]).unwrap();
        assert_eq!(paths.config_dir, p("/Users/u/.config/riptide"));
        assert_eq!(
            paths.data_dir,
            p("/Users/u/Library/Application Support/riptide")
        );
    }

    // Path joining uses the host separator, so only check this where it matches.
    #[cfg(windows)]
    #[test]
    fn windows_uses_appdata() {
        let paths = resolve(
            Platform::Windows,
            &[
                ("USERPROFILE", r"C:\Users\u"),
                ("APPDATA", r"C:\Users\u\AppData\Roaming"),
                ("LOCALAPPDATA", r"C:\Users\u\AppData\Local"),
            ],
        )
        .unwrap();
        assert_eq!(
            paths.config_dir,
            p(r"C:\Users\u\AppData\Roaming\riptide\config")
        );
        assert_eq!(paths.data_dir, p(r"C:\Users\u\AppData\Local\riptide\data"));
    }

    // Unix-style absolute paths stand in for Windows ones on other hosts.
    #[cfg(not(windows))]
    #[test]
    fn windows_layout() {
        let paths = resolve(
            Platform::Windows,
            &[("APPDATA", "/roaming"), ("LOCALAPPDATA", "/local")],
        )
        .unwrap();
        assert_eq!(paths.config_dir, p("/roaming/riptide/config"));
        assert_eq!(paths.data_dir, p("/local/riptide/data"));
    }

    #[test]
    fn basedir_overrides_everything() {
        let paths =
            Paths::resolve_with(Platform::Linux, Some(Path::new("/tmp/b")), &|_| None).unwrap();
        assert_eq!(paths.config_dir, p("/tmp/b/config"));
        assert_eq!(paths.data_dir, p("/tmp/b/data"));
    }

    #[test]
    fn missing_home_is_an_error() {
        assert!(
            resolve(Platform::Linux, &[])
                .unwrap_err()
                .contains("--basedir")
        );
    }
}
