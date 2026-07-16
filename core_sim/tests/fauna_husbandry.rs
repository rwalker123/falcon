//! Phase E husbandry: a sustained Sustain hunt on a Thriving herd tames it into domesticated
//! livestock (emergent accrual + decay), which then yields steady provisions and is immune to the
//! overhunting collapse. Uses the source-centric labor allocation (a Hunt assignment) that replaced
//! the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use bevy::math::UVec2;
use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, herd_ecology, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventEntry,
    CommandEventKind, CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId,
    FactionInventory, FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry,
    GenerationId, GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, RungKey,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, WellbeingConfigHandle, FOOD, HERDING_DISCOVERY_ID,
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
    app.world.insert_resource(LadderConfigHandle::default());
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
                last_food_consumption: 0.0,
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

/// The live herd (panics if it despawned — every test here expects it to survive).
fn herd_of(app: &App, id: &str) -> Herd {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .cloned()
        .expect("herd persists")
}

/// Re-seat a herd at a chosen carrying capacity / biomass — how a *species*' K is put under test
/// without depending on which species the map happened to spawn.
fn reseat(app: &mut App, id: &str, cap: f32, biomass: f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.carrying_capacity = cap;
    herd.biomass = biomass;
    herd.refresh_ecology_phase(&fauna);
}

fn domesticate(app: &mut App, id: &str) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.claim_domestication(FactionId(0));
}

/// The single band's FOOD larder.
fn larder_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

/// The provisions the band's (only) assignment produced last turn — the retained yield telemetry, i.e.
/// what the sim *actually paid*, not a preview.
fn yield_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("band exists")
        .last_yields
        .first()
        .map(|y| y.actual)
        .unwrap_or(0.0)
}

/// Top the band's larder up to `amount` (so a keeper can always pay its pen's feed).
fn stock_larder(app: &mut App, band: bevy::prelude::Entity, amount: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists");
    cohort.stores.set(FOOD, scalar_from_f32(amount));
}

/// Empty the band's larder (so a keeper *cannot* pay its pen's feed → the herd starves).
fn drain_larder(app: &mut App, band: bevy::prelude::Entity) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists");
    cohort.stores.set(FOOD, scalar_zero());
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

/// **You are not paid twice for the same animals.** The passive pastoral rung is what a herd pays when
/// *nobody* is working it; a band with a labor assignment on it is already paid through the Hunt arm.
/// Paying both stacks them — and it is what turned the corral's *investment cost* into a profit.
#[test]
fn a_domesticated_herd_worked_by_labor_is_not_also_paid_the_passive_rung() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);

    // Nobody working it → the passive rung pays.
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_husbandry);
    let passive = provisions_f32(&mut app);
    assert!(
        passive > 0.0,
        "an unworked tame herd pays its owner passively"
    );

    // Now a band works it (any policy). Population sets `worked_this_turn`; the NEXT Logistics
    // `advance_husbandry` must skip the passive payment (the deliberate one-turn lag).
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    app.world.run_system_once(advance_labor_allocation);
    drain_larder(&mut app, band);
    let before = provisions_f32(&mut app);

    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_husbandry);

    let passive_while_worked = provisions_f32(&mut app) - before;
    assert!(
        passive_while_worked.abs() < 1e-4,
        "a herd worked by labor must NOT also collect the passive rung (got {passive_while_worked})"
    );
}

