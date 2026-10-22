//! **THE STORAGE BRANCHES** (#708, `.claude/rules/core_sim/intensification.md` -> "The band kind",
//! `campaign.md` -> "Food spoils by keeping class"): a drying rack a BAND holds at its camp and a
//! storage pit a people digs on a TILE. What each buys is a shelf life - `demographics_config.json`
//! `keeping.preservation` - read through the one `spoilage::band_shelf_life` seam.
//!
//! Three halves, each driven through the real systems against the SHIPPED config:
//!
//! - **the payoff**: the larder rot (`rot_band_larders`) on a band holding a rack, a pit in reach,
//!   and every near miss (out of reach, another people's, half built);
//! - **the lifecycle**: the rack is gated on `storage`, eats its wood as the meter climbs, is kept by
//!   a crew and decays back to `carried` unkept; the pit is placed and kept by a tile crew;
//! - **the identity of a band improvement**: a split leaves it on the parent and a save
//!   carries it.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::spoilage::{
    band_shelf_life, base_shelf_life, rot_band_larders, rots_in_transit, HeldPreservation,
    STORAGE_DISCOVERY_ID,
};
use core_sim::{
    band_rung_span, build_test_app, scalar_from_f32, scalar_one, scalar_zero, simulate_population,
    tile_rung_span, BandEquipment, BandId, BranchId, BuildJob, BuildSource,
    DemographicsConfigHandle, DiscoveryProgressLedger, EquipmentConfig, FactionId, GenerationId,
    LaborAllocation, LaborTarget, LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, RungId, SimulationConfig, SimulationTick, SnapshotHistory,
    SupplyNetworkConfigHandle, Tile, TileImprovementRegistry, TileRegistry, ViewerFaction,
};

// ---------------------------------------------------------------------------------------------
// The payoff: a band's shelf life, through the real larder rot
// ---------------------------------------------------------------------------------------------

const BAND: u64 = 31;
/// A faction no opening band belongs to, so the ledger reads are this fixture's alone.
const FACTION: FactionId = FactionId(77);
/// Another people, whose pit is never the band's.
const OTHER_PEOPLE: FactionId = FactionId(78);
/// A larder far past what a turn's meal eats, so the lot is certain to be left to age.
const SURPLUS: f32 = 100_000.0;
/// A dry tile on the harness map for the camp.
const CAMP: (u32, u32) = (17, 24);
const BAND_SIZE: u32 = 30;
const CREW: f32 = 6.0;
/// The share of a lot that has to be gone for the lot to count as rotted (the meal eats a sliver).
const MOST_OF_THE_LOT: f32 = SURPLUS / 2.0;

fn camp_stores() -> BranchId {
    BranchId::new("camp_stores")
}
fn drying_rack() -> RungId {
    RungId::new(camp_stores(), "drying_rack")
}
fn food_stores() -> BranchId {
    BranchId::new("food_stores")
}
fn storage_pit() -> RungId {
    RungId::new(food_stores(), "storage_pit")
}

fn spawn_band(app: &mut App) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(CAMP.0, CAMP.1)
        .expect("the harness map carries the camp tile");
    app.world
        .spawn((
            ResidentBand,
            BandId(BAND),
            BandEquipment::start_stocked_for(&EquipmentConfig::for_a_stocked_fixture(), CREW),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: BAND_SIZE,
                children: scalar_zero(),
                working: scalar_from_f32(CREW),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                fed_party_workers: Default::default(),
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FACTION,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
        ))
        .id()
}

fn rot_world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = spawn_band(&mut app);
    (app, band)
}

/// Complete the rack the band holds: the meter at the top of the rung.
fn hold_the_rack(app: &mut App, band: BandId) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = band_rung_span(drying_rack(), &ladder);
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .bands_mut()
        .place(band, camp_stores(), &ladder)
        .set_position(base + width, &ladder);
}

/// Dig a pit on `tile` for `faction`, `fraction` of the way up its rung (`1.0` is a finished pit).
fn dig_a_pit(app: &mut App, tile: UVec2, faction: FactionId, fraction: f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = tile_rung_span(storage_pit(), &ladder);
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(tile, food_stores(), faction, &ladder)
        .set_position(base + width * fraction, &ladder);
}

