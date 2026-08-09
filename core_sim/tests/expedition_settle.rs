//! **"Start a life here"** — the arrival verb that founds a resident band
//! (`docs/plan_band_fission.md` §Q2, issue #510).
//!
//! A party sitting in `ExpeditionPhase::AwaitingOrders` drops its `Expedition` component and gains
//! `ResidentBand`, keeping the `BandId` it was allocated at launch and the map it learned on the
//! way. Two things are worth pinning beyond the swap itself:
//!
//! - **The reachability gate is tested as a PAIR.** A one-sided test passes against a gate that
//!   always allows, so every refusal fixture below has an admission twin built from the same world
//!   with one fact changed — the corridor being mapped or not.
//! - **The gate is evaluated against what the faction knew BEFORE the founding.** The fixtures
//!   therefore reset the visibility ledger and state exactly what the faction has discovered,
//!   rather than inheriting whatever worldgen happened to reveal.

use std::collections::HashSet;
use std::sync::Arc;

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::grid_utils::{hex_distance_wrapped, hex_neighbors_wrapped};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    culture_region_at, found_band_from_expedition, founding_refusals, founding_site_is_reachable,
    run_turn, BandEquipment, BandId, CultureManager, CultureOwner, DemographicFlowAccumulator,
    EquipmentConfigHandle, Expedition, ExpeditionConfig, ExpeditionConfigHandle, ExpeditionMission,
    ExpeditionPhase, FactionId, FoundedBand, FoundingRefusal, FoundingRefusals, KitJob,
    LaborAllocation, PopulationCohort, ProvinceMap, ResidentBand, SimulationConfig, StartingUnit,
    Tile, TileRegistry, VisibilityLedger, CULTURE_TRAIT_AXES, TRADE_GOODS,
};
use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
use sim_runtime::TerrainTags;

/// The id every fixture party carries. Worldgen's own bands take the low ids, so a value well clear
/// of them keeps "which band is this?" unambiguous when a test reads the feed or a checkpoint.
const PARTY_BAND_ID: u64 = 9_001;

/// A second party, for the fixture that proves an expedition is not an anchor.
const BYSTANDER_BAND_ID: u64 = 9_002;

/// How far the fixtures walk their corridor away from the home band. Far enough that the site is
/// well outside the band's own fog reveal, short enough that a land run of this length exists on
/// every seeded map.
const CORRIDOR_TILES: usize = 6;

/// Trade goods loaded into a fixture party's pack, so "the haul settles into the new band's own
/// store" is measurable rather than a no-op on an empty pack.
const FIXTURE_CARRIED_TRADE: f32 = 3.5;

