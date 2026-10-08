//! Network-level ad and tracker blocking with Adblock Plus filter lists
//! (EasyList and friends), using Brave's `adblock` crate.
//!
//! Lists are kept as downloaded in `<data>/adblock/lists/`, and the compiled
//! engine is cached in `<data>/adblock/engine.dat` so startup doesn't parse
//! every list again.
//!
//! Scriptlets (`##+js(...)`) and `$redirect` files come from uBlock Origin
//! (GPL-3.0), built into `resources/ubo.json` by
//! `scripts/update-adblock-resources.sh`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adblock::Engine;
use adblock::lists::{FilterFormat, FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::{PermissionMask, Resource};

pub struct Blocker {
    engine: Engine,
}

/// uBlock Origin's scriptlets and `$redirect` files.
const UBO_RESOURCES: &str = include_str!("../resources/ubo.json");

/// The uBlock Origin release they come from.
pub const UBO_VERSION: &str = include_str!("../resources/ubo.version");

fn resources() -> Vec<Resource> {
    serde_json::from_str(UBO_RESOURCES).unwrap_or_else(|e| {
        tracing::error!("the built-in adblock resources don't parse: {e}");
        Vec::new()
    })
}

/// The permission bit uBlock Origin's trusted scriptlets need
/// (`scripts/adblock-resources.mjs` sets it).
const TRUSTED: u8 = 1;

/// Whether a list may use trusted scriptlets: uBlock Origin's own lists,
/// as uBlock Origin itself trusts them. Others could run arbitrary code.
pub fn trusted_list(url: &str) -> bool {
    [
        "https://ublockorigin.github.io/uAssets/",
        "https://ublockorigin.pages.dev/",
        "https://raw.githubusercontent.com/uBlockOrigin/uAssets/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

/// What to do with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block,
    /// Answer with this `data:` URL instead (a `$redirect` rule): a harmless
    /// stand-in, so the page doesn't notice the blocking.
    Redirect(String),
    /// Load this URL instead: the same address without the tracking
    /// parameters a `$removeparam` rule takes out.
    Rewrite(String),
}

/// Element hiding for one page: CSS for its site-specific rules, and what the
/// generic class and id rules need.
pub struct Cosmetic {
    pub css: String,
    /// Scriptlets to run in the page before its own scripts, or empty.
    pub script: String,
    /// Procedural and action filters (`:has-text()`, `:remove()`), as
    /// adblock-rust's JSON, for `procedural.js`.
    pub procedural: Vec<String>,
    /// False when the lists say `#@#` generic hiding is off for the site.
    pub generic: bool,
    exceptions: HashSet<String>,
}

/// Applies procedural filters in the page; see the file.
const PROCEDURAL_JS: &str = include_str!("procedural.js");

impl Cosmetic {
    /// What runs in the page before its own scripts: the scriptlets, then
    /// the procedural filters' applier. Empty when there's neither.
    pub fn page_script(&self) -> String {
        let mut out = String::new();
        if !self.script.is_empty() {
            // uBlock Origin's scriptlets read their settings from here.
            out.push_str("(function () { const scriptletGlobals = {};\n");
            out.push_str(&self.script);
            out.push_str("\n})();\n");
        }
        out.push_str(&self.procedural_script());
        out
    }

    /// The procedural filters' applier alone, or empty.
    pub fn procedural_script(&self) -> String {
        if self.procedural.is_empty() {
            return String::new();
        }
        format!(
            "{}([{}]);\n",
            PROCEDURAL_JS.trim_end(),
            self.procedural.join(",")
        )
    }
}

/// One rule per selector: a single invalid selector would void a whole list.
fn hide_css<'a>(selectors: impl IntoIterator<Item = &'a String>) -> String {
    let mut selectors: Vec<&String> = selectors.into_iter().collect();
    selectors.sort();
    selectors
        .into_iter()
        .map(|s| format!("{s}{{display:none!important}}\n"))
        .collect()
}

/// Whether `text` is a hosts file (`0.0.0.0 ads.example.com` lines, as
/// StevenBlack's lists are) rather than an Adblock Plus list: most of its
/// first rule lines start with an IP address.
pub fn is_hosts_file(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .take(200)
        .collect();
    let hosts = lines
        .iter()
        .filter(|l| {
            let mut words = l.split_whitespace();
            matches!(words.next(), Some("0.0.0.0" | "127.0.0.1" | "::" | "::1"))
                && words.next().is_some()
        })
        .count();
    !lines.is_empty() && hosts * 10 >= lines.len() * 8
}

impl Blocker {
    /// Compile filter list texts. Returns the blocker and the number of rules
    /// (lines that aren't comments or headers).
    pub fn from_lists<'a>(lists: impl IntoIterator<Item = &'a str>) -> (Self, usize) {
        Self::from_sources(lists.into_iter().map(|text| (false, text)))
    }

    /// [`Blocker::from_lists`] with each list marked trusted or not: a
    /// trusted list may use uBlock Origin's trusted scriptlets.
    pub fn from_sources<'a>(lists: impl IntoIterator<Item = (bool, &'a str)>) -> (Self, usize) {
        let mut set = FilterSet::new(false);
        let mut rules = 0;
        for (trusted, text) in lists {
            let hosts = is_hosts_file(text);
            rules += text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('!') && !l.starts_with('['))
                .filter(|l| !(hosts && l.starts_with('#')))
                .count();
            let format = if hosts {
                FilterFormat::Hosts
            } else {
                FilterFormat::Standard
            };
            let options = ParseOptions {
                format,
                permissions: if trusted {
                    PermissionMask::from_bits(TRUSTED)
                } else {
                    PermissionMask::default()
                },
                ..ParseOptions::default()
            };
            set.add_filter_list(text.to_string(), options);
        }
        let mut engine = Engine::new_with_filter_set(set);
        engine.use_resources(resources());
        (Blocker { engine }, rules)
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.engine.serialize()
    }

    /// Load a cached engine. Fails if the cache came from another version.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        let mut engine = Engine::default();
        engine.deserialize(bytes).ok()?;
        // Resources aren't in the cache.
        engine.use_resources(resources());
        Some(Blocker { engine })
    }

    /// The site-specific element hiding for `url`.
    pub fn cosmetic(&self, url: &str) -> Cosmetic {
        let resources = self.engine.url_cosmetic_resources(url);
        Cosmetic {
            css: hide_css(&resources.hide_selectors),
            script: resources.injected_script,
            procedural: {
                let mut actions: Vec<String> = resources.procedural_actions.into_iter().collect();
                actions.sort();
                actions
            },
            generic: !resources.generichide,
            exceptions: resources.exceptions,
        }
    }

    /// CSS for the generic rules matching the classes and ids a page uses.
    pub fn generic_css(&self, classes: &[String], ids: &[String], cosmetic: &Cosmetic) -> String {
        hide_css(
            &self
                .engine
                .hidden_class_id_selectors(classes, ids, &cosmetic.exceptions),
        )
    }

    /// `kind` is a request type such as "script", "image" or "sub_frame".
    pub fn should_block(&self, url: &str, source_url: &str, kind: &str) -> bool {
        matches!(
            self.check(url, source_url, kind),
            Verdict::Block | Verdict::Redirect(_)
        )
    }

    /// What to do with a request: allow it, block it, or answer it with a
    /// stand-in file.
    pub fn check(&self, url: &str, source_url: &str, kind: &str) -> Verdict {
        let Ok(request) = Request::new(url, source_url, kind, "GET") else {
            return Verdict::Allow;
        };
        let result = self.engine.check_network_request(&request);
        if result.should_block() {
            return result.redirect.map_or(Verdict::Block, Verdict::Redirect);
        }
        result
            .rewritten_url
            .map_or(Verdict::Allow, Verdict::Rewrite)
    }
}

