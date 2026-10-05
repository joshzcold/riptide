//! Browser-independent logic for riptide. Nothing here depends on CEF,
//! so modes, key handling and command parsing are tested without a browser.

pub mod changelog;
pub mod cmdline;
pub mod command;
pub mod completion;
pub mod config;
pub mod engine;
pub mod help;
pub mod hints;
pub mod key;
pub mod keymap;
pub mod mode;
pub mod path_complete;
pub mod permissions;
pub mod prompt;
pub mod settings;
pub mod shell_words;
pub mod tabs;
pub mod title;
pub mod ui_message;
pub mod url;
pub mod vk;
pub mod zoom;

pub use command::Command;
pub use engine::{Effect, Engine, KeyOutcome};
pub use key::{Key, KeyCode, Modifiers};
pub use keymap::Keymap;
pub use mode::Mode;
