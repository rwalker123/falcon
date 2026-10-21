//! **THE TILE KIND, PROVED ON A TEST-ONLY BRANCH.** No tile branch ships: this file merges the
//! overlay in `tests/fixtures/orchard_tile_branch.json` (a root rung and one built rung, `graft`,
//! on a branch the code has never heard of) onto the builtin ladder and drives the real turn.
//!
//! What it proves is the crew-keeper rule and the whole life of an improvement: a band's `tile`
//! crew **keeps the improvement first** while its `builders` pool raises rung 2, the improvement
//! **decays after its grace once nobody crews it**, a crew past `band_work_range` **goes as a work
//! party** exactly as a far deposit's does, and all of it reaches the wire through the same
//! accessor chain a client uses. Nothing in `core_sim/src` names the branch.

use std::sync::Arc;

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};
use core_sim::{
    build_test_app, BandId, BranchId, BuildJob, BuildSource, FactionId, LaborAllocation,
    LaborTarget, LadderConfig, LadderConfigHandle, PopulationCohort, ResidentBand, RungId,
    SimulationConfig, SnapshotHistory, Tile, TileImprovementRegistry, TileRegistry, ViewerFaction,
    BUILTIN_INTENSIFICATION_LADDER,
};
use serde_json::Value;

/// A pinned earthlike world, so the ground under the band is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// The overlay's one branch and its two rungs' ids.
const BRANCH: &str = "orchard";
const ROOT_RUNG: &str = "bare_ground";
const BUILT_RUNG: &str = "grafted";

/// Hands the fixture band puts on the builders and on the improvement's crew. The built rung costs 20
/// work and the keeping bill is 1 work a turn against one work per bare worker, so these are
/// comfortably enough for both.
const BUILDERS: u32 = 5;
const CREW: u32 = 2;
/// Every hand the fixture band assigns - the `available` figure `set_assignment` clamps against.
const ALL_HANDS: u32 = BUILDERS + CREW;

/// The turn budget for any one phase of a scenario - far above what the fixture needs, so a
/// failure is a wrong answer and not a short loop.
const TURN_BUDGET: usize = 40;

/// One published tile-improvement row, read off the encoded envelope.
#[derive(Debug, Clone)]
struct PublishedImprovement {
    branch: String,
    current_rung: String,
    destination: String,
    is_queued: bool,
    upkeep_workers_needed: u32,
    demand: f32,
    supplied: f32,
    shortfall: f32,
}

/// How far from the band an improvement is put to make its crew a work party: past
/// `labor_config.band_work_range` (2) by a wide margin, and still on the map.
const FAR_AWAY_TILES: u32 = 6;

/// **THE OVERLAY MERGED ONTO THE BUILTIN LADDER** - the branch descriptors and the rung records the
/// fixture adds, appended exactly as a designer would edit the JSON.
fn ladder_with_the_orchard() -> LadderConfig {
    let mut ladder: Value = serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER)
        .expect("the builtin ladder parses as json");
    let overlay: Value = serde_json::from_str(include_str!("fixtures/orchard_tile_branch.json"))
        .expect("the fixture parses as json");
    for (id, branch) in overlay["branches"]
        .as_object()
        .expect("the overlay declares branches")
    {
        ladder["branches"][id] = branch.clone();
    }
    let rungs = ladder["rungs"].as_array_mut().expect("rungs is an array");
    rungs.extend(
        overlay["rungs"]
            .as_array()
            .expect("the overlay declares rungs")
            .iter()
            .cloned(),
    );
    LadderConfig::from_json_str(&ladder.to_string())
        .expect("a tile branch is a pure config edit and the overlay must load")
}

fn branch() -> BranchId {
    BranchId::new(BRANCH)
}

fn rung(id: &str) -> RungId {
    RungId::new(branch(), id)
}

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.world
        .insert_resource(LadderConfigHandle::new(Arc::new(ladder_with_the_orchard())));
    app.update();
    app
}