/// Build a headless world on a pinned earthlike map — one `update()` runs the whole Startup
/// worldgen chain and resolves turn 1, so there is a real resident band standing on real terrain.
fn spawn_world() -> App {
    let mut app = core_sim::build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The first resident band: its entity, faction and the tile it stands on.
fn home_band(app: &mut App) -> (Entity, FactionId, UVec2) {
    let (entity, faction, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
        let (entity, cohort) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    (entity, faction, position)
}

/// A **walkable path** of `CORRIDOR_TILES` land steps out from `from`, `from` itself at the head and
/// the founding site at the tail.
///
/// It has to be a *path* and not merely a set of nearby land: the refusal fixture leaves the
/// interior unmapped, so if the tail were adjacent to the head the gate would rightly admit it and
/// the pair would be testing nothing. Breadth-first with parent pointers, because a straight line
/// runs into the sea.
fn land_corridor(app: &App, from: UVec2) -> Vec<UVec2> {
    let registry = app.world.resource::<TileRegistry>();
    let (width, height) = (registry.width, registry.height);
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let is_land = |pos: UVec2| {
        registry
            .index(pos.x, pos.y)
            .and_then(|tile| app.world.get::<Tile>(tile))
            .map(|tile| !tile.terrain_tags.contains(TerrainTags::WATER))
            .unwrap_or(false)
    };

    let mut came_from: std::collections::HashMap<UVec2, UVec2> = std::collections::HashMap::new();
    let mut seen: HashSet<UVec2> = HashSet::from([from]);
    let mut frontier = vec![from];
    for _ in 0..CORRIDOR_TILES {
        let mut next = Vec::new();
        for pos in frontier.drain(..) {
            for (x, y) in hex_neighbors_wrapped(pos.x, pos.y, width, height, wrap) {
                let neighbor = UVec2::new(x, y);
                if !seen.insert(neighbor) || !is_land(neighbor) {
                    continue;
                }
                came_from.insert(neighbor, pos);
                next.push(neighbor);
            }
        }
        frontier = next;
        assert!(
            !frontier.is_empty(),
            "the pinned map must offer a contiguous land walk of {CORRIDOR_TILES} steps from \
             {from:?}"
        );
    }

    let mut corridor = vec![frontier[0]];
    while let Some(previous) = came_from.get(corridor.last().expect("path is non-empty")) {
        corridor.push(*previous);
    }
    corridor.reverse();
    assert_eq!(
        corridor.len(),
        CORRIDOR_TILES + 1,
        "the reconstructed path must be exactly the walk that was searched"
    );
    corridor
}

/// Wipe the faction map and state exactly what this faction has discovered.
///
/// Replacing the ledger rather than adding to it is deliberate: the gate asks what the faction knew,
/// so a fixture about *not* knowing something has to be able to say so.
fn set_discovered(app: &mut App, faction: FactionId, tiles: &[UVec2]) {
    let registry = app.world.resource::<TileRegistry>();
    let (width, height) = (registry.width, registry.height);
    app.world.insert_resource(VisibilityLedger::default());
    let mut ledger = app.world.resource_mut::<VisibilityLedger>();
    let map = ledger.ensure_faction(faction, width, height);
    for pos in tiles {
        map.discover(pos.x, pos.y, 0);
    }
}

/// The founding floor as the shipped config states it. Read rather than restated, so a retune moves
/// every fixture below with it instead of turning them red.
fn min_founding_workers(app: &App) -> u32 {
    app.world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .min_founding_workers
}

/// **Ground this faction genuinely cannot see** — the farthest undiscovered land tile from `from`.
///
/// The corridor fixtures state what the faction knows by *replacing* the ledger, which works because
/// they never advance a turn: `calculate_visibility` rebuilds the faction map every turn, and a
/// starting band's scout vantages map ~12 tiles out, so a wiped ledger is partly undone by the very
/// next turn. A fixture that has to still be standing on unmapped ground *after* a turn therefore
/// has to find ground the band cannot reach at all, and the far side of the map is that ground.
fn farthest_undiscovered_land(app: &App, faction: FactionId, from: UVec2) -> UVec2 {
    let registry = app.world.resource::<TileRegistry>();
    let (width, height) = (registry.width, registry.height);
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let ledger = app.world.resource::<VisibilityLedger>();
    let mut best: Option<(u32, UVec2)> = None;
    for y in 0..height {
        for x in 0..width {
            let position = UVec2::new(x, y);
            let is_land = registry
                .index(x, y)
                .and_then(|tile| app.world.get::<Tile>(tile))
                .map(|tile| !tile.terrain_tags.contains(TerrainTags::WATER))
                .unwrap_or(false);
            if !is_land || ledger.is_discovered(faction, x, y) {
                continue;
            }
            let distance = hex_distance_wrapped(from, position, width, wrap);
            if best.map(|(far, _)| distance > far).unwrap_or(true) {
                best = Some((distance, position));
            }
        }
    }
    best.expect("a seeded map holds land the starting band has never seen")
        .1
}

/// Staff `party` with exactly `working` working-age people (fractional on purpose — `working` carries
/// sub-person demographic precision, and the gate floors it).
fn staff_party(app: &mut App, party: Entity, working: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(party)
        .expect("the fixture party has a cohort");
    cohort.working = core_sim::scalar_from_f32(working);
    cohort.sync_size();
}

/// Install `config` as the live expedition tuning, so a test can drive the gate off a number nobody
/// ships.
fn set_expedition_config(app: &mut App, config: ExpeditionConfig) {
    app.world
        .insert_resource(ExpeditionConfigHandle::new(Arc::new(config)));
}

/// Spawn a detached party standing at `at`, shaped exactly as `handle_send_expedition` shapes one:
/// its own `BandId`, a pristine kit, no `ResidentBand`, and **no** `DemographicFlowAccumulator`.
///
/// **The party is staffed at exactly `settle.min_founding_workers`** — the smallest party that may
/// found. Every fixture here is about something other than party size, so sitting on the floor keeps
/// them honest (a founding fixture that happened to carry the parent band's whole workforce would
/// stop testing the gate the moment the floor was raised) and doubles as the admission twin for the
/// refusal below it.
fn spawn_party(
    app: &mut App,
    home: Entity,
    band_id: u64,
    at: UVec2,
    phase: ExpeditionPhase,
) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("fixture site resolves to a tile");
    let mut cohort = app
        .world
        .get::<PopulationCohort>(home)
        .expect("the home band has a cohort")
        .clone();
    cohort.home = tile;
    cohort.current_tile = tile;
    cohort.children = core_sim::scalar_from_f32(0.0);
    cohort.elders = core_sim::scalar_from_f32(0.0);
    cohort.stores = core_sim::LocalStore::new();
    cohort.working = core_sim::scalar_from_f32(min_founding_workers(app) as f32);
    // The parent's age, exactly as the launcher leaves it after twenty turns of walking — the value
    // `age_turns = 0` at founding exists to overwrite.
    cohort.age_turns = 42;
    cohort.sync_size();
    let kit = app
        .world
        .resource::<EquipmentConfigHandle>()
        .get()
        .default_kit(KitJob::Hunt);
    app.world
        .spawn((
            cohort,
            BandId(band_id),
            LaborAllocation::default(),
            BandEquipment::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: home,
                mission: ExpeditionMission::Scout,
                phase,
                announced: true,
                pending_reveal: Vec::new(),
                carried_trade: FIXTURE_CARRIED_TRADE,
                kit,
                // Derived per-turn telemetry: `assess_foundings` fills it on the next Visibility
                // stage, so a freshly spawned fixture party starts with no verdict.
                founding_refusals: Vec::new(),
            },
        ))
        .id()
}

