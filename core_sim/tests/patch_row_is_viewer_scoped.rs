//! ⛔ **A PATCH ROW STATES THE TILE, THE IMPROVEMENT THE VIEWER MAY READ, AND NOTHING ELSE.**
//!
//! `frame_is_viewer_scoped.rs` pins the *membership* rule section by section — whose rows are in the
//! frame. This file pins the one section where the answer is not "yours or absent": a
//! `foragePatch` row is a fact about a **tile**, so it always rides, and only the improvement
//! standing on it is scoped. Two rules divide that row, and they are different rules:
//!
//! 1. **The improvement follows the GROUND.** A field is built into the ground and does not wander
//!    off, so a rival's is legible exactly where the viewer has **explored** — `route_states`'
//!    precedent.
//! 2. **The builder's state follows the BUILDER.** Who is raising it, with what kit, how many turns
//!    from done and where it sits in their queue is internal state, the same category as the larder
//!    and bench a foreign band's row already withholds. Viewer's own bands, or nothing.
//!
//! **The defects these pin were both fail-open, and both are worth naming.** The improvement half
//! gated five fields by name — `owner`, the two flags, the two meters — while `carryingCapacity`,
//! the two rung yields, `provisionsPerBiomass` and the basket were derived from the same
//! improvement and published bare, so `carryingCapacity != tileCapacity` was an exact test for
//! *"this rival tile carries a standing improvement"* sitting on the row that denied one. The
//! builder half was not gated at all: the kit indices keyed purely by tile, so a rival mid-build
//! published `isField: false, fieldProgress: 0` beside their kit id, their finish date and their
//! queue position.
//!
//! **Every assertion reads the ENCODED envelope**, on `frame_is_viewer_scoped.rs`' rule: a field
//! that never reached the codec still satisfies an in-process assertion, and the published artifact
//! is the thing that leaks.

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    patch_carrying_capacity, publish_baseline_snapshot, run_turn, BuildJob, BuildQueueEntry,
    BuildSource, BuildTurns, ForagePatch, ForageRegistry, Improvement, LaborAllocation,
    LaborConfigHandle, LadderConfigHandle, PopulationCohort, RungKey, SnapshotHistory,
    VisibilityLedger,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The finish date every staged build is stamped with, and the place in the line beside it. Distinct
/// values so a leaked figure is unmistakable in a failure message rather than a plausible one.
const STAGED_BUILD_TURNS: u32 = 7;
const STAGED_QUEUE_POSITION: i32 = 0;
/// What the crew's tools add per turn — a third distinct figure, for the same reason.
const STAGED_BUILD_GEAR: f32 = 3.0;

/// The wire's *"there is no answer"* countdown and *"no band has queued this"* place.
const NO_BUILD_TURNS: i32 = sim_schema::NO_BUILD_TURNS_ESTIMATE;
const NOT_QUEUED: i32 = sim_schema::NOT_IN_ANY_BUILD_QUEUE;

/// The three tiles the fixture stages, and which faction improved each.
struct StagedTiles {
    /// The viewer's own improvement — the control arm. Nothing on this row may move.
    home: UVec2,
    /// A rival's improvement on ground the viewer has **never walked**. The row must read as wild.
    rival_unexplored: UVec2,
    /// A rival's improvement on ground the viewer **has** explored. The improvement reads through;
    /// the builder's state does not.
    rival_explored: UVec2,
}

