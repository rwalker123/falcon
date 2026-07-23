//! Hunting-expedition take + trip semantics (`advance_expeditions`, `ExpeditionPhase::Hunting`).
//!
//! **A hunting expedition is a greedy RAID** (the playtest fix), distinct from a resident band's
//! throttled kill-credit skim: the party grabs the herd's standing surplus above the policy's floor
//! (Sustain `K/2`, Surplus `0.30·K`, Market `0.15·K`, Eradicate `0`) as fast as its throughput allows,
//! then comes home when the pack fills OR the surplus is spent — so more hunters take more animals in
//! fewer-or-equal turns. The launch forecast (`hunt_trip_forecast`) is a bounded forward simulation of
//! that raid, pinned here to a real party run (fix the forecast, never the sim). The band-path guards
//! below still pin the *resident* `hunt_take`, which this arc leaves untouched.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_expeditions, advance_herds, build_headless_app, hunt_credit_ceiling, hunt_policy_rate,
    hunt_provisions, hunt_source_yield_preview, hunt_take, hunt_trip_forecast,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, Expedition, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionMission, ExpeditionPhase, FactionId, FactionInventory, FaunaConfig,
    FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry, GenerationId,
    GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborConfig, LaborConfigHandle, LadderConfig, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Scalar, SimulationConfig,
    SimulationTick, SizeClass, SnapshotHistory, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
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
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
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

// ---------------------------------------------------------------------------------------------------
// The greedy hunting RAID (the playtest fix): a party grabs the herd's standing surplus above the
// policy's floor in a burst — so more hunters take more animals in fewer-or-equal turns — and comes
// home when the pack fills OR the surplus is spent. These pin the raid math + completion semantics.
// ---------------------------------------------------------------------------------------------------

/// Wild Boar's playtest numbers (the greedy-raid worked example): carrying capacity 1433, a 50-unit
/// body (⇒ food/animal = 50 × `hunt.provisions_per_biomass` 0.02 = 1.0), wild `r` 0.10. Sustain's floor
/// is `K/2` = 716.5, so a herd at 1010 stands 293.5 (≈ 5 boar) of surplus above it.
const BOAR_K: f32 = 1433.0;
const BOAR_BODY: f32 = 50.0;
const BOAR_R: f32 = 0.10;

/// A constructed wild herd for the pure-`hunt_trip_forecast` tests — no ECS, no graze, so `K` is the
/// fixed `carrying_capacity` we set (the live-arm harness recomputes `K` from graze; these tests pin
/// the raid math against a known ecology, not the ecology itself).
fn wild_herd(biomass: f32, cap: f32, body: f32, r: f32) -> Herd {
    Herd::new(
        "game_raid".to_string(),
        "Wild Boar".to_string(),
        SizeClass::Big,
        vec![UVec2::new(1, 1)],
        biomass,
        cap,
        0.0,
        r,
        body,
    )
}

/// An expedition config whose carry cap is effectively unbounded, so a raid is limited ONLY by the
/// standing surplus (never by the pack) — isolating the surplus-bound regime the floor tests are about.
fn unbounded_carry_config() -> Arc<ExpeditionConfig> {
    let mut cfg = (*ExpeditionConfig::builtin()).clone();
    cfg.hunt.per_worker_carry = 1.0e6;
    Arc::new(cfg)
}