/// **The refusal set a founding attempt produced**, as a plain vec.
///
/// An `Ok` maps to the **empty** set on purpose: empty *is* the "you may found" signal, so a test
/// that expects a legal founding and one that expects a refusal compare the same kind of thing.
fn refusals_of(result: Result<FoundedBand, FoundingRefusals>) -> Vec<FoundingRefusal> {
    result
        .err()
        .map(|refusals| refusals.to_vec())
        .unwrap_or_default()
}

/// **What the sim actually SHIPPED for `band_id`** — the founding refusal tokens and the published
/// worker floor, read back off the encoded FlatBuffers frame rather than off the in-process state.
///
/// The whole point of the field is that a client reads it, and a field that never reached the codec
/// still passes an in-process assertion — so every wire claim in this file goes through here.
fn published_founding_verdict(app: &mut App, band_id: u64) -> (Vec<String>, u32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    recapture_snapshot_in_place(&mut app.world);
    let bytes = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .encode_flat();
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohorts = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the snapshot carries a population section");
    let cohort = cohorts
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the fixture band is on the wire");
    let tokens = cohort
        .foundingRefusals()
        .expect("the refusal list is always written, empty or not")
        .iter()
        .map(|token| token.to_string())
        .collect();
    (tokens, cohort.foundingMinWorkers())
}

/// The entity carrying `band_id`, whatever components it happens to hold.
fn entity_for_band(app: &mut App, band_id: u64) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| id.0 == band_id)
        .map(|(entity, _)| entity)
        .expect("the fixture band is in the world")
}

// -------------------------------------------------------------------------------------------
// The swap
// -------------------------------------------------------------------------------------------

/// **The founding is one component swap, and the identity survives it.** The party stops being an
/// expedition, joins the resident systems, and is still the same band by id.
#[test]
fn a_founding_swaps_the_expedition_for_a_resident_band() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );

    let founded =
        found_band_from_expedition(&mut app.world, party).expect("a mapped site admits a founding");
    assert_eq!(founded.at, site, "the band stands where the party stood");

    assert!(
        app.world.get::<Expedition>(party).is_none(),
        "the party stopped being an expedition"
    );
    assert!(
        app.world.get::<ResidentBand>(party).is_some(),
        "the party became a resident band"
    );
    assert!(
        app.world.get::<DemographicFlowAccumulator>(party).is_some(),
        "a resident band without a flow accumulator can never report a birth or a death"
    );
    assert_eq!(
        app.world.get::<BandId>(party).map(|id| id.0),
        Some(PARTY_BAND_ID),
        "the band keeps the id it was allocated at launch"
    );

    let cohort = app
        .world
        .get::<PopulationCohort>(party)
        .expect("the founded band still has its cohort");
    assert_eq!(
        cohort.age_turns, 0,
        "the band was founded now — the parent's age came along with the cloned cohort and must \
         not be inherited (`migration_min_settled_turns` reads it)"
    );
    let site_tile = app
        .world
        .resource::<TileRegistry>()
        .index(site.x, site.y)
        .expect("site tile resolves");
    assert_eq!(
        cohort.home, site_tile,
        "the founding site is the band's home, which is the tile its demographics read terrain off"
    );
    assert!(
        (cohort.stores.get(TRADE_GOODS).to_f32() - FIXTURE_CARRIED_TRADE).abs() < 1e-3,
        "the pack settles into the new band's OWN store — the haul never went home"
    );
    assert!(
        (founded.trade_settled.to_f32() - FIXTURE_CARRIED_TRADE).abs() < 1e-3,
        "the feed line reports what was actually banked"
    );
}

