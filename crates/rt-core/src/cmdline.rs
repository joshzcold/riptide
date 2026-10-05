use crate::command::Readline;

/// Single-line text buffer with readline-style editing. `cursor` is a char index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineEditor {
    text: String,
    cursor: usize,
    /// What the last kill deleted, for `rl-yank`.
    killed: String,
}

impl LineEditor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, c: char) {
        let at = self.byte_index(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
    }

    pub fn apply(&mut self, action: Readline) {
        match action {
            Readline::BackwardChar => self.cursor = self.cursor.saturating_sub(1),
            Readline::ForwardChar => self.cursor = (self.cursor + 1).min(self.len()),
            Readline::BeginningOfLine => self.cursor = 0,
            Readline::EndOfLine => self.cursor = self.len(),
            Readline::BackwardDeleteChar => {
                if self.cursor > 0 {
                    self.delete_range(self.cursor - 1, self.cursor);
                }
            }
            Readline::DeleteChar => {
                if self.cursor < self.len() {
                    self.delete_range(self.cursor, self.cursor + 1);
                }
            }
            Readline::UnixLineDiscard => self.kill(0, self.cursor),
            Readline::KillLine => self.kill(self.cursor, self.len()),
            Readline::Rubout => self.rubout(char::is_whitespace),
            Readline::FilenameRubout => self.rubout(|c| c.is_whitespace() || c == '/' || c == '\\'),
            Readline::BackwardWord => self.cursor = self.word_start(),
            Readline::ForwardWord => self.cursor = self.word_end(),
            Readline::KillWord => self.kill(self.cursor, self.word_end()),
            Readline::BackwardKillWord => self.kill(self.word_start(), self.cursor),
            Readline::Yank => {
                for c in self.killed.clone().chars() {
                    self.insert(c);
                }
            }
        }
    }

    /// Readline words: runs of letters and digits.
    fn word_start(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i > 0 && !chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        while i > 0 && chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        i
    }

    fn word_end(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && !chars[i].is_alphanumeric() {
            i += 1;
        }
        while i < chars.len() && chars[i].is_alphanumeric() {
            i += 1;
        }
        i
    }

    /// Delete a range and keep it for `rl-yank`.
    fn kill(&mut self, start: usize, end: usize) {
        if start < end {
            let (a, b) = (self.byte_index(start), self.byte_index(end));
            self.killed = self.text[a..b].to_string();
        }
        self.delete_range(start, end);
    }

    /// Delete separators before the cursor, then back to the next separator.
    fn rubout(&mut self, is_separator: impl Fn(char) -> bool) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut start = self.cursor;
        while start > 0 && is_separator(chars[start - 1]) {
            start -= 1;
        }
        while start > 0 && !is_separator(chars[start - 1]) {
            start -= 1;
        }
        self.kill(start, self.cursor);
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_index)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn delete_range(&mut self, start: usize, end: usize) {
        let (a, b) = (self.byte_index(start), self.byte_index(end));
        self.text.replace_range(a..b, "");
        self.cursor = start;
    }
}

/// Command line history with prefix-filtered navigation, like qutebrowser.
#[derive(Clone, Debug, Default)]
pub struct History {
    entries: Vec<String>,
    position: Option<usize>,
    prefix: String,
}

impl History {
    /// Keep only the newest `max` entries (`completion.cmd_history_max_items`).
    pub fn truncate(&mut self, max: usize) {
        if self.entries.len() > max {
            self.entries.drain(..self.entries.len() - max);
        }
    }

    pub fn push(&mut self, entry: &str) {
        if entry.trim().is_empty() || self.entries.last().is_some_and(|e| e == entry) {
            return;
        }
        self.entries.push(entry.to_string());
    }

    pub fn reset(&mut self) {
        self.position = None;
        self.prefix.clear();
    }

