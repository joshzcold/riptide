//! Splits `:spawn` arguments like a POSIX shell does (quotes and
//! backslashes), without running a shell.

pub fn split(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("unterminated ' quote".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        // Inside double quotes a backslash only escapes these.
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => word.push(c),
                            Some(c) => {
                                word.push('\\');
                                word.push(c);
                            }
                            None => return Err("unterminated \" quote".into()),
                        },
                        Some(c) => word.push(c),
                        None => return Err("unterminated \" quote".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    word.push(c);
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// `text` as one shell word, e.g. for putting a URL into a `:spawn` line.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Vec<String> {
        split(text).unwrap()
    }

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(
            ok("mpv  --fs https://x.org/a"),
            ["mpv", "--fs", "https://x.org/a"]
        );
        assert_eq!(
            ok(r#"notify-send 'two words' "and \"more\"""#),
            ["notify-send", "two words", "and \"more\""]
        );
        assert_eq!(ok(r"a\ b c"), ["a b", "c"]);
        assert_eq!(ok("''"), [""]);
        assert_eq!(ok(r#""a\nb""#), [r"a\nb"]);
        assert!(ok("   ").is_empty());
        assert!(split("'open").is_err());
        assert!(split("\"open").is_err());
    }

    #[test]
    fn quoting_round_trips() {
        for text in ["https://x.org/a b", "it's", "plain", ""] {
            assert_eq!(split(&quote(text)).unwrap(), [text]);
        }
    }
}
