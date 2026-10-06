//! Themes: the colors of riptide's own bars, overlay, prompts and hint
//! labels. A theme is a small palette that every color token is derived
//! from; `colors.*` settings override single tokens. The UI pages read the
//! tokens as `--rt-<token>` CSS variables.

use std::collections::BTreeMap;

use crate::settings::Settings;

/// Every token, with the `colors.*` setting that overrides it.
pub const TOKENS: &[(&str, &str)] = &[
    ("statusbar-bg", "colors.statusbar.normal.bg"),
    ("statusbar-fg", "colors.statusbar.normal.fg"),
    ("statusbar-insert-bg", "colors.statusbar.insert.bg"),
    ("statusbar-insert-fg", "colors.statusbar.insert.fg"),
    (
        "statusbar-passthrough-bg",
        "colors.statusbar.passthrough.bg",
    ),
    (
        "statusbar-passthrough-fg",
        "colors.statusbar.passthrough.fg",
    ),
    ("statusbar-private-bg", "colors.statusbar.private.bg"),
    ("statusbar-private-fg", "colors.statusbar.private.fg"),
    (
        "statusbar-https-fg",
        "colors.statusbar.url.success.https.fg",
    ),
    ("statusbar-http-fg", "colors.statusbar.url.success.http.fg"),
    ("statusbar-url-error-fg", "colors.statusbar.url.error.fg"),
    ("messages-error-bg", "colors.messages.error.bg"),
    ("messages-error-fg", "colors.messages.error.fg"),
    ("messages-warning-bg", "colors.messages.warning.bg"),
    ("messages-warning-fg", "colors.messages.warning.fg"),
    ("tabs-bar-bg", "colors.tabs.bar.bg"),
    ("tabs-odd-bg", "colors.tabs.odd.bg"),
    ("tabs-even-bg", "colors.tabs.even.bg"),
    ("tabs-fg", "colors.tabs.odd.fg"),
    ("tabs-selected-bg", "colors.tabs.selected.odd.bg"),
    ("tabs-selected-fg", "colors.tabs.selected.odd.fg"),
    ("tabs-pinned-bg", "colors.tabs.pinned.odd.bg"),
    ("tabs-pinned-fg", "colors.tabs.pinned.odd.fg"),
    ("tabs-indicator-start", "colors.tabs.indicator.start"),
    ("tabs-indicator-error", "colors.tabs.indicator.error"),
    ("completion-bg", "colors.completion.odd.bg"),
    ("completion-fg", "colors.completion.fg"),
    ("completion-category-bg", "colors.completion.category.bg"),
    ("completion-category-fg", "colors.completion.category.fg"),
    (
        "completion-description-fg",
        "colors.completion.description.fg",
    ),
    (
        "completion-selected-bg",
        "colors.completion.item.selected.bg",
    ),
    (
        "completion-selected-fg",
        "colors.completion.item.selected.fg",
    ),
    ("completion-match-fg", "colors.completion.match.fg"),
    ("keyhint-fg", "colors.keyhint.suffix.fg"),
    ("prompts-bg", "colors.prompts.bg"),
    ("prompts-fg", "colors.prompts.fg"),
    ("prompts-border", "colors.prompts.border"),
    ("prompts-key-bg", "colors.prompts.key.bg"),
    ("hints-bg", "colors.hints.bg"),
    ("hints-fg", "colors.hints.fg"),
    ("hints-border", "colors.hints.border"),
    ("hints-match-fg", "colors.hints.match.fg"),
];

