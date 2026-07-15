//! Hunting-expedition take + trip semantics (`advance_expeditions`, `ExpeditionPhase::Hunting`).
//!
//! The load-bearing invariant: **"Sustain" means one thing** — the Maximum Sustainable Yield *flow*
//! (`hunt_policy_ceiling`) — for a resident band's Hunt arm AND for a hunting expedition. It is not
//! a stock target, so a party sent at a herd that happens to have spawned below the old
//! `0.7 × carrying_capacity` line no longer completes instantly with a zero take.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_expeditions, advance_herds, build_headless_app, hunt_policy_ceiling, hunt_take,
    hunt_trip_forecast, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_forage, spawn_initial_herds, spawn_initial_world, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, Expedition, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionMission, ExpeditionPhase, FactionId, FactionInventory, FaunaConfig,
    FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry, GenerationId,
    GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    ResidentBand, Scalar, SimulationConfig, SimulationTick, SnapshotHistory,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, VisibilityConfig,
    VisibilityConfigHandle, VisibilityLedger, WellbeingConfigHandle, FOOD,
};

/// Party size used by every trip test: 4 hunters (the design's reference party).
const PARTY_WORKERS: u32 = 4;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
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
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(ExpeditionConfigHandle::default());
    app.world
        .insert_resource(VisibilityConfigHandle::new(VisibilityConfig::builtin()));
    app.world.insert_resource(VisibilityLedger::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// Replace the expedition config with a JSON override (test-local tuning, e.g. a tiny carry cap).
fn set_expedition_config(app: &mut App, json: &str) {
    app.world
        .insert_resource(ExpeditionConfigHandle::new(Arc::new(
            ExpeditionConfig::from_json_str(json).expect("test expedition config parses"),
        )));
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
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: Default::default(),
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

/// A home band far from the herd (so no near-band early delivery / comm flush interferes).
fn spawn_home_band(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let width = app.world.resource::<TileRegistry>().width;
    let height = app.world.resource::<TileRegistry>().height;
    let far = UVec2::new(
        (herd_pos.x + width / 3) % width,
        (herd_pos.y + height / 3) % height,
    );
    let tile = tile_at(app, far);
    app.world.spawn((cohort(tile, 10), ResidentBand)).id()
}

/// A `PARTY_WORKERS`-strong hunting party at `pos`, already in the `Hunting` phase.
fn spawn_hunt_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    policy: FollowPolicy,
) -> bevy::prelude::Entity {
    spawn_hunt_party_of(app, home_band, pos, fauna_id, policy, PARTY_WORKERS)
}

/// A hunting party of `workers` positioned at `pos`, already in the `Hunting` phase (as
/// `send_hunt_expedition` spawns it).
fn spawn_hunt_party_of(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    policy: FollowPolicy,
    workers: u32,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    app.world
        .spawn((
            cohort(tile, workers),
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.to_string(),
                    policy,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
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

/// (1) **The reported bug.** A Sustain party sent at a herd seeded *below* the retired
/// `0.7 × carrying_capacity` stock floor used to complete on turn 1 with a zero take (the floor was
/// already crossed). It must now travel, arrive, take a real MSY skim, and keep hunting.
#[test]
fn sustain_expedition_below_old_stock_floor_takes_a_real_skim() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    // 0.5 × K: below the retired 0.7 × K floor, above the Allee threshold → a positive MSY skim.
    let (herd_pos, before, _cap) = seed_herd(&mut app, &id, 0.5);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_hunt_party(&mut app, home, herd_pos, &id, FollowPolicy::Sustain);

    app.world.run_system_once(advance_expeditions);

    assert!(
        carried(&app, party) > 0.0,
        "a Sustain expedition must land a real take, got {}",
        carried(&app, party)
    );
    assert!(
        herd_biomass(&app, &id) < before,
        "the herd must lose the take: {before} -> {}",
        herd_biomass(&app, &id)
    );
    assert_eq!(
        phase(&app, party),
        ExpeditionPhase::Hunting,
        "with a nearly-empty pack the party keeps hunting — it must NOT insta-complete"
    );
}

/// (1b) …and it eventually fills its pack and heads home with food (a small carry cap keeps the
/// test short; the real economics of a full-size pack are what `hunt_trip_forecast` warns about).
#[test]
fn sustain_expedition_fills_and_delivers() {
    let mut app = spawn_world();
    set_expedition_config(
        &mut app,
        r#"{
            "max_party_size": 8,
            "comm_range_tiles": 2,
            "comm_range_tech_factor": 1.0,
            "observe_sight_range": 6,
            "provision_draw_per_worker_per_tile": 1.0,
            "provision_upkeep_per_worker": 0.5,
            "hunt": {
                "per_worker_carry": 0.05,
                "reach_tiles": 1,
                "drop_off_within_tiles": 3,
                "min_deliver_fraction": 0.5,
                "viability_warn_turns": 20,
                "forecast_horizon_turns": 60
            },
            "replenish": { "low_turns": 3, "reach_tiles": 1 }
        }"#,
    );
    let id = stationary_game_herd(&app);
    let (herd_pos, _before, _cap) = seed_herd(&mut app, &id, 0.5);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_hunt_party(&mut app, home, herd_pos, &id, FollowPolicy::Sustain);

    let mut delivered = false;
    for _ in 0..20 {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_expeditions);
        if phase(&app, party) == ExpeditionPhase::Returning {
            delivered = true;
            break;
        }
    }
    assert!(delivered, "the party should fill its pack and head home");
    assert!(
        carried(&app, party) > 0.0,
        "it must be carrying food when it turns for home"
    );
}

/// (2) **Scoping fix.** A party still walking (beyond `hunt.reach_tiles`) must not take, and must
/// not conclude the trip — the completion check is now inside the in-reach guard.
#[test]
fn walking_party_never_concludes_the_trip() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    let (herd_pos, before, _cap) = seed_herd(&mut app, &id, 0.5);
    let home = spawn_home_band(&mut app, herd_pos);
    // Well beyond `reach_tiles` (1) — the party is en route.
    let width = app.world.resource::<TileRegistry>().width;
    let away = UVec2::new((herd_pos.x + width / 4) % width, herd_pos.y);
    let party = spawn_hunt_party(&mut app, home, away, &id, FollowPolicy::Sustain);

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

/// (3) **One word, one meaning.** The expedition's Sustain take *is* `hunt_policy_ceiling(Sustain,
/// …)` — exactly what a resident band's Hunt arm takes from the same herd state.
#[test]
fn sustain_expedition_take_equals_the_shared_msy_ceiling() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    let (herd_pos, before, cap) = seed_herd(&mut app, &id, 0.5);
    let home = spawn_home_band(&mut app, herd_pos);
    let _party = spawn_hunt_party(&mut app, home, herd_pos, &id, FollowPolicy::Sustain);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    // A wild herd → the wild ecology (the shared `herd_ecology` mapping; a tamed/penned herd would
    // resolve to the pastoral/pen curve instead).
    let expected = hunt_policy_ceiling(FollowPolicy::Sustain, before, cap, &fauna.ecology, &fauna);
    assert!(expected > 0.0, "a half-capacity herd has a positive MSY");

    app.world.run_system_once(advance_expeditions);

    let taken = before - herd_biomass(&app, &id);
    assert!(
        (taken - expected).abs() <= expected * 1e-3,
        "expedition Sustain take {taken} must equal the shared ceiling {expected}"
    );
}

