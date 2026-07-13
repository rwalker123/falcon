//! Phase E husbandry: a sustained Sustain hunt on a Thriving herd tames it into domesticated
//! livestock (emergent accrual + decay), which then yields steady provisions and is immune to the
//! overhunting collapse. Uses the source-centric labor allocation (a Hunt assignment) that replaced
//! the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use bevy::math::UVec2;
use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventEntry, CommandEventKind,
    CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory,
    FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry, GenerationId,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
    HERDING_DISCOVERY_ID,
};

/// Whole-worker head-count assigned to the hunt — large enough that the per-worker biomass cap
/// never binds, so a Sustain hunt takes exactly the net regrowth (herd stays Thriving → accrues).
const HUNT_WORKERS: u32 = 5000;

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
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// A stationary game herd (route length 1) primed to half its cap → Thriving and a clean
/// domestication candidate. Returns its id.
fn prime_thriving_herd(app: &mut App) -> String {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
            .map(|h| h.id.clone())
            .expect("expected short-range game to spawn")
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = (herd.carrying_capacity * 0.5).max(1.0);
    id
}

fn spawn_hunter(app: &mut App, herd_id: &str, policy: FollowPolicy) -> bevy::prelude::Entity {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(HUNT_WORKERS as f32),
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
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        policy,
                    },
                    workers: HUNT_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One full turn's fauna pipeline in real stage order: Logistics (herds regrow, husbandry upkeep)
/// then Population (labor allocation resolves the hunt + accrues husbandry).
fn run_turns_with_hunt(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_labor_allocation);
    }
}

/// Turns with no active band: only the Logistics-stage systems run.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
}

fn progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0)
}

/// Total provisions carried by faction 0's bands (food is band-local now, so the husbandry yield
/// lands in the owner's cohort larders, not the faction pool).
fn provisions(app: &mut App) -> i64 {
    provisions_f32(app).round() as i64
}

/// Un-rounded total FOOD carried by faction 0's bands — needed to observe sub-1 fractional yields
/// that the rounding `provisions` helper would collapse to zero.
fn provisions_f32(app: &mut App) -> f32 {
    let mut total = 0.0f32;
    let mut query = app.world.query::<&PopulationCohort>();
    for cohort in query.iter(&app.world) {
        if cohort.faction == FactionId(0) {
            total += cohort.stores.get(FOOD).to_f32();
        }
    }
    total
}

/// A sustained Sustain hunt on a Thriving herd tames it: progress climbs to 1.0 (domesticated) and
/// the hunter's faction owns it.
#[test]
fn sustain_hunt_domesticates_thriving_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    // net accrual = progress_per_turn(0.04) - decay(0.01) = 0.03/turn → ~34 turns to 1.0.
    run_turns_with_hunt(&mut app, 45);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("domesticated herd persists");
    assert!(
        herd.is_domesticated(),
        "sustained Sustain hunt should domesticate: progress {}",
        herd.domestication_progress
    );
    assert_eq!(herd.owner, Some(FactionId(0)), "the hunter owns the herd");
    assert_eq!(registry.domesticated_count(FactionId(0)), 1);
}

/// Only a Sustain hunt tames; an Eradicate hunt never accrues husbandry.
#[test]
fn eradicate_hunt_does_not_domesticate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Eradicate);
    run_turns_with_hunt(&mut app, 10);
    assert_eq!(
        progress_of(&app, &id),
        0.0,
        "eradicate accrues no husbandry"
    );
}

/// Husbandry progress decays and ownership lapses once the herd isn't being tended.
#[test]
fn progress_decays_without_sustained_hunt() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    run_turns_with_hunt(&mut app, 6);
    let built = progress_of(&app, &id);
    assert!(built > 0.0, "some progress should have accrued");

    // Stop hunting, then let husbandry decay run.
    app.world.despawn(band);
    run_turns_untended(&mut app, 6);
    let decayed = progress_of(&app, &id);
    assert!(
        decayed < built,
        "progress should decay: {built} -> {decayed}"
    );

    // Decay all the way down clears ownership.
    run_turns_untended(&mut app, 60);
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("herd still exists");
    assert_eq!(herd.domestication_progress, 0.0);
    assert_eq!(herd.owner, None, "ownership lapses at zero progress");
}

/// A domesticated (managed) herd is immune to the overhunting collapse: driven below the Allee
/// threshold it recovers logistically instead of crashing to extinction.
#[test]
fn domesticated_herd_is_collapse_immune() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let low = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0)); // sets owner + progress = 1.0 → domesticated
                                                // Below the 15% collapse threshold — a wild herd here would crash.
        let low = herd.carrying_capacity * 0.10;
        herd.biomass = low;
        low
    };

    run_turns_untended(&mut app, 10);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&id)
        .expect("a domesticated herd never collapses to extinction");
    assert!(
        herd.biomass > low,
        "managed herd should recover, not crash: {low} -> {}",
        herd.biomass
    );
}

