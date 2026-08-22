//! **WHAT A SOURCE'S ROW AND A BAND'S STANDING ROLES LOOK LIKE ON THE WIRE**
//! (`docs/plan_standing_upkeep.md` §2.2 and §2.5).
//!
//! The player allocates hands per activity: a **take** crew on a source, and **three band-level
//! pools** — `agriculture`, `husbandry`, `builders`. Every one of them arrives as an ordinary row of
//! `laborAssignments`, distinguished by `kind` and with its hands in `workers`, exactly like scout
//! and warrior. That is what this file pins, because a role published under a kind no client
//! recognises is invisible in precisely the way the retired `maintainWorkers` was — and
//! `improvementWorkers` is retired the same way now that the **build** left the tile too.
//!
//! **What survives per source is `improvement`**, which the sim **derives** at capture from that
//! band's build queue: the resolved job token, or `""` when nothing is queued there. So a client
//! still reads *what is being raised here* off the row and still does no arithmetic to get it —
//! and it can no longer read a per-source builder count, because the sim has stopped having one.
//!
//! **Asserted on the ENCODED envelope, never on the in-process `LaborAssignmentState`**, because a
//! field can be right in the capture and absent from the buffer — the schema/codec/reader path is
//! what a client actually sees. And every count is staffed at a **different number**: a fixture that
//! used one crew everywhere would pass with every slot wired to the same source.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::TakeSelection;
use core_sim::{
    build_test_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    BuildJob, BuildQueueEntry, BuildSource, FactionId, GenerationId, Improvement, LaborAllocation,
    LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand,
    SnapshotHistory, StartingUnit, TileRegistry, UpkeepFundMode, DEFAULT_ESCAPEMENT_FLOOR,
};

/// **Three counts, three DIFFERENT numbers, none of them equal to another** — the whole point of the
/// fixture. Wiring every wire slot to `workers` would pass a fixture that staffed the same count on
/// each, so the counts are chosen pairwise distinct and none is `0` (which would also be the value
/// of an unwritten slot).
const TAKE_CREW: u32 = 7;
const BUILD_CREW: u32 = 3;
const KEEP_CREW: u32 = 2;

/// **What the fixture band has queued on its one source** — the verb whose token the row must
/// publish. `Sow` rather than `Cultivate` so the assertion cannot pass on a capture that hard-codes
/// the first plant rung.
const DECLARED: Improvement = Improvement::Sow;

/// The tile the fixture band works and lives on — **a tile the world actually carries a patch on**,
/// resolved at run time rather than named as a literal, because the `improvement` token is derived
/// against the ground and a patch that is not in the registry can only answer `""`.
fn source_tile(app: &App) -> UVec2 {
    app.world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        // Deterministic: the map iterates in hash order, so the fixture pins the lowest coord.
        .min_by_key(|tile| (tile.y, tile.x))
        .expect("worldgen seeded at least one forage patch")
}

/// **The crop this band asked for** — the field that was write-only from the client's side. A
/// `flora_config.json` key rather than a display name, because that is what the assignment stores
/// and what `resolve_committed_species` matches on. Its VALUE does not matter here (no crew has
/// worked the patch, so nothing validates it); what matters is that the string the player stated
/// comes back out of the buffer as itself.
const CROP: &str = "wild_emmer";

/// A headless world with one resident band that staffs a Forage source's take crew and the band's
/// own **agriculture** and **builders** roles. The band is sized to afford all three rows: they draw
/// on one pool, so a band short of `TAKE + BUILD + KEEP` would have `LaborAllocation::normalize` trim
/// the tail and the fixture would publish numbers it never staffed.
fn world_with_a_keeping_band() -> (App, UVec2) {
    let mut app = build_test_app();
    // One `update()` runs the whole Startup worldgen chain, which is what seeds the `TileRegistry`
    // the band is homed on and the patches it works.
    let source = {
        app.update();
        source_tile(&app)
    };
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
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
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: source,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: Some(CROP.to_string()),
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: TAKE_CREW,
                    kit: None,
                },
                // The keeping — a row of its own, on the band rather than the tile.
                LaborAssignment {
                    target: LaborTarget::Agriculture,
                    workers: KEEP_CREW,
                    kit: None,
                },
                // …and so is the building, since §2.5.
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: BUILD_CREW,
                    kit: Some(bare_builders()),
                },
            ],
            // The declaration the source row's `improvement` token is derived from.
            build_queue: vec![BuildQueueEntry {
                source: BuildSource::Patch(source),
                declared: BuildJob::Rung(DECLARED),
            }],
            upkeep_fund_mode: UpkeepFundMode::Priority,
            ..Default::default()
        },
    ));
    recapture_snapshot_in_place(&mut app.world);
    (app, source)
}

/// The fixture band's **source** row, read back out of the encoded buffer — the artifact a client
/// parses, rather than the state struct the capture built.
fn published_source_row(app: &App) -> (u32, String, String) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let bytes = encoded_snapshot(app);
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
        .expect("the fixture band's source assignment is on the wire");
    (
        row.workers(),
        row.improvement().unwrap_or_default().to_string(),
        row.species().unwrap_or_default().to_string(),
    )
}

