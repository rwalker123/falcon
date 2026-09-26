//! **How a provisioned party feeds itself on the march** — the replenish arm of
//! `advance_expeditions`, and the order it runs in.
//!
//! A scout or a shipment walks out with a larder and drains it every turn. When it falls below the
//! low-water mark (`party × provision_upkeep_per_worker × replenish.low_turns`) it tops itself up
//! off the ground it is standing on, and it does so in ONE ORDER: **gather, then hunt**.
//!
//! # ⛔ THE ORDER IS THE MODEL, AND THESE TESTS ARE WHAT PINS IT
//!
//! Gathering costs no lives, no animals and no weapon wear; a kill costs casualties, spears and a
//! herd. So the party exhausts the safe option before it picks a fight — never a score between the
//! two, which would let a fat herd outbid a stand the party could have stripped for nothing.
//!
//! The four pins here are: a party the stand can fill does not hunt; a party with no stand in reach
//! does; a starving party beside an **inedible** herd does not kill it (issue #373); and the gather
//! rate is really kit-resolved, so the `ranging` kit's baskets are not decoration.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_expeditions, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, BandEquipment, BandId, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, Expedition, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionMission, ExpeditionPhase, FactionId, FactionInventory, FaunaConfigHandle,
    FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, KitChoice, KitJob, LaborAllocation, LaborConfigHandle, LadderConfigHandle,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, VisibilityConfig, VisibilityConfigHandle, VisibilityLedger,
    WellbeingConfigHandle, FOOD,
};

/// The party every fixture fields. Four is the reference party the expedition arc quotes.
const PARTY_WORKERS: u32 = 4;

/// The `BandId` the fixture home band carries — one is enough, no test here fields two.
const FIXTURE_BAND_ID: u64 = 1;

/// **The species that is not food** — `fauna_config.json`'s wolf declares
/// `provisions_per_biomass: 0.0`, which is what makes `HuntYield::edible()` false for it. Stamped on
/// a live herd by the #373 fixture, so the guard is tested against the roster's real inedible row
/// rather than an invented one.
const INEDIBLE_SPECIES: &str = "Grey Wolf Pack";

/// **The gap the stand is asked to close**, in provisions — deliberately small, so the gather is
/// bounded by the party's ROOM rather than by its arms and lands the party exactly on the low-water
/// mark. A gap wider than one turn's gathering would leave the party short and it would (correctly)
/// go on to hunt, which is the *other* test.
const TOP_UP_GAP: f32 = 0.05;

/// A world with terrain, herds, patches and every config `advance_expeditions` reads — the same
/// shape `expedition_hunt.rs` builds, kept local because these fixtures seed the plant web too.
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
    // The retreat stage held at its identity, so a roadside kill either happens or does not for
    // reasons this file states — see `FaunaConfig::without_retreat`.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
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

/// The low-water mark this party replenishes at — the same expression the system evaluates.
fn low_buffer(app: &App) -> f32 {
    let cfg = expedition_config(app);
    PARTY_WORKERS as f32 * cfg.provision_upkeep_per_worker * cfg.replenish.low_turns as f32
}

/// One turn's provisions upkeep for the fixture party.
fn upkeep(app: &App) -> f32 {
    PARTY_WORKERS as f32 * expedition_config(app).provision_upkeep_per_worker
}

fn tile_at(app: &App, pos: UVec2) -> bevy::prelude::Entity {
    app.world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("tile resolves")
}

/// **The stand the fixtures gather from** — the lowest-coordinate patch whose tile has a live
/// gathering season, topped up to its own carrying capacity.
///
/// The season matters and is asserted rather than assumed: a tile with no food module offers
/// `NO_FORAGE_SEASON`, and a fixture that happened to pick one would show "no gather" for a reason
/// that has nothing to do with what is being tested.
fn richest_stand(app: &mut App) -> UVec2 {
    let mut candidates: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .filter(|tile| {
            app.world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .and_then(|entity| app.world.get::<FoodModuleTag>(entity))
                .is_some_and(|module| module.seasonal_weight > 0.0)
        })
        .collect();
    candidates.sort_by_key(|tile| (tile.y, tile.x));
    let tile = *candidates
        .first()
        .expect("the harness world seeds at least one in-season forage patch");
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(tile).expect("the stand was just found");
    patch.biomass = patch.carrying_capacity;
    tile
}

