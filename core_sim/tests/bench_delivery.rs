//! **A FINISHED TOOL REACHES THE STORE THE TURN IT IS FIRST ISSUED** (issue #720).
//!
//! `advance_crafting` runs after the labour pass that settles a band's tools, so a tool the bench
//! finishes on turn N cannot be issued until turn N+1. It used to be stocked into `BandEquipment` at
//! completion anyway, which left the published pool card reading the pool short beside a ledger that
//! already held the tool. The bench now parks a finished item and `deliver_bench_output` stocks it
//! before the next turn's first stage — so the store and the issue move on the same turn.
//!
//! Driven through the real schedule (`app.update()`), and the issue is read off the **encoded
//! envelope**'s `poolToe`, the line a client draws the pool card from.

use bevy::prelude::{App, Entity, UVec2, With};
use core_sim::{
    build_test_app, BandBench, BandEquipment, BandId, LaborAllocation, LaborTarget, LadderConfig,
    MaterialsConfigHandle, PopulationCohort, ResidentBand, RoadKeeper, RoadRegistry, RungKey,
    SourcePriority, Tile, TileRegistry, UpkeepFundMode, ViewerFaction,
};
use std::collections::BTreeMap;

/// **An order no fixture finishes** — what the retired repeat-until-cleared job was, as a count.
const NEVER_FINISHED: u32 = u32::MAX;

/// **The bench works `recipe` and nothing else, with `workers` on it** — the queue holding one
/// [`NEVER_FINISHED`] order, discarding whatever was there (and any pile it had drawn).
trait PutOnBench {
    fn put_on(&mut self, recipe: &str, workers: u32);
}

impl PutOnBench for BandBench {
    fn put_on(&mut self, recipe: &str, workers: u32) {
        self.orders = vec![core_sim::BenchOrder::new(recipe, NEVER_FINISHED)];
        self.workers = workers;
    }
}

/// The `route:dirt_road` rung's tool, and the bench recipe that makes it.
const EARTHMOVING: &str = "earthmoving";
/// The Roadwork pool's wire token on a `poolToe` line.
const ROADWORK_POOL: &str = "roadwork";
/// The recipe's two inputs (`recipes.json`: 3 wood + 2 stone), stocked as exactly one pass so the
/// bench finishes one tool and cannot re-draw.
const WOOD: &str = "wood";
const STONE: &str = "stone";
const ONE_PASS_OF_WOOD: f32 = 3.0;
const ONE_PASS_OF_STONE: f32 = 2.0;
/// Any reading inside the material's table — the grade is not the subject here.
const NEUTRAL_READING: f32 = 0.5;
/// **One pass in one turn, bare-handed** — `16 × 1.0 × 0.5` is exactly the recipe's `work` of `8`,
/// so the turn the fixture advances is the turn the bench finishes.
const CREW_THAT_FINISHES_IN_ONE_TURN: u32 = 16;
/// Hands on the dirt road — each wants one earthmoving set, so one finished set is visibly short of
/// the requirement and visibly more than nothing.
const KEEPERS: u32 = 3;
/// A haul long enough that the road's bill is real work for the keepers.
const A_LONG_HAUL: f32 = 12.0;
const NOTHING_ISSUED: f32 = 0.0;
const ONE_SET: u32 = 1;

/// A band keeping one dirt road, holding no earthmoving gear, with one pass of the earthmoving
/// recipe on its bench and the crew to finish it in one turn. Nothing has run yet.
fn a_band_making_its_own_earthmoving_gear() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let (band, band_id, home) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let road_tile = UVec2::new((home.x + 1) % width, home.y);
    seat_dirt_road(&mut app, road_tile, band_id);

    let mut allocation = LaborAllocation {
        upkeep_fund_mode: UpkeepFundMode::Spread,
        ..Default::default()
    };
    allocation.assignments.push(core_sim::LaborAssignment {
        party: None,
        muster_crew: 0,
        target: LaborTarget::Roadwork,
        workers: KEEPERS,
        kit: None,
        priority: SourcePriority::default(),
    });
    app.world.entity_mut(band).insert(allocation);
    // **Sized to exactly what it staffs**, so `LaborAllocation::normalize` trims neither the road
    // nor the bench.
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band has a cohort")
        .working = core_sim::scalar_from_f32((KEEPERS + CREW_THAT_FINISHES_IN_ONE_TURN) as f32);
    // An empty ledger — the only earthmoving gear the band can ever hold is the bench's.
    app.world.entity_mut(band).insert(BandEquipment::default());

    strip(&mut app, band, WOOD);
    strip(&mut app, band, STONE);
    deposit(&mut app, band, WOOD, ONE_PASS_OF_WOOD);
    deposit(&mut app, band, STONE, ONE_PASS_OF_STONE);
    app.world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench")
        .put_on(EARTHMOVING, CREW_THAT_FINISHES_IN_ONE_TURN);
    (app, band)
}