/// **A party still under orders cannot found.** Founding is the third *arrival* action, so it is
/// offered only where onward and recall are.
#[test]
fn a_party_that_has_not_arrived_is_refused() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    // The site is fully mapped, so only the phase can refuse this.
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::Outbound,
    );

    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::NotAwaitingOrders],
        "a structural refusal stands alone: nothing else can be assessed about a party that is \
         not there yet"
    );
    assert!(
        app.world.get::<Expedition>(party).is_some(),
        "a refusal refuses the founding, not the party"
    );
    assert!(
        app.world.get::<ResidentBand>(party).is_none(),
        "a refused founding leaves the party detached"
    );
}

// -------------------------------------------------------------------------------------------
// The minimum-worker floor — a gate on the PARTY, not on the place
// -------------------------------------------------------------------------------------------

/// **A party below the floor cannot found**, and the refusal names the numbers it turned on. The
/// founding is otherwise perfectly legal — mapped corridor, `AwaitingOrders` — so only the party's
/// size can refuse it.
///
/// The load-bearing half is the second: a refusal refuses the *founding*, never the party. The
/// expedition is left exactly as it stood, so recall still works and the player has lost nothing by
/// asking.
#[test]
fn a_party_below_the_worker_floor_cannot_found_a_band() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    let floor = min_founding_workers(&app);
    let short = floor.saturating_sub(1);
    staff_party(&mut app, party, short as f32);

    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::PartyTooSmall {
            workers: short,
            required: floor,
        }],
        "the refusal must name the party it counted and the floor it compared against — a gate \
         that refuses with the wrong numbers tells the player something false"
    );
    assert!(
        app.world.get::<Expedition>(party).is_some(),
        "a refusal refuses the founding, not the party"
    );
    assert!(
        app.world.get::<ResidentBand>(party).is_none(),
        "a refused founding leaves the party detached"
    );
    assert_eq!(
        app.world
            .get::<Expedition>(party)
            .map(|expedition| expedition.phase),
        Some(ExpeditionPhase::AwaitingOrders),
        "the party is still awaiting orders, so recall is still available"
    );
}

/// **The floor admits the party standing on it.** Paired with the refusal above so "refuse
/// everything" cannot pass: the same fixture at exactly `settle.min_founding_workers` founds.
#[test]
fn a_party_at_the_worker_floor_founds() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    let floor = min_founding_workers(&app);
    staff_party(&mut app, party, floor as f32);

    assert!(
        found_band_from_expedition(&mut app.world, party).is_ok(),
        "a party of exactly the floor is a party that may found — the gate is `<`, not `<=`"
    );
    assert!(
        app.world.get::<ResidentBand>(party).is_some(),
        "the admitted party became a resident band"
    );
}

