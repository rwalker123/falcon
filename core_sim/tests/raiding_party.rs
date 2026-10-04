//! A detached **raiding party** — the denial raid's party (`advance_expeditions`,
//! `ExpeditionPhase::Hunting`) — on the paths every party shares: it does not conclude a trip it has
//! not reached, it fights (bloodier than a resident band), an orphaned party folds back where it
//! stands, and it publishes the forecast horizon and its quarry's name on the wire.
//!
//! The denial raid's own completion and forecast are pinned in `denial_raid.rs`.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_band_movement, advance_expeditions, advance_herds, build_test_app,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, BandEquipment, BandId, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, Expedition, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionMission, ExpeditionPhase, FactionId, FactionInventory, FaunaConfigHandle,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborConfigHandle, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, SimulationConfig,
    SimulationTick, SnapshotHistory, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, VisibilityConfig, VisibilityConfigHandle, VisibilityLedger,
    WellbeingConfigHandle, FOOD,
};

/// Party size used by every fixture: 4 workers.
const PARTY_WORKERS: u32 = 4;

/// Mark the named herds' tiles visible to the viewer faction.
///
/// Herd display telemetry is **fog-filtered** (issue #264) — a herd on ground the viewer cannot see
/// is not published at all. The tests below pick herds off the registry by index rather than by
/// where the starting band happens to stand, so most of them are in the dark; they are about *what a
/// visible herd's exported readout says*, not about whether it is visible. Revealing the herd is the
/// in-game precondition for reading that panel at all (a band works or scouts within sight of it),
/// so the fixture states it explicitly rather than blanketing the map.
fn reveal_herds(app: &mut App, ids: &[String]) {
    let positions: Vec<UVec2> = {
        let registry = app.world.resource::<HerdRegistry>();
        ids.iter()
            .filter_map(|id| registry.find(id).map(|herd| herd.position()))
            .collect()
    };
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    for pos in positions {
        map.mark_active(pos.x, pos.y, 0);
    }
}

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
    // **This harness is a deterministic pin, so the retreat stage is held at its identity.**
    // Slice 7 authored a non-zero `combat.wariness` across the roster
    // (`docs/plan_hunt_through_combat.md` §3.1); `FaunaConfig::without_retreat` carries the whole
    // reasoning for why the pre-existing suite neutralises it rather than re-baselining.
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
    // Belief on a place — a hunt or a raid credits its dead to the tile the band stands on.
    app.world
        .insert_resource(core_sim::BeliefRegistry::default());
    app.world
        .insert_resource(core_sim::BeliefConfigHandle::default());
    app.world.insert_resource(ExpeditionConfigHandle::default());
    app.world
        .insert_resource(VisibilityConfigHandle::new(VisibilityConfig::builtin()));
    app.world.insert_resource(VisibilityLedger::default());
    app.world
        .insert_resource(core_sim::ContactsThisTurn::default());
    // What each party saw this turn — `advance_expeditions` rebuilds it every run.
    app.world
        .insert_resource(core_sim::PartySightings::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);
    app
}

fn expedition_config(app: &App) -> Arc<ExpeditionConfig> {
    app.world.resource::<ExpeditionConfigHandle>().get()
}

/// A stationary wild-game group (`route_len == 1` → it stays on its anchor), so a test party stays
/// in reach across turns without running `advance_band_movement`.
fn stationary_game_herd(app: &App) -> String {
    let registry = app.world.resource::<HerdRegistry>();
    registry
        .herds
        .iter()
        .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
        .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
        .map(|h| h.id.clone())
        .expect("expected at least one short-range game group")
}

/// Seed a herd's biomass as a fraction of its carrying capacity; returns `(position, biomass, cap)`.
fn seed_herd(app: &mut App, id: &str, cap_fraction: f32) -> (UVec2, f32, f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("herd present");
    herd.biomass = herd.carrying_capacity * cap_fraction;
    (herd.position(), herd.biomass, herd.carrying_capacity)
}

fn herd_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("herd present")
        .biomass
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
        last_food_need: 0.0,
        last_food_spoiled: 0.0,
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
    }
}

/// The `BandId` every fixture band in this file carries. One id is enough: no test here fields two
/// resident bands at once, and a named constant beats a literal threaded through four spawn helpers.
const FIXTURE_BAND_ID: u64 = 1;