/// Whether `host` is one of `whitelist` or a subdomain of one.
pub fn whitelisted(host: &str, whitelist: &[String]) -> bool {
    whitelist.iter().any(|w| {
        let w = w.trim_start_matches("*.").trim_end_matches('.');
        !w.is_empty() && (host == w || host.strip_suffix(w).is_some_and(|rest| rest.ends_with('.')))
    })
}

/// Where lists and the compiled engine live.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Store {
            dir: data_dir.join("adblock"),
        }
    }

    /// The file a downloaded list is saved in, named after its URL.
    pub fn list_path(&self, url: &str) -> PathBuf {
        self.dir
            .join("lists")
            .join(format!("{}.txt", short_hash(url)))
    }

    pub fn cache_path(&self) -> PathBuf {
        self.dir.join("engine.dat")
    }

    pub fn save_list(&self, url: &str, text: &str) -> std::io::Result<()> {
        let path = self.list_path(url);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)
    }

    /// Texts of the lists that have been downloaded, in `urls` order.
    pub fn read_lists(&self, urls: &[String]) -> Vec<(bool, String)> {
        urls.iter()
            .filter_map(|u| {
                Some((
                    trusted_list(u),
                    std::fs::read_to_string(self.list_path(u)).ok()?,
                ))
            })
            .collect()
    }

    /// Compile the downloaded lists and refresh the cache.
    pub fn compile(&self, urls: &[String]) -> Option<(Blocker, usize)> {
        let texts = self.read_lists(urls);
        if texts.is_empty() {
            return None;
        }
        let (blocker, rules) = Blocker::from_sources(texts.iter().map(|(t, s)| (*t, s.as_str())));
        if let Err(e) = std::fs::write(self.cache_path(), blocker.serialize()) {
            tracing::warn!("can't cache the adblock engine: {e}");
        }
        Some((blocker, rules))
    }

    /// The cached engine, or a fresh compile when there is no usable cache.
    pub fn load(&self, urls: &[String]) -> Option<Blocker> {
        if let Some(blocker) = std::fs::read(self.cache_path())
            .ok()
            .and_then(|bytes| Blocker::deserialize(&bytes))
        {
            return Some(blocker);
        }
        self.compile(urls).map(|(blocker, _)| blocker)
    }
}

