//! **A predator raid's food forfeit must reconcile with the band's larder** (Predators Phase 3).
//!
//! A casualty-causing raid costs the band a fraction of that turn's food income
//! (`predators.raid_yield_forfeit_fraction`), debited straight off `cohort.stores` by
//! `advance_predator_raids`. Like a pen's feed it is in **neither** `foodIncome` (Σ per-source
//! `actual`) nor `foodConsumption` (the people's debit), so without exporting it the client's net-food
//! row overstates the surplus by exactly the forfeit. `PopulationCohortState.raidForfeit` closes it,
//! extending the pen ledger identity by one term:
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit
//! ```
//!
//! Pinned against a **real turn** through the real systems and the real snapshot export — a foraging
//! band with a wolf on its camp — so `raidForfeit` is genuinely non-zero.

use bevy::prelude::Entity;
use core_sim::TakeSelection;
use core_sim::{
    available_workers, build_test_app, run_turn, scalar_from_f32, CommandEventKind,
    CommandEventLog, ForageRegistry, Herd, HerdRegistry, LaborAllocation, LaborAssignment,
    LaborTarget, PopulationCohort, SimulationConfig, SizeClass, SnapshotHistory, SourcePriority,
    Tile, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own.
const SEED: u64 = 119_304_647;
/// Ample larder so the whole forfeit is payable (never capped) and the identity has a clean
/// non-zero `raidForfeit` to reconcile.
const AMPLE_LARDER: f32 = 500.0;
/// The exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;
/// The shipped carnivore — `Grey Wolf Pack` (`attack 3`, `aggression 0.6` → raid attack `1.8 > 0`).
const WOLF: &str = "Grey Wolf Pack";

#[test]
fn the_food_ledger_reconciles_with_a_predator_raid() {
    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();

    let (band, band_tile_entity, workers) = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        let (e, c) = q.iter(&app.world).next().expect("a starting band");
        (e, c.current_tile, available_workers(c.working))
    };
    let band_pos = app
        .world
        .get::<Tile>(band_tile_entity)
        .expect("band tile")
        .position;

    // Forage a patch on the band's own tile or right beside it — inside `band_work_range`, so the
    // assignment actually yields food income (the thing a raid forfeits a slice of). ALL workers
    // gather (0 warriors), so the raid also lands its casualties on the exposed populace.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .filter(|p| p.x.abs_diff(band_pos.x).max(p.y.abs_diff(band_pos.y)) <= 1)
        .min_by_key(|p| (p.y, p.x))
        .expect("the starting band must sit on or beside a forage patch");
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Forage {
                tile: patch,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: workers.max(1),
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        }],
        ..Default::default()
    });

    // Seat a wolf ON the band's camp — a carnivore with `aggression > 0` in `raid_radius`, so the raid
    // trigger fires this turn (it may step ≤1 hex in Logistics, still well inside the default radius 2).
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            "ledger_wolf".to_string(),
            WOLF.to_string(),
            SizeClass::Small,
            vec![band_pos],
            120.0,
            120.0,
            0.0,
            0.15,
            30.0,
        ));

    // Seed a full larder so the forfeit is payable in full (never capped).
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("band")
        .stores
        .set(FOOD, scalar_from_f32(AMPLE_LARDER));

    let working_before = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .working
        .to_f32();
    let before = larder_of(&app, band);

    run_turn(&mut app);

    let after = larder_of(&app, band);
    let working_after = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .working
        .to_f32();

    // Read the numbers the CLIENT reads — the exported snapshot, not the sim's internals.
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|c| c.entity == band.to_bits())
        .expect("the resident band is exported");

    // **The raid happened: people died AND food was forfeited.**
    //
    // Read off the raid's own feed line rather than the band's net `working` delta. The delta was
    // never a clean signal — births move the same bracket — and since the take resolves through the
    // gate (`docs/plan_hunt_through_combat.md` §4.2) a wolf's `attack 3 × aggression 0.6 = 1.8`
    // clears a human's `defense 1` by only `0.8`, spread over the whole camp: real casualties, and a
    // fraction of a person, which one turn of births comfortably outruns. The forfeit below is gated
    // on those same casualties, so the ledger identity this file exists for is untouched.
    assert!(
        raid_casualties(&app) > 0.0,
        "the wolf must raid the camp and cost lives (working {working_before} -> {working_after})"
    );
    assert!(
        cohort.raid_forfeit > 0.0,
        "a casualty-causing raid on a foraging band must forfeit food (got {})",
        cohort.raid_forfeit
    );
    assert!(
        cohort.food_income > 0.0,
        "the foraging band earned income to forfeit (got {})",
        cohort.food_income
    );
    // The extended identity holds against the real larder movement.
    let delta = after - before;
    let ledger = cohort.food_income - cohort.food_consumption - cohort.raid_forfeit;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "larder_delta must equal foodIncome − foodConsumption − raidForfeit: \
         delta={delta} vs ledger={ledger} (income={} consumption={} raidForfeit={})",
        cohort.food_income,
        cohort.food_consumption,
        cohort.raid_forfeit,
    );

    // The bug the field exists to kill: the pre-fix net-food row (income − consumption) overstates the
    // true larder change by exactly the forfeit.
    let naive_net = cohort.food_income - cohort.food_consumption;
    assert!(
        (naive_net - delta - cohort.raid_forfeit).abs() < EPSILON,
        "the pre-fix readout overstates the true change by exactly the raid forfeit"
    );
}

/// The band's FOOD store, in `f32` — the number the ledger reconciles against.
fn larder_of(app: &bevy::app::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32()
}

/// The **killed** figure the `PredatorRaid` feed line reports this turn, summed over every raid — the
/// precise casualty count, where the cohort's `working` delta is the casualty count *net of births*.
/// The detail is `killed=<k> wounded=<w> …` (`advance_predator_raids`).
fn raid_casualties(app: &bevy::prelude::App) -> f32 {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::PredatorRaid)
        .filter_map(|entry| entry.detail.as_deref())
        .filter_map(|detail| {
            detail
                .split_whitespace()
                .find_map(|field| field.strip_prefix("killed="))
                .and_then(|value| value.parse::<f32>().ok())
        })
        .sum()
}
