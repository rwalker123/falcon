//! **THE THREE CREWS A SOURCE CARRIES, ON THE WIRE** (`docs/plan_standing_upkeep.md` §2.2).
//!
//! The player allocates hands per activity — a take crew, a build crew and a keeping crew, three
//! independent numbers on one source drawing on one finite band. `LaborAssignment.workers` published
//! only the first, so the other two were **write-only from the client's side**: it could send
//! `cultivate … <workers>` and `maintain … <workers>` and never read back what a band already had.
//!
//! Two things broke on that, and both are the reason these fields exist:
//!
//! - **A trap with no way out.** A compose sheet clamps its steppers to the band's *idle* workers,
//!   because the sim refuses an over-staffed command. A fully-allocated band therefore has `0` idle
//!   and offered a keeper maximum of `0` — the player could not **re-state** a crew they already
//!   had, only take it to zero. The honest clamp is `idle + this source's own crew for this
//!   activity`, which needs this source's own crew.
//! - **A readout that said the opposite of the truth.** A sheet reopened on a herd with two keepers
//!   read `0`, i.e. *"nothing is holding this"*, on the very row built to warn about decay.
//!
//! **Asserted on the ENCODED envelope, never on the in-process `LaborAssignmentState`**, because a
//! field can be right in the capture and absent from the buffer — the schema/codec/reader path is
//! what a client actually sees. And the three crews are staffed at **three different numbers**: a
//! fixture that used one crew everywhere would pass with all three slots wired to the same source.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    FactionId, GenerationId, Improvement, LaborAllocation, LaborAssignment, LaborTarget,
    LocalStore, MoraleCause, PopulationCohort, ResidentBand, SnapshotHistory, StartingUnit,
    TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};

/// **Three crews, three DIFFERENT numbers, none of them equal to another** — the whole point of the
/// fixture. Wiring all three wire slots to `workers` would pass a fixture that staffed the same
/// count on each, so the counts are chosen pairwise distinct and none is `0` (which would also be
/// the value of an unwritten slot).
const TAKE_CREW: u32 = 7;
const BUILD_CREW: u32 = 3;
const KEEP_CREW: u32 = 2;

/// The tile the fixture band works and lives on.
const SOURCE: UVec2 = UVec2::new(1, 1);

/// **The crop this band asked for** — the fourth field that was write-only from the client's side.
/// A `flora_config.json` key rather than a display name, because that is what the assignment stores
/// and what `resolve_committed_species` matches on. Its VALUE does not matter here (no crew has
/// worked the patch, so nothing validates it); what matters is that the string the player stated
/// comes back out of the buffer as itself.
const CROP: &str = "wild_emmer";

/// A headless world with one resident band whose single Forage assignment staffs all three
/// activities. The band is sized to afford the three together: they draw on one pool, so a band
/// short of `TAKE + BUILD + KEEP` would have `LaborAllocation::normalize` trim the tail and the
/// fixture would publish numbers it never staffed.
fn world_with_a_three_crew_band() -> App {
    let mut app = build_headless_app();
    // One `update()` runs the whole Startup worldgen chain, which is what seeds the `TileRegistry`
    // the band is homed on.
    app.update();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(SOURCE.x, SOURCE.y)
        .expect("the fixture tile resolves");
    app.world.spawn((
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 30,
            children: scalar_zero(),
            working: scalar_from_f32((TAKE_CREW + BUILD_CREW + KEEP_CREW) as f32),
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
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: Some(CROP.to_string()),
                },
                workers: TAKE_CREW,
                improvement: Some(Improvement::Cultivate),
                kit: None,
                improvement_workers: BUILD_CREW,
                maintain_workers: KEEP_CREW,
            }],
            ..Default::default()
        },
    ));
    recapture_snapshot_in_place(&mut app.world);
    app
}

/// The fixture band's one assignment row, **read back out of the encoded buffer** — the artifact a
/// client parses, rather than the state struct the capture built.
fn published_crews(app: &App) -> (u32, u32, u32) {
    let (take, build, keep, _) = published_row(app);
    (take, build, keep)
}

/// The whole set the client reads back off one assignment row: the three crews and the stated crop.
fn published_row(app: &App) -> (u32, u32, u32, String) {
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
    let populations = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list");
    let row = populations
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.workers() == TAKE_CREW)
        .expect("the fixture band's assignment is on the wire");
    (
        row.workers(),
        row.improvementWorkers(),
        row.maintainWorkers(),
        row.species().unwrap_or_default().to_string(),
    )
}

/// **All three crews survive the wire, and survive it DISTINCTLY.**
#[test]
fn a_sources_take_build_and_keeping_crews_all_reach_the_client() {
    let app = world_with_a_three_crew_band();
    assert_eq!(
        published_crews(&app),
        (TAKE_CREW, BUILD_CREW, KEEP_CREW),
        "each activity's own crew must arrive as itself — the client cannot derive any of the \
         three from the others"
    );
}

/// **THE CROP THE PLAYER STATED COMES BACK TOO** — the fourth field of the assignment that the
/// client could set and never read.
///
/// It is **not** the patch's `committedSpecies`, and the difference is the whole reason it ships: the
/// patch's is what the GROUND is committed to and is set only once a crew has worked it, while this
/// is the selection the player made, which exists from the moment they make it. A compose sheet
/// reopened on a patch nobody has worked yet has no other way to show the crop it is about to plant.
#[test]
fn the_crop_a_crew_asked_for_reaches_the_client() {
    let app = world_with_a_three_crew_band();
    let (_, _, _, species) = published_row(&app);
    assert_eq!(
        species, CROP,
        "the player's stated crop must survive the wire as itself"
    );
}

/// **A source with nothing being built and nobody keeping it publishes an honest pair of zeroes**,
/// not an absent field a reader has to guess about. `0` is the common reading on both — most sources
/// carry no verb and no keepers — so it has to be a value rather than a gap, and the take crew
/// beside it has to be untouched by their absence.
#[test]
fn a_bare_gathering_row_publishes_zero_for_the_two_it_does_not_staff() {
    let mut app = world_with_a_three_crew_band();
    {
        let mut allocation = app
            .world
            .query::<&mut LaborAllocation>()
            .iter_mut(&mut app.world)
            .find(|allocation| {
                allocation
                    .assignments
                    .first()
                    .is_some_and(|a| a.workers == TAKE_CREW)
            })
            .expect("the fixture band exists");
        let assignment = &mut allocation.assignments[0];
        assignment.improvement = None;
        assignment.improvement_workers = 0;
        assignment.maintain_workers = 0;
    }
    recapture_snapshot_in_place(&mut app.world);

    assert_eq!(
        published_crews(&app),
        (TAKE_CREW, 0, 0),
        "a pure gather states its take crew and an honest zero on the other two"
    );
}
