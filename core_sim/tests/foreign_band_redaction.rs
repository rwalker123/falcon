//! ⛔ **A SNAPSHOT IS ONE VIEWER'S VIEW, AND THE BAND LIST WAS THE ONE THING PUBLISHED WHOLE.**
//!
//! Herds are fog-filtered, connections are filtered on the observer's faction and roads are gated on
//! `Discovered` — but `PopulationSnapshotQuery` carried no `With`, no faction predicate and no
//! visibility test, and `population_states` mapped straight over it. So every connected client
//! received every band's complete internal state: morale, larder, runway, knowledge fragments, labor
//! assignments, equipment, bench, build queue, reachable stockpile, outfitting window, exact
//! position — and its **pending defection**. The Godot client declined to *draw* most of it, which
//! is presentation, not a boundary.
//!
//! **Every assertion here reads the ENCODED envelope**, through the same accessor chain a client
//! uses. A field that never reached the codec still satisfies an in-process assertion, and the
//! published artifact is the thing that leaks.

use bevy::prelude::*;

mod faction_support;

use core_sim::{run_turn, BandId, PopulationCohort, SnapshotHistory, TileRegistry};
use faction_support::{human_and_ai, world_with, HOME, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// How far from the viewer's own band the rival is parked when a test wants it **out of sight**.
///
/// Far past any sight range in `visibility_config.json`, and past the map's own wrap-around
/// shortcut on the shipped 80-wide grid, so "not visible" is a property of the distance rather than
/// of which direction the test happened to walk.
const OUT_OF_SIGHT_TILES: u32 = 30;

/// What one published row says about a band, decoded off the wire.
///
/// A struct rather than a pile of asserts so a failure prints **everything** the row leaked at once
/// — a redaction that lets one field through is much easier to read as a whole row than as the
/// first assertion that happened to trip.
#[derive(Debug, Default, PartialEq)]
struct PublishedRow {
    band_id: u64,
    faction: u32,
    name: String,
    position: (u32, u32),
    size: u32,
    // …and everything below this line is what a foreign band must NOT say.
    morale: i64,
    turns_of_food: f32,
    age_turns: u32,
    activity: String,
    knowledge_fragments: usize,
    labor_assignments: usize,
    stores: usize,
    working_age: u32,
    children_count: u32,
    elders_count: u32,
    idle_workers: u32,
    build_queue: usize,
    equipment_batches: usize,
    craft_offers: usize,
    /// **The bench is captured by its CONTENT, not by its presence.** `PopulationCohortState.bench`
    /// is a plain `BenchState`, not an `Option`, so the codec writes a table on every row and always
    /// will; "redacted" for a non-optional table means **default-valued**, and a default bench is an
    /// idle one — empty recipe, no crew, no progress. That says nothing about the band.
    bench_recipe: String,
    bench_workers: u32,
    has_migration: bool,
    has_accessible_stockpile: bool,
    has_loadout_window: bool,
}

impl PublishedRow {
    /// The sensitive half, zeroed — what a redacted row must equal on everything but its identity.
    fn is_fully_redacted(&self) -> bool {
        *self
            == PublishedRow {
                band_id: self.band_id,
                faction: self.faction,
                name: self.name.clone(),
                position: self.position,
                size: self.size,
                ..PublishedRow::default()
            }
    }
}

/// Every band row in the latest captured frame, decoded from the encoded envelope.
fn published_rows(app: &App) -> Vec<PublishedRow> {
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
        .expect("the population section is published")
        .iter()
        .map(|row| PublishedRow {
            band_id: row.bandId(),
            faction: row.faction(),
            name: row.name().unwrap_or_default().to_string(),
            position: (row.currentX(), row.currentY()),
            size: row.size(),
            morale: row.morale(),
            turns_of_food: row.turnsOfFood(),
            age_turns: row.ageTurns(),
            activity: row.activity().unwrap_or_default().to_string(),
            knowledge_fragments: row.knowledgeFragments().map(|v| v.len()).unwrap_or(0),
            labor_assignments: row.laborAssignments().map(|v| v.len()).unwrap_or(0),
            stores: row.stores().map(|v| v.len()).unwrap_or(0),
            working_age: row.workingAge(),
            children_count: row.childrenCount(),
            elders_count: row.eldersCount(),
            idle_workers: row.idleWorkers(),
            build_queue: row.buildQueue().map(|v| v.len()).unwrap_or(0),
            equipment_batches: row.equipmentBatches().map(|v| v.len()).unwrap_or(0),
            craft_offers: row.craftOffers().map(|v| v.len()).unwrap_or(0),
            bench_recipe: row
                .bench()
                .and_then(|bench| bench.recipeId())
                .unwrap_or_default()
                .to_string(),
            bench_workers: row.bench().map(|bench| bench.workers()).unwrap_or(0),
            has_migration: row.migration().is_some(),
            has_accessible_stockpile: row.accessibleStockpile().is_some(),
            has_loadout_window: row.loadoutWindow().is_some(),
        })
        .collect()
}

fn row_for(app: &App, band: BandId) -> Option<PublishedRow> {
    published_rows(app)
        .into_iter()
        .find(|row| row.band_id == band.0)
}

fn opening_band(app: &mut App, faction: core_sim::FactionId) -> (BandId, Entity) {
    app.world
        .query::<(Entity, &BandId, &PopulationCohort)>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, band, _)| (*band, entity))
        .min_by_key(|(band, _)| *band)
        .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
}