fn reach_tiles(app: &App) -> u32 {
    app.world
        .resource::<SupplyNetworkConfigHandle>()
        .get()
        .reach_tiles
}

fn camp() -> UVec2 {
    UVec2::new(CAMP.0, CAMP.1)
}

/// Seat one lot of `class` at `age` as the band's whole larder, run one turn's meal then rot - the
/// Population chain's order - and say how much of the lot rotted.
fn rotted_of_a_lot(app: &mut App, band: Entity, class: &str, age: u32) -> f32 {
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.stores = LocalStore::new();
        cohort
            .stores
            .add_food_aged(class, scalar_from_f32(SURPLUS), age);
    }
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(simulate_population);
    app.world.run_system_once(rot_band_larders);
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .last_food_spoiled
}

fn shelf_of(app: &App, class: &str) -> u32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(class)
        .expect("the shipped table keeps the class") as u32
}

/// The shelf life a preservation names for `class`, off the shipped table.
fn preserved_shelf(app: &App, rung: RungId, class: &str) -> u32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .preservation
        .iter()
        .find(|entry| entry.rung == rung && entry.class == class)
        .expect("the shipped table preserves this class on this rung")
        .shelf_life_turns as u32
}

/// **A DRYING RACK KEEPS MEAT TO ITS OWN LIFE.** Without one a flesh lot rots when it reaches its
/// base shelf life; with one it survives past that and rots at the rack's. Each arm is read on both
/// sides of its boundary, so a rack that did nothing - or that kept meat for ever - fails.
#[test]
fn a_band_holding_a_drying_rack_keeps_a_flesh_lot_to_the_racks_life() {
    let (mut app, band) = rot_world();
    let base = shelf_of(&app, "flesh");
    let racked = preserved_shelf(&app, drying_rack(), "flesh");
    assert!(racked > base, "fixture: the rack lengthens flesh");

    assert!(
        rotted_of_a_lot(&mut app, band, "flesh", base - 1) > MOST_OF_THE_LOT,
        "without a rack a flesh lot rots when its age reaches {base}"
    );
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "flesh", base - 2),
        0.0,
        "control: one turn younger it keeps"
    );

    hold_the_rack(&mut app, BandId(BAND));
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "flesh", base - 1),
        0.0,
        "with a rack the lot that would have rotted at {base} keeps"
    );
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "flesh", racked - 2),
        0.0,
        "and keeps until the turn before the rack's life"
    );
    assert!(
        rotted_of_a_lot(&mut app, band, "flesh", racked - 1) > MOST_OF_THE_LOT,
        "and rots when its age reaches the rack's {racked}"
    );
}

/// **A HALF-BUILT RACK DRIES NOTHING** (`partial_credit: on_completion`).
#[test]
fn a_half_built_rack_preserves_nothing() {
    let (mut app, band) = rot_world();
    let base = shelf_of(&app, "flesh");
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (rack_base, width) = band_rung_span(drying_rack(), &ladder);
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .bands_mut()
        .place(BandId(BAND), camp_stores(), &ladder)
        .set_position(rack_base + width * 0.5, &ladder);
    assert!(
        rotted_of_a_lot(&mut app, band, "flesh", base - 1) > MOST_OF_THE_LOT,
        "a rack the builders have not finished keeps the base life"
    );
}

