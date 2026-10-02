use std::collections::HashMap;

use crate::key::{Key, KeyParseError};
use crate::mode::Mode;

#[derive(Debug, PartialEq, Eq)]
pub enum Lookup<'a> {
    Exact(&'a str),
    Partial,
    None,
}

/// Key sequence → command string bindings, per mode.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    bindings: HashMap<Mode, HashMap<Vec<Key>, String>>,
}

impl Keymap {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The qutebrowser defaults that are implemented so far.
    pub fn defaults() -> Self {
        let mut map = Self::empty();
        for (keys, cmd) in NORMAL_DEFAULTS {
            map.bind(Mode::Normal, keys, cmd)
                .expect("valid default binding");
        }
        for n in 1..=8 {
            map.bind(
                Mode::Normal,
                &format!("<Alt-{n}>"),
                &format!("tab-focus {n}"),
            )
            .expect("valid default binding");
        }
        for (keys, cmd) in COMMAND_DEFAULTS {
            map.bind(Mode::Command, keys, cmd)
                .expect("valid default binding");
        }
        map.bind(Mode::Hint, "<Escape>", "mode-leave")
            .expect("valid default binding");
        map.bind(Mode::Insert, "<Escape>", "mode-leave")
            .expect("valid default binding");
        map.bind(Mode::Passthrough, "<Shift-Escape>", "mode-leave")
            .expect("valid default binding");
        map
    }

    pub fn bind(&mut self, mode: Mode, keys: &str, command: &str) -> Result<(), KeyParseError> {
        let seq = Key::parse_sequence(keys)?;
        self.bindings
            .entry(mode)
            .or_default()
            .insert(seq, command.to_string());
        Ok(())
    }

    pub fn unbind(&mut self, mode: Mode, keys: &str) -> Result<bool, KeyParseError> {
        let seq = Key::parse_sequence(keys)?;
        Ok(self
            .bindings
            .get_mut(&mode)
            .is_some_and(|m| m.remove(&seq).is_some()))
    }

    /// An exact match wins even when longer bindings share the prefix.
    pub fn lookup(&self, mode: Mode, seq: &[Key]) -> Lookup<'_> {
        let Some(map) = self.bindings.get(&mode) else {
            return Lookup::None;
        };
        if let Some(cmd) = map.get(seq) {
            return Lookup::Exact(cmd);
        }
        if map
            .keys()
            .any(|k| k.len() > seq.len() && k.starts_with(seq))
        {
            Lookup::Partial
        } else {
            Lookup::None
        }
    }
}

const NORMAL_DEFAULTS: &[(&str, &str)] = &[
    ("j", "scroll down"),
    ("k", "scroll up"),
    ("h", "scroll left"),
    ("l", "scroll right"),
    ("<Down>", "scroll down"),
    ("<Up>", "scroll up"),
    ("gg", "scroll-to-perc 0"),
    ("G", "scroll-to-perc"),
    ("0", "scroll-to-perc --horizontal 0"),
    ("$", "scroll-to-perc --horizontal 100"),
    ("<Ctrl-d>", "scroll-page 0 0.5"),
    ("<Ctrl-u>", "scroll-page 0 -0.5"),
    ("<Ctrl-f>", "scroll-page 0 1"),
    ("<Ctrl-b>", "scroll-page 0 -1"),
    ("H", "back"),
    ("L", "forward"),
    ("r", "reload"),
    ("R", "reload -f"),
    ("<Ctrl-r>", "reload -f"),
    ("<F5>", "reload"),
    ("o", "cmd-set-text -s :open"),
    ("go", "cmd-set-text :open {url}"),
    (":", "cmd-set-text :"),
    ("i", "mode-enter insert"),
    ("<Ctrl-v>", "mode-enter passthrough"),
    ("<Escape>", "clear-keychain"),
    ("J", "tab-next"),
    ("K", "tab-prev"),
    ("gt", "tab-next"),
    ("gT", "tab-prev"),
    ("<Ctrl-PgDown>", "tab-next"),
    ("<Ctrl-PgUp>", "tab-prev"),
    ("<Alt-9>", "tab-focus -1"),
    ("g0", "tab-focus 1"),
    ("g^", "tab-focus 1"),
    ("g$", "tab-focus -1"),
    ("<Ctrl-Tab>", "tab-focus last"),
    ("<Ctrl-^>", "tab-focus last"),
    ("d", "tab-close"),
    ("<Ctrl-w>", "tab-close"),
    ("u", "undo"),
    ("<Ctrl-Shift-t>", "undo"),
    ("O", "cmd-set-text -s :open -t"),
    ("gO", "cmd-set-text :open -t -r {url}"),
    ("<Ctrl-t>", "open -t"),
    ("gJ", "tab-move +"),
    ("gK", "tab-move -"),
    ("gm", "tab-move"),
    ("co", "tab-only"),
    ("f", "hint"),
    ("F", "hint all tab"),
    (";b", "hint all tab-bg"),
    (";f", "hint all tab"),
    (";h", "hint all hover"),
    (";i", "hint images current"),
    (";I", "hint images tab"),
    (";o", "hint links fill :open {hint-url}"),
    (";O", "hint links fill :open -t -r {hint-url}"),
    (";r", "hint --rapid links tab-bg"),
    (";t", "hint inputs"),
    (";y", "hint links yank"),
    ("yy", "yank"),
    ("yt", "yank title"),
    ("yd", "yank domain"),
    ("pp", "open -- {clipboard}"),
    ("PP", "open -t -- {clipboard}"),
    ("ZQ", "quit"),
    ("ZZ", "quit"),
    ("<Ctrl-q>", "quit"),
];

