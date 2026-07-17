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
    TileRegistry, WellbeingConfigHandle, FOOD, FULLY_HERDED, HERDING_DISCOVERY_ID,
    MSY_BIOMASS_FRACTION, PENNING_DISCOVERY_ID, RUNG_COMPLETE, RUNG_TIMESCALE_UNSCALED,
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

/// Hand a herd a **completed, staffed** pastoral rung — the "give me a tamed herd" fixture.
///
/// **Both halves are needed since slice 8.** `accrue_domestication` fills the meter, but a tamed herd
/// now also demands *herders* every turn (`fauna::herders_needed`), and `advance_husbandry` (Logistics)
/// runs **before** the labor arm (Population) that would staff it. So a herd handed only the meter is
/// read as *unherded* on its very first turn, decays a step, drops under the `>= 1.0` bar, and every
/// row measuring it silently measures a **wild** herd instead. Seating `herded_fraction` too says "a
/// crew was already with them last turn", which is exactly the state the fixture is claiming.
fn domesticate(app: &mut App, id: &str) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
    herd.herded_fraction = FULLY_HERDED;
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

/// **The `Tame` verb tames** — sustained work under `FollowPolicy::Tame` on a Thriving herd climbs
/// `domestication_progress` to 1.0 and the taming faction owns it.
///
/// **Retargeted from `sustain_hunt_domesticates_thriving_herd`.** The guarantee "sustained work on a
/// Thriving herd domesticates it, and the worker owns it" is *preserved verbatim* — only the verb
/// that earns it changed, from the `Sustain` harvest policy to the explicit `Tame` investment
/// (`plan_intensification_ladder.md` §4.1: taming was a hidden side effect of a harvest policy; it is
/// now a paid verb). Its inverse — that Sustain *no longer* does this — is
/// `sustain_hunt_no_longer_tames_it_only_teaches_herding` below.
#[test]
fn tame_policy_domesticates_thriving_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    grant_herding(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Tame);

    // A herd under active taming is spared the decay pass (`tamed_this_turn`, mirroring a patch under
    // Cultivate), so it accrues the FULL progress_per_turn(0.04) → 25 turns at the rung's own pace.
    // The map picks the species, and slice 3c makes the *timescale* per-species (a `taming_rate` 0.2
    // herd needs 125), so run the slowest tameable row's worth of turns rather than a bare 30 —
    // this test is about "sustained Tame work tames and the tamer owns it", not about the pace.
    let slowest_taming_rate = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        fauna
            .species
            .values()
            .map(|def| def.taming_rate)
            .fold(f32::INFINITY, f32::min)
    };
    run_turns_with_hunt(&mut app, (30.0 / slowest_taming_rate).ceil() as u32);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("domesticated herd persists");
    assert!(
        herd.is_domesticated(),
        "sustained Tame work should domesticate: progress {}",
        herd.domestication_progress
    );
    assert_eq!(herd.owner, Some(FactionId(0)), "the tamer owns the herd");
    assert_eq!(registry.domesticated_count(FactionId(0)), 1);
}

/// **Sustain no longer tames anything — it only TEACHES.** The §4.1 de-conflation, at the sim level:
/// the one `Sustain` branch that used to advance Herding knowledge *and* `accrue_domestication` now
/// does only the former, exactly mirroring the plant side's Sustain→Cultivation branch.
///
/// This is the inverse half of the retargeted `sustain_hunt_domesticates_thriving_herd`: run the
/// *same* herd under the *same* policy for well past the old ~34-turn taming horizon and assert the
/// meter never moves — while the knowledge it *does* earn climbs to complete.
#[test]
fn sustain_hunt_no_longer_tames_it_only_teaches_herding() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    // Far past the 34 turns that used to be a full taming under the old conflated branch.
    run_turns_with_hunt(&mut app, 45);

    let herd = herd_of(&app, &id);
    assert_eq!(
        herd.domestication_progress, 0.0,
        "a Sustain hunt must never tame — Tame is the taming verb"
    );
    assert_eq!(
        herd.owner, None,
        "a Sustain hunt must not claim ownership of a wild herd either"
    );
    // ...but it DOES teach, which is the whole point: Sustain is how you earn the Tame verb.
    assert!(
        app.world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FactionId(0), HERDING_DISCOVERY_ID)
            >= scalar_one(),
        "Sustain-hunting a Thriving herd must still teach the faction Herding"
    );
}

/// **The Tame take dips to the rung's fraction.** Taming costs yield — the crew is gentling the herd,
/// not harvesting it — so the take ceiling is the `animal:pastoral` rung's
/// `yield_fraction_while_building × the herd's Sustain (MSY) ceiling`. Asserted against the *same*
/// herd state under Sustain, so it is a true ratio and not a pinned magic number.
#[test]
fn the_tame_take_dips_to_the_rungs_yield_fraction() {
    let dip = {
        let app = spawn_world();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        ladder
            .rung(RungKey::AnimalPastoral)
            .yield_fraction_while_building()
            .expect("the pastoral rung is an investment")
    };

    // One turn of Sustain (the MSY baseline) vs one turn of Tame, from an identical start.
    let harvest = |policy: FollowPolicy| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        grant_herding(&mut app);
        spawn_hunter(&mut app, &id, policy);
        let before = provisions_f32(&mut app);
        run_turns_with_hunt(&mut app, 1);
        provisions_f32(&mut app) - before
    };

    let sustained = harvest(FollowPolicy::Sustain);
    let tamed = harvest(FollowPolicy::Tame);
    assert!(sustained > 0.0, "the Sustain baseline must pay something");
    assert!(
        (tamed - sustained * dip).abs() < sustained * 0.02,
        "Tame must pay the rung's dip: expected ~{}, got {tamed}",
        sustained * dip
    );
}