/// **The floor is the CONFIGURED number, not a constant in the code.** One fixture, driven twice: a
/// floor of one admits the party a floor well above it refuses.
#[test]
fn the_worker_floor_is_the_configured_number() {
    /// A floor no fixture party could ever clear — the "refuses" half.
    const IMPOSSIBLE_FLOOR: u32 = 500;
    /// The smallest floor the validator accepts: a party of one, which is a real party.
    const PERMISSIVE_FLOOR: u32 = 1;
    /// What both halves staff the party at — comfortably above `PERMISSIVE_FLOOR` and hopelessly
    /// below `IMPOSSIBLE_FLOOR`, so only the config can decide the verdict.
    const PARTY_WORKERS: f32 = 2.0;

    for (floor, admits) in [(PERMISSIVE_FLOOR, true), (IMPOSSIBLE_FLOOR, false)] {
        let mut app = spawn_world();
        let (home, faction, home_pos) = home_band(&mut app);
        let corridor = land_corridor(&app, home_pos);
        let site = *corridor.last().expect("corridor has a tail");
        set_discovered(&mut app, faction, &corridor);
        let mut config = (*app.world.resource::<ExpeditionConfigHandle>().get()).clone();
        config.settle.min_founding_workers = floor;
        set_expedition_config(&mut app, config);
        let party = spawn_party(
            &mut app,
            home,
            PARTY_BAND_ID,
            site,
            ExpeditionPhase::AwaitingOrders,
        );
        staff_party(&mut app, party, PARTY_WORKERS);

        let verdict = found_band_from_expedition(&mut app.world, party);
        assert_eq!(
            verdict.is_ok(),
            admits,
            "a party of {PARTY_WORKERS} against a configured floor of {floor} must follow the \
             config, got {verdict:?}"
        );
    }
}

/// **A fraction of a person is not a worker.** `working` carries sub-person demographic precision,
/// so the gate floors it: a party of 3.9 is three workers against a floor of four, and it is refused
/// reporting *three* — the number the labor allocator would give it.
#[test]
fn a_fractional_worker_does_not_round_up_into_the_floor() {
    /// How far below the floor the fixture sits — enough that rounding would cross it.
    const SHORTFALL: f32 = 0.1;

    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    let floor = min_founding_workers(&app);
    staff_party(&mut app, party, floor as f32 - SHORTFALL);

    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::PartyTooSmall {
            workers: floor - 1,
            required: floor,
        }],
        "a party a tenth of a person short of the floor is a party one worker short of it"
    );
}

// -------------------------------------------------------------------------------------------
// The reachability gate — always a pair
// -------------------------------------------------------------------------------------------

/// **The pair.** One world, one difference: whether the ground between the site and the home band
/// has been mapped. Written as a single test so the two halves cannot drift apart — a gate that
/// always allows fails the second half, and one that always refuses fails the first.
#[test]
fn the_gate_admits_a_mapped_corridor_and_refuses_an_unmapped_one() {
    // --- admitted: the whole corridor is mapped -------------------------------------------------
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    assert!(
        found_band_from_expedition(&mut app.world, party).is_ok(),
        "a site joined to a resident band by mapped land can be pointed at from home"
    );

    // --- refused: the same site, with the ground between it and home unmapped -------------------
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    // The faction knows where it is standing and where the party is, and nothing in between.
    set_discovered(&mut app, faction, &[home_pos, site]);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::Unreachable],
        "a party that walked into the unknown and stopped is a party you have lost track of"
    );
    assert!(
        app.world.get::<Expedition>(party).is_some(),
        "the refused party is still an expedition, and recall still works"
    );
}

/// **An expedition is not an anchor.** Two parties standing on mapped ground with no resident band
/// reachable would otherwise bootstrap a colony out of ground neither has reported.
#[test]
fn another_party_cannot_anchor_a_founding() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    let neighbor = corridor[corridor.len() - 2];
    // Mapped: the two tiles the parties stand on, and nothing joining them to home.
    set_discovered(&mut app, faction, &[site, neighbor]);
    spawn_party(
        &mut app,
        home,
        BYSTANDER_BAND_ID,
        neighbor,
        ExpeditionPhase::AwaitingOrders,
    );
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );

    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::Unreachable],
        "a party may not anchor a path to another party"
    );
}

/// **Water does not bridge two mapped land masses.** Discovery says nothing about walkability, so
/// the gate's grid is the conjunction of the two — a mapped strait is still a strait.
///
/// Driven against the pure predicate with a hand-built grid, because "two land masses separated by
/// exactly one column of sea" is a statement about the rule rather than about any seeded map.
#[test]
fn a_mapped_strait_does_not_join_two_shores() {
    const WIDTH: u32 = 5;
    const HEIGHT: u32 = 1;
    const STRAIT_COLUMN: u32 = 2;
    let bridged: Vec<bool> = (0..WIDTH).map(|_| true).collect();
    let split: Vec<bool> = (0..WIDTH).map(|x| x != STRAIT_COLUMN).collect();
    let anchors = HashSet::from([UVec2::new(0, 0)]);
    let site = UVec2::new(WIDTH - 1, 0);

    assert!(
        founding_site_is_reachable(site, &anchors, &bridged, WIDTH, HEIGHT, false),
        "the same shores joined by land are reachable — the control for the assertion below"
    );
    assert!(
        !founding_site_is_reachable(site, &anchors, &split, WIDTH, HEIGHT, false),
        "a path across a mapped strait would qualify a colony nobody can walk to"
    );
}