/// A home band far from the herd (so no comm flush interferes).
fn spawn_home_band(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let width = app.world.resource::<TileRegistry>().width;
    let height = app.world.resource::<TileRegistry>().height;
    let far = UVec2::new(
        (herd_pos.x + width / 3) % width,
        (herd_pos.y + height / 3) % height,
    );
    let tile = tile_at(app, far);
    app.world
        .spawn((
            cohort(tile, 10),
            ResidentBand,
            // Addressable + kitted, like every real band.
            BandId(FIXTURE_BAND_ID),
            BandEquipment::start_stocked(&core_sim::EquipmentConfig::builtin()),
        ))
        .id()
}

/// The species display name `send_denial_raid` stamps on a mission — resolved off the registry
/// exactly as [`outfit_raiding_party`] does, so a directly-spawned party fixture is indistinguishable
/// from a launched one by the name of its quarry. **Display-only**: every raid mechanic resolves the
/// herd through `fauna_id`, so nothing branches on this string; what it must not be is silently empty
/// where production always has a name.
fn target_species_of(app: &App, fauna_id: &str) -> String {
    app.world
        .resource::<HerdRegistry>()
        .find(fauna_id)
        .map(|herd| herd.species.clone())
        .unwrap_or_default()
}

/// A `PARTY_WORKERS`-strong denial-raid party at `pos`, already in the `Hunting` phase (as
/// `send_denial_raid` spawns it).
fn spawn_raid_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    let target_species = target_species_of(app, fauna_id);
    app.world
        .spawn((
            cohort(tile, PARTY_WORKERS),
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission: ExpeditionMission::Deny {
                    fauna_id: fauna_id.to_string(),
                    target_species,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo: core_sim::LocalStore::new(),
                defection_pull: core_sim::Scalar::zero(),
            },
        ))
        .id()
}

fn phase(app: &App, party: bevy::prelude::Entity) -> ExpeditionPhase {
    app.world
        .get::<Expedition>(party)
        .expect("party alive")
        .phase
}

fn carried(app: &App, party: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(party)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

/// [`build_headless_app`] with the whole roster's `combat.wariness` held at `0` — the world-driven
/// twin of [`deterministic_fauna`], for the fixtures that run a real turn instead of calling a pure
/// helper. See [`FaunaConfig::without_retreat`].
fn deterministic_headless_app() -> App {
    let mut app = build_test_app();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app
}

/// The shipped `person` row's `combat.durability` (`creatures.json`) and the mammoth's
/// (`fauna_config.json`) — how much damage each body soaks before it goes down
/// (`docs/plan_hunt_through_combat.md` §4.2). Restated so the hand-built fights below field the
/// roster's creatures rather than neutral stand-ins.
const PERSON_DURABILITY: f32 = 20.0;
const MAMMOTH_DURABILITY: f32 = 500.0;

/// **Scoping fix.** A party still walking (beyond `hunt.reach_tiles`) must not take, and must not
/// conclude the trip — the completion check is inside the in-reach guard.
#[test]
fn walking_party_never_concludes_the_trip() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    let (herd_pos, before, _cap) = seed_herd(&mut app, &id, 1.0);
    let home = spawn_home_band(&mut app, herd_pos);
    let width = app.world.resource::<TileRegistry>().width;
    let away = UVec2::new((herd_pos.x + width / 4) % width, herd_pos.y);
    let party = spawn_raid_party(&mut app, home, away, &id);

    for _ in 0..3 {
        app.world.run_system_once(advance_expeditions);
        assert_eq!(
            phase(&app, party),
            ExpeditionPhase::Hunting,
            "a party that has not reached its herd must stay in Hunting"
        );
    }
    assert_eq!(carried(&app, party), 0.0, "out of reach → no take");
    assert_eq!(
        herd_biomass(&app, &id),
        before,
        "out of reach → the herd is untouched"
    );
}

/// The first wild-game group of a `size_class`, **pinned** to its anchor so it stays in reach for a
/// whole trip (the map seeds no big game stationary — pin one rather than fight the fauna-movement
/// redesign).
fn pinned_game_herd(app: &mut App, size_class: &str) -> String {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.size_class.as_str() == size_class)
            .map(|h| h.id.clone())
            .unwrap_or_else(|| panic!("map seeds at least one {size_class}-game group"))
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.route = vec![herd.current_pos];
    herd.step_index = 0;
    id
}

/// The mammoth's shipped display name — combat `{ attack 8, defense 12 }`.
const MAMMOTH: &str = "Thunder Mammoths";

