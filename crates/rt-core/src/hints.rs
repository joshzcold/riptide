use std::fmt;
use std::str::FromStr;

pub const DEFAULT_HINT_CHARS: &str = "asdfghjkl";

/// The built-in `hints.selectors` groups: CSS selector lists, keyed by the
/// group name `:hint` takes. User entries are merged over these.
pub const DEFAULT_SELECTORS: &[(&str, &str)] = &[
    (
        "all",
        "a, area, textarea, select, input:not([type=hidden]), button, iframe, summary, \
         [contenteditable]:not([contenteditable=false]), [onclick], [onmousedown], \
         [role=link], [role=option], [role=button], [role=tab], [role=checkbox], \
         [role=switch], [role=menuitem], [role=menuitemcheckbox], [role=menuitemradio], \
         [role=treeitem], [aria-haspopup], [tabindex]:not([tabindex='-1'])",
    ),
    ("links", "a[href], area[href], [role=link][href]"),
    ("images", "img"),
    ("media", "audio, img, video"),
    (
        "inputs",
        "input:not([type]), input[type=text], input[type=search], input[type=email], \
         input[type=url], input[type=tel], input[type=password], input[type=number], \
         input[type=date], input[type=datetime-local], input[type=month], input[type=time], \
         input[type=week], textarea, [contenteditable]:not([contenteditable=false])",
    ),
];

/// When a hint is followed without pressing Return (`hints.auto_follow`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoFollow {
    /// Whenever only one hint is left, or a label is typed in full.
    Always,
    /// When only one hint is left (qutebrowser's default).
    #[default]
    UniqueMatch,
    /// Only when a label is typed in full.
    FullMatch,
    /// Never; Return follows.
    Never,
}

impl AutoFollow {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "always" => Self::Always,
            "full-match" => Self::FullMatch,
            "never" => Self::Never,
            _ => Self::UniqueMatch,
        }
    }
}

/// What to do with the chosen element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintTarget {
    /// Click it.
    Normal,
    /// Open its URL in a new tab, focused.
    Tab,
    /// Open its URL in a new background tab.
    TabBg,
    /// Copy its URL to the clipboard.
    Yank,
    /// Move the mouse over it.
    Hover,
    /// Put a command on the command line with `{hint-url}` filled in.
    Fill,
    /// Open its URL in the current tab.
    Current,
    /// Download its URL.
    Download,
    /// Run a program with `{hint-url}` (or the URL appended).
    Spawn,
    /// Run a userscript with `QUTE_URL` set to the URL.
    Userscript,
}

macro_rules! names {
    ($ty:ident { $($variant:ident => $name:literal),* $(,)? }) => {
        impl $ty {
            pub fn name(self) -> &'static str {
                match self { $($ty::$variant => $name),* }
            }
        }
        impl FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s {
                    $($name => Ok($ty::$variant),)*
                    _ => Err(format!("unknown {}: {s}", stringify!($ty))),
                }
            }
        }
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}

names!(HintTarget {
    Normal => "normal",
    Tab => "tab",
    TabBg => "tab-bg",
    Yank => "yank",
    Hover => "hover",
    Fill => "fill",
    Current => "current",
    Download => "download",
    Spawn => "spawn",
    Userscript => "userscript",
});

/// A parsed `:hint` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintRequest {
    /// A `hints.selectors` group name.
    pub group: String,
    pub target: HintTarget,
    /// Stay in hint mode after following, like `--rapid`.
    pub rapid: bool,
    /// Command text for the `fill` target.
    pub fill: Option<String>,
}

/// One hintable element as reported by the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintItem {
    pub url: Option<String>,
    /// Its text, lowercased, which number hints filter on.
    pub text: String,
}

/// Number hints use digits; `1` first reads more naturally than `0`.
const NUMBER_CHARS: &str = "1234567890";