/// Re-badge a primed herd **as another species** — the display name is what
/// `FaunaConfig::taming_rate_for` resolves, so this puts the *same herd, on the same code path*, at a
/// different species' taming timescale with one dial changed and nothing else. The husbandry ceiling
/// is taken from the same roster row, so the fixture can never be an incoherent species (a herd that
/// tames at a ceiling its species forbids). Everything the turn loop keys off the herd itself
/// (`size_class` → graze range → `K`, `fodder_per_biomass`, `regrowth_rate`) is untouched, so the
/// ecology under the two runs is identical and only the taming rate differs.
fn rebadge_as(app: &mut App, id: &str, species_key: &str) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let def = fauna
        .species
        .get(species_key)
        .expect("the roster defines the species under test");
    let (display, ceiling) = (def.display_name.clone(), def.husbandry_ceiling);
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.species = display;
    herd.husbandry_ceiling = ceiling;
}

/// Turns of sustained `Tame` work before the herd is domesticated (capped, so a species that can
/// never tame fails loudly instead of hanging).
fn turns_to_tame(species_key: &str, cap_turns: u32) -> u32 {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    rebadge_as(&mut app, &id, species_key);
    grant_herding(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Tame);
    for turn in 1..=cap_turns {
        run_turns_with_hunt(&mut app, 1);
        if herd_of(&app, &id).is_domesticated() {
            return turn;
        }
    }
    panic!("{species_key} never tamed within {cap_turns} turns");
}

/// **Taming speed is a PER-SPECIES dial on one shared rung** (slice 3c). Before it, the
/// `animal:pastoral` rung's single `progress_per_turn` tamed every animal in the same 25 turns — a
/// rabbit cost what a Steppe Runner cost. Now the rung owns the mechanic and the species scales it:
/// a quick, forgiving warren is 25 turns; binding a large migratory herd is generational (125).
///
/// Same herd, same verb, same code path — one dial apart (`rebadge_as`).
#[test]
fn taming_speed_is_a_per_species_dial_on_the_shared_rung() {
    // `taming_rate` 1.0 → the rung's own pace.
    let rabbit = turns_to_tame("rabbit", 40);
    // `taming_rate` 0.2 → a 5× longer build. (Also the case that proves the multiplier scales the
    // whole *timescale*: at 0.04 × 0.2 = 0.008/turn against the rung's 0.01/turn decay, a
    // progress-only multiplier would leave this species literally untameable.)
    let steppe_runner = turns_to_tame("steppe_runner", 160);

    assert!(
        rabbit.abs_diff(25) <= 1,
        "a rabbit tames at the rung's own pace (~25 turns), took {rabbit}"
    );
    assert!(
        steppe_runner.abs_diff(125) <= 1,
        "a steppe runner tames 5× slower (~125 turns), took {steppe_runner}"
    );
}

/// **The multiplier is a TIMESCALE, not a speed — the decay scales with it.** From the *same*
/// partial taming, a Steppe Runner bleeds progress ~5× slower than a rabbit: slow to tame, slow to
/// forget. This is what keeps the rung's build:decay ratio invariant per species (and is why the
/// ladder's "taming must out-run its decay" bound needs no per-species restatement).
#[test]
fn a_slow_taming_species_is_equally_slow_to_forget() {
    /// A partial taming both species start the decay from — well clear of the `0.0` clamp, so
    /// neither run bottoms out and flatters the ratio.
    const PARTIAL_TAMING: f32 = 0.5;
    const ABANDONED_TURNS: u32 = 10;

    let progress_lost = |species_key: &str| -> f32 {
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        rebadge_as(&mut app, &id, species_key);
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
            herd.accrue_domestication(FactionId(0), PARTIAL_TAMING);
        }
        assert_eq!(progress_of(&app, &id), PARTIAL_TAMING);
        // Nobody is working it: the herd goes feral at its species' own decay.
        run_turns_untended(&mut app, ABANDONED_TURNS);
        PARTIAL_TAMING - progress_of(&app, &id)
    };

    let rabbit = progress_lost("rabbit");
    let steppe_runner = progress_lost("steppe_runner");
    assert!(
        rabbit > 0.0 && steppe_runner > 0.0,
        "an abandoned part-tamed herd of either species must bleed: {rabbit} / {steppe_runner}"
    );
    assert!(
        (rabbit / steppe_runner - 5.0).abs() < 0.05,
        "a steppe runner must forget 5× slower than a rabbit: lost {rabbit} vs {steppe_runner}"
    );
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

/// **Abandoning a taming decays the meter**, and ownership lapses at zero — the herd goes feral, the
/// animal mirror of an abandoned Cultivate patch reverting.
///
/// **Retargeted from `progress_decays_without_sustained_hunt`:** the guarantee (partial taming bleeds
/// away when you walk off, and a herd at zero progress keeps no stale owner) is unchanged; the meter
/// is now *built* by `Tame` rather than by `Sustain`, so that is what gets abandoned. The decay rate
/// itself now comes off the `animal:pastoral` rung, not `fauna_config.husbandry`.
#[test]
fn abandoning_a_tame_decays_the_progress() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    grant_herding(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Tame);
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
        herd.accrue_domestication(FactionId(0), RUNG_COMPLETE); // sets owner + progress = 1.0 → domesticated
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