/// Pairs of text and background tokens that must stay readable.
pub const TEXT_PAIRS: &[(&str, &str)] = &[
    ("statusbar-fg", "statusbar-bg"),
    ("statusbar-insert-fg", "statusbar-insert-bg"),
    ("statusbar-passthrough-fg", "statusbar-passthrough-bg"),
    ("statusbar-private-fg", "statusbar-private-bg"),
    ("messages-error-fg", "messages-error-bg"),
    ("messages-warning-fg", "messages-warning-bg"),
    ("tabs-fg", "tabs-odd-bg"),
    ("tabs-fg", "tabs-even-bg"),
    ("tabs-selected-fg", "tabs-selected-bg"),
    ("completion-fg", "completion-bg"),
    ("completion-category-fg", "completion-category-bg"),
    ("completion-selected-fg", "completion-selected-bg"),
    ("prompts-fg", "prompts-bg"),
    ("hints-fg", "hints-bg"),
    ("tabs-pinned-fg", "tabs-pinned-bg"),
    ("statusbar-https-fg", "statusbar-bg"),
    ("statusbar-http-fg", "statusbar-bg"),
    ("statusbar-url-error-fg", "statusbar-bg"),
    ("completion-description-fg", "completion-bg"),
    ("keyhint-fg", "completion-bg"),
    ("completion-match-fg", "completion-bg"),
    ("prompts-border", "prompts-bg"),
    ("prompts-border", "prompts-key-bg"),
    ("hints-match-fg", "hints-bg"),
];

/// A theme's base colors, as `#rrggbb`.
struct Palette<'a> {
    /// Darkest background: the status bar and headers.
    base: &'a str,
    /// Main background: the overlay and the tab bar.
    surface: &'a str,
    /// Alternating tabs.
    surface2: &'a str,
    surface3: &'a str,
    fg: &'a str,
    muted: &'a str,
    accent: &'a str,
    yellow: &'a str,
    red: &'a str,
    orange: &'a str,
    green: &'a str,
    blue: &'a str,
}

/// The built-in themes, `ui.theme`'s choices. `riptide` keeps the colors
/// riptide had before themes.
pub const THEMES: &[&str] = &[
    "riptide",
    "riptide-light",
    "gruvbox-dark",
    "gruvbox-light",
    "catppuccin-mocha",
    "catppuccin-latte",
    "nord",
    "dracula",
    "solarized-dark",
    "solarized-light",
    "tokyo-night",
];

/// `ui.theme`'s choices: `auto`, then [`THEMES`].
pub const THEME_CHOICES: &[&str] = &[
    "auto",
    "riptide",
    "riptide-light",
    "gruvbox-dark",
    "gruvbox-light",
    "catppuccin-mocha",
    "catppuccin-latte",
    "nord",
    "dracula",
    "solarized-dark",
    "solarized-light",
    "tokyo-night",
];

/// Whether pages are asked for dark colors (the desktop's preference, or
/// `colors.webpage.preferred_color_scheme`), as the status bar last saw.
static PREFERS_DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Record the light/dark preference; true if it changed.
pub fn set_prefers_dark(dark: bool) -> bool {
    PREFERS_DARK.swap(dark, std::sync::atomic::Ordering::Relaxed) != dark
}

/// The theme `ui.theme` means now: `auto` picks `ui.auto_theme.dark` or
/// `ui.auto_theme.light`.
pub fn theme_name(settings: &Settings, dark: bool) -> &str {
    auto_name(settings, settings.str("ui.theme"), dark)
}

fn auto_name<'a>(settings: &'a Settings, name: &'a str, dark: bool) -> &'a str {
    match name {
        "auto" if dark => settings.str("ui.auto_theme.dark"),
        "auto" => settings.str("ui.auto_theme.light"),
        name => name,
    }
}