/// (4) **Sustain keeps the herd healthy.** Skimming the MSY flow holds a herd steady — no downward
/// drift toward collapse over a long trip.
#[test]
fn sustain_expedition_does_not_drift_the_herd_down() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    let (herd_pos, before, cap) = seed_herd(&mut app, &id, 0.5);
    let home = spawn_home_band(&mut app, herd_pos);
    let _party = spawn_hunt_party(&mut app, home, herd_pos, &id, FollowPolicy::Sustain);

    for _ in 0..30 {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_expeditions);
    }

    let after = herd_biomass(&app, &id);
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    assert!(
        after >= before * 0.95,
        "a Sustain skim must not drift the herd down: {before} -> {after}"
    );
    assert!(
        after > fauna.ecology.collapse_fraction * cap,
        "the herd stays well above the collapse threshold"
    );
}

/// (5) **No collateral damage.** Surplus / Market keep their *stock* headroom down to the collapse
/// floor, Eradicate keeps its unfloored take + carries no food, and none of them completes a trip
/// on a partial pack.
#[test]
fn depleting_policies_are_unchanged() {
    for policy in [
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
    ] {
        let mut app = spawn_world();
        let id = stationary_game_herd(&app);
        let (herd_pos, before, cap) = seed_herd(&mut app, &id, 0.5);
        let home = spawn_home_band(&mut app, herd_pos);
        let party = spawn_hunt_party(&mut app, home, herd_pos, &id, policy);

        let (fauna, labor, cfg) = {
            (
                app.world.resource::<FaunaConfigHandle>().get(),
                app.world.resource::<LaborConfigHandle>().get(),
                expedition_config(&app),
            )
        };
        let floor = match policy {
            FollowPolicy::Eradicate => 0.0,
            _ => fauna.ecology.collapse_fraction * cap,
        };
        let worker_cap = PARTY_WORKERS as f32 * labor.hunt.per_worker_biomass_capacity;
        let carry_room = if matches!(policy, FollowPolicy::Eradicate) {
            f32::INFINITY
        } else {
            PARTY_WORKERS as f32 * cfg.hunt.per_worker_carry / fauna.hunt.provisions_per_biomass
        };
        let expected = worker_cap
            .min((before - floor).max(0.0))
            .min(carry_room)
            .clamp(0.0, before);

        app.world.run_system_once(advance_expeditions);

        let taken = before - herd_biomass(&app, &id);
        assert!(
            (taken - expected).abs() <= expected.max(1.0) * 1e-3,
            "{:?}: stock-headroom take {taken} must equal {expected}",
            policy
        );
        if matches!(policy, FollowPolicy::Eradicate) {
            assert_eq!(
                carried(&app, party),
                0.0,
                "Eradicate is denial — it carries no food"
            );
        } else {
            assert!(carried(&app, party) > 0.0, "{:?} carries food", policy);
        }
        assert_eq!(
            phase(&app, party),
            ExpeditionPhase::Hunting,
            "{:?}: a partial pack away from the band keeps hunting",
            policy
        );
    }
}