/// **A pastoral herd pays NOTHING without workers — every rung is worker-driven**
/// (`docs/plan_intensification_ladder.md` §3, slice 3b).
///
/// **Retargeted from `a_domesticated_herd_worked_by_labor_is_not_also_paid_the_passive_rung`.** That
/// test guarded the no-double-pay skip: the passive rung had to be withheld from a herd a band was
/// already working, because paying both turned the corral's *investment cost* into a profit. Retiring
/// passive-free pastoral makes that guarantee **structural** — there is no second payment left to
/// stack — so this asserts the stronger thing the same run measures: an unworked tamed herd earns its
/// owner nothing at all, and is not even drawn down. A tamed herd is livestock, not an annuity; it is
/// worked, or it is idle capital. (What the *workers* then get is the pastoral rung's 1.5× `r` — see
/// `the_husbandry_ladder_is_a_per_species_growth_rate_ladder`.)
#[test]
fn a_pastoral_herd_pays_nothing_without_workers() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    assert_eq!(provisions_f32(&mut app), 0.0, "larders start empty");

    // Nobody works it. Run the herd's whole Logistics pipeline for a long stretch — the old passive
    // rung would have printed its MSY into the owner's larders every one of these turns.
    run_turns_untended(&mut app, 20);

    assert_eq!(
        provisions_f32(&mut app),
        0.0,
        "an unworked tame herd must yield its owner NOTHING — the passive rung is retired"
    );
    assert!(
        (herd_of(&app, &id).biomass - cap).abs() < cap * 1e-3,
        "and nothing harvested it, so it sits at capacity"
    );
}

/// **The Corral build is a genuine net LOSS while it runs** — the investment the whole intensification
/// ladder is built on.
///
/// **Retargeted baseline, same guarantee.** The comparison used to be "building the pen vs *walking
/// away*", because a tamed herd left alone paid its owner the passive rung for free; that free path is
/// what made the dip a profit before the no-double-pay fix, and slice 3b deletes it outright (walking
/// away now pays **0**, which `a_pastoral_herd_pays_nothing_without_workers` pins). So the baseline is
/// now the *real* alternative use of the same crew: **Sustain-hunting that same tamed herd**. The
/// guarantee is unchanged and if anything sharper — the pen must cost the builder something against
/// the best thing those workers could otherwise be doing on this herd, or there is no decision.
#[test]
fn building_a_corral_costs_more_than_hunting_the_same_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;

    // (a) The alternative: the same band Sustain-hunts the tamed herd → the full pastoral MSY.
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    let hunter = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    run_turns_with_hunt(&mut app, 1);
    let hunting = yield_of(&app, hunter);
    assert!(hunting > 0.0, "the alternative use of the crew pays");

    // (b) Build the pen: the same band, same herd, under Corral → the dip and nothing else.
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let cap = herd_of(&app, &id).carrying_capacity;
    domesticate(&mut app, &id);
    reseat(&mut app, &id, cap, cap);
    grant_penning(&mut app);
    let builder = spawn_hunter(&mut app, &id, FollowPolicy::Corral);
    run_turns_with_hunt(&mut app, 1);
    let building = yield_of(&app, builder);

    let dip_fraction = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPen)
        .yield_fraction_while_building()
        .expect("the pen rung is an investment");
    assert!(
        (building - dip_fraction * hunting).abs() < hunting * 0.05,
        "building pays only the dip ({dip_fraction} × the pastoral MSY {hunting}): got {building}"
    );
    assert!(
        building < hunting,
        "**the pen must COST something**: building ({building}/turn) has to be a real loss against \
         hunting the same herd ({hunting}/turn), or corralling is free and there is no decision"
    );
}

/// **The pastoral rung pays its pastoral MSY, and the harvest DRAWS THE HERD DOWN** — which is what
/// makes it sustainable (the flow-based ladder, `docs/plan_corral_managed_population.md`).
///
/// **Retargeted from the passive path, guarantee intact.** The *what* is verbatim — a tamed herd's
/// harvest is the MSY of the **pastoral** ecology (per-species `r` × `pastoral_gain`, resolved through
/// the one `herd_ecology` seam) and it is a real take out of the herd, not a share of standing stock.
/// Only the *who* changed: it is paid to a **worker** on a Hunt assignment rather than dropped into
/// the owner's larders for free (slice 3b, §3 — every rung is worker-driven). That the pastoral `r`
/// really does reach the worker's take is the crux of the slice, so it is asserted here against the
/// herd's own resolved ecology.
#[test]
fn a_worker_hunting_a_pastoral_herd_takes_its_pastoral_msy_and_draws_the_herd_down() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);

    let cap = herd_of(&app, &id).carrying_capacity;
    let (expected_take, expected_provisions) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        // Per-species pastoral rate (Grazing 2d): read the same seam the sim harvests through.
        let pastoral_r = herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate;
        let take = pastoral_r * cap / 4.0;
        (take, take * fauna.hunt.provisions_per_biomass)
    };
    // **Seated at the OPERATING POINT, not at capacity** (slice 8). A Sustain hunt is constant
    // escapement to `K/2`, so what a herd hands over is the **standing surplus above that point**,
    // not a rate. This test is about the *rate* — that the pastoral `r` reaches the worker's take —
    // so it seats the herd exactly where a converged herd stands when the Population stage runs:
    // `K/2` **plus the turn's own regrowth** (`r·K/4`, the MSY). The escapement it spares is then
    // precisely that MSY, and the assertion below measures the thing it was written to measure.
    //
    // Seating at `B = K` (as this used to) makes the herd spare `K/2` — the accumulated **stock**,
    // which is identical for every rung because `r` cancels out of `K − K/2`. That is correct
    // behaviour and it is exactly why it cannot measure a growth rate; see
    // `the_husbandry_ladder_is_a_per_species_growth_rate_ladder` for the long-run form.
    let biomass_before = cap * MSY_BIOMASS_FRACTION + expected_take;
    reseat(&mut app, &id, cap, biomass_before);
    assert_eq!(provisions(&mut app), 0);

    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    app.world.run_system_once(advance_labor_allocation);

    let paid = yield_of(&app, band);
    assert!(
        (paid - expected_provisions).abs() < expected_provisions * 0.02,
        "a worker's take on a tamed herd is the PASTORAL MSY: expected {expected_provisions}, got {paid}"
    );
    assert!(
        (larder_of(&app, band) - paid).abs() < paid * 0.02,
        "and it lands in the working band's own larder, place-local"
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

/// Teach faction 0 **Herding** — the unlock gate on the **`Tame`** verb (rung 2), and since the
/// §4.3 reshuffle that alone. A taming test must grant it; a *corralling* test needs
/// [`grant_penning`] instead.
fn grant_herding(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), HERDING_DISCOVERY_ID, scalar_one());
}

