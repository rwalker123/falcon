//! **THE BUILD COUNTDOWN'S FOUR STATES, ON THE WIRE** (`docs/plan_standing_upkeep.md` §2.4).
//!
//! `buildTurnsRemaining` shipped as one sentinel covering four different situations, so the tile card
//! and the herd drawer — the two surfaces a player reads every turn — rendered **no line at all** for
//! all of them. Three of those situations are not an absence of information:
//!
//! | published | meaning |
//! |---|---|
//! | `>= 0` | a real finish date at the crew that is on it |
//! | `BUILD_METER_HOLDS` (`-2`) | a real, staffed, priced build whose net supply is **exactly zero** — the crew's whole output goes on the maintenance rate |
//! | `BUILD_METER_ROTS` (`-3`) | the same build with a **negative** net — banked work is being lost |
//! | `NO_BUILD_TURNS_ESTIMATE` (`-1`) | there is genuinely no answer — nobody is on the source |
//!
//! The maintenance rate is a tax on building, so a crew at or below it never finishes. **That is
//! actionable and permanent** — the player adds hands — where the no-answer state is transient.
//! Folding it into `-1` made it visible only to a compose sheet that redid the comparison itself.
//!
//! **AND "NEVER FINISHES" WAS ITSELF TWO PIECES OF NEWS.** Holding wastes a crew's turn; rotting
//! destroys progress the player has already bought, and the client renders them yellow and red
//! against a real count's green. One `-2` for both made the worse of them unspeakable.
//!
//! **Asserted on the ENCODED envelope, never on the in-process value**, the discipline
//! `source_crews_on_the_wire.rs` follows: a field can be right in the capture and wrong in the
//! buffer, and the schema/codec/reader path is what a client actually sees.
//!
//! **All four states run on ONE fixture and are asserted pairwise distinct**, because a test that
//! exercised only three of them would pass with the new sentinel wired to the wrong branch. **The
//! EXACT-EQUALITY arm is the one that matters**: a fixture staffed only *below* the rate passes with
//! holding and rotting wired to the same branch, since both are then reached the same way.
//!
//! **And the SECOND TERM of the same quote lives here too** — `cultivationUpkeepDemand`, the rate
//! the rung being quoted costs to hold. `upkeepDemand` beside it resolves through the **at-risk**
//! rung and is therefore `0` on the unstarted source a compose sheet is by definition looking at, so
//! a sheet netting its quote against that promised `workCost / crew` turns for a build that never
//! moves. The fixture that matters is the **no-progress** one: mid-build the two fields name the
//! same rung and agree, so a mid-build fixture would pass with the gap still open.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    FactionId, ForageRegistry, GenerationId, LaborAllocation, LaborAssignment, LaborTarget,
    LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort, ResidentBand, RungKey,
    SnapshotHistory, StartingUnit, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
    NO_CREW_ON_THIS_ACTIVITY, UNSCALED_UPKEEP,
};
use sim_schema::{BUILD_METER_HOLDS, BUILD_METER_ROTS, NO_BUILD_TURNS_ESTIMATE};

/// **A GATHERING SITE THE CULTIVATE GATE ADMITS.** Every plant rung requires one
/// (`RungSiteRequirement::requires_gathering_site`), and a refused gate publishes *no estimate* for a
/// reason that has nothing to do with staffing — which would make three of this test's four arms the
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
    world_with_a_patch(builders, HALF_BUILT)
}

/// **The meter of a patch NOBODY HAS STARTED** — the state a compose sheet is by definition looking
/// at, and the only state in which the two upkeep readouts can disagree.
const NOTHING_BANKED: f32 = core_sim::RUNG_UNSTARTED;

/// **A meter genuinely mid-build**, as a fraction of the rung's cost — enough that the patch is
/// *building* by derivation, so the at-risk rung and the quoted rung are the same rung.
const HALF_BUILT: f32 = 0.5;