/// (6) **The launch forecast's three honest verdicts.** It is a bounded forward *simulation*, so
/// "no ETA" is no longer one undifferentiated `None`: a collapsing (sub-Allee) herd yields nothing at
/// all (`first_turn_provisions == 0`), a denial mission brings nothing home (`delivers_food ==
/// false`), and a healthy herd fills in a real number of hunting turns.
#[test]
fn hunt_trip_forecast_reports_viability() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let cfg = expedition_config(&app);

    // Below the Allee threshold → no sustainable take → the party can never fill.
    let collapse_fraction = fauna.ecology.collapse_fraction;
    seed_herd(&mut app, &id, collapse_fraction * 0.5);
    {
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(&id).expect("herd present");
        let forecast = hunt_trip_forecast(
            PARTY_WORKERS,
            herd,
            FollowPolicy::Sustain,
            &fauna,
            &labor,
            &cfg,
        );
        assert!(
            forecast.turns_to_fill.is_none(),
            "a collapsing herd yields no sustainable take"
        );
        assert_eq!(
            forecast.first_turn_provisions, 0.0,
            "…and lands nothing on the first hunting turn — the 'returns empty' signal"
        );
        assert!(forecast.delivers_food, "Sustain does bring food home");
    }

    // A **big** herd, full: its stock headroom covers a whole pack, so a Surplus party stays
    // throughput-bound and fills fast. (A *small* herd would not — that is the stock-exhaustion case
    // (7)/(9) cover.)
    let big = pinned_game_herd(&mut app, "big");
    seed_herd(&mut app, &big, 1.0);
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&big).expect("herd present");

    // A denial mission delivers no food, so it gets no ETA — never a fillable-looking number.
    let denial = hunt_trip_forecast(
        PARTY_WORKERS,
        herd,
        FollowPolicy::Eradicate,
        &fauna,
        &labor,
        &cfg,
    );
    assert!(!denial.delivers_food);
    assert_eq!(denial.turns_to_fill, None, "no ETA for a denial mission");
    assert_eq!(denial.first_turn_provisions, 0.0);

    // A healthy herd under Surplus: the party is throughput-bound, so it fills fast.
    let forecast = hunt_trip_forecast(
        PARTY_WORKERS,
        herd,
        FollowPolicy::Surplus,
        &fauna,
        &labor,
        &cfg,
    );
    assert!(forecast.first_turn_provisions > 0.0);
    let turns = forecast
        .turns_to_fill
        .expect("a healthy herd fills a Surplus pack inside the horizon");
    assert!(
        (1..=cfg.hunt.viability_warn_turns).contains(&turns),
        "a full big herd under Surplus is a viable trip — the party is throughput-bound, got {turns}"
    );
}

/// Party sizes the estimate-table guard sweeps: a lone hunter, a pair, the reference party, and a
/// full pack. Every entry must be a **legal** party size (`1 ..= expedition_config.max_party_size`),
/// because that is exactly the range the sim exports an estimate for.
const ESTIMATE_PARTY_SIZES: [u32; 4] = [1, 2, PARTY_WORKERS, 8];

/// Both sides run the same linear formula but land on the sim's fixed-point grid at *different*
/// points (the exported ceiling is quantized before the party-throughput `min` / the band's output
/// multiplier; the take quantizes the whole product), so allow a few `Scalar` quanta of rounding.
const TAKE_ABS_EPSILON: f32 = 4.0 / Scalar::SCALE as f32;

/// …plus f32 slop proportional to the magnitude (a big-game take runs to hundreds of provisions,
/// where an f32 mantissa is coarser than the fixed-point quantum).
const TAKE_REL_EPSILON: f32 = 1e-5;

/// Assert a snapshot-derived preview matches the provisions the sim's real take produced.
fn assert_provisions_eq(preview: f32, real_take: f32, context: &str) {
    let tolerance = TAKE_ABS_EPSILON + real_take.abs() * TAKE_REL_EPSILON;
    assert!(
        (preview - real_take).abs() <= tolerance,
        "{context}: snapshot preview {preview} != real take {real_take}"
    );
}

/// Collapse a herd's wander route to the tile it is standing on: `advance_herd_roam` treats a
/// `route_len == 1` group as a stationary cluster, so the party stays in reach for a whole trip and
/// travel never confounds a turn count. (The map seeds no *big* game stationary, so the trip tests
/// pin one rather than fight the separately-tracked fauna-movement redesign.)
fn pin_herd(app: &mut App, id: &str) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("herd present");
    herd.route = vec![herd.current_pos];
    herd.step_index = 0;
}

/// The first wild-game group of a `size_class` ("small" = rabbit/fowl, "big" = deer/boar), **pinned**
/// to its anchor. Small game is the stock-exhaustion case; big game has the headroom to fill a pack.
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
    pin_herd(app, &id);
    id
}

/// Put a herd back exactly as it was — re-inserting it if the previous leg of a sweep hunted it to
/// extinction — so every (policy × party size) leg runs a real party against the *same* herd state
/// the exported estimate was computed from.
fn restore_herd(app: &mut App, baseline: &Herd) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    match registry.herds.iter_mut().find(|h| h.id == baseline.id) {
        Some(live) => *live = baseline.clone(),
        None => registry.herds.push(baseline.clone()),
    }
}

