//! A band's chief keeps an ordered material on hand (#860 / #874, `.claude/rules/core_sim/work-party.md`
//! -> "A chief keeps an ordered material on hand"). An order belongs to a band that has a leader: the
//! chief takes over every ordered kind when he arrives, the player can take a kind back ("Crew it
//! yourself") and hand it to him again.

use std::collections::BTreeMap;

use bevy::prelude::{App, Entity, UVec2, With};
use core_sim::extraction::{
    deposit_rung_span, tile_deposit_capacity, DepositRegistry, DepositSource,
};
use core_sim::keep_on_hand::{claim_order, release_order};
use core_sim::shipped;
use core_sim::work_party::HomewardWalk;
use core_sim::{
    build_test_app, scalar_from_f32, BandLeader, BandLeadership, BandOrder, CommandEventLog,
    ExtractionConfigHandle, LaborAllocation, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    LeaderKind, MaterialsConfigHandle, OrderOwner, PopulationCohort, ResidentBand,
    SimulationConfig, SourcePriority, Tile, TileRegistry,
};
use sim_schema::TerrainType;

const STONE: &str = "stone";
const WOOD: &str = "wood";
/// Rock that holds a body of stone and no timber.
const ROCK: TerrainType = TerrainType::AlpineMountain;
/// A target well above anything the fixture band stores.
/// Ground that holds no stone, so the camp itself is no candidate.
const BARREN: TerrainType = TerrainType::DeepOcean;
const A_BIG_TARGET: u32 = 500;

/// A world whose band has a chief already: the first managing pass is his arrival.
fn world() -> (App, Entity, UVec2) {
    let (mut app, band, camp) = leaderless_world();
    give_a_leader(&mut app, band);
    (app, band, camp)
}

/// Name the kinds a chief takes orders for; the stone tests run on a stone-only config so the timber
/// the same rock may hold does not add rows to what they count.
fn ordered(app: &mut App, kinds: &[&str]) {
    let mut handle = app.world.resource_mut::<LaborConfigHandle>();
    let mut config = (*handle.0).clone();
    config.band_orders.ordered_kinds = kinds.iter().map(|kind| kind.to_string()).collect();
    handle.0 = std::sync::Arc::new(config);
}

/// A world whose band crews by hand: no leader, so no order exists and nothing is managed.
fn leaderless_world() -> (App, Entity, UVec2) {
    let mut app = build_test_app();
    app.update();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    let (band, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
        let (band, cohort) = query.iter(&app.world).next().expect("a resident band");
        (band, cohort.current_tile)
    };
    let camp = app.world.get::<Tile>(tile).expect("camp tile").position;
    app.world.entity_mut(band).remove::<BandLeadership>();
    ordered(&mut app, &[STONE]);
    (app, band, camp)
}

fn give_a_leader(app: &mut App, band: Entity) {
    app.world.entity_mut(band).insert(BandLeadership {
        leader: Some(BandLeader {
            name: "Ash".to_string(),
            kind: LeaderKind::Chief,
            since_turn: 0,
        }),
        ..Default::default()
    });
}

fn reground(app: &mut App, at: UVec2, terrain: TerrainType) {
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("on the map");
    app.world.get_mut::<Tile>(entity).expect("tile").terrain = terrain;
}

/// Every tile holds no stone, so only the tiles a test regrounds are candidates.
fn bare_the_map(app: &mut App) {
    let mut tiles = app.world.query::<&mut Tile>();
    for mut tile in tiles.iter_mut(&mut app.world) {
        tile.terrain = BARREN;
    }
}

/// The chief's stone order at `target`, then a restaff (the managing pass).
fn keep(app: &mut App, band: Entity, target: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .orders
        .insert(
            STONE.to_string(),
            BandOrder {
                target,
                priority: SourcePriority::Normal,
                owner: OrderOwner::Chief,
            },
        );
    core_sim::staffing::restaff_band(&mut app.world, band);
}