fn position_of(app: &App, entity: Entity) -> UVec2 {
    let cohort = app
        .world
        .get::<PopulationCohort>(entity)
        .expect("the band exists");
    app.world
        .get::<core_sim::Tile>(cohort.current_tile)
        .expect("the band stands on a tile")
        .position
}

/// Re-home `band` onto the tile at `target`, the way an arriving band would stand there.
fn stand_at(app: &mut App, band: Entity, target: UVec2) {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("the target tile is on the map");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.home = tile;
    cohort.current_tile = tile;
}

/// A two-faction world with the rival standing **beside** the viewer's own band, so the viewer can
/// genuinely see it, and one turn resolved so `calculate_visibility` has run over those positions.
fn a_rival_in_plain_sight() -> (App, BandId, BandId) {
    let mut app = world_with(&human_and_ai(), |_| {});
    let (home_band, home_entity) = opening_band(&mut app, HOME);
    let (rival_band, rival_entity) = opening_band(&mut app, RIVAL);
    let beside = position_of(&app, home_entity);
    stand_at(&mut app, rival_entity, beside);
    run_turn(&mut app);
    (app, home_band, rival_band)
}

/// The same world with the rival left where worldgen put it, then walked further still — far outside
/// any band's sight.
fn a_rival_over_the_horizon() -> (App, BandId, BandId) {
    let mut app = world_with(&human_and_ai(), |_| {});
    let (home_band, home_entity) = opening_band(&mut app, HOME);
    let (rival_band, rival_entity) = opening_band(&mut app, RIVAL);
    let home_pos = position_of(&app, home_entity);
    let grid = app.world.resource::<core_sim::SimulationConfig>().grid_size;
    let far = UVec2::new(
        (home_pos.x + OUT_OF_SIGHT_TILES) % grid.x.max(1),
        home_pos.y.min(grid.y.saturating_sub(1)),
    );
    stand_at(&mut app, rival_entity, far);
    run_turn(&mut app);
    (app, home_band, rival_band)
}