/// **The Corral build is a genuine net LOSS while it runs** — that is the investment the whole
/// intensification ladder is built on. Before the no-double-pay fix the builder collected the dip
/// (0.25 × MSY) *plus* the passive rung (MSY), i.e. **more** than walking away — corralling was pure
/// upside and there was no decision.
#[test]
fn building_a_corral_costs_more_than_walking_away() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;

    // (a) Walk away: nobody works the tame herd → it pays the full passive pastoral rung.
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_husbandry);
    let walk_away = provisions_f32(&mut app);

    // (b) Build the pen: a band works the same herd under Corral. It collects the dip and NOTHING
    // else — the passive rung is skipped because the band is working the herd.
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    grant_herding(&mut app);
    let builder = spawn_hunter(&mut app, &id, FollowPolicy::Corral);
    // Turn 1 seeds `worked_this_turn`; turn 2 is the steady state (the passive rung is skipped).
    run_turns_with_hunt(&mut app, 1);
    drain_larder(&mut app, builder);
    let before = provisions_f32(&mut app);
    run_turns_with_hunt(&mut app, 1);
    let building = provisions_f32(&mut app) - before;

    let dip_fraction = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPen)
        .yield_fraction_while_building()
        .expect("the pen rung is an investment");
    assert!(
        (building - dip_fraction * walk_away).abs() < walk_away * 0.05,
        "building pays only the dip ({dip_fraction} × the pastoral MSY {walk_away}): got {building}"
    );
    assert!(
        building < walk_away,
        "**the pen must COST something**: building ({building}/turn) has to be a real loss against \
         walking away ({walk_away}/turn), or corralling is free and there is no decision"
    );
}

/// **The pastoral rung pays MSY, and the harvest DRAWS THE HERD DOWN** — which is what makes it
/// sustainable (the flow-based ladder, `docs/plan_corral_managed_population.md`). It is still passive
/// (no worker) and still split across the owner's bands; it is just no longer a share of standing
/// *stock* that printed food forever.
#[test]
fn domesticated_herd_harvests_its_pastoral_msy_and_draws_the_herd_down() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let (biomass_before, cap) = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        // At capacity: MSY = r·K/4 (the ceiling plateaus above K/2).
        herd.biomass = herd.carrying_capacity;
        (herd.biomass, herd.carrying_capacity)
    };
    assert_eq!(provisions(&mut app), 0);

    app.world.run_system_once(advance_husbandry);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    // Per-species pastoral rate (Grazing 2d): read the same seam the sim harvests through.
    let pastoral_r = herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate;
    let expected_take = pastoral_r * cap / 4.0;
    let expected_provisions = expected_take * fauna.hunt.provisions_per_biomass;
    drop(fauna);

    let paid = provisions_f32(&mut app);
    assert!(
        (paid - expected_provisions).abs() < expected_provisions * 0.02,
        "the pastoral yield is the pastoral MSY: expected {expected_provisions}, got {paid}"
    );
    // **The premise that used to be false:** the managed harvest is a real take out of the herd.
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert!(
        (biomass_before - after - expected_take).abs() < expected_take * 0.02,
        "the harvest draws the herd down by exactly its MSY: {biomass_before} -> {after}"
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

/// **The pen is a managed population.** A tended corral harvests the *pen's* MSY (`r` = 0.60) each
/// turn, which **draws the herd down** — and that is exactly what makes it sustainable: taking MSY
/// while the herd regrows logistically converges it on `K_pen/2` and holds it there, paying `r·K/4`
/// forever. (The retired flat rate never drew the herd down at all: a penned herd parked at capacity
/// and printed food.)
#[test]
fn tended_corral_harvests_msy_and_settles_at_half_capacity() {
    const CONVERGENCE_TURNS: u32 = 80;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let cap = herd_of(&app, &id).carrying_capacity;
    let (pen_r, prov_rate) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        // Per-species pen rate (Grazing 2d): the corralled herd's own rung, via the shared seam.
        (
            herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate,
            fauna.hunt.provisions_per_biomass,
        )
    };
    // MSY = r·K/4 (the ceiling plateaus for any biomass at or above K/2).
    let msy_provisions = pen_r * cap / 4.0 * prov_rate;

    // A Hunt assignment on the penned herd = herding/tending it. Keep its larder stocked so the pen's
    // feed is always paid (the starvation path has its own test).
    let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    stock_larder(&mut app, keeper, cap);

    let mut last_yield = 0.0f32;
    for _ in 0..CONVERGENCE_TURNS {
        stock_larder(&mut app, keeper, cap); // never let the feed run out
        run_turns_with_hunt(&mut app, 1);
        last_yield = yield_of(&app, keeper);
    }

    let herd = herd_of(&app, &id);
    assert!(herd.is_corralled(), "a tended corral stays penned");
    // **Converged on the MSY point**, not parked at capacity.
    assert!(
        (herd.biomass - cap * 0.5).abs() < cap * 0.05,
        "a harvested pen settles at K/2 ({}): got {}",
        cap * 0.5,
        herd.biomass
    );
    // ...and it pays the full MSY there, stably, forever.
    assert!(
        (last_yield - msy_provisions).abs() < msy_provisions * 0.05,
        "the settled pen pays r·K/4 × p = {msy_provisions}: got {last_yield}"
    );
}

