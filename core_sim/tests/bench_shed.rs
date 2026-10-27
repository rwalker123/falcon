//! **THE CRAFTING BENCH IS A CANDIDATE IN THE SHEDDING ORDER** — the defect this file was opened
//! for, and the rules that keep it one (`docs/plan_standing_upkeep.md` §2.9).
//!
//! # What #871 changed
//!
//! The bench is one of the crews the player sets **by hand**, and those are the only crews the shed
//! walks now: a forage, hunt, extraction, tile or Roadwork row is staffed by the sim from what the
//! hand-set crews leave (`core_sim::staffing`), so a source row is no longer a shed candidate at all.
//! The bench therefore ranks beside the other hand-set roles - a spare warrior goes first - and the
//! three tests that ranked it against a SOURCE row (the bench thinned before the row that feeds the
//! band, a `High` bench outliving a `Low` source, the bench's last hand going before a source was
//! emptied) are deleted: there is no source row left for it to be ranked against.
//!
//! `BandBench` holds a crew out of the same pool `assign_labor` spends, but it is **not** a
//! `LaborTarget` and not a row in `assignments` — deliberately, because *"make IS the assignment"*
//! and a bench is not an in-range source. `LaborAllocation::normalize` walks `assignments`, so the
//! bench was **invisible to the shed**: a starving band stripped every worked row, every standing
//! role and its last builder while four crafters kept hammering.
//!
//! Driven through the real `advance_labor_allocation` rather than through `normalize` directly,
//! because the claim is about a band losing people in a turn — which is where the player met it.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_labor_allocation, build_test_app, scalar_from_f32, scalar_one, scalar_zero, BandBench,
    BandId, BenchOrder, DrawnInputs, DrawnMaterial, FactionId, GenerationId, LaborAllocation,
    LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand, Scalar,
    SourcePriority, StartingUnit, TileRegistry,
};

const FACTION: FactionId = FactionId(0);
const FIXTURE_BAND: u64 = 77;
/// The recipe the fixture bench carries. Its identity is inert here — nothing in this file advances
/// a craft — but a bench with no recipe is not running and holds no crew.
const RECIPE: &str = "sled";
/// The bench's crew, and the gatherers beside it. **Different numbers**, so an assertion that read
/// one for the other could not pass.
const CRAFTERS: u32 = 4;
/// **The fixture's order count** — the shed never advances the bench, so any count would do; a long
/// one says *"nothing here finishes an order"*.
const ORDER_COUNT: u32 = 10;
const WARRIORS: u32 = 3;

/// The tile the fixture band works and lives on.
fn source_tile(app: &App) -> UVec2 {
    app.world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .min_by_key(|tile| (tile.y, tile.x))
        .expect("worldgen seeded at least one forage patch")
}

/// A band with a standing warrior crew and a staffed bench, sized to afford **exactly** both — so the
/// pool it is holding is fully committed and any loss of people has to come off one of them.
fn world_with_a_band_at_the_bench(bench_priority: SourcePriority) -> (App, Entity, UVec2) {
    let mut app = build_test_app();
    app.update();
    let source = source_tile(&app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32((WARRIORS + CRAFTERS) as f32),
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
                predators_kept_off: 0,
                last_cohesion: Default::default(),
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
            BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    muster_crew: 0,
                    planned_crew: 0,
                    scouting_range: 0,
                    scouting_reach: 0,
                    max_workers: Some(WARRIORS),
                    staffing: Default::default(),
                    target: LaborTarget::Warrior,
                    workers: WARRIORS,
                    kit: None,
                    priority: SourcePriority::default(),
                }],
                ..Default::default()
            },
            BandBench {
                orders: vec![BenchOrder::new(RECIPE, ORDER_COUNT)],
                workers: CRAFTERS,
                last_output_grade: None,
                priority: bench_priority,
                last_started: Default::default(),
                finished: Vec::new(),
                auto: false,
                auto_skipped: Default::default(),
                lent_to_hunt: 0,
                kit_orders: Vec::new(),
                next_kit_order: 0,
            },
        ))
        .id();
    (app, band, source)
}

/// Take `lost` whole people out of the band's working-age bracket — the demographic event the shed
/// exists to answer, rather than a hand-written allocation the sim could not have produced.
fn lose_people(app: &mut App, band: Entity, lost: u32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band keeps its cohort");
    cohort.working -= Scalar::from_u32(lost);
}

fn bench_crew(app: &App, band: Entity) -> u32 {
    app.world
        .get::<BandBench>(band)
        .expect("the fixture band keeps its bench")
        .workers
}

fn warriors(app: &App, band: Entity) -> u32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the fixture band keeps its allocation")
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Warrior))
        .map_or(0, |row| row.workers)
}

/// **A BAND THAT LOSES PEOPLE SHEDS ITS SPARE WARRIORS BEFORE IT THINS ITS BENCH.**
///
/// Step 2 of the order is an unthreatened warrior - the cheapest hand - and the bench
/// is only a candidate at step 5, so the two hands the band lost come off the warriors and the crafters
/// keep hammering. (The case the bench was made a candidate for - a starving band stripping every
/// worked row while four crafters kept hammering - is the staffed rows' now: they are restaffed from
/// the smaller pool, and the bench is a hand-set crew like a warrior.)
#[test]
fn a_band_losing_people_sheds_a_spare_warrior_before_it_thins_its_bench() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::default());
    lose_people(&mut app, band, 2);
    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        warriors(&app, band),
        WARRIORS - 2,
        "the two hands the band lost came off the scouts"
    );
    assert_eq!(
        bench_crew(&app, band),
        CRAFTERS,
        "…and the bench kept its crew"
    );
}