/// ⛔ **THE STORE AND THE POOL CARD AGREE ON BOTH TURNS.**
///
/// Turn N: the bench finishes the set, the ledger does **not** hold it, and the pool card issues
/// none — the settlement ran before the set existed. Turn N+1: the ledger holds it **and** the
/// settlement that turn issued it. Before #720 the first half failed: the ledger held the set while
/// the card read it unissued.
#[test]
fn a_tool_finished_this_turn_is_stocked_and_issued_on_the_next() {
    let (mut app, band) = a_band_making_its_own_earthmoving_gear();

    app.update();
    assert_eq!(
        bench(&app, band)
            .head()
            .expect("the fixture's order is on the bench")
            .made,
        1,
        "fixture: the bench must finish the set on this turn"
    );
    assert_eq!(
        held(&app, band),
        0,
        "the turn that finished the set does NOT put it in the store"
    );
    assert_eq!(
        issued(&app),
        NOTHING_ISSUED,
        "…and the pool card issues none, so the store and the card agree"
    );

    app.update();
    assert_eq!(
        held(&app, band),
        ONE_SET,
        "the top of the next turn stocks the finished set"
    );
    assert_eq!(
        issued(&app),
        ONE_SET as f32,
        "…and that turn's settlement issues it to the road's keepers"
    );
}

/// ⛔ **A JOB CHANGE BETWEEN THE TWO TURNS DOES NOT LOSE THE FINISHED SET.** The clear and the
/// re-task are what `clear_bench` / `set_bench` do to the bench between turns (the handlers live in
/// the server binary, which no integration test can call); either way the set the old job finished
/// is already made, and it is delivered and issued exactly as if the bench had been left alone.
#[test]
fn a_job_change_between_turns_still_delivers_the_finished_tool() {
    /// Any other recipe the band could put on the bench instead.
    const RETASKED_RECIPE: &str = "stone_dressing";

    type JobChange = fn(&mut BandBench);
    let changes: [(&str, JobChange); 2] = [
        ("cleared", |bench| {
            bench
                .remove_order(core_sim::HEAD_ORDER)
                .expect("the fixture's order is on the bench");
        }),
        ("re-tasked", |bench| {
            bench.put_on(RETASKED_RECIPE, CREW_THAT_FINISHES_IN_ONE_TURN)
        }),
    ];
    for (label, change) in changes {
        let (mut app, band) = a_band_making_its_own_earthmoving_gear();
        app.update();
        assert_eq!(
            held(&app, band),
            0,
            "{label}: fixture — the set is parked, not stocked"
        );

        change(
            &mut app
                .world
                .get_mut::<BandBench>(band)
                .expect("the band has a bench"),
        );
        app.update();
        assert_eq!(
            held(&app, band),
            ONE_SET,
            "{label}: the finished set reaches the store all the same"
        );
        assert_eq!(
            issued(&app),
            ONE_SET as f32,
            "{label}: …and is issued that turn"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/// The campaign's first resident band: entity, `BandId` and the tile it stands on. Also makes its
/// faction the viewer, so the published frame carries its row.
fn first_band(app: &mut App) -> (Entity, BandId, UVec2) {
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
    (entity, band, position)
}

/// **Seat a dirt road kept by `keeper`**, at the top of its rung. The keeper goes in first and the
/// span is read at its own remoteness — see `pool_toe.rs`'s `seat_road` for why.
fn seat_dirt_road(app: &mut App, tile: UVec2, keeper: BandId) {
    let ladder = LadderConfig::builtin();
    let (base, width) = core_sim::road_rung_span(RungKey::RouteDirtRoad, &ladder, A_LONG_HAUL);
    let faction = app.world.resource::<ViewerFaction>().0;
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.take_keeper(
        RoadKeeper {
            faction,
            band: keeper,
        },
        A_LONG_HAUL,
        &ladder,
    );
    road.set_position(base + width, &ladder);
    assert_eq!(road.held_rung(), RungKey::RouteDirtRoad);
    assert!(
        road.keeper.is_some(),
        "fixture: the road is this band's job"
    );
}

/// Empty the band's store of one material, so the bench draws exactly the fixture's pass.
fn strip(app: &mut App, band: Entity, material: &str) {
    let axis = app
        .world
        .resource::<MaterialsConfigHandle>()
        .get()
        .material(material)
        .expect("the shipped table carries this material")
        .characteristics
        .first()
        .expect("every material declares an axis")
        .clone();
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort");
    let held = cohort.stores.material_total(material);
    cohort.stores.take_material(material, &axis, held);
}

/// Bank `amount` of `material`, every axis at [`NEUTRAL_READING`].
fn deposit(app: &mut App, band: Entity, material: &str, amount: f32) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let readings: BTreeMap<String, f32> = materials
        .material(material)
        .expect("the shipped table carries this material")
        .characteristics
        .iter()
        .map(|axis| (axis.clone(), NEUTRAL_READING))
        .collect();
    let key = materials
        .band_key(material, &readings)
        .expect("the reading falls in the material's table");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .deposit_material(material, key, core_sim::scalar_from_f32(amount), &readings);
}

fn bench(app: &App, band: Entity) -> &BandBench {
    app.world
        .get::<BandBench>(band)
        .expect("the band has a bench")
}

/// Earthmoving sets the band's ledger holds.
fn held(app: &App, band: Entity) -> u32 {
    app.world
        .get::<BandEquipment>(band)
        .expect("the band has a ledger")
        .count_of(EARTHMOVING)
}

/// **What the Roadwork pool's published `poolToe` line says it was issued** — `filled` on the
/// earthmoving line of the first band's row, decoded off the encoded envelope.
fn issued(app: &App) -> f32 {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .get(0);
    cohort
        .poolToe()
        .and_then(|lines| {
            lines.iter().find(|line| {
                line.pool() == Some(ROADWORK_POOL) && line.itemId() == Some(EARTHMOVING)
            })
        })
        .map(|line| line.filled())
        .expect("the Roadwork pool keeping a dirt road publishes an earthmoving line")
}