/// One published patch row, reduced to the fields that could carry an improvement.
///
/// **A list of `(name, rendering)` pairs rather than a struct of typed fields**, because the claim
/// under test is *"nothing here differs"* and a mismatch has to print which field differed. Floats
/// are rendered rather than compared with a tolerance: both sides of every equality here are the
/// same expression evaluated on the same inputs, so bit-equality is the honest bar.
type PatchFingerprint = Vec<(&'static str, String)>;

fn fingerprint(row: &fb::ForagePatchState<'_>) -> PatchFingerprint {
    vec![
        ("hasOwner", format!("{}", row.hasOwner())),
        ("owner", format!("{}", row.owner())),
        ("isCultivated", format!("{}", row.isCultivated())),
        (
            "cultivationProgress",
            format!("{}", row.cultivationProgress()),
        ),
        ("isField", format!("{}", row.isField())),
        ("fieldProgress", format!("{}", row.fieldProgress())),
        ("carryingCapacity", format!("{}", row.carryingCapacity())),
        ("tileCapacity", format!("{}", row.tileCapacity())),
        ("biomass", format!("{}", row.biomass())),
        ("ecologyPhase", row.ecologyPhase().unwrap_or("").to_string()),
        (
            "provisionsPerBiomass",
            format!("{}", row.provisionsPerBiomass()),
        ),
        ("fodderPerBiomass", format!("{}", row.fodderPerBiomass())),
        ("perWorkerYield", format!("{}", row.perWorkerYield())),
        ("tendedYield", format!("{}", row.tendedYield())),
        ("fieldYield", format!("{}", row.fieldYield())),
        ("tendedFodder", format!("{}", row.tendedFodder())),
        ("fieldFodder", format!("{}", row.fieldFodder())),
        (
            "committedSpecies",
            row.committedSpecies().unwrap_or("").to_string(),
        ),
        (
            "committedDisplayName",
            row.committedDisplayName().unwrap_or("").to_string(),
        ),
        ("currentRung", row.currentRung().unwrap_or("").to_string()),
        (
            "cultivationWorkDone",
            format!("{}", row.cultivationWorkDone()),
        ),
        ("fieldWorkDone", format!("{}", row.fieldWorkDone())),
        (
            "cultivationWorkCost",
            format!("{}", row.cultivationWorkCost()),
        ),
        ("fieldWorkCost", format!("{}", row.fieldWorkCost())),
        ("upkeepDemand", format!("{}", row.upkeepDemand())),
        (
            "upkeepWorkersNeeded",
            format!("{}", row.upkeepWorkersNeeded()),
        ),
        ("meterRotPerTurn", format!("{}", row.meterRotPerTurn())),
        ("hasNeglectGrace", format!("{}", row.hasNeglectGrace())),
        (
            "buildDestinationRung",
            row.buildDestinationRung().unwrap_or("").to_string(),
        ),
        (
            "buildDestinationCapacity",
            format!("{}", row.buildDestinationCapacity()),
        ),
        ("buildKitId", row.buildKitId().unwrap_or("").to_string()),
        ("upkeepKitId", row.upkeepKitId().unwrap_or("").to_string()),
        ("upkeepKitNamed", format!("{}", row.upkeepKitNamed())),
        (
            "buildTurnsRemaining",
            format!("{}", row.buildTurnsRemaining()),
        ),
        (
            "buildQueuePosition",
            format!("{}", row.buildQueuePosition()),
        ),
        ("buildWorkFromGear", format!("{}", row.buildWorkFromGear())),
        (
            "composition",
            format!("{}", row.composition().map(|c| c.len()).unwrap_or(0)),
        ),
        (
            "compositionShares",
            row.composition()
                .map(|entries| {
                    entries
                        .iter()
                        .map(|entry| format!("{}:{}", entry.species().unwrap_or(""), entry.share()))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default(),
        ),
    ]
}

/// Every published patch row of the latest frame, fingerprinted and keyed by tile.
fn patch_rows(app: &App) -> std::collections::HashMap<(u32, u32), PatchFingerprint> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the frame publishes forage patches");
    rows.iter()
        .map(|row| ((row.x(), row.y()), fingerprint(&row)))
        .collect()
}

fn field_of<'a>(row: &'a PatchFingerprint, name: &str) -> &'a str {
    row.iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the fingerprint carries `{name}`"))
}

fn key(tile: UVec2) -> (u32, u32) {
    (tile.x, tile.y)
}

/// **Every tile whose published basket carries a plant that can climb to a Field, and which plant.**
///
/// Read off the wire rather than typed in: the roster is `flora_config.json`'s and a fixture holding
/// its own copy goes quietly vacuous the day the catalogue is retuned. **Not every patch qualifies**
/// — a whole basket of `wild`-ceiling plants is a perfectly good gathering site that nothing can be
/// sown on (`cultivation.md` -> "A GATHERING SITE ADMITS BASKETS RUNG 3 CANNOT COMMIT TO") — so the
/// fixture picks its three tiles out of this set rather than out of the registry at large.
fn sowable_species_by_tile(app: &App) -> std::collections::HashMap<(u32, u32), String> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the frame publishes forage patches");
    let mut out = std::collections::HashMap::new();
    for row in rows.iter() {
        if let Some(species) = row
            .composition()
            .into_iter()
            .flatten()
            .find(|entry| entry.canSow())
            .and_then(|entry| entry.species())
        {
            out.insert((row.x(), row.y()), species.to_string());
        }
    }
    out
}