/// **A BENCH WITH NOTHING BESIDE IT IS THINNED, ONE HAND AT A TIME.**
#[test]
fn a_bench_with_no_warriors_left_is_thinned_by_the_people_the_band_lost() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::default());
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        allocation.assignments.clear();
        allocation.last_yields.clear();
    }
    lose_people(&mut app, band, 1 + WARRIORS);
    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        bench_crew(&app, band),
        CRAFTERS - 1,
        "the one hand the band lost beyond what the warriors held came off the bench"
    );
}

/// **THE LAST HAND STALLS THE JOB; IT DOES NOT CLEAR IT.**
///
/// `BandBench::remove_order` **forfeits the order's drawn pile** — the materials are dropped rather
/// than returned to the store. The shed must never call it, so everything the player had is still
/// there afterwards and re-staffing resumes rather than restarts.
#[test]
fn a_stalled_bench_keeps_its_recipe_its_progress_and_the_pile_it_drew() {
    const PROGRESS: f32 = 3.5;
    const ITEMS_DONE: u32 = 2;

    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::default());
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("the fixture band keeps its bench");
        bench.workers = 1;
        let head = bench.head_mut().expect("the fixture bench has an order");
        head.progress = scalar_from_f32(PROGRESS);
        head.made = ITEMS_DONE;
        head.drawn = Some(DrawnInputs {
            reading: Some(0.5),
            grade: Some("good".to_string()),
            withdrawn: vec![DrawnMaterial {
                material: "hide".to_string(),
                amount: scalar_from_f32(2.0),
            }],
        });
    }
    // A bench of one and nobody left to field it: the last hand goes and the job stalls.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        allocation.assignments.clear();
        allocation.last_yields.clear();
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band keeps its cohort");
        cohort.working = scalar_from_f32(0.0);
    }
    app.world.run_system_once(advance_labor_allocation);

    let bench = app
        .world
        .get::<BandBench>(band)
        .expect("the fixture band keeps its bench");
    assert_eq!(bench.workers, 0, "the bench stalled");
    assert_eq!(
        bench.head_recipe(),
        Some(RECIPE),
        "the order is still the order the player chose"
    );
    let head = bench
        .head()
        .expect("the shed never takes an order off the bench");
    assert_eq!(
        head.progress,
        scalar_from_f32(PROGRESS),
        "its progress stands, so re-crewing RESUMES rather than restarting"
    );
    assert_eq!(head.made, ITEMS_DONE, "and the finished count is intact");
    let withdrawn = head
        .drawn
        .as_ref()
        .expect("⛔ the drawn pile is FORFEITED by remove_order — the shed must never call it");
    assert_eq!(
        withdrawn.withdrawn.len(),
        1,
        "the materials already cut are still on the bench, not dropped on the floor"
    );
    assert_eq!(
        withdrawn.grade.as_deref(),
        Some("good"),
        "…and so is the grade the draw fixed, which the design says never moves"
    );
}

/// **RE-STAFFING RESUMES.** The other half of the claim above, driven: hands put back on a stalled
/// bench pick up the progress that was already banked.
#[test]
fn a_re_crewed_bench_resumes_from_the_progress_it_stalled_on() {
    const BANKED: f32 = 3.5;

    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::default());
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("the fixture band keeps its bench");
        bench.workers = 0;
        bench
            .head_mut()
            .expect("the fixture bench has an order")
            .progress = scalar_from_f32(BANKED);
    }
    // The player puts hands back on it. Nothing else about the job changed.
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("the fixture band keeps its bench");
        bench.workers = CRAFTERS;
    }
    let bench = app
        .world
        .get::<BandBench>(band)
        .expect("the fixture band keeps its bench");
    assert_eq!(
        bench.head().expect("the stall kept the order").progress,
        scalar_from_f32(BANKED),
        "the banked progress survived the stall, so the crew coming back continues the same item"
    );
    assert_eq!(bench.workers, CRAFTERS, "…and the crew is back on it");
}

/// **THE MARK SURVIVES A CHECKPOINT.** It rides `BandRecord::bench` with the rest of the bench; a
/// rollback that forgot it would silently re-rank the band's work.
#[test]
fn the_benchs_mark_survives_a_checkpoint_round_trip() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::Low);
    let state = capture_sim_state(&app.world);
    // Scramble the live value, so a restore that did nothing would be visible.
    app.world
        .get_mut::<BandBench>(band)
        .expect("the fixture band keeps its bench")
        .priority = SourcePriority::High;
    restore_sim_state(&mut app.world, &state);

    let restored = app
        .world
        .query::<&BandBench>()
        .iter(&app.world)
        .find(|bench| bench.head_recipe() == Some(RECIPE))
        .expect("the restored world carries the fixture band's bench");
    assert_eq!(
        restored.priority,
        SourcePriority::Low,
        "the rank came back with the bench — a checkpoint that forgot it would re-rank the band"
    );
}
