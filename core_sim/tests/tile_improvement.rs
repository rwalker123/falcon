//! **THE TILE KIND, PROVED ON A TEST-ONLY BRANCH.** No tile branch ships: this file merges the
//! overlay in `tests/fixtures/orchard_tile_branch.json` (a root rung and one built rung, `graft`,
//! on a branch the code has never heard of) onto the builtin ladder and drives the real turn.
//!
//! What it proves is the keeper rule and the whole life of an improvement: a band builds rung 2 with
//! its `builders` pool, **keeps it while it stands within reach** from the `Roadwork` pool, and the
//! improvement **decays after its grace once the band leaves** - and that all of it reaches the wire
//! through the same accessor chain a client uses. Nothing in `core_sim/src` names the branch.

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

/// Hands the fixture band puts on a pool. The built rung costs 20 work and the keeping bill is 1
/// work a turn against one work per bare worker, so these are comfortably enough for both.
const BUILDERS: u32 = 5;
const KEEPERS: u32 = 1;

/// The turn budget for any one phase of a scenario - far above what the fixture needs, so a
/// failure is a wrong answer and not a short loop.
const TURN_BUDGET: usize = 40;

/// One published tile-improvement row, read off the encoded envelope.
#[derive(Debug, Clone)]
struct PublishedImprovement {
    branch: String,
    current_rung: String,
    destination: String,
    has_keeper: bool,
    keeper_band_id: u64,
    demand: f32,
    supplied: f32,
    shortfall: f32,
}

/// How far from the camp the band is moved to leave the improvement behind: past
/// `supply_network_config.reach_tiles` by a wide margin.
const FAR_AWAY_TILES: u32 = 12;

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

/// Place an orchard at the band's own tile and stand the band's builders on it: `BUILDERS` builders
/// with the climb to `grafted` queued. The keeping is staffed separately ([`staff_the_keeping`]),
/// because the shedding order spares a `Roadwork` hand above the bill and an unbuilt rung owes none.
fn begin_building(app: &mut App, band: Entity, faction: FactionId, tile: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(tile, branch(), faction, &ladder);
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, BUILDERS, None);
    allocation.enqueue_build(
        BuildSource::Tile {
            tile,
            branch: branch(),
        },
        BuildJob::Climb(rung(BUILT_RUNG)),
    );
    app.world.entity_mut(band).insert(allocation);
}

/// Put `KEEPERS` hands on the band's `Roadwork` pool - the pool that keeps a tile improvement.
fn staff_the_keeping(app: &mut App, band: Entity) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band has an allocation")
        .set_assignment(LaborTarget::Roadwork, KEEPERS, BUILDERS + KEEPERS, None);
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

/// Move the band's camp to a tile `FAR_AWAY_TILES` east of where it stood.
fn walk_away(app: &mut App, band: Entity, from: UVec2) {
    let width = app.world.resource::<TileRegistry>().width;
    let away = UVec2::new((from.x + FAR_AWAY_TILES) % width, from.y);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(away.x, away.y)
        .expect("the destination is on the map");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .current_tile = entity;
}

/// **THE HAPPY PATH, END TO END**: rung 2 is built by the named band's builders, the entry leaves
/// the queue, and while the band stands within reach the improvement is KEPT - a keeper is named,
/// the bill is paid in full, and the neglect clock never starts.
#[test]
fn a_band_builds_rung_two_and_keeps_it_while_it_stands_within_reach() {
    let mut app = spawn_world();
    let (band, faction, band_id, tile) = first_band(&mut app);
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
    staff_the_keeping(&mut app, band);
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
    assert_eq!(
        improvement.keeper.map(|keeper| keeper.band),
        Some(band_id),
        "the band standing on it keeps it"
    );
    assert_eq!(improvement.neglect_turns, 0, "its bill is met every turn");
}

/// **THE LEAVING HALF**: with the keeper out of reach the improvement has no keeper, its bill goes
/// unpaid, and it **decays after its grace** back to the branch's floor - while a control run, with
/// the band staying put, holds the rung over the same turns.
#[test]
fn an_improvement_with_no_band_in_reach_decays_after_its_grace() {
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

    staff_the_keeping(&mut app, band);
    walk_away(&mut app, band, tile);

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
        assert!(
            improvement.keeper.is_none(),
            "nobody of its people stands within reach any more"
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

    // Past the grace it bleeds, and the rung is lost.
    run_until(&mut app, "the decay back to the floor", |app| {
        held_rung(app, tile) == rung(ROOT_RUNG)
    });
}

/// **THE WIRE**: a kept improvement on explored ground is published with its rung, its keeper and
/// its stamped bill, and the builder's scratch rides only while the viewer's own band has it queued.
#[test]
fn a_kept_improvement_reaches_the_wire_with_its_keeper_and_its_bill() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (band, faction, band_id, tile) = first_band(&mut app);
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
                has_keeper: row.hasKeeper(),
                keeper_band_id: row.keeperBandId(),
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
    assert!(row.has_keeper, "the band standing on it keeps it");
    assert_eq!(row.keeper_band_id, band_id.0);
    assert_eq!(
        row.destination,
        rung(BUILT_RUNG).wire_key(),
        "the viewer's own queue publishes where the head is going"
    );

    run_until(&mut app, "the climb to the grafted rung", |app| {
        held_rung(app, tile) == rung(BUILT_RUNG)
    });
    staff_the_keeping(&mut app, band);
    app.update();
    app.update();
    let rows = published(&app);
    let PublishedImprovement {
        current_rung,
        destination,
        has_keeper,
        demand,
        supplied,
        shortfall,
        ..
    } = rows[0].clone();
    assert_eq!(current_rung, rung(BUILT_RUNG).wire_key());
    assert!(destination.is_empty(), "nothing is queued any more");
    assert!(has_keeper);
    assert!(demand > 0.0, "a built rung owes a bill: {demand}");
    assert!(
        (demand - supplied - shortfall).abs() < 1.0e-4,
        "demand - supplied == shortfall verbatim: {demand} {supplied} {shortfall}"
    );
    assert!(shortfall.abs() < 1.0e-4, "the keepers cover the bill");
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
