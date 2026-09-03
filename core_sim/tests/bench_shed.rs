//! **THE CRAFTING BENCH IS A CANDIDATE IN THE SHEDDING ORDER** — the defect this file was opened
//! for, and the rules that keep it one (`docs/plan_standing_upkeep.md` §2.9).
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
    BandId, DrawnInputs, DrawnMaterial, FactionId, GenerationId, LaborAllocation, LaborAssignment,
    LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand, Scalar, SourcePriority,
    StartingUnit, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};

const FACTION: FactionId = FactionId(0);
const FIXTURE_BAND: u64 = 77;
/// The recipe the fixture bench carries. Its identity is inert here — nothing in this file advances
/// a craft — but a bench with no recipe is not running and holds no crew.
const RECIPE: &str = "sled";
/// The bench's crew, and the gatherers beside it. **Different numbers**, so an assertion that read
/// one for the other could not pass.
const CRAFTERS: u32 = 4;
const GATHERERS: u32 = 3;

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

/// A band with one worked Forage row and a staffed bench, sized to afford **exactly** both — so the
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
                working: scalar_from_f32((GATHERERS + CRAFTERS) as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
            BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: source,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: core_sim::TakeSelection::EVERYTHING,
                    },
                    workers: GATHERERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
            BandBench {
                recipe_id: Some(RECIPE.to_string()),
                workers: CRAFTERS,
                progress: scalar_zero(),
                drawn: None,
                items_completed: 0,
                last_output_grade: None,
                priority: bench_priority,
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

fn gatherers(app: &App, band: Entity) -> u32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the fixture band keeps its allocation")
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Forage { .. }))
        .map_or(0, |row| row.workers)
}

/// **THE REPORTED DEFECT** — a band that loses people while it holds a staffed bench.
///
/// At equal priority the bench is the first thing thinned, and that is not a preference: a craft
/// pays into **no** food, fodder or material account and carries no per-worker yield, so it ranks
/// bottom on both of the levels the shed orders on. In a famine that is the right default, and it is
/// exactly what the mark exists to override.
#[test]
fn a_band_losing_people_thins_its_bench_before_the_row_that_feeds_it() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::default());
    lose_people(&mut app, band, 2);
    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        bench_crew(&app, band),
        CRAFTERS - 2,
        "the two hands the band lost came off the bench — it was invisible to the shed, so a \
         starving band used to strip every worked row while the crafters kept hammering"
    );
    assert_eq!(
        gatherers(&app, band),
        GATHERERS,
        "…and the row that feeds the band kept its crew"
    );
}

/// **THE MARK IS WHAT DECIDES IT, NOT THE BENCH BEING A BENCH.** With the bench `High` and the row
/// `Low`, the row gives instead — so the ranking above is an ordering the player controls and not a
/// hard-wired preference for rows over crafts.
#[test]
fn a_high_marked_bench_outlives_a_low_marked_source_in_the_same_step() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::High);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        allocation.assignments[0].priority = SourcePriority::Low;
    }
    lose_people(&mut app, band, 1);
    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        bench_crew(&app, band),
        CRAFTERS,
        "the bench the player marked High keeps its crew"
    );
    assert_eq!(
        gatherers(&app, band),
        GATHERERS - 1,
        "…and the row they marked Low gives the hand, though it is the one that pays"
    );
}

/// ⛔ **A `High` BENCH STILL STALLS BEFORE A `Low` SOURCE IS EMPTIED, AND THAT IS THE DESIGN.**
///
/// The bench's last hand is its own step, above the step that empties a source, because a stalled
/// craft **ends nothing** — the recipe, the progress and the drawn pile all stand — where emptying a
/// row drops it and takes its queued build with it. **The steps encode consequence; the mark orders
/// candidates within a step.** It is the same rule already pinned for rows, where an unimproved
/// `High` row is emptied before an improved `Normal` one is even a candidate.
///
/// Driven down to one hand each so neither is thinnable and the walk is past step 5 entirely.
#[test]
fn the_benchs_last_hand_goes_before_a_source_is_emptied_whatever_the_marks_say() {
    let (mut app, band, _source) = world_with_a_band_at_the_bench(SourcePriority::High);
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("the fixture band keeps its bench");
        bench.workers = 1;
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        allocation.assignments[0].workers = 1;
        allocation.assignments[0].priority = SourcePriority::Low;
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band keeps its cohort");
        cohort.working = scalar_from_f32(2.0);
    }
    lose_people(&mut app, band, 1);
    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        bench_crew(&app, band),
        0,
        "the bench stalls — its last hand goes at its own step, above the step that ends a row"
    );
    assert_eq!(
        gatherers(&app, band),
        1,
        "…and the Low-marked source is untouched, because the step decided this and not the mark"
    );
}

/// **THE LAST HAND STALLS THE JOB; IT DOES NOT CLEAR IT.**
///
/// `BandBench::clear_job` is `*self = default()`, which **forfeits the drawn pile** — the materials
/// are dropped rather than returned to the store. The shed must never call it, so everything the
/// player had is still there afterwards and re-staffing resumes rather than restarts.
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
        bench.progress = scalar_from_f32(PROGRESS);
        bench.items_completed = ITEMS_DONE;
        bench.drawn = Some(DrawnInputs {
            reading: Some(0.5),
            grade: Some("good".to_string()),
            withdrawn: vec![DrawnMaterial {
                material: "hide".to_string(),
                amount: scalar_from_f32(2.0),
            }],
        });
    }
    // One row of one hand and a bench of one: the band can field one of them.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        allocation.assignments[0].workers = 1;
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band keeps its cohort");
        cohort.working = scalar_from_f32(1.0);
    }
    app.world.run_system_once(advance_labor_allocation);

    let bench = app
        .world
        .get::<BandBench>(band)
        .expect("the fixture band keeps its bench");
    assert_eq!(bench.workers, 0, "the bench stalled");
    assert_eq!(
        bench.recipe_id.as_deref(),
        Some(RECIPE),
        "the job is still the job the player chose"
    );
    assert_eq!(
        bench.progress,
        scalar_from_f32(PROGRESS),
        "its progress stands, so re-crewing RESUMES rather than restarting"
    );
    assert_eq!(
        bench.items_completed, ITEMS_DONE,
        "and the finished count is intact"
    );
    let withdrawn = bench
        .drawn
        .as_ref()
        .expect("⛔ the drawn pile is FORFEITED by clear_job — the shed must never call it");
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
        bench.progress = scalar_from_f32(BANKED);
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
        bench.progress,
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
        .find(|bench| bench.recipe_id.as_deref() == Some(RECIPE))
        .expect("the restored world carries the fixture band's bench");
    assert_eq!(
        restored.priority,
        SourcePriority::Low,
        "the rank came back with the bench — a checkpoint that forgot it would re-rank the band"
    );
}