/// **Raise this patch to a completed Field, committed to `species`, and stamp a build on it.**
///
/// The capacity is written through [`patch_carrying_capacity`] — the *one* expression
/// `advance_forage_regrowth` writes the live figure with — off the tile's own `K`, which is what the
/// patch is holding while it is still wild. Doing it here rather than by resolving another turn is
/// what keeps `biomass` still between the two captures, so the wild-ground comparison below is an
/// equality on the whole row and not on a hand-picked subset of it.
fn raise_to_a_field(
    patch: &mut ForagePatch,
    faction: core_sim::FactionId,
    species: &str,
    ladder: &core_sim::LadderConfig,
    labor: &core_sim::LaborConfig,
) {
    let tile_capacity = patch.carrying_capacity;
    // `(base, width)` — the rung is complete at the TOP of its span, which is their sum.
    let (base, width) = core_sim::plant_rung_span(RungKey::PlantField, ladder);
    patch.set_ladder_position(base + width, ladder);
    patch.owner = Some(faction);
    patch.species = Some(species.to_string());
    patch.carrying_capacity = patch_carrying_capacity(tile_capacity, patch, &labor.forage);
    assert!(
        patch.is_field(),
        "the fixture has to stage a real Field, or every assertion below is about wild ground"
    );
    // **The per-turn scratch the labor arm stamps for whichever band worked the source** — written
    // after the last turn resolved, because `advance_cultivation` clears it every Logistics pass.
    patch.build_turns_remaining = Some(BuildTurns::Turns(STAGED_BUILD_TURNS));
    patch.build_queue_position = STAGED_QUEUE_POSITION;
    patch.build_work_from_gear = STAGED_BUILD_GEAR;
}

/// A two-faction world in which **both** peoples hold a Field, and the rival holds two: one on
/// ground the viewer has walked and one on ground it has not.
///
/// ⛔ **NEVER `take(n)` OFF A `HashMap` IN A TEST** — `ForageRegistry::patches` iterates
/// non-deterministically and Rust seeds the hasher per process, so every pick here is made off a
/// sorted list and every precondition is *asserted* rather than assumed.
fn a_world_where_both_peoples_farm() -> (App, StagedTiles) {
    let mut app = world_with(ONE_RIVAL, |_| {});

    // **A turn first, so visibility has settled** — the ground rule below is chosen against the
    // ledger this turn produces, not against a guess about it.
    run_turn(&mut app);
    publish_baseline_snapshot(&mut app.world);

    let sowable = sowable_species_by_tile(&app);
    let mut candidates: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .filter(|tile| sowable.contains_key(&key(*tile)))
        .collect();
    // Row-major, the same order the capture publishes patches in.
    candidates.sort_unstable_by_key(|tile| (tile.y, tile.x));
    assert!(
        candidates.len() >= 3,
        "the map has to seed at least three sowable patches for the fixture to stage — got {}",
        candidates.len()
    );

    // **Chosen through the capture path's own predicate**, so no precondition can be wrong by
    // construction: `is_discovered` is exactly what `snapshot_forage_patches` asks.
    let (rival_explored, rival_unexplored, home) = {
        let ledger = app.world.resource::<VisibilityLedger>();
        let rival_explored = candidates
            .iter()
            .copied()
            .find(|tile| ledger.is_discovered(HOME, tile.x, tile.y))
            .expect(
                "the fixture needs a patch tile HOME HAS explored — the ground rule's surviving \
                 half is what proves the redaction is not a blanket one",
            );
        // The far end of the map: unexplored, and far enough that it stays that way.
        let rival_unexplored = candidates
            .iter()
            .rev()
            .copied()
            .find(|tile| *tile != rival_explored && !ledger.is_discovered(HOME, tile.x, tile.y))
            .expect(
                "the fixture needs a patch tile HOME has NOT explored — the rival's improvement \
                 has to sit on unexplored ground for the gate to be the thing under test",
            );
        let home = candidates
            .iter()
            .copied()
            .find(|tile| *tile != rival_explored && *tile != rival_unexplored)
            .expect("the map seeds enough forage patches for three distinct tiles");
        (rival_explored, rival_unexplored, home)
    };

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        for (tile, faction) in [
            (home, HOME),
            (rival_explored, RIVAL),
            (rival_unexplored, RIVAL),
        ] {
            let species = &sowable[&key(tile)];
            let patch = registry
                .patches
                .get_mut(&tile)
                .expect("the tile came out of this registry");
            raise_to_a_field(patch, faction, species, &ladder, &labor);
        }
    }

    // **Each faction's own band queues its own builds**, which is what makes `buildKitId` a live
    // reading rather than an index that happens to be empty.
    {
        let mut bands = app
            .world
            .query::<(&PopulationCohort, &mut LaborAllocation)>();
        for (cohort, mut allocation) in bands.iter_mut(&mut app.world) {
            let tiles: &[UVec2] = if cohort.faction == HOME {
                &[home]
            } else {
                &[rival_explored, rival_unexplored]
            };
            for tile in tiles {
                allocation.build_queue.push(BuildQueueEntry {
                    source: BuildSource::Patch(*tile),
                    declared: BuildJob::Rung(Improvement::Sow),
                    kit: None,
                });
            }
        }
    }

    // **Re-capture rather than resolve a second turn.** The scratch has to reach a published frame,
    // and another `run_turn` would clear it in the very Logistics pass that owns its one-turn cycle.
    publish_baseline_snapshot(&mut app.world);
    (
        app,
        StagedTiles {
            home,
            rival_unexplored,
            rival_explored,
        },
    )
}