/// **The site itself must be mapped.** An expedition is excluded from live fog reveal, so a party's
/// own tile is not discovered unless the faction saw it some other way — and that is the rule doing
/// its job, not a hole in it.
#[test]
fn an_unmapped_site_is_refused_even_beside_home() {
    const WIDTH: u32 = 3;
    const HEIGHT: u32 = 1;
    let anchors = HashSet::from([UVec2::new(0, 0)]);
    let site = UVec2::new(1, 0);
    let all_mapped = vec![true; WIDTH as usize];
    let site_unmapped = vec![true, false, true];

    assert!(
        founding_site_is_reachable(site, &anchors, &all_mapped, WIDTH, HEIGHT, false),
        "the control: the tile next door, mapped, is reachable"
    );
    assert!(
        !founding_site_is_reachable(site, &anchors, &site_unmapped, WIDTH, HEIGHT, false),
        "an unmapped site cannot be named, however close to home it is"
    );
}

// -------------------------------------------------------------------------------------------
// Membership in the `With<ResidentBand>` systems, and persistence
// -------------------------------------------------------------------------------------------

/// **The founded band joins the resident systems on its founding turn.** `simulate_population` runs
/// `With<ResidentBand>` and ages every band it sees before anything else it does, so an aged cohort
/// is the most direct evidence of membership there is: an expedition's `age_turns` never moves.
#[test]
fn a_founded_band_is_simulated_as_a_band_on_the_next_turn() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    found_band_from_expedition(&mut app.world, party).expect("the founding is admitted");

    run_turn(&mut app);

    let cohort = app
        .world
        .get::<PopulationCohort>(party)
        .expect("the founded band survived the turn");
    assert_eq!(
        cohort.age_turns, 1,
        "`simulate_population` ages resident bands and only resident bands"
    );
}

/// **The settlers are their own people, not the locals.** A colony's culture layer is attached at
/// founding time and seeded from the **home band**, then parented on the province it landed in — so
/// it opens as the band that sent it and chases the locals from there, exactly as a band that walked
/// the same distance does (`CultureManager::set_band_parent`).
///
/// **The vacuity guard is the point of the fixture.** On a fresh world every layer is still the
/// baseline, so a colony seeded from its parent and one seeded from the destination province land on
/// identical numbers and any assertion here passes for free. The parent is therefore pushed a full
/// unit clear of the destination province first, and the gap between the two is asserted before the
/// colony is looked at.
#[test]
fn a_founded_colony_opens_with_its_parent_bands_culture() {
    /// The axis the fixture reads; the seeding is per-axis, so any one of them tells the story.
    const AXIS: usize = 0;
    /// How far the parent is staged from the baseline every other layer still sits on.
    const PARENT_MARKER: f32 = 1.5;
    /// The gap the fixture insists on between the parent and the destination province, below which
    /// the two candidate seeds are indistinguishable and the test proves nothing.
    const MIN_STAGED_GAP: f32 = 1.0;

    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);

    let parent_band = *app
        .world
        .get::<BandId>(home)
        .expect("the home band carries an id");
    {
        let mut manager = app.world.resource_mut::<CultureManager>();
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(parent_band))
            .expect("worldgen's bands own culture layers after the first update");
        for idx in 0..CULTURE_TRAIT_AXES {
            layer
                .traits
                .update_value(idx, core_sim::scalar_from_f32(PARENT_MARKER));
        }
    }

    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    found_band_from_expedition(&mut app.world, party).expect("the founding is admitted");

    let destination_region = culture_region_at(app.world.get_resource::<ProvinceMap>(), site);
    let manager = app.world.resource::<CultureManager>();
    let region_layer = manager
        .regional_layers()
        .find(|layer| layer.owner == CultureOwner::from_region(destination_region))
        .expect("the province the colony landed in owns a regional layer");
    let region_value = region_layer.traits.values()[AXIS].to_f32();
    assert!(
        (PARENT_MARKER - region_value).abs() > MIN_STAGED_GAP,
        "the fixture must stage a real divergence between the parent band ({PARENT_MARKER}) and \
         the destination province ({region_value}), or both seeds give the same answer"
    );

    let colony = manager
        .band_layer_by_owner(CultureOwner::from_band(BandId(PARTY_BAND_ID)))
        .expect(
            "the colony owns a culture layer the moment it is founded — waiting for the next \
             reconcile is what seeded it from the province it landed in",
        );
    let opened = colony.traits.values()[AXIS].to_f32();
    assert!(
        (opened - PARENT_MARKER).abs() < 1e-3,
        "the colony opens as the band that sent it ({PARENT_MARKER}), got {opened}"
    );
    assert!(
        (opened - region_value).abs() > MIN_STAGED_GAP,
        "…and not as the province it landed in ({region_value})"
    );
    assert_eq!(
        colony.parent,
        Some(region_layer.id),
        "the colony is parented on the province it was founded in, so it lags toward the locals \
         rather than snapping to them"
    );
}