fn stone_order(app: &App, band: Entity) -> BandOrder {
    *app.world
        .get::<LaborAllocation>(band)
        .expect("allocation")
        .order_for(STONE)
        .expect("a stone order")
}

fn stone_rows(app: &App, band: Entity) -> Vec<(UVec2, u32)> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("allocation")
        .assignments
        .iter()
        .filter_map(|row| match &row.target {
            LaborTarget::Extract { tile, material, .. } if material == STONE => {
                Some((*tile, row.staffing.total()))
            }
            _ => None,
        })
        .collect()
}

fn bank_stone(app: &mut App, band: Entity, amount: f32) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let config = app.world.resource::<ExtractionConfigHandle>().get();
    let readings: BTreeMap<String, f32> = config
        .deposit(STONE)
        .and_then(|def| def.terrain(ROCK))
        .expect("stone on rock")
        .characteristics
        .clone();
    let key = materials
        .band_key(STONE, &readings)
        .expect("stone is rated");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("cohort")
        .stores
        .deposit_material(STONE, key, scalar_from_f32(amount), &readings);
}

/// Seat a working on `tile` at the top of `rung`.
fn seat(app: &mut App, tile: UVec2, rung: core_sim::RungId) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let config = app.world.resource::<ExtractionConfigHandle>().get();
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("tile");
    let capacity = tile_deposit_capacity(&config, STONE, app.world.get::<Tile>(entity).expect("t"));
    let mut working =
        DepositSource::opening(tile, STONE, capacity, shipped::root_standing(rung.branch));
    let (base, width) = deposit_rung_span(rung, &ladder);
    working.set_ladder_position(base + width, &ladder, rung.branch);
    app.world.resource_mut::<DepositRegistry>().insert(working);
}

#[test]
fn below_the_target_a_dig_row_is_placed_and_staffed_and_at_target_nobody_digs() {
    let (mut app, band, camp) = world();
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    let rows = stone_rows(&app, band);
    assert_eq!(rows.len(), 1, "one dig row per band per mined material");
    assert!(
        rows[0].1 > 0,
        "**LIVENESS**: a deficit sends diggers: {rows:?}"
    );

    bank_stone(&mut app, band, A_BIG_TARGET as f32);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let rows = stone_rows(&app, band);
    assert_eq!(rows.len(), 1, "the row stands at target");
    assert_eq!(rows[0].1, 0, "at target nobody digs: {rows:?}");
}

/// **ARRIVAL**: the first managing pass over a band with a leader gives it the stone order at the
/// configured starting target, owned by the chief, and queues the announcement.
#[test]
fn a_chiefs_arrival_creates_the_stone_order_at_the_starting_target() {
    let (mut app, band, camp) = leaderless_world();
    ordered(&mut app, &[STONE, WOOD]);
    reground(&mut app, camp, ROCK);
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .unwrap()
            .orders
            .is_empty(),
        "no leader, no order"
    );
    give_a_leader(&mut app, band);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let order = stone_order(&app, band);
    let opening = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_orders
        .starting_target_for(STONE)
        .expect("stone opens at a target");
    assert_eq!(order.target, opening);
    assert_eq!(order.owner, OrderOwner::Chief);
    assert_eq!(order.priority, SourcePriority::Normal);
    assert_eq!(
        app.world
            .get::<LaborAllocation>(band)
            .unwrap()
            .unannounced_takeover,
        vec![STONE.to_string(), WOOD.to_string()],
        "the chief takes over every ordered kind, in config order"
    );
}

/// **THE ARRIVAL IS TOLD ONCE, PER BAND**, through a real turn.
#[test]
fn the_arrival_is_one_feed_line_naming_the_chief_and_the_band() {
    let (mut app, band, camp) = leaderless_world();
    ordered(&mut app, &[STONE, WOOD]);
    reground(&mut app, camp, ROCK);
    give_a_leader(&mut app, band);
    for _ in 0..2 {
        core_sim::run_turn(&mut app);
    }
    let opening = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_orders
        .starting_target_for(STONE)
        .unwrap();
    let wanted = format!(
        "he takes over the band's work: he keeps {opening} stone and {opening} wood on hand"
    );
    let lines: Vec<String> = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.label.contains(&wanted))
        .map(|entry| entry.label.clone())
        .collect();
    assert_eq!(lines.len(), 1, "one line, once: {lines:?}");
    assert!(lines[0].starts_with("Ash is chief of "), "{lines:?}");
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .unwrap()
            .unannounced_takeover
            .is_empty(),
        "the queue drained"
    );
}