const COMMAND_DEFAULTS: &[(&str, &str)] = &[
    ("<Escape>", "mode-leave"),
    ("<Return>", "command-accept"),
    ("<Up>", "command-history-prev"),
    ("<Down>", "command-history-next"),
    ("<Ctrl-p>", "command-history-prev"),
    ("<Ctrl-n>", "command-history-next"),
    ("<Left>", "rl-backward-char"),
    ("<Right>", "rl-forward-char"),
    ("<Ctrl-b>", "rl-backward-char"),
    ("<Ctrl-f>", "rl-forward-char"),
    ("<Home>", "rl-beginning-of-line"),
    ("<End>", "rl-end-of-line"),
    ("<Ctrl-a>", "rl-beginning-of-line"),
    ("<Ctrl-e>", "rl-end-of-line"),
    ("<Backspace>", "rl-backward-delete-char"),
    ("<Ctrl-h>", "rl-backward-delete-char"),
    ("<Delete>", "rl-delete-char"),
    ("<Ctrl-d>", "rl-delete-char"),
    ("<Ctrl-u>", "rl-unix-line-discard"),
    ("<Ctrl-k>", "rl-kill-line"),
    ("<Ctrl-w>", "rl-rubout"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(s: &str) -> Vec<Key> {
        Key::parse_sequence(s).unwrap()
    }

    #[test]
    fn lookup_exact_partial_none() {
        let map = Keymap::defaults();
        assert_eq!(
            map.lookup(Mode::Normal, &seq("j")),
            Lookup::Exact("scroll down")
        );
        assert_eq!(map.lookup(Mode::Normal, &seq("g")), Lookup::Partial);
        assert_eq!(
            map.lookup(Mode::Normal, &seq("gg")),
            Lookup::Exact("scroll-to-perc 0")
        );
        assert_eq!(map.lookup(Mode::Normal, &seq("gx")), Lookup::None);
        assert_eq!(map.lookup(Mode::Insert, &seq("j")), Lookup::None);
    }

    #[test]
    fn bind_and_unbind() {
        let mut map = Keymap::empty();
        map.bind(Mode::Normal, "<Ctrl-x>", "quit").unwrap();
        assert_eq!(
            map.lookup(Mode::Normal, &seq("<Ctrl-x>")),
            Lookup::Exact("quit")
        );
        assert!(map.unbind(Mode::Normal, "<Ctrl-x>").unwrap());
        assert_eq!(map.lookup(Mode::Normal, &seq("<Ctrl-x>")), Lookup::None);
    }
}