/// **The pen EATS.** Its keeper's larder is debited exactly `pen.upkeep_per_biomass × biomass` every
/// turn it tends — a confined herd cannot graze, so the keeper brings it food.
#[test]
fn tending_a_pen_debits_the_keepers_larder_by_its_upkeep() {
    const STOCK: f32 = 500.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let biomass = herd_of(&app, &id).biomass;
    let (upkeep_rate, pen_r, prov_rate) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            fauna.husbandry.pen.upkeep_per_biomass,
            // Per-species pen rate (Grazing 2d): the corralled herd's own rung.
            herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate,
            fauna.hunt.provisions_per_biomass,
        )
    };
    let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    stock_larder(&mut app, keeper, STOCK);

    // One Population turn only, so the herd's biomass (and thus the demand) is the one we measured.
    app.world.run_system_once(advance_labor_allocation);

    let expected_upkeep = upkeep_rate * biomass;
    let gross = yield_of(&app, keeper);
    let expected_gross = pen_r * herd_of(&app, &id).carrying_capacity / 4.0 * prov_rate;
    assert!(
        (gross - expected_gross).abs() < expected_gross * 0.02,
        "the credited yield is GROSS (upkeep is a separate debit): {gross} vs {expected_gross}"
    );
    // larder = stock − upkeep + gross yield.
    let expected_larder = STOCK - expected_upkeep + gross;
    let larder = larder_of(&app, keeper);
    assert!(
        (larder - expected_larder).abs() < 0.05,
        "the pen debits exactly upkeep_per_biomass × biomass ({expected_upkeep}): \
         larder {larder} vs expected {expected_larder}"
    );
    assert!(
        expected_upkeep > 0.0 && expected_upkeep < gross,
        "the pen must cost real food, and still net positive: upkeep {expected_upkeep}, yield {gross}"
    );
}

/// **An underfed pen starves — and recovers.** A keeper with an empty larder cannot pay the feed, so
/// the herd shrinks (its yield falling with it) and floors at the extinction floor rather than
/// despawning or losing the pen. Feed it again and it grows back. Starving your animals to feed your
/// people is a *decision*, not an accident.
#[test]
fn an_underfed_pen_shrinks_to_a_remnant_then_recovers_when_fed() {
    const STARVE_TURNS: u32 = 40;
    const RECOVER_TURNS: u32 = 30;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let cap = herd_of(&app, &id).carrying_capacity;
    let floor = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        fauna.husbandry.pen.ecology.extinction_floor * cap
    };
    let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    // Starve it: drain the keeper's larder every turn, so the feed can never be paid.
    let mut previous = herd_of(&app, &id).biomass;
    for _ in 0..STARVE_TURNS {
        drain_larder(&mut app, keeper);
        run_turns_with_hunt(&mut app, 1);
        let now = herd_of(&app, &id).biomass;
        assert!(
            now <= previous + 1e-3,
            "an unfed pen must never grow: {previous} -> {now}"
        );
        previous = now;
    }

    let starved = herd_of(&app, &id);
    assert!(
        starved.is_corralled(),
        "a starved pen is NOT lost — it withers"
    );
    assert!(
        (starved.biomass - floor).abs() < floor * 0.05,
        "a starved herd converges on the extinction floor ({floor}), not zero and not oscillating: {}",
        starved.biomass
    );
    // The famine is announced exactly once (edge-gated), naming the species, never the internal id.
    let lines = corral_feed_lines(&app);
    let starving: Vec<_> = lines
        .iter()
        .filter(|e| {
            e.detail
                .as_deref()
                .unwrap_or_default()
                .contains("status=starving")
        })
        .collect();
    assert_eq!(starving.len(), 1, "the famine is announced ONCE: {lines:?}");
    assert!(
        starving[0].label.contains(&starved.species) && starving[0].label.contains("starving"),
        "the line names the species and says what happened: {}",
        starving[0].label
    );

    // Feed it again → it recovers (the pen's r = 0.60 is the fastest curve on the ladder).
    let remnant = herd_of(&app, &id).biomass;
    for _ in 0..RECOVER_TURNS {
        stock_larder(&mut app, keeper, cap);
        run_turns_with_hunt(&mut app, 1);
    }
    let recovered = herd_of(&app, &id);
    assert!(
        recovered.biomass > remnant * 2.0,
        "a re-fed pen recovers: {remnant} -> {}",
        recovered.biomass
    );
    assert!(recovered.is_corralled(), "and it still has its pen");
}