/// The campaign's first resident band: entity, faction, `BandId` and the tile it stands on. The
/// viewer is pinned to its faction, so what is published is what this people can see.
fn first_band(app: &mut App) -> (Entity, FactionId, BandId, UVec2) {
    let (entity, faction, band, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
        let (entity, cohort, band) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, *band, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    app.world.insert_resource(ViewerFaction(faction));
    (entity, faction, band, position)
}

/// The improvement's crew row on `tile`.
fn tile_row(tile: UVec2) -> LaborTarget {
    LaborTarget::Tile {
        tile,
        branch: branch(),
    }
}

/// Place an orchard at `tile`, stand the band's builders on the queue head and put a crew of `CREW`
/// on the improvement: `BUILDERS` builders with the climb to `grafted` queued, and the `tile` row
/// that keeps the improvement first.
fn begin_building(app: &mut App, band: Entity, faction: FactionId, tile: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(tile, branch(), faction, &ladder);
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.set_assignment(tile_row(tile), CREW, ALL_HANDS, None);
    allocation.enqueue_build(
        BuildSource::Tile {
            tile,
            branch: branch(),
        },
        BuildJob::Climb(rung(BUILT_RUNG)),
    );
    app.world.entity_mut(band).insert(allocation);
}

/// Set the improvement's crew to `hands` (`0` holds the row unstaffed: the holding survives, the
/// keeping stops).
fn set_crew(app: &mut App, band: Entity, tile: UVec2, hands: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band has an allocation")
        .set_assignment(tile_row(tile), hands, ALL_HANDS, None);
}

fn held_rung(app: &App, tile: UVec2) -> RungId {
    app.world
        .resource::<TileImprovementRegistry>()
        .get(tile, branch())
        .expect("the improvement was placed")
        .held_rung()
}

/// Run turns until `done` holds, failing with `what` if the budget runs out first.
fn run_until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) -> usize {
    for turn in 0..TURN_BUDGET {
        if done(app) {
            return turn;
        }
        app.update();
    }
    panic!("{what} did not happen within {TURN_BUDGET} turns");
}

/// **THE HAPPY PATH, END TO END**: rung 2 is built by the band's builders while its crew keeps the
/// improvement, the entry leaves the queue, and while the crew stays on it the improvement is KEPT -
/// the bill is paid in full by the crew's hands and the neglect clock never starts.
#[test]
fn a_crew_builds_rung_two_and_keeps_it_while_it_works_it() {
    let mut app = spawn_world();
    let (band, faction, _, tile) = first_band(&mut app);
    begin_building(&mut app, band, faction, tile);

    run_until(&mut app, "the climb to the grafted rung", |app| {
        held_rung(app, tile) == rung(BUILT_RUNG)
    });
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band has an allocation")
            .build_queue
            .is_empty(),
        "an entry retires at its destination"
    );

    // Long enough to outlast the rung's grace several times over: a kept improvement holds.
    for _ in 0..8 {
        app.update();
    }
    let registry = app.world.resource::<TileImprovementRegistry>();
    let improvement = registry.get(tile, branch()).expect("it is still there");
    assert_eq!(
        improvement.held_rung(),
        rung(BUILT_RUNG),
        "a kept rung holds"
    );
    assert_eq!(improvement.neglect_turns, 0, "its bill is met every turn");
    assert!(
        improvement.upkeep_hands > 0.0,
        "the crew's hands are what paid it: {}",
        improvement.upkeep_hands
    );
    assert!(
        (improvement.upkeep_supplied - improvement.upkeep_demanded.unwrap_or_default()).abs()
            < 1.0e-4,
        "the crew supplied the whole bill: {} of {:?}",
        improvement.upkeep_supplied,
        improvement.upkeep_demanded
    );
}

