//! Whether Chromium's sandbox can run here. On Linux it needs either a
//! setuid-root `chrome-sandbox` next to the executable or unprivileged user
//! namespaces; without both, Chromium aborts with "No usable sandbox!".

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sandbox {
    /// The sandbox is on, using the named mechanism.
    On(&'static str),
    /// The sandbox is off, for the given reason.
    Off(String),
}

impl Sandbox {
    pub fn is_on(&self) -> bool {
        matches!(self, Sandbox::On(_))
    }

    pub fn describe(&self) -> String {
        match self {
            Sandbox::On(how) => format!("on ({how})"),
            Sandbox::Off(why) => format!("off: {why}"),
        }
    }
}

/// What the Linux checks look at, so tests can supply their own.
pub struct LinuxProbe {
    /// `chrome-sandbox` exists, is owned by root and has the setuid bit.
    pub suid_helper: bool,
    /// `/proc/sys/kernel/unprivileged_userns_clone`, absent on most kernels.
    pub userns_clone: Option<String>,
    /// `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` (Ubuntu 23.10+).
    pub apparmor_restrict: Option<String>,
    /// `/proc/sys/user/max_user_namespaces`.
    pub max_user_namespaces: Option<String>,
}

impl LinuxProbe {
    #[cfg(unix)]
    pub fn read(exe_dir: &Path) -> Self {
        use std::os::unix::fs::MetadataExt;
        let sysctl = |p: &str| {
            std::fs::read_to_string(p)
                .ok()
                .map(|s| s.trim().to_string())
        };
        let suid_helper = std::fs::metadata(exe_dir.join("chrome-sandbox"))
            .is_ok_and(|m| m.uid() == 0 && m.mode() & 0o4000 != 0);
        LinuxProbe {
            suid_helper,
            userns_clone: sysctl("/proc/sys/kernel/unprivileged_userns_clone"),
            apparmor_restrict: sysctl("/proc/sys/kernel/apparmor_restrict_unprivileged_userns"),
            max_user_namespaces: sysctl("/proc/sys/user/max_user_namespaces"),
        }
    }

    pub fn decide(&self) -> Sandbox {
        if self.suid_helper {
            return Sandbox::On("setuid chrome-sandbox");
        }
        let blocked = if self.userns_clone.as_deref() == Some("0") {
            Some("kernel.unprivileged_userns_clone is 0")
        } else if self.apparmor_restrict.as_deref() == Some("1") {
            Some("AppArmor restricts unprivileged user namespaces")
        } else if self.max_user_namespaces.as_deref() == Some("0") {
            Some("user.max_user_namespaces is 0")
        } else {
            None
        };
        match blocked {
            None => Sandbox::On("user namespaces"),
            Some(why) => Sandbox::Off(format!(
                "{why} and chrome-sandbox is not setuid root; see the README's Sandbox section"
            )),
        }
    }
}

/// Decide for this platform. `disabled` is the `--no-sandbox` switch.
pub fn detect(exe_dir: &Path, disabled: bool) -> Sandbox {
    if disabled {
        return Sandbox::Off("--no-sandbox".into());
    }
    #[cfg(target_os = "linux")]
    {
        LinuxProbe::read(exe_dir).decide()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = exe_dir;
        Sandbox::Off("not supported on this platform yet".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(suid: bool, clone: Option<&str>, apparmor: Option<&str>) -> LinuxProbe {
        LinuxProbe {
            suid_helper: suid,
            userns_clone: clone.map(String::from),
            apparmor_restrict: apparmor.map(String::from),
            max_user_namespaces: Some("63000".into()),
        }
    }

    #[test]
    fn decisions() {
        assert_eq!(
            probe(false, None, None).decide(),
            Sandbox::On("user namespaces")
        );
        assert_eq!(
            probe(false, None, Some("0")).decide(),
            Sandbox::On("user namespaces")
        );
        assert!(!probe(false, None, Some("1")).decide().is_on());
        assert!(!probe(false, Some("0"), None).decide().is_on());
        assert_eq!(
            probe(true, Some("0"), Some("1")).decide(),
            Sandbox::On("setuid chrome-sandbox")
        );
        let mut p = probe(false, None, None);
        p.max_user_namespaces = Some("0".into());
        assert!(!p.decide().is_on());
    }

    #[test]
    fn switch_wins() {
        assert_eq!(
            detect(Path::new("/nonexistent"), true),
            Sandbox::Off("--no-sandbox".into())
        );
    }
}
