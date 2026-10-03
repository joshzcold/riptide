use std::fmt;
use std::str::FromStr;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Passthrough,
    Hint,
    /// Typing an answer to a prompt.
    Prompt,
    /// Answering a yes/no question.
    YesNo,
    /// The next key names a mark to set (after `` ` ``).
    #[serde(rename = "set_mark")]
    SetMark,
    /// The next key names a mark to jump to (after `'`).
    #[serde(rename = "jump_mark")]
    JumpMark,
}

impl Mode {
    pub const ALL: [Mode; 9] = [
        Mode::Normal,
        Mode::Insert,
        Mode::Command,
        Mode::Passthrough,
        Mode::Hint,
        Mode::Prompt,
        Mode::YesNo,
        Mode::SetMark,
        Mode::JumpMark,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Insert => "insert",
            Mode::Command => "command",
            Mode::Passthrough => "passthrough",
            Mode::Hint => "hint",
            Mode::Prompt => "prompt",
            Mode::YesNo => "yesno",
            Mode::SetMark => "set_mark",
            Mode::JumpMark => "jump_mark",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Mode::ALL
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| format!("unknown mode: {s}"))
    }
}
