/// Where a new tab goes, like qutebrowser's `tabs.new_position.*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// Right before the current tab.
    Prev,
    /// Right after the current tab (popups, hints).
    Next,
    /// At the start.
    First,
    /// At the end (`:open -t`).
    Last,
    /// At an index (clamped), e.g. where `undo` found the closed tab.
    At(usize),
}

/// Ordered tabs with a current one. Generic so the browser layer can store
/// CEF views while the index logic stays testable here. Pinned tabs can sit
/// anywhere, as in qutebrowser.
#[derive(Debug)]
pub struct TabList<T> {
    tabs: Vec<T>,
    /// One flag per tab.
    pinned: Vec<bool>,
    current: usize,
    previous: Option<usize>,
}

impl<T> Default for TabList<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            pinned: Vec::new(),
            current: 0,
            previous: None,
        }
    }
}

impl Position {
    /// Parse a `tabs.new_position.*` value.
    pub fn from_setting(value: &str) -> Self {
        match value {
            "prev" => Position::Prev,
            "first" => Position::First,
            "last" => Position::Last,
            _ => Position::Next,
        }
    }
}

impl<T> TabList<T> {
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current(&self) -> Option<&T> {
        self.tabs.get(self.current)
    }

    pub fn current_mut(&mut self) -> Option<&mut T> {
        self.tabs.get_mut(self.current)
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.tabs.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.tabs.get_mut(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.tabs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.tabs.iter_mut()
    }

    pub fn position(&self, pred: impl FnMut(&T) -> bool) -> Option<usize> {
        self.tabs.iter().position(pred)
    }

    pub fn pinned_count(&self) -> usize {
        self.pinned.iter().filter(|&&p| p).count()
    }

    pub fn is_pinned(&self, index: usize) -> bool {
        self.pinned.get(index).copied().unwrap_or(false)
    }

    /// Pin or unpin a tab where it is. Returns whether anything changed.
    pub fn set_pinned(&mut self, index: usize, pinned: bool) -> bool {
        match self.pinned.get_mut(index) {
            Some(flag) if *flag != pinned => {
                *flag = pinned;
                true
            }
            _ => false,
        }
    }

    /// Move any tab to `to` (clamped), pinned or not.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        self.relocate(from, to.min(self.tabs.len() - 1));
    }

    /// Move a tab, keeping `current` and `previous` on the same tabs.
    fn relocate(&mut self, from: usize, to: usize) {
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        let pinned = self.pinned.remove(from);
        self.pinned.insert(to, pinned);
        self.current = shifted(self.current, from, to);
        self.previous = self.previous.map(|p| shifted(p, from, to));
    }

    /// Insert a tab and return its index. The first tab is always focused.
    pub fn insert(&mut self, tab: T, position: Position, focus: bool) -> usize {
        let index = match position {
            _ if self.tabs.is_empty() => 0,
            Position::Prev => self.current,
            Position::Next => self.current + 1,
            Position::First => 0,
            Position::Last => self.tabs.len(),
            Position::At(i) => i.min(self.tabs.len()),
        };
        self.tabs.insert(index, tab);
        self.pinned.insert(index, false);
        if self.tabs.len() > 1 && index <= self.current {
            self.current += 1;
        }
        self.previous = self.previous.map(|p| if p >= index { p + 1 } else { p });
        if focus {
            self.focus(index);
        }
        index
    }

    /// Remove a tab. Focus moves to the right neighbour, or left at the end.
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        self.pinned.remove(index);
        self.previous = match self.previous {
            Some(p) if p == index => None,
            Some(p) if p > index => Some(p - 1),
            p => p,
        };
        if index < self.current
            || (index == self.current && self.current == self.tabs.len() && self.current > 0)
        {
            self.current -= 1;
        }
        Some(tab)
    }

    /// Focus a tab, remembering the old one for `tab-focus last`. Returns
    /// whether the current tab changed.
    pub fn focus(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.current {
            return false;
        }
        self.previous = Some(self.current);
        self.current = index;
        true
    }

    pub fn previous(&self) -> Option<usize> {
        self.previous
    }

    /// The index `n` tabs away from the current one, wrapping around.
    pub fn offset(&self, n: i64) -> usize {
        let len = self.tabs.len().max(1) as i64;
        (self.current as i64 + n).rem_euclid(len) as usize
    }

    /// `n` tabs away, wrapping around the ends only if `wrap` (`tabs.wrap`).
    pub fn offset_wrapping(&self, n: i64, wrap: bool) -> usize {
        if wrap {
            return self.offset(n);
        }
        let last = self.tabs.len().saturating_sub(1) as i64;
        (self.current as i64 + n).clamp(0, last) as usize
    }

    /// Remove a tab and pick the next current one as `tabs.select_on_remove`
    /// says, when the current tab is the one removed.
    pub fn remove_selecting(&mut self, index: usize, select: SelectOnRemove) -> Option<T> {
        let was_current = index == self.current;
        let previous = self.previous;
        let tab = self.remove(index)?;
        if was_current && !self.tabs.is_empty() {
            match select {
                SelectOnRemove::Next => {}
                SelectOnRemove::Prev => {
                    self.current = index.saturating_sub(1).min(self.tabs.len() - 1)
                }
                SelectOnRemove::LastUsed => {
                    if let Some(p) = previous.filter(|&p| p != index) {
                        self.current = if p > index { p - 1 } else { p };
                    }
                }
            }
        }
        Some(tab)
    }

    /// Move the current tab to `to` (clamped to its block), keeping it focused.
    pub fn move_current(&mut self, to: usize) {
        self.move_tab(self.current, to);
    }
}

