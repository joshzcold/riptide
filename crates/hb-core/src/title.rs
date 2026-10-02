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

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "{current_title}{title_sep}hackers-browser";

    #[test]
    fn default_format() {
        assert_eq!(
            format(DEFAULT, "Rust", "https://rust-lang.org/", "normal"),
            "Rust - hackers-browser"
        );
        assert_eq!(
            format(DEFAULT, "", "https://x.org/a", "normal"),
            "https://x.org/a - hackers-browser"
        );
        assert_eq!(format(DEFAULT, "", "", "normal"), "hackers-browser");
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
}
