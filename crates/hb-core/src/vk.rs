//! Translation from Chromium key events (Windows virtual-key codes, which CEF
//! uses on every platform) into [`Key`]s.

use crate::key::{Key, KeyCode, Modifiers};

/// Raw fields of a CEF key event.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawKey {
    pub windows_key_code: i32,
    pub character: u16,
    pub unmodified_character: u16,
    pub mods: Modifiers,
}

/// Returns `None` for keys that are only modifiers or locks.
pub fn translate(raw: RawKey) -> Option<Key> {
    let vk = raw.windows_key_code;
    let code = match vk {
        0x08 => KeyCode::Backspace,
        0x09 => KeyCode::Tab,
        0x0D => KeyCode::Enter,
        0x1B => KeyCode::Escape,
        0x21 => KeyCode::PageUp,
        0x22 => KeyCode::PageDown,
        0x23 => KeyCode::End,
        0x24 => KeyCode::Home,
        0x25 => KeyCode::Left,
        0x26 => KeyCode::Up,
        0x27 => KeyCode::Right,
        0x28 => KeyCode::Down,
        0x2D => KeyCode::Insert,
        0x2E => KeyCode::Delete,
        0x70..=0x87 => KeyCode::F((vk - 0x6F) as u8),
        0x10..=0x12 | 0x14 | 0x5B..=0x5D | 0x90 | 0x91 | 0xA0..=0xA5 | 0xE5 => return None,
        _ => KeyCode::Char(character(raw)?),
    };
    Some(Key::new(code, raw.mods))
}

/// The event fields for `key`, for replaying it into a page (macros).
/// Characters outside a US layout get no key code; pages see them as text.
pub fn to_raw(key: Key) -> RawKey {
    let (vk, character) = match key.code {
        KeyCode::Backspace => (0x08, 0x08),
        KeyCode::Tab => (0x09, 0x09),
        KeyCode::Enter => (0x0D, 0x0D),
        KeyCode::Escape => (0x1B, 0x1B),
        KeyCode::PageUp => (0x21, 0),
        KeyCode::PageDown => (0x22, 0),
        KeyCode::End => (0x23, 0),
        KeyCode::Home => (0x24, 0),
        KeyCode::Left => (0x25, 0),
        KeyCode::Up => (0x26, 0),
        KeyCode::Right => (0x27, 0),
        KeyCode::Down => (0x28, 0),
        KeyCode::Insert => (0x2D, 0),
        KeyCode::Delete => (0x2E, 0x7F),
        KeyCode::F(n) => (0x6F + i32::from(n), 0),
        KeyCode::Char(c) => (us_key_code(c).unwrap_or(0), c as u16),
    };
    let unmodified = match key.code {
        KeyCode::Char(c) => c.to_lowercase().next().unwrap_or(c) as u16,
        _ => character,
    };
    RawKey {
        windows_key_code: vk,
        character,
        unmodified_character: unmodified,
        mods: key.mods,
    }
}

fn us_key_code(c: char) -> Option<i32> {
    (0x20..=0xDE).find(|&vk| us_layout(vk, false) == Some(c) || us_layout(vk, true) == Some(c))
}

fn printable(c: u16) -> Option<char> {
    char::from_u32(u32::from(c)).filter(|c| !c.is_control())
}

fn character(raw: RawKey) -> Option<char> {
    if !raw.mods.ctrl
        && !raw.mods.alt
        && let Some(c) = printable(raw.character)
    {
        return Some(c);
    }
    // With Ctrl/Alt held `character` is a control code, so use the unmodified one.
    printable(raw.unmodified_character).or_else(|| us_layout(raw.windows_key_code, raw.mods.shift))
}

/// Last-resort mapping for events that carry no character, assuming a US layout.
fn us_layout(vk: i32, shift: bool) -> Option<char> {
    let (plain, shifted) = match vk {
        0x20 => (' ', ' '),
        0x30..=0x39 => {
            let digit = char::from_u32(vk as u32)?;
            (digit, b")!@#$%^&*("[(vk - 0x30) as usize] as char)
        }
        0x41..=0x5A => {
            let upper = char::from_u32(vk as u32)?;
            (upper.to_ascii_lowercase(), upper)
        }
        0xBA => (';', ':'),
        0xBB => ('=', '+'),
        0xBC => (',', '<'),
        0xBD => ('-', '_'),
        0xBE => ('.', '>'),
        0xBF => ('/', '?'),
        0xC0 => ('`', '~'),
        0xDB => ('[', '{'),
        0xDC => ('\\', '|'),
        0xDD => (']', '}'),
        0xDE => ('\'', '"'),
        _ => return None,
    };
    Some(if shift { shifted } else { plain })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_round_trips() {
        for keys in [
            "a",
            "Z",
            "5",
            ":",
            "<Ctrl-w>",
            "<Alt-x>",
            "<Escape>",
            "<Return>",
            "<BackSpace>",
            "<F5>",
            "<Down>",
            "é",
        ] {
            let key = Key::parse_sequence(keys).unwrap()[0];
            assert_eq!(translate(to_raw(key)), Some(key), "{keys}");
        }
    }

    fn raw(vk: i32, character: char, mods: Modifiers) -> RawKey {
        RawKey {
            windows_key_code: vk,
            character: character as u16,
            unmodified_character: character as u16,
            mods,
        }
    }

    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };

    #[test]
    fn printable_characters() {
        assert_eq!(
            translate(raw(0x4A, 'j', Modifiers::NONE)),
            Some(Key::char('j'))
        );
        assert_eq!(translate(raw(0x4A, 'J', SHIFT)), Some(Key::char('J')));
        assert_eq!(translate(raw(0xBA, ':', SHIFT)), Some(Key::char(':')));
    }

    #[test]
    fn ctrl_uses_unmodified_character() {
        let mut event = raw(0x44, 'd', CTRL);
        event.character = 0x04;
        assert_eq!(translate(event), Some(Key::ctrl('d')));
    }

    #[test]
    fn falls_back_to_us_layout() {
        let event = |vk, mods| RawKey {
            windows_key_code: vk,
            mods,
            ..RawKey::default()
        };
        assert_eq!(translate(event(0xBA, SHIFT)), Some(Key::char(':')));
        assert_eq!(translate(event(0x44, CTRL)), Some(Key::ctrl('d')));
        assert_eq!(translate(event(0x34, SHIFT)), Some(Key::char('$')));
    }

    #[test]
    fn special_keys_keep_shift() {
        let key = translate(raw(0x1B, '\u{1b}', SHIFT)).unwrap();
        assert_eq!(key.code, KeyCode::Escape);
        assert!(key.mods.shift);
        assert_eq!(
            translate(raw(0x0D, '\r', Modifiers::NONE)),
            Some(Key::plain(KeyCode::Enter))
        );
    }

    #[test]
    fn ignores_modifier_keys() {
        assert_eq!(translate(raw(0x10, '\0', SHIFT)), None);
        assert_eq!(translate(raw(0xA2, '\0', CTRL)), None);
    }
}
