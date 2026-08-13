//! **THE BUILD COUNTDOWN'S THREE STATES, ON THE WIRE** (`docs/plan_standing_upkeep.md` §2.4).
//!
//! `buildTurnsRemaining` shipped as one sentinel covering four different situations, so the tile card
//! and the herd drawer — the two surfaces a player reads every turn — rendered **no line at all** for
//! all of them. One of those situations is not an absence of information:
//!
//! | published | meaning |
//! |---|---|
//! | `>= 0` | a real finish date at the crew that is on it |
//! | `BUILD_NEVER_FINISHES` (`-2`) | **this staffing never finishes** — a real, staffed, priced build whose net supply is zero or negative |
//! | `NO_BUILD_TURNS_ESTIMATE` (`-1`) | there is genuinely no answer — nobody is on the source |
//!
//! The maintenance rate is a tax on building, so a crew at or below it holds the meter exactly where
//! it is, forever. **That is actionable and permanent** — the player adds hands — where the other
//! no-answer states are transient. Folding it into `-1` made it visible only to a compose sheet that
//! happened to redo the comparison itself.
//!
//! **Asserted on the ENCODED envelope, never on the in-process value**, the discipline
//! `source_crews_on_the_wire.rs` follows: a field can be right in the capture and wrong in the
//! buffer, and the schema/codec/reader path is what a client actually sees.
//!
//! **All three states run on ONE fixture and are asserted pairwise distinct**, because a test that
//! exercised only two of them would pass with the new sentinel wired to the wrong branch.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    FactionId, ForageRegistry, GenerationId, LaborAllocation, LaborAssignment, LaborTarget,
    LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort, ResidentBand, RungKey,
    SnapshotHistory, StartingUnit, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
    NO_CREW_ON_THIS_ACTIVITY, UNSCALED_UPKEEP,
};
use sim_schema::{BUILD_NEVER_FINISHES, NO_BUILD_TURNS_ESTIMATE};

/// **A GATHERING SITE THE CULTIVATE GATE ADMITS.** Every plant rung requires one
/// (`RungSiteRequirement::requires_gathering_site`), and a refused gate publishes *no estimate* for a
/// reason that has nothing to do with staffing — which would make two of this test's three arms the
/// same state for the wrong reason.
fn a_cultivable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let candidates: Vec<UVec2> = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .iter()
        .map(|site| site.position)
        .collect();
    let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
        let mut query = app.world.query::<&core_sim::Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    let registry = app.world.resource::<ForageRegistry>();
    candidates
        .into_iter()
        .find(|position| {
            if registry.patch(*position).is_none() {
                return false;
            }
            // **And the ground must grow something the tended rung can commit to.** The Cultivate
            // gate needs a committed species, so a basket whose whole `cultivation_ceiling` is wild
            // would make `eligible` false for a reason that is not the staffing under test.
            let Some(tile) = tiles.get(position) else {
                return false;
            };
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
            core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended).is_some()
        })
        .expect("worldgen curated a gathering site whose basket the tended rung can commit to")
}

/// **The hands a real countdown is quoted at** — comfortably above `plant:tended`'s maintenance
/// rate, so the net is positive and the job takes several turns.
const NET_ABOVE_THE_RATE: u32 = 2;

/// `plant:tended`'s maintenance rate in whole hands — the minimum viable build crew. A build staffed
/// **at** it banks nothing and never finishes (`intensification::net_build_supply`).
fn rate_in_hands(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(UNSCALED_UPKEEP)
}

/// A headless world whose `SOURCE` tile carries a patch, with `builders` on a running `Cultivate`.
/// `builders == 0` leaves the source unstaffed for the build.
fn world_with_a_cultivate_staffed_at(builders: u32) -> (App, UVec2) {
    let mut app = build_headless_app();
    // One `update()` runs the Startup worldgen chain, which seeds the tile registry and the patches.
    app.update();
    let source = a_cultivable_site(&mut app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    // **The gate's knowledge half**, so `eligible` turns on the staffing and nothing else.
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );

    // **A half-built meter**, so the source is genuinely *building* by derivation and every arm of
    // this test is about the countdown rather than about whether a build exists.
    let cost = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the tended rung builds");
    let crop = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<&core_sim::Tile>();
        let ground = query
            .iter(&app.world)
            .find(|tile| tile.position == source)
            .expect("the source tile exists")
            .clone();
        let composition =
            core_sim::tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
        core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended)
            .expect("the site was chosen for having one")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(source)
            .expect("the site carries a patch");
        patch.cultivation_cost = cost;
        patch.cultivation_progress = cost / 2.0;
        patch.owner = Some(FactionId(0));
        // **Stock standing above the floor**, or `crew_is_working_the_source` is false and the arm
        // is measuring an empty patch rather than a staffing.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        // **And the ground is already committed to its crop**, which a patch with progress on it
        // always is in play: the species is stamped on the first worked turn. A fixture that writes
        // the meter directly has to write it too, or the Cultivate gate refuses for a reason that is
        // not the staffing under test.
        patch.species = Some(crop);
    }

    /// The gathering crew beside the build — a real take, so the row is a worked source either way.
    const GATHERERS: u32 = 1;
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 30,
            children: scalar_zero(),
            working: scalar_from_f32((GATHERERS + builders + 8) as f32),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: Default::default(),
            last_fertility_factors: Default::default(),
            discontent_fraction: scalar_zero(),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0 as GenerationId,
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        },
        StartingUnit {
            kind: "BandForager".to_string(),
            tags: Vec::new(),
        },
        ResidentBand,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: source,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: None,
                },
                workers: GATHERERS,
                // The verb is derived from the meter, so this states nothing: what decides the arm
                // under test is the **crew**.
                improvement: None,
                kit: None,
                improvement_workers: builders,
            }],
            ..Default::default()
        },
    ));
    (app, source)
}

