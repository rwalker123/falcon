//! **A STAGED `supply_network` TUNING OVERRIDE REACHES THE RESOURCE THE POOLING TEST READS**, and the
//! cross-config rule (`reach_tiles_with_messengers` under the trail's `holds_link_to_tiles`) is
//! refused AT THE EDIT, from either side, rather than as a boot panic at the next New Game. Its own
//! test binary: the override registry is process-global.

use core_sim::{
    build_test_app, config_override::clear_config_overrides,
    config_override::install_config_override, SupplyNetworkConfigHandle,
};
use std::sync::{Mutex, PoisonError};

use sim_runtime::commands::ConfigOverrideKind;

static REGISTRY: Mutex<()> = Mutex::new(());

const STAGED_REACH: u32 = 5;
/// The shipped trail reach is 6, so this reach is not under it.
const REACH_NOT_UNDER_THE_TRAIL: u32 = 6;
/// The trail reach a ladder patch lowers to, so the shipped messengers reach (4) is no longer under it.
const LOWERED_TRAIL_REACH: u32 = 4;

#[test]
fn a_staged_supply_network_override_changes_the_messengers_reach_a_new_world_reads() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("supply_network_override_takes_effect");
    install_config_override(
        ConfigOverrideKind::SupplyNetwork,
        &format!(r#"{{"reach_tiles_with_messengers":{STAGED_REACH}}}"#),
        &scratch,
    )
    .expect("a valid supply_network patch installs");

    let mut app = build_test_app();
    app.update();
    clear_config_overrides();

    let config = app.world.resource::<SupplyNetworkConfigHandle>().get();
    assert_eq!(
        config.pooling_reach(true),
        STAGED_REACH,
        "the seam the link test reads"
    );
    assert_eq!(
        config.pooling_reach(false),
        3,
        "an unstaged lever keeps its shipped value"
    );
}

#[test]
fn a_supply_network_override_past_the_trail_or_below_the_free_reach_is_refused() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("supply_network_override_refused");
    for patch in [
        format!(r#"{{"reach_tiles_with_messengers":{REACH_NOT_UNDER_THE_TRAIL}}}"#),
        r#"{"reach_tiles_with_messengers":2}"#.to_string(),
    ] {
        let refused = install_config_override(ConfigOverrideKind::SupplyNetwork, &patch, &scratch);
        assert!(refused.is_err(), "validate rejects {patch}");
    }
    clear_config_overrides();
}

#[test]
fn a_ladder_patch_that_lowers_the_trail_under_the_messengers_reach_is_refused() {
    let _registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    let scratch = std::env::temp_dir().join("ladder_override_trail_refused");
    let mut ladder: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_INTENSIFICATION_LADDER).unwrap();
    let trail = ladder["rungs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|rung| rung["id"] == "trail")
        .expect("the ladder has a trail rung");
    trail["route_payoff"]["holds_link_to_tiles"] = LOWERED_TRAIL_REACH.into();
    let patch = serde_json::json!({ "rungs": ladder["rungs"] }).to_string();
    let refused =
        install_config_override(ConfigOverrideKind::IntensificationLadder, &patch, &scratch);
    let message = refused
        .expect_err("the cross-config rule refuses it")
        .to_string();
    assert!(message.contains("trail"), "{message}");
    clear_config_overrides();
}
