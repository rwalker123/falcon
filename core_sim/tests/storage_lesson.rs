//! **ROT TEACHES STORAGE** (#707, `.claude/rules/core_sim/campaign.md` → "Rot teaches
//! storage"). A surplus that sits in a band's larder until it spoils is the practice that teaches
//! the `storage` lesson through the same ledger the ladder uses; a band with no excess never
//! learns it. These drive the turn's real systems in the Population chain's order — the meal
//! (`simulate_population`), then the larder rot (`rot_band_larders`) — on a real band.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;

use core_sim::spoilage::{
    rot_band_larders, storage_practice, STORAGE_DISCOVERY_ID, STORAGE_KNOWLEDGE,
};
use core_sim::{
    build_test_app, knows, scalar_from_f32, scalar_one, scalar_zero, simulate_population,
    BandEquipment, BandId, DemographicsConfigHandle, DiscoveryProgressLedger, EquipmentConfig,
    FactionId, GenerationId, LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, SimulationTick, TileRegistry,
};

const BAND: u64 = 31;
/// A faction no opening band belongs to, so the ledger reads are this fixture's alone.
const FACTION: FactionId = FactionId(77);
/// The keeping class of the meat the larder holds; its shelf life is read off the shipped table.
const MEAT_CLASS: &str = "flesh";
/// A larder far past what a turn's meal eats, so the surplus is certain to be left to rot.
const SURPLUS: f32 = 100_000.0;
/// Fewer provisions than one turn's meal needs: the band eats all of it and nothing is left to rot.
const SCANT: f32 = 0.01;
/// A dry tile on the harness map for the camp.
const CAMP: (u32, u32) = (17, 24);
const BAND_SIZE: u32 = 30;
const CREW: f32 = 6.0;
/// Tolerance for ledger progress, which is stored fixed-point.
const EPSILON: f32 = 1e-3;

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
                last_food_kept: 0.0,
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
                last_cohesion: Default::default(),
            },
        ))
        .id()
}

fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = spawn_band(&mut app);
    (app, band)
}

/// Replace the band's larder with `amount` of meat on the last turn of its shelf life, so the rot
/// pass of this turn takes whatever the meal leaves.
fn stock_a_lot_about_to_expire(app: &mut App, band: Entity, amount: f32) {
    let shelf = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(MEAT_CLASS)
        .expect("the shipped table keeps meat") as u32;
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.stores = LocalStore::new();
    cohort
        .stores
        .add_food_aged(MEAT_CLASS, scalar_from_f32(amount), shelf - 1);
}

/// One turn's meal then rot, the order the Population chain runs them.
fn meal_then_rot(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(simulate_population);
    app.world.run_system_once(rot_band_larders);
}

fn storage_progress(app: &App) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FACTION, STORAGE_DISCOVERY_ID)
        .to_f32()
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

#[test]
fn a_surplus_that_rots_teaches_storage_by_the_predicted_credit() {
    let (mut app, band) = world();
    stock_a_lot_about_to_expire(&mut app, band, SURPLUS);
    meal_then_rot(&mut app);

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (rotted, kept, need) = {
        let c = cohort(&app, band);
        (c.last_food_spoiled, c.last_food_kept, c.last_food_need)
    };
    assert!(
        rotted > 0.0 && need > 0.0,
        "fixture: rot {rotted}, need {need}"
    );
    let practice = storage_practice(rotted, kept, need, &ladder.knowledge);
    let expected = ladder
        .knowledge
        .ledger_credit(STORAGE_KNOWLEDGE, practice)
        .expect("storage is priced");
    assert!(expected > 0.0, "the surplus pays something");
    let got = storage_progress(&app);
    assert!(
        (got - expected).abs() < EPSILON,
        "got {got}, expected {expected}"
    );
}

#[test]
fn a_band_with_no_surplus_learns_nothing() {
    let (mut app, band) = world();
    // A scant FRESH lot: eaten the turn it landed, so it neither rots nor counts as kept.
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.stores = LocalStore::new();
        cohort.stores.add_food(MEAT_CLASS, scalar_from_f32(SCANT));
    }
    meal_then_rot(&mut app);

    let c = cohort(&app, band);
    assert!(
        c.last_food_need > SCANT,
        "fixture: the meal outruns the larder"
    );
    assert_eq!(c.last_food_spoiled, 0.0, "fixture: nothing rotted");
    assert_eq!(storage_progress(&app), 0.0);
}

#[test]
fn full_rate_rot_for_the_lessons_length_teaches_it() {
    let (mut app, band) = world();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let knowledge = &ladder.knowledge;
    let turns =
        (knowledge.lesson_cost(STORAGE_KNOWLEDGE).unwrap() / knowledge.learn_rate).ceil() as u32;
    let known = |app: &App| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            FACTION,
            STORAGE_DISCOVERY_ID,
            knowledge.completion_threshold,
        )
    };
    for turn in 1..=turns {
        assert!(!known(&app), "learned early, before turn {turn}");
        stock_a_lot_about_to_expire(&mut app, band, SURPLUS);
        meal_then_rot(&mut app);
    }
    assert!(
        known(&app),
        "after {turns} turns progress is {}",
        storage_progress(&app)
    );
}

