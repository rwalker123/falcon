//! **THE ROW THE PLAYER JUST TOUCHED IS NOT THE FIRST ONE SHED** — the decided shedding order
//! (`docs/plan_standing_upkeep.md` §2.9) seen from where the player sees it.
//!
//! The reported case: a Field's tenders were raised `2 → 3`, an elder died that same turn, and the
//! worker came straight back off the row that had just been chosen. Two individually reasonable
//! halves composed into it — `LaborAllocation::set_assignment` re-pushes an edited row to the **end**
//! of `assignments`, and `normalize` trimmed from the **end** — so *the shedding order was the edit
//! order*, and the list position a player controls was silently overwritten by the act of editing
//! the row.
//!
//! **Asserted on the ENCODED envelope**, not on the in-process allocation, because the claim is about
//! what the player watched move: the crew count on the band's row. A `laborAssignments` row that is
//! right in the capture and wrong in the buffer is the same defect to them.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, FactionId, ForageRegistry, GenerationId, LaborAllocation,
    LaborConfigHandle, LaborTarget, LocalStore, MaterialPayoff, MoraleCause, PopulationCohort,
    ResidentBand, SnapshotHistory, SourceYield, StartingUnit, TakeSelection, TileRegistry,
    DEFAULT_ESCAPEMENT_FLOOR,
};

/// The crew on the patch the player has **not** touched — larger than the raised one, so a rule that
/// shed the biggest crew, or the first row, would also pick it and the test would pass for the wrong
/// reason. It is chosen against on **yield per worker** alone.
const SETTLED_CREW: u32 = 4;

/// What the player raises the second patch's crew to — the number that must still be on the wire
/// after the band loses a worker. The reported case's `2 → 3`.
const RAISED_CREW: u32 = 3;

/// The steady per-turn yield the two rows are seeded at, mirroring the `assign_labor` command's own
/// assign-time forecast seed (`LaborAllocation::set_source_yield`). The raised patch is the **richer
/// ground per head** — `18 ÷ 3 = 6.0` against `8 ÷ 4 = 2.0` — which is what makes the settled row the
/// honest choice and the raised row the one the old order would have taken anyway.
const SETTLED_REALIZED: f32 = 8.0;
const RAISED_REALIZED: f32 = 18.0;

/// A forage target on `tile` at the default floor — the shape both rows carry.
fn forage_on(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: DEFAULT_ESCAPEMENT_FLOOR,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// A telemetry row whose only live field is the headline the shedding order reads.
fn seeded(realized: f32) -> SourceYield {
    SourceYield {
        realized,
        ..SourceYield::ZERO
    }
}

/// **Two patches the same band can work at once** — the home patch and the nearest other one inside
/// `labor_config.band_work_range`. Resolved from the live registry rather than named as literals,
/// because a row that re-resolves out of range **lapses**, and a fixture that lost a row that way
/// would be measuring the leash instead of the shedding order.
fn two_worked_patches(app: &App) -> (UVec2, UVec2) {
    let range = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_work_range;
    let config = app.world.resource::<core_sim::SimulationConfig>();
    let width = config.grid_size.x;
    let wrap = config.map_topology.wrap_horizontal;
    let mut patches: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .collect();
    // The registry iterates in hash order; sort so the pair is the same on every run.
    patches.sort_by_key(|tile| (tile.y, tile.x));
    for home in &patches {
        if let Some(other) = patches.iter().find(|tile| {
            *tile != home
                && core_sim::grid_utils::hex_distance_wrapped(*home, **tile, width, wrap) <= range
        }) {
            return (*home, *other);
        }
    }
    panic!("worldgen seeded no two forage patches within one band's work range");
}

/// A band standing on `home`, working both patches, with the second row **raised last** — the exact
/// composition the defect needed. Its `working` is one short of what it holds, which is the elder's
/// death: the band is fully committed and the pool has just shrunk under it.
fn band_that_just_lost_a_worker(app: &mut App, home: UVec2, other: UVec2) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(home.x, home.y)
        .expect("the home patch resolves to a tile");
    let committed = SETTLED_CREW + RAISED_CREW;
    let mut allocation = LaborAllocation::default();
    // Staffed through `set_assignment` + the command's own forecast seed, in the order a player
    // would have issued them — the settled patch first, then the raise.
    allocation.set_assignment(forage_on(home), SETTLED_CREW, committed, None);
    allocation.set_source_yield(&forage_on(home), seeded(SETTLED_REALIZED));
    allocation.set_assignment(forage_on(other), RAISED_CREW, committed, None);
    allocation.set_source_yield(&forage_on(other), seeded(RAISED_REALIZED));
    assert_eq!(
        allocation.assignments.last().map(|row| row.workers),
        Some(RAISED_CREW),
        "fixture: the edited row must sit at the TAIL, which is the composition the defect needed"
    );

    spawn_committed_band(app, tile, committed - 1, allocation)
}

