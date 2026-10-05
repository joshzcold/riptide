//! Renders `CHANGELOG.md` for `riptide://changelog/`. git-cliff writes a small
//! subset of Markdown (headings, bullets, bold, code and links), so that is
//! all this handles. Everything else is escaped text.

/// HTML for the changelog's body.
pub fn to_html(markdown: &str) -> String {
    let mut html = String::new();
    let mut in_list = false;
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |html: &mut String, paragraph: &mut Vec<&str>| {
        if !paragraph.is_empty() {
            html.push_str(&format!("<p>{}</p>\n", inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    for line in markdown.lines() {
        let line = line.trim_end();
        let item = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
        if item.is_none() && in_list {
            html.push_str("</ul>\n");
            in_list = false;
        }
        if let Some(item) = item {
            flush(&mut html, &mut paragraph);
            if !in_list {
                html.push_str("<ul>\n");
                in_list = true;
            }
            html.push_str(&format!("<li>{}</li>\n", inline(item)));
        } else if let Some((level, text)) = heading(line) {
            flush(&mut html, &mut paragraph);
            html.push_str(&format!("<h{level}>{}</h{level}>\n", inline(text)));
        } else if line.is_empty() {
            flush(&mut html, &mut paragraph);
        } else {
            paragraph.push(line);
        }
    }
    flush(&mut html, &mut paragraph);
    if in_list {
        html.push_str("</ul>\n");
    }
    html
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|&b| b == b'#').count();
    let text = line.get(level..)?.strip_prefix(' ')?;
    (1..=4).contains(&level).then_some((level, text))
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `**bold**`, `` `code` `` and `[text](https://…)`.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            out.push_str(&format!("<strong>{}</strong>", inline(&after[..end])));
            rest = &after[end + 2..];
        } else if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            out.push_str(&format!("<code>{}</code>", escape(&after[..end])));
            rest = &after[end + 1..];
        } else if let Some((label, url, after)) = link(rest) {
            out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                escape(url),
                inline(label)
            ));
            rest = after;
        } else {
            let c = rest.chars().next().unwrap_or_default();
            out.push_str(&escape(&c.to_string()));
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// `[label](url)` at the start of `text`, for https URLs only.
fn link(text: &str) -> Option<(&str, &str, &str)> {
    let after = text.strip_prefix('[')?;
    let close = after.find("](")?;
    let label = &after[..close];
    let target = &after[close + 2..];
    let end = target.find(')')?;
    let url = &target[..end];
    url.starts_with("https://")
        .then_some((label, url, &target[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_git_cliff_output() {
        let md = "# Changelog\n\nAll changes. See [git-cliff](https://git-cliff.org).\n\n## Unreleased\n\n### Features\n\n- **ui:** Add `{host}` to titles\n- Plain item\n\n### Bug fixes\n\n- Fix <script> & stuff\n";
        let html = to_html(md);
        assert_eq!(
            html,
            "<h1>Changelog</h1>\n\
             <p>All changes. See <a href=\"https://git-cliff.org\">git-cliff</a>.</p>\n\
             <h2>Unreleased</h2>\n\
             <h3>Features</h3>\n\
             <ul>\n<li><strong>ui:</strong> Add <code>{host}</code> to titles</li>\n<li>Plain item</li>\n</ul>\n\
             <h3>Bug fixes</h3>\n\
             <ul>\n<li>Fix &lt;script&gt; &amp; stuff</li>\n</ul>\n"
        );
    }

    #[test]
    fn only_https_links_become_links() {
        assert_eq!(
            inline("[x](javascript:alert(1))"),
            "[x](javascript:alert(1))"
        );
        assert_eq!(
            inline("[a\"b](https://x.org/\")"),
            "<a href=\"https://x.org/&quot;\">a&quot;b</a>"
        );
        assert_eq!(inline("**unclosed"), "**unclosed");
    }
}
