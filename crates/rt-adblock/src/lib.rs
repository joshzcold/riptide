//! Network-level ad and tracker blocking with Adblock Plus filter lists
//! (EasyList and friends), using Brave's `adblock` crate.
//!
//! Lists are kept as downloaded in `<data>/adblock/lists/`, and the compiled
//! engine is cached in `<data>/adblock/engine.dat` so startup doesn't parse
//! every list again.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adblock::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

pub struct Blocker {
    engine: Engine,
}

/// Element hiding for one page: CSS for its site-specific rules, and what the
/// generic class and id rules need.
pub struct Cosmetic {
    pub css: String,
    /// False when the lists say `#@#` generic hiding is off for the site.
    pub generic: bool,
    exceptions: HashSet<String>,
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

impl Blocker {
    /// Compile filter list texts. Returns the blocker and the number of rules
    /// (lines that aren't comments or headers).
    pub fn from_lists<'a>(lists: impl IntoIterator<Item = &'a str>) -> (Self, usize) {
        let mut set = FilterSet::new(false);
        let mut rules = 0;
        for text in lists {
            rules += text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('!') && !l.starts_with('['))
                .count();
            set.add_filter_list(text.to_string(), ParseOptions::default());
        }
        let engine = Engine::new_with_filter_set(set);
        (Blocker { engine }, rules)
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.engine.serialize()
    }

    /// Load a cached engine. Fails if the cache came from another version.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        let mut engine = Engine::default();
        engine.deserialize(bytes).ok()?;
        Some(Blocker { engine })
    }

    /// The site-specific element hiding for `url`.
    pub fn cosmetic(&self, url: &str) -> Cosmetic {
        let resources = self.engine.url_cosmetic_resources(url);
        Cosmetic {
            css: hide_css(&resources.hide_selectors),
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
        match Request::new(url, source_url, kind, "GET") {
            Ok(request) => self.engine.check_network_request(&request).should_block(),
            Err(_) => false,
        }
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
    pub fn read_lists(&self, urls: &[String]) -> Vec<String> {
        urls.iter()
            .filter_map(|u| std::fs::read_to_string(self.list_path(u)).ok())
            .collect()
    }

    /// Compile the downloaded lists and refresh the cache.
    pub fn compile(&self, urls: &[String]) -> Option<(Blocker, usize)> {
        let texts = self.read_lists(urls);
        if texts.is_empty() {
            return None;
        }
        let (blocker, rules) = Blocker::from_lists(texts.iter().map(String::as_str));
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