/// Where index `i` ends up after the tab at `from` moves to `to`.
fn shifted(i: usize, from: usize, to: usize) -> usize {
    if i == from {
        to
    } else if from < i && i <= to {
        i - 1
    } else if to <= i && i < from {
        i + 1
    } else {
        i
    }
}

/// Resolve a 1-based tab number where negative values count from the end.
pub fn resolve_index(number: i64, len: usize) -> Option<usize> {
    let len = len as i64;
    let index = match number {
        n if n > 0 => n - 1,
        n if n < 0 => len + n,
        _ => return None,
    };
    (0..len).contains(&index).then_some(index as usize)
}

/// Which tab becomes current when the current one closes (`tabs.select_on_remove`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectOnRemove {
    Next,
    Prev,
    LastUsed,
}

impl SelectOnRemove {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "prev" => Self::Prev,
            "last-used" => Self::LastUsed,
            _ => Self::Next,
        }
    }
}

/// Whether `tabs.show` wants the tab bar visible. `switching` is true
/// while the delay after a tab switch hasn't run out.
pub fn bar_visible(show: &str, tab_count: usize, switching: bool) -> bool {
    match show {
        "never" => false,
        "multiple" => tab_count > 1,
        "switching" => switching,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(names: &[&'static str]) -> TabList<&'static str> {
        let mut tabs = TabList::default();
        for name in names {
            tabs.insert(*name, Position::Last, false);
        }
        tabs
    }

    fn names(tabs: &TabList<&'static str>) -> Vec<&'static str> {
        tabs.iter().copied().collect()
    }

    #[test]
    fn insert_positions() {
        let mut tabs = list(&["a", "b", "c"]);
        assert_eq!(tabs.current(), Some(&"a"));
        tabs.insert("x", Position::Next, false);
        assert_eq!(names(&tabs), ["a", "x", "b", "c"]);
        assert_eq!(tabs.current(), Some(&"a"));
        let at = tabs.insert("y", Position::Last, true);
        assert_eq!(at, 4);
        assert_eq!(tabs.current(), Some(&"y"));
        assert_eq!(tabs.previous(), Some(0));
    }

    #[test]
    fn insert_shifts_previous() {
        let mut tabs = list(&["a", "b", "c"]);
        tabs.focus(2);
        tabs.focus(0);
        tabs.insert("x", Position::Next, false);
        assert_eq!(tabs.previous(), Some(3));
        assert_eq!(tabs.get(3), Some(&"c"));
    }

    #[test]
    fn insert_at_before_current_keeps_focus() {
        let mut tabs = list(&["a", "b"]);
        tabs.focus(1);
        tabs.insert("x", Position::At(0), false);
        assert_eq!(names(&tabs), ["x", "a", "b"]);
        assert_eq!(tabs.current(), Some(&"b"));
        assert_eq!(tabs.previous(), Some(1));
        tabs.insert("y", Position::At(99), false);
        assert_eq!(tabs.get(3), Some(&"y"));
    }

    #[test]
    fn prev_and_first_positions() {
        let mut tabs = list(&["a", "b"]);
        tabs.focus(1);
        tabs.insert("p", Position::Prev, false);
        assert_eq!(names(&tabs), ["a", "p", "b"]);
        assert_eq!(tabs.current(), Some(&"b"));
        tabs.insert("f", Position::First, true);
        assert_eq!(names(&tabs), ["f", "a", "p", "b"]);
        assert_eq!(tabs.current_index(), 0);
        assert_eq!(tabs.previous(), Some(3));
    }

    #[test]
    fn remove_selects_right_then_left() {
        let mut tabs = list(&["a", "b", "c"]);
        tabs.focus(1);
        assert_eq!(tabs.remove(1), Some("b"));
        assert_eq!(tabs.current(), Some(&"c"));
        assert_eq!(tabs.remove(1), Some("c"));
        assert_eq!(tabs.current(), Some(&"a"));
        assert_eq!(tabs.remove(0), Some("a"));
        assert!(tabs.current().is_none());
        assert_eq!(tabs.remove(0), None);
    }

    #[test]
    fn remove_before_current_shifts_index() {
        let mut tabs = list(&["a", "b", "c"]);
        tabs.focus(2);
        tabs.remove(0);
        assert_eq!(tabs.current(), Some(&"c"));
        assert_eq!(tabs.current_index(), 1);
    }

    #[test]
    fn offset_wraps() {
        let mut tabs = list(&["a", "b", "c"]);
        assert_eq!(tabs.offset(1), 1);
        assert_eq!(tabs.offset(-1), 2);
        tabs.focus(2);
        assert_eq!(tabs.offset(1), 0);
        assert_eq!(tabs.offset(4), 0);
    }

    #[test]
    fn previous_tracks_last_focused() {
        let mut tabs = list(&["a", "b", "c"]);
        tabs.focus(2);
        tabs.focus(1);
        assert_eq!(tabs.previous(), Some(2));
        tabs.remove(2);
        assert_eq!(tabs.previous(), None);
    }

    #[test]
    fn move_current_clamps() {
        let mut tabs = list(&["a", "b", "c"]);
        tabs.move_current(10);
        assert_eq!(names(&tabs), ["b", "c", "a"]);
        assert_eq!(tabs.current(), Some(&"a"));
        tabs.move_current(0);
        assert_eq!(names(&tabs), ["a", "b", "c"]);
    }

    #[test]
    fn pinned_tabs_stay_where_they_are_and_move_anywhere() {
        let mut tabs = list(&["a", "b", "c", "d"]);
        tabs.focus(2);
        assert!(tabs.set_pinned(2, true));
        assert_eq!(
            names(&tabs),
            ["a", "b", "c", "d"],
            "pinning doesn't move a tab"
        );
        assert!(!tabs.set_pinned(2, true));
        assert!(tabs.set_pinned(0, true));
        assert_eq!(tabs.pinned_count(), 2);
        // An unpinned tab can sit between pinned ones, and the flags travel with their tabs.
        tabs.move_tab(3, 1);
        assert_eq!(names(&tabs), ["a", "d", "b", "c"]);
        assert!(tabs.is_pinned(0) && !tabs.is_pinned(1) && !tabs.is_pinned(2) && tabs.is_pinned(3));
        assert_eq!(tabs.current(), Some(&"c"));
        tabs.insert("x", Position::First, false);
        assert_eq!(names(&tabs), ["x", "a", "d", "b", "c"]);
        assert!(!tabs.is_pinned(0) && tabs.is_pinned(1));
        tabs.remove(1);
        assert_eq!(tabs.pinned_count(), 1);
        assert!(tabs.is_pinned(3));
        assert!(tabs.set_pinned(3, false));
        assert_eq!(tabs.pinned_count(), 0);
    }

    #[test]
    fn resolves_tab_numbers() {
        assert_eq!(resolve_index(1, 3), Some(0));
        assert_eq!(resolve_index(3, 3), Some(2));
        assert_eq!(resolve_index(-1, 3), Some(2));
        assert_eq!(resolve_index(-3, 3), Some(0));
        assert_eq!(resolve_index(4, 3), None);
        assert_eq!(resolve_index(0, 3), None);
        assert_eq!(resolve_index(-4, 3), None);
    }

    #[test]
    fn tab_bar_visibility_follows_tabs_show() {
        assert!(bar_visible("always", 1, false));
        assert!(!bar_visible("never", 5, true));
        assert!(!bar_visible("multiple", 1, false));
        assert!(bar_visible("multiple", 2, false));
        assert!(bar_visible("switching", 3, true));
        assert!(!bar_visible("switching", 3, false));
    }

    #[test]
    fn select_on_remove_and_wrap() {
        let tabs = |current: usize| {
            let mut t = TabList::default();
            for i in 0..4 {
                t.insert(i, Position::Last, false);
            }
            t.focus(1);
            t.focus(current);
            t
        };
        let mut t = tabs(2);
        t.remove_selecting(2, SelectOnRemove::Next);
        assert_eq!(t.current().copied(), Some(3));
        let mut t = tabs(2);
        t.remove_selecting(2, SelectOnRemove::Prev);
        assert_eq!(t.current().copied(), Some(1));
        let mut t = tabs(3);
        t.remove_selecting(3, SelectOnRemove::LastUsed);
        assert_eq!(t.current().copied(), Some(1), "back to the tab used before");
        let mut t = tabs(2);
        t.remove_selecting(0, SelectOnRemove::Prev);
        assert_eq!(
            t.current().copied(),
            Some(2),
            "closing another tab keeps the current one"
        );
        let t = tabs(3);
        assert_eq!(t.offset_wrapping(1, true), 0);
        assert_eq!(t.offset_wrapping(1, false), 3);
        assert_eq!(tabs(0).offset_wrapping(-1, false), 0);
    }
}
