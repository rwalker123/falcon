mod common;

use core_sim::build_headless_app;

#[test]
fn app_initializes() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    // run a single update tick to ensure schedule executes without panic
    app.update();
}
