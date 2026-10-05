use std::fmt;

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Delete,
    Tab,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
    };

    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }
}

/// A single key press. For `Char` keys, shift is folded into the character
/// itself (`J`, not `<Shift-j>`), so `mods.shift` is always false for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Modifiers,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyParseError {
    #[error("empty key sequence")]
    Empty,
    #[error("unterminated '<' in key sequence: {0}")]
    Unterminated(String),
    #[error("unknown key name: {0}")]
    UnknownKey(String),
    #[error("unknown modifier: {0}")]
    UnknownModifier(String),
}

impl Key {
    pub fn new(code: KeyCode, mods: Modifiers) -> Self {
        let mut mods = mods;
        if matches!(code, KeyCode::Char(_)) {
            mods.shift = false;
        }
        Self { code, mods }
    }

    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), Modifiers::NONE)
    }

    pub fn plain(code: KeyCode) -> Self {
        Self::new(code, Modifiers::NONE)
    }

    pub fn ctrl(c: char) -> Self {
        Self::new(
            KeyCode::Char(c),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        )
    }

    /// The character this key would type into a text field, if any.
    pub fn text(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if !self.mods.ctrl && !self.mods.alt && !self.mods.meta => Some(c),
            _ => None,
        }
    }

    pub fn digit(&self) -> Option<u32> {
        self.text().and_then(|c| c.to_digit(10))
    }

    /// Parse a qutebrowser-style key sequence such as `gg`, `<Ctrl-d>` or `;y`.
    pub fn parse_sequence(s: &str) -> Result<Vec<Key>, KeyParseError> {
        let mut keys = Vec::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '<' {
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '>' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if !closed {
                    return Err(KeyParseError::Unterminated(s.to_string()));
                }
                keys.push(parse_angle(&name)?);
            } else {
                keys.push(Key::char(c));
            }
        }
        if keys.is_empty() {
            return Err(KeyParseError::Empty);
        }
        Ok(keys)
    }
}

fn parse_angle(name: &str) -> Result<Key, KeyParseError> {
    // Split on '-' but allow a literal '-' as the final key, e.g. `<Ctrl-->`.
    let mut parts: Vec<&str> = name.split('-').collect();
    if name.ends_with("--") {
        parts.truncate(parts.len() - 2);
        parts.push("-");
    }
    let (key_name, mod_names) = parts.split_last().ok_or(KeyParseError::Empty)?;
    let mut mods = Modifiers::NONE;
    for m in mod_names {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "c" => mods.ctrl = true,
            "alt" | "mod1" | "a" | "m" => mods.alt = true,
            "shift" | "s" => mods.shift = true,
            "meta" | "super" | "mod4" | "win" => mods.meta = true,
            _ => return Err(KeyParseError::UnknownModifier(m.to_string())),
        }
    }
    let code = parse_key_name(key_name)?;
    let code = match code {
        KeyCode::Char(c) if mods.shift => KeyCode::Char(c.to_uppercase().next().unwrap_or(c)),
        other => other,
    };
    Ok(Key::new(code, mods))
}

fn parse_key_name(name: &str) -> Result<KeyCode, KeyParseError> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(KeyCode::Char(c));
    }
    let lower = name.to_ascii_lowercase();
    let code = match lower.as_str() {
        "escape" | "esc" => KeyCode::Escape,
        "return" | "enter" | "cr" => KeyCode::Enter,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" | "prior" => KeyCode::PageUp,
        "pagedown" | "pgdown" | "next" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "space" => KeyCode::Char(' '),
        "less" | "lt" => KeyCode::Char('<'),
        "greater" | "gt" => KeyCode::Char('>'),
        "minus" => KeyCode::Char('-'),
        _ => match lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
            Some(n @ 1..=24) => KeyCode::F(n),
            _ => return Err(KeyParseError::UnknownKey(name.to_string())),
        },
    };
    Ok(code)
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('<') => "Less".to_string(),
            KeyCode::Char('>') => "Greater".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Escape => "Escape".into(),
            KeyCode::Enter => "Return".into(),
            KeyCode::Backspace => "Backspace".into(),
            KeyCode::Delete => "Delete".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::Insert => "Insert".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            KeyCode::PageUp => "PgUp".into(),
            KeyCode::PageDown => "PgDown".into(),
            KeyCode::Up => "Up".into(),
            KeyCode::Down => "Down".into(),
            KeyCode::Left => "Left".into(),
            KeyCode::Right => "Right".into(),
            KeyCode::F(n) => format!("F{n}"),
        };
        let bare_char = matches!(self.code, KeyCode::Char(c) if !matches!(c, ' ' | '<' | '>'));
        if bare_char && self.mods.is_empty() {
            return f.write_str(&name);
        }
        f.write_str("<")?;
        if self.mods.ctrl {
            f.write_str("Ctrl-")?;
        }
        if self.mods.alt {
            f.write_str("Alt-")?;
        }
        if self.mods.meta {
            f.write_str("Meta-")?;
        }
        if self.mods.shift {
            f.write_str("Shift-")?;
        }
        write!(f, "{name}>")
    }
}

pub fn format_sequence(keys: &[Key]) -> String {
    keys.iter().map(Key::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_sequence() {
        let keys = Key::parse_sequence("gg").unwrap();
        assert_eq!(keys, vec![Key::char('g'), Key::char('g')]);
    }

    #[test]
    fn parses_modifiers() {
        assert_eq!(
            Key::parse_sequence("<Ctrl-d>").unwrap(),
            vec![Key::ctrl('d')]
        );
        let key = Key::parse_sequence("<ctrl-shift-t>").unwrap()[0];
        assert_eq!(key.code, KeyCode::Char('T'));
        assert!(key.mods.ctrl && !key.mods.shift);
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(
            Key::parse_sequence("<Esc>").unwrap(),
            vec![Key::plain(KeyCode::Escape)]
        );
        assert_eq!(
            Key::parse_sequence("<F12>").unwrap(),
            vec![Key::plain(KeyCode::F(12))]
        );
        let shift_esc = Key::parse_sequence("<Shift-Escape>").unwrap()[0];
        assert!(shift_esc.mods.shift);
        assert_eq!(
            Key::parse_sequence("<Ctrl-->").unwrap(),
            vec![Key::ctrl('-')]
        );
    }

    #[test]
    fn rejects_bad_sequences() {
        assert!(matches!(
            Key::parse_sequence("<Ctrl-d"),
            Err(KeyParseError::Unterminated(_))
        ));
        assert!(matches!(
            Key::parse_sequence("<Bogus>"),
            Err(KeyParseError::UnknownKey(_))
        ));
        assert!(matches!(
            Key::parse_sequence("<Hyper-x>"),
            Err(KeyParseError::UnknownModifier(_))
        ));
        assert_eq!(Key::parse_sequence(""), Err(KeyParseError::Empty));
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "gg",
            "<Ctrl-d>",
            "<Escape>",
            ";y",
            "<Space>",
            "<Shift-Escape>",
            "<Ctrl-Alt-x>",
        ] {
            let keys = Key::parse_sequence(s).unwrap();
            assert_eq!(format_sequence(&keys), s);
        }
    }
}