/// **A PIT KEEPS GRAIN FOR THE PEOPLE WHO DUG IT, NEAR ENOUGH.** One scenario per world: the pit in
/// reach keeps dry food past its base life; a pit a step out of reach, another people's pit and a
/// half-dug pit each leave the band at the base.
#[test]
fn a_pit_keeps_dry_food_only_for_its_own_people_within_reach() {
    let base = |app: &App| shelf_of(app, "dry");
    let pitted = |app: &App| preserved_shelf(app, storage_pit(), "dry");

    // The pit at the camp.
    let (mut app, band) = rot_world();
    let (base, pitted) = (base(&app), pitted(&app));
    assert!(pitted > base, "fixture: the pit lengthens dry food");
    dig_a_pit(&mut app, camp(), FACTION, 1.0);
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "dry", base - 1),
        0.0,
        "a finished pit at the camp keeps dry food past {base}"
    );
    assert_eq!(rotted_of_a_lot(&mut app, band, "dry", pitted - 2), 0.0);
    assert!(
        rotted_of_a_lot(&mut app, band, "dry", pitted - 1) > MOST_OF_THE_LOT,
        "and rots it when its age reaches the pit's {pitted}"
    );

    // A pit exactly at the edge of the supply network's reach still serves the band ...
    let (mut app, band) = rot_world();
    let reach = reach_tiles(&app);
    dig_a_pit(&mut app, UVec2::new(CAMP.0 + reach, CAMP.1), FACTION, 1.0);
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "dry", base - 1),
        0.0,
        "a pit {reach} tiles away is within reach"
    );

    // ... and one step beyond it does not.
    let (mut app, band) = rot_world();
    dig_a_pit(
        &mut app,
        UVec2::new(CAMP.0 + reach + 1, CAMP.1),
        FACTION,
        1.0,
    );
    assert!(
        rotted_of_a_lot(&mut app, band, "dry", base - 1) > MOST_OF_THE_LOT,
        "a pit {} tiles away is out of reach",
        reach + 1
    );

    // Another people's pit at the camp is not this band's.
    let (mut app, band) = rot_world();
    dig_a_pit(&mut app, camp(), OTHER_PEOPLE, 1.0);
    assert!(
        rotted_of_a_lot(&mut app, band, "dry", base - 1) > MOST_OF_THE_LOT,
        "a stranger's pit keeps nothing of ours"
    );

    // A half-dug pit stores nothing.
    let (mut app, band) = rot_world();
    dig_a_pit(&mut app, camp(), FACTION, 0.5);
    assert!(
        rotted_of_a_lot(&mut app, band, "dry", base - 1) > MOST_OF_THE_LOT,
        "a pit the diggers have not finished keeps the base life"
    );
}

/// **A STORE LENGTHENS THE CLASS IT NAMES AND NO OTHER.** With a rack and a pit both held, a plant
/// lot still rots at its base life, and the rack does not touch dry food nor the pit flesh.
#[test]
fn a_store_leaves_every_other_class_at_its_base_life() {
    let (mut app, band) = rot_world();
    hold_the_rack(&mut app, BandId(BAND));
    dig_a_pit(&mut app, camp(), FACTION, 1.0);

    let fresh = shelf_of(&app, "fresh_plant");
    assert!(
        rotted_of_a_lot(&mut app, band, "fresh_plant", fresh - 1) > MOST_OF_THE_LOT,
        "fresh plant food rots at its base life with a rack and a pit held"
    );
    let flesh = preserved_shelf(&app, drying_rack(), "flesh");
    let dry = preserved_shelf(&app, storage_pit(), "dry");
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "flesh", flesh - 2),
        0.0,
        "control: the rack keeps flesh"
    );
    assert_eq!(
        rotted_of_a_lot(&mut app, band, "dry", dry - 2),
        0.0,
        "control: the pit keeps dry food"
    );

    // The rack alone, in a world with no pit: dry food is at its base.
    let (mut app, band) = rot_world();
    hold_the_rack(&mut app, BandId(BAND));
    let dry_base = shelf_of(&app, "dry");
    assert!(
        rotted_of_a_lot(&mut app, band, "dry", dry_base - 1) > MOST_OF_THE_LOT,
        "a rack does not keep grain"
    );
    // And the pit alone leaves flesh at its base.
    let (mut app, band) = rot_world();
    dig_a_pit(&mut app, camp(), FACTION, 1.0);
    let flesh_base = shelf_of(&app, "flesh");
    assert!(
        rotted_of_a_lot(&mut app, band, "flesh", flesh_base - 1) > MOST_OF_THE_LOT,
        "a pit does not keep meat"
    );
}