/// Run a **real** hunting party forward against `baseline` and report the first turn its larder
/// reaches the carry cap (`None` = it does not fill within `horizon` turns). The herd is restored to
/// `baseline` first and the party is despawned after, so a caller can sweep every (policy, workers)
/// leg in one world. The party is parked on the (pinned, stationary) herd's tile → in reach from turn
/// 1, and its home band is far away → the near-band early-delivery gate never fires: the trip can end
/// only on a full pack.
fn real_trip_turns(
    app: &mut App,
    baseline: &Herd,
    home: bevy::prelude::Entity,
    policy: FollowPolicy,
    workers: u32,
    horizon: u32,
) -> Option<u32> {
    restore_herd(app, baseline);
    let cap = scalar_from_f32(workers as f32 * expedition_config(app).hunt.per_worker_carry);
    let party = spawn_hunt_party_of(
        app,
        home,
        baseline.position(),
        &baseline.id,
        policy,
        workers,
    );

    let mut filled = None;
    for turn in FIRST_HUNTING_TURN..=horizon {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_expeditions);
        if carried_scalar(app, party) >= cap {
            filled = Some(turn);
            break;
        }
    }
    app.world.despawn(party);
    filled
}

/// (7) **THE EXPEDITION ANTI-DRIFT GUARD — the client's job is a TABLE LOOKUP.** The outfit UI shows
/// the trip's length *before* the player commits workers, and it must never re-implement (or even
/// re-arithmetic) the sim: it reads `HerdTelemetryState.hunt_trip_estimates[(policy, party_workers)]
/// .turns_to_fill` and prints it (`0` = "won't fill").
///
/// So this test asserts the exported table **is the truth**: for a small-game herd (the
/// stock-exhaustion case), a big-game herd, and a collapsing (sub-Allee) herd, across every legal
/// party size and all four policies, the exported entry equals what a **real party run forward
/// through the real systems** actually does.
///
/// The bug it exists to prevent: the forecast used to divide the carry cap by a single per-policy
/// number. For Sustain that number is a genuine per-turn *flow* (MSY) and the division is exact — but
/// for **Surplus/Market it is a total *stock*** (headroom down to the collapse floor). On a small herd
/// the party strips that headroom in a turn or two and then crawls at the regrowth trickle, so the
/// division read a **4-hunter party on a full Rabbit Warren (K = 200) as a ~5-turn trip** when the
/// truth is that it **never fills** inside the 60-turn horizon (only a lone hunter fills, in 23
/// turns). `SMALL_HERD_DEPLETING_MIN_TRUTH` pins that specific lie shut.
#[test]
fn exported_hunt_trip_estimates_match_a_real_party_run() {
    let mut app = build_headless_app();
    // Turn 1: worldgen seeds the herds and `capture_snapshot` records the ring entry.
    app.update();

    let collapse_fraction = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .ecology
        .collapse_fraction;
    let cfg = expedition_config(&app);
    let horizon = cfg.hunt.forecast_horizon_turns;

    let small_game = pinned_game_herd(&mut app, "small");
    let big_game = pinned_game_herd(&mut app, "big");
    // Any third herd carries the collapsing case, so the two huntable cases stay healthy.
    let collapsing = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id != small_game && h.id != big_game)
            .map(|h| h.id.clone())
            .expect("map seeds at least three herds")
    };
    pin_herd(&mut app, &collapsing);

    // A **full** small herd is the motivating case: its whole stock headroom is worth a fraction of a
    // pack, so a depleting policy empties it and then waits on regrowth. A near-full big herd has the
    // headroom to keep the party throughput-bound. The third is driven below the Allee threshold.
    seed_herd(&mut app, &small_game, 1.0);
    seed_herd(&mut app, &big_game, 0.9);
    seed_herd(&mut app, &collapsing, collapse_fraction * 0.5);

    // Re-capture so the snapshot carries the seeded herd states (the same path a post-command
    // re-capture takes on the server).
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("turn 1 captured a snapshot")
        .snapshot;

    // The levers are echoed onto every cohort; the outfit UI reads them off the selected band.
    let cohort = snapshot
        .populations
        .first()
        .expect("the campaign spawns at least one band");
    assert_eq!(
        cohort.expedition_viability_warn_turns, cfg.hunt.viability_warn_turns,
        "the exported viability threshold must be the config lever the sim warns on"
    );

    // Snapshot every herd's pre-trip state up front: a real party run advances the ecology of EVERY
    // herd (and despawns the collapsing one), so a baseline read lazily mid-sweep would be stale — or
    // gone.
    let baselines: Vec<Herd> = [&small_game, &big_game, &collapsing]
        .iter()
        .map(|id| {
            app.world
                .resource::<HerdRegistry>()
                .find(id)
                .expect("herd present")
                .clone()
        })
        .collect();

    for baseline in &baselines {
        let id = &baseline.id;
        let exported = snapshot
            .herds
            .iter()
            .find(|h| &h.id == id)
            .unwrap_or_else(|| panic!("herd {id} is in the snapshot"));
        assert!(
            exported.huntable,
            "{id}: the estimate table is only exported for huntable herds"
        );
        assert_eq!(
            exported.hunt_trip_estimates.len(),
            FollowPolicy::EXTRACTIVE.len() * cfg.max_party_size as usize,
            "{id}: every EXTRACTIVE policy × every legal party size must export an estimate"
        );
        // The investment policies are place-bound band work — `send_hunt_expedition` rejects them, so
        // a trip estimate for one would be a number for a trip that cannot be launched.
        for investment in [FollowPolicy::Cultivate, FollowPolicy::Corral] {
            assert!(
                !exported
                    .hunt_trip_estimates
                    .iter()
                    .any(|e| e.policy == investment.as_str()),
                "{id}: {investment:?} is not an expedition policy and must export no trip estimate"
            );
        }

        // A home band far from THIS herd, so the near-band early-delivery gate never fires: the trip
        // can end only on a full pack (which is what the estimate predicts).
        let home = spawn_home_band(&mut app, baseline.position());

        for policy in FollowPolicy::EXTRACTIVE {
            for workers in ESTIMATE_PARTY_SIZES {
                let estimate = exported
                    .hunt_trip_estimates
                    .iter()
                    .find(|e| e.policy == policy.as_str() && e.party_workers == workers)
                    .unwrap_or_else(|| panic!("{id}: no estimate for {policy:?} ×{workers}"));
                let context = format!("{id} {policy:?} ×{workers}");

                assert_eq!(
                    estimate.delivers_food,
                    policy.delivers_food(),
                    "{context}: the estimate must say whether the mission brings food home"
                );
                // Denial: no food, so no ETA — the client renders "no food delivered", not a number.
                if !policy.delivers_food() {
                    assert_eq!(
                        estimate.turns_to_fill, 0,
                        "{context}: a denial mission never fills a pack"
                    );
                    continue;
                }

                // THE TRUTH: a real party, run forward through the real systems.
                let actual = real_trip_turns(&mut app, baseline, home, policy, workers, horizon);
                let exported_turns =
                    (estimate.turns_to_fill != 0).then_some(estimate.turns_to_fill);
                assert_eq!(
                    exported_turns, actual,
                    "{context}: the exported estimate ({} = {}) must be the turn the party really \
                     fills (0 = won't fill within {horizon}). If these disagree, fix the forecast — \
                     never the sim.",
                    estimate.turns_to_fill,
                    if estimate.turns_to_fill == 0 {
                        "won't fill"
                    } else {
                        "turns"
                    }
                );
            }
        }
    }

    // The collapsing herd is one load-bearing case: no take at all → "won't fill", from the snapshot
    // alone (`0`), for every party size under Sustain/Surplus.
    let collapsed = snapshot
        .herds
        .iter()
        .find(|h| h.id == collapsing)
        .expect("collapsing herd exported");
    for policy in [FollowPolicy::Sustain, FollowPolicy::Surplus] {
        for workers in ESTIMATE_PARTY_SIZES {
            let estimate = collapsed
                .hunt_trip_estimates
                .iter()
                .find(|e| e.policy == policy.as_str() && e.party_workers == workers)
                .expect("collapsing herd estimate exported");
            assert_eq!(
                estimate.turns_to_fill, 0,
                "a sub-Allee herd can never fill a {policy:?} pack — the client's 'won't fill' signal"
            );
        }
    }

    // …and the other, the one this whole rewrite exists for: a **full small herd under a depleting
    // policy**. The old closed-form forecast divided the carry cap by the herd's *stock* headroom and
    // reported ~5 turns for this 4-hunter party. The truth is dozens of turns — and on the shipped
    // levers (a full Rabbit Warren), past the horizon: "won't fill".
    let small = snapshot
        .herds
        .iter()
        .find(|h| h.id == small_game)
        .expect("small-game herd exported");
    for policy in [FollowPolicy::Surplus, FollowPolicy::Market] {
        let estimate = small
            .hunt_trip_estimates
            .iter()
            .find(|e| e.policy == policy.as_str() && e.party_workers == PARTY_WORKERS)
            .expect("small-game herd estimate exported");
        assert!(
            estimate.turns_to_fill == 0 || estimate.turns_to_fill >= SMALL_HERD_DEPLETING_MIN_TRUTH,
            "{policy:?} on a full small herd: a {}-turn estimate is the OLD stock-divided lie — the \
             party strips the stock headroom in a turn or two and then crawls at the herd's regrowth",
            estimate.turns_to_fill
        );
    }
}

