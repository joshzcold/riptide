use std::collections::HashMap;

use crate::key::{Key, KeyParseError, format_sequence};
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
    /// Where each binding came from, e.g. `default`, `config.toml` or `plugin foo`.
    sources: HashMap<(Mode, Vec<Key>), String>,
    /// The command and source a binding from another source replaced. Kept
    /// through an unbind, so a lazy plugin's placeholder passes it on.
    replaced: HashMap<(Mode, Vec<Key>), (String, String)>,
    /// Bindings for pages whose URL matches a pattern, in the order they were made.
    site: Vec<SiteBinding>,
}

/// A binding that wins over the usual one while the page matches `pattern`.
#[derive(Clone, Debug)]
pub struct SiteBinding {
    pub pattern: String,
    pub mode: Mode,
    pub keys: Vec<Key>,
    pub command: String,
    pub source: String,
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
        map.bind(Mode::Hint, "<Return>", "hint-follow")
            .expect("valid default binding");
        // Prompts edit text like the command line, minus history and completion.
        for (keys, cmd) in COMMAND_DEFAULTS
            .iter()
            .filter(|(_, c)| c.starts_with("rl-"))
        {
            map.bind(Mode::Prompt, keys, cmd)
                .expect("valid default binding");
        }
        for (keys, cmd) in PROMPT_DEFAULTS {
            map.bind(Mode::Prompt, keys, cmd)
                .expect("valid default binding");
        }
        for (keys, cmd) in YESNO_DEFAULTS {
            map.bind(Mode::YesNo, keys, cmd)
                .expect("valid default binding");
        }
        map.bind(Mode::Insert, "<Escape>", "mode-leave")
            .expect("valid default binding");
        map.bind(Mode::Insert, "<Ctrl-e>", "open-editor")
            .expect("valid default binding");
        // Alt, since pages use Ctrl-s (save) in their own editors.
        map.bind(Mode::Insert, "<Alt-s>", "spell-suggest")
            .expect("valid default binding");
        map.bind(Mode::Normal, "`", "mode-enter set_mark")
            .expect("valid default binding");
        map.bind(Mode::Normal, "'", "mode-enter jump_mark")
            .expect("valid default binding");
        map.bind(Mode::Normal, "q", "macro-record")
            .expect("valid default binding");
        map.bind(Mode::Normal, "@", "macro-run")
            .expect("valid default binding");
        for (keys, cmd) in [
            ("gu", "navigate up"),
            ("gU", "navigate up -t"),
            ("[[", "navigate prev"),
            ("]]", "navigate next"),
            ("{{", "navigate prev -t"),
            ("}}", "navigate next -t"),
            ("<Ctrl-a>", "navigate increment"),
            ("<Ctrl-x>", "navigate decrement"),
        ] {
            map.bind(Mode::Normal, keys, cmd)
                .expect("valid default binding");
        }
        for (keys, cmd) in [
            ("+", "zoom-in"),
            ("-", "zoom-out"),
            ("=", "zoom"),
            ("wi", "devtools"),
            ("<F11>", "fullscreen"),
            ("gf", "view-source"),
            ("<Alt-m>", "tab-mute"),
            (".", "repeat-command"),
        ] {
            map.bind(Mode::Normal, keys, cmd)
                .expect("valid default binding");
        }
        map.bind(Mode::Normal, "<Return>", "selection-follow")
            .expect("valid default binding");
        map.bind(Mode::Normal, "<Ctrl-Return>", "selection-follow -t")
            .expect("valid default binding");
        map.bind(Mode::Normal, "/", "cmd-set-text /")
            .expect("valid default binding");
        map.bind(Mode::Normal, "?", "cmd-set-text ?")
            .expect("valid default binding");
        map.bind(Mode::Normal, "n", "search-next")
            .expect("valid default binding");
        map.bind(Mode::Normal, "N", "search-prev")
            .expect("valid default binding");
        map.bind(Mode::Normal, "v", "mode-enter caret")
            .expect("valid default binding");
        map.bind(
            Mode::Normal,
            "V",
            "mode-enter caret ;; selection-toggle --line",
        )
        .expect("valid default binding");
        for (keys, cmd) in CARET_DEFAULTS {
            map.bind(Mode::Caret, keys, cmd)
                .expect("valid default binding");
        }
        for mode in [
            Mode::SetMark,
            Mode::JumpMark,
            Mode::RecordMacro,
            Mode::RunMacro,
        ] {
            map.bind(mode, "<Escape>", "mode-leave")
                .expect("valid default binding");
        }
        map.bind(Mode::Passthrough, "<Shift-Escape>", "mode-leave")
            .expect("valid default binding");
        // <Ctrl-j> is Return where Return accepts; in normal mode it moves between splits.
        for (mode, cmd) in [
            (Mode::Command, "command-accept"),
            (Mode::Hint, "hint-follow"),
            (Mode::Prompt, "prompt-accept"),
            (Mode::YesNo, "prompt-accept"),
            (Mode::Caret, "yank selection"),
        ] {
            map.bind(mode, "<Ctrl-j>", cmd)
                .expect("valid default binding");
        }
        for source in map.sources.values_mut() {
            *source = "default".to_string();
        }
        map
    }

    pub fn bind(&mut self, mode: Mode, keys: &str, command: &str) -> Result<(), KeyParseError> {
        self.bind_from(mode, keys, command, "")
    }

    /// Bind, noting where the binding came from; see [`Self::source`].
    pub fn bind_from(
        &mut self,
        mode: Mode,
        keys: &str,
        command: &str,
        source: &str,
    ) -> Result<(), KeyParseError> {
        let seq = Key::parse_sequence(keys)?;
        let before = self
            .bindings
            .entry(mode)
            .or_default()
            .insert(seq.clone(), command.to_string());
        let at = (mode, seq);
        let before_source = self.sources.insert(at.clone(), source.to_string());
        if let (Some(command), Some(from)) = (before, before_source)
            && from != source
        {
            self.replaced.insert(at, (command, from));
        }
        Ok(())
    }

    /// The command and source the binding for `keys` replaced, if it replaced
    /// one from elsewhere.
    pub fn replaced(&self, mode: Mode, keys: &str) -> Option<(&str, &str)> {
        let seq = Key::parse_sequence(keys).ok()?;
        let (command, source) = self.replaced.get(&(mode, seq.clone()))?;
        let now = self.sources.get(&(mode, seq))?;
        (now != source).then_some((command.as_str(), source.as_str()))
    }

    /// Where the binding for `keys` came from; empty if unknown or unbound.
    pub fn source(&self, mode: Mode, keys: &str) -> &str {
        Key::parse_sequence(keys)
            .ok()
            .and_then(|seq| self.sources.get(&(mode, seq)))
            .map_or("", String::as_str)
    }

    pub fn unbind(&mut self, mode: Mode, keys: &str) -> Result<bool, KeyParseError> {
        let seq = Key::parse_sequence(keys)?;
        self.sources.remove(&(mode, seq.clone()));
        Ok(self
            .bindings
            .get_mut(&mode)
            .is_some_and(|m| m.remove(&seq).is_some()))
    }

    /// A mode's bindings as `(keys, command)` in qutebrowser notation, sorted by keys.
    pub fn bindings(&self, mode: Mode) -> Vec<(String, String)> {
        let mut list: Vec<(String, String)> = self
            .bindings
            .get(&mode)
            .map(|m| {
                m.iter()
                    .map(|(k, c)| (format_sequence(k), c.clone()))
                    .collect()
            })
            .unwrap_or_default();
        list.sort();
        list
    }

    /// Bind `keys` for pages matching `pattern` only; a later one for the
    /// same pattern and keys replaces it.
    pub fn bind_for(
        &mut self,
        pattern: &str,
        mode: Mode,
        keys: &str,
        command: &str,
        source: &str,
    ) -> Result<(), KeyParseError> {
        let keys = Key::parse_sequence(keys)?;
        self.site
            .retain(|b| !(b.pattern == pattern && b.mode == mode && b.keys == keys));
        self.site.push(SiteBinding {
            pattern: pattern.to_string(),
            mode,
            keys,
            command: command.to_string(),
            source: source.to_string(),
        });
        Ok(())
    }

    /// A mode's per-site bindings, in the order they were made.
    pub fn site_bindings(&self, mode: Mode) -> impl Iterator<Item = &SiteBinding> {
        self.site.iter().filter(move |b| b.mode == mode)
    }

    /// The per-site bindings that apply at `url`, the last made first.
    fn site_at<'a, 'u>(
        &'a self,
        mode: Mode,
        url: &'u str,
    ) -> impl Iterator<Item = &'a SiteBinding> + use<'a, 'u> {
        self.site
            .iter()
            .rev()
            .filter(move |b| b.mode == mode && crate::url::pattern_matches(&b.pattern, url))
    }

    /// [`Self::continuations`], with the per-site bindings for `url`.
    pub fn continuations_at(&self, mode: Mode, prefix: &[Key], url: &str) -> Vec<(String, String)> {
        let mut list = self.continuations(mode, prefix);
        let mut seen = Vec::new();
        // The last made first, so an older one for the same keys doesn't win.
        for b in self.site_at(mode, url) {
            if b.keys.len() <= prefix.len() || !b.keys.starts_with(prefix) {
                continue;
            }
            let rest = format_sequence(&b.keys[prefix.len()..]);
            if seen.contains(&rest) {
                continue;
            }
            match list.iter_mut().find(|(r, _)| *r == rest) {
                Some(entry) => entry.1 = b.command.clone(),
                None => list.push((rest.clone(), b.command.clone())),
            }
            seen.push(rest);
        }
        list.sort();
        list
    }

    /// [`Self::lookup`] on a page at `url`: its per-site bindings win.
    pub fn lookup_at(&self, mode: Mode, seq: &[Key], url: &str) -> Lookup<'_> {
        if let Some(b) = self.site_at(mode, url).find(|b| b.keys == seq) {
            return Lookup::Exact(&b.command);
        }
        match self.lookup(mode, seq) {
            Lookup::None
                if self
                    .site_at(mode, url)
                    .any(|b| b.keys.len() > seq.len() && b.keys.starts_with(seq)) =>
            {
                Lookup::Partial
            }
            found => found,
        }
    }

    /// Bindings that start with `prefix` (and are longer), as `(the rest of
    /// the keys, command)` in qutebrowser notation, sorted by keys.
    pub fn continuations(&self, mode: Mode, prefix: &[Key]) -> Vec<(String, String)> {
        let mut list: Vec<(String, String)> = self
            .bindings
            .get(&mode)
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| k.len() > prefix.len() && k.starts_with(prefix))
                    .map(|(k, c)| (format_sequence(&k[prefix.len()..]), c.clone()))
                    .collect()
            })
            .unwrap_or_default();
        list.sort();
        list
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
    ("gt", "cmd-set-text -s :tab-select"),
    ("gD", "tab-give"),
    ("T", "cmd-set-text -s :tab-select"),
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
    // Splits: vim's <Ctrl-w> keys under s, and <Ctrl> with hjkl to move.
    ("ss", "split"),
    ("sv", "vsplit"),
    ("so", "only"),
    ("sc", "split-close"),
    ("sw", "split-focus next"),
    ("s=", "split-equal"),
    ("s>", "split-resize width +10"),
    ("s<Less>", "split-resize width -10"),
    ("s+", "split-resize height +10"),
    ("s-", "split-resize height -10"),
    ("<Ctrl-h>", "split-focus left"),
    ("<Ctrl-j>", "split-focus down"),
    ("<Ctrl-k>", "split-focus up"),
    ("<Ctrl-l>", "split-focus right"),
    ("cm", "call-mute"),
    ("gp", "pip"),
    ("<Ctrl-p>", "tab-pin"),
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
    (";d", "hint links download"),
    (";x", "hint blocks hide"),
    ("yy", "yank"),
    ("yt", "yank title"),
    ("yd", "yank domain"),
    ("yY", "yank -s"),
    ("yT", "yank -s title"),
    ("yD", "yank -s domain"),
    ("pp", "open -- {clipboard}"),
    ("pP", "open -- {primary}"),
    ("Pp", "open -t -- {clipboard}"),
    ("PP", "open -t -- {primary}"),
    ("m", "cmd-set-text -s :quickmark-add {url}"),
    ("b", "cmd-set-text -s :quickmark-load"),
    ("B", "cmd-set-text -s :quickmark-load -t"),
    ("M", "bookmark-add"),
    ("gb", "cmd-set-text -s :bookmark-load"),
    ("gB", "cmd-set-text -s :bookmark-load -t"),
    ("<F1>", "help"),
    ("ZQ", "quit"),
    ("ZZ", "quit --save"),
    ("<Ctrl-q>", "quit"),
];