/// **The husbandry ladder, per species — now a per-species GROWTH-RATE ladder (Grazing 2d §3).**
/// Rabbit Warren (K=200) → Red Deer (K=1200) → Thunder Mammoths (K=12000), each measured at its **own
/// per-species wild `r`** (rabbit 0.35, deer 0.10, mammoth 0.04). Every number below is **measured from
/// a real sim run** (a band's actual take / a real larder debit), never arithmetic.
///
/// **2d retires the flat pastoral 0.25 / pen 0.90 and the fast-breeder pastoral inversion with them.**
/// The managed rungs now scale each species' own wild `r` (`pastoral_gain` 1.5, `pen_gain` 3.0, capped
/// at 0.75), so `pastoral_r = wild_r × 1.5 > wild_r` for **every** species — the pastoral rung out-pays
/// wild Sustain unconditionally, and the pen's GROSS growth-rate tops pastoral unconditionally. That
/// GROSS ladder (`wild < pastoral < pen_gross`) is what "management buys a growth rate" means, and it is
/// the invariant asserted here.
///
/// **What 2d does NOT guarantee at the BARREN worst case** (this harness runs no graze layer, so the pen
/// is fully larder-fed): the pen's *net* payoff over pastoral. A penned herd normally grazes its fenced
/// footprint and the larder pays only the shortfall (§2.3), so on real pasture `upkeep → 0` and
/// `pen_net → pen_gross`, topping pastoral. Fully larder-fed, the feed is a real cost that can erase the
/// advantage — and for a slow breeder the barren pen is a **net loss by design** (§2.4: mammoth pen
/// `r ≈ 0.12`, feed > yield). `FaunaConfig::validate` enforces only a best-case floor (the *fastest*
/// breeder stays net-positive even fully larder-fed); the rest is a placement decision, not a config
/// error. So this test asserts the GROSS growth-rate ladder + that the barren pen costs real feed, and
/// records `pen_net` for observability rather than asserting it tops pastoral (which self-feeding, not
/// this harness, delivers).
#[test]
fn the_husbandry_ladder_is_a_per_species_growth_rate_ladder() {
    const MEASURE_STOCK: f32 = 50_000.0;

    // (display, cap, per-species wild r) — the wild rung must be measured at each species' OWN r.
    let species_caps: Vec<(String, f32, f32)> = {
        let fauna = FaunaConfigHandle::default().get();
        let wild_default = fauna.ecology.regrowth_rate;
        ["rabbit", "deer", "mammoth"]
            .iter()
            .map(|key| {
                let def = &fauna.species[*key];
                (
                    def.display_name.clone(),
                    def.carrying_capacity(),
                    def.regrowth_rate_or(wild_default),
                )
            })
            .collect()
    };

    // Measured twice: **at capacity** (a freshly-penned herd, `B = K`) and at the **settled operating
    // point** (`B* = K/2` — where a harvested herd actually converges; the point the pen's
    // net-positive invariant is derived against). Every row runs a **full turn in real stage order**
    // (Logistics: `advance_herds` regrows → `advance_husbandry`; Population:
    // `advance_labor_allocation`), so the numbers are what the sim pays, not what a single system does
    // in isolation. The gross yields match at both biomasses (the MSY ceiling plateaus above `K/2`);
    // the **feed** is what differs, because the feed follows the herd — and it is charged on the
    // *post-regrowth* biomass (you feed every animal in the pen, including the ones you are about to
    // harvest).
    for (label, biomass_fraction) in [
        ("at capacity (B = K)", 1.0f32),
        ("at the settled operating point (B* = K/2)", 0.5f32),
    ] {
        println!("\n=== husbandry ladder, MEASURED {label} (provisions/turn) ===");
        println!(
            "{:<18} {:>8} {:>9} {:>9} {:>11} {:>9} {:>9}",
            "species", "K", "wild", "pastoral", "pen gross", "upkeep", "pen net"
        );
        for (species, cap, wild_r) in &species_caps {
            let (species, cap, wild_r) = (species.clone(), *cap, *wild_r);
            let biomass = cap * biomass_fraction;

            // --- Wild Sustain: a band hunting a wild herd — its ACTUAL take, from the yield telemetry.
            // Seat the herd at THIS species' per-species wild `r` (2b-ii), since the spawned short-range
            // game the harness reuses carries its own rate.
            let mut app = spawn_world();
            let id = prime_thriving_herd(&mut app);
            reseat(&mut app, &id, cap, biomass);
            set_wild_regrowth_rate(&mut app, &id, wild_r);
            let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
            run_turns_with_hunt(&mut app, 1);
            let wild = yield_of(&app, band);

            // --- Pastoral (passive, no worker): the faction's larder credit. The yield is split
            // evenly across ALL the owner's bands (the start profile spawns some too), so measure the
            // faction total, not one band's larder.
            let mut app = spawn_world();
            let id = prime_thriving_herd(&mut app);
            reseat(&mut app, &id, cap, biomass);
            domesticate(&mut app, &id);
            app.world.run_system_once(advance_herds);
            app.world.run_system_once(advance_husbandry);
            let pastoral = provisions_f32(&mut app);

            // --- Pen: the gross yield credited + the feed debited, both read off the keeper's larder.
            let mut app = spawn_world();
            let id = prime_thriving_herd(&mut app);
            reseat(&mut app, &id, cap, cap);
            corral_herd(&mut app, &id);
            reseat(&mut app, &id, cap, biomass); // corral_herd seats at cap; re-seat for B*
            let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
            stock_larder(&mut app, keeper, MEASURE_STOCK);
            run_turns_with_hunt(&mut app, 1);
            let pen_gross = yield_of(&app, keeper);
            // larder = stock − upkeep + gross ⇒ upkeep = stock + gross − larder.
            let upkeep = MEASURE_STOCK + pen_gross - larder_of(&app, keeper);
            let pen_net = pen_gross - upkeep;

            println!(
                "{species:<18} {cap:>8.0} {wild:>9.3} {pastoral:>9.3} {pen_gross:>11.3} {upkeep:>9.3} {pen_net:>9.3}"
            );

            assert_growth_rate_ladder(&species, wild_r, wild, pastoral, pen_gross, upkeep, pen_net);
        }
    }
    println!();
}