/// **CREW IT YOURSELF**: the chief's row stays where it is with its crew as the Max and the priority
/// kept; the kind is the player's, and later passes leave it alone.
#[test]
fn releasing_keeps_the_row_in_place_with_the_crew_as_its_max() {
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    let before = stone_rows(&app, band);
    assert_eq!(before.len(), 1);
    assert!(before[0].1 > 0, "**LIVENESS**: the chief sends diggers");
    let crew = app.world.get::<LaborAllocation>(band).unwrap().assignments[0].workers;
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.assignments[0].priority = SourcePriority::High;
        release_order(&mut allocation, STONE);
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(stone_order(&app, band).owner, OrderOwner::Player);
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(allocation.assignments.len(), 1, "no row removed");
    assert_eq!(
        allocation.assignments[0].max_workers,
        Some(crew.max(1)),
        "the Max is the crew it was staffed with"
    );
    assert_eq!(allocation.assignments[0].priority, SourcePriority::High);
    assert_eq!(stone_rows(&app, band)[0].0, before[0].0, "same site");
    assert!(
        stone_rows(&app, band)[0].1 > 0,
        "the hand crew is staffed to its Max"
    );
}

/// A row that was staffed to nobody (stone at target) is released with a Max of one, so a
/// never-renewing dig keeps the count the hand-crew rule needs.
#[test]
fn a_released_row_staffed_to_nobody_gets_a_max_of_one() {
    let (mut app, band, camp) = world();
    reground(&mut app, camp, ROCK);
    bank_stone(&mut app, band, A_BIG_TARGET as f32);
    keep(&mut app, band, A_BIG_TARGET);
    assert_eq!(stone_rows(&app, band)[0].1, 0, "at target nobody digs");
    release_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        STONE,
    );
    assert_eq!(
        app.world.get::<LaborAllocation>(band).unwrap().assignments[0].max_workers,
        Some(1)
    );
}

/// **HAND IT TO THE CHIEF**: a player-owned kind goes back to the chief at its current target, and the
/// managing pass takes the rows over as on arrival (Max cleared).
#[test]
fn claiming_takes_the_kind_back_at_its_target_and_clears_the_max() {
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    release_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        STONE,
    );
    core_sim::staffing::restaff_band(&mut app.world, band);
    claim_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        STONE,
        1,
    );
    core_sim::staffing::restaff_band(&mut app.world, band);
    let order = stone_order(&app, band);
    assert_eq!(order.owner, OrderOwner::Chief);
    assert_eq!(
        order.target, A_BIG_TARGET,
        "its current target, not the opening one"
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(allocation.assignments.len(), 1);
    assert!(allocation.assignments[0].max_workers.is_none());
}

/// A kind the player owns is crewed exactly as a leaderless band crews it: the managing pass leaves
/// a hand-made row alone, whoever leads the band.
#[test]
fn a_player_owned_kind_is_crewed_by_hand_under_a_chief() {
    const MAX: u32 = 2;
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    core_sim::staffing::restaff_band(&mut app.world, band);
    release_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        STONE,
    );
    hand_row(&mut app, band, camp, MAX);
    let rows = stone_rows(&app, band);
    assert_eq!(rows.len(), 1, "the hand-made row stands");
    assert_eq!(rows[0].1, MAX, "its Max is the crew: {rows:?}");
    assert_eq!(stone_order(&app, band).owner, OrderOwner::Player);
}