/// **A CARAVAN PACK ON THE WALK IS AWAY FROM THE RACK.** The band's shelf life is the rack's, and
/// the transit rot - which takes no preservation at all - still rots a flesh pack at its base life.
#[test]
fn a_caravan_packs_transit_rot_ignores_the_rack() {
    let (app, _) = rot_world();
    let config = app.world.resource::<DemographicsConfigHandle>().get();
    let keeping = &config.keeping;
    let base = shelf_of(&app, "flesh");
    let racked = preserved_shelf(&app, drying_rack(), "flesh");

    let mut held = HeldPreservation::none();
    held.raise("flesh", racked as f32);
    assert_eq!(
        band_shelf_life(keeping, &held, "flesh"),
        Some(racked as f32),
        "the band keeps flesh for the rack's life"
    );
    assert_eq!(
        base_shelf_life(keeping, "flesh"),
        Some(base as f32),
        "the base life is unmoved"
    );
    assert!(
        rots_in_transit("flesh", base, keeping),
        "a pack walking home for {base} turns rots, rack or no rack"
    );
    assert!(
        !rots_in_transit("flesh", base - 1, keeping),
        "control: one turn shorter it keeps"
    );
}

// ---------------------------------------------------------------------------------------------
// The lifecycle: a campaign band raising, keeping and losing the rack and the pit
// ---------------------------------------------------------------------------------------------

const MAP_SEED: u64 = 119_304_647;
/// Hands the fixture band puts on the builders and on the improvement's crew.
const BUILDERS: u32 = 5;
const CREW_HANDS: u32 = 2;
const ALL_HANDS: u32 = BUILDERS + CREW_HANDS;
/// The turn budget for any one phase of a scenario - far above what the fixture needs.
const TURN_BUDGET: usize = 60;
/// Wood in the store when the rack is raised: well over the rung's pile.
const WOOD_STOCK: f32 = 20.0;
const WOOD: &str = "wood";
/// Tolerance on a drawn pile, which is fixed-point.
const PILE_EPSILON: f32 = 1.0e-2;

fn campaign_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The campaign's first resident band: entity, faction, `BandId` and the tile it stands on. The
/// viewer is pinned to its faction.
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

fn rack_row(band: BandId) -> LaborTarget {
    LaborTarget::Band {
        band,
        branch: camp_stores(),
    }
}

fn pit_row(tile: UVec2) -> LaborTarget {
    LaborTarget::Tile {
        tile,
        branch: food_stores(),
    }
}

/// Teach the faction the rot-taught lesson both built rungs unlock on.
fn learn_storage(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, STORAGE_DISCOVERY_ID, scalar_one());
}

/// Put `units` of wood in the band's store, at the opening loadout's reading.
fn stock_wood(app: &mut App, band: Entity, units: f32) {
    let materials = core_sim::MaterialsConfig::builtin();
    let characteristics: std::collections::BTreeMap<String, f32> = materials
        .material(WOOD)
        .expect("the shipped roster carries wood")
        .characteristics
        .iter()
        .map(|axis| (axis.clone(), core_sim::OPENING_MATERIAL_READING))
        .collect();
    let key = materials
        .band_key(WOOD, &characteristics)
        .expect("the shipped roster rates wood");
    // Added beside the larder, never replacing it: a band with nothing to eat starves and sheds the
    // crews this scenario is about.
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort
        .stores
        .deposit_material(WOOD, key, scalar_from_f32(units), &characteristics);
}

fn wood_held(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .material_total(WOOD)
        .to_f32()
}

/// Place the band's rack, stand the builders on the queue head and put the crew on the rack.
fn begin_the_rack(app: &mut App, entity: Entity, band: BandId) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .bands_mut()
        .place(band, camp_stores(), &ladder);
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.set_assignment(rack_row(band), CREW_HANDS, ALL_HANDS, None);
    allocation.enqueue_build(
        BuildSource::Band {
            band,
            branch: camp_stores(),
        },
        BuildJob::Climb(drying_rack()),
    );
    app.world.entity_mut(entity).insert(allocation);
}