/// Labels for `count` hints, using qutebrowser's scattered letter algorithm:
/// as many labels as possible are one character shorter, and no label is a
/// prefix of another.
pub fn labels(count: usize, chars: &str) -> Vec<String> {
    let chars: Vec<char> = chars.chars().collect();
    let base = chars.len();
    if count == 0 || base < 2 {
        return Vec::new();
    }
    let mut needed = 1u32;
    while base.pow(needed) < count {
        needed += 1;
    }
    let short_count = if needed > 1 {
        (base.pow(needed) - count) / base
    } else {
        0
    };
    let long_count = count - short_count;
    let mut labels: Vec<String> = (0..short_count)
        .map(|i| number_to_label(i, &chars, needed - 1))
        .collect();
    let start = short_count * base;
    labels.extend((start..start + long_count).map(|i| number_to_label(i, &chars, needed)));
    scatter(labels, base)
}

fn number_to_label(mut number: usize, chars: &[char], digits: u32) -> String {
    let base = chars.len();
    let mut label = Vec::new();
    loop {
        label.push(chars[number % base]);
        number /= base;
        if number == 0 {
            break;
        }
    }
    while label.len() < digits as usize {
        label.push(chars[0]);
    }
    label.iter().rev().collect()
}

/// Spread labels so neighbouring elements start with different characters.
fn scatter(labels: Vec<String>, buckets: usize) -> Vec<String> {
    let mut out: Vec<Vec<String>> = vec![Vec::new(); buckets];
    for (i, label) in labels.into_iter().enumerate() {
        out[i % buckets].push(label);
    }
    out.into_iter().flatten().collect()
}

/// State while hint mode is active.
#[derive(Clone, Debug)]
pub struct HintSession {
    pub request: HintRequest,
    pub items: Vec<HintItem>,
    /// One per item; empty for items hidden by the number-mode text filter.
    pub labels: Vec<String>,
    pub typed: String,
    /// `hints.mode = number`: digits pick, other characters filter by text.
    pub numbers: bool,
    /// The text typed so far in number mode.
    pub filter: String,
    pub auto_follow: AutoFollow,
    /// A match `auto_follow` didn't follow by itself; Return follows it.
    pub ready: Option<usize>,
}

/// Result of typing a character in hint mode.
#[derive(Debug, PartialEq, Eq)]
pub enum HintInput {
    /// The typed text narrowed the hints; nothing chosen yet.
    Filtered,
    /// The typed text uniquely matches this item.
    Chosen(usize),
    /// Number mode: the text filter changed which items have labels.
    Relabeled,
    /// No label starts with the typed text; the key was ignored.
    NoMatch,
    /// Matched, but `hints.auto_follow` waits for Return (`hint-follow`).
    Ready(usize),
}

impl HintSession {
    pub fn new(request: HintRequest, items: Vec<HintItem>, chars: &str) -> Self {
        let labels = labels(items.len(), chars);
        Self {
            request,
            items,
            labels,
            typed: String::new(),
            numbers: false,
            filter: String::new(),
            auto_follow: AutoFollow::default(),
            ready: None,
        }
    }

    /// Follow `index` now, or keep it ready for Return, as `auto_follow` says.
    /// `full` is true when a whole label was typed rather than the text
    /// filter narrowing to one.
    fn matched(&mut self, index: usize, full: bool) -> HintInput {
        let follow = match self.auto_follow {
            AutoFollow::Always => true,
            AutoFollow::UniqueMatch => true,
            AutoFollow::FullMatch => full,
            AutoFollow::Never => false,
        };
        if follow {
            self.ready = None;
            HintInput::Chosen(index)
        } else {
            self.ready = Some(index);
            HintInput::Ready(index)
        }
    }

    /// qutebrowser's `hints.mode = number`.
    pub fn new_numbers(request: HintRequest, items: Vec<HintItem>) -> Self {
        let mut session = Self::new(request, items, NUMBER_CHARS);
        session.numbers = true;
        session.relabel();
        session
    }

