//! **A STAGED `mobility` TUNING OVERRIDE REACHES THE RESOURCE THE SCORE READS** (#800).
//!
//! The Workbench stages the mobility levers through `ConfigOverrideKind::Mobility`; the next world
//! boots on the merged file. This pins the whole path: the override is installed and a freshly built
//! app's `MobilityConfigHandle` carries the new numbers.
//!
//! Its own test binary: the override registry is process-global, so staging one must not leak into
//! any other test's world.

use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, MobilityConfigHandle,
};
use sim_runtime::commands::ConfigOverrideKind;

const STAGED_SMOOTHING: f32 = 0.5;
const STAGED_SKILLS_WEIGHT: f32 = 0.25;
const STAGED_DISTANCE_WEIGHT: f32 = 0.75;
const STAGED_HEXES_PER_TURN: f32 = 3.0;

#[test]
fn a_staged_mobility_override_changes_the_levers_a_new_world_scores_with() {
    let scratch = std::env::temp_dir().join("mobility_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::Mobility,
        &format!(
            r#"{{"smoothing":{STAGED_SMOOTHING},"weights":{{"skills":{STAGED_SKILLS_WEIGHT},"distance":{STAGED_DISTANCE_WEIGHT}}},"references":{{"hexes_per_turn":{STAGED_HEXES_PER_TURN}}}}}"#
        ),
        &scratch,
    )
    .expect("a valid mobility patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let mobility = app.world.resource::<MobilityConfigHandle>().get();
    assert_eq!(mobility.smoothing, STAGED_SMOOTHING);
    assert_eq!(mobility.weights.skills, STAGED_SKILLS_WEIGHT);
    assert_eq!(mobility.weights.distance, STAGED_DISTANCE_WEIGHT);
    assert_eq!(mobility.references.hexes_per_turn, STAGED_HEXES_PER_TURN);
}

#[test]
fn a_mobility_override_with_an_unusable_lever_is_refused() {
    let scratch = std::env::temp_dir().join("mobility_override_refused");
    for patch in [
        r#"{"smoothing":1.0}"#,
        r#"{"weights":{"skills":-0.5}}"#,
        r#"{"references":{"hexes_per_turn":0}}"#,
    ] {
        let refused = install_config_override(ConfigOverrideKind::Mobility, patch, &scratch);
        assert!(refused.is_err(), "validate rejects {patch}");
    }
    clear_config_overrides();
}