fn palette(name: &str) -> Option<Palette<'static>> {
    let p =
        |base, surface, surface2, surface3, fg, muted, accent, yellow, red, orange, green, blue| {
            Palette {
                base,
                surface,
                surface2,
                surface3,
                fg,
                muted,
                accent,
                yellow,
                red,
                orange,
                green,
                blue,
            }
        };
    Some(match name {
        "riptide" => p(
            "#061826", "#0b2a3f", "#12364f", "#164260", "#e9f7f6", "#8fb3c4", "#2ec4b6", "#d9b44a",
            "#b8323f", "#a8651a", "#0d6e5a", "#24418f",
        ),
        "riptide-light" => p(
            "#d7e6ec", "#eef5f7", "#e2edf1", "#d5e4ea", "#0b2a3f", "#4b6a7a", "#0f8a80", "#9a7210",
            "#b8323f", "#a8651a", "#1f7a5a", "#2d55b0",
        ),
        "gruvbox-dark" => p(
            "#1d2021", "#282828", "#32302f", "#3c3836", "#ebdbb2", "#a89984", "#83a598", "#fabd2f",
            "#cc241d", "#d65d0e", "#98971a", "#458588",
        ),
        "gruvbox-light" => p(
            "#ebdbb2", "#fbf1c7", "#f2e5bc", "#ebdbb2", "#3c3836", "#7c6f64", "#076678", "#b57614",
            "#9d0006", "#af3a03", "#79740e", "#076678",
        ),
        "catppuccin-mocha" => p(
            "#11111b", "#1e1e2e", "#313244", "#45475a", "#cdd6f4", "#a6adc8", "#89b4fa", "#f9e2af",
            "#f38ba8", "#fab387", "#a6e3a1", "#89b4fa",
        ),
        "catppuccin-latte" => p(
            "#dce0e8", "#eff1f5", "#e6e9ef", "#ccd0da", "#4c4f69", "#6c6f85", "#1e66f5", "#df8e1d",
            "#d20f39", "#fe640b", "#40a02b", "#1e66f5",
        ),
        "nord" => p(
            "#242933", "#2e3440", "#3b4252", "#434c5e", "#eceff4", "#a3abb9", "#88c0d0", "#ebcb8b",
            "#bf616a", "#d08770", "#a3be8c", "#5e81ac",
        ),
        "dracula" => p(
            "#191a21", "#282a36", "#343746", "#44475a", "#f8f8f2", "#a6accd", "#bd93f9", "#f1fa8c",
            "#ff5555", "#ffb86c", "#50fa7b", "#6272a4",
        ),
        "solarized-dark" => p(
            "#00212b", "#002b36", "#073642", "#0a4050", "#eee8d5", "#93a1a1", "#268bd2", "#b58900",
            "#dc322f", "#cb4b16", "#859900", "#268bd2",
        ),
        "solarized-light" => p(
            "#eee8d5", "#fdf6e3", "#f5efdc", "#eee8d5", "#073642", "#586e75", "#268bd2", "#b58900",
            "#dc322f", "#cb4b16", "#859900", "#268bd2",
        ),
        "tokyo-night" => p(
            "#16161e", "#1a1b26", "#24283b", "#292e42", "#c0caf5", "#9aa5ce", "#7aa2f7", "#e0af68",
            "#f7768e", "#ff9e64", "#9ece6a", "#7aa2f7",
        ),
        _ => return None,
    })
}

/// The tokens of theme `name`: a built-in one, or one from `themes/`.
pub fn theme(name: &str) -> Option<BTreeMap<&'static str, String>> {
    match palette(name) {
        Some(p) => Some(derive(&p, name)),
        None => user_themes().get(name).cloned(),
    }
}

