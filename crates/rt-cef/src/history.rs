//! What a session keeps of each tab beyond its page: the back/forward
//! history and how far it was scrolled. CEF can read a tab's history but
//! can't give one to a new browser, so a restored tab's history is kept in
//! `rt_core::tab_history` instead.

use std::sync::{Arc, Mutex};

use cef::*;
use rt_core::tab_history::{Page, TabHistory};
use rt_storage::{PageState, TabState};

use crate::shell::{self, Tab};

/// Pages kept on each side of the current one.
const KEEP_PAGES: usize = 50;

/// The eval channel takes a string, hence `String(…)`.
const SCROLL_JS: &str = "String(Math.round(scrollY))";

type Entries = Arc<Mutex<Vec<(PageState, bool)>>>;

wrap_navigation_entry_visitor! {
    struct CollectEntries {
        entries: Entries,
    }

    impl NavigationEntryVisitor {
        fn visit(
            &self,
            entry: Option<&mut NavigationEntry>,
            current: ::std::os::raw::c_int,
            _index: ::std::os::raw::c_int,
            _total: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if let Some(entry) = entry
                && let Ok(mut entries) = self.entries.lock()
            {
                let page = PageState {
                    url: CefString::from(&entry.url()).to_string(),
                    title: CefString::from(&entry.title()).to_string(),
                };
                entries.push((page, current != 0));
            }
            1
        }
    }
}

/// Split a tab's pages, oldest first, around the current one, leaving out
/// pages that can't be loaded again and keeping [`KEEP_PAGES`] on each side.
fn split(entries: Vec<(PageState, bool)>) -> (Vec<PageState>, Vec<PageState>) {
    let Some(current) = entries.iter().position(|(_, current)| *current) else {
        return Default::default();
    };
    let keep = |p: &PageState| {
        !p.url.is_empty() && !p.url.starts_with("chrome-error:") && p.url != "about:blank"
    };
    let mut back: Vec<PageState> = entries[..current]
        .iter()
        .map(|(p, _)| p.clone())
        .filter(keep)
        .collect();
    back.drain(..back.len().saturating_sub(KEEP_PAGES));
    let forward = entries[current + 1..]
        .iter()
        .map(|(p, _)| p.clone())
        .filter(keep)
        .take(KEEP_PAGES)
        .collect();
    (back, forward)
}

/// CEF's back/forward history for a tab that riptide doesn't keep itself.
/// CEF visits the entries before this returns, on the UI thread.
fn live_history(browser: &Browser) -> (Vec<PageState>, Vec<PageState>) {
    let entries: Entries = Arc::default();
    let mut visitor = CollectEntries::new(entries.clone());
    if let Some(host) = browser.host() {
        host.navigation_entries(Some(&mut visitor), 0);
    }
    let entries = entries.lock().map(|e| e.clone()).unwrap_or_default();
    split(entries)
}

fn page_state(page: &Page) -> PageState {
    PageState {
        url: page.url.clone(),
        title: page.title.clone(),
    }
}

fn page(state: &PageState) -> Page {
    Page {
        url: state.url.clone(),
        title: state.title.clone(),
    }
}

/// A tab as a session saves it.
pub fn tab_state(tab: &Tab, pinned: bool) -> TabState {
    let (back, forward) = match (&tab.history, tab.pending.is_some()) {
        (Some(h), _) => (
            h.back().iter().map(page_state).collect(),
            h.forward().iter().map(page_state).collect(),
        ),
        // A lazily restored tab hasn't loaded anything of its own yet.
        (None, true) => Default::default(),
        (None, false) => tab.browser().map(|b| live_history(&b)).unwrap_or_default(),
    };
    TabState {
        url: tab.url.clone(),
        title: tab.title.clone(),
        pinned,
        back,
        forward,
        scroll: tab.restore_scroll.or(tab.scroll_y).filter(|&y| y > 0),
    }
}

/// Give a tab that's about to load a saved tab's page the rest of what was saved.
pub fn restore(tab: &mut Tab, saved: &TabState) {
    if !saved.back.is_empty() || !saved.forward.is_empty() {
        tab.history = Some(TabHistory::restored(
            saved.back.iter().map(page).collect(),
            Page {
                url: saved.url.clone(),
                title: saved.title.clone(),
            },
            saved.forward.iter().map(page).collect(),
        ));
    }
    tab.restore_scroll = saved.scroll;
}

/// Read how far each tab is scrolled, for the next save.
pub fn read_scroll_positions() {
    let browsers: Vec<Browser> = shell::with(|s| {
        s.windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| t.pending.is_none() && t.restore_scroll.is_none())
            .filter_map(Tab::browser)
            .collect()
    })
    .unwrap_or_default();
    for browser in browsers {
        let mut target = browser.clone();
        crate::eval::eval(&browser, SCROLL_JS, move |result| {
            let Ok(y) = result.map(|text| text.parse::<u32>().ok()) else {
                return;
            };
            shell::with_tab(Some(&mut target), |s, index, _| {
                if let Some(tab) = s.tabs.get_mut(index) {
                    tab.scroll_y = y;
                }
            });
        });
    }
}

/// A restored tab's page has loaded: scroll to where it was.
pub fn restore_scroll(frame: &Frame, y: u32) {
    shell::exec_js(frame, &format!("scrollTo(0, {y})"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, current: bool) -> (PageState, bool) {
        (
            PageState {
                url: url.into(),
                title: String::new(),
            },
            current,
        )
    }

    #[test]
    fn history_splits_around_the_current_page_without_dead_pages() {
        let (back, forward) = split(vec![
            entry("about:blank", false),
            entry("https://a/", false),
            entry("chrome-error://chromewebdata/", false),
            entry("https://b/", true),
            entry("https://c/", false),
        ]);
        let urls = |pages: &[PageState]| pages.iter().map(|p| p.url.clone()).collect::<Vec<_>>();
        assert_eq!(urls(&back), ["https://a/"]);
        assert_eq!(urls(&forward), ["https://c/"]);
        assert_eq!(split(vec![entry("https://a/", false)]), Default::default());
    }

    #[test]
    fn only_the_nearest_pages_are_kept() {
        let mut entries: Vec<_> = (0..KEEP_PAGES + 5)
            .map(|i| entry(&format!("https://{i}/"), false))
            .collect();
        entries.push(entry("https://now/", true));
        let (back, _) = split(entries);
        assert_eq!(back.len(), KEEP_PAGES);
        assert_eq!(back[0].url, "https://5/");
    }
}