/// Place a pit at `tile`, stand the builders on the queue head and put the tile crew on it.
fn begin_the_pit(app: &mut App, entity: Entity, faction: FactionId, tile: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world.resource_mut::<TileImprovementRegistry>().place(
        tile,
        food_stores(),
        faction,
        &ladder,
    );
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.set_assignment(pit_row(tile), CREW_HANDS, ALL_HANDS, None);
    allocation.enqueue_build(
        BuildSource::Tile {
            tile,
            branch: food_stores(),
        },
        BuildJob::Climb(storage_pit()),
    );
    app.world.entity_mut(entity).insert(allocation);
}

fn set_crew(app: &mut App, band: Entity, row: LaborTarget, hands: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band has an allocation")
        .set_assignment(row, hands, ALL_HANDS, None);
}

fn rack_rung_held(app: &App, band: BandId) -> RungId {
    app.world
        .resource::<TileImprovementRegistry>()
        .bands()
        .get(band, camp_stores())
        .expect("the rack was placed")
        .held_rung()
}

fn pit_rung_held(app: &App, tile: UVec2) -> RungId {
    app.world
        .resource::<TileImprovementRegistry>()
        .get(tile, food_stores())
        .expect("the pit was placed")
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

/// **THE RACK IS GATED ON WHAT ROT TAUGHT, AND IT EATS ITS WOOD AS THE METER CLIMBS.** Without the
/// `storage` lesson the build banks nothing, draws no wood and says why; with it the rack is raised
/// and exactly the rung's pile has left the store.
#[test]
fn a_rack_waits_on_storage_and_then_eats_its_wood() {
    let mut app = campaign_world();
    let (entity, faction, band, _) = first_band(&mut app);
    begin_the_rack(&mut app, entity, band);
    stock_wood(&mut app, entity, WOOD_STOCK);

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let pile = ladder
        .rung(drying_rack())
        .build
        .as_ref()
        .and_then(|build| build.materials.get(WOOD))
        .copied()
        .expect("the rack's build declares a pile of wood");

    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        rack_rung_held(&app, band),
        RungId::new(camp_stores(), "carried"),
        "without the lesson the rack is not raised"
    );
    assert_eq!(
        app.world
            .resource::<TileImprovementRegistry>()
            .bands()
            .get(band, camp_stores())
            .unwrap()
            .position(),
        0.0,
        "nothing is banked toward it"
    );
    assert!(
        (wood_held(&app, entity) - WOOD_STOCK).abs() < PILE_EPSILON,
        "and no wood is drawn: {}",
        wood_held(&app, entity)
    );

    learn_storage(&mut app, faction);
    run_until(&mut app, "the rack going up", |app| {
        rack_rung_held(app, band) == drying_rack()
    });
    assert!(
        (wood_held(&app, entity) - (WOOD_STOCK - pile)).abs() < PILE_EPSILON,
        "the climb drew the rung's pile of {pile} and nothing else: {} left of {WOOD_STOCK}",
        wood_held(&app, entity)
    );
    assert!(
        app.world
            .get::<LaborAllocation>(entity)
            .unwrap()
            .build_queue
            .is_empty(),
        "the entry retires at its destination"
    );
}

/// **A KEPT RACK HOLDS; AN UNKEPT ONE DECAYS BACK TO `carried` AFTER ITS GRACE.**
#[test]
fn an_unkept_rack_decays_to_carried_after_its_grace_and_a_kept_one_holds() {
    let mut app = campaign_world();
    let (entity, faction, band, _) = first_band(&mut app);
    begin_the_rack(&mut app, entity, band);
    stock_wood(&mut app, entity, WOOD_STOCK);
    learn_storage(&mut app, faction);
    run_until(&mut app, "the rack going up", |app| {
        rack_rung_held(app, band) == drying_rack()
    });

    // Long enough to outlast the grace several times over: a kept rack holds.
    for _ in 0..8 {
        app.update();
    }
    {
        let registry = app.world.resource::<TileImprovementRegistry>();
        let rack = registry.bands().get(band, camp_stores()).unwrap();
        assert_eq!(rack.held_rung(), drying_rack(), "a kept rack holds");
        assert_eq!(rack.neglect_turns, 0, "its bill is met every turn");
        assert!(rack.upkeep_hands > 0.0, "the crew's hands paid it");
    }

    let grace = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(drying_rack())
        .upkeep
        .as_ref()
        .expect("the rack declares a bill")
        .grace_turns;
    set_crew(&mut app, entity, rack_row(band), 0);
    for _ in 0..grace {
        app.update();
    }
    {
        let registry = app.world.resource::<TileImprovementRegistry>();
        let rack = registry.bands().get(band, camp_stores()).unwrap();
        assert_eq!(
            rack.held_rung(),
            drying_rack(),
            "the rack survives its grace"
        );
        assert!(rack.neglect_turns > 0, "the neglect clock is running");
    }
    run_until(&mut app, "the rack decaying to carried", |app| {
        rack_rung_held(app, band) == RungId::new(camp_stores(), "carried")
    });
}

