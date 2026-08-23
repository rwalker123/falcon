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
    LaborConfigHandle, LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand,
    SnapshotHistory, SourceYield, StartingUnit, TakeSelection, TileRegistry,
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

    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                // One short of what the band holds: the turn's death, already applied.
                working: scalar_from_f32((committed - 1) as f32),
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
