//! User themes: `themes/<name>.toml` in the config directory, or a base16
//! scheme (`.yaml`) or qutebrowser theme (`.py`) there (see `theme_import`).
//!
//! ```toml
//! [palette]
//! base = "#0a0c0f"     # status bar, headers
//! surface = "#181616"  # tab bar, completion, prompts
//! fg = "#c5c9c5"
//! accent = "#8ba4b0"
//! yellow = "#c4b28a"
//! red = "#c4746e"
//! green = "#8a9a7b"
//! blue = "#8ba4b0"
//!
//! [colors]             # optional, by colors.* name
//! "statusbar.insert.bg" = "#87a987"
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    palette: BTreeMap<String, String>,
    #[serde(default)]
    colors: BTreeMap<String, String>,
}

/// Read every theme in `dir` and make them available to `ui.theme`.
/// Returns the problems found, one per bad file.
pub fn load(dir: &Path) -> Vec<String> {
    let (themes, errors) = read(dir);
    rt_core::theme::set_user_themes(themes);
    errors
}

type Themes = BTreeMap<String, BTreeMap<&'static str, String>>;

fn read(dir: &Path) -> (Themes, Vec<String>) {
    let mut themes = BTreeMap::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (themes, errors);
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| ["toml", "yaml", "yml", "py"].iter().any(|x| e == *x))
        })
        .collect();
    paths.sort();
    for path in paths {
        let shown = path.display();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !valid_name(name) {
            errors.push(format!("{shown}: a theme name uses only a-z, 0-9, - and _"));
            continue;
        }
        if rt_core::theme::THEME_CHOICES.contains(&name) {
            errors.push(format!("{shown}: {name} is a built-in theme"));
            continue;
        }
        if themes.contains_key(name) {
            errors.push(format!("{shown}: another file already makes theme {name}"));
            continue;
        }
        let kind = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| match kind {
                "toml" => toml::from_str::<ThemeFile>(&text)
                    .map(|f| (f.palette, f.colors))
                    .map_err(|e| e.to_string()),
                "py" => crate::theme_import::qutebrowser(&text),
                _ => crate::theme_import::base16(&text),
            })
            .and_then(|(palette, colors)| rt_core::theme::user_theme(&palette, &colors));
        match parsed {
            Ok(tokens) => {
                themes.insert(name.to_string(), tokens);
            }
            Err(e) => errors.push(format!("{shown}: {e}")),
        }
    }
    (themes, errors)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: &str = r##"
[palette]
base = "#0a0c0f"
surface = "#181616"
fg = "#c5c9c5"
accent = "#8ba4b0"
yellow = "#c4b28a"
red = "#c4746e"
green = "#8a9a7b"
blue = "#8ba4b0"
"##;

    #[test]
    fn reads_good_themes_and_reports_bad_ones() {
        let dir = std::env::temp_dir().join(format!("rt-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
        write(
            "ink.toml",
            &format!("{PALETTE}\n[colors]\n\"statusbar.insert.bg\" = \"#123456\"\n"),
        );
        write("Bad Name.toml", PALETTE);
        write("nord.toml", PALETTE);
        write("short.toml", "[palette]\nbase = \"#000000\"\n");
        write(
            "typo.toml",
            &format!("{PALETTE}\n[colors]\n\"statusbar.nope\" = \"#123456\"\n"),
        );
        write("notes.txt", "ignored");
        let (themes, errors) = read(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(themes.keys().collect::<Vec<_>>(), ["ink"]);
        let ink = &themes["ink"];
        assert_eq!(ink["statusbar-bg"], "#0a0c0f");
        assert_eq!(ink["statusbar-insert-bg"], "#123456");
        assert_eq!(ink.len(), rt_core::theme::TOKENS.len());
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("built-in")));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("palette.surface is missing"))
        );
        assert!(errors.iter().any(|e| e.contains("statusbar.nope")));
    }
}