/// FNV-1a, stable across Rust versions.
fn short_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\
||ads.example.com^
/banner/*$image
@@||ads.example.com/allowed.js
||tracker.test^$third-party
";

    #[test]
    fn blocks_and_allows() {
        let (b, rules) = Blocker::from_lists([LIST]);
        assert_eq!(rules, 4);
        let page = "https://news.example.org/";
        assert!(b.should_block("https://ads.example.com/x.js", page, "script"));
        assert!(!b.should_block("https://ads.example.com/allowed.js", page, "script"));
        assert!(b.should_block("https://cdn.example.org/banner/1.png", page, "image"));
        assert!(!b.should_block("https://cdn.example.org/banner/1.js", page, "script"));
        assert!(b.should_block("https://tracker.test/p", page, "xmlhttprequest"));
        assert!(!b.should_block(
            "https://tracker.test/p",
            "https://tracker.test/",
            "xmlhttprequest"
        ));
        assert!(!b.should_block("https://example.org/app.js", page, "script"));
    }

    #[test]
    fn hosts_files_block_their_hosts() {
        let hosts = "# A hosts file\n127.0.0.1 localhost\n0.0.0.0 ads.example.com\n0.0.0.0 track.example.net # tracker\n";
        assert!(is_hosts_file(hosts));
        assert!(!is_hosts_file(LIST));
        let (b, rules) = Blocker::from_lists([hosts]);
        assert_eq!(rules, 3);
        let page = "https://news.example.org/";
        assert!(b.should_block("https://ads.example.com/x.js", page, "script"));
        assert!(b.should_block("https://track.example.net/p", page, "image"));
        assert!(!b.should_block("https://cdn.example.org/x.js", page, "script"));
    }

    #[test]
    fn element_hiding() {
        let (b, _) = Blocker::from_lists([concat!(
            "##.ad-banner\n",
            "news.example.org##.sponsored\n",
            "##div[data-ad]\n",
            "news.example.org#@#.ad-banner\n",
        )]);
        let page = b.cosmetic("https://news.example.org/today");
        assert!(page.css.contains(".sponsored{display:none!important}"));
        assert!(page.generic);
        let other = b.cosmetic("https://other.example.com/");
        assert!(!other.css.contains(".sponsored"));
        let classes = vec!["ad-banner".to_string(), "content".to_string()];
        assert!(
            b.generic_css(&classes, &[], &other)
                .contains(".ad-banner{display:none!important}")
        );
        assert!(
            b.generic_css(&classes, &[], &page).is_empty(),
            "#@# makes an exception"
        );
    }

    #[test]
    fn scriptlets_redirects_and_removeparam() {
        let list = concat!(
            "news.example.org##+js(set-constant, adsEnabled, false)\n",
            "news.example.org##+js(aopr, detectAdblock)\n",
            "||ads.example.com/ima3.js$script,redirect=google-ima.js\n",
            "||ads.example.com/pixel.gif$image,redirect=1x1.gif\n",
            "$removeparam=utm_source\n",
        );
        let (b, _) = Blocker::from_lists([list]);
        let script = b.cosmetic("https://news.example.org/a").script;
        // The scriptlets and what they need, called with the rules' arguments.
        assert!(script.contains("function setConstant("), "{script}");
        assert!(script.contains("function safeSelf("));
        assert!(
            script.contains("setConstant(\"adsEnabled\", \"false\")"),
            "{script}"
        );
        assert!(script.contains("abortOnPropertyRead(\"detectAdblock\")"));
        assert!(b.cosmetic("https://other.example.org/").script.is_empty());

        let page = "https://news.example.org/";
        let Verdict::Redirect(url) = b.check("https://ads.example.com/ima3.js", page, "script")
        else {
            panic!("not redirected");
        };
        assert!(url.starts_with("data:application/javascript;base64,"));
        let Verdict::Redirect(url) = b.check("https://ads.example.com/pixel.gif", page, "image")
        else {
            panic!("not redirected");
        };
        assert!(url.starts_with("data:image/gif;base64,"));
        // As in uBlock Origin, removeparam applies to pages and requests for data.
        for kind in ["document", "xmlhttprequest"] {
            assert_eq!(
                b.check("https://cdn.example.net/a?utm_source=x&id=1", page, kind),
                Verdict::Rewrite("https://cdn.example.net/a?id=1".into()),
                "{kind}"
            );
        }
        assert_eq!(
            b.check("https://cdn.example.net/a.js", page, "script"),
            Verdict::Allow
        );
        // The cache keeps them working.
        let b = Blocker::deserialize(&b.serialize()).unwrap();
        assert!(
            b.cosmetic("https://news.example.org/a")
                .script
                .contains("setConstant(")
        );
        assert!(UBO_VERSION.trim().starts_with("1."), "{UBO_VERSION}");
    }

    #[test]
    fn procedural_filters_reach_the_page_script() {
        let list = concat!(
            "news.example.org##.card:has-text(Sponsored)\n",
            "news.example.org##span.label:upward(2)\n",
            "news.example.org##.banner:remove()\n",
            "news.example.org##.box:style(color: red)\n",
        );
        let (b, _) = Blocker::from_lists([list]);
        let cosmetic = b.cosmetic("https://news.example.org/");
        assert_eq!(cosmetic.procedural.len(), 4, "{:?}", cosmetic.procedural);
        assert!(
            cosmetic
                .procedural
                .iter()
                .any(|p| p.contains(r#"{"type":"has-text","arg":"Sponsored"}"#)),
            "{:?}",
            cosmetic.procedural
        );
        assert!(
            cosmetic
                .procedural
                .iter()
                .any(|p| p.contains(r#""action":{"type":"remove"}"#))
        );
        let script = cosmetic.page_script();
        assert!(
            script.contains("const OPS = {") && script.ends_with("]);\n"),
            "{script}"
        );
        assert!(!script.contains("scriptletGlobals"), "no scriptlets here");
        assert!(
            b.cosmetic("https://other.example.org/")
                .page_script()
                .is_empty()
        );
    }

    #[test]
    fn only_ubos_own_lists_run_trusted_scriptlets() {
        let rule = "news.example.org##+js(trusted-set-constant, rtLevel, 3)";
        let url = "https://news.example.org/";
        let (untrusted, _) = Blocker::from_sources([(false, rule)]);
        assert!(untrusted.cosmetic(url).script.is_empty());
        let (trusted, _) = Blocker::from_sources([(true, rule)]);
        assert!(trusted.cosmetic(url).script.contains("trustedSetConstant("));
        assert!(trusted_list(
            "https://ublockorigin.github.io/uAssets/filters/filters.min.txt"
        ));
        assert!(!trusted_list("https://easylist.to/easylist/easylist.txt"));
        assert!(!trusted_list(
            "https://ublockorigin.github.io.evil.net/x.txt"
        ));
    }

    #[test]
    fn engine_round_trips_through_the_cache() {
        let (b, _) = Blocker::from_lists([LIST]);
        let b = Blocker::deserialize(&b.serialize()).unwrap();
        assert!(b.should_block("https://ads.example.com/x.js", "https://a.org/", "script"));
        assert!(Blocker::deserialize(b"not an engine").is_none());
    }

    #[test]
    fn whitelist_covers_subdomains() {
        let list = vec!["example.com".to_string(), "*.test.org".to_string()];
        assert!(whitelisted("example.com", &list));
        assert!(whitelisted("www.example.com", &list));
        assert!(!whitelisted("badexample.com", &list));
        assert!(whitelisted("a.test.org", &list));
        assert!(!whitelisted("other.org", &list));
    }

    #[test]
    fn store_compiles_saved_lists() {
        let dir = std::env::temp_dir().join(format!("rt-adblock-{}", std::process::id()));
        let store = Store::new(&dir);
        let urls = vec!["https://lists.test/a.txt".to_string()];
        assert!(store.load(&urls).is_none());
        store.save_list(&urls[0], LIST).unwrap();
        let (_, rules) = store.compile(&urls).unwrap();
        assert_eq!(rules, 4);
        assert!(store.cache_path().exists());
        let b = store.load(&urls).unwrap();
        assert!(b.should_block("https://ads.example.com/x.js", "https://a.org/", "script"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn engine_is_shareable_across_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Blocker>();
    }
}
