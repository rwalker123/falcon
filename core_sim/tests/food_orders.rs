//! The chief's food and fodder orders (#860 step 3, `.claude/rules/core_sim/work-party.md` -> "A chief
//! keeps an ordered material on hand" and "The food and fodder orders"). The band under test has a
//! chief; the larder and the band's per-turn need are set directly so the order's turns are exact.

use bevy::prelude::{App, Entity, With};
use core_sim::food_orders::OrderReadings;
use core_sim::keep_on_hand::{release_row, source_key};
use core_sim::{
    build_test_app, scalar_from_f32, BandLeader, BandLeadership, BandOrder, LaborAllocation,
    LaborConfigHandle, LaborTarget, LeaderKind, OrderOwner, PopulationCohort, ResidentBand,
    SimulationConfig, SourcePriority, TakeSelection, DEFAULT_ESCAPEMENT_FLOOR,
};

const FOOD: &str = "food";
const FODDER: &str = "fodder";
/// The per-turn need the fixture sets: well above zero so a draw exists.
const NEED: f32 = 4.0;
/// A larder that covers far more than any target.
const MANY_TURNS_OF_FOOD: f32 = NEED * 200.0;

/// A world whose band has a chief, only the food and fodder orders listed.
fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    {
        let mut handle = app.world.resource_mut::<LaborConfigHandle>();
        let mut config = (*handle.0).clone();
        config.band_orders.ordered_kinds = vec![FOOD.to_string(), FODDER.to_string()];
        handle.0 = std::sync::Arc::new(config);
    }
    let band = {
        let mut query = app.world.query_filtered::<Entity, With<ResidentBand>>();
        query.iter(&app.world).next().expect("a resident band")
    };
    app.world.entity_mut(band).insert(BandLeadership {
        leader: Some(BandLeader {
            name: "Ash".to_string(),
            kind: LeaderKind::Chief,
            since_turn: 0,
        }),
        ..Default::default()
    });
    (app, band)
}

/// Set what the band holds of food and what it eats a turn, then run the managing pass and staffing.
fn set_larder(app: &mut App, band: Entity, stock: f32) {
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.last_food_need = NEED;
        cohort
            .stores
            .reset_food("fresh_plant", scalar_from_f32(stock));
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
}

fn set_target(app: &mut App, band: Entity, kind: &str, target: u32) {
    let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
    allocation.orders.insert(
        kind.to_string(),
        BandOrder {
            target,
            priority: SourcePriority::Normal,
            owner: OrderOwner::Chief,
        },
    );
}

/// `(key, floor, whole basket, crew)` of every row the chief lists under `kind`.
fn chief_rows(app: &App, band: Entity, kind: &str) -> Vec<(String, f32, bool, u32)> {
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    allocation
        .assignments
        .iter()
        .filter_map(|row| {
            let key = source_key(&row.target)?;
            (allocation.order_rows.get(&key).map(String::as_str) == Some(kind)).then(|| {
                let (floor, basket) = match &row.target {
                    LaborTarget::Forage {
                        floor,
                        take_species,
                        ..
                    } => (*floor, *take_species == TakeSelection::EVERYTHING),
                    LaborTarget::Hunt { floor, .. } => (*floor, true),
                    _ => unreachable!("only forage and hunt rows are listed"),
                };
                (key, floor, basket, row.staffing.total())
            })
        })
        .collect()
}