/// Retag a stationary game herd to a chosen species and park it on a fat standing stock.
fn retag_herd(app: &mut App, species_display: &str) -> String {
    let id = stationary_game_herd(app);
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.species = species_display.to_string();
    herd.carrying_capacity = herd.carrying_capacity.max(4000.0);
    herd.biomass = herd.carrying_capacity;
    id
}

fn party_working(app: &App, party: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(party)
        .expect("party alive")
        .working
        .to_f32()
}

/// A raiding party against a mammoth (attack 8) loses party working-age population over an
/// engagement turn.
#[test]
fn a_raiding_party_takes_casualties_against_a_mammoth() {
    let mut app = spawn_world();
    let id = retag_herd(&mut app, MAMMOTH);
    let (pos, _b, _cap) = seed_herd(&mut app, &id, 1.0);
    let home = spawn_home_band(&mut app, pos);
    // Party ON the herd's tile → in reach, so it engages this turn.
    let party = spawn_raid_party(&mut app, home, pos, &id);
    let before = party_working(&app, party);
    app.world.run_system_once(advance_expeditions);
    let after = party_working(&app, party);
    assert!(
        after < before,
        "a mammoth (attack 8) raid must cost party working-age: {before} -> {after}"
    );
    // ...and it narrates on the command feed.
    let narrated = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .any(|e| e.kind.as_str() == "hunt_danger");
    assert!(narrated, "a dangerous raid pushes a hunt_danger feed line");
}

/// **A detached party's dead are not where the band stands** (`core_sim::belief`, issue #697). The
/// same lethal mammoth raid as above costs the party people, and credits **no** place with belief:
/// the deaths source is the band's own tile, and a raiding party died somewhere else.
#[test]
fn a_raiding_partys_dead_credit_no_belief() {
    let mut app = spawn_world();
    let id = retag_herd(&mut app, MAMMOTH);
    let (pos, _b, _cap) = seed_herd(&mut app, &id, 1.0);
    let home = spawn_home_band(&mut app, pos);
    let party = spawn_raid_party(&mut app, home, pos, &id);
    let belief_before = app.world.resource::<core_sim::BeliefRegistry>().clone();
    let before = party_working(&app, party);
    app.world.run_system_once(advance_expeditions);
    assert!(
        party_working(&app, party) < before,
        "fixture: the raiding party must lose people, or this proves nothing"
    );
    assert_eq!(
        *app.world.resource::<core_sim::BeliefRegistry>(),
        belief_before,
        "a raiding party's casualties must not add belief anywhere"
    );
}

/// The `expedition_danger_multiplier` makes the fight bloodier — a direct `resolve_fight` comparison
/// (same payload, two tunings) loses strictly more at `> 1` than at `1`.
#[test]
fn the_expedition_danger_multiplier_scales_losses() {
    use core_sim::{
        resolve_fight, CombatStats, CombatTuning, Contingent, ContingentId, FightPayload, Force,
        ForceId, Posture, RangeBand,
    };

    let payload = FightPayload {
        sides: vec![
            Force {
                id: ForceId(0),
                posture: Posture::Aggressor,
                contingents: vec![Contingent {
                    kind: ContingentId::from("person"),
                    count: 4.0,
                    profile: CombatStats {
                        attack: 1.0,
                        defense: 1.0,
                        durability: PERSON_DURABILITY,
                        range: RangeBand::Melee,
                        wariness: 0.0,
                    },
                }],
            },
            Force {
                id: ForceId(1),
                posture: Posture::Defender,
                contingents: vec![Contingent {
                    kind: ContingentId::from("mammoth"),
                    count: 1.0,
                    profile: CombatStats {
                        attack: 8.0,
                        defense: 12.0,
                        durability: MAMMOTH_DURABILITY,
                        range: RangeBand::Melee,
                        wariness: 0.0,
                    },
                }],
            },
        ],
        terrain: vec![],
        seed: 0,
    };

    // Only `lethality` differs — the point of the assertion — so both start from the shipped tuning.
    let local = CombatTuning {
        lethality: 1.0,
        ..CombatTuning::default()
    };
    let expedition = CombatTuning {
        lethality: 1.5,
        ..CombatTuning::default()
    };
    let band_losses = |tuning: &CombatTuning| -> f32 {
        let out = resolve_fight(&payload, tuning);
        out.results
            .iter()
            .find(|r| r.force == ForceId(0))
            .map(|r| r.killed + r.wounded)
            .unwrap_or(0.0)
    };
    assert!(
        band_losses(&expedition) > band_losses(&local),
        "a bloodier (>1) expedition multiplier must cost strictly more than a local hunt"
    );
}