/// A 4-hunter party on a **full small herd** (rabbit/fowl) under Surplus/Market: the least the truth
/// can be. Its total stock headroom is worth a fraction of one carry cap, so after the first turn or
/// two the party is living on the herd's regrowth trickle — the trip cannot be short. (On the shipped
/// levers it does not finish at all: a full Rabbit Warren never fills a 4-hunter pack inside the
/// 60-turn horizon.) Comfortably above the old closed-form lie (~5 turns) *and* above
/// `viability_warn_turns` (20), so the guard fails loudly if anyone reintroduces a `carry_cap / stock`
/// division.
const SMALL_HERD_DEPLETING_MIN_TRUTH: u32 = 30;

/// Hard stop for the real-party fill loop in (9). A trip that has not filled by here has blown its
/// forecast by so much that the assertion is meaningless anyway — and the sim's own forecast horizon
/// (`hunt.forecast_horizon_turns`) is well inside it, so a "won't fill" verdict is always reachable.
const MAX_TRIP_TURNS: u32 = 400;

/// Turn 1 of hunting = the **first** `advance_expeditions` run after launch. The party is parked on
/// the herd's tile (in reach from turn 1) and the herd is stationary, so travel never confounds the
/// count: every iteration of the loop below is a hunting turn, and the loop index *is* the turn the
/// forecast is predicting. (The forecast excludes travel for exactly this reason: it means "turns
/// spent hunting once you arrive".)
const FIRST_HUNTING_TURN: u32 = 1;

