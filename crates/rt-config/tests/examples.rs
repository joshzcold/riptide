//! The example configs in docs/ must load and apply without errors.

use rt_config::Paths;
use rt_core::{Engine, Keymap};

#[test]
fn example_configs_load_cleanly() {
    let dir = std::env::temp_dir().join(format!("rt-examples-{}", std::process::id()));
    let paths = Paths {
        config_dir: dir.join("config"),
        data_dir: dir.join("data"),
    };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        paths.config_toml(),
        include_str!("../../../docs/config.example.toml"),
    )
    .unwrap();
    std::fs::write(
        paths.config_lua(),
        include_str!("../../../docs/config.example.lua"),
    )
    .unwrap();

    let loaded = rt_config::load(&paths);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);

    let mut engine = Engine::new(Keymap::defaults());
    for op in &loaded.ops {
        engine
            .apply_config(op)
            .unwrap_or_else(|e| panic!("{op:?}: {e}"));
    }
    let settings = engine.settings();
    assert_eq!(settings.str("hints.chars"), "asdfjkl");
    let engines = settings.map("url.searchengines").unwrap();
    assert!(engines.contains_key("w") && engines.contains_key("rs"));
}