/// **A resident band standing on `tile` with `working` hands and `allocation` already committed** —
/// the spawn every fixture in this file shares. `working` is deliberately the caller's to state,
/// because the whole subject of this file is a band whose pool has just shrunk under what it holds.
fn spawn_committed_band(
    app: &mut App,
    tile: Entity,
    working: u32,
    allocation: LaborAllocation,
) -> Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(working as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
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
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
            allocation,
        ))
        .id()
}

/// The crew this band has on the row working `tile`, read back out of the encoded buffer.
fn published_crew(app: &App, tile: UVec2) -> Option<u32> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

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
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|row| {
            row.kind().unwrap_or_default() == "forage"
                && row.targetX() == tile.x
                && row.targetY() == tile.y
        })
        .map(|row| row.workers())
}

/// # ⛔ THE RAISE STANDS; THE OTHER ROW GIVES
///
/// This is the test that would have caught the original. Both halves are asserted, because either
/// one alone passes for a band that simply shed nothing: the raised crew is untouched **and** the
/// settled one is down by exactly the hand the band no longer has.
#[test]
fn the_row_the_player_just_raised_survives_the_turn_the_band_shrinks() {
    let mut app = build_test_app();
    // One `update()` runs the whole Startup worldgen chain, which seeds the patches and the registry.
    app.update();
    let (home, other) = two_worked_patches(&app);
    band_that_just_lost_a_worker(&mut app, home, other);

    // **The labor pass alone, not a whole turn.** A turn would also feed the band, and this fixture
    // opens with an empty larder — the starvation deaths would shrink the pool again and the test
    // would be measuring demographics rather than the shedding order.
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    assert_eq!(
        published_crew(&app, other),
        Some(RAISED_CREW),
        "the crew the player had just chosen is the number still on the wire"
    );
    assert_eq!(
        published_crew(&app, home),
        Some(SETTLED_CREW - 1),
        "and the poorer ground per head is what gave the hand up — the row's list position had \
         nothing to do with it"
    );
}

// ---------------------------------------------------------------------------
// **A DEAD ROW SHEDS BEFORE A PRODUCTIVE ONE** — the first level of the comparison
// (`LaborAllocation::pays_any_account`), asserted through the same published rows above.
// ---------------------------------------------------------------------------

/// The crew each row of the two-row fixtures below carries. **Two**, because step 5 of the order
/// (`ShedStep::ThinLeastProductive`) only names a row with at least that many hands — so the loser
/// is *thinned* and both rows survive onto the wire, which is where these claims are asserted.
const RANKED_CREW: u32 = 2;

/// The food a **food row** in these fixtures pays per turn. Any positive number would do; what the
/// assertions turn on is that it is positive where the row beside it reads zero provisions.
const FOOD_ROW_REALIZED: f32 = 4.0;

/// The material a **cash-crop row** pays per turn, and the id it pays it in — the shape of a tobacco
/// Field: zero in both scalar accounts, paid entirely by its materials rows (`flora_config.json`).
const CASH_ROW_MATERIAL: &str = "tobacco";
const CASH_ROW_AMOUNT: f32 = 0.75;

/// A telemetry row paying **nothing in any account** — the dead row. Spelled as `SourceYield::ZERO`
/// so it cannot drift from the row every assignment starts a turn's resolution with.
fn pays_nothing() -> SourceYield {
    SourceYield::ZERO
}

/// A telemetry row paying **materials and no food** — the cash-crop Field. `realized` stays `0.0`,
/// which is exactly the tie with a dead row that `yield_per_worker` alone could not break.
fn pays_only_materials() -> SourceYield {
    SourceYield {
        materials: vec![MaterialPayoff {
            material: CASH_ROW_MATERIAL.to_string(),
            amount: CASH_ROW_AMOUNT,
        }],
        ..SourceYield::ZERO
    }
}

/// A band standing on `home` and working both patches at [`RANKED_CREW`] apiece, one hand short of
/// what it holds — the fully-committed band that has just lost someone, which is the only state the
/// shedding order ever runs in. Each row carries the telemetry it is handed, in list order.
fn band_ranked_by(app: &mut App, home: UVec2, other: UVec2, yields: [SourceYield; 2]) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(home.x, home.y)
        .expect("the home patch resolves to a tile");
    let committed = RANKED_CREW * 2;
    let mut allocation = LaborAllocation::default();
    let [home_yield, other_yield] = yields;
    allocation.set_assignment(forage_on(home), RANKED_CREW, committed, None);
    allocation.set_source_yield(&forage_on(home), home_yield);
    allocation.set_assignment(forage_on(other), RANKED_CREW, committed, None);
    allocation.set_source_yield(&forage_on(other), other_yield);
    spawn_committed_band(app, tile, committed - 1, allocation)
}