#[test]
fn the_draw_sums_the_three_seams_and_sets_the_hold_crew() {
    let (mut app, band, camp) = world();
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    let hold_without = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Extract { .. }))
        .map(|row| row.staffing.hold_crew)
        .expect("row");
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.last_material_need.insert(STONE.into(), 1.0);
        allocation.last_road_material_bill.insert(STONE.into(), 2.0);
        allocation
            .last_build_material_draw
            .insert(STONE.into(), 3.0);
        assert!((allocation.material_draw(STONE) - 6.0).abs() < 1e-6);
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    let hold_with = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Extract { .. }))
        .map(|row| row.staffing.hold_crew)
        .expect("row");
    assert!(
        hold_with > hold_without,
        "a draw of 6 a turn needs a bigger hold crew: {hold_without} -> {hold_with}"
    );
}

#[test]
fn a_quarry_outranks_a_nearer_gathering_site_and_a_tie_goes_to_the_nearer() {
    let (mut app, band, camp) = world();
    let width = app.world.resource::<TileRegistry>().width;
    let near = UVec2::new((camp.x + 1) % width, camp.y);
    let far = UVec2::new((camp.x + 3) % width, camp.y);
    bare_the_map(&mut app);
    reground(&mut app, near, ROCK);
    reground(&mut app, far, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    assert_eq!(
        stone_rows(&app, band)
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        vec![near],
        "on a tie the nearer wins"
    );

    seat(&mut app, far, shipped::extraction_quarry());
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(
        stone_rows(&app, band)
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        vec![far],
        "a quarry (more per hand) outranks the nearer gathering site"
    );
}

#[test]
fn a_row_with_cargo_walking_home_does_not_go_until_the_cargo_lands() {
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    let target = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Extract { .. }))
        .map(|row| row.target.clone())
        .expect("row");
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .push(HomewardWalk {
            target,
            workers: 1,
            turns_left: 2,
            walk_turns: 2,
            cargo: 5.0,
            classes: Default::default(),
            goods: Default::default(),
            rejoins_row: false,
        });
    // The best site moves (the camp's rock is worked out) but a load is walking home from the row.
    let config = app.world.resource::<ExtractionConfigHandle>().get();
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(camp.x, camp.y)
        .unwrap();
    let capacity = tile_deposit_capacity(&config, STONE, app.world.get::<Tile>(entity).unwrap());
    let mut empty = DepositSource::opening(
        camp,
        STONE,
        capacity,
        shipped::root_standing(shipped::extraction()),
    );
    empty.stock = 0.0;
    app.world.resource_mut::<DepositRegistry>().insert(empty);
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(
        stone_rows(&app, band).len(),
        1,
        "a load is walking home: the row stays"
    );
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .clear();
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert!(
        stone_rows(&app, band).is_empty(),
        "once the cargo lands the worked-out row goes"
    );
}