/// **THE UNSTAFFING HALF**: with the crew taken off, the improvement has nobody keeping it, its bill
/// goes unpaid, and it **decays after its grace** back to the branch's floor - the row surviving as
/// the band's holding meanwhile.
#[test]
fn an_unstaffed_improvement_decays_after_its_grace() {
    let mut app = spawn_world();
    let (band, faction, _, tile) = first_band(&mut app);
    begin_building(&mut app, band, faction, tile);
    run_until(&mut app, "the climb to the grafted rung", |app| {
        held_rung(app, tile) == rung(BUILT_RUNG)
    });

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let grace = ladder
        .rung(rung(BUILT_RUNG))
        .upkeep
        .as_ref()
        .expect("the built rung declares a bill")
        .grace_turns;

    set_crew(&mut app, band, tile, 0);

    // Inside the grace nothing is lost: the bill is short, the clock is running, the rung stands.
    for _ in 0..grace {
        app.update();
    }
    {
        let improvement = app
            .world
            .resource::<TileImprovementRegistry>()
            .get(tile, branch())
            .expect("it is still there");
        assert_eq!(
            improvement.upkeep_hands, 0.0,
            "nobody is keeping it any more"
        );
        assert_eq!(
            improvement.held_rung(),
            rung(BUILT_RUNG),
            "the rung survives its grace"
        );
        assert!(
            improvement.neglect_turns > 0,
            "the neglect clock is running"
        );
    }
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band has an allocation")
            .assignments
            .iter()
            .any(|row| row.target.same_source(&tile_row(tile))),
        "the unstaffed row survives as the band's holding while the improvement stands"
    );

    // Past the grace it bleeds, and the rung is lost.
    run_until(&mut app, "the decay back to the floor", |app| {
        held_rung(app, tile) == rung(ROOT_RUNG)
    });
}

/// **DISTANCE IS NOTHING SPECIAL**: an improvement beyond `band_work_range` is worked by a work
/// party exactly as a far deposit is - the row keeps its crew, a party is posted at the improvement's
/// tile, and the wire's `partyWorkers` says so.
#[test]
fn a_far_crew_goes_as_a_work_party() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (band, faction, _, home) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let far = UVec2::new((home.x + FAR_AWAY_TILES) % width, home.y);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(far, branch(), faction, &ladder);
    // **Raised to the grafted rung, so it owes a keeping bill** (#871): the sim staffs a tile crew
    // with the keepers the bill calls for, and a bare improvement at its floor owes none.
    {
        let (base, width) = core_sim::tile_improvement::tile_rung_span(rung(BUILT_RUNG), &ladder);
        app.world
            .resource_mut::<TileImprovementRegistry>()
            .get_mut(far, branch())
            .expect("the improvement was placed")
            .set_position(base + width, &ladder);
    }
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(tile_row(far), CREW, ALL_HANDS, None);
    app.world.entity_mut(band).insert(allocation);

    for _ in 0..3 {
        app.update();
    }
    let allocation = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band has an allocation");
    let row = allocation
        .assignments
        .iter()
        .find(|row| row.target.same_source(&tile_row(far)))
        .expect("a far row is not given up on");
    let party = row.party.as_ref().expect("a far crew is a work party");
    assert_eq!(party.position, far, "the party stands at the improvement");
    assert!(party.workers > 0, "the party holds the crew's hands");

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let wire_row = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "tile")
        .expect("the crew's row is on the wire");
    assert_eq!(wire_row.branch().unwrap_or_default(), BRANCH);
    assert_eq!((wire_row.targetX(), wire_row.targetY()), (far.x, far.y));
    assert!(
        wire_row.partyWorkers() > 0,
        "the wire says the crew is a party"
    );
}