/// Teach faction 0 **Penning** — the unlock gate on the **`Corral`** verb (rung 3) since the §4.3
/// reshuffle, so a Corral assignment actually accrues pen progress. Earned in play by working a
/// *pastoral* herd; granted directly here, as these tests are about the pen, not the climb to it.
fn grant_penning(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), PENNING_DISCOVERY_ID, scalar_one());
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
    herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
    // A freshly-penned herd has a crew (`corral_at` grants the tending grace for the same reason) —
    // see `domesticate` for why the meter alone is not enough.
    herd.herded_fraction = FULLY_HERDED;
    let tile = herd.position();
    assert!(herd.corral_at(tile), "the fixture species must be pennable");
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
///
/// **Seated at the OPERATING POINT (slice 8).** `corral_herd` seats the herd at `B = K`, where the
/// pen's escapement harvest is the standing **stock** `K/2` — a one-off windfall identical at every
/// rung, not the `r·K/4` *rate* this test asserts the pen pays. So it re-seats to where a running
/// pen actually stands (`K/2` + the turn's regrowth) before measuring. The **upkeep** half was never
/// in doubt (it is charged on biomass, whatever the biomass is); what the reseat restores is the
/// **gross-yield** half and the `upkeep < gross` net-positive claim underneath it.
#[test]
fn tending_a_pen_debits_the_keepers_larder_by_its_upkeep() {
    const STOCK: f32 = 500.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    corral_herd(&mut app, &id);
    let (upkeep_rate, pen_r, prov_rate) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            fauna.husbandry.pen.upkeep_per_biomass,
            // Per-species pen rate (Grazing 2d): the corralled herd's own rung.
            herd_ecology(&herd_of(&app, &id), &fauna).regrowth_rate,
            fauna.hunt.provisions_per_biomass,
        )
    };
    // Re-seat onto the settled operating point — `corral_herd` leaves the herd at capacity.
    let cap = herd_of(&app, &id).carrying_capacity;
    let pen_msy = pen_r * cap / 4.0;
    reseat(&mut app, &id, cap, cap * MSY_BIOMASS_FRACTION + pen_msy);
    let biomass = herd_of(&app, &id).biomass;

    let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    stock_larder(&mut app, keeper, STOCK);

    // One Population turn only, so the herd's biomass (and thus the demand) is the one we measured.
    app.world.run_system_once(advance_labor_allocation);

    let expected_upkeep = upkeep_rate * biomass;
    let gross = yield_of(&app, keeper);
    let expected_gross = pen_msy * prov_rate;
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
/// **Slice 3b makes it a YIELD-PER-WORKER ladder, which is what the ladder now promises.** Every rung
/// is worker-driven, so all three rows are measured the same way: the **same band, the same head-count
/// (`HUNT_WORKERS`), the same `K`, the same per-species wild `r`** — only the rung differs. The
/// monotone `wild < pastoral < pen` therefore reads directly as *food per worker*, and the
/// `pastoral / wild` ratio is asserted to be exactly `pastoral_gain` — the payoff that replaced
/// "pastoral = zero workers".
///
/// **Slice 8 makes it a LONG-RUN average, and that is a correction to the MEASUREMENT, not a
/// weakening of the guarantee.** Every hunt is constant escapement now, so a herd hands over
/// `B − K/2` — a **stock**, not a rate. At `B = K` that is `K/2` **for every rung**: `r` cancels
/// clean out of `K − K/2`, so a full herd's first harvest is *identical* wild, pastoral and penned.
/// That is not a bug and it must not be "fixed": the surplus standing above the escapement point is
/// **accumulated stock**, and stock does not care how fast you breed. What management buys is that
/// **the next animal comes sooner** — so the ladder is monotone in the rate a rung sustains over
/// time, which is exactly `r·K/4`, and the only way to see it is to average a run long enough to
/// contain the refills. Hence: seat at the operating point, run `MEASURE_TURNS`, average.
///
/// A single turn cannot measure this any more, at either biomass. At `B = K` you read the stock
/// (rung-blind). At `B = K/2` you read *zero* for any species whose one-turn MSY is lighter than one
/// animal (a wild mammoth: 120 biomass of regrowth against an 800-unit beast) — the herd correctly
/// **waits**. Both readings are honest; neither is the ladder.
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
    /// Turns averaged per rung, seeded at the operating point.
    ///
    /// Sized by the **slowest pulse the table contains**: a wild Thunder Mammoth sustains
    /// `r·K/4` = 120 biomass/turn against an **800-unit body**, so it spares one beast roughly every
    /// 7 turns. 600 turns is ~85 of those cycles — enough that the ≤1 uncollected body still standing
    /// at the end is a fraction of a percent of the run, so the average reads the rung's rate rather
    /// than where its last pulse happened to land. Every other row pulses far faster.
    const MEASURE_TURNS: u32 = 600;

    // The pastoral rung's promised multiple of the wild rung — read off the config, never pinned.
    let pastoral_gain = FaunaConfigHandle::default().get().husbandry.pastoral_gain;
    // (display, cap, per-species wild r, body_mass) — the wild rung must be measured at each species'
    // OWN r, and since slice 8 at its own **body**: the take quantises to whole animals, so a
    // mammoth's `K` measured against the fixture's 1-unit fowl body would be a different economy
    // entirely (it would never wait).
    let species_caps: Vec<(String, f32, f32, f32)> = {
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
                    def.body_mass,
                )
            })
            .collect()
    };

    // **Measured once, as a long-run average from the settled operating point** (`B* = K/2` — where a
    // harvested herd converges, and the point the pen's net-positive invariant is derived against).
    // The retired "at capacity (B = K)" pass measured the standing **stock** every rung shares (see
    // the doc comment), not the ladder.
    //
    // Every row runs **full turns in real stage order** (Logistics: `advance_herds` regrows →
    // `advance_husbandry`; Population: `advance_labor_allocation`), so the numbers are what the sim
    // pays, not what a single system does in isolation. The feed is charged on the *post-regrowth*
    // biomass (you feed every animal in the pen, including the ones you are about to harvest).
    println!(
        "\n=== husbandry ladder, MEASURED as the {MEASURE_TURNS}-turn average from the operating \
         point (B* = K/2) (provisions/turn) ==="
    );
    println!(
        "{:<18} {:>8} {:>9} {:>9} {:>11} {:>9} {:>9}",
        "species", "K", "wild", "pastoral", "pen gross", "upkeep", "pen net"
    );
    for (species, cap, wild_r, body_mass) in &species_caps {
        let (species, cap, wild_r, body_mass) = (species.clone(), *cap, *wild_r, *body_mass);
        let biomass = cap * MSY_BIOMASS_FRACTION;

        // --- Wild Sustain: a band hunting a wild herd — its ACTUAL take, from the yield telemetry.
        // Seat the herd at THIS species' per-species wild `r` (2b-ii) and body (slice 8), since the
        // spawned short-range game the harness reuses carries its own.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, biomass);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
        let wild = average_yield_over_run(&mut app, band, MEASURE_TURNS);

        // --- Pastoral: **the same band, the same head-count, hunting a TAMED herd** — its ACTUAL
        // take. Passive-free pastoral is retired (slice 3b), so this row is now measured exactly
        // like the wild one and the three rows are directly comparable **per worker**: same
        // workers, same `K`, only the rung differs.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, biomass);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        domesticate(&mut app, &id);
        let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
        let pastoral = average_yield_over_run(&mut app, band, MEASURE_TURNS);

        // --- Pen: the gross yield credited + the feed debited, both read off the keeper's larder.
        let mut app = spawn_world();
        let id = prime_thriving_herd(&mut app);
        reseat(&mut app, &id, cap, cap);
        seat_species_traits(&mut app, &id, wild_r, body_mass);
        corral_herd(&mut app, &id);
        reseat(&mut app, &id, cap, biomass); // corral_herd seats at cap; re-seat for B*
        let keeper = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
        let (pen_gross, upkeep) =
            average_pen_yield_and_upkeep(&mut app, keeper, MEASURE_TURNS, MEASURE_STOCK);
        let pen_net = pen_gross - upkeep;

        println!(
            "{species:<18} {cap:>8.0} {wild:>9.3} {pastoral:>9.3} {pen_gross:>11.3} {upkeep:>9.3} {pen_net:>9.3}"
        );

        assert_growth_rate_ladder(
            &species,
            wild_r,
            pastoral_gain,
            wild,
            pastoral,
            pen_gross,
            upkeep,
            pen_net,
        );
    }
    println!();
}

