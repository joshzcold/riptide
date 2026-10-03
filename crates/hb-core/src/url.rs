use std::collections::BTreeMap;

pub const DEFAULT_SEARCH_ENGINE: &str = "https://duckduckgo.com/?q={}";
pub const DEFAULT_START_PAGE: &str = "https://start.duckduckgo.com/";

const KNOWN_SCHEMES: &[&str] = &[
    "http",
    "https",
    "file",
    "about",
    "data",
    "chrome",
    "view-source",
    "ftp",
    "blob",
];

/// Turn `:open` input into a URL: explicit URLs pass through, things that look
/// like hosts get `https://`, and anything else becomes a search. A first word
/// naming an engine in `engines` (e.g. `g rust`) picks that engine; otherwise
/// the `DEFAULT` entry is used.
pub fn fuzzy_url(input: &str, engines: &BTreeMap<String, String>) -> String {
    let input = input.trim();
    if has_known_scheme(input) {
        return input.to_string();
    }
    if !input.contains(char::is_whitespace) && looks_like_host(input) {
        let scheme = if is_local(input) { "http" } else { "https" };
        return format!("{scheme}://{input}");
    }
    let (engine, query) = match input.split_once(char::is_whitespace) {
        Some((name, rest)) if name != "DEFAULT" && engines.contains_key(name) => {
            (&engines[name], rest.trim())
        }
        _ => match engines.get("DEFAULT") {
            Some(engine) => (engine, input),
            None => return format!("https://{input}"),
        },
    };
    engine.replace("{}", &encode_query(query))
}

fn has_known_scheme(input: &str) -> bool {
    match input.split_once(':') {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            KNOWN_SCHEMES.contains(&scheme.as_str())
                || (rest.starts_with("//") && is_valid_scheme(&scheme))
        }
        None => false,
    }
}

fn is_valid_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Split `scheme://authority` from the rest (path, query and fragment).
fn split_origin(url: &str) -> Option<(&str, &str)> {
    let start = url.find("://")? + 3;
    let end = url[start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| start + i);
    Some((&url[..end], &url[end..]))
}