/// Run one real turn of the three systems, in pipeline order (Logistics regrow → Population
/// move → Population expedition step).
fn drive_expedition_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_expeditions);
}

/// Pin a big-game herd stationary, **freeze its K** (`fodder_per_biomass = 0` → a non-grazing herd
/// keeps its constant `carrying_capacity`), and seed it full. Returns `(id, position)`.
fn pin_frozen_full_big_herd(app: &mut App) -> (String, UVec2) {
    // A large fixed K, independent of which big-game species the map seeded first.
    const FROZEN_CAP: f32 = 2000.0;
    let id = pinned_game_herd(app, "big");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.fodder_per_biomass = 0.0;
    herd.carrying_capacity = FROZEN_CAP;
    herd.biomass = FROZEN_CAP;
    // A harmless species (attack 0), so no fixture here loses its party to the fight it is not
    // about.
    herd.species = "Rabbit Warren".to_string();
    let pos = herd.position();
    (id, pos)
}

/// **A returning party whose home band cannot be resolved folds back where it stands, instead of
/// haunting the map forever.**
///
/// The `Returning` arm branched on `near_home` alone, and `near_home` is `false` whenever the home
/// band's live tile cannot be read — so an orphaned party failed the fold-back test **and** the
/// `else if let Some(home)` retarget below it, leaving a live cohort parked on its tile for the rest
/// of the game with its workers, pack and pelts held out of the economy. This is the exact shape a
/// playtester reported: a recalled party that never folded back and whose marker never moved.
///
/// The arm's own comment already said what should happen — "no home band left to receive them means
/// the haul is simply lost, exactly as the carried food is" — it was merely unreachable. `near_home`
/// asks *"am I close enough to hand things over?"*; whether there is anyone to hand them **to** is a
/// different question and now has its own answer.
#[test]
fn a_returning_party_with_no_home_band_left_does_not_haunt_the_map() {
    /// Long enough that any plausible walk home would have finished — a party that is still here is
    /// stuck, not travelling.
    const TURNS_A_WALK_HOME_COULD_NEED: u32 = 12;

    let mut app = deterministic_headless_app();
    app.update();
    let (herd_id, herd_pos) = pin_frozen_full_big_herd(&mut app);
    // A home band that is not a band at all: the party's `home_band` resolves to nothing, which is
    // what makes both `home_pos` and `near_home` unanswerable.
    let orphaned_home = app.world.spawn_empty().id();
    let party = spawn_raid_party(&mut app, orphaned_home, herd_pos, &herd_id);
    app.world
        .get_mut::<Expedition>(party)
        .expect("the party exists")
        .phase = ExpeditionPhase::Returning;

    for _ in 0..TURNS_A_WALK_HOME_COULD_NEED {
        drive_expedition_turn(&mut app);
        if !app.world.entities().contains(party) {
            return;
        }
    }
    panic!(
        "an orphaned Returning party is still on the map after {TURNS_A_WALK_HOME_COULD_NEED} \
         turns — it has no home to walk to, so it must fold back where it stands"
    );
}

/// The published horizon must be the lever the projections were actually run over, and it must be
/// **positive**: a `0` on the wire would let the client render *"more than 0 turns"*, which is the
/// exact failure this field exists to prevent.
///
/// Asserted on the **exported snapshot** — a field that never reached the codec still satisfies an
/// in-process assertion on `WorldSnapshot`.
#[test]
fn every_cohort_publishes_the_forecast_horizon_on_the_wire() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = deterministic_headless_app();
    app.update();

    let (herd_id, herd_pos) = pin_frozen_full_big_herd(&mut app);
    let home = spawn_home_band(&mut app, herd_pos);
    // A party as well as a resident band: the lever rides EVERY cohort, because the outfit UI is
    // read off the resident band.
    spawn_raid_party(&mut app, home, herd_pos, &herd_id);

    let horizon = expedition_config(&app).hunt.forecast_horizon_turns;
    assert!(
        horizon > 0,
        "`ExpeditionConfig::validate` pins the horizon above zero — a zero here would make the \
         assertion below vacuous"
    );

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

    assert!(
        !cohorts.is_empty(),
        "the fixture published cohorts — otherwise this test proves nothing"
    );
    for cohort in cohorts.iter() {
        assert_eq!(
            cohort.expeditionForecastHorizonTurns(),
            horizon,
            "cohort {} published horizon {} on the wire, but the raid projection runs over {horizon}",
            cohort.entity(),
            cohort.expeditionForecastHorizonTurns()
        );
    }
}