/// A domesticated herd yields steady provisions to its owner each turn without depleting.
#[test]
fn domesticated_herd_yields_provisions() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let biomass_before = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        herd.biomass
    };
    assert_eq!(provisions(&mut app), 0);

    app.world.run_system_once(advance_husbandry);

    assert!(
        provisions(&mut app) > 0,
        "a domesticated herd should pay its owner provisions"
    );
    // The yield is a sustainable harvest — it does not reduce the herd.
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert_eq!(
        after, biomass_before,
        "husbandry yield must not deplete biomass"
    );
}

// --- Corral (Intensification Rung 1c) -------------------------------------------------------------

/// Faction Herding knowledge for faction 0's ledger.
fn herding_knowledge(app: &App) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), HERDING_DISCOVERY_ID)
        .to_f32()
}

/// Complete faction 0's **Herding** knowledge — the `Corral` policy's gate — so a Corral assignment
/// actually accrues pen progress.
fn grant_herding(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), HERDING_DISCOVERY_ID, scalar_one());
}

/// The `Corral`-kind command-feed entries — the pen's whole life (completion AND escape) rides this
/// one kind.
fn corral_feed_lines(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| matches!(entry.kind, CommandEventKind::Corral))
        .cloned()
        .collect()
}

/// A herd's pen-construction progress (0 = no pen, 1.0 = built).
fn corral_progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.corral_progress)
        .unwrap_or(0.0)
}

/// Pen a herd: prime it to full biomass (Thriving, and at cap so logistic regrowth is 0 → a clean
/// no-draw-down check), domesticate it for faction 0, and corral it at its current tile.
fn corral_herd(app: &mut App, id: &str) -> UVec2 {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = herd.carrying_capacity;
    herd.claim_domestication(FactionId(0));
    let tile = herd.position();
    herd.corral_at(tile);
    tile
}

/// Rung 1c earned knowledge: a Sustain hunt on a Thriving herd teaches the faction **Herding** (the
/// `corral` gate), accrued in the shared `DiscoveryProgressLedger`.
#[test]
fn sustain_hunt_earns_herding_knowledge() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    assert_eq!(
        herding_knowledge(&app),
        0.0,
        "no faction starts knowing Herding"
    );

    run_turns_with_hunt(&mut app, 3);

    assert!(
        herding_knowledge(&app) > 0.0,
        "Sustain-hunting a Thriving herd earns Herding knowledge"
    );
}

/// A corralled herd does NOT roam: `advance_herds` leaves its position fixed at the pen tile (and
/// clears any heading arrow), even given a multi-tile route it would otherwise wander.
#[test]
fn corralled_herd_stops_roaming() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let pen = corral_herd(&mut app, &id);
    // Give it a route to a distant tile + prime it to step, so an un-penned herd would move.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.route = vec![pen, UVec2::new(pen.x.saturating_add(3), pen.y)];
        herd.step_index = 1;
        herd.dwell_remaining = 0;
    }

    for _ in 0..5 {
        app.world.run_system_once(advance_herds);
        let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
        assert_eq!(herd.position(), pen, "a corralled herd stays put");
        assert_eq!(herd.next_position(), None, "a penned herd shows no heading");
    }
}

/// A corralled herd tended by a Hunt assignment pays the keeper band place-local at the higher corral
/// rate WITHOUT drawing the herd down, and stays penned (does not escape).
#[test]
fn tended_corral_pays_keeper_place_local_no_drawdown() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let _pen = corral_herd(&mut app, &id);
    // The mobile even-split rate vs the higher penned rate.
    let (mobile_rate, corral_rate, cap) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let cap = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .unwrap()
            .carrying_capacity;
        (
            fauna.husbandry.provisions_per_biomass,
            fauna.husbandry.corral_provisions_per_biomass,
            cap,
        )
    };
    // A Hunt assignment on the penned herd = herding/tending it.
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    run_turns_with_hunt(&mut app, 4);

    let herd_biomass = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert!(
        (herd_biomass - cap).abs() < 1e-3,
        "a tended corral is a managed harvest — no draw-down: {herd_biomass} vs {cap}"
    );
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(&id)
            .unwrap()
            .is_corralled(),
        "a tended corral stays penned"
    );
    let paid = provisions_f32(&mut app);
    // Four turns of place-local corral yield; each ≈ cap × corral_rate (output mult 1.0).
    let expected_one_turn = cap * corral_rate;
    assert!(
        paid > expected_one_turn * 0.5,
        "the keeper collects the corral's place-local yield: {paid}"
    );
    // The corral rate out-pays the mobile even-split rate (the intensification incentive).
    assert!(
        corral_rate > mobile_rate,
        "penning pays more per biomass than mobile pastoralism"
    );
}