/// The **per-species GROWTH-RATE ladder** (Grazing 2d §3), asserted on **measured** numbers. Since the
/// managed rungs now scale each species' own wild `r` (`pastoral_gain` 1.5 < `pen_gain` 3.0, capped),
/// the ladder is monotone in GROSS yield for **every** species — the old fast-breeder pastoral
/// inversion is gone. The pen's *net* payoff over pastoral is realized by SELF-FEEDING (this barren
/// harness runs the pen fully larder-fed, so it only asserts the pen costs real feed; the net-positive
/// floor for the fastest breeder lives in `fauna_config`'s validate tests).
fn assert_growth_rate_ladder(
    species: &str,
    wild_r: f32,
    wild: f32,
    pastoral: f32,
    pen_gross: f32,
    upkeep: f32,
    pen_net: f32,
) {
    assert!(
        wild > 0.0,
        "{species}: a thriving wild herd has a positive Sustain MSY ({wild})"
    );
    // The fast-breeder pastoral inversion is FIXED (2d §3): pastoral r = wild_r × 1.5 > wild_r for
    // every species, so the pastoral rung out-pays wild Sustain unconditionally.
    assert!(
        pastoral > wild,
        "{species}: pastoral ({pastoral}) out-pays wild Sustain ({wild}) — per-species pastoral r = \
         wild r ({wild_r}) × pastoral_gain > wild r"
    );
    // Management buys a growth rate: the pen's GROSS yield tops the pastoral rung for every species.
    assert!(
        pen_gross > pastoral,
        "{species}: the pen's GROSS yield ({pen_gross}) tops the pastoral rung ({pastoral})"
    );
    // The barren pen costs real feed — the worst-case cost self-feeding removes (§2.3). `pen_net` is
    // recorded (it may sit below pastoral, or negative for a slow breeder, BY DESIGN — §2.4).
    assert!(
        upkeep > 0.0,
        "{species}: the barren pen costs real feed ({upkeep}); net of it = {pen_net}"
    );
}

