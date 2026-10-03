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
pub mod downloads;
pub mod greasemonkey;
pub mod lua;
pub mod lua_types;
pub mod paths;
pub mod remote;
pub mod sandbox;
pub mod toml_file;
pub mod userscripts;

use std::collections::{BTreeMap, BTreeSet};

use hb_core::config::ConfigOp;
use hb_core::settings::Settings;

pub use autoconfig::AutoConfig;
pub use cli::Cli;
pub use paths::{Paths, Platform};

pub struct Loaded {
    pub ops: Vec<ConfigOp>,
    /// Settings that `config.toml` or `config.lua` set, which override `:set`.
    pub overridden: BTreeSet<String>,
    /// Which file last set each setting, for the help page.
    pub sources: BTreeMap<String, String>,
    /// Config files that exist and were read.
    pub files: Vec<std::path::PathBuf>,
    pub errors: Vec<String>,
    pub autoconfig: AutoConfig,
}

pub fn load(paths: &Paths) -> Loaded {
    let (autoconfig, mut ops, mut errors) = AutoConfig::load(&paths.autoconfig());
    let auto_ops = ops.len();
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    if paths.autoconfig().exists() {
        files.push(paths.autoconfig());
    }

    let toml_path = paths.config_toml();
    match std::fs::read_to_string(&toml_path) {
        Ok(text) => {
            let (more, errs) = toml_file::parse(&text, &toml_path.display().to_string());
            ops.extend(more);
            errors.extend(errs);
            files.push(toml_path.clone());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => errors.push(format!("{}: {e}", toml_path.display())),
    }

    let toml_ops = ops.len();
    let lua_path = paths.config_lua();
    if lua_path.exists() {
        files.push(lua_path.clone());
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

    let mut sources = BTreeMap::new();
    for (i, op) in ops.iter().enumerate() {
        if let ConfigOp::Set { name, .. } = op {
            let file = match i {
                i if i < auto_ops => "autoconfig.toml",
                i if i < toml_ops => "config.toml",
                _ => "config.lua",
            };
            sources.insert(name.clone(), file.to_string());
        }
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
        sources,
        files,
        errors,
        autoconfig,
    }
}