#[test]
fn a_row_moves_off_a_site_that_is_worked_out() {
    let (mut app, band, camp) = world();
    let width = app.world.resource::<TileRegistry>().width;
    let near = UVec2::new((camp.x + 1) % width, camp.y);
    let far = UVec2::new((camp.x + 3) % width, camp.y);
    bare_the_map(&mut app);
    reground(&mut app, near, ROCK);
    reground(&mut app, far, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    assert_eq!(stone_rows(&app, band)[0].0, near);
    // Work the near body out.
    let config = app.world.resource::<ExtractionConfigHandle>().get();
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(near.x, near.y)
        .unwrap();
    let capacity = tile_deposit_capacity(&config, STONE, app.world.get::<Tile>(entity).unwrap());
    let mut empty = DepositSource::opening(
        near,
        STONE,
        capacity,
        shipped::root_standing(shipped::extraction()),
    );
    empty.stock = 0.0;
    app.world.resource_mut::<DepositRegistry>().insert(empty);
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(
        stone_rows(&app, band)[0].0,
        far,
        "the row moved to the rock that is left"
    );
}

/// **THROUGH REAL TURNS**: with no order a band digs nothing; told to keep stone it digs it home and
/// the dig stops near the target (overshooting by at most a pack).
#[test]
fn real_turns_dig_stone_home_up_to_the_target_and_not_without_one() {
    const TARGET: u32 = 12;
    const TURNS: u32 = 6;
    const OVERSHOOT_PACKS: f32 = 20.0;
    let stone_in_store = |app: &App, band: Entity| {
        app.world
            .get::<PopulationCohort>(band)
            .expect("cohort")
            .stores
            .material_total(STONE)
            .to_f32()
    };
    let (mut app, band, camp) = leaderless_world();
    reground(&mut app, camp, ROCK);
    for _ in 0..TURNS {
        core_sim::run_turn(&mut app);
    }
    assert_eq!(stone_in_store(&app, band), 0.0, "no order, no stone is dug");

    give_a_leader(&mut app, band);
    keep(&mut app, band, TARGET);
    for _ in 0..TURNS {
        core_sim::run_turn(&mut app);
    }
    let held = stone_in_store(&app, band);
    assert!(
        held >= TARGET as f32 * 0.5,
        "**LIVENESS**: the band dug stone home: {held}"
    );
    assert!(
        held < TARGET as f32 * OVERSHOOT_PACKS,
        "the dig stopped near the target instead of running on: {held}"
    );
}

/// A hand-made stone row at `tile`, staffed with a Max, as the player makes one on main.
fn hand_row(app: &mut App, band: Entity, tile: UVec2, max: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .set_staffed_row(
            LaborTarget::Extract {
                tile,
                material: STONE.to_string(),
                floor: 0.0,
            },
            Some(max),
            None,
        );
    core_sim::staffing::restaff_band(&mut app.world, band);
}

/// **A LEADERLESS BAND'S HAND ROW STANDS**, and an order set on it without a leader places nothing.
#[test]
fn a_leaderless_hand_row_is_left_alone() {
    const MAX: u32 = 2;
    let (mut app, band, camp) = leaderless_world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    hand_row(&mut app, band, camp, MAX);
    let rows = stone_rows(&app, band);
    assert_eq!(rows.len(), 1, "the hand-made row stands");
    assert_eq!(rows[0].1, MAX, "its Max is the crew: {rows:?}");
    assert!(app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .orders
        .is_empty());
}

/// **THE TAKEOVER**: on his arrival the chief keeps the hand row at the best site (Max cleared) and
/// retires the others.
#[test]
fn an_arrival_takes_over_a_hand_row_and_retires_the_others() {
    const MAX: u32 = 2;
    let (mut app, band, camp) = leaderless_world();
    bare_the_map(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let far = UVec2::new((camp.x + 3) % width, camp.y);
    reground(&mut app, camp, ROCK);
    reground(&mut app, far, ROCK);
    hand_row(&mut app, band, far, MAX);
    give_a_leader(&mut app, band);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let rows = stone_rows(&app, band);
    assert_eq!(rows.len(), 1, "one row: the order's");
    assert_eq!(
        rows[0].0, camp,
        "the hand row at the farther site was retired for the best one"
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert!(allocation.chief_owns(STONE));
    assert!(allocation.assignments[0].max_workers.is_none());
}

#[test]
fn a_pinned_hand_row_survives_a_takeover_until_its_cargo_lands() {
    let (mut app, band, camp) = leaderless_world();
    bare_the_map(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let far = UVec2::new((camp.x + 3) % width, camp.y);
    reground(&mut app, camp, ROCK);
    reground(&mut app, far, ROCK);
    hand_row(&mut app, band, far, 2);
    let target = app.world.get::<LaborAllocation>(band).unwrap().assignments[0]
        .target
        .clone();
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .push(HomewardWalk {
            target,
            workers: 1,
            turns_left: 2,
            walk_turns: 2,
            cargo: 5.0,
            classes: Default::default(),
            goods: Default::default(),
            rejoins_row: false,
        });
    give_a_leader(&mut app, band);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let rows = stone_rows(&app, band);
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![far, camp],
        "cargo is walking home from the hand row: it stays, and the best site gets its row"
    );
    assert_eq!(
        rows[0].1, 0,
        "the pinned row off the set is staffed to nobody"
    );
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .clear();
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(
        stone_rows(&app, band)
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        vec![camp],
        "once it lands the order moves the row to the best site"
    );
}

#[test]
fn a_leaderless_band_is_never_owned_by_an_order() {
    let (mut app, band, camp) = leaderless_world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    assert!(
        stone_rows(&app, band).is_empty(),
        "an order without a leader places no row"
    );
    hand_row(&mut app, band, camp, 2);
    assert_eq!(stone_rows(&app, band).len(), 1, "the hand row stands");
}

// ---------------------------------------------------------------------------------------------
// Wood: the chief works as many stands as the want needs, each at the Best floor (#860 step 2).
// ---------------------------------------------------------------------------------------------

/// Standing timber.
const WOODLAND: TerrainType = TerrainType::MixedWoodland;
/// How many woodland stands the wood fixtures lay down.
const STANDS: usize = 3;

/// A leader world on bare ground with `STANDS` woodland tiles beside the camp, the chief's order for
/// wood only.
fn timber_world() -> (App, Entity, Vec<UVec2>) {
    let (mut app, band, camp) = world();
    ordered(&mut app, &[WOOD]);
    bare_the_map(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    let stands: Vec<UVec2> = (0..STANDS as u32)
        .map(|step| UVec2::new((camp.x + step) % width, camp.y))
        .collect();
    for stand in &stands {
        reground(&mut app, *stand, WOODLAND);
    }
    (app, band, stands)
}

fn keep_wood(app: &mut App, band: Entity, target: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation")
        .orders
        .insert(
            WOOD.to_string(),
            BandOrder {
                target,
                priority: SourcePriority::Normal,
                owner: OrderOwner::Chief,
            },
        );
    core_sim::staffing::restaff_band(&mut app.world, band);
}

/// `(tile, floor, crew)` of every wood row.
fn wood_rows(app: &App, band: Entity) -> Vec<(UVec2, f32, u32)> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("allocation")
        .assignments
        .iter()
        .filter_map(|row| match &row.target {
            LaborTarget::Extract {
                tile,
                material,
                floor,
            } if material == WOOD => Some((*tile, *floor, row.staffing.total())),
            _ => None,
        })
        .collect()
}

fn sustainable(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .unwrap()
        .keep_sustainable[WOOD]
}

/// What one stand gives a turn at Best, read off the published total of an order that wants more
/// than every stand together.
fn one_stands_give(app: &mut App, band: Entity) -> f32 {
    keep_wood(app, band, A_BIG_TARGET * 20);
    sustainable(app, band) / STANDS as f32
}

fn is_wood_row(row: &core_sim::LaborAssignment) -> bool {
    matches!(&row.target, LaborTarget::Extract { material, .. } if material == WOOD)
}

/// **A WANT BIGGER THAN ONE STAND GIVES TAKES SEVERAL**, nearest first, each at the Best floor, and
/// the diggers are really sent.
#[test]
fn a_wood_order_takes_several_stands_when_one_gives_too_little() {
    let (mut app, band, stands) = timber_world();
    let each = one_stands_give(&mut app, band);
    assert!(each > 0.0, "**LIVENESS**: a woodland stand regrows");
    // A want of one and a half stands' regrowth, with nothing on hand and no draw.
    keep_wood(&mut app, band, (each * 15.0).ceil() as u32);
    let rows = wood_rows(&app, band);
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        stands[..2].to_vec(),
        "two stands cover the want, the nearest two"
    );
    for (tile, floor, _) in &rows {
        assert_eq!(
            *floor,
            core_sim::DEFAULT_ESCAPEMENT_FLOOR,
            "the chief works {tile:?} at Best"
        );
    }
    assert!(
        rows.iter().map(|row| row.2).sum::<u32>() > 0,
        "**LIVENESS**: diggers are sent"
    );
}

/// **THE STAND LIST SHRINKS WHEN THE WANT FALLS**, and a stand with cargo walking home from it is
/// kept (staffed to nobody) until the cargo lands.
#[test]
fn the_stand_list_shrinks_when_the_want_falls_and_a_pinned_row_stays() {
    let (mut app, band, stands) = timber_world();
    let each = one_stands_give(&mut app, band);
    keep_wood(&mut app, band, (each * 25.0).ceil() as u32);
    assert_eq!(
        wood_rows(&app, band).len(),
        STANDS,
        "a big want works every stand"
    );

    // Cargo is walking home from the farthest stand.
    let far = *stands.last().unwrap();
    let target = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .find(|row| {
            matches!(&row.target, LaborTarget::Extract { tile, material, .. }
                if *tile == far && material == WOOD)
        })
        .map(|row| row.target.clone())
        .expect("a row on the far stand");
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .push(HomewardWalk {
            target,
            workers: 1,
            turns_left: 2,
            walk_turns: 2,
            cargo: 5.0,
            classes: Default::default(),
            goods: Default::default(),
            rejoins_row: false,
        });
    keep_wood(&mut app, band, (each * 5.0).ceil() as u32);
    let rows = wood_rows(&app, band);
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![stands[0], far],
        "half a stand's want: the best stand and the pinned one"
    );
    assert_eq!(
        rows[1].2, 0,
        "the pinned row is off the set, staffed to nobody"
    );

    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .clear();
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert_eq!(
        wood_rows(&app, band)
            .iter()
            .map(|row| row.0)
            .collect::<Vec<_>>(),
        vec![stands[0]],
        "once the cargo lands the list is the set"
    );
}

/// **WHEN THE LAND GIVES TOO LITTLE THE CHIEF STILL NEVER OVERDRAWS**: every stand is worked at
/// Best, and the order publishes that the stands give less than the band uses.
#[test]
fn stands_that_regrow_less_than_the_draw_are_all_worked_at_best_and_publish_the_shortfall() {
    let (mut app, band, _) = timber_world();
    let each = one_stands_give(&mut app, band);
    let draw = each * STANDS as f32 * 2.0;
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .last_material_need
        .insert(WOOD.to_string(), draw);
    keep_wood(&mut app, band, A_BIG_TARGET);
    let rows = wood_rows(&app, band);
    assert_eq!(rows.len(), STANDS, "every stand is worked");
    assert!(
        rows.iter()
            .all(|row| row.1 == core_sim::DEFAULT_ESCAPEMENT_FLOOR),
        "all at Best: {rows:?}"
    );
    let published = sustainable(&app, band);
    assert!(
        published < draw,
        "the order says the stands give {published} a turn against a draw of {draw}"
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    for (_, share) in allocation.keep_shares.iter() {
        assert!(
            share.take <= each * 1.001,
            "no row is staffed past what its stand regrows: {share:?} vs {each}"
        );
    }
}

/// **RELEASE KEEPS EVERY WOOD ROW** with its crew as the Max, and a claim hands them back with the
/// Max cleared.
#[test]
fn releasing_wood_keeps_every_row_with_its_crew_as_the_max() {
    let (mut app, band, _) = timber_world();
    let each = one_stands_give(&mut app, band);
    keep_wood(&mut app, band, (each * 25.0).ceil() as u32);
    let before = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .filter(|row| is_wood_row(row))
        .map(|row| (row.target.clone(), row.workers))
        .collect::<Vec<_>>();
    assert_eq!(before.len(), STANDS);
    release_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        WOOD,
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    for (target, crew) in &before {
        let row = allocation
            .assignments
            .iter()
            .find(|row| &row.target == target)
            .expect("the row is still there, floor and all");
        assert_eq!(row.max_workers, Some((*crew).max(1)));
    }
    claim_order(
        &mut app.world.get_mut::<LaborAllocation>(band).unwrap(),
        WOOD,
        1,
    );
    core_sim::staffing::restaff_band(&mut app.world, band);
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert!(allocation
        .assignments
        .iter()
        .filter(|row| is_wood_row(row))
        .all(|row| row.max_workers.is_none()));
}

/// **STONE STILL ENDS WITH ONE ROW** however much rock is in reach: it never regrows, so its share
/// is unbounded and the best site covers the want.
#[test]
fn stone_ends_with_exactly_one_row_among_many_rock_sites() {
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    for step in 0..STANDS as u32 {
        reground(&mut app, UVec2::new((camp.x + step) % width, camp.y), ROCK);
    }
    keep(&mut app, band, A_BIG_TARGET);
    assert_eq!(stone_rows(&app, band).len(), 1);
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert!(
        allocation.keep_sustainable[STONE] >= allocation.material_draw(STONE),
        "stone never reads short"
    );
}

/// **THE ORDER'S CREW IS THE STAFFING'S OWN NUMBER**: a band short of hands staffs the High order's
/// keep-up crew in full and the Low order's short, and the order reports both from the passes.
#[test]
fn a_band_short_of_hands_reports_the_low_orders_hold_crew_short_while_the_high_one_is_full() {
    let (mut app, band, camp) = world();
    ordered(&mut app, &[STONE, WOOD]);
    bare_the_map(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    reground(&mut app, camp, ROCK);
    for step in 1..=STANDS as u32 {
        reground(
            &mut app,
            UVec2::new((camp.x + step) % width, camp.y),
            WOODLAND,
        );
    }
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        for kind in [STONE, WOOD] {
            allocation.last_material_need.insert(kind.to_string(), 2.0);
        }
        for (kind, priority) in [(STONE, SourcePriority::High), (WOOD, SourcePriority::Low)] {
            allocation.orders.insert(
                kind.to_string(),
                BandOrder {
                    target: A_BIG_TARGET,
                    priority,
                    owner: OrderOwner::Chief,
                },
            );
        }
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    let crew = |app: &App, kind: &str| {
        app.world
            .get::<LaborAllocation>(band)
            .unwrap()
            .order_crew(kind)
    };
    let (stone_heads, stone_wanted, ..) = crew(&app, STONE);
    let (wood_heads, wood_wanted, ..) = crew(&app, WOOD);
    assert!(
        stone_wanted > 0 && wood_wanted > 0,
        "**LIVENESS**: both orders want a keep-up crew"
    );
    assert_eq!(
        stone_heads, stone_wanted,
        "with hands to spare both are full"
    );
    assert_eq!(wood_heads, wood_wanted);

    // Only the hands the High order's keep-up crew needs.
    app.world.get_mut::<PopulationCohort>(band).unwrap().working =
        scalar_from_f32(stone_wanted as f32);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let (stone_heads, stone_wanted, ..) = crew(&app, STONE);
    let (wood_heads, wood_wanted, ..) = crew(&app, WOOD);
    assert_eq!(stone_heads, stone_wanted, "the High order is full");
    assert!(
        wood_heads < wood_wanted,
        "the Low order is short: {wood_heads} of {wood_wanted}"
    );
}

/// **THE ORDER'S TAKE IS WHAT ITS ROWS BROUGHT IN LAST TURN**, in the order's own units.
#[test]
fn an_orders_take_is_what_its_rows_brought_in_last_turn() {
    let (mut app, band, camp) = world();
    bare_the_map(&mut app);
    reground(&mut app, camp, ROCK);
    keep(&mut app, band, A_BIG_TARGET);
    let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
    assert_eq!(allocation.order_take(STONE), 0.0, "nothing brought in yet");
    let index = allocation
        .assignments
        .iter()
        .position(
            |row| matches!(&row.target, LaborTarget::Extract { material, .. } if material == STONE),
        )
        .expect("the dig row");
    allocation.last_yields[index].materials = vec![core_sim::MaterialPayoff {
        material: STONE.to_string(),
        amount: 2.5,
    }];
    assert_eq!(allocation.order_take(STONE), 2.5);
}
