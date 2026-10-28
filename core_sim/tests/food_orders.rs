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