const PROMPT_DEFAULTS: &[(&str, &str)] = &[
    ("<Tab>", "prompt-complete"),
    ("<Alt-y>", "prompt-yank"),
    ("<Ctrl-x>", "prompt-open-download"),
    ("<Alt-e>", "prompt-fileselect-external"),
    ("<Ctrl-w>", "rl-filename-rubout"),
    ("<Return>", "prompt-accept"),
    ("<Escape>", "mode-leave"),
];

const YESNO_DEFAULTS: &[(&str, &str)] = &[
    ("y", "prompt-accept yes"),
    ("n", "prompt-accept no"),
    ("A", "prompt-accept --save yes"),
    ("N", "prompt-accept --save no"),
    ("<Return>", "prompt-accept"),
    ("<Escape>", "mode-leave"),
    ("<Alt-y>", "prompt-yank"),
];

/// Caret mode, as in qutebrowser.
const CARET_DEFAULTS: &[(&str, &str)] = &[
    ("h", "move-to-prev-char"),
    ("l", "move-to-next-char"),
    ("j", "move-to-next-line"),
    ("k", "move-to-prev-line"),
    ("w", "move-to-next-word"),
    ("b", "move-to-prev-word"),
    ("e", "move-to-end-of-word"),
    ("0", "move-to-start-of-line"),
    ("$", "move-to-end-of-line"),
    ("gg", "move-to-start-of-document"),
    ("G", "move-to-end-of-document"),
    ("[", "move-to-start-of-prev-block"),
    ("]", "move-to-start-of-next-block"),
    ("{", "move-to-end-of-prev-block"),
    ("}", "move-to-end-of-next-block"),
    ("<Ctrl-Space>", "selection-drop"),
    ("v", "selection-toggle"),
    ("<Space>", "selection-toggle"),
    ("V", "selection-toggle --line"),
    ("o", "selection-reverse"),
    ("y", "yank selection"),
    ("Y", "yank -s selection"),
    ("<Return>", "yank selection"),
    ("<Escape>", "mode-leave"),
];