    /// Step back to an older entry starting with the text typed before browsing began.
    pub fn older(&mut self, current: &str) -> Option<&str> {
        let end = match self.position {
            None => {
                self.prefix = current.to_string();
                self.entries.len()
            }
            Some(p) => p,
        };
        let found = self.entries[..end]
            .iter()
            .rposition(|e| e.starts_with(&self.prefix))?;
        self.position = Some(found);
        Some(&self.entries[found])
    }

    /// Step forward; returns the original typed text once past the newest entry.
    pub fn newer(&mut self) -> Option<String> {
        let start = self.position? + 1;
        match self.entries[start..]
            .iter()
            .position(|e| e.starts_with(&self.prefix))
        {
            Some(offset) => {
                self.position = Some(start + offset);
                Some(self.entries[start + offset].clone())
            }
            None => {
                self.position = None;
                Some(std::mem::take(&mut self.prefix))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> LineEditor {
        let mut e = LineEditor::default();
        e.set(text);
        e
    }

    #[test]
    fn insert_and_move() {
        let mut e = editor(":opn");
        e.apply(Readline::BackwardChar);
        e.insert('e');
        assert_eq!(e.text(), ":open");
        assert_eq!(e.cursor(), 4);
    }

    #[test]
    fn deletes() {
        let mut e = editor(":open foo bar");
        e.apply(Readline::Rubout);
        assert_eq!(e.text(), ":open foo ");
        e.apply(Readline::BackwardDeleteChar);
        assert_eq!(e.text(), ":open foo");
        e.apply(Readline::BeginningOfLine);
        e.apply(Readline::DeleteChar);
        assert_eq!(e.text(), "open foo");
        e.apply(Readline::KillLine);
        assert_eq!(e.text(), "");
    }

    #[test]
    fn filename_rubout_stops_at_separators() {
        let mut e = editor("/home/me/file.txt");
        e.apply(Readline::FilenameRubout);
        assert_eq!(e.text(), "/home/me/");
        e.apply(Readline::FilenameRubout);
        assert_eq!(e.text(), "/home/");
        let mut e = editor(r"C:\Users\me");
        e.apply(Readline::FilenameRubout);
        assert_eq!(e.text(), r"C:\Users\");
    }

    #[test]
    fn handles_multibyte_text() {
        let mut e = editor(":öpen");
        e.apply(Readline::BeginningOfLine);
        e.apply(Readline::ForwardChar);
        e.apply(Readline::DeleteChar);
        assert_eq!(e.text(), ":pen");
        e.apply(Readline::UnixLineDiscard);
        assert_eq!(e.text(), "pen");
    }

    #[test]
    fn history_filters_by_prefix() {
        let mut h = History::default();
        for entry in [":open a", ":back", ":open b"] {
            h.push(entry);
        }
        assert_eq!(h.older(":open"), Some(":open b"));
        assert_eq!(h.older(":open"), Some(":open a"));
        assert_eq!(h.older(":open"), None);
        assert_eq!(h.newer().as_deref(), Some(":open b"));
        assert_eq!(h.newer().as_deref(), Some(":open"));
        assert_eq!(h.newer(), None);
    }

    #[test]
    fn words_and_yank() {
        let mut e = LineEditor::default();
        e.set("open -t foo-bar baz");
        e.apply(Readline::BackwardWord);
        assert_eq!(e.cursor(), 16);
        e.apply(Readline::BackwardWord);
        assert_eq!(e.cursor(), 12, "a hyphen ends a word");
        e.apply(Readline::KillWord);
        assert_eq!(e.text(), "open -t foo- baz");
        e.apply(Readline::BeginningOfLine);
        e.apply(Readline::ForwardWord);
        assert_eq!(e.cursor(), 4);
        e.apply(Readline::Yank);
        assert_eq!(e.text(), "openbar -t foo- baz");
        e.apply(Readline::EndOfLine);
        e.apply(Readline::BackwardKillWord);
        assert_eq!(e.text(), "openbar -t foo- ");
        e.apply(Readline::Yank);
        assert_eq!(e.text(), "openbar -t foo- baz");
    }
}