/// Every token from a palette. `name` is only for riptide's own exceptions.
fn derive(p: &Palette, name: &str) -> BTreeMap<&'static str, String> {
    let readable = |bg: &str| best_text(bg, &[p.fg, p.base, "#000000", "#ffffff"]);
    let mut t: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut set = |k: &'static str, v: &str| {
        t.insert(k, v.to_string());
    };
    // Colored text (accents, links, keys) moved toward readable where the
    // palette's own color is too faint on its background.
    let legible = |color: &str, bg: &str| legible(color, bg);
    // A label background must be light enough for dark text: light themes'
    // yellows are too dark for that.
    let mut hint_bg = p.yellow.to_string();
    while luminance(&hint_bg) < 0.45 {
        hint_bg = mix(&hint_bg, "#ffffff", 0.2);
    }
    let pinned_bg = mix(p.surface2, p.blue, 0.35);
    // The current tab is always the darkest tab, pinned ones included, with
    // a hint of the accent; the accent alone can match the pinned tint.
    let darkest_other = [p.surface, p.surface2, p.surface3, pinned_bg.as_str()]
        .iter()
        .map(|c| luminance(c))
        .fold(f64::MAX, f64::min);
    let mut selected_tab = mix(p.base, p.accent, 0.12);
    while luminance(&selected_tab) > darkest_other * 0.7 && luminance(&selected_tab) > 0.002 {
        selected_tab = mix(&selected_tab, "#000000", 0.15);
    }
    // In dark themes, very bright colors would glare across a whole bar.
    let dark = luminance(p.base) < 0.2;
    let bar = |c: &str| -> String {
        if dark && luminance(c) > 0.3 {
            mix(c, p.base, 0.35)
        } else {
            c.to_string()
        }
    };
    let (green, blue, red, orange) = (bar(p.green), bar(p.blue), bar(p.red), bar(p.orange));
    set("statusbar-bg", p.base);
    set("statusbar-fg", p.fg);
    set("statusbar-insert-bg", &green);
    set("statusbar-insert-fg", &readable(&green));
    set("statusbar-passthrough-bg", &blue);
    set("statusbar-passthrough-fg", &readable(&blue));
    set("statusbar-private-bg", p.surface3);
    set("statusbar-private-fg", &readable(p.surface3));
    set("statusbar-https-fg", &legible(p.accent, p.base));
    set("statusbar-http-fg", p.fg);
    set("statusbar-url-error-fg", &legible(p.yellow, p.base));
    set("messages-error-bg", &red);
    set("messages-error-fg", &readable(&red));
    set("messages-warning-bg", &orange);
    set("messages-warning-fg", &readable(&orange));
    set("tabs-bar-bg", p.surface);
    set("tabs-odd-bg", p.surface2);
    set("tabs-even-bg", p.surface3);
    set("tabs-fg", p.fg);
    set("tabs-selected-bg", &selected_tab);
    set("tabs-selected-fg", &readable(&selected_tab));
    set("tabs-pinned-bg", &pinned_bg);
    set("tabs-pinned-fg", &readable(&pinned_bg));
    set("tabs-indicator-start", p.accent);
    set("tabs-indicator-error", p.red);
    set("completion-bg", p.surface);
    set("completion-fg", p.fg);
    set("completion-category-bg", p.base);
    set("completion-category-fg", p.fg);
    set("completion-description-fg", &legible(p.muted, p.surface));
    set("completion-selected-bg", p.accent);
    set("completion-selected-fg", &readable(p.accent));
    set("keyhint-fg", &legible(p.accent, p.surface));
    set("completion-match-fg", &legible(p.yellow, p.surface));
    set("prompts-bg", p.surface);
    set("prompts-fg", p.fg);
    let key_bg = mix(p.surface, p.yellow, 0.18);
    set(
        "prompts-border",
        &legible(&legible(p.yellow, p.surface), &key_bg),
    );
    set("prompts-key-bg", &key_bg);
    set("hints-bg", &hint_bg);
    set("hints-fg", &readable(&hint_bg));
    set("hints-border", &mix(&hint_bg, "#000000", 0.3));
    set(
        "hints-match-fg",
        &legible(
            &best_text(&hint_bg, &[p.green, p.blue, p.red, "#006400"]),
            &hint_bg,
        ),
    );
    // The riptide theme's exact colors from before themes.
    if name == "riptide" {
        for (k, v) in [
            ("tabs-selected-bg", "#04121c"),
            ("tabs-selected-fg", "#ffffff"),
            ("tabs-pinned-bg", "#1b5e86"),
            ("tabs-pinned-fg", "#e9f7f6"),
            ("tabs-indicator-start", "#8fe3dc"),
            ("tabs-indicator-error", "#ff6b6b"),
            ("statusbar-private-bg", "#3b4a5a"),
            ("statusbar-url-error-fg", "#f4c56a"),
            ("hints-bg", "#ffc542"),
            ("hints-fg", "#000000"),
            ("hints-border", "#e3be23"),
            ("hints-match-fg", "#006400"),
        ] {
            t.insert(k, v.to_string());
        }
    }
    t
}

