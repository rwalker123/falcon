//! ⛔ **THE CLIENT MUST BE TOLD WHERE THE COLD KILLS — IT CANNOT DERIVE IT** (issue #614).
//!
//! `systems::population` kills a fraction of *every* age bracket per turn, food-independent, from a
//! model the client has never been handed:
//!
//! ```text
//! min(excess * <tail>.mortality_scale, <tail>.max_mortality)
//! ```
//!
//! where `excess` is how far the tile sits **below `cold.onset_temp`** or **above
//! `heat.onset_temp`**. The two tails are independent and carry their own rate parameters — they had
//! to, once the onsets stopped being mirrored about an ambient, because past its onset each tail has
//! a different runway before the ground runs out.
//!
//! Until now the only temperature thresholds on the wire were `MapSection.climateBands` — the
//! *biome* cut points, which are a different, unrelated set of numbers. At the shipped tuning they
//! visibly disagree: `climate.boreal_max_temp` is 3.0, so a 4 ° tile is labelled **Temperate** while
//! the mortality model still kills on it every turn. A client that guessed a lethal threshold from
//! the band ladder would guess wrong, so the sim publishes the model itself.
//!
//! **The six published constants are the BASE rate — the per-bracket vulnerabilities are not on the
//! wire and are not meant to be.** `{cold,heat}.{child,working,elder}_vulnerability` scale the
//! fraction per age bracket inside the sim, so what a client can state is the rate the *tile*
//! imposes, which is what the climate chip claims. A band's actual losses depend on its age mix,
//! which is a property of the band and not of the ground.
//!
//! **Read off the ENCODED buffer**, never off the capture's state struct: a constant can be right in
//! the capture and absent from the envelope, and the envelope is what a client parses.

use std::sync::Arc;

use bevy::app::App;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, DemographicsConfigHandle,
    SnapshotHistory,
};

/// The shipped `demographics_config.json` tuning, tail by tail. Stated here so a retuning has to
/// come through this test and be seen.
///
/// Each tail rises from zero at its onset to its **own** cap at the extreme it is calibrated to —
/// −57 ° for cold (63 ° of runway, `0.10 ÷ 63`) and +57 ° for heat (17 °, `0.03 ÷ 17`). The two
/// differ in threshold, slope *and* ceiling, because extreme heat is survivable with shade and water
/// where −57 ° is not; that asymmetry is why the wire carries six numbers rather than a symmetric
/// four. Both extremes are ahead of what the generator makes today (−18.5 ° to +31.0 °); issue #622
/// widens the range to match.
const SHIPPED_COLD_ONSET_TEMP: f32 = 6.0;
const SHIPPED_COLD_MORTALITY_SCALE: f32 = 0.00159;
const SHIPPED_COLD_MAX_MORTALITY: f32 = 0.1;
const SHIPPED_HEAT_ONSET_TEMP: f32 = 40.0;
const SHIPPED_HEAT_MORTALITY_SCALE: f32 = 0.00176;
const SHIPPED_HEAT_MAX_MORTALITY: f32 = 0.03;

/// A retuning no shipped config uses, applied to prove the publishing path READS the config rather
/// than restating the shipped numbers. Deliberately far from the shipped tuning on every axis, and
/// **different per tail** — a publisher that read the cold block twice would pass a same-value
/// retuning.
const A_RETUNED_COLD_ONSET: f32 = -3.5;
const A_RETUNED_COLD_MORTALITY_SCALE: f32 = 0.05;
const A_RETUNED_COLD_MAX_MORTALITY: f32 = 0.25;
const A_RETUNED_HEAT_ONSET: f32 = 33.0;
const A_RETUNED_HEAT_MORTALITY_SCALE: f32 = 0.11;
const A_RETUNED_HEAT_MAX_MORTALITY: f32 = 0.4;

/// The mortality model as the wire carries it, in schema order:
/// `(cold onset, cold scale, cold cap, heat onset, heat scale, heat cap)`.
type PublishedModel = (f32, f32, f32, f32, f32, f32);

fn published_survivability(app: &App) -> PublishedModel {
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
        model.coldOnsetTemp(),
        model.coldMortalityScale(),
        model.coldMaxMortality(),
        model.heatOnsetTemp(),
        model.heatMortalityScale(),
        model.heatMaxMortality(),
    )
}

/// A world stood up and turned once, so the snapshot under test is a captured one rather than the
/// baseline placeholder.
fn a_running_world() -> App {
    let mut app = build_test_app();
    run_turn(&mut app);
    app
}

