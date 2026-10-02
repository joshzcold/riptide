/// Where a new tab goes, like qutebrowser's `tabs.new_position.*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// Right after the current tab (popups, hints).
    Next,
    /// At the end (`:open -t`).
    Last,
    /// At an index (clamped), e.g. where `undo` found the closed tab.
    At(usize),
}

/// Ordered tabs with a current one. Generic so the browser layer can store
/// CEF views while the index logic stays testable here.
#[derive(Debug)]
pub struct TabList<T> {
    tabs: Vec<T>,
    current: usize,
    previous: Option<usize>,
}

impl<T> Default for TabList<T> {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            current: 0,
            previous: None,
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

    pub fn position(&self, pred: impl FnMut(&T) -> bool) -> Option<usize> {
        self.tabs.iter().position(pred)
    }

    /// Insert a tab and return its index. The first tab is always focused.
    pub fn insert(&mut self, tab: T, position: Position, focus: bool) -> usize {
        let index = match position {
            _ if self.tabs.is_empty() => 0,
            Position::Next => self.current + 1,
            Position::Last => self.tabs.len(),
            Position::At(i) => i.min(self.tabs.len()),
        };
        self.tabs.insert(index, tab);
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

    /// Move the current tab to `to` (clamped), keeping it focused.
    pub fn move_current(&mut self, to: usize) {
        if self.tabs.is_empty() {
            return;
        }
        let to = to.min(self.tabs.len() - 1);
        let tab = self.tabs.remove(self.current);
        self.tabs.insert(to, tab);
        self.current = to;
        self.previous = None;
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
    fn resolves_tab_numbers() {
        assert_eq!(resolve_index(1, 3), Some(0));
        assert_eq!(resolve_index(3, 3), Some(2));
        assert_eq!(resolve_index(-1, 3), Some(2));
        assert_eq!(resolve_index(-3, 3), Some(0));
        assert_eq!(resolve_index(4, 3), None);
        assert_eq!(resolve_index(0, 3), None);
        assert_eq!(resolve_index(-4, 3), None);
    }
}