/// **One rung's LONG-RUN average take**, in provisions/turn: run the full turn pipeline `turns` times
/// and average the band's *actual* take off the retained yield telemetry.
///
/// This is the only honest way to read a rung since slice 8 made the hunt constant escapement on
/// whole animals: a single turn reads either the standing **stock** (at `B = K`, identical at every
/// rung) or a **pulse** (at `B*`, where a herd whose MSY is lighter than one animal takes nothing and
/// waits). Averaged over many refill cycles, both artifacts wash out and what is left is the rate the
/// rung sustains — which *is* the thing the ladder claims to raise. See the caller's doc comment.
fn average_yield_over_run(app: &mut App, band: bevy::prelude::Entity, turns: u32) -> f32 {
    let mut total = 0.0;
    for _ in 0..turns {
        run_turns_with_hunt(app, 1);
        total += yield_of(app, band);
    }
    total / turns as f32
}

/// [`average_yield_over_run`] for the **pen**, which also has a bill: returns
/// `(mean gross yield, mean larder upkeep)`.
///
/// The larder is topped back up to `stock` **before every turn**, so each turn's debit is readable in
/// isolation (`upkeep = stock + gross − larder`) *and* the pen never goes hungry mid-run — an unfed
/// pen shrinks (`starve_shrink_rate`), which would quietly turn this into a measurement of starvation
/// rather than of the rung.
fn average_pen_yield_and_upkeep(
    app: &mut App,
    keeper: bevy::prelude::Entity,
    turns: u32,
    stock: f32,
) -> (f32, f32) {
    let mut gross_total = 0.0;
    let mut upkeep_total = 0.0;
    for _ in 0..turns {
        stock_larder(app, keeper, stock);
        run_turns_with_hunt(app, 1);
        let gross = yield_of(app, keeper);
        gross_total += gross;
        // larder = stock − upkeep + gross ⇒ upkeep = stock + gross − larder.
        upkeep_total += stock + gross - larder_of(app, keeper);
    }
    (gross_total / turns as f32, upkeep_total / turns as f32)
}