/// **The staging really staged something** — the liveness half, and it is not optional: every
/// assertion below is satisfied by a world in which no improvement was ever raised.
#[test]
fn the_viewers_own_field_reads_through_in_every_field() {
    let (app, tiles) = a_world_where_both_peoples_farm();
    let rows = patch_rows(&app);
    let own = rows
        .get(&key(tiles.home))
        .expect("the viewer's own row rides");

    assert_eq!(field_of(own, "isField"), "true", "the viewer's own Field");
    assert_eq!(field_of(own, "hasOwner"), "true");
    assert_eq!(field_of(own, "owner"), HOME.0.to_string());
    assert_ne!(
        field_of(own, "carryingCapacity"),
        field_of(own, "tileCapacity"),
        "a Field raises the ground's `K`, and the viewer's own row states it"
    );
    assert_ne!(
        field_of(own, "committedSpecies"),
        "",
        "the viewer's own row names the crop it is committed to"
    );
    assert_eq!(
        field_of(own, "buildTurnsRemaining"),
        STAGED_BUILD_TURNS.to_string(),
        "the viewer's own builder state reads through"
    );
    assert_eq!(
        field_of(own, "buildQueuePosition"),
        STAGED_QUEUE_POSITION.to_string()
    );
    assert_eq!(
        field_of(own, "buildWorkFromGear"),
        STAGED_BUILD_GEAR.to_string()
    );
    assert_ne!(
        field_of(own, "buildKitId"),
        "",
        "the viewer's own queue resolves a builders kit onto its own row — without this the kit \
         assertions below pass on an index that is simply empty"
    );
}