/// A headless world whose `SOURCE` tile carries a patch with `banked × cost` on its tended meter and
/// `builders` on the build allocation.
fn world_with_a_patch(builders: u32, banked: f32) -> (App, UVec2) {
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

    // **The meter the arms are quoted from.** At [`HALF_BUILT`] the source is genuinely *building* by
    // derivation, so the countdown arms are about the staffing rather than about whether a build
    // exists; at [`NOTHING_BANKED`] it is unstarted, which is the pre-commit state.
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
        // **An unstarted patch carries NO STAMPED COST either** — the cost is stamped with the first
        // accrual, and one notion of empty is what `reconcile_owner` clears back to.
        patch.cultivation_cost = if banked > NOTHING_BANKED {
            cost
        } else {
            NOTHING_BANKED
        };
        patch.cultivation_progress = cost * banked;
        // ...and an unstarted patch is unowned, for the same reason: the owner lands with the work.
        patch.owner = (banked > NOTHING_BANKED).then_some(FactionId(0));
        // **Stock standing above the floor**, or `crew_is_working_the_source` is false and the arm
        // is measuring an empty patch rather than a staffing.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        // **And the ground is already committed to its crop**, which a patch with progress on it
        // always is in play: the species is stamped on the first worked turn. A fixture that writes
        // the meter directly has to write it too, or the Cultivate gate refuses for a reason that is
        // not the staffing under test. An **unstarted** patch is uncommitted, and the quote resolves
        // its crop off the tile's own basket.
        patch.species = (banked > NOTHING_BANKED).then_some(crop);
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

/// **One field of the fixture patch's row, read out of the ENCODED buffer** — the artifact a client
/// parses, rather than the state struct the capture built. The row borrows the buffer, so the read
/// happens inside.
fn published_patch_field<T>(
    app: &App,
    source: UVec2,
    read: impl Fn(&shadow_scale_flatbuffers::generated::shadow_scale::sim::ForagePatchState<'_>) -> T,
) -> T {
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
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == source.x && patch.y() == source.y)
        .expect("the fixture patch is on the wire");
    read(&row)
}

/// **The patch row's `buildTurnsRemaining`, off the encoded buffer.**
fn published_build_turns(app: &App, source: UVec2) -> i32 {
    published_patch_field(app, source, |patch| patch.buildTurnsRemaining())
}

/// Run one turn and republish, so the countdown on the wire is the one the turn just stamped.
fn resolve_a_turn(app: &mut App, source: UVec2) -> i32 {
    core_sim::run_turn(app);
    recapture_snapshot_in_place(&mut app.world);
    published_build_turns(app, source)
}

/// **A CREW ONE HAND SHORT OF THE RATE** — the staffing that makes the net strictly negative, so the
/// meter is bled rather than merely held. Named because `rate - 1` at a call site reads as
/// arithmetic when what it states is *"under the rate, by the smallest step a player can take"*.
const ONE_HAND_UNDER_THE_RATE: u32 = 1;

/// **THE FOUR STATES, PAIRWISE DISTINCT, ON ONE FIXTURE.**
#[test]
fn the_build_countdown_publishes_a_count_holding_rotting_and_no_estimate_as_four_states() {
    let rate = {
        let probe = build_headless_app();
        rate_in_hands(&probe)
    };
    // **The rate must be at least two hands, or the rotting arm is unreachable**: at a rate of one,
    // the only staffing below it is no staffing at all, which is `NO_BUILD_TURNS_ESTIMATE` by
    // design. The shipped `plant:tended` demand of `2.0` is exactly why this fixture is a patch.
    assert!(
        rate > ONE_HAND_UNDER_THE_RATE,
        "fixture: the rung must charge a rate of at least two hands, or 'under it' is unreachable"
    );

    // (1) **A crew above the rate publishes a real count.**
    let (mut app, source) = world_with_a_cultivate_staffed_at(rate + NET_ABOVE_THE_RATE);
    let counted = resolve_a_turn(&mut app, source);
    assert!(
        counted >= 0,
        "a crew above the maintenance rate is quoted a finish date, got {counted}"
    );

    // (2) **A crew EXACTLY AT the rate publishes HOLDING** — a real, staffed, priced build whose net
    // supply is precisely zero, so the meter stands still: nothing is banked and nothing is lost.
    //
    // **THIS IS THE ARM THE SPLIT LIVES OR DIES ON.** Rotting is reached by a `< 0` comparison and
    // holding by falling through it, so a suite that only staffed *below* the rate would pass with
    // both wired to the same branch. Pinning the exact equality is what makes them two answers.
    let (mut app, source) = world_with_a_cultivate_staffed_at(rate);
    let holding = resolve_a_turn(&mut app, source);
    assert_eq!(
        holding, BUILD_METER_HOLDS,
        "a crew whose whole output pays the rate holds the meter where it is, and says so"
    );

    // (3) **A crew BELOW it publishes ROTTING** — it is going backwards, not standing still, and the
    // player is losing work already bought rather than merely wasting a turn.
    let (mut app, source) = world_with_a_cultivate_staffed_at(rate - ONE_HAND_UNDER_THE_RATE);
    let rotting = resolve_a_turn(&mut app, source);
    assert_eq!(
        rotting, BUILD_METER_ROTS,
        "a crew under the rate leaves a shortfall — the meter loses ground, and says so"
    );

    // (4) **An UNSTAFFED source publishes NO ESTIMATE, neither holding nor rotting.** Nobody has
    // promised anything there; claiming it will never finish would fire on every idle improvement on
    // the map.
    let (mut app, source) = world_with_a_cultivate_staffed_at(NO_CREW_ON_THIS_ACTIVITY);
    let unstaffed = resolve_a_turn(&mut app, source);
    assert_eq!(
        unstaffed, NO_BUILD_TURNS_ESTIMATE,
        "an unstaffed build has no answer — it has not promised one"
    );

    // **Pairwise distinct**, which is what stops a new sentinel being wired to the wrong branch.
    let published = [counted, holding, rotting, unstaffed];
    for (index, left) in published.iter().enumerate() {
        for right in published.iter().skip(index + 1) {
            assert_ne!(
                left, right,
                "the four states must be four numbers on the wire: {published:?}"
            );
        }
    }
}

/// **THE THREE SENTINELS ARE OUTSIDE THE RANGE A REAL COUNT LIVES IN, and are not each other** — the
/// property every reader leans on when it branches on the sign.
#[test]
fn the_three_sentinels_are_distinct_and_below_every_real_count() {
    const {
        assert!(NO_BUILD_TURNS_ESTIMATE != BUILD_METER_HOLDS);
        assert!(NO_BUILD_TURNS_ESTIMATE != BUILD_METER_ROTS);
        assert!(BUILD_METER_HOLDS != BUILD_METER_ROTS);
        assert!(NO_BUILD_TURNS_ESTIMATE < 0);
        assert!(BUILD_METER_HOLDS < 0);
        assert!(BUILD_METER_ROTS < 0);
    }
}

/// **THE QUOTE'S SECOND TERM, ON AN UNSTARTED SOURCE** — the trap this pair of fields exists to
/// close (`docs/plan_standing_upkeep.md` §2).
///
/// `upkeepDemand` resolves through the **at-risk** rung: the newest meter carrying progress. A wild
/// patch has progress on neither, so it publishes an honest `0` — and a compose sheet that netted
/// its quote against *that* subtracted nothing, promised `workCost / crew` turns, and sent the
/// player into a build whose meter never moves because the crew is under the rung's rate.
///
/// `cultivationUpkeepDemand` is the **ladder's** rate for the rung being quoted, resolved at capture
/// whether or not a build is in flight — `workCost`'s own rule, applied to the term that eats it.
///
/// **A MID-BUILD FIXTURE WOULD PASS WITH THE BUG STILL PRESENT**, because there the at-risk rung and
/// the quoted rung are the same rung and the two fields agree. The no-progress case is the whole
/// test; the mid-build arm below is here only to pin that they do not drift apart where they must
/// agree.
#[test]
fn an_unstarted_patch_publishes_the_quoted_rungs_upkeep_where_the_billed_one_is_zero() {
    let ladder_rate = {
        let probe = build_headless_app();
        probe
            .world
            .resource::<LadderConfigHandle>()
            .get()
            .rung(RungKey::PlantTended)
            .upkeep_demand(UNSCALED_UPKEEP)
    };
    assert!(
        ladder_rate > core_sim::NO_UPKEEP_DEMAND,
        "fixture: the tended rung must charge a rate, or the two fields cannot differ"
    );

    // **A crew of ONE, below the rung's rate** — the repro's staffing.
    const A_CREW_UNDER_THE_RATE: u32 = 1;
    let (mut app, source) = world_with_a_patch(A_CREW_UNDER_THE_RATE, NOTHING_BANKED);
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let billed = published_patch_field(&app, source, |patch| patch.upkeepDemand());
    let quoted = published_patch_field(&app, source, |patch| patch.cultivationUpkeepDemand());
    assert_eq!(
        billed,
        core_sim::NO_UPKEEP_DEMAND,
        "nothing is built here yet, so nothing is billed — `upkeepDemand`'s own meaning, unchanged"
    );
    assert_eq!(
        quoted, ladder_rate,
        "the rung the patch would CLIMB costs the ladder's rate to hold, quoted before the commit"
    );

    // **And the projection agrees with it**: a crew that cannot clear the rate never finishes, so
    // the sim publishes the answer rather than a turn count computed against a rate of zero. One
    // builder against a demand of `2.0` is *under* the rate, not at it, so the honest answer is that
    // the meter rots — the quote states which of the two it is.
    assert_eq!(
        published_build_turns(&app, source),
        BUILD_METER_ROTS,
        "one builder is under the tended rung's rate — the meter never reaches its cost"
    );

    // **MID-BUILD, THE TWO ARE ONE NUMBER.** Same rung on both sides, so a drift between the seams
    // would show up here.
    let (mut app, source) = world_with_a_patch(A_CREW_UNDER_THE_RATE, HALF_BUILT);
    core_sim::run_turn(&mut app);
    recapture_snapshot_in_place(&mut app.world);
    assert_eq!(
        published_patch_field(&app, source, |patch| patch.upkeepDemand()),
        published_patch_field(&app, source, |patch| patch.cultivationUpkeepDemand()),
        "a source mid-build is BILLED exactly what the rung it is raising is QUOTED at"
    );
}