/// Themes from `themes/*.toml` in the config directory, by name.
static USER_THEMES: std::sync::RwLock<BTreeMap<String, BTreeMap<&'static str, String>>> =
    std::sync::RwLock::new(BTreeMap::new());

fn user_themes()
-> std::sync::RwLockReadGuard<'static, BTreeMap<String, BTreeMap<&'static str, String>>> {
    USER_THEMES.read().unwrap_or_else(|e| e.into_inner())
}

/// Replace the user's themes (on loading the config).
pub fn set_user_themes(themes: BTreeMap<String, BTreeMap<&'static str, String>>) {
    *USER_THEMES.write().unwrap_or_else(|e| e.into_inner()) = themes;
}

/// Every theme name `ui.theme` accepts: `auto`, the built-in themes, then the user's.
pub fn names() -> Vec<String> {
    THEME_CHOICES
        .iter()
        .map(|t| t.to_string())
        .chain(user_themes().keys().cloned())
        .collect()
}

/// The palette keys a theme file's `[palette]` may set; the first eight are required.
pub const PALETTE_KEYS: &[&str] = &[
    "base", "surface", "fg", "accent", "yellow", "red", "green", "blue", "surface2", "surface3",
    "muted", "orange",
];

/// A user theme from its `[palette]` and `[colors]` tables. `colors` uses
/// `colors.*` setting names, with or without the `colors.` prefix.
pub fn user_theme(
    palette: &BTreeMap<String, String>,
    colors: &BTreeMap<String, String>,
) -> Result<BTreeMap<&'static str, String>, String> {
    for (key, value) in palette {
        if !PALETTE_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "unknown palette color {key:?}; use {}",
                PALETTE_KEYS.join(", ")
            ));
        }
        if rgb(value).is_none() {
            return Err(format!("palette.{key}: {value:?} isn't a #rrggbb color"));
        }
    }
    let get = |key: &str| palette.get(key).map(String::as_str);
    let need = |key: &str| get(key).ok_or_else(|| format!("palette.{key} is missing"));
    let (base, surface, fg, accent) = (
        need("base")?,
        need("surface")?,
        need("fg")?,
        need("accent")?,
    );
    let (yellow, red, green, blue) = (need("yellow")?, need("red")?, need("green")?, need("blue")?);
    // Colors a palette may leave out, made from the others.
    let surface2 = get("surface2").map_or_else(|| mix(surface, fg, 0.06), str::to_string);
    let surface3 = get("surface3").map_or_else(|| mix(surface, fg, 0.11), str::to_string);
    let muted = get("muted").map_or_else(|| mix(fg, surface, 0.4), str::to_string);
    let orange = get("orange").map_or_else(|| mix(red, yellow, 0.5), str::to_string);
    let p = Palette {
        base,
        surface,
        surface2: &surface2,
        surface3: &surface3,
        fg,
        muted: &muted,
        accent,
        yellow,
        red,
        orange: &orange,
        green,
        blue,
    };
    let mut tokens = derive(&p, "");
    for (setting, value) in colors {
        let full = if setting.starts_with("colors.") {
            setting.clone()
        } else {
            format!("colors.{setting}")
        };
        let Some((token, _)) = TOKENS.iter().find(|(_, s)| *s == full) else {
            return Err(format!("colors: unknown color {setting:?}"));
        };
        if !is_color(value) {
            return Err(format!("colors.{setting}: {value:?} isn't a color"));
        }
        tokens.insert(token, value.clone());
    }
    Ok(tokens)
}

/// riptide's own fonts: the CSS variable (`--rt-<name>`) and its setting.
pub const FONTS: &[(&str, &str)] = &[
    ("font-statusbar", "fonts.statusbar"),
    ("font-tabs-selected", "fonts.tabs.selected"),
    ("font-tabs-unselected", "fonts.tabs.unselected"),
    ("font-completion-entry", "fonts.completion.entry"),
    ("font-completion-category", "fonts.completion.category"),
    ("font-prompts", "fonts.prompts"),
    ("font-hints", "fonts.hints"),
    ("font-keyhint", "fonts.keyhint"),
];