/// Provisions carried by the party, in the sim's own fixed-point (`Scalar`) — the quantity the
/// `Hunting` arm compares against the carry cap. Deliberately **not** the `f32` view: the party's
/// larder accumulates on the fixed-point grid, and the forecast must simulate it on the same grid.
fn carried_scalar(app: &App, party: bevy::prelude::Entity) -> Scalar {
    app.world
        .get::<PopulationCohort>(party)
        .map(|c| c.stores.get(FOOD))
        .unwrap_or_else(scalar_zero)
}

/// Run the sim forward a turn at a time (herd ecology, then the expedition) and report the **first**
/// turn on which the party's larder reaches its carry cap — the turn the trip actually fills.
fn turn_the_party_fills(app: &mut App, party: bevy::prelude::Entity, cap: Scalar) -> Option<u32> {
    for turn in FIRST_HUNTING_TURN..=MAX_TRIP_TURNS {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_expeditions);
        if carried_scalar(app, party) >= cap {
            return Some(turn);
        }
    }
    None
}

/// Biomass a depleting leg starts from: a **full** herd (`1.0 × K`) — the most stock headroom a
/// Surplus/Market party can ever be handed. On **big** game that headroom covers a whole pack, so the
/// party stays throughput-bound for the entire trip (the fast leg). On **small** game it does not:
/// the party strips the herd to the collapse floor in a turn or two and then lives on the regrowth
/// trickle — the stock-exhaustion case the closed-form forecast got catastrophically wrong.
const DEPLETING_LEG_CAP_FRACTION: f32 = 1.0;

/// Biomass the Sustain leg starts from: `0.5 × K` = the MSY plateau, where `sustainable_yield` peaks
/// and the herd's own regrowth exactly refills the skim — so the flow rate is *constant* across the
/// trip (the one regime the retired closed-form forecast actually described correctly).
const SUSTAIN_LEG_CAP_FRACTION: f32 = 0.5;

/// (9) **THE FORECAST CANNOT LIE — end to end, on a real party.** (7) pins the *exported* estimate to
/// a real run; this pins `hunt_trip_forecast` itself, across every regime the trip can be in:
///
/// | leg | herd | policy | the party is… |
/// |---|---|---|---|
/// | big / Surplus  | big game, full  | Surplus | **throughput-bound** the whole trip (the `ceil()` boundary case: `16 / 3.2` = exactly 5 turns — an f32 rate of 3.1999999 used to invent a phantom 6th) |
/// | big / Sustain  | big game, K/2   | Sustain | **flow-bound** (the MSY skim is below party throughput) |
/// | small / Surplus | small game, full | Surplus | **stock-exhausted** — headroom gone in a turn or two, then the regrowth trickle |
/// | small / Market  | small game, full | Market  | ditto, via the commercial share |
///
/// The last two are the regression: the retired closed-form forecast divided the carry cap by the
/// herd's *stock* headroom as if it were a per-turn flow and promised **~5 turns** on a full rabbit
/// warren. A real 4-hunter party takes dozens of turns — on the shipped levers it never fills inside
/// the horizon at all. If the estimate and the party ever disagree, **fix the forecast, never the
/// sim.**
#[test]
fn party_fills_on_the_forecast_turn() {
    // (size_class, policy, starting biomass as a fraction of K)
    let legs = [
        ("big", FollowPolicy::Surplus, DEPLETING_LEG_CAP_FRACTION),
        ("big", FollowPolicy::Sustain, SUSTAIN_LEG_CAP_FRACTION),
        ("small", FollowPolicy::Surplus, DEPLETING_LEG_CAP_FRACTION),
        ("small", FollowPolicy::Market, DEPLETING_LEG_CAP_FRACTION),
    ];
    for (size_class, policy, cap_fraction) in legs {
        let mut app = spawn_world();
        let id = pinned_game_herd(&mut app, size_class);
        let (herd_pos, _biomass, _cap) = seed_herd(&mut app, &id, cap_fraction);
        let home = spawn_home_band(&mut app, herd_pos);
        // Parked on the herd's tile → in reach from turn 1, so travel never confounds the count. The
        // home band is far away, so the near-band early-delivery gate never fires: the trip can end
        // only on a full pack.
        let party = spawn_hunt_party(&mut app, home, herd_pos, &id, policy);

        let (fauna, labor, cfg) = (
            app.world.resource::<FaunaConfigHandle>().get(),
            app.world.resource::<LaborConfigHandle>().get(),
            expedition_config(&app),
        );
        let forecast = {
            let registry = app.world.resource::<HerdRegistry>();
            let herd = registry.find(&id).expect("herd present");
            hunt_trip_forecast(PARTY_WORKERS, herd, policy, &fauna, &labor, &cfg)
        };
        let carry_cap = PARTY_WORKERS as f32 * cfg.hunt.per_worker_carry;
        let cap = scalar_from_f32(carry_cap);
        let context = format!("{size_class} game / {policy:?}");

        let actual = turn_the_party_fills(&mut app, party, cap);
        // The forecast's horizon is the only reason it may decline to name a turn, so a real party
        // that does fill must fill inside it — and on exactly the promised turn.
        let expected = actual.filter(|turns| *turns <= cfg.hunt.forecast_horizon_turns);
        assert_eq!(
            forecast.turns_to_fill, expected,
            "{context}: the party must fill on the turn the launch forecast promised (real fill turn \
             {actual:?}, forecast {:?}, horizon {}). If these disagree, the forecast's arithmetic has \
             drifted from the sim the party really runs in — fix the forecast, never the sim",
            forecast.turns_to_fill, cfg.hunt.forecast_horizon_turns
        );

        match (size_class, policy, actual) {
            // Throughput-bound: big game has the stock headroom for a whole pack, so a depleting
            // policy fills fast. (This is the `ceil()` boundary leg.)
            ("big", FollowPolicy::Surplus, Some(turns)) => assert!(
                turns <= cfg.hunt.viability_warn_turns,
                "{context}: a full big herd is a viable Surplus trip, got {turns} turns"
            ),
            // Flow-bound: the MSY skim is far below party throughput, so even big game takes dozens
            // of turns under Sustain. That is ecologically honest — and it is what the launch feed
            // warns about (`viability_warn_turns`), not a bug.
            ("big", FollowPolicy::Sustain, Some(turns)) => assert!(
                turns > cfg.hunt.viability_warn_turns,
                "{context}: an MSY skim cannot fill a pack in {turns} turns — this leg is supposed \
                 to exercise the flow-bound branch"
            ),
            // Stock-exhaustion: the leg this whole rewrite exists for. It must be slow, or never fill.
            ("small", _, Some(turns)) => assert!(
                turns >= SMALL_HERD_DEPLETING_MIN_TRUTH,
                "{context}: a full small herd cannot fill a pack in {turns} turns — that is the OLD \
                 closed-form lie (it divided the carry cap by the herd's total STOCK headroom)"
            ),
            ("small", _, None) => {} // Never fills at all — even more emphatically not ~5 turns.
            (class, _, None) => panic!("{class} game / {policy:?}: the party never filled its pack"),
            _ => unreachable!("legs cover only big and small game"),
        }

        if actual.is_some() {
            assert_eq!(
                phase(&app, party),
                ExpeditionPhase::Returning,
                "{context}: a full pack completes the trip"
            );
        }
    }
}

