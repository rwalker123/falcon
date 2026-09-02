//! ⛔ **THE CLIENT MUST BE TOLD WHERE THE COLD KILLS — IT CANNOT DERIVE IT** (issue #614).
//!
//! `systems::population` kills a fraction of *every* age bracket per turn, food-independent, from a
//! model the client has never been handed:
//!
//! ```text
//! min((|tile.temperature - ambient_temperature| - cold.temp_tolerance) * cold.mortality_scale,
//!     cold.max_mortality)
//! ```
//!
//! Until now the only temperature thresholds on the wire were `MapSection.climateBands` — the
//! *biome* cut points, which are a different, unrelated set of numbers. At the shipped tuning they
//! visibly disagree: `climate.boreal_max_temp` is 3.0, so a 4 ° tile is labelled **Temperate** while
//! the mortality model kills 4.5 % of every bracket on it each turn. A client that guessed a lethal
//! threshold from the band ladder would guess wrong, so the sim publishes the model itself.
//!
//! **Read off the ENCODED buffer**, never off the capture's state struct: a constant can be right in
//! the capture and absent from the envelope, and the envelope is what a client parses.

use std::sync::Arc;

use bevy::app::App;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, DemographicsConfigHandle,
    SimulationConfig, SnapshotHistory,
};

/// f32 slack for a number that made one trip through the wire's `float` and back — a copy, not a
/// computation, so anything above representation noise is a real disagreement.
const A_COPY: f32 = 1e-6;

/// The shipped `simulation_config.json` `ambient_temperature` (°): the temperature the model treats
/// as costless. Stated here so a retuning has to come through this test and be seen.
const SHIPPED_AMBIENT_TEMP: f32 = 18.0;

/// The shipped `demographics_config.json` `cold.temp_tolerance` (°): the symmetric deviation from
/// ambient that costs nothing, so the survivable range is 6.0 °–30.0 °.
const SHIPPED_TEMP_TOLERANCE: f32 = 12.0;

/// The shipped `cold.mortality_scale`: fraction of every bracket killed per degree past tolerance.
const SHIPPED_MORTALITY_SCALE: f32 = 0.02;

/// The shipped `cold.max_mortality`: the cap that fraction is clamped to.
const SHIPPED_MAX_MORTALITY: f32 = 0.1;

/// A retuning no shipped config uses, applied to prove the publishing path READS the config rather
/// than restating the shipped numbers. Deliberately far from the shipped tuning on every axis.
const A_RETUNED_TOLERANCE: f32 = 7.5;
const A_RETUNED_MORTALITY_SCALE: f32 = 0.05;
const A_RETUNED_MAX_MORTALITY: f32 = 0.25;

/// The mortality model as the wire carries it: `(ambient, tolerance, scale, cap)`.
fn published_survivability(app: &App) -> (f32, f32, f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let model = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .map()
        .and_then(|section| section.temperatureSurvivability())
        .expect("the map section carries the temperature-survivability model");
    (
        model.ambientTemp(),
        model.tempTolerance(),
        model.mortalityScale(),
        model.maxMortality(),
    )
}

/// A world stood up and turned once, so the snapshot under test is a captured one rather than the
/// baseline placeholder.
fn a_running_world() -> App {
    let mut app = build_test_app();
    run_turn(&mut app);
    app
}