fn stand_biomass(app: &App, tile: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(tile)
        .expect("the stand is live")
        .biomass
}

/// Move the **lightest edible** herd onto `pos`, so a roadside kill is never blocked by the
/// quantum: a party with room for a fraction of a mammoth can still take a whole rabbit.
/// Returns the herd's id.
fn edible_herd_at(app: &mut App, pos: UVec2) -> String {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .filter(|herd| fauna.hunt_yield_for(&herd.species).edible())
        .min_by(|a, b| {
            a.body_mass
                .partial_cmp(&b.body_mass)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
        .expect("the harness world seeds edible game");
    herd.current_pos = pos;
    herd.biomass = herd.carrying_capacity;
    herd.id.clone()
}

/// The same herd, restamped as the roster's **inedible** species — the #373 fixture.
fn make_inedible(app: &mut App, id: &str) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
        .expect("the herd is live");
    herd.species = INEDIBLE_SPECIES.to_string();
}

fn herd_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the herd is live")
        .biomass
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
        faction: FactionId(0),
        knowledge: Vec::new(),
        migration: None,
    }
}

/// A home band a third of the map away, so nothing in these fixtures is a comm-range flush or a
/// fold-back.
fn spawn_home_band(app: &mut App, party_pos: UVec2) -> bevy::prelude::Entity {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
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
            BandEquipment::start_stocked(&core_sim::EquipmentConfig::builtin()),
        ))
        .id()
}

/// The kit a launch resolves when the player names none — `equipment.json`'s `ranging`.
fn ranging_kit(app: &App) -> KitChoice {
    app.world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get()
        .default_kit(KitJob::Expedition)
}

/// The empty roster entry — a party that carries nothing at all.
fn bare_kit(app: &App) -> KitChoice {
    app.world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get()
        .kit("none")
        .expect("the roster ships the empty kit")
}

/// A provisioned (scout) party standing at `pos` with `provisions` in its larder, awaiting orders —
/// the phase a party sits in while the replenish arm runs.
fn spawn_scout_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    provisions: f32,
    kit: KitChoice,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    let mut party = cohort(tile, PARTY_WORKERS);
    if provisions > 0.0 {
        party.stores.add(FOOD, scalar_from_f32(provisions));
    }
    let equipment = {
        let config = app
            .world
            .resource::<core_sim::EquipmentConfigHandle>()
            .get();
        BandEquipment::start_stocked_for(&config, PARTY_WORKERS as f32)
    };
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

fn carried(app: &App, party: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(party)
        .expect("the party is alive")
        .stores
        .get(FOOD)
        .to_f32()
}

/// **A party the stand can fill GATHERS AND DOES NOT HUNT.**
///
/// The order is the whole model: a stand costs nothing to work, so a party that can close its gap
/// off the ground never spends casualties, spears and animals to close it. The herd is standing on
/// the party's own tile — well inside `replenish.reach_tiles` — so "it did not hunt" is a decision
/// here and not an absence of opportunity.
#[test]
fn a_party_the_stand_can_fill_gathers_and_does_not_hunt() {
    let mut app = spawn_world();
    let stand = richest_stand(&mut app);
    let herd = edible_herd_at(&mut app, stand);
    let home = spawn_home_band(&mut app, stand);
    // Just under the mark: the gap is small enough for one turn's gathering to close, so the hunt
    // arm is reached only if the gather did not happen at all.
    let opening = upkeep(&app) + low_buffer(&app) - TOP_UP_GAP;
    let kit = ranging_kit(&app);
    let party = spawn_scout_party(&mut app, home, stand, opening, kit);

    let stand_before = stand_biomass(&app, stand);
    let herd_before = herd_biomass(&app, &herd);
    app.world.run_system_once(advance_expeditions);

    assert!(
        stand_biomass(&app, stand) < stand_before,
        "the party must have gathered: the stand stood at {stand_before} and still does"
    );
    assert_eq!(
        herd_biomass(&app, &herd),
        herd_before,
        "a party the stand could fill must not have hunted — the kill costs lives, spears and \
         animals, and the gather cost none of them"
    );
    assert_eq!(
        carried(&app, party),
        low_buffer(&app),
        "the gather is capped at the low-water mark, so the party lands exactly on it"
    );
}

