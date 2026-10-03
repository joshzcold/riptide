//! Finding userscripts, qutebrowser-style: a bare name is looked up in
//! `<config>/userscripts`, then `<data>/userscripts`, then `PATH`.

use std::path::{Path, PathBuf};

use crate::Paths;

pub fn dirs(paths: &Paths) -> [PathBuf; 2] {
    [
        paths.config_dir.join("userscripts"),
        paths.data_dir.join("userscripts"),
    ]
}

/// The program to run for `name`. Names with a path separator are used as
/// given (with `~` expanded); other names fall back to `PATH` lookup by the OS.
pub fn resolve(name: &str, paths: &Paths, home: Option<&Path>) -> PathBuf {
    if let (Some(rest), Some(home)) = (name.strip_prefix("~/"), home) {
        return home.join(rest);
    }
    if name.contains(['/', '\\']) {
        return PathBuf::from(name);
    }
    dirs(paths)
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_in_config_then_data_then_path() {
        let base = std::env::temp_dir().join(format!("hb-userscripts-{}", std::process::id()));
        let paths = Paths::resolve(Some(&base)).unwrap();
        let [config, data] = dirs(&paths);
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("both"), "").unwrap();
        std::fs::write(config.join("both"), "").unwrap();
        std::fs::write(data.join("data-only"), "").unwrap();

        assert_eq!(resolve("both", &paths, None), config.join("both"));
        assert_eq!(resolve("data-only", &paths, None), data.join("data-only"));
        assert_eq!(resolve("ls", &paths, None), PathBuf::from("ls"));
        assert_eq!(resolve("/bin/x", &paths, None), PathBuf::from("/bin/x"));
        assert_eq!(
            resolve("~/bin/x", &paths, Some(Path::new("/home/u"))),
            Path::new("/home/u").join("bin/x")
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