/// One level up: drop the query and fragment, then the last path segment.
/// `None` at the site's root.
pub fn up(url: &str) -> Option<String> {
    let (origin, rest) = split_origin(url)?;
    let path = rest.split(['?', '#']).next().unwrap_or_default();
    if path.len() < rest.len() {
        return Some(format!("{origin}{path}"));
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parent = &trimmed[..trimmed.rfind('/').unwrap_or(0) + 1];
    Some(format!("{origin}{parent}"))
}

/// Add `delta` to the last number in the path or query, keeping leading zeros.
pub fn increment(url: &str, delta: i64) -> Option<String> {
    let (origin, rest) = split_origin(url)?;
    let rest_end = rest.find('#').unwrap_or(rest.len());
    let digits_end = rest[..rest_end].rfind(|c: char| c.is_ascii_digit())? + 1;
    let digits_start = rest[..digits_end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let number = &rest[digits_start..digits_end];
    let value = number.parse::<i64>().ok()?.checked_add(delta)?;
    if value < 0 {
        return None;
    }
    let width = if number.starts_with('0') {
        number.len()
    } else {
        0
    };
    Some(format!(
        "{origin}{}{value:0width$}{}",
        &rest[..digits_start],
        &rest[digits_end..]
    ))
}

/// The host name of a URL, without user info or port; empty if it has none.
pub fn host(url: &str) -> &str {
    let Some((_, rest)) = url.split_once("://") else {
        return "";
    };
    let authority = host_of(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

fn host_of(input: &str) -> &str {
    let end = input.find(['/', '?', '#']).unwrap_or(input.len());
    let authority = &input[..end];
    match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    }
}

fn is_local(input: &str) -> bool {
    let host = host_of(input);
    host == "localhost"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_private())
}

fn looks_like_host(input: &str) -> bool {
    let host = host_of(input);
    if host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    let mut labels = host.split('.');
    let tld = labels.next_back().unwrap_or_default();
    host.contains('.')
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'))
        && tld.len() >= 2
        && tld.chars().all(char::is_alphabetic)
}

fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn going_up() {
        assert_eq!(
            up("https://x.org/a/b/c").as_deref(),
            Some("https://x.org/a/b/")
        );
        assert_eq!(
            up("https://x.org/a/b/").as_deref(),
            Some("https://x.org/a/")
        );
        assert_eq!(
            up("https://x.org/a?q=1#f").as_deref(),
            Some("https://x.org/a")
        );
        assert_eq!(up("https://x.org/a").as_deref(), Some("https://x.org/"));
        assert_eq!(up("https://x.org/"), None);
        assert_eq!(up("https://x.org"), None);
        assert_eq!(up("about:blank"), None);
    }

    #[test]
    fn incrementing() {
        assert_eq!(
            increment("https://x.org/page/9", 1).as_deref(),
            Some("https://x.org/page/10")
        );
        assert_eq!(
            increment("https://x.org/img007.png", 1).as_deref(),
            Some("https://x.org/img008.png")
        );
        assert_eq!(
            increment("https://x.org/p?page=2&x=a#s3", -1).as_deref(),
            Some("https://x.org/p?page=1&x=a#s3")
        );
        assert_eq!(
            increment("https://x2.org/a", 1),
            None,
            "the host isn't touched"
        );
        assert_eq!(increment("https://x.org/p0", -1), None);
    }

    #[test]
    fn hosts() {
        assert_eq!(
            host("https://user:pw@www.example.com:8443/a?b#c"),
            "www.example.com"
        );
        assert_eq!(host("http://localhost/"), "localhost");
        assert_eq!(host("about:blank"), "");
        assert_eq!(host("file:///tmp/x"), "");
    }

    fn engines() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("DEFAULT".to_string(), DEFAULT_SEARCH_ENGINE.to_string()),
            (
                "w".to_string(),
                "https://en.wikipedia.org/w/index.php?search={}".to_string(),
            ),
        ])
    }

    fn fuzzy(input: &str) -> String {
        fuzzy_url(input, &engines())
    }

    #[test]
    fn engine_keywords() {
        assert_eq!(
            fuzzy("w rust lang"),
            "https://en.wikipedia.org/w/index.php?search=rust+lang"
        );
        // A lone keyword is a search for that word, not an empty engine query.
        assert_eq!(fuzzy("w"), "https://duckduckgo.com/?q=w");
        assert_eq!(fuzzy("DEFAULT x"), "https://duckduckgo.com/?q=DEFAULT+x");
    }

    #[test]
    fn passes_through_urls() {
        assert_eq!(fuzzy("https://example.com/a?b"), "https://example.com/a?b");
        assert_eq!(fuzzy("about:blank"), "about:blank");
        assert_eq!(fuzzy("file:///tmp/x.html"), "file:///tmp/x.html");
        assert_eq!(fuzzy("gopher://x.org"), "gopher://x.org");
    }

    #[test]
    fn adds_scheme_to_hosts() {
        assert_eq!(fuzzy("example.com"), "https://example.com");
        assert_eq!(
            fuzzy("example.com:8080/path"),
            "https://example.com:8080/path"
        );
        assert_eq!(fuzzy("localhost:3000"), "http://localhost:3000");
        assert_eq!(fuzzy("192.168.1.1"), "http://192.168.1.1");
        assert_eq!(fuzzy("8.8.8.8"), "https://8.8.8.8");
    }

    #[test]
    fn searches_everything_else() {
        assert_eq!(fuzzy("rust lang"), "https://duckduckgo.com/?q=rust+lang");
        assert_eq!(fuzzy("c++"), "https://duckduckgo.com/?q=c%2B%2B");
        assert_eq!(fuzzy("v1.2"), "https://duckduckgo.com/?q=v1.2");
        assert_eq!(fuzzy("hello"), "https://duckduckgo.com/?q=hello");
    }
}
