use crate::mode::Mode;
use crate::settings::Value;

/// One configuration change, produced by every config source (TOML, Lua,
/// autoconfig, `:set`/`:bind`) and applied the same way.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigOp {
    Set {
        name: String,
        value: Value,
    },
    /// A setting for pages matching a URL pattern.
    SetFor {
        pattern: String,
        name: String,
        value: Value,
    },
    Bind {
        mode: Mode,
        keys: String,
        command: String,
    },
    Unbind {
        mode: Mode,
        keys: String,
    },
    /// Back to the default value.
    Unset {
        name: String,
    },
    /// Forget a setting's value for one URL pattern.
    UnsetFor {
        pattern: String,
        name: String,
    },
}
