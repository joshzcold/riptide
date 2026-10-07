//! The window title, from qutebrowser's `window.title_format` fields.

/// Fill `{current_title}`, `{title_sep}`, `{current_url}`, `{host}` and
/// `{mode}`. `title` falls back to the URL; `title_sep` is " - " only when
/// there is a title to separate.
pub fn format(template: &str, title: &str, url: &str, mode: &str) -> String {
    let current = if title.is_empty() { url } else { title };
    let host = crate::url::host(url);
    let sep = if current.is_empty() { "" } else { " - " };
    template
        .replace("{current_title}", current)
        .replace("{title_sep}", sep)
        .replace("{current_url}", url)
        .replace("{host}", host)
        .replace("{mode}", mode)
}

/// A tab's label for `tabs.title.format` and `tabs.title.format_pinned`:
/// `{index}`, `{aligned_index}`, `{current_title}`, `{current_url}`,
/// `{host}`, `{perc}` (loading progress), `{audio}` (`[M] ` when muted),
/// `{media}` (see [`media_label`]) and `{private}` (`[Private] ` in private
/// windows).
pub struct TabFields<'a> {
    pub index: usize,
    pub count: usize,
    pub title: &'a str,
    pub url: &'a str,
    pub progress: Option<f64>,
    pub muted: bool,
    /// The page has access to video (a camera or the screen) and to audio (a microphone).
    pub media: (bool, bool),
    pub private: bool,
}

/// `[A/V] `, `[V] ` (a camera or the screen) or `[A] ` (a microphone)
/// while a page captures them; empty otherwise.
pub fn media_label((video, audio): (bool, bool)) -> &'static str {
    match (video, audio) {
        (true, true) => "[A/V] ",
        (true, false) => "[V] ",
        (false, true) => "[A] ",
        (false, false) => "",
    }
}

pub fn tab_label(template: &str, f: &TabFields) -> String {
    let current = if f.title.is_empty() { f.url } else { f.title };
    let width = f.count.to_string().len();
    template
        .replace("{index}", &f.index.to_string())
        .replace("{aligned_index}", &format!("{:>width$}", f.index))
        .replace("{current_title}", current)
        .replace("{current_url}", f.url)
        .replace("{host}", crate::url::host(f.url))
        .replace(
            "{perc}",
            &f.progress
                .map(|p| format!("[{}%] ", (p * 100.0).round()))
                .unwrap_or_default(),
        )
        .replace("{audio}", if f.muted { "[M] " } else { "" })
        .replace("{media}", media_label(f.media))
        .replace("{private}", if f.private { "[Private] " } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_marks_what_a_page_captures() {
        assert_eq!(media_label((true, true)), "[A/V] ");
        assert_eq!(media_label((true, false)), "[V] ");
        assert_eq!(media_label((false, true)), "[A] ");
        assert_eq!(media_label((false, false)), "");
    }

    const DEFAULT: &str = "{current_title}{title_sep}Riptide";

    #[test]
    fn default_format() {
        assert_eq!(
            format(DEFAULT, "Rust", "https://rust-lang.org/", "normal"),
            "Rust - Riptide"
        );
        assert_eq!(
            format(DEFAULT, "", "https://x.org/a", "normal"),
            "https://x.org/a - Riptide"
        );
        assert_eq!(format(DEFAULT, "", "", "normal"), "Riptide");
    }

    #[test]
    fn other_fields() {
        assert_eq!(
            format(
                "{mode}|{host}|{current_url}",
                "T",
                "https://x.org:8080/p?q",
                "insert"
            ),
            "insert|x.org|https://x.org:8080/p?q"
        );
        assert_eq!(format("{host}", "", "about:blank", "normal"), "");
    }

    #[test]
    fn tab_labels() {
        let f = TabFields {
            index: 3,
            count: 12,
            title: "Rust",
            url: "https://rust-lang.org/learn",
            progress: Some(0.42),
            muted: true,
            media: (false, false),
            private: false,
        };
        assert_eq!(
            tab_label("{audio}{index}: {current_title}", &f),
            "[M] 3: Rust"
        );
        assert_eq!(
            tab_label("{aligned_index} {host} {perc}", &f),
            " 3 rust-lang.org [42%] "
        );
        assert_eq!(tab_label("{index}", &f), "3");
    }
}