/// ⛔ **THE FOUR CONSTANTS THE SIM KILLS FROM ARRIVE ON THE WIRE, AT THE SHIPPED TUNING.**
///
/// Asserted twice over: against the named shipped values (so a retuning is visible here) and against
/// the live config resources (so the wire is pinned to what the sim itself reads, not to a copy).
#[test]
fn the_mortality_model_the_sim_enforces_is_published() {
    let app = a_running_world();
    let (ambient, tolerance, scale, cap) = published_survivability(&app);

    assert!(
        (ambient - SHIPPED_AMBIENT_TEMP).abs() < A_COPY,
        "published ambient temperature {ambient} is not the shipped {SHIPPED_AMBIENT_TEMP}"
    );
    assert!(
        (tolerance - SHIPPED_TEMP_TOLERANCE).abs() < A_COPY,
        "published tolerance {tolerance} is not the shipped {SHIPPED_TEMP_TOLERANCE}"
    );
    assert!(
        (scale - SHIPPED_MORTALITY_SCALE).abs() < A_COPY,
        "published mortality scale {scale} is not the shipped {SHIPPED_MORTALITY_SCALE}"
    );
    assert!(
        (cap - SHIPPED_MAX_MORTALITY).abs() < A_COPY,
        "published mortality cap {cap} is not the shipped {SHIPPED_MAX_MORTALITY}"
    );

    let config_ambient = app
        .world
        .resource::<SimulationConfig>()
        .ambient_temperature
        .to_f32();
    let demographics = app.world.resource::<DemographicsConfigHandle>().get();
    assert!(
        (ambient - config_ambient).abs() < A_COPY,
        "the wire's ambient temperature disagrees with the config the sim reads"
    );
    assert!(
        (tolerance - demographics.cold.temp_tolerance).abs() < A_COPY,
        "the wire's tolerance disagrees with the config the sim reads"
    );
    assert!(
        (scale - demographics.cold.mortality_scale).abs() < A_COPY,
        "the wire's mortality scale disagrees with the config the sim reads"
    );
    assert!(
        (cap - demographics.cold.max_mortality).abs() < A_COPY,
        "the wire's mortality cap disagrees with the config the sim reads"
    );
}

/// ⛔ **THE PUBLISHED MODEL FOLLOWS THE CONFIG, IT IS NOT A RESTATEMENT OF THE SHIPPED NUMBERS.**
///
/// The test above would pass just as well against four literals hardcoded in the capture. So the
/// cold block is retuned underneath a running world and the wire is required to move with it — the
/// only arm that separates "publishes the model" from "publishes the default".
#[test]
fn a_retuned_cold_block_moves_the_published_model() {
    let mut app = a_running_world();
    let before = published_survivability(&app);

    let mut retuned = (*app.world.resource::<DemographicsConfigHandle>().get()).clone();
    retuned.cold.temp_tolerance = A_RETUNED_TOLERANCE;
    retuned.cold.mortality_scale = A_RETUNED_MORTALITY_SCALE;
    retuned.cold.max_mortality = A_RETUNED_MAX_MORTALITY;
    app.world
        .resource_mut::<DemographicsConfigHandle>()
        .replace(Arc::new(retuned));
    recapture_snapshot_in_place(&mut app.world);

    let (ambient, tolerance, scale, cap) = published_survivability(&app);
    assert!(
        (tolerance - A_RETUNED_TOLERANCE).abs() < A_COPY,
        "the wire kept tolerance {tolerance} after the config moved to {A_RETUNED_TOLERANCE}"
    );
    assert!(
        (scale - A_RETUNED_MORTALITY_SCALE).abs() < A_COPY,
        "the wire kept mortality scale {scale} after the config moved"
    );
    assert!(
        (cap - A_RETUNED_MAX_MORTALITY).abs() < A_COPY,
        "the wire kept mortality cap {cap} after the config moved"
    );
    assert!(
        (ambient - before.0).abs() < A_COPY,
        "ambient temperature is not part of the cold block and must not have moved"
    );
    assert_ne!(
        (before.1, before.2, before.3),
        (tolerance, scale, cap),
        "the retuning has to differ from the shipped tuning or this arm proves nothing"
    );
}

/// The shipped tuning's survivable range, restated as the client will state it: the model is
/// symmetric about ambient, so it has a lethal cold tail AND a lethal heat tail. A guard on the
/// arithmetic the published constants are meant to support — if a future retuning made the range
/// degenerate (tolerance ≤ 0) the client's readout would be nonsense and this fails first.
#[test]
fn the_published_constants_describe_a_nondegenerate_symmetric_range() {
    let app = a_running_world();
    let (ambient, tolerance, scale, cap) = published_survivability(&app);

    assert!(
        tolerance > 0.0,
        "a tolerance of {tolerance} leaves no survivable range at all"
    );
    assert!(
        scale > 0.0 && cap > 0.0 && cap <= 1.0,
        "the death fraction {scale}/deg capped at {cap} is not a fraction of a population"
    );
    let coldest_survivable = ambient - tolerance;
    let warmest_survivable = ambient + tolerance;
    assert!(
        coldest_survivable < warmest_survivable,
        "the survivable range {coldest_survivable}–{warmest_survivable} is inverted"
    );
}