/// **THE PIT IS PLACED AND KEPT BY A TILE CREW, AND DECAYS UNKEPT** - the tile kind's lifecycle on
/// the shipped `food_stores` branch.
#[test]
fn a_tile_crew_digs_and_keeps_a_pit_and_an_unkept_one_fills_in() {
    let mut app = campaign_world();
    let (entity, faction, _, tile) = first_band(&mut app);
    begin_the_pit(&mut app, entity, faction, tile);
    learn_storage(&mut app, faction);
    run_until(&mut app, "the pit being dug", |app| {
        pit_rung_held(app, tile) == storage_pit()
    });

    for _ in 0..8 {
        app.update();
    }
    {
        let registry = app.world.resource::<TileImprovementRegistry>();
        let pit = registry.get(tile, food_stores()).unwrap();
        assert_eq!(pit.held_rung(), storage_pit(), "a kept pit holds");
        assert_eq!(pit.neglect_turns, 0, "its bill is met every turn");
        assert!(pit.upkeep_hands > 0.0, "the tile crew paid it");
    }

    set_crew(&mut app, entity, pit_row(tile), 0);
    run_until(&mut app, "the pit filling in", |app| {
        pit_rung_held(app, tile) == RungId::new(food_stores(), "unstored")
    });
}

// ---------------------------------------------------------------------------------------------
// The identity of a band improvement: split, merge, save, wire
// ---------------------------------------------------------------------------------------------

/// **A SPLIT LEAVES THE RACK ON THE PARENT, AND THE NEW BAND STARTS AT THE ROOT.** The improvement
/// is keyed by the band, so the splinter - a band with a fresh id - has none.
#[test]
fn a_split_keeps_the_rack_on_the_parent_only() {
    let mut app = campaign_world();
    core_sim::run_turn(&mut app);
    let (parent, _, parent_id, _) = first_band(&mut app);
    hold_the_rack(&mut app, parent_id);

    let split = core_sim::split_band_from_parent(
        &mut app.world,
        parent,
        6,
        &core_sim::SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        },
    )
    .expect("the split is admitted");

    let registry = app.world.resource::<TileImprovementRegistry>();
    assert_eq!(
        registry
            .bands()
            .get(parent_id, camp_stores())
            .expect("the parent keeps its rack")
            .held_rung(),
        drying_rack()
    );
    assert!(
        registry.bands().get(split.band, camp_stores()).is_none(),
        "the new band holds nothing: it starts at the root"
    );
}