/// **A mid-game founding survives a rollback as a band.** The checkpoint captures `ResidentBand` by
/// *presence* rather than by origin, so a band worldgen never made restores as one — and nothing
/// re-attaches `Expedition`, because the record no longer carries one.
#[test]
fn a_founding_survives_a_rollback() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    found_band_from_expedition(&mut app.world, party).expect("the founding is admitted");

    let checkpoint = capture_sim_state(&app.world);
    run_turn(&mut app);
    restore_sim_state(&mut app.world, &checkpoint);

    // The restore renumbers entities, so the band is found by the identity that survives: its id.
    let restored = entity_for_band(&mut app, PARTY_BAND_ID);
    assert!(
        app.world.get::<ResidentBand>(restored).is_some(),
        "a founded band comes back a band"
    );
    assert!(
        app.world.get::<Expedition>(restored).is_none(),
        "nothing re-attaches `Expedition` to a band whose record carries none"
    );
    assert!(
        app.world
            .get::<DemographicFlowAccumulator>(restored)
            .is_some(),
        "the carry is checkpoint state, so the restored band can still report births and deaths"
    );
    assert_eq!(
        app.world
            .get::<PopulationCohort>(restored)
            .map(|cohort| cohort.age_turns),
        Some(0),
        "the restore puts the band back at the age it was checkpointed at"
    );
}

// -------------------------------------------------------------------------------------------
// The refusal SET — the sim reports every applicable reason, not the first one
// -------------------------------------------------------------------------------------------

/// A party of one — the playtest case. Well under any shipped floor, and small enough that no
/// retune of `settle.min_founding_workers` above `1` makes this fixture stop testing the floor.
const LONE_WORKER: f32 = 1.0;

/// **The shipped founding floor.** Restated here rather than read from config on purpose: this file
/// asserts the number reaches the *client*, and a wire test that reads its expectation out of the
/// same config the capture reads would pass against a field that shipped a zero.
const SHIPPED_MIN_FOUNDING_WORKERS: u32 = 4;

/// **Both reasons at once, and this is the whole point of the refusal set.** A party of one standing
/// on ground nobody has mapped fails *both* eligibility gates, and each has to be fixed before the
/// founding is legal — so telling the player one of them means they fix it, press again, and learn
/// the second rule from a second refusal.
#[test]
fn a_small_party_on_unmapped_ground_is_refused_for_both_reasons() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    // The faction knows where it stands and where the party is, and nothing joining the two.
    set_discovered(&mut app, faction, &[home_pos, site]);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    let floor = min_founding_workers(&app);
    staff_party(&mut app, party, LONE_WORKER);

    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![
            FoundingRefusal::PartyTooSmall {
                workers: LONE_WORKER as u32,
                required: floor,
            },
            FoundingRefusal::Unreachable,
        ],
        "both eligibility gates hold, so both are reported — party size first, because it is the \
         O(1) one and it reads first"
    );
}