/// The band's **keeping role** row, by kind — `None` when no row of that kind was published.
fn published_role(app: &App, kind: &str) -> Option<u32> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let bytes = encoded_snapshot(app);
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let populations = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list");
    populations
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == kind)
        .map(|assignment| assignment.workers())
}

fn encoded_snapshot(app: &App) -> Vec<u8> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
}

/// **A SOURCE ROW STATES ITS TAKE CREW AND WHAT IS BEING RAISED ON IT** — two facts, two fields, and
/// neither derivable from the other.
///
/// The row no longer states a **build crew**, because there is no per-source one to state
/// (`docs/plan_standing_upkeep.md` §2.5); the hands are the `builders` row asserted below, and this
/// pins that the source still says what those hands are working on.
#[test]
fn a_source_row_states_its_take_crew_and_the_job_queued_on_it() {
    let (app, _source) = world_with_a_keeping_band();
    let (take, job, _) = published_source_row(&app);
    assert_eq!(take, TAKE_CREW, "the take crew arrives as itself");
    assert_eq!(
        job,
        DECLARED.as_str(),
        "…beside the RESOLVED job the band has queued here — the client does no arithmetic for it"
    );
}

/// **EVERY STANDING POOL ARRIVES AS A ROW, UNDER ITS OWN KIND.** It is the whole of what replaced
/// `maintainWorkers` and now `improvementWorkers`, and a role published under a kind no client
/// recognises is invisible in exactly the way those retired fields were.
///
/// **The three counts are pairwise distinct**, so no assertion here can pass on a capture that wired
/// every row to the same number.
#[test]
fn the_bands_standing_pools_reach_the_client_as_their_own_rows() {
    let (app, _source) = world_with_a_keeping_band();
    assert_eq!(
        published_role(&app, "agriculture"),
        Some(KEEP_CREW),
        "the agriculture role is an ordinary assignment row with its hands in `workers`"
    );
    assert_eq!(
        published_role(&app, "builders"),
        Some(BUILD_CREW),
        "and so is the builders pool — the band's only build staffing since §2.5"
    );
    assert_eq!(
        published_role(&app, "husbandry"),
        None,
        "and the roles are separate rows — a band keeping no herds publishes none"
    );
}

/// **THE FUND MODE IS ON THE COHORT, AS THE TOKEN THE COMMAND TAKES.** Without it a client cannot
/// tell a band on `priority` from a band nobody has set, and would have to guess at the default.
#[test]
fn the_bands_upkeep_fund_mode_reaches_the_client() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let (app, _source) = world_with_a_keeping_band();
    let bytes = encoded_snapshot(&app);
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let modes: Vec<String> = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .map(|cohort| cohort.upkeepFundMode().unwrap_or_default().to_string())
        .collect();
    assert!(
        modes
            .iter()
            .any(|mode| mode == UpkeepFundMode::Priority.as_str()),
        "the band's stated mode must arrive as the command's own token: {modes:?}"
    );
    assert!(
        modes.iter().all(|mode| !mode.is_empty()),
        "and every band states one — an empty string is a frame the sim did not write: {modes:?}"
    );
}

/// **THE CROP THE PLAYER STATED COMES BACK TOO** — the other field of the assignment that the client
/// could set and never read.
///
/// It is **not** the patch's `committedSpecies`, and the difference is the whole reason it ships: the
/// patch's is what the GROUND is committed to and is set only once a crew has worked it, while this
/// is the selection the player made, which exists from the moment they make it. A compose sheet
/// reopened on a patch nobody has worked yet has no other way to show the crop it is about to plant.
#[test]
fn the_crop_a_crew_asked_for_reaches_the_client() {
    let (app, _source) = world_with_a_keeping_band();
    let (_, _, species) = published_source_row(&app);
    assert_eq!(
        species, CROP,
        "the player's stated crop must survive the wire as itself"
    );
}

/// **A source with nothing QUEUED publishes an honest empty token**, not an absent field a reader
/// has to guess about — and the take crew beside it is untouched by that emptiness.
///
/// It is the common reading: most sources have nothing being raised on them.
#[test]
fn a_bare_gathering_row_publishes_an_empty_job_token() {
    let (mut app, source) = world_with_a_keeping_band();
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
        assert!(
            allocation.unqueue_build(&BuildSource::Patch(source)),
            "fixture: the band had a declaration to withdraw"
        );
    }
    recapture_snapshot_in_place(&mut app.world);

    let (take, job, _) = published_source_row(&app);
    assert_eq!(
        (take, job.as_str()),
        (TAKE_CREW, ""),
        "a pure gather states its take crew and an honest empty job"
    );
}

/// **THE EMPTY KIT, NAMED ON A FIXTURE'S `builders` ROW** — an isolation, not a default.
///
/// An absent kit means *derive per entry*, and the roster's answer (`tillage` for a patch,
/// `hurdling` for a herd) adds `+0.5` work per covered worker per turn. A start-stocked band holds a
/// unit per worker and a half, so at the crews these fixtures staff every builder is geared and the
/// pool delivers half again what it asserts, moving every pacing claim below. Naming `none` holds
/// the gear axis at its identity so these arms measure the **crew**, exactly as
/// `FaunaConfig::without_retreat` holds the retreat at its identity across the hunt suites. The
/// geared default is pinned in `core_sim/tests/build_turns_closed_form.rs`.
fn bare_builders() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}