/// Well above the escapement floor, so the crew is genuinely working the source every turn.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// **The patch row's `buildTurnsRemaining`, read out of the ENCODED buffer** — the artifact a client
/// parses, rather than the state struct the capture built.
fn published_build_turns(app: &App, source: UVec2) -> i32 {
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
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == source.x && patch.y() == source.y)
        .expect("the fixture patch is on the wire")
        .buildTurnsRemaining()
}

/// Run one turn and republish, so the countdown on the wire is the one the turn just stamped.
fn resolve_a_turn(app: &mut App, source: UVec2) -> i32 {
    core_sim::run_turn(app);
    recapture_snapshot_in_place(&mut app.world);
    published_build_turns(app, source)
}

/// **THE THREE STATES, PAIRWISE DISTINCT, ON ONE FIXTURE.**
#[test]
fn the_build_countdown_publishes_a_count_never_and_no_estimate_as_three_states() {
    let rate = {
        let probe = build_headless_app();
        rate_in_hands(&probe)
    };
    assert!(
        rate >= 1,
        "fixture: the rung must charge a rate, or 'at or below it' is unreachable"
    );

    // (1) **A crew above the rate publishes a real count.**
    let (mut app, source) = world_with_a_cultivate_staffed_at(rate + NET_ABOVE_THE_RATE);
    let counted = resolve_a_turn(&mut app, source);
    assert!(
        counted >= 0,
        "a crew above the maintenance rate is quoted a finish date, got {counted}"
    );

    // (2) **A crew AT the rate publishes NEVER** — it is a real, staffed, priced build whose net
    // supply is zero, so the meter holds exactly where it is forever. This is the state the whole
    // sentinel exists for: an answer, not the absence of one.
    let (mut app, source) = world_with_a_cultivate_staffed_at(rate);
    let never = resolve_a_turn(&mut app, source);
    assert_eq!(
        never, BUILD_NEVER_FINISHES,
        "a crew that cannot clear the rate never finishes, and says so"
    );

    // …and so does one BELOW it, which is going backwards rather than standing still.
    if rate > 1 {
        let (mut app, source) = world_with_a_cultivate_staffed_at(rate - 1);
        assert_eq!(
            resolve_a_turn(&mut app, source),
            BUILD_NEVER_FINISHES,
            "a crew under the rate is losing ground — still never"
        );
    }

    // (3) **An UNSTAFFED source publishes NO ESTIMATE, not never.** Nobody has promised anything
    // there; claiming it will never finish would fire on every idle improvement on the map.
    let (mut app, source) = world_with_a_cultivate_staffed_at(NO_CREW_ON_THIS_ACTIVITY);
    let unstaffed = resolve_a_turn(&mut app, source);
    assert_eq!(
        unstaffed, NO_BUILD_TURNS_ESTIMATE,
        "an unstaffed build has no answer — it has not promised one"
    );

    // **Pairwise distinct**, which is what stops the new sentinel being wired to the wrong branch.
    assert_ne!(counted, never);
    assert_ne!(counted, unstaffed);
    assert_ne!(never, unstaffed);
}

/// **THE TWO SENTINELS ARE OUTSIDE THE RANGE A REAL COUNT LIVES IN, and are not each other** — the
/// property every reader leans on when it branches on the sign.
#[test]
fn the_two_sentinels_are_distinct_and_below_every_real_count() {
    const {
        assert!(NO_BUILD_TURNS_ESTIMATE != BUILD_NEVER_FINISHES);
        assert!(NO_BUILD_TURNS_ESTIMATE < 0);
        assert!(BUILD_NEVER_FINISHES < 0);
    }
}
