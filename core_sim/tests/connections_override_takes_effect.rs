//! **A STAGED `connections` TUNING OVERRIDE REACHES THE RESOURCE THE RELAY AND THE DECAY READ.**
//! Its own test binary: the override registry is process-global.

use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, ConnectionsConfigHandle,
};
use std::sync::{Mutex, PoisonError};

use sim_runtime::commands::ConfigOverrideKind;

static REGISTRY: Mutex<()> = Mutex::new(());

const STAGED_HOPS: u32 = 5;
const STAGED_DECAY: f32 = 0.005;
/// A lever the patch does not name keeps its shipped value.
const SHIPPED_DECAY: f32 = 0.02;

#[test]
fn a_staged_connections_override_changes_the_levers_a_new_world_reads() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("connections_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::Connections,
        &format!(
            r#"{{"kin_relay":{{"max_hops_with_messengers":{STAGED_HOPS}}},"strength":{{"decay_per_turn_with_messengers":{STAGED_DECAY}}}}}"#
        ),
        &scratch,
    )
    .expect("a valid connections patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    assert_eq!(
        config.kin_relay.depth(true),
        STAGED_HOPS,
        "the depth the relay reads"
    );
    assert_eq!(
        config.strength.decay(true),
        STAGED_DECAY,
        "the drain the decay reads"
    );
    assert_eq!(config.strength.decay(false), SHIPPED_DECAY);
}

#[test]
fn a_connections_override_that_hurts_rather_than_helps_is_refused() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("connections_override_refused");
    for patch in [
        r#"{"kin_relay":{"max_hops_with_messengers":1}}"#,
        r#"{"strength":{"decay_per_turn_with_messengers":0.5}}"#,
        r#"{"strength":{"decay_per_turn_with_messengers":-0.01}}"#,
    ] {
        let refused = install_config_override(ConfigOverrideKind::Connections, patch, &scratch);
        assert!(refused.is_err(), "validate rejects {patch}");
    }
    clear_config_overrides();
}
