//! **What a detached party sees, and what looking costs it.**
//!
//! A ranging party's per-turn observation radius used to be the flat
//! `expedition_config.observe_sight_range` with no kit term at all, while the *resident* band's
//! posted vantage had been kit-aware for a long time. This file pins the correction:
//!
//! - `observe_sight_range` is the **equipped** tier (`9`), and the `wayfinding` item declares the
//!   **bare** one (`6` — today's shipped flat radius, so nothing regresses for a bare party);
//! - the party wears that gear on `WearQuantum::TileRevealed`, charged **at observe time** and only
//!   for ground that is genuinely new.
//!
//! # ⛔ THE TURN-CLOCK GUARD IS THE POINT OF THIS FILE
//!
//! A party buffers its observations and flushes them to the faction map when it comes home, so
//! charging per *buffered* tile would bill a party walking back over ground it already mapped — a
//! turn clock wearing a per-use costume, which `docs/plan_denial_raid.md` §1.2 forbids outright.
//! `a_party_re_walking_mapped_ground_wears_nothing` is the assertion that keeps that shut, and
//! `a_party_mapping_new_ground_wears_the_kit_down` is its liveness half: a charge that never fires
//! would pass the first test on its own.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_expeditions, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, BandEquipment, BandId, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, EquipmentConfig, EquipmentConfigHandle, Expedition,
    ExpeditionConfig, ExpeditionConfigHandle, ExpeditionMission, ExpeditionPhase, FactionId,
    FactionInventory, FaunaConfigHandle, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, KitChoice, KitJob, LaborAllocation,
    LaborConfigHandle, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause,
    PopulationCohort, ResidentBand, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, VisibilityConfig,
    VisibilityConfigHandle, VisibilityLedger,
};

/// The party every fixture fields — the reference party the expedition arc quotes.
const PARTY_WORKERS: u32 = 4;

/// The `BandId` the fixture home band carries.
const FIXTURE_BAND_ID: u64 = 1;

/// The one item on the roster that lifts a party's reach, and the one these fixtures read the
/// durability of.
const WAYFINDING: &str = "wayfinding";

/// **The faction every fixture cohort belongs to** — one people, so a "already on the faction map"
/// question has exactly one map to ask.
const FIXTURE_FACTION: FactionId = FactionId(0);

/// Provisions the party opens with — comfortably above the replenish low-water mark, so nothing
/// here gathers or hunts and the only gear that moves is the wayfinding kit.
const WELL_FED: f32 = 100.0;

/// A world with terrain, herds, patches and every config `advance_expeditions` reads — the same
/// shape `expedition_replenish.rs` builds.
fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world
        .insert_resource(core_sim::WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    // **The fixture declares the gear it needs** — the shipped `start_stock_fraction` is `0.0`, so
    // without this the party would carry no wayfinding kit and every wear assertion below would be
    // vacuously true.
    app.world
        .insert_resource(EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world
        .insert_resource(core_sim::ExtractionConfigHandle::default());
    // An empty deposit registry is the shipped turn-1 state: a working is opened the
    // first turn a crew stands on it, so a harness with no `extract` row has none.
    app.world
        .insert_resource(core_sim::extraction::DepositRegistry::default());
    app.world.insert_resource(ExpeditionConfigHandle::default());
    app.world
        .insert_resource(VisibilityConfigHandle::new(VisibilityConfig::builtin()));
    app.world.insert_resource(VisibilityLedger::default());
    app.world
        .insert_resource(core_sim::ContactsThisTurn::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);
    app
}

fn expedition_config(app: &App) -> Arc<ExpeditionConfig> {
    app.world.resource::<ExpeditionConfigHandle>().get()
}

fn equipment_config(app: &App) -> Arc<EquipmentConfig> {
    app.world.resource::<EquipmentConfigHandle>().get()
}

fn map_size(app: &App) -> (u32, u32) {
    let registry = app.world.resource::<TileRegistry>();
    (registry.width, registry.height)
}

/// **The middle of the map** — far enough from the north and south edges that a radius-9 disc is
/// never clipped, so the two ranges differ for the reason under test and not because one of them
/// ran off the grid.
fn map_centre(app: &App) -> UVec2 {
    let (width, height) = map_size(app);
    UVec2::new(width / 2, height / 2)
}

fn tile_at(app: &App, pos: UVec2) -> bevy::prelude::Entity {
    app.world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("tile resolves")
}

fn cohort(tile: bevy::prelude::Entity, working: u32) -> PopulationCohort {
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
        faction: FIXTURE_FACTION,
        knowledge: Vec::new(),
        migration: None,
    }
}