/// **One reason when only one holds — as a PAIR**, so "always report both" cannot pass. Same two
/// gates, each isolated by making the other pass.
#[test]
fn each_eligibility_gate_is_reported_alone_when_it_is_the_only_one_that_holds() {
    // --- a full-size party on unmapped ground: reachability alone -------------------------------
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &[home_pos, site]);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    // `spawn_party` staffs at exactly the floor, so the party gate passes.
    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::Unreachable],
        "a party that clears the floor is refused for the ground alone"
    );

    // --- a small party on mapped ground: the floor alone ----------------------------------------
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    let floor = min_founding_workers(&app);
    staff_party(&mut app, party, LONE_WORKER);
    assert_eq!(
        refusals_of(found_band_from_expedition(&mut app.world, party)),
        vec![FoundingRefusal::PartyTooSmall {
            workers: LONE_WORKER as u32,
            required: floor,
        }],
        "a party standing on ground it can be pointed at is refused for its size alone"
    );
}

/// **The empty set is the "you may found" signal, and it has to be reachable.** A legal founding
/// reports no refusals at all — otherwise the client's affordance could never enable.
#[test]
fn a_legal_founding_reports_no_refusals() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    let corridor = land_corridor(&app, home_pos);
    let site = *corridor.last().expect("corridor has a tail");
    set_discovered(&mut app, faction, &corridor);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );

    assert!(
        founding_refusals(&mut app.world, party).is_empty(),
        "a founding that would succeed is refused for nothing"
    );
    assert!(
        found_band_from_expedition(&mut app.world, party).is_ok(),
        "…and the command it gates agrees, which is the only thing that makes the empty set \
         readable as permission"
    );
}

// -------------------------------------------------------------------------------------------
// The wire — the verdict a client actually reads
// -------------------------------------------------------------------------------------------

/// **The PUBLISHED verdict is the verdict.** Run a turn so `assess_foundings` writes it, encode the
/// frame, and check the party's exported `foundingRefusals` carries exactly the tokens
/// `found_band_from_expedition` refuses with — plus the floor the tooltip has to name.
///
/// Asserted on the encoded frame rather than on the component, because a field that never reached
/// the codec still passes an in-process assertion.
#[test]
fn the_published_refusals_are_the_tokens_the_command_refuses_with() {
    let mut app = spawn_world();
    let (home, faction, home_pos) = home_band(&mut app);
    // Ground the faction has never seen, rather than a wiped ledger: this fixture advances a turn,
    // and a turn rebuilds the faction map (see `farthest_undiscovered_land`).
    let site = farthest_undiscovered_land(&app, faction, home_pos);
    let party = spawn_party(
        &mut app,
        home,
        PARTY_BAND_ID,
        site,
        ExpeditionPhase::AwaitingOrders,
    );
    staff_party(&mut app, party, LONE_WORKER);
    run_turn(&mut app);

    let (published, floor) = published_founding_verdict(&mut app, PARTY_BAND_ID);
    assert_eq!(
        floor, SHIPPED_MIN_FOUNDING_WORKERS,
        "the tooltip has to be able to name the number, so the shipped floor rides every cohort"
    );

    // The party has not moved and the map has not changed, so the command must reach the same
    // verdict the turn published.
    let party = entity_for_band(&mut app, PARTY_BAND_ID);
    let refused_with: Vec<String> = refusals_of(found_band_from_expedition(&mut app.world, party))
        .into_iter()
        .map(|refusal| refusal.token().to_string())
        .collect();
    assert_eq!(
        published, refused_with,
        "the button's reason and the command's refusal are one assessment, in one order — a client \
         that greys the button out must be able to say exactly why it will fail"
    );
    assert_eq!(
        published,
        vec!["party_too_small".to_string(), "unreachable".to_string()],
        "…and that assessment is both gates, on the wire, for a lone party on unmapped ground"
    );
}

/// **A resident band publishes an empty set**, so a client reading the field on a normal band is
/// never shown a phantom refusal. The field speaks only to a party the arrival affordance is
/// offered for, and a band is not one.
#[test]
fn a_resident_band_publishes_no_founding_refusals() {
    let mut app = spawn_world();
    let (home, _, _) = home_band(&mut app);
    let home_id = app
        .world
        .get::<BandId>(home)
        .copied()
        .expect("a worldgen band carries an id");
    run_turn(&mut app);

    let (published, floor) = published_founding_verdict(&mut app, home_id.0);
    assert!(
        published.is_empty(),
        "a resident band has nothing to found, so it carries no refusal — got {published:?}"
    );
    assert_eq!(
        floor, SHIPPED_MIN_FOUNDING_WORKERS,
        "the floor rides EVERY cohort, because the outfit and arrival readouts both need it"
    );
}
