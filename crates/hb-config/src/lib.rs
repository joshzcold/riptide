//! Configuration loading: platform paths, the command line, and the config
//! files, which are read in this order (later files win):
//!
//! 1. `autoconfig.toml`: written by `:set`, `:bind` and `:unbind`
//! 2. `config.toml`: declarative settings and bindings
//! 3. `config.lua`: the same, programmable
//!
//! Every source becomes a list of [`ConfigOp`]s that the engine applies.

pub mod autoconfig;
pub mod cli;
pub mod lua;
pub mod lua_types;
pub mod paths;
pub mod toml_file;

use std::collections::BTreeSet;

use hb_core::config::ConfigOp;
use hb_core::settings::Settings;

pub use autoconfig::AutoConfig;
pub use cli::Cli;
pub use paths::{Paths, Platform};

pub struct Loaded {
    pub ops: Vec<ConfigOp>,
    /// Settings that `config.toml` or `config.lua` set, which override `:set`.
    pub overridden: BTreeSet<String>,
    pub errors: Vec<String>,
    pub autoconfig: AutoConfig,
}

pub fn load(paths: &Paths) -> Loaded {
    let (autoconfig, mut ops, mut errors) = AutoConfig::load(&paths.autoconfig());
    let auto_ops = ops.len();

    let toml_path = paths.config_toml();
    match std::fs::read_to_string(&toml_path) {
        Ok(text) => {
            let (more, errs) = toml_file::parse(&text, &toml_path.display().to_string());
            ops.extend(more);
            errors.extend(errs);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => errors.push(format!("{}: {e}", toml_path.display())),
    }

    let lua_path = paths.config_lua();
    if lua_path.exists() {
        // Let `hb.get` see what the earlier files set.
        let mut settings = Settings::default();
        for op in &ops {
            if let ConfigOp::Set { name, value } = op {
                let _ = settings.set(name, value.clone());
            }
        }
        let (more, error) = lua::run(&lua_path, paths, settings);
        ops.extend(more);
        errors.extend(error);
    }

    let overridden = ops[auto_ops..]
        .iter()
        .filter_map(|op| match op {
            ConfigOp::Set { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for error in &errors {
        tracing::warn!("config: {error}");
    }
    Loaded {
        ops,
        overridden,
        errors,
        autoconfig,
    }
}
