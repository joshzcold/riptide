//! Back/forward history for a restored tab. CEF can't give a new browser the
//! history a saved tab had, so riptide keeps that tab's history itself:
//! going back or forward loads the saved page, and pages the tab goes to
//! are added as a browser would.

/// One page in a tab's history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabHistory {
    pages: Vec<Page>,
    index: usize,
    /// The next page the tab shows is the one [`TabHistory::go`] (or the
    /// restore) loaded, not a new one.
    loading_ours: bool,
}

impl TabHistory {
    /// `back` oldest first, `forward` nearest first, as saved. The tab is
    /// about to load `current`.
    pub fn restored(back: Vec<Page>, current: Page, forward: Vec<Page>) -> Self {
        let index = back.len();
        let mut pages = back;
        pages.push(current);
        pages.extend(forward);
        Self {
            pages,
            index,
            loading_ours: true,
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.pages.len()
    }

    /// Move `n` pages (negative is back), as far as the history goes.
    /// Returns the URL to load, or `None` when already at that end.
    pub fn go(&mut self, n: i64) -> Option<String> {
        let last = self.pages.len() as i64 - 1;
        let target = (self.index as i64 + n).clamp(0, last) as usize;
        if target == self.index {
            return None;
        }
        self.index = target;
        self.loading_ours = true;
        Some(self.pages[target].url.clone())
    }

    /// The tab's main frame now shows `url`. After [`TabHistory::go`] that's
    /// the page it loaded (perhaps redirected); otherwise it's a new page,
    /// which drops the pages ahead, unless it's the current one reloading.
    pub fn committed(&mut self, url: &str) {
        if std::mem::take(&mut self.loading_ours) {
            self.pages[self.index].url = url.to_string();
            return;
        }
        if self.pages[self.index].url == url {
            return;
        }
        self.pages.truncate(self.index + 1);
        self.pages.push(Page {
            url: url.to_string(),
            title: String::new(),
        });
        self.index += 1;
    }

    pub fn titled(&mut self, title: &str) {
        self.pages[self.index].title = title.to_string();
    }

    /// Oldest first.
    pub fn back(&self) -> &[Page] {
        &self.pages[..self.index]
    }

    /// Nearest first.
    pub fn forward(&self) -> &[Page] {
        &self.pages[self.index + 1..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str) -> Page {
        Page {
            url: url.into(),
            title: String::new(),
        }
    }

    fn urls(pages: &[Page]) -> Vec<&str> {
        pages.iter().map(|p| p.url.as_str()).collect()
    }

    /// Saved as a → b → [c] → d, and c is loading.
    fn restored() -> TabHistory {
        let mut h = TabHistory::restored(vec![page("a"), page("b")], page("c"), vec![page("d")]);
        h.committed("c");
        h
    }

    #[test]
    fn back_and_forward_load_the_saved_pages() {
        let mut h = restored();
        assert_eq!(h.go(-1).as_deref(), Some("b"));
        h.committed("b");
        assert_eq!(h.go(-5).as_deref(), Some("a"), "goes as far as it can");
        h.committed("a");
        assert!(!h.can_go_back());
        assert_eq!(h.go(-1), None);
        assert_eq!(h.go(3).as_deref(), Some("d"));
        h.committed("d");
        assert!(!h.can_go_forward());
        assert_eq!(urls(h.back()), ["a", "b", "c"]);
    }

    #[test]
    fn a_new_page_drops_the_pages_ahead() {
        let mut h = restored();
        h.go(-1);
        h.committed("b");
        h.committed("e");
        h.titled("E");
        assert_eq!(urls(h.back()), ["a", "b"]);
        assert!(h.forward().is_empty());
        assert!(h.can_go_back());
        h.go(-1);
        h.committed("b");
        assert_eq!(urls(h.forward()), ["e"]);
        assert_eq!(h.forward()[0].title, "E");
    }

    #[test]
    fn a_redirect_replaces_the_page_it_loaded_and_a_reload_adds_nothing() {
        let mut h = restored();
        h.go(1);
        h.committed("d2");
        h.committed("d2");
        assert_eq!(urls(h.back()), ["a", "b", "c"]);
        assert!(h.forward().is_empty());
        h.go(-1);
        h.committed("c");
        assert_eq!(urls(h.forward()), ["d2"]);
    }
}