/// **A party with NO stand in reach still hunts** — the same fixture as above with the plant web
/// emptied, so the two differ in exactly one fact.
///
/// Without this the test above proves nothing: a party that never hunts under any circumstances
/// would pass it.
#[test]
fn a_party_with_no_stand_in_reach_still_hunts() {
    let mut app = spawn_world();
    let stand = richest_stand(&mut app);
    let herd = edible_herd_at(&mut app, stand);
    let home = spawn_home_band(&mut app, stand);
    let opening = upkeep(&app) + low_buffer(&app) - TOP_UP_GAP;
    let kit = ranging_kit(&app);
    let party = spawn_scout_party(&mut app, home, stand, opening, kit);
    // The one difference from the pair above: there is nothing standing here to gather.
    app.world.resource_mut::<ForageRegistry>().patches.clear();

    let herd_before = herd_biomass(&app, &herd);
    app.world.run_system_once(advance_expeditions);

    assert!(
        herd_biomass(&app, &herd) < herd_before,
        "with no stand in reach the party must fall through to the roadside kill: the herd stood \
         at {herd_before} and still does"
    );
    assert!(
        carried(&app, party) > low_buffer(&app) - TOP_UP_GAP,
        "and the kill must have fed it"
    );
}

/// **A STARVING PARTY BESIDE AN INEDIBLE HERD DOES NOT KILL IT** (issue #373).
///
/// The replenish arm is triggered by the FOOD low-water mark and exists to feed the party, so a
/// quarry that pays no provisions cannot answer the question that was asked. The party used to
/// spend casualties, spears and a whole herd to bank pelts and then walk on still starving.
/// Materials are a byproduct of a food take here and never the reason for one — the hunt verb
/// remains the way to go after pelts deliberately.
#[test]
fn a_starving_party_does_not_kill_an_inedible_herd() {
    let mut app = spawn_world();
    let stand = richest_stand(&mut app);
    let herd = edible_herd_at(&mut app, stand);
    make_inedible(&mut app, &herd);
    let home = spawn_home_band(&mut app, stand);
    // Empty larder — as hungry as a party gets — and nothing to gather, so the herd is the only
    // thing in reach that could feed it, and it cannot.
    let kit = ranging_kit(&app);
    let party = spawn_scout_party(&mut app, home, stand, 0.0, kit);
    app.world.resource_mut::<ForageRegistry>().patches.clear();

    let herd_before = herd_biomass(&app, &herd);
    app.world.run_system_once(advance_expeditions);

    assert_eq!(
        herd_biomass(&app, &herd),
        herd_before,
        "a starving party must leave an inedible herd standing — killing it feeds nobody"
    );
    assert_eq!(
        carried(&app, party),
        0.0,
        "and it is still starving, which is the honest outcome"
    );
}

/// **THE GATHER RATE IS KIT-RESOLVED, so the ranging kit's baskets are not decoration.**
///
/// Two identical starving parties on the same stand, differing only in what they carry: the
/// `ranging` kit steps the per-gatherer throughput up from the bare-handed baseline
/// (`labor_config.json`'s `forage.per_worker_biomass_capacity`) to the baskets' own tier. Both are
/// bounded by their ARMS rather than by their room here — the larder is empty, so the room is the
/// whole low-water buffer — which is the precondition for the two rates to be visible at all.
///
/// The herds are cleared, so every provision either party ends the turn with came off the stand.
#[test]
fn the_ranging_kit_gathers_more_than_bare_hands() {
    let gathered = |kit_of: fn(&App) -> KitChoice| -> f32 {
        let mut app = spawn_world();
        let stand = richest_stand(&mut app);
        let home = spawn_home_band(&mut app, stand);
        let kit = kit_of(&app);
        let party = spawn_scout_party(&mut app, home, stand, 0.0, kit);
        app.world.resource_mut::<HerdRegistry>().herds.clear();
        app.world.run_system_once(advance_expeditions);
        carried(&app, party)
    };

    let with_baskets = gathered(ranging_kit);
    let bare_handed = gathered(bare_kit);

    assert!(
        bare_handed > 0.0,
        "PRECONDITION: a bare-handed party must still gather something, or this is 0 against 0"
    );
    assert!(
        with_baskets > bare_handed,
        "the ranging kit must gather more than bare hands: {with_baskets} against {bare_handed}"
    );
    assert!(
        with_baskets < low_buffer(&spawn_world()),
        "PRECONDITION: both parties must be bounded by their ARMS, not by the room to the \
         low-water mark — otherwise the two kits would be capped at the same number"
    );
}