/// Worker counts the band-hunt guard sweeps: an unstaffed assignment (both sides must read 0), a
/// lone hunter, the reference party, and a crew big enough that its throughput overshoots a herd's
/// policy ceiling — so **both** branches of the `min(worker_cap, ceiling)` are exercised.
const BAND_HUNT_WORKER_COUNTS: [u32; 4] = [0, 1, PARTY_WORKERS, 60];

/// Discontent seeded on the band for the second pass, so its exported `outputMultiplier` is
/// genuinely `!= 1.0` (with the shipped wellbeing levers — `discontent_weight` 1.0, `floor_mult`
/// 0.5 — this lands at 0.6). Without it the multiplier would be the identity and the guard would
/// pass even if the client's `× outputMultiplier` term were dropped.
const BAND_DISCONTENT_FRACTION: f32 = 0.4;

/// Biomass (as a fraction of carrying capacity) of the depleted-but-viable herd: above the Allee
/// threshold (`collapse_fraction` = 0.15 → a *positive* Sustain/Surplus ceiling), but low enough
/// that under `CLAMP_BINDING_REGROWTH_RATE` the policy ceiling overshoots what is actually left, so
/// the biomass clamp binds.
const DEPLETED_CAP_FRACTION: f32 = 0.2;

/// Regrowth rate for the clamp-binding pass. The **shipped** `ecology.regrowth_rate` (0.05) is far
/// too gentle for any policy ceiling to exceed a herd's remaining biomass (MSY ≤ 0.05 × biomass,
/// Surplus ≤ 0.08 × biomass), so the biomass clamp is inert under today's levers — but it is a
/// *config lever*, and a designer raising it (or `surplus_multiplier` / `market.take_fraction`) must
/// not silently break the client's preview. At 2.0 the Surplus/Sustain ceiling on a
/// `DEPLETED_CAP_FRACTION` herd is ~1.6×/~0.3× its biomass, so the exported ceiling's biomass clamp
/// (and `hunt_take`'s) genuinely binds and the two must still agree.
const CLAMP_BINDING_REGROWTH_RATE: f32 = 2.0;

/// Seed every cohort's discontent, so the exported `outputMultiplier` is a known non-identity value.
fn set_discontent(app: &mut App, fraction: f32) {
    let mut cohorts = app.world.query::<&mut PopulationCohort>();
    for mut cohort in cohorts.iter_mut(&mut app.world) {
        cohort.discontent_fraction = scalar_from_f32(fraction);
    }
}

/// Swap in a fauna config with a tweaked ecology regrowth rate (test-local tuning — the species
/// table and every other lever stay as shipped).
fn set_fauna_regrowth_rate(app: &mut App, regrowth_rate: f32) {
    let mut fauna = FaunaConfig::clone(&app.world.resource::<FaunaConfigHandle>().get());
    fauna.ecology.regrowth_rate = regrowth_rate;
    app.world
        .insert_resource(FaunaConfigHandle::new(Arc::new(fauna)));
}

