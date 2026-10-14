//! **A STAGED `fauna` TUNING OVERRIDE REACHES THE RESOURCE THE HUNT READS** (#799).
//!
//! The Workbench stages `hunt.drive_engage_gain` / `hunt.drive_full_head_count` through
//! `ConfigOverrideKind::Fauna`; the next world boots on the merged file. This pins the whole path:
//! the override is installed, a freshly built app's `FaunaConfigHandle` carries the new number, and
//! `herd_engage_rate` — the seam the live take and every forecast read — sees it.
//!
//! Its own test binary: the override registry is process-global, so staging one must not leak into
//! any other test's world.

use bevy::math::UVec2;
use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, herd_engage_rate, FaunaConfigHandle, Herd,
};
use sim_runtime::commands::ConfigOverrideKind;

const SPECIES: &str = "Wild Reindeer";
const STAGED_GAIN: f32 = 1.5;
const STAGED_FULL_HEAD_COUNT: f32 = 40.0;
const SAME: f32 = 1e-4;

#[test]
fn a_staged_fauna_override_changes_the_drive_reach_a_new_world_hunts_with() {
    let scratch = std::env::temp_dir().join("fauna_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::Fauna,
        &format!(
            r#"{{"hunt":{{"drive_engage_gain":{STAGED_GAIN},"drive_full_head_count":{STAGED_FULL_HEAD_COUNT}}}}}"#
        ),
        &scratch,
    )
    .expect("a valid fauna patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    assert_eq!(fauna.hunt.drive_engage_gain, STAGED_GAIN);
    assert_eq!(fauna.hunt.drive_full_head_count, STAGED_FULL_HEAD_COUNT);

    let def = fauna
        .species_by_display(SPECIES)
        .expect("the species ships");
    let biomass = STAGED_FULL_HEAD_COUNT * def.body_mass;
    let herd = Herd::new(
        "override_probe".to_string(),
        SPECIES.to_string(),
        def.size_class,
        vec![UVec2::new(1, 1)],
        biomass,
        biomass,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    let bare = herd_engage_rate(&herd, &fauna, false);
    let driven = herd_engage_rate(&herd, &fauna, true);
    assert!(
        (driven - bare * (1.0 + STAGED_GAIN)).abs() < SAME,
        "a herd at the staged full-drive count gets the staged gain: {driven} vs {bare}"
    );
}

#[test]
fn a_fauna_override_with_an_unusable_lever_is_refused() {
    let scratch = std::env::temp_dir().join("fauna_override_refused");
    let refused = install_config_override(
        ConfigOverrideKind::Fauna,
        r#"{"hunt":{"drive_full_head_count":0}}"#,
        &scratch,
    );
    assert!(refused.is_err(), "validate rejects a zero full-drive count");
    clear_config_overrides();
}