/// ⛔ **THE SIX CONSTANTS THE SIM KILLS FROM ARRIVE ON THE WIRE, AT THE SHIPPED TUNING.**
///
/// Asserted twice over: against the named shipped values (so a retuning is visible here) and against
/// the live config resource (so the wire is pinned to what the sim itself reads, not to a copy).
#[test]
fn the_mortality_model_the_sim_enforces_is_published() {
    let app = a_running_world();
    let published = published_survivability(&app);
    let shipped = (
        SHIPPED_COLD_ONSET_TEMP,
        SHIPPED_COLD_MORTALITY_SCALE,
        SHIPPED_COLD_MAX_MORTALITY,
        SHIPPED_HEAT_ONSET_TEMP,
        SHIPPED_HEAT_MORTALITY_SCALE,
        SHIPPED_HEAT_MAX_MORTALITY,
    );
    assert_eq!(
        published, shipped,
        "the published model {published:?} is not the shipped tuning {shipped:?}"
    );

    let demographics = app.world.resource::<DemographicsConfigHandle>().get();
    let from_config = (
        demographics.cold.onset_temp,
        demographics.cold.mortality_scale,
        demographics.cold.max_mortality,
        demographics.heat.onset_temp,
        demographics.heat.mortality_scale,
        demographics.heat.max_mortality,
    );
    assert_eq!(
        published, from_config,
        "the wire {published:?} disagrees with the config the sim reads {from_config:?}"
    );
}

/// ⛔ **THE PUBLISHED MODEL FOLLOWS THE CONFIG, IT IS NOT A RESTATEMENT OF THE SHIPPED NUMBERS.**
///
/// The test above would pass just as well against six literals hardcoded in the capture. So **both**
/// tails are retuned underneath a running world, to different values, and the wire is required to
/// move with them — the only arm that separates "publishes the model" from "publishes the default",
/// and the only one that catches a publisher reading the cold block twice.
#[test]
fn a_retuned_cold_block_moves_the_published_model() {
    let mut app = a_running_world();
    let before = published_survivability(&app);

    let mut retuned = (*app.world.resource::<DemographicsConfigHandle>().get()).clone();
    retuned.cold.onset_temp = A_RETUNED_COLD_ONSET;
    retuned.cold.mortality_scale = A_RETUNED_COLD_MORTALITY_SCALE;
    retuned.cold.max_mortality = A_RETUNED_COLD_MAX_MORTALITY;
    retuned.heat.onset_temp = A_RETUNED_HEAT_ONSET;
    retuned.heat.mortality_scale = A_RETUNED_HEAT_MORTALITY_SCALE;
    retuned.heat.max_mortality = A_RETUNED_HEAT_MAX_MORTALITY;
    app.world
        .resource_mut::<DemographicsConfigHandle>()
        .replace(Arc::new(retuned));
    recapture_snapshot_in_place(&mut app.world);

    let after = published_survivability(&app);
    let expected = (
        A_RETUNED_COLD_ONSET,
        A_RETUNED_COLD_MORTALITY_SCALE,
        A_RETUNED_COLD_MAX_MORTALITY,
        A_RETUNED_HEAT_ONSET,
        A_RETUNED_HEAT_MORTALITY_SCALE,
        A_RETUNED_HEAT_MAX_MORTALITY,
    );
    assert_eq!(
        after, expected,
        "the wire kept {after:?} after the config moved to {expected:?}"
    );
    assert_ne!(
        before, after,
        "the retuning has to differ from the shipped tuning or this arm proves nothing"
    );
}

/// The shipped tuning's survivable range, restated as the client will state it: a lethal cold tail
/// below the cold onset and a lethal heat tail above the heat one, with a habitable band between. A
/// guard on the arithmetic the published constants are meant to support — a retuning that inverted
/// the onsets or zeroed a rate would make the client's readout nonsense, and this fails first.
#[test]
fn the_published_constants_describe_a_nondegenerate_habitable_band() {
    let app = a_running_world();
    let (cold_onset, cold_scale, cold_cap, heat_onset, heat_scale, heat_cap) =
        published_survivability(&app);

    assert!(
        cold_onset < heat_onset,
        "the habitable band {cold_onset}–{heat_onset} is inverted or empty"
    );
    for (scale, cap, tail) in [
        (cold_scale, cold_cap, "cold"),
        (heat_scale, heat_cap, "heat"),
    ] {
        assert!(
            scale > 0.0 && cap > 0.0 && cap <= 1.0,
            "the {tail} tail's {scale}/deg capped at {cap} is not a fraction of a population"
        );
    }
}
