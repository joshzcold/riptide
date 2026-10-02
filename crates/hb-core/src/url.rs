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
/// like hosts get `https://`, and anything else becomes a search.
pub fn fuzzy_url(input: &str, search_engine: &str) -> String {
    let input = input.trim();
    if has_known_scheme(input) {
        return input.to_string();
    }
    if !input.contains(char::is_whitespace) && looks_like_host(input) {
        let scheme = if is_local(input) { "http" } else { "https" };
        return format!("{scheme}://{input}");
    }
    search_engine.replace("{}", &encode_query(input))
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

    fn fuzzy(input: &str) -> String {
        fuzzy_url(input, DEFAULT_SEARCH_ENGINE)
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