/// **The playtest fix — more hunters raid FASTER, never slower** (the anti-regression). Under the old
/// model the per-turn ceiling was worker-independent (the MSY-credit rate), so a second hunter only
/// added pack to fill and the trip took *longer*. The greedy raid's per-turn take scales with the
/// party's throughput, so more hunters draw the surplus down in strictly fewer turns. (With a pack too
/// big to bind, the raid runs until the herd hits its floor; a *slower* raid sits on the herd longer
/// and harvests more of its regrowth on the way down, so the animal count is not party-size-invariant —
/// the load-bearing claim is the turn count.) Prints the boar numbers.
#[test]
fn more_hunters_raid_the_surplus_faster() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(1010.0, BOAR_K, BOAR_BODY, BOAR_R);

    let mut prev_turns = u32::MAX;
    for workers in 1..=4u32 {
        let f = hunt_trip_forecast(workers, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
        let turns = f
            .turns_to_fill
            .expect("a surplus-bound boar raid completes");
        println!(
            "[surplus-bound] Sustain raid, {workers} hunter(s): {} animals over {} turns",
            f.animals_taken, turns
        );
        assert!(
            turns <= prev_turns,
            "more hunters must never take MORE turns to raid the surplus ({prev_turns} then {turns})"
        );
        prev_turns = turns;
    }
    let one = hunt_trip_forecast(1, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    let four = hunt_trip_forecast(4, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert!(
        four.turns_to_fill.unwrap() < one.turns_to_fill.unwrap(),
        "four hunters must raid the surplus strictly faster than one ({} vs {} turns)",
        four.turns_to_fill.unwrap(),
        one.turns_to_fill.unwrap()
    );
}

/// **The worked-example regime (the real pack).** A lone hunter's pack caps its haul a boar short of
/// the surplus; a second hunter clears more of it — and never in more turns. Prints the boar numbers
/// the playtest report quotes.
#[test]
fn a_second_hunter_raids_more_animals_no_slower() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin(); // pack = party × per_worker_carry (4 food = 4 boar)
    let herd = wild_herd(1010.0, BOAR_K, BOAR_BODY, BOAR_R);

    for workers in 1..=3u32 {
        let f = hunt_trip_forecast(workers, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
        println!(
            "[pack=4/worker] Sustain raid, {workers} hunter(s): {} animals over {} turns",
            f.animals_taken,
            f.turns_to_fill.expect("a boar raid completes")
        );
    }
    let one = hunt_trip_forecast(1, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    let two = hunt_trip_forecast(2, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert!(
        two.animals_taken >= one.animals_taken,
        "a second hunter must never raid FEWER animals ({} vs {})",
        two.animals_taken,
        one.animals_taken
    );
    assert!(
        two.turns_to_fill.unwrap() <= one.turns_to_fill.unwrap() + 1,
        "a second hunter must not blow the trip length out (the old bug: bigger pack, same fill rate)"
    );
}

/// **Animals delivered SCALE WITH THE PACK (the over-kill regression).** A heavy-bodied herd with a
/// large standing surplus (a Marsh Grazer: body 100, food/animal 2, surplus far bigger than any pack)
/// is pack-limited at every party size, so the raid delivers `floor(pack ÷ food-per-animal)` whole
/// animals and **never over-kills** (a hunter carries its 100-body kills home *whole*, over several
/// turns, wasting nothing). This is the bug the rework fixes: the old model killed at the throughput
/// rate and wasted the carcass it couldn't carry, then reported the *kill* count (which plateaued at 1
/// useful worker). Prints the table. **After the biomass-anchor retune (`per_worker_carry` 4.0 → 0.8)
/// a whole 2-food Marsh Grazer needs a ≥3-worker crew** (a 1–2 worker party can't seat one and instead
/// force-partials — a separate regime, covered elsewhere), so this test sweeps the whole-seating range.
#[test]
fn animals_delivered_scale_with_the_pack_and_never_over_kill() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin(); // pack = workers × per_worker_carry (0.8 food/worker)
                                           // Marsh Grazer: body 100 ⇒ food/animal = 100 × 0.02 = 2; a full 6000-K herd stands 3000 (30 animals)
                                           // of surplus above K/2 — vastly more than any legal party's pack, so every size is pack-limited.
    const MARSH_BODY: f32 = 100.0;
    let herd = wild_herd(6000.0, 6000.0, MARSH_BODY, 0.04);
    let food_per_animal = MARSH_BODY * fauna.hunt.provisions_per_biomass; // 2.0

    // Sweep the whole-seating regime: at per_worker_carry 0.8, a 2-food animal needs ceil(2/0.8)=3
    // workers before the pack seats one whole, so a 1–2 worker party force-partials instead (its own
    // regime). Here the pack seats 1,1,2,2,2,3 whole animals for 3..=8 hunters — scaling, no over-kill.
    for workers in 3..=8u32 {
        let f = hunt_trip_forecast(workers, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
        let pack_animals =
            (workers as f32 * cfg.hunt.per_worker_carry / food_per_animal).floor() as u32;
        println!(
            "[pack-scaling] Marsh Grazer, {workers} hunter(s): {} animals over {} turns (pack fits {})",
            f.animals_taken,
            f.turns_to_fill.expect("a pack-limited raid completes"),
            pack_animals
        );
        assert_eq!(
            f.animals_taken, pack_animals,
            "a pack-limited raid delivers exactly what the pack seats whole, no over-kill"
        );
    }
}

/// **Sustain leaves the herd at ~K/2.** A raid on a full herd draws the standing stock down to (within
/// a body of) the Sustain floor and comes home; the animals it takes account for the surplus above
/// `K/2` (plus the regrowth it earns along the way).
#[test]
fn a_sustain_raid_leaves_about_half_k() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(BOAR_K, BOAR_K, BOAR_BODY, BOAR_R);

    let f = hunt_trip_forecast(4, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    let taken_biomass = f.animals_taken as f32 * BOAR_BODY;
    let floor = BOAR_K * 0.5;
    println!(
        "[leaves K/2] full boar Sustain raid: {} animals ({taken_biomass} biomass), leftover ≈ {}",
        f.animals_taken,
        BOAR_K - taken_biomass
    );
    // It grabs the surplus above K/2 (716.5), plus the regrowth earned over the raid — so a touch more
    // than the standing surplus, never less.
    assert!(
        taken_biomass >= (BOAR_K - floor) - BOAR_BODY,
        "a Sustain raid must clear ~all the surplus above K/2"
    );
    assert!(
        taken_biomass <= (BOAR_K - floor) + 4.0 * fauna_msy(&fauna, BOAR_K, BOAR_R),
        "…but never eat into K/2 (the leftover stays ≈ half the herd)"
    );
}

/// One MSY (`r·K/4`) — the most the herd regrows in a turn, used as the slop bound above.
fn fauna_msy(_fauna: &FaunaConfig, cap: f32, r: f32) -> f32 {
    r * cap / 4.0
}

/// **Surplus and Market raid deeper than Sustain.** The floors descend (0.50·K > 0.30·K > 0.15·K), so
/// a deeper policy leaves a leaner herd and its raid takes strictly more animals off a full herd.
#[test]
fn deeper_policies_raid_deeper() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(BOAR_K, BOAR_K, BOAR_BODY, BOAR_R);

    let animals = |p| hunt_trip_forecast(4, &herd, p, &fauna, &labor, &cfg).animals_taken;
    let sustain = animals(FollowPolicy::Sustain);
    let surplus = animals(FollowPolicy::Surplus);
    let market = animals(FollowPolicy::Market);
    println!("[deeper] full boar: Sustain {sustain} < Surplus {surplus} < Market {market} animals");
    assert!(
        sustain < surplus && surplus < market,
        "a deeper policy must raid strictly more animals: Sustain {sustain}, Surplus {surplus}, Market {market}"
    );
}

/// **The standing surplus caps the raid.** Beyond the party size whose pack matches the surplus, extra
/// hunters cannot deliver more animals — the herd simply has no more to spare above the floor.
#[test]
fn the_standing_surplus_caps_the_raid() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(1010.0, BOAR_K, BOAR_BODY, BOAR_R);

    let four = hunt_trip_forecast(4, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    let eight = hunt_trip_forecast(8, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert!(
        four.animals_taken.abs_diff(eight.animals_taken) <= 1,
        "the take is surplus-capped: 8 hunters cannot raid materially more than 4 ({} vs {})",
        eight.animals_taken,
        four.animals_taken
    );
}

/// **A herd at its floor has no surplus to raid** — the honest non-viable case. A herd at/below the
/// policy's floor delivers **zero** animals (the party would return empty).
#[test]
fn a_herd_at_its_floor_has_no_surplus() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    // Exactly at Sustain's K/2 floor → no surplus.
    let herd = wild_herd(BOAR_K * 0.5, BOAR_K, BOAR_BODY, BOAR_R);

    let f = hunt_trip_forecast(4, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert_eq!(
        f.animals_taken, 0,
        "a herd at its floor spares no whole animal to a Sustain raid"
    );
    // Below the Allee threshold (a collapsing remnant) likewise has no Sustain surplus.
    let collapsing = wild_herd(
        BOAR_K * fauna.ecology.collapse_fraction * 0.5,
        BOAR_K,
        BOAR_BODY,
        BOAR_R,
    );
    let g = hunt_trip_forecast(4, &collapsing, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert_eq!(
        g.animals_taken, 0,
        "a collapsing herd has no Sustain surplus either"
    );
}

/// **A party too small to seat a whole animal still KILLS one and wastes the rest** — the reconciliation
/// with the resident band's `quantise_animal_take` (`max(1, carryable)`). The motivating case: a Thunder
/// Mammoth herd (body 800 biomass = 16 food) with real surplus above K/2, raided by a 1-worker party
/// whose pack holds only `per_worker_carry` = 4 food = 200 biomass < one body. It used to deliver a flat
/// 0 ("too lean to raid"); it now kills ONE, carries the pack's ~200 biomass (≈ 25%), and wastes ~600.
/// "Too lean" now means only `delivered_food == 0` (no surplus), which a genuinely at-floor herd still is.
#[test]
fn a_small_party_on_a_big_animal_delivers_a_partial_with_waste() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin(); // pack = workers × per_worker_carry (4 food/worker = 200 biomass)
    const MAMMOTH_BODY: f32 = 800.0; // 16 food; a 1-worker pack (200 biomass) seats 0 whole
    const MAMMOTH_K: f32 = 15600.0;
    const MAMMOTH_R: f32 = 0.04;
    let ppb = fauna.hunt.provisions_per_biomass; // 0.02
    let pack_biomass = cfg.hunt.per_worker_carry / ppb; // 200 biomass for one worker
    let body_food = MAMMOTH_BODY * ppb; // 16 food

    // Standing surplus above K/2 ≈ 3213 biomass ≈ 4 whole mammoths — NOT lean.
    let herd = wild_herd(11013.0, MAMMOTH_K, MAMMOTH_BODY, MAMMOTH_R);
    let f = hunt_trip_forecast(1, &herd, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    println!(
        "[partial] 1-worker mammoth: killed {} animals, delivered {:.2} / wasted {:.2} food over {:?} turns",
        f.animals_taken, f.delivered_food, f.wasted_food, f.turns_to_fill
    );
    // The pack-full stop ends the trip after exactly ONE forced-partial kill — kills 1, not many.
    assert_eq!(
        f.animals_taken, 1,
        "the party kills exactly one animal it cannot seat whole (the pack-full stop prevents over-kill)"
    );
    // Delivers ≈ the pack's worth (200 biomass → 4 food), wasting the remainder of the body (12 food).
    assert!(
        (f.delivered_food - pack_biomass * ppb).abs() <= TAKE_ABS_EPSILON,
        "delivers ≈ one pack's worth of food (≈ per_worker_carry): {} vs {}",
        f.delivered_food,
        pack_biomass * ppb
    );
    assert!(
        f.delivered_food > 0.0,
        "a partial delivery is non-zero — the herd is not too lean to raid"
    );
    assert!(
        (f.wasted_food - (body_food - f.delivered_food)).abs() <= TAKE_ABS_EPSILON,
        "wastes the rest of the body it could not haul: {} vs {}",
        f.wasted_food,
        body_food - f.delivered_food
    );

    // A genuinely at-floor herd (surplus < one body) still delivers NOTHING — the true too-lean case.
    let at_floor = wild_herd(MAMMOTH_K * 0.5, MAMMOTH_K, MAMMOTH_BODY, MAMMOTH_R);
    let lean = hunt_trip_forecast(1, &at_floor, FollowPolicy::Sustain, &fauna, &labor, &cfg);
    assert_eq!(
        lean.animals_taken, 0,
        "a herd at K/2 has no surplus to raid — kills nothing"
    );
    assert_eq!(
        lean.delivered_food, 0.0,
        "…and delivers nothing: THIS is 'too lean to raid'"
    );
}

/// (2) **Scoping fix.** A party still walking (beyond `hunt.reach_tiles`) must not take, and must not
/// conclude the trip — the completion check is inside the in-reach guard.
#[test]
fn walking_party_never_concludes_the_trip() {
    let mut app = spawn_world();
    let id = stationary_game_herd(&app);
    let (herd_pos, before, _cap) = seed_herd(&mut app, &id, 1.0);
    let home = spawn_home_band(&mut app, herd_pos);
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

/// **THE forecast cannot lie — pinned to a real party run.** The `hunt_trip_forecast` that drives the
/// outfit UI must equal what a real party does turn-by-turn through the real systems: it completes on
/// exactly the turn the party leaves `Hunting` (pack full OR surplus spent). If they disagree, fix the
/// forecast, never the sim. (Run in `spawn_world`, whose empty graze layer keeps `K` constant, so the
/// pure forecast's fixed-`K` clone and the live arm agree exactly.)
#[test]
fn the_raid_forecast_matches_a_real_party_run() {
    for cap_fraction in [1.0_f32, 0.75, 0.6] {
        for policy in [
            FollowPolicy::Sustain,
            FollowPolicy::Surplus,
            FollowPolicy::Market,
        ] {
            let mut app = spawn_world();
            let id = pinned_game_herd(&mut app, "big");
            // Neutralize combat: `hunt_trip_forecast` deliberately does NOT model casualties in
            // Phase 0, so a dangerous big-game species would shrink the party mid-raid and diverge
            // the real run from the forecast. This test is about the raid economy, not combat (that
            // has its own test), so retag to a harmless species (attack 0) while keeping the heavy
            // body_mass the partial/waste mechanics need. Wiring casualties into the forecast is a
            // Phase-1+ follow-up.
            {
                let mut registry = app.world.resource_mut::<HerdRegistry>();
                registry
                    .herds
                    .iter_mut()
                    .find(|h| h.id == id)
                    .unwrap()
                    .species = "Rabbit Warren".to_string();
            }
            let (herd_pos, _before, _cap) = seed_herd(&mut app, &id, cap_fraction);
            let home = spawn_home_band(&mut app, herd_pos);

            let (fauna, labor, cfg) = (
                app.world.resource::<FaunaConfigHandle>().get(),
                app.world.resource::<LaborConfigHandle>().get(),
                expedition_config(&app),
            );
            let forecast = {
                let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
                hunt_trip_forecast(PARTY_WORKERS, herd, policy, &fauna, &labor, &cfg)
            };
            let context = format!("{policy:?} @ {cap_fraction}·K");

            let party = spawn_hunt_party(&mut app, home, herd_pos, &id, policy);
            let mut completed = None;
            for turn in 1..=cfg.hunt.forecast_horizon_turns {
                app.world.run_system_once(advance_herds);
                app.world.run_system_once(advance_expeditions);
                if phase(&app, party) != ExpeditionPhase::Hunting {
                    completed = Some(turn);
                    break;
                }
            }
            assert_eq!(
                forecast.turns_to_fill, completed,
                "{context}: the forecast must complete on the turn the real party leaves Hunting \
                 (forecast {:?}, real {completed:?}) — fix the forecast, never the sim",
                forecast.turns_to_fill
            );
        }
    }
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

/// Both sides run the same linear formula but land on the sim's fixed-point grid at *different* points,
/// so the band-hunt guards allow a few `Scalar` quanta of rounding.
const TAKE_ABS_EPSILON: f32 = 4.0 / Scalar::SCALE as f32;
/// …plus f32 slop proportional to the magnitude (a big-game take runs to hundreds of provisions).
const TAKE_REL_EPSILON: f32 = 1e-5;

/// Assert a snapshot-derived preview matches the provisions the sim's real take produced.
fn assert_provisions_eq(preview: f32, real_take: f32, context: &str) {
    let tolerance = TAKE_ABS_EPSILON + real_take.abs() * TAKE_REL_EPSILON;
    assert!(
        (preview - real_take).abs() <= tolerance,
        "{context}: snapshot preview {preview} != real take {real_take}"
    );
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

/// Pin the **exported local-hunt yield preview** to the provisions `hunt_take` really pays a resident
/// band, over every worker count × every policy × each of `herd_ids`.
///
/// **RETARGETED IN SLICE 8 — the preview is an exported ANSWER now, not client arithmetic.** This used
/// to replay the client's own formula, `min(workers × huntPerWorkerProvisions, ceiling) ×
/// outputMultiplier`, which was exact because every term was linear and factored out of the `min`.
/// A whole-animal take runs through `floor()`, and **`floor` does not factor out of anything**: no
/// combination of a per-worker rate and a ceiling lets the client re-derive "3 boars, one of them only
/// half carried". So the sim exports the number (`fauna::hunt_source_yield_preview` →
/// `SourceYield.actual`, the same seam that seeds the assign-time telemetry) and this asserts THAT
/// equals the take.
///
/// The guard is **stronger, not weaker**: it still pins a client-visible preview to the sim's real
/// take across the same sweep, and it now pins the *actual* thing the client renders instead of a
/// formula the client is no longer allowed to use. The exported per-policy `ceiling` rows are still
/// checked to exist and to exclude the forage-only verbs — they remain the honest "what will this herd
/// give up at all" readout, they are simply no longer a *staffing* formula's input.
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

            // A ceiling row must never promise more than the herd is standing there holding.
            // **Inherent since slice 8** rather than a clamp someone has to remember: every ceiling is
            // `biomass − floor` with `floor >= 0`.
            let live_biomass = app
                .world
                .resource::<HerdRegistry>()
                .find(id)
                .expect("herd present")
                .biomass;
            assert!(
                ceiling <= hunt_provisions(live_biomass, &fauna, 1.0) + TAKE_ABS_EPSILON,
                "{case}: {id} {policy:?}: exported ceiling {ceiling} exceeds the herd's own biomass"
            );

            for workers in BAND_HUNT_WORKER_COUNTS {
                // What the client renders: the sim's own exported preview for this staffing.
                let preview = {
                    let registry = app.world.resource::<HerdRegistry>();
                    let herd = registry.find(id).expect("herd present");
                    hunt_source_yield_preview(
                        herd,
                        &fauna,
                        &LadderConfig::builtin(),
                        labor.hunt.per_worker_biomass_capacity,
                        output_multiplier,
                        workers,
                        policy,
                        labor.yield_average_horizon_turns,
                        labor.arrivals_horizon_turns,
                    )
                    .actual
                };

                // The sim's real band take (a resident band has no carry limit — it eats/banks the
                // whole take, so `carry_room_biomass = INFINITY`, exactly as the Hunt labor arm
                // passes). Clone the herd so each sweep entry sees the same pre-take state.
                let mut herd = app
                    .world
                    .resource::<HerdRegistry>()
                    .find(id)
                    .expect("herd present")
                    .clone();
                let take = hunt_take(
                    &mut herd,
                    workers,
                    policy,
                    labor.hunt.per_worker_biomass_capacity,
                    &fauna,
                    &LadderConfig::builtin(),
                    f32::INFINITY,
                );
                let sim_rate = hunt_provisions(take.carried, &fauna, output_multiplier);

                assert_provisions_eq(
                    preview,
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

    // Pass 3: **the case that used to need a biomass clamp, kept as the proof nothing can make it
    // fire.** `CLAMP_BINDING_REGROWTH_RATE` is an extreme (hot-reloadable) `r` under which the OLD
    // **flow** ceilings — `MSY` (Sustain) and `1.6 × MSY` (Surplus) — computed a take *larger than the
    // herd was standing there holding*, so the exported ceiling had to be explicitly clamped or the
    // preview over-stated it.
    //
    // Slice 8 makes that unreachable **by construction**, and the reason is the same one that killed
    // both flows: **every rule on the axis is now a STOCK rule bounded by `B`** — Sustain is
    // `B − K/2`, Surplus/Market are `fraction × B`, Eradicate is `take_from(B)` (itself `.min(B)`). No
    // `r`, however hot, can lift any of them above the biomass, because none of them reads `r` as a
    // rate at all.
    //
    // The pass is kept (retargeted from "the clamp fires" to "nothing can make it need to fire"): it
    // still sweeps the whole preview==take matrix at an off-nominal lever, and it now pins the
    // stronger property. `assert_band_preview_matches_hunt_take` asserts the bound on every row.
    set_fauna_regrowth_rate(&mut app, CLAMP_BINDING_REGROWTH_RATE);
    {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        for policy in FollowPolicy::HUNT_POLICIES {
            assert!(
                {
                    let rate = hunt_policy_rate(
                        policy,
                        depleted_biomass,
                        depleted_cap,
                        &fauna.ecology,
                        &fauna,
                        &LadderConfig::builtin(),
                    );
                    hunt_credit_ceiling(policy, depleted_biomass, 0.0, rate)
                } <= depleted_biomass,
                "{policy:?}: the credit ceiling can never exceed the herd's own biomass, at any \
                 regrowth rate"
            );
        }
    }
    assert_band_preview_matches_hunt_take(&mut app, &herds, "clamp-binding ecology");
}

// ---------------------------------------------------------------------------------------------------
// Predators Phase 0 — a hunting EXPEDITION takes casualties too, and BLOODIER than a local hunt
// (far from home, unsupported, tired: `expedition_danger_multiplier`). `docs/plan_predators.md`.
// ---------------------------------------------------------------------------------------------------

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

/// A hunting expedition against a mammoth (attack 8) loses party working-age population over an
/// engagement turn.
#[test]
fn a_hunting_expedition_takes_casualties_against_a_mammoth() {
    let mut app = spawn_world();
    let id = retag_herd(&mut app, MAMMOTH);
    let (pos, _b, _cap) = seed_herd(&mut app, &id, 1.0);
    let home = spawn_home_band(&mut app, pos);
    // Party ON the herd's tile → in reach, so it engages this turn.
    let party = spawn_hunt_party(&mut app, home, pos, &id, FollowPolicy::Surplus);
    let before = party_working(&app, party);
    app.world.run_system_once(advance_expeditions);
    let after = party_working(&app, party);
    assert!(
        after < before,
        "a mammoth (attack 8) expedition hunt must cost party working-age: {before} -> {after}"
    );
    // ...and it narrates on the command feed.
    let narrated = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .any(|e| e.kind.as_str() == "hunt_danger");
    assert!(
        narrated,
        "a dangerous expedition hunt pushes a hunt_danger feed line"
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
                        range: RangeBand::Melee,
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
                        range: RangeBand::Melee,
                    },
                }],
            },
        ],
        terrain: vec![],
        seed: 0,
    };

    let local = CombatTuning {
        lethality: 1.0,
        disengage_fraction: 0.5,
    };
    let expedition = CombatTuning {
        lethality: 1.5,
        disengage_fraction: 0.5,
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