/// The **per-species GROWTH-RATE ladder** (Grazing 2d §3), asserted on **measured** numbers. Since the
/// managed rungs now scale each species' own wild `r` (`pastoral_gain` 1.5 < `pen_gain` 3.0, capped),
/// the ladder is monotone in GROSS yield for **every** species — the old fast-breeder pastoral
/// inversion is gone. The pen's *net* payoff over pastoral is realized by SELF-FEEDING (this barren
/// harness runs the pen fully larder-fed, so it only asserts the pen costs real feed; the net-positive
/// floor for the fastest breeder lives in `fauna_config`'s validate tests).
#[allow(clippy::too_many_arguments)] // one measured column per argument — a struct would only rename them
fn assert_growth_rate_ladder(
    species: &str,
    wild_r: f32,
    pastoral_gain: f32,
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
    // **And by exactly the gain** (slice 3b): the rows are equal-worker and equal-K, so this ratio IS
    // the yield-per-worker payoff for taming — the whole of what replaced passive-free pastoral.
    assert!(
        (pastoral / wild - pastoral_gain).abs() < 0.02 * pastoral_gain,
        "{species}: the SAME workers on a tamed herd take pastoral_gain ({pastoral_gain}×) the wild \
         take — that multiple IS the taming payoff. wild {wild} → pastoral {pastoral}"
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

/// Seat the two cached per-species traits a rung is measured against — the **wild** regrowth rate
/// (Grazing 2b-ii) and the **body mass** (slice 8) — since the harness reuses one spawned short-range
/// herd for every row and that herd carries whatever species the map happened to place.
///
/// **Both, together, or the row is a different animal.** `r` alone was enough while the take was a
/// smooth flow; now the take quantises to whole bodies, so `r` and `body_mass` jointly decide the
/// *rhythm* (`body_mass / (r·K/4)` turns per animal). A mammoth's `K` and `r` measured against a
/// 1-unit fowl body would never wait for a whole animal — the exact property the slice added.
fn seat_species_traits(app: &mut App, id: &str, r: f32, body_mass: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.regrowth_rate = r;
    herd.body_mass = body_mass;
}

/// **A properly-herded tamed herd does NOT rot — including one you are merely HARVESTING**
/// (intensification ladder slice 8).
///
/// This is the guarantee that had to survive deleting `decay_domestication`'s `is_domesticated()`
/// early return. That return made a tamed herd permanently tame for **zero labor**; removing it makes
/// husbandry a standing cost — but it must not make *harvesting your own herd* corrode it. A crew is
/// standing right there under Sustain: the animals stay gentled.
///
/// So the rule is staffing, **not** the verb: enough herders ⇒ no decay under *any* policy. `Tame` is
/// what makes the meter go *up*; it is not what stops it going down.
#[test]
fn a_properly_herded_tamed_herd_does_not_decay_under_a_harvest_policy() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    // A big crew, so `herders_needed` is comfortably met however the biomass breathes.
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    run_turns_with_hunt(&mut app, 20);

    let herd = herd_of(&app, &id);
    assert_eq!(
        herd.domestication_progress, RUNG_COMPLETE,
        "a fully-staffed Sustain hunt must not rot its own tamed herd: {} after 20 turns",
        herd.domestication_progress
    );
    assert!(herd.is_domesticated(), "and it is still a pastoral herd");
}

/// **An UNDER-herded tamed herd forgets — proportionally, and it recovers.** The other half of
/// `a_properly_herded_tamed_herd_does_not_decay_under_a_harvest_policy`, and the reason the decay is a
/// *fraction* rather than a threshold: half the herders you need ⇒ half the decay, nothing snaps, and
/// staffing it again stops the bleed. A binary escape here would void a ~32-turn taming investment on
/// rounding, as biomass breathes across a herder boundary.
#[test]
fn an_under_herded_tamed_herd_decays_proportionally_and_recovers() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);

    // Nobody herding at all — the abandonment case, full decay.
    run_turns_untended(&mut app, 6);
    let abandoned = herd_of(&app, &id).domestication_progress;
    assert!(
        abandoned < RUNG_COMPLETE,
        "an unherded tamed herd starts forgetting: {abandoned}"
    );
    assert!(
        abandoned > 0.9,
        "but it forgets SLOWLY — the investment is not destroyed: {abandoned}"
    );

    // A crew returns under a plain harvest policy: the bleed stops dead, with no `Tame` needed.
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    run_turns_with_hunt(&mut app, 1); // one turn to write `herded_fraction`
    let before = herd_of(&app, &id).domestication_progress;
    run_turns_with_hunt(&mut app, 6);
    let after = herd_of(&app, &id).domestication_progress;
    assert_eq!(
        after, before,
        "staffing the herd again stops the decay outright: {before} -> {after}"
    );
}

/// A corralled herd left untended **escapes**: `advance_husbandry` clears `corralled_at`, reverting it
/// to a mobile herd that keeps its taming **investment**.
///
/// **What "still domesticated" means changed in slice 8, and the guarantee is stated more precisely
/// here.** This used to assert `is_domesticated()` outright — true only because
/// `decay_domestication` opened with an `is_domesticated()` early return, i.e. a tamed herd was
/// permanently tame for **zero labor**. That return is deleted (§3: every rung is worked), so an
/// abandoned herd — nobody feeding it, nobody herding it — begins going feral **immediately**, exactly
/// as an abandoned tended patch does on the plant web (`is_cultivated()` is the same `>= 1.0`
/// threshold, and `advance_cultivation` drops it below on the first untended turn).
///
/// So the escape costs the **pen**, not the **taming**: `corral_progress` is zeroed outright (25 turns
/// gone) while `domestication_progress` is merely *bleeding* at the rung's `decay_per_turn` — still
/// ~99% intact after the turn it broke out, and cheap to top back up with `Tame`. That is the real
/// invariant, and asserting the meter rather than the flag is what makes it legible.
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
    // The **taming investment survives the escape** — it is bleeding (nobody is herding them), not
    // reset. Losing the pen must not also delete the ~32 turns of gentling underneath it.
    assert!(
        herd.domestication_progress > 0.9,
        "the escape costs the PEN, not the taming: domestication still {} after breaking out",
        herd.domestication_progress
    );
    assert!(
        herd.owner.is_some(),
        "and it is still the owner's herd — feral takes ~100 turns, not 3"
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
        .build_accrual(FollowPolicy::Corral, true, RUNG_TIMESCALE_UNSCALED);

    // No keeper: the grace turn is consumed, then the herd breaks out.
    run_turns_untended(&mut app, 3);
    let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
    assert!(!herd.is_corralled(), "an untended corral escapes");
    assert_eq!(
        herd.corral_progress, 0.0,
        "the escaped herd's pen is lost — progress resets"
    );

    // A keeper returns under the Corral policy: it must REBUILD, not snap straight back to penned.
    //
    // **Re-tame first (slice 8).** The abandoned herd has been bleeding its taming since it broke out
    // (nobody was herding it — `decay_domestication`'s `is_domesticated()` early return is gone), so
    // it is a hair under the `>= 1.0` bar `Corral` gates on and would accrue nothing. That is real
    // behaviour and it is `untended_corral_escapes_to_mobile`'s subject; here it is *setup noise*, so
    // top the meter back up and keep this test on its own question: **does the pen rebuild from zero?**
    domesticate(&mut app, &id);
    grant_penning(&mut app);
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
        herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
    }
    grant_penning(&mut app);
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