/// A home band a third of the map away, so no fixture here is a comm-range flush or a fold-back:
/// what the party buffers stays buffered, and the faction map moves only where a test moves it.
fn spawn_home_band(app: &mut App, party_pos: UVec2) -> bevy::prelude::Entity {
    let (width, height) = map_size(app);
    let far = UVec2::new(
        (party_pos.x + width / 3) % width,
        (party_pos.y + height / 3) % height,
    );
    let tile = tile_at(app, far);
    app.world
        .spawn((
            cohort(tile, 10),
            ResidentBand,
            BandId(FIXTURE_BAND_ID),
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
        ))
        .id()
}

/// The kit a launch resolves when the player names none — `equipment.json`'s `ranging`, which
/// carries the wayfinding item.
fn ranging_kit(app: &App) -> KitChoice {
    equipment_config(app).default_kit(KitJob::Expedition)
}

/// The empty roster entry — a party that carries nothing at all.
fn bare_kit(app: &App) -> KitChoice {
    equipment_config(app)
        .kit("none")
        .expect("the roster ships the empty kit")
}

fn spawn_scout_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    kit: KitChoice,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    let mut party = cohort(tile, PARTY_WORKERS);
    party.stores.add(core_sim::FOOD, scalar_from_f32(WELL_FED));
    let equipment = BandEquipment::start_stocked_for(&equipment_config(app), PARTY_WORKERS as f32);
    app.world
        .spawn((
            party,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            equipment,
            Expedition {
                home_band,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::AwaitingOrders,
                announced: true,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit,
                cargo: LocalStore::new(),
            },
        ))
        .id()
}

fn buffered(app: &App, party: bevy::prelude::Entity) -> Vec<UVec2> {
    app.world
        .get::<Expedition>(party)
        .expect("the party is alive")
        .pending_reveal
        .clone()
}

/// How much of the wayfinding kit the party has left, in units of durability.
fn wayfinding_left(app: &App, party: bevy::prelude::Entity) -> f32 {
    let config = equipment_config(app);
    app.world
        .get::<BandEquipment>(party)
        .expect("the party carries a ledger")
        .remaining(WAYFINDING, &config)
}

/// The squared straight-line distance from the party's tile, wrapping in x exactly as the reveal
/// geometry does. Squared because that is the comparison the geometry itself makes — taking a root
/// here would only add a rounding question nobody needs.
fn dist_sq_from(centre: UVec2, pos: UVec2, width: u32) -> i64 {
    let raw = (pos.x as i64 - centre.x as i64).abs();
    let dx = raw.min(width as i64 - raw);
    let dy = pos.y as i64 - centre.y as i64;
    dx * dx + dy * dy
}

/// Mark the WHOLE grid as already known to the fixture faction — the "this party is walking home
/// over ground it already mapped" state, applied with the same `discover` promotion the comm flush
/// uses so the fixture cannot describe a state the sim never produces.
fn map_the_whole_world(app: &mut App) {
    let (width, height) = map_size(app);
    let turn = app.world.resource::<SimulationTick>().0;
    let mut ledger = app.world.resource_mut::<VisibilityLedger>();
    let map = ledger.ensure_faction(FIXTURE_FACTION, width, height);
    for y in 0..height {
        for x in 0..width {
            map.discover(x, y, turn);
        }
    }
}

/// **A party carrying the ranging kit observes at the EQUIPPED tier, a bare one at the item's own
/// bare reading** — `9` against `6`, straight off the resolution seam the system reads.
///
/// Asserted on the numbers as well as on the geometry below, because the geometry alone cannot say
/// *which* two numbers widened it.
#[test]
fn the_ranging_kit_buys_three_tiles_of_reach_and_a_bare_party_keeps_the_shipped_six() {
    let app = spawn_world();
    let equipment = equipment_config(&app);
    let equipped = expedition_config(&app).observe_sight_range as f32;
    let wear = BandEquipment::start_stocked_for(&equipment, PARTY_WORKERS as f32);

    assert_eq!(
        equipment.expedition_sight_range(equipped, &ranging_kit(&app), &wear),
        equipped,
        "a party carrying the wayfinding item resolves the EQUIPPED tier, which is \
         `expedition_config.observe_sight_range` itself"
    );
    assert_eq!(
        equipment.expedition_sight_range(equipped, &bare_kit(&app), &wear),
        6.0,
        "and a bare party resolves the item's own unequipped side — the flat radius this lever \
         used to be, so nothing regresses for a party carrying nothing"
    );
    assert!(
        equipped > 6.0,
        "the whole change is upside for carrying the gear: the equipped tier must be the wider \
         of the two, and it is {equipped}"
    );
}

