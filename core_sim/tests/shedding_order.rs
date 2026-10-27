//! **A STAFFED ROW IS NEVER SHED** (#871, `docs/plan_labor_at_scale.md` "Decided") - the decided
//! shedding order (`docs/plan_standing_upkeep.md` §2.9) seen from where the player sees it.
//!
//! The reported case this file was written for: a Field's tenders were raised `2 -> 3`, an elder
//! died that same turn, and the worker came straight back off the row that had just been chosen. The
//! shedding order was the edit order. **The sim now decides every forage, hunt, extraction, tile and
//! Roadwork crew from the player's floor**, so a smaller pool is no longer a reason to shed such a
//! row: the passes (`core_sim::staffing`) simply staff it again from what is left, and the order of
//! who gives is the order of who is served - the player's rank, then the row's place in the list.
//!
//! # Retired with that model
//!
//! The yield-ranked shed of source rows (a row paying nothing before one paying only materials, the
//! earliest row on a tie, the keeping line a thinned crew may not cross, the tool-gated keeper) has
//! nothing left to decide for a staffed row, and the `ShedStep` walk now runs over the crews the
//! player sets by hand only (scouts, warriors, builders, the bench and a need row's muster). Those six
//! tests are deleted rather than ported: each asserted which source row the walk gave a hand from.
//!
//! **Asserted on the ENCODED envelope**, because the claim is about what the player watched move: the
//! crew count on the band's row. A `laborAssignments` row that is right in the capture and wrong in
//! the buffer is the same defect to them.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, CommandEventLog, FactionId, ForageRegistry, GenerationId,
    LaborAllocation, LaborConfigHandle, LaborTarget, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, SnapshotHistory, StartingUnit, TakeSelection, TileRegistry,
    DEFAULT_ESCAPEMENT_FLOOR,
};

/// The most hands the player lets each patch take - more than the band has, so the pool is what binds.
const MAX_PER_ROW: u32 = 4;

/// The hands the band has after the elder's death.
const WORKING_AFTER_THE_LOSS: u32 = 3;

/// A forage target on `tile` at the default floor.
fn forage_on(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: DEFAULT_ESCAPEMENT_FLOOR,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// **Two patches the same band can work at once** - the home patch and the nearest other one inside
/// `labor_config.band_work_range`. Resolved from the live registry rather than named as literals,
/// because a row that re-resolves out of range posts a party, and a fixture that did that would be
/// measuring the walk and not the pool.
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

/// **A resident band standing on `tile` with `working` hands and `allocation` already committed.**
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
                faction: FactionId(0),
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

/// # A SMALLER POOL RESTAFFS THE ROWS; IT SHEDS NOTHING AND SAYS NOTHING
///
/// Two patches, each allowed more hands than the band has. The crews the passes put on them never
/// pass the pool, both rows stand, and the shed's `trimmed` / `lapsed` lines - which exist for the
/// crews a player sets by hand - are never written for a row the sim staffs.
#[test]
fn a_smaller_pool_restaffs_the_rows_and_sheds_nothing() {
    let mut app = build_test_app();
    // One `update()` runs the whole Startup worldgen chain, which seeds the patches and the registry.
    app.update();
    let (home, other) = two_worked_patches(&app);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(home.x, home.y)
        .expect("the home patch resolves to a tile");
    let mut allocation = LaborAllocation::default();
    allocation.set_staffed_row(forage_on(home), Some(MAX_PER_ROW), None);
    allocation.set_staffed_row(forage_on(other), Some(MAX_PER_ROW), None);
    let band = spawn_committed_band(&mut app, tile, WORKING_AFTER_THE_LOSS, allocation);

    // **The labor pass alone, not a whole turn.** A turn would also feed the band, and this fixture
    // opens with an empty larder - the starvation deaths would shrink the pool again and the test
    // would be measuring demographics.
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);

    let allocation = app
        .world
        .get::<LaborAllocation>(band)
        .expect("band allocation");
    assert_eq!(
        allocation.assignments.len(),
        2,
        "both rows stand: a staffed row is never shed"
    );
    assert!(
        allocation.assigned_total() <= WORKING_AFTER_THE_LOSS,
        "the crews never pass the pool: {}",
        allocation.assigned_total()
    );
    for tile in [home, other] {
        let on_the_wire = published_crew(&app, tile).expect("both rows are published");
        assert!(
            on_the_wire <= MAX_PER_ROW,
            "the Max is the most the sim staffs: {on_the_wire}"
        );
    }
    assert!(
        published_crew(&app, home).unwrap_or(0) + published_crew(&app, other).unwrap_or(0) > 0,
        "liveness: the pool is put to work, or every bound above is two zeros"
    );
    assert!(
        !app.world
            .resource::<CommandEventLog>()
            .iter()
            .any(|entry| entry
                .detail
                .as_deref()
                .is_some_and(|detail| { detail.contains("reason=too_few_workers") })),
        "nothing was shed, so nothing is announced"
    );
}