/// **A SAVE AND A CHECKPOINT CARRY A BAND'S RACK.**
#[test]
fn a_save_and_a_checkpoint_carry_a_bands_rack() {
    use core_sim::save::{decode_save, encode_save};
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let mut app = campaign_world();
    let (_, _, band, _) = first_band(&mut app);
    hold_the_rack(&mut app, band);
    let staged_position = app
        .world
        .resource::<TileImprovementRegistry>()
        .bands()
        .get(band, camp_stores())
        .unwrap()
        .position();

    let checkpoint = capture_sim_state(&app.world);
    app.world
        .insert_resource(TileImprovementRegistry::default());
    restore_sim_state(&mut app.world, &checkpoint);
    let restored = app
        .world
        .resource::<TileImprovementRegistry>()
        .bands()
        .get(band, camp_stores())
        .expect("the checkpoint carries the rack");
    assert_eq!(restored.held_rung(), drying_rack());
    assert_eq!(restored.position(), staged_position);

    let blob = encode_save(&app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    let saved = payload
        .sim
        .tile_improvements
        .bands()
        .get(band, camp_stores())
        .expect("the save carries the rack");
    assert_eq!(saved.held_rung(), drying_rack());
    assert_eq!(saved.position(), staged_position);
}

/// **A DEAD BAND'S CAMP IS FORGOTTEN** - its improvements are dropped rather than left to decay for
/// ever, and a live band's are untouched.
#[test]
fn a_dead_bands_rack_is_forgotten() {
    let mut app = campaign_world();
    let (_, _, band, _) = first_band(&mut app);
    let ghost = BandId(9_999);
    hold_the_rack(&mut app, band);
    hold_the_rack(&mut app, ghost);

    app.update();
    let registry = app.world.resource::<TileImprovementRegistry>();
    assert!(
        registry.bands().get(band, camp_stores()).is_some(),
        "a live band keeps its rack"
    );
    assert!(
        registry.bands().get(ghost, camp_stores()).is_none(),
        "no band carries the ghost's id, so its rack is forgotten"
    );
}

/// **THE WIRE**: a band's rack is published on the band's own row - its branch, the rung it holds, a
/// stamped bill that reconciles verbatim, and no builder's scratch once nothing is queued.
#[test]
fn a_bands_rack_rides_its_own_row_on_the_wire() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = campaign_world();
    let (entity, faction, band, _) = first_band(&mut app);
    begin_the_rack(&mut app, entity, band);
    stock_wood(&mut app, entity, WOOD_STOCK);
    learn_storage(&mut app, faction);
    run_until(&mut app, "the rack going up", |app| {
        rack_rung_held(app, band) == drying_rack()
    });
    app.update();
    app.update();

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .filter(|cohort| cohort.bandId() == band.0)
        .flat_map(|cohort| cohort.bandImprovements().into_iter().flatten())
        .next()
        .expect("the band's own row carries its rack");
    assert_eq!(row.branch().unwrap_or_default(), camp_stores().as_str());
    assert_eq!(
        row.currentRung().unwrap_or_default(),
        drying_rack().wire_key()
    );
    assert!(!row.isQueued(), "nothing is queued once the rack is up");
    assert!(
        row.buildDestinationRung().unwrap_or_default().is_empty(),
        "so no destination is published"
    );
    assert!(row.upkeepDemand() > 0.0, "a built rack owes a bill");
    assert!(
        (row.upkeepDemand() - row.upkeepSupplied() - row.upkeepShortfall()).abs() < 1.0e-4,
        "demand - supplied == shortfall verbatim"
    );
    assert!(
        row.upkeepWorkersNeeded() > 0,
        "the bill calls for whole hands"
    );
}

/// **THE KNOWLEDGE ROSTER ON THE WIRE**: `storage` is taught by rot, not by a rung, so it reaches
/// `ladderKnowledge` through `standalone_knowledges` - on the `stores` row under `food`, and a STEP
/// because it gates both stores. The roaming skills gate no rung and stay capabilities.
#[test]
fn the_published_roster_carries_storage_as_a_step_and_the_roaming_skills_as_capabilities() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = campaign_world();
    app.update();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let roster = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderKnowledge())
        .expect("the roster is published");
    let row = |id: &str| {
        roster
            .iter()
            .find(|row| row.knowledgeId() == Some(id))
            .unwrap_or_else(|| panic!("the roster carries {id}"))
    };

    let storage = row("storage");
    assert_eq!(storage.branch().unwrap_or_default(), "stores");
    assert_eq!(storage.area().unwrap_or_default(), "food");
    assert!(storage.isStep(), "storage gates the rack and the pit");
    for skill in ["herd_lore", "travois", "drives"] {
        let skill = row(skill);
        assert_eq!(skill.branch().unwrap_or_default(), "roam");
        assert!(!skill.isStep(), "a roaming skill gates no rung");
    }
}