/// **A party's own quarry is NOT published just because the party is standing on it — and the party
/// names it anyway.**
///
/// The herd's tile is marked `Discovered`, which is the live shape of this: the player walked that
/// ground earlier, the party is out there raiding now, and nothing is watching the hex today. The herd
/// is therefore absent from the snapshot (`Active`, not `Discovered` —
/// `snapshot::subsistence::HerdSnapshotInputs::herd_is_visible`) while the party still carries its id.
/// That is exactly the state that put a raw `game_deer_57` on screen.
///
/// Asserted on the **encoded wire**, and with a positive control: the same herd, watched, IS published.
/// Without that control the absence proves nothing — a fixture publishing no herds at all would pass
/// the first half.
#[test]
fn a_party_names_its_quarry_when_the_herd_has_left_the_snapshot() {
    let mut app = deterministic_headless_app();
    app.update();

    let herd_id = stationary_game_herd(&app);
    let (herd_pos, herd_species) = {
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(&herd_id).expect("the picked herd resolves");
        (herd.position(), herd.species.clone())
    };
    assert!(
        !herd_species.is_empty(),
        "the roster names this species — an empty name would make the assertions below vacuous"
    );

    // The party stands ON the herd; its home band is far away, so nothing else can light the hex.
    let home = spawn_home_band(&mut app, herd_pos);
    spawn_raid_party(&mut app, home, herd_pos, &herd_id);

    // Seen once, not seen now — a REAL faction map rather than the fail-closed absent-map path, so
    // what this measures is the `Active`-not-`Discovered` rule and not the lack of a map.
    {
        let grid = app.world.resource::<SimulationConfig>().grid_size;
        let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
        let mut ledger = app.world.resource_mut::<VisibilityLedger>();
        let map = ledger.ensure_faction(viewer, grid.x, grid.y);
        map.mark_discovered(herd_pos.x, herd_pos.y);
    }

    let (published_ids, target_herd, target_species) = published_party_target(&mut app);
    assert!(
        !published_ids.contains(&herd_id),
        "the party's own quarry {herd_id} is published while nothing watches its hex — the fog gate \
         this test is built on has changed, and the bug it guards no longer has this shape"
    );
    assert_eq!(
        target_herd, herd_id,
        "the party is still bound to the herd it launched at"
    );
    assert_eq!(
        target_species, herd_species,
        "the party must publish its quarry's NAME, so the client never falls back to the id"
    );

    // POSITIVE CONTROL — the same herd, watched, is published. This is what makes the absence above a
    // statement about visibility rather than about the fixture.
    reveal_herds(&mut app, std::slice::from_ref(&herd_id));
    let (published_ids, _, target_species) = published_party_target(&mut app);
    assert!(
        published_ids.contains(&herd_id),
        "a watched herd is published — otherwise the absence asserted above proves nothing"
    );
    assert_eq!(
        target_species, herd_species,
        "and the published name does not depend on whether the herd is in view"
    );
}

/// Recapture, encode, and read back `(every published herd id, the party's target id, its target
/// species)` off the **wire**. A field that never reached the codec still satisfies an in-process
/// assertion on `WorldSnapshot`, so this reads the encoded buffer the client actually gets.
fn published_party_target(app: &mut App) -> (Vec<String>, String, String) {
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
    let snapshot = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");
    let published_ids: Vec<String> = snapshot
        .subsistence()
        .and_then(|section| section.herds())
        .map(|herds| {
            herds
                .iter()
                .filter_map(|herd| herd.id().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let cohorts = snapshot
        .population()
        .and_then(|section| section.populations())
        .expect("the snapshot carries a population section");
    let party = cohorts
        .iter()
        .find(|cohort| cohort.isExpedition())
        .expect("the fixture published its expedition cohort");
    (
        published_ids,
        party.expeditionTargetHerd().unwrap_or("").to_string(),
        party.expeditionTargetSpecies().unwrap_or("").to_string(),
    )
}