/// **THE WIRE**: an improvement on explored ground is published with its rung, its stamped bill and
/// the keeping block a deposit publishes; the builder's scratch rides only while the viewer's own
/// band has it queued, and the crew's row names its branch.
#[test]
fn a_kept_improvement_reaches_the_wire_with_its_crew_and_its_bill() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (band, faction, _, tile) = first_band(&mut app);
    begin_building(&mut app, band, faction, tile);

    // One turn in, the climb is queued and being worked: the builder's scratch is published.
    app.update();
    let published = |app: &App| -> Vec<PublishedImprovement> {
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
            .routes()
            .and_then(|section| section.tileImprovements())
            .expect("the tile improvement list is published")
            .iter()
            .map(|row| PublishedImprovement {
                branch: row.branch().unwrap_or_default().to_string(),
                current_rung: row.currentRung().unwrap_or_default().to_string(),
                destination: row.buildDestinationRung().unwrap_or_default().to_string(),
                is_queued: row.isQueued(),
                upkeep_workers_needed: row.upkeepWorkersNeeded(),
                demand: row.upkeepDemand(),
                supplied: row.upkeepSupplied(),
                shortfall: row.upkeepShortfall(),
            })
            .collect()
    };
    let rows = published(&app);
    assert_eq!(rows.len(), 1, "one improvement is on the map: {rows:?}");
    let row = rows[0].clone();
    assert_eq!(row.branch, BRANCH);
    assert!(row.is_queued, "the viewer's own band has a build queued");
    assert_eq!(
        row.destination,
        rung(BUILT_RUNG).wire_key(),
        "the viewer's own queue publishes where the head is going"
    );

    run_until(&mut app, "the climb to the grafted rung", |app| {
        held_rung(app, tile) == rung(BUILT_RUNG)
    });
    app.update();
    app.update();
    let rows = published(&app);
    let PublishedImprovement {
        current_rung,
        destination,
        is_queued,
        upkeep_workers_needed,
        demand,
        supplied,
        shortfall,
        ..
    } = rows[0].clone();
    assert_eq!(current_rung, rung(BUILT_RUNG).wire_key());
    assert!(destination.is_empty(), "nothing is queued any more");
    assert!(!is_queued);
    assert!(demand > 0.0, "a built rung owes a bill: {demand}");
    assert!(
        upkeep_workers_needed > 0,
        "the bill calls for whole hands: {upkeep_workers_needed}"
    );
    assert!(
        (demand - supplied - shortfall).abs() < 1.0e-4,
        "demand - supplied == shortfall verbatim: {demand} {supplied} {shortfall}"
    );
    assert!(shortfall.abs() < 1.0e-4, "the crew covers the bill");

    // The crew's own row names the tile and the branch, so a client can join it to the row above.
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let crew_row = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "tile")
        .expect("the crew's row is on the wire");
    assert_eq!(crew_row.branch().unwrap_or_default(), BRANCH);
    assert_eq!((crew_row.targetX(), crew_row.targetY()), (tile.x, tile.y));
    // **The crew is the bill's keepers** (#871): the count named is only the most the sim may staff.
    assert_eq!(crew_row.workers(), upkeep_workers_needed);
    assert!(crew_row.workers() <= CREW);
}

/// **FOG**: an improvement on ground the viewer has never explored is not published.
#[test]
fn an_improvement_on_unexplored_ground_is_not_published() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (_, faction, _, home) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let far = UVec2::new((home.x + width / 2) % width, home.y);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(far, branch(), faction, &ladder);
    app.update();

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let count = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .routes()
        .and_then(|section| section.tileImprovements())
        .map_or(0, |rows| rows.len());
    assert_eq!(count, 0, "ground nobody has explored publishes nothing");
}

/// **A TILE RUNG IS WORK ALONE**: the ladder refuses a material on one, loudly, rather than
/// publishing a bill nothing pays.
#[test]
fn a_material_on_a_tile_rung_is_refused() {
    let mut ladder: Value = serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER)
        .expect("the builtin ladder parses as json");
    let overlay: Value = serde_json::from_str(include_str!("fixtures/orchard_tile_branch.json"))
        .expect("the fixture parses as json");
    ladder["branches"][BRANCH] = overlay["branches"][BRANCH].clone();
    let mut rungs: Vec<Value> = overlay["rungs"].as_array().expect("rungs").clone();
    rungs[1]["build"]["materials"] = serde_json::json!({ "stone": 4.0 });
    ladder["rungs"].as_array_mut().expect("rungs").extend(rungs);
    let refusal = LadderConfig::from_json_str(&ladder.to_string())
        .expect_err("a material on a tile rung must not load");
    assert!(
        refusal.to_string().contains("WORK alone"),
        "the refusal names the rule: {refusal}"
    );
}