/// Replay the **client's local-hunt yield preview** — pure arithmetic over exported snapshot fields
/// — against the provisions `hunt_take` really returns for a resident band, over every worker count
/// × every policy × each of `herd_ids`.
fn assert_band_preview_matches_hunt_take(app: &mut App, herd_ids: &[String], case: &str) {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();

    let cohort = snapshot
        .populations
        .first()
        .expect("the campaign spawns at least one band");
    // The band applies its morale/discontent productivity modifier at payout — the client reads the
    // already-exported multiplier rather than recomputing the wellbeing stack.
    let output_multiplier = Scalar::from_raw(cohort.output_multiplier).to_f32();

    for id in herd_ids {
        let exported = snapshot
            .herds
            .iter()
            .find(|h| &h.id == id)
            .unwrap_or_else(|| panic!("{case}: herd {id} is in the snapshot"));

        assert!(
            !exported
                .hunt_policy_ceilings
                .iter()
                .any(|c| c.policy == FollowPolicy::Cultivate.as_str()),
            "{case}: {id}: Cultivate is forage-only — a herd has no cultivate ceiling row"
        );
        // Every policy a Hunt assignment accepts — the four extractive rungs AND Corral, whose
        // deliberately dipped yield the player must see before committing to the pen.
        for policy in FollowPolicy::HUNT_POLICIES {
            let ceiling = exported
                .hunt_policy_ceilings
                .iter()
                .find(|c| c.policy == policy.as_str())
                .unwrap_or_else(|| panic!("{case}: {id}: no exported ceiling for {policy:?}"))
                .provisions_per_turn;

            for workers in BAND_HUNT_WORKER_COUNTS {
                // The client's arithmetic, verbatim — no ecology model, just the exported numbers.
                let client_rate = (workers as f32 * cohort.hunt_per_worker_provisions).min(ceiling)
                    * output_multiplier;

                // The sim's real band take (a resident band has no carry limit — it eats/banks the
                // whole take, so `carry_room_biomass = INFINITY`, exactly as the Hunt labor arm
                // passes). Clone the herd so each sweep entry sees the same pre-take state.
                let mut herd = app
                    .world
                    .resource::<HerdRegistry>()
                    .find(id)
                    .expect("herd present")
                    .clone();
                let sim_rate = hunt_take(
                    &mut herd,
                    workers,
                    policy,
                    labor.hunt.per_worker_biomass_capacity,
                    &fauna,
                    output_multiplier,
                    f32::INFINITY,
                )
                .to_f32();

                assert_provisions_eq(
                    client_rate,
                    sim_rate,
                    &format!("{case}: {id} {policy:?} ×{workers} (mult {output_multiplier})"),
                );
            }
        }
    }
}

/// (8) **THE BAND-TAKE ANTI-DRIFT GUARD** — the local-hunt sibling of (7). The client previews a
/// resident band's per-turn hunt yield from the snapshot alone:
///
/// ```text
/// rate = min(workers × huntPerWorkerProvisions, ceiling_for(policy)) × outputMultiplier
/// ```
///
/// which is arithmetically `hunt_take(.., carry_room_biomass = INFINITY)` — the biomass→provisions
/// conversion and the productivity multiplier are both linear, so they factor out of the `min`, and
/// the exported ceiling is **biomass-clamped** exactly as the take is. This test replays that
/// arithmetic over a **real captured snapshot** and asserts it equals the provisions `hunt_take`
/// actually hands the band, across every party size × all four policies × a healthy herd, a
/// **depleted herd where the biomass clamp binds**, and a collapsing (sub-Allee) herd — under both
/// a unit and a discontent-reduced output multiplier. If the two ever diverge, the client's
/// local-hunt preview is lying, and this test fails.
#[test]
fn exported_snapshot_fields_reproduce_band_hunt_take() {
    let mut app = build_headless_app();
    app.update();

    let collapse_fraction = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .ecology
        .collapse_fraction;

    let (healthy, depleted, collapsing) = {
        let registry = app.world.resource::<HerdRegistry>();
        let mut ids = registry.herds.iter().map(|h| h.id.clone());
        (
            ids.next().expect("map seeds at least three herds"),
            ids.next().expect("map seeds at least three herds"),
            ids.next().expect("map seeds at least three herds"),
        )
    };
    seed_herd(&mut app, &healthy, 0.9);
    let (_, depleted_biomass, depleted_cap) = seed_herd(&mut app, &depleted, DEPLETED_CAP_FRACTION);
    // Sub-Allee: Sustain/Surplus yield nothing there, so both sides must agree on a 0 take.
    seed_herd(&mut app, &collapsing, collapse_fraction * 0.5);
    let herds = [healthy, depleted, collapsing];

    // Pass 1: the shipped ecology levers, unit output multiplier (a content band).
    assert_band_preview_matches_hunt_take(&mut app, &herds, "shipped ecology, content band");

    // Pass 2: a discontented band — the exported `outputMultiplier` is now genuinely != 1.0.
    set_discontent(&mut app, BAND_DISCONTENT_FRACTION);
    assert_band_preview_matches_hunt_take(&mut app, &herds, "shipped ecology, discontented band");

    // Pass 3: the clamp-binding ecology (see `CLAMP_BINDING_REGROWTH_RATE`) — the depleted herd's
    // raw policy ceiling now exceeds its remaining biomass, so the exported ceiling MUST be clamped
    // to the biomass or the preview over-states the take. This is the case the pre-fix code failed.
    set_fauna_regrowth_rate(&mut app, CLAMP_BINDING_REGROWTH_RATE);
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    assert!(
        hunt_policy_ceiling(
            FollowPolicy::Surplus,
            depleted_biomass,
            depleted_cap,
            &fauna.ecology,
            &fauna
        ) > depleted_biomass,
        "the depleted-herd case must actually exercise the biomass clamp"
    );
    assert_band_preview_matches_hunt_take(&mut app, &herds, "clamp-binding ecology");
}