/// A class that keeps for many turns, so a lot of it is eaten long before it rots.
const KEEPING_CLASS: &str = "dry";

fn stock_fresh(app: &mut App, band: Entity, class: &str, amount: f32) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.stores = LocalStore::new();
    cohort.stores.add_food(class, scalar_from_f32(amount));
}

/// **KEPT FOOD TEACHES STORAGE, SLOWLY (#708), through the real meal and rot systems.** A band that
/// rots nothing still learns from eating food it had kept a while, at the kept weight's share of
/// what rot pays - and a lot is kept from the SECOND meal after it landed, not the first.
#[test]
fn eating_kept_food_teaches_storage_at_the_kept_weight_and_only_from_the_second_meal() {
    let (mut app, band) = world();
    stock_fresh(&mut app, band, KEEPING_CLASS, SURPLUS);

    // Meal T+1: the lot landed last turn and has not aged - fresh, no practice.
    meal_then_rot(&mut app);
    assert_eq!(cohort(&app, band).last_food_spoiled, 0.0, "nothing rots");
    assert_eq!(
        cohort(&app, band).last_food_kept,
        0.0,
        "fresh at the first meal"
    );
    assert_eq!(storage_progress(&app), 0.0);

    // Meal T+2: the same lot has aged a turn - kept at the shipped threshold of 2.
    meal_then_rot(&mut app);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (kept, need) = {
        let c = cohort(&app, band);
        (c.last_food_kept, c.last_food_need)
    };
    assert!(
        kept > 0.0 && need > 0.0,
        "fixture: kept {kept}, need {need}"
    );
    assert_eq!(
        cohort(&app, band).last_food_spoiled,
        0.0,
        "still nothing rots"
    );
    let practice = storage_practice(0.0, kept, need, &ladder.knowledge);
    let expected = ladder
        .knowledge
        .ledger_credit(STORAGE_KNOWLEDGE, practice)
        .expect("storage is priced");
    let got = storage_progress(&app);
    assert!(expected > 0.0, "kept food pays something");
    assert!(
        (got - expected).abs() < EPSILON,
        "got {got}, expected {expected}"
    );
    // And it is the weight's share of what a full turn of rot would pay.
    let full_rot = ladder
        .knowledge
        .ledger_credit(STORAGE_KNOWLEDGE, ladder.knowledge.learn_rate)
        .unwrap();
    assert!(got < full_rot * ladder.knowledge.storage_kept_weight + EPSILON);
}

/// **A caravan pack and a party's own meals never teach**: the credit is the band's own larder
/// meal, so a band whose meal ate nothing kept pays nothing however old its pack was.
#[test]
fn a_band_that_eats_only_fresh_food_learns_nothing_even_with_an_old_lot_beside_it() {
    let (mut app, band) = world();
    stock_fresh(&mut app, band, MEAT_CLASS, SURPLUS);
    {
        // An old lot of the SLOWER class sits untouched: the meal draws the fastest class first.
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort
            .stores
            .add_food_aged(KEEPING_CLASS, scalar_from_f32(SURPLUS), 10);
    }
    meal_then_rot(&mut app);
    assert_eq!(cohort(&app, band).last_food_kept, 0.0);
    assert_eq!(storage_progress(&app), 0.0);
}

/// A walk long enough that the pack's spoilage age already exceeds the kept threshold, yet well
/// inside the class's shelf life.
const WALK_TURNS: u32 = 5;

/// **KEPT MEANS SAT IN THE LARDER, NOT WALKED HOME (#708).** A caravan pack lands home with its
/// spoilage age already set to its walk. That age must not count as larder time: the pack is fresh
/// at its first meal however far it came, and kept only once it has sat the threshold turns.
#[test]
fn a_far_pack_is_not_kept_at_its_first_meal_and_is_once_it_has_sat_the_threshold() {
    let (mut app, band) = world();
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.stores = LocalStore::new();
        // Exactly what `land_food_home` does with a pack that walked `WALK_TURNS`.
        cohort
            .stores
            .add_food_aged(KEEPING_CLASS, scalar_from_f32(SURPLUS), WALK_TURNS);
    }
    let threshold = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .storage_kept_age_turns;
    assert!(
        WALK_TURNS >= threshold,
        "fixture: the walk alone must reach the threshold"
    );

    // Meal after landing: the pack has walked, but it has not sat in the larder.
    meal_then_rot(&mut app);
    assert_eq!(
        cohort(&app, band).last_food_kept,
        0.0,
        "a walked pack is fresh at its first meal"
    );
    assert_eq!(storage_progress(&app), 0.0);

    // Every later meal until the threshold is met: the last of them eats a lot that has sat
    // `threshold` turns (the meal reads the clock before that turn's rot pass advances it).
    for _ in 1..threshold {
        meal_then_rot(&mut app);
    }
    assert!(
        cohort(&app, band).last_food_kept > 0.0,
        "kept once it has sat {threshold} turns in the larder"
    );
    assert!(storage_progress(&app) > 0.0);
}