/// Regression (fully-fractional FOOD income): a **tiny** tamed herd whose per-turn MSY harvest is
/// below 1.0 provisions must still credit the larder — rounding the credit to an i64 used to drop it
/// entirely.
///
/// **Retargeted to the worker path** (slice 3b retired the passive payout this used to ride), and
/// re-seated onto a deliberately tiny `K` so the take is sub-unit for **every** shipped species'
/// `r` rather than for the one the map happened to spawn.
///
/// **Re-seated again for whole animals (slice 8).** It used to seat `B = 0.52 · K` and lean on the
/// take being the *flow* `r·K/4` — a fraction of an animal, which now quantises to **zero** and the
/// hunt waits. That is correct behaviour and it deletes the case the test exists for, so the seat is
/// now stated in the unit the take is actually denominated in: **`K/2` plus exactly one body**, so
/// the herd spares precisely one beast. The credit is then `body_mass × provisions_per_biomass` —
/// **0.02 on the fixture's 1-unit Wild Fowl**, still emphatically sub-unit, so the i64-rounding
/// regression is exercised at the smallest take the sim can now produce. (There is no longer *any*
/// seat that yields a sub-unit take on a heavy species: one mammoth is 16 provisions. The
/// fractional-credit path is a small-game property now, which is exactly what the quantiser means.)
#[test]
fn sub_unit_pastoral_yield_credits_larder() {
    /// A tiny herd, so `K/2 + one body` is still a Thriving fraction of `K`.
    const SUB_UNIT_CAP: f32 = 40.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    domesticate(&mut app, &id);
    // Seat the herd exactly one animal above its Sustain escapement point, so the take is the
    // **smallest whole take that exists**: one body. `reseat` refreshes the phase, and (20 + 1)/40
    // is comfortably Thriving.
    let body_mass = herd_of(&app, &id).body_mass;
    reseat(
        &mut app,
        &id,
        SUB_UNIT_CAP,
        SUB_UNIT_CAP * MSY_BIOMASS_FRACTION + body_mass,
    );
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    app.world.run_system_once(advance_labor_allocation);

    let larder = provisions_f32(&mut app);
    assert!(
        larder > 0.0 && larder < 1.0,
        "a sub-1 pastoral yield must credit a positive fractional amount (got {larder})"
    );
}

// --- The full climb (intensification ladder slice 4) ----------------------------------------------

/// Faction 0's ledger progress on `discovery`.
fn ladder_knowledge(app: &App, discovery: u32) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), discovery)
        .to_f32()
}

/// Switch the band's (only) Hunt assignment onto `policy` — the sim-side of the client's policy
/// picker, which is what the player does at each rung of the climb.
fn set_hunt_policy(
    app: &mut App,
    band: bevy::prelude::Entity,
    herd_id: &str,
    policy: FollowPolicy,
) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists");
    allocation.assignments[0].target = LaborTarget::Hunt {
        fauna_id: herd_id.to_string(),
        policy,
    };
}

/// Run turns until `done`, returning how many it took. Capped so a leg that can never complete fails
/// loudly with its own name instead of hanging the suite.
fn turns_until(app: &mut App, leg: &str, cap: u32, done: impl Fn(&App) -> bool) -> u32 {
    for turn in 1..=cap {
        run_turns_with_hunt(app, 1);
        if done(app) {
            return turn;
        }
    }
    panic!("the '{leg}' leg never completed within {cap} turns");
}