/// Run the labor pass and republish, so the assertions below read the crew the player would see.
fn resolve_and_publish(app: &mut App) {
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// # ⛔ THE ROW PAYING NOTHING IS THE ONE THAT GIVES
///
/// A hay Field and the five cash crops pay **zero food by design** (`flora_config.json`), so a
/// productive tobacco Field and a genuinely dead row both read `0` provisions per worker and tied
/// under the old single-level order — which meant list position decided it. Ray's call is that the
/// dead one goes first: a row paying into *any* account outranks a row paying into none.
///
/// **The dead row is listed FIRST here**, so a comparison that still fell through to the earliest-row
/// tie-break would pick it for the wrong reason. The sibling test below lists it second.
#[test]
fn a_row_paying_nothing_is_shed_before_one_paying_only_materials() {
    let mut app = build_test_app();
    app.update();
    let (dead, cash) = two_worked_patches(&app);
    band_ranked_by(
        &mut app,
        dead,
        cash,
        [pays_nothing(), pays_only_materials()],
    );

    resolve_and_publish(&mut app);

    assert_eq!(
        published_crew(&app, cash),
        Some(RANKED_CREW),
        "the row paying a material keeps its crew — it is paying into an account"
    );
    assert_eq!(
        published_crew(&app, dead),
        Some(RANKED_CREW - 1),
        "and the row paying into no account at all is what gave the hand up"
    );
}

/// The same claim with the two rows **swapped in list order**, because the earliest-row tie-break is
/// what the fix has to beat: a comparison that never separated the two would answer *"the first
/// row"* both times, and one of the two orderings would pass by accident.
#[test]
fn the_dead_row_still_gives_when_it_is_listed_second() {
    let mut app = build_test_app();
    app.update();
    let (cash, dead) = two_worked_patches(&app);
    band_ranked_by(
        &mut app,
        cash,
        dead,
        [pays_only_materials(), pays_nothing()],
    );

    resolve_and_publish(&mut app);

    assert_eq!(
        published_crew(&app, cash),
        Some(RANKED_CREW),
        "list position does not decide this — the material row keeps its crew either way"
    );
    assert_eq!(
        published_crew(&app, dead),
        Some(RANKED_CREW - 1),
        "the dead row gives the hand from whichever slot it happens to occupy"
    );
}

/// # ⛔ A FOOD ROW STILL OUTRANKS A NON-FOOD ROW — the behaviour the tie fix must not invert
///
/// The presence test is the **first** level and food per worker is the second, which is what keeps
/// Ray's standing intent: a band short of hands keeps its people on food and drops the tobacco. Both
/// rows here pay into an account, so the first level cannot separate them and the order is decided
/// exactly as it always was.
///
/// It is asserted rather than assumed because the failure is silent and in the opposite direction: a
/// first level that ranked *material presence* above food would read as working right up until the
/// band starved.
#[test]
fn a_food_row_still_outranks_a_row_paying_only_materials() {
    let mut app = build_test_app();
    app.update();
    let (food, cash) = two_worked_patches(&app);
    band_ranked_by(
        &mut app,
        food,
        cash,
        [seeded(FOOD_ROW_REALIZED), pays_only_materials()],
    );

    resolve_and_publish(&mut app);

    assert_eq!(
        published_crew(&app, food),
        Some(RANKED_CREW),
        "the food row keeps its crew — the band short of hands keeps its people fed"
    );
    assert_eq!(
        published_crew(&app, cash),
        Some(RANKED_CREW - 1),
        "and the cash crop is what gets dropped, exactly as before the presence test existed"
    );
}

/// # ⛔ TWO DEAD ROWS FALL BACK TO THE EXISTING TIE-BREAK
///
/// The presence test answers `false` for both and their per-worker yields are equal, so the choice
/// is `min_by`'s first minimum — the **earliest** row — which is the stable answer the order has
/// always given. Asserted so the new level cannot quietly make an all-dead band's shedding depend on
/// how the vector happens to be ordered, which is the whole defect
/// `.claude/rules/core_sim/yield-forecast.md` records under *"it used to be the EDIT order"*.
#[test]
fn two_rows_paying_nothing_still_fall_back_to_the_earliest_row() {
    let mut app = build_test_app();
    app.update();
    let (first, second) = two_worked_patches(&app);
    band_ranked_by(&mut app, first, second, [pays_nothing(), pays_nothing()]);

    resolve_and_publish(&mut app);

    assert_eq!(
        published_crew(&app, first),
        Some(RANKED_CREW - 1),
        "with nothing to separate them the earliest row gives, as it always has"
    );
    assert_eq!(
        published_crew(&app, second),
        Some(RANKED_CREW),
        "and the later row is untouched"
    );
}
