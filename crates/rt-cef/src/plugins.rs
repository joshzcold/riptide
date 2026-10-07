//! Plugins from `rt.pack.add`: once config.lua has run, each is loaded in
//! its sandbox, after you approve any permissions it asks for that you
//! haven't approved before.

use rt_config::plugins::{Lockfile, Manifest, Permissions};
use rt_core::engine::Level;
use rt_core::prompt::{PromptAnswer, PromptKind, Remember, Topic};

use crate::prompts::{self, Scope};
use crate::shell;

/// Load every plugin `rt.pack.add` asked for, asking about permissions first.
pub fn start() {
    let Some((config_dir, data_dir)) =
        shell::with(|s| (s.paths.config_dir.clone(), s.paths.data_dir.clone()))
    else {
        return;
    };
    for spec in rt_config::lua::plugin_specs() {
        let dir = spec
            .dir
            .clone()
            .unwrap_or_else(|| data_dir.join("pack").join(&spec.name));
        if !dir.is_dir() {
            shell::show_message(
                Level::Error,
                format!("Plugin {}: {} doesn't exist", spec.name, dir.display()),
            );
            continue;
        }
        let manifest = match Manifest::read(&dir) {
            Ok(manifest) => manifest,
            Err(e) => {
                shell::show_message(Level::Error, format!("Plugin {}: {e}", spec.name));
                continue;
            }
        };
        let wanted = manifest.permissions;
        let approved = Lockfile::load(&config_dir)
            .ok()
            .and_then(|lock| lock.plugins.get(&spec.name).map(|l| l.approved.clone()))
            .unwrap_or_default();
        let needed = wanted.beyond(&approved);
        if spec.trusted || needed.is_empty() {
            load(&spec.name, &dir, &wanted, spec.trusted);
        } else {
            ask(spec.name, dir, wanted, needed, config_dir.clone());
        }
    }
}

/// Ask whether `name` may do what it asks; yes saves that and loads it.
fn ask(
    name: String,
    dir: std::path::PathBuf,
    wanted: Permissions,
    needed: Permissions,
    config_dir: std::path::PathBuf,
) {
    let list: String = needed
        .describe()
        .iter()
        .map(|line| format!("\n  • {line}"))
        .collect();
    let message = format!("The plugin {name} asks to:{list}\nAllow it?");
    let kind = PromptKind::YesNo {
        default: false,
        remember: Remember::Never,
    };
    prompts::ask(
        None,
        Scope::Other,
        Topic::Confirm,
        "Plugin permissions",
        message,
        kind,
        move |answer| {
            if !matches!(answer, PromptAnswer::Yes { .. }) {
                shell::show_message(
                    Level::Warning,
                    format!("Didn't load {name}: its permissions weren't approved"),
                );
                return;
            }
            let saved = Lockfile::load(&config_dir).and_then(|mut lock| {
                lock.plugins.entry(name.clone()).or_default().approved = wanted.clone();
                lock.save(&config_dir)
            });
            if let Err(e) = saved {
                shell::show_message(Level::Error, format!("Plugin {name}: {e}"));
            }
            load(&name, &dir, &wanted, false);
        },
    );
}

fn load(name: &str, dir: &std::path::Path, permissions: &Permissions, trusted: bool) {
    let context = crate::lua::current_context();
    let result = rt_config::lua::load_plugin(name, dir, permissions, trusted, &context);
    crate::lua::carry_out_for(name, result);
    // Commands it defined complete and run like the others.
    let commands = rt_config::lua::user_commands();
    shell::with(|s| s.engine.set_user_commands(commands));
    shell::refresh_ui();
}