    fn visible(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|&i| self.items[i].text.contains(&self.filter))
            .collect()
    }

    /// Number the items matching the filter; the rest get no label.
    fn relabel(&mut self) {
        let visible = self.visible();
        let mut numbers = labels(visible.len(), NUMBER_CHARS).into_iter();
        self.labels = vec![String::new(); self.items.len()];
        for i in visible {
            self.labels[i] = numbers.next().unwrap_or_default();
        }
    }

    pub fn push(&mut self, c: char) -> HintInput {
        if self.numbers && !c.is_ascii_digit() {
            self.filter.extend(c.to_lowercase());
            let visible = self.visible();
            if visible.is_empty() {
                self.filter.pop();
                return HintInput::NoMatch;
            }
            self.typed.clear();
            self.ready = None;
            if let [only] = visible.as_slice()
                && let chosen @ HintInput::Chosen(_) = self.matched(*only, false)
            {
                return chosen;
            }
            self.relabel();
            return HintInput::Relabeled;
        }
        let mut typed = self.typed.clone();
        typed.push(c);
        if !self
            .labels
            .iter()
            .any(|l| !l.is_empty() && l.starts_with(&typed))
        {
            return HintInput::NoMatch;
        }
        match self.labels.iter().position(|l| *l == typed) {
            Some(index) => {
                self.typed = typed;
                match self.matched(index, true) {
                    HintInput::Chosen(index) => {
                        self.typed.clear();
                        HintInput::Chosen(index)
                    }
                    ready => ready,
                }
            }
            None => {
                self.typed = typed;
                self.ready = None;
                HintInput::Filtered
            }
        }
    }

    /// Backspace: undo the last label character, or else the last filter one.
    pub fn pop(&mut self) -> HintInput {
        self.ready = None;
        if self.typed.pop().is_some() || !self.numbers || self.filter.pop().is_none() {
            return HintInput::Filtered;
        }
        self.relabel();
        HintInput::Relabeled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_prefix_free(labels: &[String]) -> bool {
        labels.iter().enumerate().all(|(i, a)| {
            labels
                .iter()
                .enumerate()
                .all(|(j, b)| i == j || !b.starts_with(a.as_str()))
        })
    }

    #[test]
    fn single_chars_when_they_fit() {
        assert_eq!(labels(3, "asd"), ["a", "s", "d"]);
        assert_eq!(labels(1, DEFAULT_HINT_CHARS), ["a"]);
        assert!(labels(0, DEFAULT_HINT_CHARS).is_empty());
    }

    #[test]
    fn mixes_short_and_long_labels() {
        // 3 chars, 4 items: floor((9 - 4) / 3) = 1 one-char label, 3 two-char labels.
        let l = labels(4, "asd");
        assert_eq!(l.len(), 4);
        assert_eq!(l.iter().filter(|s| s.len() == 1).count(), 1);
        assert!(is_prefix_free(&l));
    }

    #[test]
    fn matches_qutebrowser_for_small_alphabet() {
        // Same output as qutebrowser's _hint_scattered(1, "abc", 5 elems).
        assert_eq!(labels(5, "abc"), ["a", "bc", "ba", "ca", "bb"]);
    }

    #[test]
    fn large_counts_are_unique_and_prefix_free() {
        for count in [9, 10, 80, 81, 82, 500] {
            let l = labels(count, DEFAULT_HINT_CHARS);
            assert_eq!(l.len(), count);
            let mut unique = l.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), count, "duplicates for {count}");
            assert!(is_prefix_free(&l), "prefix clash for {count}");
        }
    }

    #[test]
    fn session_filters_and_chooses() {
        let request = HintRequest {
            group: "all".into(),
            target: HintTarget::Normal,
            rapid: false,
            fill: None,
        };
        let items = vec![HintItem::default(); 5];
        // Labels: [a, bc, ba, ca, bb]
        let mut s = HintSession::new(request, items, "abc");
        assert_eq!(s.push('x'), HintInput::NoMatch);
        assert_eq!(s.push('c'), HintInput::Filtered);
        assert_eq!(s.typed, "c");
        s.pop();
        assert_eq!(s.push('b'), HintInput::Filtered);
        assert_eq!(s.push('a'), HintInput::Chosen(2));
        assert_eq!(s.typed, "");
    }

    #[test]
    fn number_hints_filter_by_text() {
        let request = HintRequest {
            group: "all".into(),
            target: HintTarget::Normal,
            rapid: false,
            fill: None,
        };
        let item = |text: &str| HintItem {
            url: None,
            text: text.into(),
        };
        let items = vec![item("home"), item("news"), item("new post"), item("about")];
        let mut s = HintSession::new_numbers(request, items);
        assert!(
            s.labels
                .iter()
                .all(|l| l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty())
        );
        assert_eq!(s.push('n'), HintInput::Relabeled);
        assert_eq!(s.labels.iter().filter(|l| !l.is_empty()).count(), 2);
        assert!(s.labels[0].is_empty() && s.labels[3].is_empty());
        assert_eq!(s.push('z'), HintInput::NoMatch);
        assert_eq!(s.filter, "n");
        assert_eq!(s.pop(), HintInput::Relabeled);
        assert_eq!(s.labels.iter().filter(|l| !l.is_empty()).count(), 4);
        assert_eq!(
            s.push('A'),
            HintInput::Chosen(3),
            "a single match is followed at once"
        );
        let label = s.labels[1].clone();
        let chosen = label.chars().fold(HintInput::NoMatch, |_, c| s.push(c));
        assert_eq!(chosen, HintInput::Chosen(1));
    }

    fn auto_follow_session(policy: AutoFollow, numbers: bool) -> HintSession {
        let request = HintRequest {
            group: "all".into(),
            target: HintTarget::Normal,
            rapid: false,
            fill: None,
        };
        let item = |text: &str| HintItem {
            url: None,
            text: text.into(),
        };
        let items = vec![item("home"), item("news"), item("about")];
        let mut s = if numbers {
            HintSession::new_numbers(request, items)
        } else {
            HintSession::new(request, items, "abc")
        };
        s.auto_follow = policy;
        s
    }

    #[test]
    fn auto_follow_decides_when_a_match_is_followed() {
        // A full label: followed unless the policy is never.
        for (policy, followed) in [
            (AutoFollow::UniqueMatch, true),
            (AutoFollow::FullMatch, true),
            (AutoFollow::Always, true),
            (AutoFollow::Never, false),
        ] {
            let mut s = auto_follow_session(policy, false);
            let label = s.labels[0].clone();
            let result = label.chars().fold(HintInput::NoMatch, |_, c| s.push(c));
            if followed {
                assert_eq!(result, HintInput::Chosen(0), "{policy:?}");
            } else {
                assert_eq!(result, HintInput::Ready(0), "{policy:?}");
                assert_eq!(s.ready, Some(0));
            }
        }
        // Text narrowing number hints to one: not a full match.
        for (policy, followed) in [
            (AutoFollow::UniqueMatch, true),
            (AutoFollow::Always, true),
            (AutoFollow::FullMatch, false),
            (AutoFollow::Never, false),
        ] {
            let mut s = auto_follow_session(policy, true);
            let result = s.push('b');
            if followed {
                assert_eq!(result, HintInput::Chosen(2), "{policy:?}");
            } else {
                assert_eq!(result, HintInput::Relabeled, "{policy:?}");
                assert_eq!(s.ready, Some(2), "{policy:?}");
                assert_eq!(s.pop(), HintInput::Relabeled);
                assert_eq!(s.ready, None, "backspace forgets the match");
            }
        }
    }
}