const COMMAND_DEFAULTS: &[(&str, &str)] = &[
    ("<Escape>", "mode-leave"),
    ("<Return>", "command-accept"),
    ("<Tab>", "completion-item-focus next"),
    ("<Shift-Tab>", "completion-item-focus prev"),
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
    ("<Ctrl-d>", "completion-item-del"),
    ("<Ctrl-c>", "completion-item-yank"),
    ("<Ctrl-Shift-c>", "completion-item-yank --sel"),
    ("<Ctrl-u>", "rl-unix-line-discard"),
    ("<Ctrl-k>", "rl-kill-line"),
    ("<Ctrl-w>", "rl-rubout"),
    ("<Alt-b>", "rl-backward-word"),
    ("<Alt-f>", "rl-forward-word"),
    ("<Alt-d>", "rl-kill-word"),
    ("<Alt-Backspace>", "rl-backward-kill-word"),
    ("<Ctrl-y>", "rl-yank"),
    ("<Ctrl-v>", "rl-paste"),
    ("<Shift-Insert>", "rl-paste --sel"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(s: &str) -> Vec<Key> {
        Key::parse_sequence(s).unwrap()
    }

    #[test]
    fn site_bindings_win_on_matching_pages() {
        let mut map = Keymap::defaults();
        map.bind_for("*.github.com", Mode::Normal, "zp", "reload", "config.lua")
            .unwrap();
        map.bind_for("*.github.com", Mode::Normal, "j", "scroll up", "config.lua")
            .unwrap();
        let gh = "https://github.com/joshzcold/riptide";
        let other = "https://example.com/";
        assert_eq!(
            map.lookup_at(Mode::Normal, &seq("zp"), gh),
            Lookup::Exact("reload")
        );
        assert_eq!(
            map.lookup_at(Mode::Normal, &seq("j"), gh),
            Lookup::Exact("scroll up")
        );
        assert_eq!(
            map.lookup_at(Mode::Normal, &seq("j"), other),
            Lookup::Exact("scroll down")
        );
        assert_eq!(map.lookup_at(Mode::Normal, &seq("zp"), other), Lookup::None);
        // A per-site chain makes its first key a prefix there only.
        assert_eq!(map.lookup_at(Mode::Normal, &seq("z"), gh), Lookup::Partial);
        assert_eq!(map.lookup_at(Mode::Normal, &seq("z"), other), Lookup::None);
        assert_eq!(
            map.continuations_at(Mode::Normal, &seq("z"), gh),
            [("p".to_string(), "reload".to_string())]
        );
        assert!(
            map.continuations_at(Mode::Normal, &seq("z"), other)
                .is_empty()
        );
        // The same pattern and keys again replace the first.
        map.bind_for("*.github.com", Mode::Normal, "zp", "stop", "")
            .unwrap();
        assert_eq!(
            map.lookup_at(Mode::Normal, &seq("zp"), gh),
            Lookup::Exact("stop")
        );
        assert_eq!(map.site_bindings(Mode::Normal).count(), 2);
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
        assert_eq!(
            map.lookup(Mode::Insert, &seq("<Alt-s>")),
            Lookup::Exact("spell-suggest")
        );
    }

    #[test]
    fn ctrl_j_moves_between_splits_but_accepts_elsewhere() {
        let map = Keymap::defaults();
        assert_eq!(
            map.lookup(Mode::Normal, &seq("<Ctrl-j>")),
            Lookup::Exact("split-focus down")
        );
        assert_eq!(
            map.lookup(Mode::Command, &seq("<Ctrl-j>")),
            Lookup::Exact("command-accept")
        );
        assert_eq!(
            map.lookup(Mode::Prompt, &seq("<Ctrl-j>")),
            Lookup::Exact("prompt-accept")
        );
        assert_eq!(
            map.lookup(Mode::Normal, &seq("sv")),
            Lookup::Exact("vsplit")
        );
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