/// A corralled herd left untended **escapes**: `advance_husbandry` clears `corralled_at`, reverting it
/// to a mobile domesticated herd (which resumes the even-split yield).
#[test]
fn untended_corral_escapes_to_mobile() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    assert!(app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .is_corralled());

    // No keeper: the one-turn grace is consumed, then it breaks out.
    run_turns_untended(&mut app, 3);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(!herd.is_corralled(), "an untended corral escapes");
    assert!(
        herd.is_domesticated(),
        "an escaped herd is still domesticated — just mobile again"
    );
}

/// The one-turn grace holds: a **freshly-penned** herd is spared its first `advance_husbandry` pass
/// (`corral_at` marks it tended), so a keeper has a turn to take up the tending assignment.
#[test]
fn freshly_penned_herd_survives_its_grace_turn() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);

    run_turns_untended(&mut app, 1);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(
        herd.is_corralled(),
        "the grace turn spares a freshly-penned herd"
    );
    assert_eq!(
        herd.corral_progress, 1.0,
        "a spared pen keeps its completed progress"
    );
    assert!(
        corral_feed_lines(&app).is_empty(),
        "no escape line on the grace turn — nothing was lost"
    );
}

/// The escape **destroys a 25-turn investment**, so it must never be silent: it pushes a
/// `CommandEventKind::Corral` feed line naming the **species** (not the internal herd id) and saying
/// both what happened and why, with the machine-readable bits in the detail field.
#[test]
fn corral_escape_announces_the_lost_pen_in_the_feed() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let species = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .species
        .clone();

    run_turns_untended(&mut app, 3);

    let lines = corral_feed_lines(&app);
    assert_eq!(lines.len(), 1, "exactly one escape line: {lines:?}");
    let entry = &lines[0];
    assert_eq!(entry.faction, FactionId(0), "the owner is told");
    assert!(
        entry.label.contains(&species) && !entry.label.contains(&id),
        "the human line names the species, not the internal id: {}",
        entry.label
    );
    assert!(
        entry.label.contains("broke out") && entry.label.contains("pen is lost"),
        "the line says what happened AND why: {}",
        entry.label
    );
    let detail = entry.detail.as_deref().unwrap_or_default();
    assert!(
        detail.contains("status=escaped")
            && detail.contains("reason=untended")
            && detail.contains(&format!("herd={id}")),
        "the detail carries the machine-readable fields: {detail}"
    );
}

/// **The pen is lost, not merely opened.** An escaping herd resets `corral_progress` to `0.0`, so the
/// next `Corral` turn does NOT instantly re-pen it (at whatever tile it has roamed to) — the keeper
/// must pay the full rebuild investment again.
#[test]
fn escaped_corral_loses_its_pen_progress_and_must_rebuild() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let build_per_turn = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .husbandry
        .corral_build_progress_per_turn;

    // No keeper: the grace turn is consumed, then the herd breaks out.
    run_turns_untended(&mut app, 3);
    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(!herd.is_corralled(), "an untended corral escapes");
    assert_eq!(
        herd.corral_progress, 0.0,
        "the escaped herd's pen is lost — progress resets"
    );

    // A keeper returns under the Corral policy: it must REBUILD, not snap straight back to penned.
    grant_herding(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Corral);
    run_turns_with_hunt(&mut app, 1);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(
        !herd.is_corralled(),
        "one Corral turn must not instantly re-pen an escaped herd"
    );
    assert!(
        (herd.corral_progress - build_per_turn).abs() < 1e-4,
        "re-penning restarts the investment from zero: {} after one turn",
        herd.corral_progress
    );
}

/// Guard against over-reaching the escape fix: a **half-built** pen whose gate lapses (its keeper
/// leaves mid-build) **keeps** its progress — materials on the ground at a tile the herd is still at.
/// Only a *completed* pen that escapes loses it.
#[test]
fn half_built_pen_keeps_progress_when_its_keeper_leaves() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
    }
    grant_herding(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Corral);

    run_turns_with_hunt(&mut app, 5);
    let half_built = corral_progress_of(&app, &id);
    assert!(
        half_built > 0.0 && half_built < 1.0,
        "the pen should be part-built: {half_built}"
    );

    // The keeper walks off mid-build — the investment is NOT an escape and is NOT lost.
    app.world.despawn(band);
    run_turns_untended(&mut app, 5);

    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(
        !herd.is_corralled(),
        "a half-built pen never penned the herd"
    );
    assert_eq!(
        herd.corral_progress, half_built,
        "a mid-build lapse keeps its progress"
    );
}

/// Regression (fully-fractional FOOD income): a domesticated herd whose per-turn yield is below
/// 1.0 provisions (biomass 30 × `provisions_per_biomass` 0.01 = 0.3) must still credit the owner's
/// larder.
#[test]
fn sub_unit_husbandry_yield_credits_larder() {
    const SUB_UNIT_BIOMASS: f32 = 30.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        herd.biomass = SUB_UNIT_BIOMASS;
    }
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    app.world.run_system_once(advance_husbandry);

    let larder = provisions_f32(&mut app);
    assert!(
        larder > 0.0 && larder < 1.0,
        "a sub-1 husbandry yield must credit a positive fractional amount (got {larder})"
    );
}
