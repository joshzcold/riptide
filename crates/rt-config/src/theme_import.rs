//! Themes made for other programs, read from `themes/` like riptide's own:
//! base16 schemes (`.yaml`) and qutebrowser theme files (`.py`). Both are
//! read line by line for colors; no Python is run.

use std::collections::BTreeMap;

/// A theme's `[palette]` and `[colors]`, as in a riptide theme file.
pub type Parts = (BTreeMap<String, String>, BTreeMap<String, String>);

/// `#rrggbb` from `#rgb`, `#rrggbb` or a bare `rrggbb`, in lower case.
fn hex(value: &str) -> Option<String> {
    let digits = value.strip_prefix('#').unwrap_or(value);
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        6 => Some(format!("#{}", digits.to_ascii_lowercase())),
        3 => Some(format!(
            "#{}",
            digits
                .chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        )),
        _ => None,
    }
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .or_else(|| text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')))
        .unwrap_or(text)
}

/// A base16 scheme: `base00` to `base0F`, either at the top level or under
/// `palette:` (the newer tinted-theming layout).
pub fn base16(text: &str) -> Result<Parts, String> {
    let mut slots: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key.len() == 6 && key.starts_with("base0") {
            // Drop a trailing comment.
            let value = value.split(" #").next().unwrap_or(value);
            if let Some(color) = hex(unquote(value)) {
                slots.insert(key, color);
            }
        }
    }
    let slot = |n: &str| {
        slots
            .get(&format!("base0{n}"))
            .cloned()
            .ok_or_else(|| format!("base16: base0{n} is missing"))
    };
    let (bg, bg_light) = (slot("0")?, slot("1")?);
    // The darker of the two backgrounds is the bars', in light schemes too.
    let (base, surface) = if rt_core::theme::luminance(&bg) <= rt_core::theme::luminance(&bg_light)
    {
        (bg, bg_light)
    } else {
        (bg_light, bg)
    };
    let palette = BTreeMap::from([
        ("base".to_string(), base),
        ("surface".to_string(), surface),
        ("surface3".to_string(), slot("2")?),
        ("muted".to_string(), slot("4")?),
        ("fg".to_string(), slot("5")?),
        ("red".to_string(), slot("8")?),
        ("orange".to_string(), slot("9")?),
        ("yellow".to_string(), slot("a")?),
        ("green".to_string(), slot("b")?),
        ("blue".to_string(), slot("c")?),
        ("accent".to_string(), slot("d")?),
    ]);
    Ok((palette, BTreeMap::new()))
}

/// A qutebrowser theme (`config.py` style). Reads `c.colors.… = …` and
/// `config.set("colors.…", …)` lines, where the value is a string, a
/// variable set to a string (`base00 = "#…"`), or a dict entry
/// (`palette['bg']`). The palette is guessed from the main bar colors and
/// every `colors.*` riptide also has is kept as is.
pub fn qutebrowser(text: &str) -> Result<Parts, String> {
    // Every `name = "…"` and `'key': '…'` seen, for looking values up.
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut colors: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        if line.starts_with('#') {
            continue;
        }
        for entry in line.split(',') {
            if let Some((key, value)) = entry.split_once(':') {
                let key = unquote(key.trim_start_matches('{').trim());
                let value = unquote(value.trim_end_matches('}').trim());
                if !key.is_empty() && value.starts_with('#') {
                    names.insert(key.to_string(), value.to_string());
                }
            }
        }
        let (target, value) = if let Some(args) = line
            .strip_prefix("config.set(")
            .and_then(|a| a.strip_suffix(')'))
        {
            match args.split_once(',') {
                Some((name, value)) => (unquote(name).to_string(), value.trim()),
                None => continue,
            }
        } else if let Some((name, value)) = line.split_once('=') {
            let name = name.trim();
            let value = value.trim();
            match name.strip_prefix("c.") {
                Some(setting) => (setting.to_string(), value),
                None => {
                    if !name.contains(['.', '[', ' ']) {
                        names.insert(name.to_string(), unquote(value).to_string());
                    }
                    continue;
                }
            }
        } else {
            continue;
        };
        let Some(setting) = target.strip_prefix("colors.") else {
            continue;
        };
        let value = resolve(value, &names);
        if let Some(value) = value.filter(|v| rt_core::theme::is_color(v)) {
            colors.insert(setting.to_string(), value);
        }
    }
    if colors.is_empty() {
        return Err("qutebrowser: no c.colors settings found".into());
    }
    let pick = |settings: &[&str]| {
        settings
            .iter()
            .find_map(|s| colors.get(*s).and_then(|v| hex(v)))
    };
    let fallback = rt_core::theme::builtin_palette("riptide").unwrap_or_default();
    let guesses: [(&str, &[&str]); 8] = [
        ("base", &["statusbar.normal.bg", "tabs.bar.bg"]),
        (
            "surface",
            &["completion.odd.bg", "completion.even.bg", "tabs.bar.bg"],
        ),
        (
            "fg",
            &["statusbar.normal.fg", "completion.fg", "tabs.odd.fg"],
        ),
        (
            "accent",
            &[
                "statusbar.url.success.https.fg",
                "tabs.indicator.start",
                "completion.item.selected.bg",
            ],
        ),
        ("yellow", &["hints.bg", "completion.match.fg"]),
        ("red", &["messages.error.bg", "statusbar.url.error.fg"]),
        ("green", &["statusbar.insert.bg"]),
        ("blue", &["statusbar.passthrough.bg"]),
    ];
    let palette = guesses
        .iter()
        .map(|(key, settings)| {
            let color = pick(settings)
                .unwrap_or_else(|| fallback.get(key).copied().unwrap_or("#000000").to_string());
            (key.to_string(), color)
        })
        .collect();
    // Only the colors riptide has; qutebrowser's even rows and tabs stand in
    // for riptide's odd ones when those aren't set.
    let known = |name: &str| {
        rt_core::theme::TOKENS
            .iter()
            .any(|(_, s)| s.strip_prefix("colors.") == Some(name))
    };
    let mut kept = BTreeMap::new();
    for (name, value) in &colors {
        let name = if known(name) {
            name.clone()
        } else {
            let odd = name.replace(".even.", ".odd.");
            if !known(&odd) || colors.contains_key(&odd) {
                continue;
            }
            odd
        };
        kept.insert(name, value.clone());
    }
    Ok((palette, kept))
}