/// ⛔ **TIER 2 — a foreign band you can see publishes where it is, who it is, and how big it is.
/// Nothing else.**
#[test]
fn a_foreign_band_in_view_publishes_only_its_position_name_and_scale() {
    let (app, _, rival_band) = a_rival_in_plain_sight();
    let row =
        row_for(&app, rival_band).expect("a rival standing in view still has a marker to draw");

    assert_eq!(row.faction, RIVAL.0, "the row says whose people they are");
    assert!(
        !row.name.is_empty(),
        "and what they call themselves, which the client has no roster to resolve"
    );
    assert!(row.size > 0, "and roughly how many there are: {row:?}");
    assert!(
        row.is_fully_redacted(),
        "a foreign band must publish nothing but its identity, position and scale: {row:?}"
    );
}

/// ⛔ **TIER 3 — a foreign band you cannot see is not in the frame at all.**
#[test]
fn a_foreign_band_out_of_view_is_absent_from_the_frame() {
    let (app, home_band, rival_band) = a_rival_over_the_horizon();
    assert!(
        row_for(&app, home_band).is_some(),
        "the liveness half: the viewer's own band is still published, so an empty section \
         cannot be what makes this pass"
    );
    assert!(
        row_for(&app, rival_band).is_none(),
        "a band over the horizon has no row: {:?}",
        published_rows(&app)
    );
}

/// ⛔ **TIER 1 — the viewer's own bands are untouched, and the gate is SYMMETRIC.**
///
/// One world, captured for each faction in turn: each sees its own band in full and the other's
/// redacted. Stated as an A/B on one world rather than against a recorded baseline because the
/// failure worth catching is a *mis-gated* comparison — `==` for `!=`, or a redaction that leaked
/// onto the owner — and both arms of one world expose it from both sides.
#[test]
fn each_faction_sees_its_own_band_in_full_and_the_others_redacted() {
    let (mut app, home_band, rival_band) = a_rival_in_plain_sight();

    let home_view_own = row_for(&app, home_band).expect("the viewer's own band is published");
    let home_view_rival = row_for(&app, rival_band).expect("the rival is in view");
    assert!(
        !home_view_own.is_fully_redacted(),
        "the viewer's own row must carry its live state: {home_view_own:?}"
    );
    assert!(home_view_rival.is_fully_redacted());

    // Now capture the same world for the other people.
    app.world.insert_resource(core_sim::ViewerFaction(RIVAL));
    run_turn(&mut app);

    let rival_view_own = row_for(&app, rival_band).expect("the rival's own band is published");
    let rival_view_home = row_for(&app, home_band).expect("the human is in view");
    assert!(
        !rival_view_own.is_fully_redacted(),
        "the same band that was redacted a frame ago is full in its owner's frame: \
         {rival_view_own:?}"
    );
    assert!(
        rival_view_home.is_fully_redacted(),
        "and the human, full a frame ago, is redacted in the rival's: {rival_view_home:?}"
    );
}

/// ⛔ **FOG IS NOT A DISCLOSURE SWITCH.**
///
/// `fog_enabled: false` moves the line between tier 2 and tier 3 — every foreign band gets a row —
/// and never the line between tier 1 and tier 2. Turning fog off shows you *where* a rival's camps
/// are; it does not entitle you to their insides.
#[test]
fn fog_off_reveals_where_a_foreign_band_is_and_still_says_nothing_about_it() {
    let mut app = world_with(&human_and_ai(), |config| config.fog_enabled = false);
    let (_, home_entity) = opening_band(&mut app, HOME);
    let (rival_band, rival_entity) = opening_band(&mut app, RIVAL);
    let home_pos = position_of(&app, home_entity);
    let grid = app.world.resource::<core_sim::SimulationConfig>().grid_size;
    stand_at(
        &mut app,
        rival_entity,
        UVec2::new(
            (home_pos.x + OUT_OF_SIGHT_TILES) % grid.x.max(1),
            home_pos.y.min(grid.y.saturating_sub(1)),
        ),
    );
    run_turn(&mut app);

    let row = row_for(&app, rival_band)
        .expect("with fog off, a band over the horizon is still on the map");
    assert!(
        row.is_fully_redacted(),
        "fog decides what you can SEE, never what you are entitled to KNOW: {row:?}"
    );
}