#[test]
fn below_the_target_the_chief_works_sources_best_first_at_the_best_floor_and_carries_every_plant() {
    let (mut app, band) = world();
    set_target(&mut app, band, FOOD, 1000);
    set_larder(&mut app, band, NEED);
    let rows = chief_rows(&app, band, FOOD);
    assert!(
        !rows.is_empty(),
        "**LIVENESS**: a short larder puts the chief on food sources"
    );
    for (key, floor, basket, _) in &rows {
        assert_eq!(
            *floor, DEFAULT_ESCAPEMENT_FLOOR,
            "{key} is worked at Best below the target"
        );
        assert!(*basket, "{key} carries the whole basket");
    }
    assert!(
        rows.iter().map(|row| row.3).sum::<u32>() > 0,
        "**LIVENESS**: hands are sent"
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert!(allocation.keep_sustainable[FOOD] > 0.0);
}

#[test]
fn at_the_target_the_chief_eases_to_the_learning_floor_and_keeps_the_keep_up_crew() {
    let (mut app, band) = world();
    set_target(&mut app, band, FOOD, 5);
    set_larder(&mut app, band, MANY_TURNS_OF_FOOD);
    let learning = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_orders
        .learning_floor;
    let rows = chief_rows(&app, band, FOOD);
    assert!(
        !rows.is_empty(),
        "**LIVENESS**: the band eats every turn, so the chief still works food at target"
    );
    for (key, floor, _, _) in &rows {
        assert_eq!(*floor, learning, "{key} eases to Learning at the target");
        assert!(*floor >= DEFAULT_ESCAPEMENT_FLOOR, "never below Best");
    }
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert!(
        allocation
            .keep_shares
            .iter()
            .any(|(kind, share)| kind == FOOD && share.take > 0.0),
        "the keep-up share is the draw, not zero"
    );
}

#[test]
fn a_row_the_player_released_is_left_alone_counted_and_handed_back_by_a_claim() {
    let (mut app, band) = world();
    set_target(&mut app, band, FOOD, 1000);
    set_larder(&mut app, band, NEED);
    let (key, floor, _, crew) = chief_rows(&app, band, FOOD)
        .into_iter()
        .next()
        .expect("a chief row");
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        release_row(&mut allocation, &key);
        assert!(!allocation.order_rows.contains_key(&key));
        let max = allocation
            .assignments
            .iter()
            .find(|row| source_key(&row.target).as_deref() == Some(key.as_str()))
            .and_then(|row| row.max_workers);
        assert_eq!(max, Some(crew.max(1)), "the Max is the crew it had");
    }
    // Even at a target the chief would ease to Learning, the released row keeps its own floor.
    set_target(&mut app, band, FOOD, 5);
    set_larder(&mut app, band, MANY_TURNS_OF_FOOD);
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    let row = allocation
        .assignments
        .iter()
        .find(|row| source_key(&row.target).as_deref() == Some(key.as_str()))
        .expect("the released row is never retired by the chief");
    assert!(row.max_workers.is_some(), "the chief never clears its Max");
    match &row.target {
        LaborTarget::Forage { floor: held, .. } | LaborTarget::Hunt { floor: held, .. } => {
            assert_eq!(*held, floor, "the chief never touches its floor")
        }
        _ => unreachable!(),
    }
    assert!(allocation.row_order(&row.target).is_none());

    // Hand it back: the chief lists it again and clears the Max.
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        core_sim::keep_on_hand::claim_row(&mut allocation, &key, FOOD);
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(
        allocation.order_rows.get(&key).map(String::as_str),
        Some(FOOD)
    );
}

