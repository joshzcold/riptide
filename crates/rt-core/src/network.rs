//! Network settings as Chromium takes them: the `proxy` preference and the
//! WebRTC IP policy.

/// What `content.proxy` asks Chromium for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proxy {
    /// The desktop's proxy settings.
    System,
    /// No proxy.
    Direct,
    /// One proxy server, e.g. `socks5://127.0.0.1:9050`.
    Server(String),
    /// A PAC script at this URL.
    Pac(String),
}

/// Parse `content.proxy`: `system`, `none`, a proxy URL (`http://`,
/// `https://`, `socks://`, `socks4://`, `socks5://`) or `pac+` followed by
/// the PAC script's URL, as in qutebrowser.
pub fn parse_proxy(value: &str) -> Result<Proxy, String> {
    let value = value.trim();
    match value {
        "system" => return Ok(Proxy::System),
        "none" => return Ok(Proxy::Direct),
        _ => {}
    }
    if let Some(pac) = value.strip_prefix("pac+") {
        return if ["http://", "https://", "file://"]
            .iter()
            .any(|s| pac.starts_with(s))
        {
            Ok(Proxy::Pac(pac.to_string()))
        } else {
            Err(format!(
                "{value:?}: a PAC script needs an http, https or file URL"
            ))
        };
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(format!(
            "{value:?} isn't system, none, a proxy URL (socks5://host:port) or pac+URL"
        ));
    };
    // qutebrowser's socks:// is SOCKS5, which is also what resolves names through the proxy.
    let scheme = match scheme {
        "http" | "https" | "socks4" | "socks5" => scheme,
        "socks" => "socks5",
        other => return Err(format!("{value:?}: unknown proxy scheme {other:?}")),
    };
    let host = rest.trim_end_matches('/');
    if host.is_empty() || host.contains('/') {
        return Err(format!("{value:?}: expected scheme://host:port"));
    }
    Ok(Proxy::Server(format!("{scheme}://{host}")))
}

impl Proxy {
    /// Chromium's `proxy` preference: `(mode, key, value)`.
    pub fn pref(&self) -> (&'static str, Option<(&'static str, &str)>) {
        match self {
            Proxy::System => ("system", None),
            Proxy::Direct => ("direct", None),
            Proxy::Server(server) => ("fixed_servers", Some(("server", server))),
            Proxy::Pac(url) => ("pac_script", Some(("pac_url", url))),
        }
    }
}

/// Chromium's `webrtc.ip_handling_policy` for `content.webrtc_ip_handling_policy`.
pub fn webrtc_policy(value: &str) -> &'static str {
    match value {
        "default-public-and-private-interfaces" => "default_public_and_private_interfaces",
        "default-public-interface-only" => "default_public_interface_only",
        "disable-non-proxied-udp" => "disable_non_proxied_udp",
        _ => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxies() {
        assert_eq!(parse_proxy("system"), Ok(Proxy::System));
        assert_eq!(parse_proxy("none"), Ok(Proxy::Direct));
        assert_eq!(
            parse_proxy("socks://localhost:9050/"),
            Ok(Proxy::Server("socks5://localhost:9050".into()))
        );
        assert_eq!(
            parse_proxy("http://proxy.example:3128"),
            Ok(Proxy::Server("http://proxy.example:3128".into()))
        );
        assert_eq!(
            parse_proxy("pac+https://example.com/proxy.pac"),
            Ok(Proxy::Pac("https://example.com/proxy.pac".into()))
        );
        assert!(parse_proxy("pac+ftp://x/p.pac").is_err());
        assert!(parse_proxy("ftp://x:1").is_err());
        assert!(parse_proxy("localhost:8080").is_err());
        assert!(parse_proxy("http://x:1/path").is_err());
        assert_eq!(
            Proxy::Pac("file:///p.pac".into()).pref(),
            ("pac_script", Some(("pac_url", "file:///p.pac")))
        );
    }

    #[test]
    fn webrtc_policies() {
        assert_eq!(webrtc_policy("all-interfaces"), "default");
        assert_eq!(
            webrtc_policy("disable-non-proxied-udp"),
            "disable_non_proxied_udp"
        );
    }
}
