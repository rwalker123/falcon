//! **A STAGED `intensification_ladder` TUNING OVERRIDE REACHES THE KNOWLEDGE PACING A NEW WORLD
//! READS** - the messengers lesson's two per-practice quanta and its price. Its own test binary: the
//! override registry is process-global.

use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, LadderConfigHandle, MESSENGERS_KNOWLEDGE,
};
use std::sync::{Mutex, PoisonError};

use sim_runtime::commands::ConfigOverrideKind;

static REGISTRY: Mutex<()> = Mutex::new(());

const STAGED_PER_TURN: f32 = 1.5;
const STAGED_PER_TILE: f32 = 2.0;
const STAGED_COST: f32 = 40.0;
/// One unit of practice, so the ledger credit reads as `1 / cost`.
const ONE_PRACTICE: f32 = 1.0;
/// A sibling lever the patch does not name keeps its shipped value (`ritual_lesson_per_gathering`).
const SHIPPED_RITUAL_PER_GATHERING: f32 = 4.0;

#[test]
fn a_staged_ladder_override_changes_the_messengers_pacing_a_new_world_reads() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("ladder_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::IntensificationLadder,
        &format!(
            r#"{{"knowledge":{{"messengers_lesson_per_turn":{STAGED_PER_TURN},"messengers_lesson_per_tile":{STAGED_PER_TILE},"lesson_costs":{{"messengers":{STAGED_COST}}}}}}}"#
        ),
        &scratch,
    )
    .expect("a valid ladder patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let knowledge = &ladder.knowledge;
    assert_eq!(knowledge.messengers_lesson_per_turn, STAGED_PER_TURN);
    assert_eq!(knowledge.messengers_lesson_per_tile, STAGED_PER_TILE);
    assert_eq!(
        knowledge.ledger_credit(MESSENGERS_KNOWLEDGE, ONE_PRACTICE),
        Some(ONE_PRACTICE / STAGED_COST),
        "the price is the divisor of the one credit seam"
    );
    assert_eq!(
        knowledge.ritual_lesson_per_gathering,
        SHIPPED_RITUAL_PER_GATHERING
    );
}

#[test]
fn a_ladder_override_with_an_unusable_messengers_lever_is_refused() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("ladder_override_refused");
    for patch in [
        r#"{"knowledge":{"messengers_lesson_per_turn":-1.0}}"#,
        r#"{"knowledge":{"messengers_lesson_per_tile":-0.5}}"#,
    ] {
        let refused =
            install_config_override(ConfigOverrideKind::IntensificationLadder, patch, &scratch);
        assert!(refused.is_err(), "validate rejects {patch}");
    }
    clear_config_overrides();
}