#[test]
fn a_herd_that_is_gone_loses_its_row_and_an_improvement_row_is_never_retired() {
    let (mut app, band) = world();
    set_target(&mut app, band, FOOD, 1000);
    set_larder(&mut app, band, NEED);
    let rows = chief_rows(&app, band, FOOD);
    let mut hunts = rows.iter().filter(|row| row.0.starts_with("h:")).cloned();
    let (key, ..) = hunts.next().expect("**LIVENESS**: the chief hunts");
    let (pinned, ..) = hunts.next().expect("a second herd to pin");
    // Cargo is walking home from the second herd: its row is pinned and must stay.
    let target = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .find(|row| source_key(&row.target).as_deref() == Some(pinned.as_str()))
        .map(|row| row.target.clone())
        .unwrap();
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .homeward
        .push(core_sim::work_party::HomewardWalk {
            target,
            workers: 1,
            turns_left: 2,
            walk_turns: 2,
            cargo: 5.0,
            classes: Default::default(),
            goods: Default::default(),
            rejoins_row: false,
        });
    for gone in [&key, &pinned] {
        let id = gone.trim_start_matches("h:").to_string();
        app.world
            .resource_mut::<core_sim::HerdRegistry>()
            .herds
            .retain(|herd| herd.id != id);
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    let after = chief_rows(&app, band, FOOD);
    assert!(
        !after.iter().any(|row| row.0 == key),
        "a herd that is gone loses its row"
    );
    assert!(
        after.iter().any(|row| row.0 == pinned),
        "a row with cargo out is pinned and stays"
    );
    // An improved patch's row stays even when the want no longer needs it.
    let forage: Vec<String> = chief_rows(&app, band, FOOD)
        .into_iter()
        .map(|row| row.0)
        .filter(|key| key.starts_with("f:"))
        .collect();
    assert!(!forage.is_empty(), "**LIVENESS**: the chief gathers");
    let keep = forage.last().cloned().unwrap();
    let tile = {
        let coords: Vec<u32> = keep
            .trim_start_matches("f:")
            .split(',')
            .map(|part| part.parse().unwrap())
            .collect();
        bevy::prelude::UVec2::new(coords[0], coords[1])
    };
    {
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let mut forage = app.world.resource_mut::<core_sim::ForageRegistry>();
        let patch = forage.patches.get_mut(&tile).expect("the patch");
        let position = patch.ladder_position() + 1.0;
        patch.set_ladder_position(position, &ladder);
    }
    // A tiny need: one source covers it, so the improved row falls out of the set.
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.last_food_need = 0.05;
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
    assert!(
        chief_rows(&app, band, FOOD).iter().any(|row| row.0 == keep),
        "an improvement row keeps existing even at a zero share"
    );
}

#[test]
fn with_no_penned_herd_the_fodder_order_wants_nothing_and_runs_no_rows() {
    let (mut app, band) = world();
    set_target(&mut app, band, FODDER, 10);
    set_larder(&mut app, band, NEED);
    assert!(chief_rows(&app, band, FODDER).is_empty());
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(allocation.last_fodder_need, 0.0);
    let readings = OrderReadings::default();
    assert_eq!(readings.turns(FODDER, allocation), 0.0);
}

// ---------------------------------------------------------------------------------------------
// Fodder: hay Fields, the hay draw of a pen, and the by-product rule (#860 step 3 follow-up).
// ---------------------------------------------------------------------------------------------

use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The hay crop `flora_config.json` ships (the one fodder species).
const HAY: &str = "hay_grass";
/// What the pens need beyond their pasture, per turn.
const HAY_NEED: f32 = 1.0;
/// A position past the top of the plant ladder: a sown Field.
const A_FIELD: f32 = 1000.0;

/// The chief's `n` nearest gathering tiles, found by letting the food order pick them first.
fn gathering_tiles(app: &mut App, band: Entity, n: usize) -> Vec<bevy::prelude::UVec2> {
    set_target(app, band, FOOD, 1000);
    set_larder(app, band, NEED);
    let mut tiles: Vec<bevy::prelude::UVec2> = chief_rows(app, band, FOOD)
        .into_iter()
        .filter_map(|row| {
            let coords = row.0.strip_prefix("f:")?;
            let (x, y) = coords.split_once(',')?;
            Some(bevy::prelude::UVec2::new(x.parse().ok()?, y.parse().ok()?))
        })
        .collect();
    tiles.truncate(n);
    assert_eq!(
        tiles.len(),
        n,
        "**LIVENESS**: enough gathering land in reach"
    );
    // Start clean: no rows, no roster, so the converted Fields meet the managing pass fresh.
    let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
    allocation.assignments.clear();
    allocation.last_yields.clear();
    allocation.order_rows.clear();
    allocation.keep_shares.clear();
    tiles
}

/// Sow `tile` to hay: a committed Field whose basket yields fodder and no food.
fn sow_hay(app: &mut App, tile: bevy::prelude::UVec2) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let mut forage = app.world.resource_mut::<core_sim::ForageRegistry>();
    let patch = forage.patches.get_mut(&tile).expect("the patch");
    patch.species = Some(HAY.to_string());
    patch.set_ladder_position(A_FIELD, &ladder);
}

fn pens(app: &mut App, band: Entity, need: f32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .last_fodder_need = need;
}