/// Seat a herd's cached per-species **wild** regrowth rate (Grazing 2b-ii) — the wild rung is measured
/// at each species' own `r`, and the harness reuses one spawned short-range herd for every row.
fn set_wild_regrowth_rate(app: &mut App, id: &str, r: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.regrowth_rate = r;
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
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPen)
        .build_accrual(FollowPolicy::Corral, true);

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

/// **The lost pen tears down its whole FENCE, not just the build meter** (Grazing 2d). A completed pen
/// with a grown radius (and even a ring mid-extension) that escapes untended resets `pen_radius` /
/// `pen_extend_progress` / `pen_extending` to defaults, so a re-corralled herd starts at radius 0 —
/// it does NOT inherit its old fenced radius for free (skipping the ~25-turn-per-ring ExtendPen labor).
#[test]
fn escaped_corral_resets_the_fenced_footprint_no_free_extension() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    // Give the completed pen a grown, mid-extension fence.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.pen_radius = 2;
        herd.pen_extend_progress = 0.5;
        herd.pen_extending = true;
        // A lush pasture share left over from being penned — must not survive going mobile.
        herd.pen_pasture_fraction = 1.0;
    }

    // No keeper: the grace turn is consumed, then the herd breaks out.
    run_turns_untended(&mut app, 3);
    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(!herd.is_corralled(), "an untended corral escapes");
    assert_eq!(
        herd.pen_radius, 0,
        "the lost pen's fence is torn down to radius 0"
    );
    assert!(!herd.pen_extending, "its in-flight extension is cancelled");
    assert_eq!(herd.pen_extend_progress, 0.0, "with zero ring progress");
    assert_eq!(
        herd.pen_pasture_fraction, 0.0,
        "and the stale penned pasture share is cleared (the wire's '0.0 when unpenned' contract)"
    );

    // Re-corralling comes back at radius 0 — the old fence is NOT inherited for free.
    corral_herd(&mut app, &id);
    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(herd.is_corralled(), "the herd re-pens");
    assert_eq!(
        herd.pen_radius, 0,
        "a re-corralled herd starts at radius 0 — no free extension"
    );
    assert!(!herd.pen_extending);
    assert_eq!(herd.pen_extend_progress, 0.0);
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

/// Regression (fully-fractional FOOD income): a small domesticated herd whose per-turn MSY harvest is
/// below 1.0 provisions must still credit the owner's larder (rounding to an i64 used to drop it).
/// Seeded above the Allee threshold (0.15 × K) and below the MSY point, so the harvest is a genuine
/// sub-unit flow rather than zero.
#[test]
fn sub_unit_husbandry_yield_credits_larder() {
    /// Just above the MSY/escapement point (`K/2`): the managed harvest takes the thin standing
    /// surplus above it, which is a fraction of a provision for every shipped species.
    const SUB_UNIT_CAP_FRACTION: f32 = 0.52;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap * SUB_UNIT_CAP_FRACTION);
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    app.world.run_system_once(advance_husbandry);

    let larder = provisions_f32(&mut app);
    assert!(
        larder > 0.0 && larder < 1.0,
        "a sub-1 husbandry yield must credit a positive fractional amount (got {larder})"
    );
}