/// A `fonts.*` value as a CSS `font`: qutebrowser's `default_size` and
/// `default_family` stand for `fonts.default_size` and `fonts.default_family`.
pub fn font(value: &str, settings: &Settings) -> String {
    value
        .replace("default_family", settings.str("fonts.default_family"))
        .replace("default_size", settings.str("fonts.default_size"))
}

/// Everything riptide's own pages style themselves with: the colors and
/// the fonts, as `--rt-<name>` variables.
pub fn ui_vars(settings: &Settings) -> BTreeMap<String, String> {
    ui_vars_previewing(settings, None)
}

/// [`ui_vars`], with theme `preview` in place of `ui.theme`.
pub fn ui_vars_previewing(settings: &Settings, preview: Option<&str>) -> BTreeMap<String, String> {
    let dark = PREFERS_DARK.load(std::sync::atomic::Ordering::Relaxed);
    let name = preview.unwrap_or(settings.str("ui.theme"));
    let mut vars: BTreeMap<String, String> = colors_of(settings, name, dark)
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    for (name, setting) in FONTS {
        vars.insert(name.to_string(), font(settings.str(setting), settings));
    }
    vars.insert(
        "statusbar-padding".into(),
        settings.str("statusbar.padding").to_string(),
    );
    vars.insert(
        "hints-padding".into(),
        settings.str("hints.padding").to_string(),
    );
    vars.insert(
        "hints-radius".into(),
        format!("{}px", settings.int("hints.radius")),
    );
    vars.insert(
        "tabs-padding".into(),
        settings.str("tabs.padding").to_string(),
    );
    vars
}

/// Whether `text` is safe as a CSS `font` value: no way out of the declaration.
pub fn is_font(text: &str) -> bool {
    !text.trim().is_empty() && !text.contains([';', '{', '}', '<', '>', '\\'])
}

/// The colors to use: `ui.theme`, with any `colors.*` setting on top.
pub fn resolve(settings: &Settings) -> BTreeMap<&'static str, String> {
    resolve_for(
        settings,
        PREFERS_DARK.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// [`resolve`], with `auto` following `dark`.
pub fn resolve_for(settings: &Settings, dark: bool) -> BTreeMap<&'static str, String> {
    colors_of(settings, settings.str("ui.theme"), dark)
}

/// The theme a command line like `:theme nord` is about to pick, to show
/// it before it's chosen.
pub fn previewed(command_line: &str) -> Option<String> {
    let rest = command_line.strip_prefix(':')?.trim_start();
    let name = rest.strip_prefix("theme")?;
    if !name.starts_with(char::is_whitespace) {
        return None;
    }
    let name = name.trim();
    names().into_iter().find(|t| t == name)
}

/// Theme `name` (`auto` follows `dark`), with any `colors.*` setting on top.
fn colors_of(settings: &Settings, name: &str, dark: bool) -> BTreeMap<&'static str, String> {
    let mut colors = theme(auto_name(settings, name, dark))
        .or_else(|| theme("riptide"))
        .unwrap_or_default();
    for (token, setting) in TOKENS {
        let value = settings.str(setting).trim();
        if !value.is_empty() {
            colors.insert(token, value.to_string());
        }
    }
    colors
}

/// Whether `text` is a CSS color riptide accepts in a `colors.*` setting:
/// `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(…)`/`rgba(…)`/`hsl(…)`, or a name.
pub fn is_color(text: &str) -> bool {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    if let Some((function, rest)) = text.split_once('(') {
        return matches!(function, "rgb" | "rgba" | "hsl" | "hsla")
            && rest.ends_with(')')
            && rest[..rest.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit() || " ,.%/".contains(c));
    }
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphabetic())
}

