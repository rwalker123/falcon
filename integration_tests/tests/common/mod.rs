use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

pub fn ensure_test_config() {
    INIT.call_once(|| {
        let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("test_simulation_config.json");

        debug_assert!(
            config_path.exists(),
            "missing test simulation config at {}",
            config_path.display()
        );

        std::env::set_var("SIM_CONFIG_PATH", &config_path);
    });
}
