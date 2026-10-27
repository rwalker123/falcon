//! **A STAGED `turn_pipeline` TUNING OVERRIDE REACHES THE RESOURCE THE MORALE TERMS READ.**
//!
//! The Workbench stages the terrain morale allowance and the fresh-water bonus through
//! `ConfigOverrideKind::TurnPipeline`; the next world boots on the merged file. Its own test binary:
//! the override registry is process-global.

use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, TurnPipelineConfigHandle,
};
use std::sync::{Mutex, PoisonError};

use sim_runtime::commands::ConfigOverrideKind;

/// Both tests clear the process-global registry on parallel threads; they take turns.
static REGISTRY: Mutex<()> = Mutex::new(());

const STAGED_TOLERANCE: f32 = 0.125;
const STAGED_BONUS: f32 = 0.0125;
/// A lever the patch does not name keeps its shipped value (`attrition_penalty_scale`).
const SHIPPED_PENALTY_SCALE: f32 = 0.2;

#[test]
fn a_staged_turn_pipeline_override_changes_the_levers_a_new_world_reads() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("turn_pipeline_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::TurnPipeline,
        &format!(
            r#"{{"population":{{"attrition_morale_tolerance":{STAGED_TOLERANCE},"fresh_water_morale_bonus":{STAGED_BONUS}}}}}"#
        ),
        &scratch,
    )
    .expect("a valid turn_pipeline patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let handle = app.world.resource::<TurnPipelineConfigHandle>().get();
    let population = handle.population();
    assert_eq!(
        population.attrition_morale_tolerance().to_f32(),
        STAGED_TOLERANCE
    );
    assert_eq!(population.fresh_water_morale_bonus().to_f32(), STAGED_BONUS);
    assert_eq!(
        population.attrition_penalty_scale().to_f32(),
        SHIPPED_PENALTY_SCALE
    );
}

#[test]
fn a_turn_pipeline_override_with_an_unusable_lever_is_refused() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("turn_pipeline_override_refused");
    for patch in [
        r#"{"population":{"attrition_morale_tolerance":-0.1}}"#,
        r#"{"population":{"fresh_water_morale_bonus":-0.001}}"#,
    ] {
        let refused = install_config_override(ConfigOverrideKind::TurnPipeline, patch, &scratch);
        assert!(refused.is_err(), "validate rejects {patch}");
    }
    clear_config_overrides();
}