fn rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(f64::from);
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// An opaque `0xAARRGGBB` for CEF from `#rgb`, `#rrggbb`, `white` or `black`.
pub fn argb(text: &str) -> Option<u32> {
    let text = text.trim().to_ascii_lowercase();
    let hex = match text.as_str() {
        "white" => "ffffff".to_string(),
        "black" => "000000".to_string(),
        _ => {
            let hex = text.strip_prefix('#')?;
            match hex.len() {
                3 => hex.chars().flat_map(|c| [c, c]).collect(),
                6 => hex.to_string(),
                _ => return None,
            }
        }
    };
    u32::from_str_radix(&hex, 16)
        .ok()
        .map(|rgb| 0xFF00_0000 | rgb)
}

/// WCAG relative luminance of a `#rrggbb` color.
fn luminance(hex: &str) -> f64 {
    let Some((r, g, b)) = rgb(hex) else {
        return 0.0;
    };
    let channel = |c: f64| {
        let c = c / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG contrast ratio between two `#rrggbb` colors, from 1 to 21.
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The first of `candidates` readable on `bg` (WCAG AA, 4.5:1), or else the most readable.
fn best_text(bg: &str, candidates: &[&str]) -> String {
    candidates
        .iter()
        .find(|c| contrast(c, bg) >= 4.5)
        .or_else(|| {
            candidates
                .iter()
                .max_by(|a, b| contrast(a, bg).total_cmp(&contrast(b, bg)))
        })
        .map_or_else(|| "#ffffff".to_string(), |c| c.to_string())
}

/// `color`, or the nearest step from it toward black or white (whichever
/// suits `bg`) that is readable on `bg` (4.5:1).
fn legible(color: &str, bg: &str) -> String {
    let target = best_text(bg, &["#000000", "#ffffff"]);
    (0..=20)
        .map(|step| mix(color, &target, f64::from(step) / 20.0))
        .find(|c| contrast(c, bg) >= 4.5)
        .unwrap_or(target)
}

/// `a` moved `amount` (0 to 1) of the way to `b`.
fn mix(a: &str, b: &str, amount: f64) -> String {
    let (Some((ar, ag, ab)), Some((br, bg, bb))) = (rgb(a), rgb(b)) else {
        return a.to_string();
    };
    let m = |x: f64, y: f64| (x + (y - x) * amount).round() as u8;
    format!("#{:02x}{:02x}{:02x}", m(ar, br), m(ag, bg), m(ab, bb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_command_lines_preview_their_theme() {
        assert_eq!(previewed(":theme nord"), Some("nord".into()));
        assert_eq!(previewed(": theme  dracula "), Some("dracula".into()));
        assert_eq!(previewed(":theme no"), None);
        assert_eq!(previewed(":themenord"), None);
        assert_eq!(previewed(":set ui.theme nord"), None);
        assert_eq!(previewed("/theme nord"), None);
    }

    #[test]
    fn auto_follows_the_preference_with_the_configured_pair() {
        assert_eq!(THEME_CHOICES[0], "auto");
        assert_eq!(&THEME_CHOICES[1..], THEMES);
        let mut settings = Settings::default();
        settings
            .set("ui.theme", crate::settings::Value::Str("auto".into()))
            .unwrap();
        assert_eq!(theme_name(&settings, true), "riptide");
        assert_eq!(theme_name(&settings, false), "riptide-light");
        settings
            .set(
                "ui.auto_theme.light",
                crate::settings::Value::Str("nord".into()),
            )
            .unwrap();
        assert_eq!(
            resolve_for(&settings, false),
            resolve_for(
                &{
                    let mut nord = Settings::default();
                    nord.set("ui.theme", crate::settings::Value::Str("nord".into()))
                        .unwrap();
                    nord
                },
                true
            )
        );
        let dark = crate::settings::find("ui.auto_theme.dark").unwrap();
        assert!(dark.parse("auto").is_err());
    }

    #[test]
    fn every_theme_defines_every_token_with_valid_colors() {
        for name in THEMES {
            let t = theme(name).unwrap_or_else(|| panic!("{name} has no palette"));
            for (token, _) in TOKENS {
                let color = t
                    .get(token)
                    .unwrap_or_else(|| panic!("{name} lacks {token}"));
                assert!(rgb(color).is_some(), "{name}: {token} = {color}");
            }
            assert_eq!(
                t.len(),
                TOKENS.len(),
                "{name} has tokens that aren't in TOKENS"
            );
        }
    }

    #[test]
    fn every_theme_keeps_its_text_readable() {
        for name in THEMES {
            let t = theme(name).unwrap();
            for (fg, bg) in TEXT_PAIRS {
                let ratio = contrast(&t[fg], &t[bg]);
                assert!(
                    ratio >= 4.5,
                    "{name}: {fg} {} on {bg} {} is only {ratio:.2}:1",
                    t[fg],
                    t[bg]
                );
            }
        }
    }

    #[test]
    fn settings_override_the_theme() {
        let mut settings = Settings::default();
        assert_eq!(resolve(&settings)["statusbar-bg"], "#061826");
        settings
            .set("ui.theme", crate::settings::Value::Str("nord".into()))
            .unwrap();
        assert_eq!(resolve(&settings)["statusbar-bg"], "#242933");
        settings
            .set(
                "colors.statusbar.normal.bg",
                crate::settings::Value::Str("#123456".into()),
            )
            .unwrap();
        assert_eq!(resolve(&settings)["statusbar-bg"], "#123456");
    }

    #[test]
    fn colors() {
        for good in [
            "#fff",
            "#a1b2c3",
            "#a1b2c3cc",
            "rgb(1, 2, 3)",
            "rgba(1,2,3,0.5)",
            "hsl(120, 50%, 50%)",
            "teal",
        ] {
            assert!(is_color(good), "{good}");
        }
        for bad in [
            "",
            "#12",
            "#ggg",
            "rgb(1;2)",
            "url(x)",
            "red;",
            "expression(alert(1))",
        ] {
            assert!(!is_color(bad), "{bad}");
        }
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.01);
        assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(argb("#1e1e2e"), Some(0xFF1E_1E2E));
        assert_eq!(argb("#fff"), Some(0xFFFF_FFFF));
        assert_eq!(argb("Black"), Some(0xFF00_0000));
        assert_eq!(argb("teal"), None);
    }

    #[test]
    fn fonts_fill_in_the_defaults() {
        let mut settings = Settings::default();
        let vars = ui_vars(&settings);
        assert_eq!(
            vars["font-statusbar"],
            settings.str("fonts.default_size").to_string()
                + " "
                + settings.str("fonts.default_family")
        );
        assert!(vars["font-hints"].starts_with("bold "));
        settings
            .set(
                "fonts.default_size",
                crate::settings::Value::Str("12pt".into()),
            )
            .unwrap();
        settings
            .set(
                "fonts.tabs.selected",
                crate::settings::Value::Str("bold default_size serif".into()),
            )
            .unwrap();
        let vars = ui_vars(&settings);
        assert_eq!(vars["font-tabs-selected"], "bold 12pt serif");
        assert!(vars["font-statusbar"].starts_with("12pt "));
        assert!(is_font("bold 10pt \"Fira Code\", monospace"));
        assert!(!is_font("10pt x; background: red"));
        assert!(!is_font("10pt x}"));
    }

    #[test]
    fn the_current_tab_is_the_darkest_tab() {
        for name in THEMES {
            let t = theme(name).unwrap();
            let selected = luminance(&t["tabs-selected-bg"]);
            for other in [
                "tabs-odd-bg",
                "tabs-even-bg",
                "tabs-pinned-bg",
                "tabs-bar-bg",
            ] {
                assert!(
                    selected < luminance(&t[other]),
                    "{name}: the current tab {} isn't darker than {other} {}",
                    t["tabs-selected-bg"],
                    t[other]
                );
            }
        }
    }
}