/// The fodder order's total take across its shares, in biomass.
fn fodder_take(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .unwrap()
        .keep_shares
        .iter()
        .filter(|(kind, _)| kind == FODDER)
        .map(|(_, share)| share.take)
        .sum()
}

/// The `bandOrders` row for `kind` as the encoded frame publishes it: `(hasConsumers, onHand, draw,
/// target)`.
fn published_order(app: &mut App, kind: &str) -> (bool, f32, f32, u32) {
    core_sim::recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .expect("the population section");
    for row in section.populations().expect("populations").iter() {
        for order in row.bandOrders().into_iter().flatten() {
            if order.kind() == Some(kind) {
                return (
                    order.hasConsumers(),
                    order.onHand(),
                    order.draw(),
                    order.target(),
                );
            }
        }
    }
    panic!("no {kind} order on the wire");
}

#[test]
fn the_fodder_order_fills_hay_fields_at_best_and_publishes_whether_anything_eats_hay() {
    let (mut app, band) = world();
    core_sim::run_turn(&mut app);
    let tiles = gathering_tiles(&mut app, band, 1);
    sow_hay(&mut app, tiles[0]);
    set_target(&mut app, band, FODDER, 10);
    set_target(&mut app, band, FOOD, 1);

    // No pen eats hay: nothing is wanted, nothing runs, and the wire says there is no consumer.
    set_larder(&mut app, band, MANY_TURNS_OF_FOOD);
    assert!(chief_rows(&app, band, FODDER).is_empty());
    let (consumers, ..) = published_order(&mut app, FODDER);
    assert!(!consumers, "no pen, no Fodder line");

    // A pen whose pasture covers only part of its need.
    pens(&mut app, band, HAY_NEED);
    set_larder(&mut app, band, MANY_TURNS_OF_FOOD);
    pens(&mut app, band, HAY_NEED);
    core_sim::staffing::restaff_band(&mut app.world, band);
    let rows = chief_rows(&app, band, FODDER);
    assert_eq!(
        rows.len(),
        1,
        "the hay Field is the fodder order's: {rows:?}"
    );
    assert_eq!(
        rows[0].1, DEFAULT_ESCAPEMENT_FLOOR,
        "worked at Best below the target"
    );
    assert!(rows[0].3 > 0, "**LIVENESS**: hands are sent to the hay");
    assert!(fodder_take(&app, band) > 0.0);
    assert!(
        chief_rows(&app, band, FOOD)
            .iter()
            .all(|row| row.0 != format!("f:{},{}", tiles[0].x, tiles[0].y)),
        "a hay Field gives more fodder than food, so it is not listed under food"
    );
    let (consumers, turns, draw, target) = published_order(&mut app, FODDER);
    assert!(consumers, "a pen eats hay");
    assert_eq!(draw, HAY_NEED, "the draw is the hay need, units a turn");
    assert_eq!(target, 10, "the target is in turns");
    assert_eq!(turns, 0.0, "no hay on hand: zero turns");
}