/// **The pacing consequence of the knowledge pattern, measured end-to-end** (slice 4,
/// `docs/plan_intensification_ladder.md` §4/§4.3).
///
/// Reaching a pen is now a **four-leg climb**, and each leg is paced by *practising the rung below*:
///
/// | leg | what the player does | gated by / earns |
/// |---|---|---|
/// | 1 | Sustain-hunt the **wild** herd | earns **Herding** (~20 turns) |
/// | 2 | **`Tame`** it | needs Herding; fills this herd's meter |
/// | 3 | Sustain-hunt the **pastoral** herd | earns **Penning** (~20 turns) — *the new leg* |
/// | 4 | **`Corral`** it | needs Penning; builds the pen |
///
/// **Leg 3 is what slice 4 added**: before the §4.3 reshuffle, Herding gated `Corral` directly, so
/// the climb was legs 1-2-4 and a pen cost ~20 turns less. That is deliberate — one knowledge per
/// transition, and you cannot skip a rung you have not practised.
///
/// Asserted as **bands, not exact turn counts**: this pins the shape of the climb (and that no leg
/// silently collapses to zero — e.g. a gate accidentally left open, or rung 2 teaching the wrong
/// knowledge) without becoming a change-detector for the `knowledge`/`build` playtest dials.
#[test]
fn the_full_wild_to_pen_climb_is_paced_by_practising_each_rung() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    // A `pen`-ceiling species that actually reaches the top of the ladder, at `taming_rate` 0.8.
    rebadge_as(&mut app, &id, "boar");
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    // Leg 1 — practise the WILD rung: a Sustain hunt teaches Herding, and nothing else.
    let leg1 = turns_until(&mut app, "learn Herding", 60, |app| {
        ladder_knowledge(app, HERDING_DISCOVERY_ID) >= RUNG_COMPLETE
    });
    assert_eq!(
        ladder_knowledge(&app, PENNING_DISCOVERY_ID),
        0.0,
        "a WILD herd teaches Herding only — Penning must NOT come free with it, or the climb \
         skips the rung the reshuffle exists to add"
    );

    // Leg 2 — `Tame` fills this herd's meter (Herding is the gate the leg above just opened).
    set_hunt_policy(&mut app, band, &id, FollowPolicy::Tame);
    let leg2 = turns_until(&mut app, "tame the herd", 120, |app| {
        herd_of(app, &id).is_domesticated()
    });

    // Leg 3 — **the new leg**: practise the PASTORAL rung. The same Sustain hunt now teaches
    // Penning, because the herd stands on a different rung.
    assert!(
        ladder_knowledge(&app, PENNING_DISCOVERY_ID) < RUNG_COMPLETE,
        "Penning cannot already be known — taming a WILD herd practises rung 1, not rung 2"
    );
    set_hunt_policy(&mut app, band, &id, FollowPolicy::Sustain);
    let leg3 = turns_until(&mut app, "learn Penning", 60, |app| {
        ladder_knowledge(app, PENNING_DISCOVERY_ID) >= RUNG_COMPLETE
    });

    // Leg 4 — `Corral`, gated on the Penning the leg above just earned.
    set_hunt_policy(&mut app, band, &id, FollowPolicy::Corral);
    let leg4 = turns_until(&mut app, "build the pen", 60, |app| {
        herd_of(app, &id).is_corralled()
    });

    let total = leg1 + leg2 + leg3 + leg4;
    println!(
        "wild -> pen climb (Wild Boar): Herding {leg1} + Tame {leg2} + Penning {leg3} + Corral \
         {leg4} = {total} turns"
    );

    // Each knowledge leg is ~20 turns of practice (threshold / progress_per_turn).
    for (leg, turns) in [("Herding", leg1), ("Penning", leg3)] {
        assert!(
            (18..=22).contains(&turns),
            "the {leg} leg should be ~20 turns of practice, got {turns}"
        );
    }
    // The two build legs: ~31 turns to tame a boar (25 / 0.8) and ~25 to fence it.
    assert!(
        (28..=34).contains(&leg2),
        "taming a boar ~31 turns, got {leg2}"
    );
    assert!((23..=27).contains(&leg4), "fencing ~25 turns, got {leg4}");
    // The headline: a pen is now a ~95-turn commitment, ~20 turns longer than the pre-slice-4
    // climb (which had no Penning leg). Broad band — these are playtest dials.
    assert!(
        (85..=110).contains(&total),
        "the whole climb should run ~95 turns, got {total}"
    );
}

/// **Penning accrues from WORKING a pastoral herd, on EVERY turn — not only on turns an animal is
/// killed** (slice 8b regression — a playtest report of "Penning stuck at 0%").
///
/// The kill-credit model (slice 8b) makes a Sustain hunt of a big-bodied species a pulse: many
/// **wait-turns** (no kill while the credit bank fills), then a kill. If knowledge earning had been
/// tied to the kill, learning would stall for big game. It is not — the earn path in
/// `advance_labor_allocation`'s Hunt arm resolves the herd's rung and credits its `earns_knowledge`
/// **before** the take branches, gated on the *policy* (stewardship) and the herd being *Thriving*,
/// never on a kill. This pins that: an **Aurochs** (`body_mass` 80 — Sustain waits several turns per
/// kill) pastoral herd Sustain-hunted accrues Penning to completion in ~20 turns, and at least one of
/// those was a 0-kill wait-turn (so the assertion genuinely exercises the decoupling).
#[test]
fn penning_accrues_every_worked_turn_not_only_on_kill_turns() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    rebadge_as(&mut app, &id, "aurochs"); // pen-ceiling, heavy body 80 ⇒ Sustain wait-turns
    domesticate(&mut app, &id); // a completed PASTORAL herd (domesticated, not corralled)
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    let _ = band;

    assert_eq!(
        herd_of(&app, &id).ecology_phase,
        core_sim::EcologyPhase::Thriving,
        "the fixture must be a Thriving pastoral herd, the earning scenario"
    );

    let mut wait_turns = 0u32;
    let mut turns = 0u32;
    while ladder_knowledge(&app, PENNING_DISCOVERY_ID) < RUNG_COMPLETE {
        let before = herd_of(&app, &id).biomass;
        run_turns_with_hunt(&mut app, 1);
        // A wait-turn: the herd's biomass did not fall (no whole animal was spared/killed this turn).
        if herd_of(&app, &id).biomass >= before - 1e-3 {
            wait_turns += 1;
        }
        turns += 1;
        assert!(turns <= 30, "Penning must accrue to completion, not stall");
    }
    assert!(
        (18..=22).contains(&turns),
        "Penning completes in ~20 turns of working the pastoral herd, got {turns}"
    );
    assert!(
        wait_turns > 0,
        "the fixture must include a 0-kill wait-turn, or it does not exercise the kill-decoupling \
         (Penning still reached completion across {turns} turns)"
    );
}