/// ⛔ **A RIVAL'S FIELD ON GROUND THE VIEWER HAS NEVER WALKED PUBLISHES WILD GROUND** — not zeros,
/// which would be a lie about terrain the map already publishes whole, and not a row whose fields
/// disagree with each other.
///
/// The bar is an equality on the **whole fingerprint** against the row that very tile published
/// while it was genuinely wild, which is why the fixture moves no biomass between the two captures.
/// A subset bar is what let `carryingCapacity` leak for as long as it did: the five fields somebody
/// thought of were gated and the ones derived from the same improvement were not.
#[test]
fn a_rivals_field_on_unexplored_ground_is_indistinguishable_from_wild_ground() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    run_turn(&mut app);
    publish_baseline_snapshot(&mut app.world);

    // The wild reading of every tile, taken before anything is staged.
    let (app, tiles) = {
        let wild = patch_rows(&app);
        drop(app);
        let (app, tiles) = a_world_where_both_peoples_farm();
        // The two worlds are built from the same seed and the same roster through the same call, so
        // the wild reading is this tile's own — asserted rather than assumed by comparing a tile
        // nothing touched.
        let staged = patch_rows(&app);
        let untouched = staged
            .keys()
            .find(|k| {
                **k != key(tiles.home)
                    && **k != key(tiles.rival_explored)
                    && **k != key(tiles.rival_unexplored)
            })
            .copied()
            .expect("the map has more patches than the three the fixture stages");
        assert_eq!(
            wild.get(&untouched),
            staged.get(&untouched),
            "the two worlds must be the same world, or the wild reading below is another map's"
        );
        assert_eq!(
            wild.get(&key(tiles.rival_unexplored)),
            staged.get(&key(tiles.rival_unexplored)),
            "a rival's Field on unexplored ground must publish exactly what that tile published \
             while it was wild — every field, not the five somebody thought of"
        );
        (app, tiles)
    };

    // …and the same row, stated positively, so a failure names the leak rather than a diff.
    let rows = patch_rows(&app);
    let row = rows
        .get(&key(tiles.rival_unexplored))
        .expect("a patch row is a fact about a tile and always rides");
    assert_eq!(field_of(row, "hasOwner"), "false");
    assert_eq!(field_of(row, "isField"), "false");
    assert_eq!(field_of(row, "isCultivated"), "false");
    assert_eq!(field_of(row, "committedSpecies"), "");
    assert_eq!(
        field_of(row, "carryingCapacity"),
        field_of(row, "tileCapacity"),
        "⛔ THE LEAK ITSELF: `carryingCapacity != tileCapacity` is an exact test for a standing \
         improvement, published on the row that denies one"
    );
}

/// ⛔ **AND IT CARRIES NONE OF THE BUILDER'S STATE** — the second rule, on the same row.
#[test]
fn a_rivals_build_on_unexplored_ground_publishes_no_kit_date_or_place() {
    let (app, tiles) = a_world_where_both_peoples_farm();
    let rows = patch_rows(&app);
    let row = rows
        .get(&key(tiles.rival_unexplored))
        .expect("a patch row is a fact about a tile and always rides");

    assert_eq!(
        field_of(row, "buildKitId"),
        "",
        "a rival's builders kit is not the viewer's to read"
    );
    assert_eq!(field_of(row, "upkeepKitId"), "");
    assert_eq!(field_of(row, "upkeepKitNamed"), "false");
    assert_eq!(
        field_of(row, "buildTurnsRemaining"),
        NO_BUILD_TURNS.to_string(),
        "a rival's finish date is not the viewer's to read"
    );
    assert_eq!(
        field_of(row, "buildQueuePosition"),
        NOT_QUEUED.to_string(),
        "a rival's place in their own line is not the viewer's to read"
    );
    assert_eq!(field_of(row, "buildWorkFromGear"), "0");
    assert_eq!(field_of(row, "buildDestinationRung"), "");
}

/// ⛔ **THE GROUND RULE SURVIVES THE FIX.** A field is built into the ground and does not wander
/// off, so a rival's improvement on ground the viewer HAS explored still reads as an improvement —
/// while the state of *their builders* on it still does not.
///
/// Without this the whole redaction could be a blanket "a rival publishes nothing", which is a
/// different rule and one the improvement half is explicitly not.
#[test]
fn a_rivals_field_on_explored_ground_reads_improved_but_not_who_is_building_it() {
    let (app, tiles) = a_world_where_both_peoples_farm();
    let rows = patch_rows(&app);
    let row = rows
        .get(&key(tiles.rival_explored))
        .expect("a patch row is a fact about a tile and always rides");

    assert_eq!(
        field_of(row, "isField"),
        "true",
        "explored ground remembers the field standing on it"
    );
    assert_eq!(field_of(row, "hasOwner"), "true");
    assert_eq!(field_of(row, "owner"), RIVAL.0.to_string());
    assert_ne!(
        field_of(row, "carryingCapacity"),
        field_of(row, "tileCapacity"),
        "the improvement's own capacity gain reads through on explored ground"
    );

    assert_eq!(
        field_of(row, "buildKitId"),
        "",
        "the improvement follows the ground; WHO is raising it follows the builder"
    );
    assert_eq!(
        field_of(row, "buildTurnsRemaining"),
        NO_BUILD_TURNS.to_string()
    );
    assert_eq!(field_of(row, "buildQueuePosition"), NOT_QUEUED.to_string());
    assert_eq!(field_of(row, "buildWorkFromGear"), "0");
}