#[test]
fn what_the_other_rows_brought_in_last_turn_reduces_the_want_and_a_row_staffed_to_nobody_does_not()
{
    let (mut app, band) = world();
    core_sim::run_turn(&mut app);
    let tiles = gathering_tiles(&mut app, band, 2);
    sow_hay(&mut app, tiles[0]);
    sow_hay(&mut app, tiles[1]);
    set_target(&mut app, band, FODDER, 10);
    set_target(&mut app, band, FOOD, 1);

    let run = |app: &mut App, brought_in: Option<f32>| -> f32 {
        {
            let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
            allocation.assignments.clear();
            allocation.last_yields.clear();
            allocation.order_rows.clear();
            allocation.keep_shares.clear();
            if let Some(fodder) = brought_in {
                // The second Field is a mixed patch the FOOD order lists; it yielded `fodder` hay
                // last turn.
                allocation.set_staffed_row(
                    LaborTarget::Forage {
                        tile: tiles[1],
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    None,
                    None,
                );
                allocation
                    .order_rows
                    .insert(format!("f:{},{}", tiles[1].x, tiles[1].y), FOOD.to_string());
                let last = allocation.assignments.len() - 1;
                allocation.last_yields[last].fodder = fodder;
            }
        }
        set_larder(app, band, MANY_TURNS_OF_FOOD);
        pens(app, band, HAY_NEED);
        core_sim::staffing::restaff_band(&mut app.world, band);
        fodder_take(app, band)
    };
    // The want is the draw plus a tenth of ten turns' deficit: two units a turn.
    let alone = run(&mut app, None);
    assert!(alone > 0.0, "**LIVENESS**: the hay Field is worked");
    let staffed_to_nobody = run(&mut app, Some(0.0));
    assert!(
        (staffed_to_nobody - alone).abs() < 1e-4,
        "a row that brought in nothing does not reduce the want: {staffed_to_nobody} vs {alone}"
    );
    let brought_in = run(&mut app, Some(1.5));
    assert!(
        brought_in < alone * 0.5,
        "a row that brought in 1.5 of the 2 wanted leaves a quarter of it: {brought_in} vs {alone}"
    );
}

#[test]
fn the_shipped_learning_floor_is_the_clients_learning_preset() {
    let config = core_sim::LaborConfig::builtin();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../clients/godot_thin_client/src/scripts/ui/hud/SourceForecast.gd");
    let text = std::fs::read_to_string(path).expect("the client's SourceForecast.gd");
    let learn: f32 = text
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("FLOOR_PRESET_LEARN:")
                .map(|rest| rest.trim_start())
        })
        .and_then(|rest| rest.split([',', ' ', '#']).next())
        .and_then(|number| number.parse().ok())
        .expect("a FLOOR_PRESET_LEARN entry in FLOOR_PRESET_VALUES");
    assert_eq!(config.band_orders.learning_floor, learn);
}

// ---------------------------------------------------------------------------------------------
// The arrival line: materials as amounts, food and fodder as turns, fodder only with pens.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_arrival_line_reads_food_and_fodder_as_turns_and_materials_as_amounts() {
    use core_sim::keep_on_hand::{takeover_line, Keeper};
    let taken = |kinds: &[(&str, u32)]| -> Vec<(String, u32)> {
        kinds
            .iter()
            .map(|(kind, target)| (kind.to_string(), *target))
            .collect()
    };
    let chief = |kind: LeaderKind| {
        Keeper::of(
            Some(&BandLeadership {
                leader: Some(BandLeader {
                    name: "Orrin".to_string(),
                    kind,
                    since_turn: 0,
                }),
                ..Default::default()
            }),
            "Bitterbrook",
        )
        .unwrap()
    };
    assert_eq!(
        takeover_line(
            &chief(LeaderKind::Chief),
            "Bitterbrook",
            &taken(&[("stone", 30), ("wood", 30), ("food", 20)])
        ),
        "Orrin is chief of Bitterbrook \u{2014} he takes over the band's work: he keeps 20 turns of food, 30 stone and 30 wood on hand."
    );
    assert_eq!(
        takeover_line(
            &chief(LeaderKind::Chief),
            "Bitterbrook",
            &taken(&[("stone", 30), ("wood", 30), ("food", 20), ("fodder", 10)])
        ),
        "Orrin is chief of Bitterbrook \u{2014} he takes over the band's work: he keeps 20 turns of food, 10 turns of fodder, 30 stone and 30 wood on hand."
    );
    assert_eq!(
        takeover_line(
            &chief(LeaderKind::ElderCouncil),
            "Bitterbrook",
            &taken(&[("food", 20), ("stone", 30)])
        ),
        "Bitterbrook's elders take over the band's work: they keep 20 turns of food and 30 stone on hand."
    );
}

#[test]
fn the_feed_leaves_fodder_out_of_the_arrival_line_while_no_pen_eats_hay() {
    let (mut app, _band) = world();
    for _ in 0..2 {
        core_sim::run_turn(&mut app);
    }
    let lines: Vec<String> = app
        .world
        .resource::<core_sim::CommandEventLog>()
        .iter()
        .filter(|entry| entry.label.contains("takes over the band's work"))
        .map(|entry| entry.label.clone())
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("20 turns of food"), "{lines:?}");
    assert!(!lines[0].contains("fodder"), "{lines:?}");
}