/// A Python value: a string, a name set earlier, or `dict['key']`.
fn resolve(value: &str, names: &BTreeMap<String, String>) -> Option<String> {
    let value = value.trim();
    if value.starts_with(['"', '\'']) {
        return Some(unquote(value).to_string());
    }
    let key = match value.split_once('[') {
        Some((_, rest)) => unquote(rest.trim_end_matches(']')),
        None => value,
    };
    names.get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base16_schemes_in_both_layouts() {
        let flat = "scheme: \"Ocean\"\nauthor: \"x\"\nbase00: \"2b303b\"\nbase01: \"343d46\"\n\
                    base02: \"4f5b66\"\nbase03: \"65737e\"\nbase04: \"a7adba\"\nbase05: \"c0c5ce\"\n\
                    base06: \"dfe1e8\"\nbase07: \"eff1f5\"\nbase08: \"bf616a\"\nbase09: \"d08770\"\n\
                    base0A: \"ebcb8b\"\nbase0B: \"a3be8c\"\nbase0C: \"96b5b4\"\nbase0D: \"8fa1b3\"\n\
                    base0E: \"b48ead\"\nbase0F: \"ab7967\"\n";
        let (palette, colors) = base16(flat).unwrap();
        assert_eq!(palette["base"], "#2b303b");
        assert_eq!(palette["surface"], "#343d46");
        assert_eq!(palette["accent"], "#8fa1b3");
        assert_eq!(palette["yellow"], "#ebcb8b");
        assert!(colors.is_empty());
        assert!(rt_core::theme::user_theme(&palette, &colors).is_ok());

        // tinted-theming's layout, a light scheme: the darker background is the bars'.
        let tinted = "system: \"base16\"\nname: \"Light\"\npalette:\n  base00: \"#fafafa\" # bg\n  base01: \"#e0e0e0\"\n\
                      \x20 base02: \"#d0d0d0\"\n  base03: \"#a0a0a0\"\n  base04: \"#505050\"\n  base05: \"#202020\"\n\
                      \x20 base06: \"#101010\"\n  base07: \"#000000\"\n  base08: \"#c00\"\n  base09: \"#c60\"\n\
                      \x20 base0A: \"#a80\"\n  base0B: \"#080\"\n  base0C: \"#088\"\n  base0D: \"#048\"\n\
                      \x20 base0E: \"#808\"\n  base0F: \"#840\"\n";
        let (palette, _) = base16(tinted).unwrap();
        assert_eq!(palette["base"], "#e0e0e0");
        assert_eq!(palette["surface"], "#fafafa");
        assert_eq!(palette["red"], "#cc0000");
        assert!(
            base16("base00: \"000000\"\n")
                .unwrap_err()
                .contains("base01")
        );
    }

    #[test]
    fn qutebrowser_themes_with_strings_variables_and_dicts() {
        let text = r##"
# A theme.
base00 = "#1d2021"
palette = {
    'bg': '#282828', 'fg': '#ebdbb2',
    "red": "#fb4934",
}
c.colors.statusbar.normal.bg = base00
c.colors.statusbar.normal.fg = palette['fg']
c.colors.completion.even.bg = palette["bg"]
c.colors.messages.error.bg = palette['red']  # errors
c.colors.hints.bg = "qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #fabd2f, stop:1 #d79921)"
config.set("colors.statusbar.insert.bg", "#b8bb26")
c.colors.downloads.bar.bg = "#000000"
c.fonts.default_size = "10pt"
"##;
        let (palette, colors) = qutebrowser(text).unwrap();
        assert_eq!(palette["base"], "#1d2021");
        assert_eq!(palette["fg"], "#ebdbb2");
        assert_eq!(palette["surface"], "#282828");
        assert_eq!(palette["red"], "#fb4934");
        assert_eq!(palette["green"], "#b8bb26");
        // Not given: riptide's own.
        assert_eq!(palette["blue"], "#24418f");
        assert_eq!(colors["statusbar.normal.bg"], "#1d2021");
        assert_eq!(colors["completion.odd.bg"], "#282828");
        assert_eq!(colors["statusbar.insert.bg"], "#b8bb26");
        // A Qt gradient isn't CSS, and riptide has no download bar.
        assert!(!colors.contains_key("hints.bg"));
        assert!(!colors.contains_key("downloads.bar.bg"));
        assert!(rt_core::theme::user_theme(&palette, &colors).is_ok());
        assert!(qutebrowser("c.fonts.default_size = '10pt'").is_err());
    }
}