/// **And the wider tier really reaches further ground.** The kitted party's buffer is a superset of
/// the bare party's — same tile, same map, same LOS — and reaches strictly past everything the bare
/// party could see.
///
/// The numbers above could both be right with the system still passing the flat lever to the reveal
/// geometry, which is exactly the bug this arc fixes; only the footprint says otherwise.
#[test]
fn a_kitted_party_maps_ground_a_bare_one_cannot_reach() {
    let (bare_tiles, kitted_tiles, centre, width) = {
        let mut app = spawn_world();
        let centre = map_centre(&app);
        let (width, _) = map_size(&app);
        let home = spawn_home_band(&mut app, centre);
        let bare = bare_kit(&app);
        let party = spawn_scout_party(&mut app, home, centre, bare);
        app.world.run_system_once(advance_expeditions);
        let bare_tiles = buffered(&app, party);

        let mut app = spawn_world();
        let home = spawn_home_band(&mut app, centre);
        let kit = ranging_kit(&app);
        let party = spawn_scout_party(&mut app, home, centre, kit);
        app.world.run_system_once(advance_expeditions);
        (bare_tiles, buffered(&app, party), centre, width)
    };

    assert!(
        !bare_tiles.is_empty(),
        "the bare party must have observed something at all, or this comparison is between two \
         empty sets"
    );
    for tile in &bare_tiles {
        assert!(
            kitted_tiles.contains(tile),
            "the wider reach must be a SUPERSET: {tile:?} was seen bare-handed and not with the kit"
        );
    }
    let bare_reach = bare_tiles
        .iter()
        .map(|tile| dist_sq_from(centre, *tile, width))
        .max()
        .expect("the bare buffer is non-empty");
    let kitted_reach = kitted_tiles
        .iter()
        .map(|tile| dist_sq_from(centre, *tile, width))
        .max()
        .expect("the kitted buffer is non-empty");
    assert!(
        kitted_reach > bare_reach,
        "the kit must reach past everything the bare party could see: bare stopped at {bare_reach} \
         (squared) and the kit at {kitted_reach}"
    );
}

/// # ⛔ THE TURN-CLOCK GUARD
///
/// **A party standing on ground its people have already mapped wears NOTHING.** Wear is charged per
/// *use*, and re-walking known country is not a use — a party that paid here would be paying for
/// the turn passing, in a per-use costume (`docs/plan_denial_raid.md` §1.2).
///
/// The liveness half is asserted in the same test: the party really did observe, and really is
/// carrying the gear, so "it wore nothing" is a decision rather than an absence of opportunity.
#[test]
fn a_party_re_walking_mapped_ground_wears_nothing() {
    let mut app = spawn_world();
    let centre = map_centre(&app);
    let home = spawn_home_band(&mut app, centre);
    let kit = ranging_kit(&app);
    let party = spawn_scout_party(&mut app, home, centre, kit);
    // The one difference from the test below: this people has already been everywhere.
    map_the_whole_world(&mut app);

    let before = wayfinding_left(&app, party);
    assert!(
        before > 0.0,
        "the fixture party must actually be carrying wayfinding gear, or this asserts nothing"
    );
    app.world.run_system_once(advance_expeditions);

    assert!(
        !buffered(&app, party).is_empty(),
        "the party must still have observed — the guard is about what looking COSTS, not about \
         whether it happened"
    );
    assert_eq!(
        wayfinding_left(&app, party),
        before,
        "every tile in reach was already on the faction map, so the party mapped nothing new and \
         must have spent nothing"
    );
}

/// **And a party mapping genuinely new ground DOES wear the kit down** — the liveness half of the
/// guard above, which would otherwise pass just as well if the charge never fired at all.
#[test]
fn a_party_mapping_new_ground_wears_the_kit_down() {
    let mut app = spawn_world();
    let centre = map_centre(&app);
    let home = spawn_home_band(&mut app, centre);
    let kit = ranging_kit(&app);
    let party = spawn_scout_party(&mut app, home, centre, kit);

    let before = wayfinding_left(&app, party);
    app.world.run_system_once(advance_expeditions);

    assert!(
        wayfinding_left(&app, party) < before,
        "the ground was unexplored, so the party mapped it for the first time and must have paid \
         for it: the kit stood at {before} and still does"
    );
}

/// **A bare party pays nothing, however much country it maps.** The charge is named by *quantum*
/// and resolved through the party's own kit, so a party holding no wayfinding gear cannot be billed
/// for wearing it — the same pairing every other wear site in the sim keeps.
#[test]
fn a_bare_party_maps_the_world_for_free() {
    let mut app = spawn_world();
    let centre = map_centre(&app);
    let home = spawn_home_band(&mut app, centre);
    let bare = bare_kit(&app);
    let party = spawn_scout_party(&mut app, home, centre, bare);

    let before = wayfinding_left(&app, party);
    app.world.run_system_once(advance_expeditions);

    assert!(
        !buffered(&app, party).is_empty(),
        "the bare party must have mapped new ground, or it had nothing to be charged for"
    );
    assert_eq!(
        wayfinding_left(&app, party),
        before,
        "the party's kit carries no wayfinding item, so nothing of it may be spent"
    );
}
