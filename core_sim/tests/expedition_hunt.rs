//! Hunting-expedition take + trip semantics (`advance_expeditions`, `ExpeditionPhase::Hunting`).
//!
//! **A hunting expedition is a greedy RAID** (the playtest fix), distinct from a resident band's
//! throttled kill-credit skim: the party grabs the herd's standing surplus above the policy's floor
//! (Sustain `K/2`, Surplus `0.30·K`, Deplete `0.15·K`, Eradicate `0`) as fast as its throughput allows,
//! then comes home when the pack fills OR the surplus is spent — so more hunters take more animals in
//! fewer-or-equal turns. The launch forecast (`hunt_trip_forecast`) is a bounded forward simulation of
//! that raid, pinned here to a real party run (fix the forecast, never the sim). The band-path guards
//! below still pin the *resident* `hunt_take`, which this arc leaves untouched.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

/// The floors these sweeps walk — the four the retired stance axis named, plus the ends of the dial
/// it could not express (`0.8` deliberate under-harvest, `1.0` take nothing).
const SWEPT_FLOORS: [f32; 6] = [0.0, 0.15, 0.3, 0.5, 0.8, 1.0];

use core_sim::{
    advance_band_movement, advance_expeditions, advance_herds, available_workers,
    build_headless_app, herd_hunt_yield, hunt_escapement_ceiling, hunt_source_yield_preview,
    hunt_take, hunt_trip_forecast, recapture_snapshot_in_place, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world, BandTravel,
    CommandEventLog, CultureManager, DiscoveryProgressLedger, Expedition, ExpeditionConfig,
    ExpeditionConfigHandle, ExpeditionMission, ExpeditionPhase, FactionId, FactionInventory,
    FaunaConfig, FaunaConfigHandle, ForageRegistry, GenerationId, GenerationRegistry, Herd,
    HerdDensityMap, HerdRegistry, HerdTelemetry, HuntTripBound, LaborAllocation, LaborConfig,
    LaborConfigHandle, LadderConfig, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, ResidentBand, Scalar, SimulationConfig, SimulationTick,
    SizeClass, SnapshotHistory, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, VisibilityConfig, VisibilityConfigHandle, VisibilityLedger,
    WellbeingConfigHandle, FOOD, NO_FILL_TARGET, NO_IMPROVEMENT_UNDERWAY,
};

/// Party size used by every trip test: 4 hunters (the design's reference party).
const PARTY_WORKERS: u32 = 4;

/// **The smallest party that can bring one Wild Boar down**, so a fixture about *delivery* is not
/// silently measuring the fight's gate (`docs/plan_hunt_through_combat.md` §4.2:
/// `ceil(durability 20 / (spear 20 − defense 2))` = **2**). Stated as a literal here rather than
/// derived, because these two fixtures also have to name the same number in a snapshot row lookup
/// (`huntTripEstimates` is sampled per whole party size); `hunters_to_bring_one_down` is the derived
/// form and `boar_raid_crew_matches_the_derived_threshold` pins the two together.
const BOAR_RAID_CREW: u32 = 2;

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
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world.insert_resource(ExpeditionConfigHandle::default());
    app.world
        .insert_resource(VisibilityConfigHandle::new(VisibilityConfig::builtin()));
    app.world.insert_resource(VisibilityLedger::default());
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

/// A home band far from the herd but on the **same row**, so a returning party's Chebyshev
/// `step_toward` (which advances x and y independently) travels purely along x — making its per-turn
/// progress equal to the `hex_distance_wrapped` the in-flight ETA measures. A diagonal offset would
/// let the party cover both axes at once and arrive well before the hex-distance ETA, which is a real
/// (deliberate) approximation of the helper, not a bug the driven pin should trip over.
fn spawn_home_band_same_row(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let width = app.world.resource::<TileRegistry>().width;
    let far = UVec2::new((herd_pos.x + width / 3) % width, herd_pos.y);
    let tile = tile_at(app, far);
    app.world.spawn((cohort(tile, 10), ResidentBand)).id()
}

/// A `PARTY_WORKERS`-strong hunting party at `pos`, already in the `Hunting` phase.
fn spawn_hunt_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    policy: f32,
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
    floor: f32,
    workers: u32,
) -> bevy::prelude::Entity {
    spawn_hunt_party_targeting(
        app,
        home_band,
        pos,
        fauna_id,
        floor,
        workers,
        NO_FILL_TARGET,
    )
}

/// A hunting party carrying a **fill target** — the party-side twin of the floor, in whole animals
/// (`docs/plan_hunt_through_combat.md` §5.2). [`NO_FILL_TARGET`] reproduces `spawn_hunt_party_of`.
fn spawn_hunt_party_targeting(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    floor: f32,
    workers: u32,
    fill_target: u32,
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
                    floor,
                    fill_target,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
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

/// **The party every pure-forecast fixture below fights with** — the shipped, fully-kitted hunter
/// (`docs/plan_hunt_through_combat.md` §4.8's spear tier). The take resolves through the fight now,
/// so a raid forecast is quoted for a *party*, and these fixtures mean "an ordinary outfitted one".
fn hunting_party() -> core_sim::HuntingParty {
    core_sim::HuntingParty::builtin_equipped()
}

/// **The smallest party that can bring one of this species down in a turn** — `ceil(durability /
/// max(0, attack − defense))` at the shipped kitted tier (`docs/plan_hunt_through_combat.md` §4.2),
/// derived from config so a retune of any of its three inputs moves it rather than stranding a
/// hard-coded crew.
///
/// **Damage carries between turns** (§4.2), so a party below this takes nothing *this* turn and a
/// whole animal several turns later. A fixture that sweeps party size therefore still has to start
/// here: below it a per-turn comparison is measuring which turn of the grind it landed on, not the
/// property it names. Boar reads **2** and Red Deer **2** at the shipped spear; a mammoth reads 63.
#[test]
fn boar_raid_crew_matches_the_derived_threshold() {
    assert_eq!(
        BOAR_RAID_CREW,
        hunters_to_bring_one_down("Wild Boar", &FaunaConfig::builtin()),
        "BOAR_RAID_CREW is a literal because two snapshot-row lookups need it; if a retune moves \
         the derived threshold, move the literal with it"
    );
}

/// **The smallest party that can bring `animals` of this species into contact at once** —
/// `ceil(animals / engage_rate)` (`docs/plan_hunt_through_combat.md` §2).
///
/// Engagement is floored to whole animals, so party size raises a take in **steps**: at a Wild Boar's
/// `engage_rate 0.33` every crew from 1 to 6 reaches exactly one, and it takes 7 to reach two. A
/// sweep that asserts "more hunters take the surplus faster" must therefore span a step, or it is
/// sampling the plateau between two of them and asserting a strict inequality against a flat line.
fn hunters_to_reach(species: &str, animals: u32, fauna: &FaunaConfig) -> u32 {
    let rate = fauna.engage_rate_for(species);
    assert!(
        rate.is_finite() && rate > 0.0,
        "{species} has no engage rate"
    );
    (animals as f32 / rate).ceil() as u32
}

fn hunters_to_bring_one_down(species: &str, fauna: &FaunaConfig) -> u32 {
    let quarry = fauna
        .species_by_display(species)
        .expect("the fixture names a shipped species");
    let per_hunter = core_sim::strike_damage(hunting_party().hunter.attack, quarry.combat.defense);
    assert!(
        per_hunter > 0.0,
        "{species} cannot be hurt at the shipped spear tier — a party-size sweep is meaningless"
    );
    (quarry.combat.durability / per_hunter).ceil() as u32
}

/// The shipped `person` row's `combat.durability` (`creatures.json`) and the mammoth's
/// (`fauna_config.json`) — how much damage each body soaks before it goes down
/// (`docs/plan_hunt_through_combat.md` §4.2). Restated so the hand-built fights below field the
/// roster's creatures rather than neutral stand-ins.
const PERSON_DURABILITY: f32 = 20.0;
const MAMMOTH_DURABILITY: f32 = 500.0;

/// A constructed wild herd for the pure-`hunt_trip_forecast` tests — no ECS, no graze, so `K` is the
/// fixed `carrying_capacity` we set (the live-arm harness recomputes `K` from graze; these tests pin
/// the raid math against a known ecology, not the ecology itself).
fn wild_herd(biomass: f32, cap: f32, body: f32, r: f32) -> Herd {
    wild_herd_of("Wild Boar", biomass, cap, body, r)
}

/// [`wild_herd`] for a named species — the engagement bound is resolved off the species' display
/// name (`FaunaConfig::engage_rate_for`), so a test about *reach* has to say which animal it means.
fn wild_herd_of(species: &str, biomass: f32, cap: f32, body: f32, r: f32) -> Herd {
    Herd::new(
        "game_raid".to_string(),
        species.to_string(),
        SizeClass::Big,
        vec![UVec2::new(1, 1)],
        biomass,
        cap,
        0.0,
        r,
        body,
    )
}

/// Red Deer's shipped shape: a **15**-unit body a hunter engages **one** of per turn — so a party's
/// *reach* is strictly tighter than its *carry* (`40 / 15` ≈ 2.6 animals per hunter), which is what
/// makes the deer the fixture for the engagement bound.
const DEER_BODY: f32 = 15.0;
/// A deer herd standing at a capacity far above what any party here can clear, so the herd's own
/// escapement room never binds and the comparison is purely between the party's two bounds.
const DEER_K: f32 = 4000.0;
/// The food peak (`MSY_BIOMASS_FRACTION`) — the default floor a fresh assignment or launch gets.
const PEAK_FLOOR: f32 = 0.5;
/// The party both halves of the parity test field. Small enough to be a plausible band *and* a legal
/// expedition, large enough that carry (13 deer) and reach (5 deer) give visibly different answers.
const PARITY_PARTY: u32 = 5;

/// **A raid and a resident band reach the SAME animals** — the same party on the same herd must not
/// take a different number of animals purely by choosing the expedition verb.
///
/// `docs/plan_hunt_through_combat.md` §1 states the hunt's stages for *the hunt*; §10 exempts only the
/// pen. The engagement bound reached `systems::hunt_take` first, and for one commit the raid path
/// skipped it: five hunters killed 5 Red Deer a turn from camp and `floor(5 × 40 / 15) = 13` a turn as
/// a raid, off the same herd. This pins the two paths to one answer, and the liveness half pins that
/// the answer is the *engagement* bound rather than the carry bound they would share anyway.
#[test]
fn a_raid_and_a_resident_band_reach_the_same_animals() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let per_worker = labor.hunt.per_worker_biomass_capacity;
    // A full herd: at `B == K` regrowth is zero, so the raid's first simulated turn sees exactly the
    // herd the band does and the two are comparable turn-for-turn.
    let herd = wild_herd_of("Red Deer", DEER_K, DEER_K, DEER_BODY, BOAR_R);

    let band_killed = {
        let mut quarry = herd.clone();
        hunt_take(
            &mut quarry,
            PARITY_PARTY,
            PEAK_FLOOR,
            NO_IMPROVEMENT_UNDERWAY,
            per_worker,
            &hunting_party(),
            &fauna,
            &LadderConfig::builtin(),
            f32::INFINITY,
            // Every shipped species has `wariness 0`, which makes the retreat draw an exact
            // identity — so the seed is unobservable and held fixed on both paths.
            0,
        )
        .take
        .killed
    };

    let raid = hunt_trip_forecast(
        PARITY_PARTY,
        &herd,
        PEAK_FLOOR,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let food_per_animal = herd_hunt_yield(&herd, &fauna)
        .apply(DEER_BODY, 1.0)
        .provisions;
    let raid_killed_first_turn = (raid.first_turn_provisions / food_per_animal).round() as u32;

    assert_eq!(
        raid_killed_first_turn, band_killed,
        "a raid and a resident band of {PARITY_PARTY} must take the same deer off the same herd \
         (raid {raid_killed_first_turn}, band {band_killed})"
    );
    // Liveness, two ways. The take is real…
    assert!(band_killed > 0, "the fixture must produce an actual take");
    // …and it is the ENGAGEMENT bound that produced it: the party's packs would have seated far more,
    // so deleting the bound on either path breaks the equality above rather than passing quietly.
    let carry_allows = (PARITY_PARTY as f32 * per_worker / DEER_BODY).floor() as u32;
    assert!(
        carry_allows > band_killed,
        "the fixture must be reach-bound, not carry-bound: carry seats {carry_allows}, reach took \
         {band_killed}"
    );
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
    // **The sweep starts at the crew that can bring one boar down, not at 1.** Below it the raid takes
    // nothing at any party size (§4.2's gate), which orders perfectly and says nothing about
    // throughput — the same reason §10 had to grow three other crew constants when the engagement
    // bound landed.
    let least = hunters_to_bring_one_down("Wild Boar", &fauna);
    // ...and it ends at the crew that can reach a SECOND boar, because engagement is quantised: every
    // crew between the two takes one a turn and the sweep would be a flat line.
    let most = hunters_to_reach("Wild Boar", 2, &fauna).max(least + 1);

    let mut prev_turns = u32::MAX;
    for workers in least..=most {
        let f = hunt_trip_forecast(
            workers,
            &herd,
            0.5,
            NO_FILL_TARGET,
            &fauna,
            &labor,
            &cfg,
            &hunting_party(),
        );
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
    let one = hunt_trip_forecast(
        least,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let four = hunt_trip_forecast(
        most,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    assert!(
        four.turns_to_fill.unwrap() < one.turns_to_fill.unwrap(),
        "{most} hunters must raid the surplus strictly faster than {least} ({} vs {} turns)",
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
    // The smallest crew that can bring one boar down — see `hunters_to_bring_one_down`. "A second
    // hunter" means a second one *past* that, not the second body in the party.
    let least = hunters_to_bring_one_down("Wild Boar", &fauna);

    for workers in least..=least + 2 {
        let f = hunt_trip_forecast(
            workers,
            &herd,
            0.5,
            NO_FILL_TARGET,
            &fauna,
            &labor,
            &cfg,
            &hunting_party(),
        );
        println!(
            "[pack=4/worker] Sustain raid, {workers} hunter(s): {} animals over {} turns",
            f.animals_taken,
            f.turns_to_fill.expect("a boar raid completes")
        );
    }
    let one = hunt_trip_forecast(
        least,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let two = hunt_trip_forecast(
        least + 1,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
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
        let f = hunt_trip_forecast(
            workers,
            &herd,
            0.5,
            NO_FILL_TARGET,
            &fauna,
            &labor,
            &cfg,
            &hunting_party(),
        );
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

    let f = hunt_trip_forecast(
        4,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let taken_biomass = f.animals_taken as f32 * BOAR_BODY;
    let floor = BOAR_K * 0.5;
    let turns = f
        .turns_to_fill
        .expect("a Sustain raid on a full herd completes");
    println!(
        "[leaves K/2] full boar Sustain raid: {} animals ({taken_biomass} biomass) over {turns} turns",
        f.animals_taken
    );
    // It grabs the surplus above K/2 (716.5), plus the regrowth earned over the raid — so a touch more
    // than the standing surplus, never less.
    assert!(
        taken_biomass >= (BOAR_K - floor) - BOAR_BODY,
        "a Sustain raid must clear ~all the surplus above K/2"
    );
    // **The regrowth allowance is per TURN OF THE RAID, not a fixed four.** By conservation the take
    // is `ΔB + Σ regrowth`, and one turn's regrowth can never exceed MSY, so this bound says exactly
    // "the herd's own biomass never fell below the floor" — the property the test is named for. A
    // constant allowance instead assumed a raid was ~4 turns long, which the engagement bound
    // (`docs/plan_hunt_through_combat.md` §2) is free to change: a party that can only bring a couple
    // of boar into contact per turn works the same surplus over many more turns, and earns every one
    // of those turns' regrowth honestly.
    assert!(
        taken_biomass <= (BOAR_K - floor) + turns as f32 * fauna_msy(&fauna, BOAR_K, BOAR_R),
        "…but never eat into K/2 (the leftover stays ≈ half the herd)"
    );
}

/// One MSY (`r·K/4`) — the most the herd regrows in a turn, used as the slop bound above.
fn fauna_msy(_fauna: &FaunaConfig, cap: f32, r: f32) -> f32 {
    r * cap / 4.0
}

/// **Surplus and Deplete raid deeper than Sustain.** The floors descend (0.50·K > 0.30·K > 0.15·K), so
/// a deeper policy leaves a leaner herd and its raid takes strictly more animals off a full herd.
#[test]
fn deeper_policies_raid_deeper() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(BOAR_K, BOAR_K, BOAR_BODY, BOAR_R);

    let animals = |p| {
        hunt_trip_forecast(
            4,
            &herd,
            p,
            NO_FILL_TARGET,
            &fauna,
            &labor,
            &cfg,
            &hunting_party(),
        )
        .animals_taken
    };
    let sustain = animals(0.5);
    let surplus = animals(0.3);
    let deplete = animals(0.15);
    println!(
        "[deeper] full boar: Sustain {sustain} < Surplus {surplus} < Deplete {deplete} animals"
    );
    assert!(
        sustain < surplus && surplus < deplete,
        "a deeper policy must raid strictly more animals: Sustain {sustain}, Surplus {surplus}, Deplete {deplete}"
    );
}

/// **The standing surplus caps the raid.** Beyond the party size whose pack matches the surplus, extra
/// hunters cannot deliver more animals — the herd simply has no more to spare above the floor.
///
/// **The cap is one-directional, and deliberately asserted as such.** It used to read as a two-sided
/// "materially the same take" (`abs_diff <= 1`), which was always in tension with
/// `more_hunters_raid_the_surplus_faster`'s own note: a bigger party spends the surplus in fewer turns
/// and therefore harvests *less* of the herd's regrowth on the way down. The engagement bound
/// (`docs/plan_hunt_through_combat.md` §2) widened that gap from a rounding to a real one — 8 boar
/// hunters reach twice as many animals a turn as 4 and finish in a fraction of the turns — so the
/// honest claim is the cap itself: **more hunters can never take MORE than the herd can spare.**
#[test]
fn the_standing_surplus_caps_the_raid() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = unbounded_carry_config();
    let herd = wild_herd(1010.0, BOAR_K, BOAR_BODY, BOAR_R);

    let four = hunt_trip_forecast(
        4,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let eight = hunt_trip_forecast(
        8,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    // Liveness: both parties genuinely raid, so the ordering below is not two zeroes agreeing.
    assert!(
        four.animals_taken > 0 && eight.animals_taken > 0,
        "both parties must actually raid the surplus ({} / {})",
        four.animals_taken,
        eight.animals_taken
    );
    assert!(
        eight.animals_taken <= four.animals_taken,
        "the take is surplus-capped: 8 hunters cannot raid more than 4 ({} vs {})",
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

    let f = hunt_trip_forecast(
        4,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
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
    let g = hunt_trip_forecast(
        4,
        &collapsing,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    assert_eq!(
        g.animals_taken, 0,
        "a collapsing herd has no Sustain surplus either"
    );
}

/// **A party too small to seat a whole animal still KILLS one and wastes the rest** — the reconciliation
/// with the resident band's `quantise_animal_take` (`max(1, carryable)`). A body of 800 biomass
/// (= 16 food) with real surplus above K/2, raided by a 1-worker party whose pack holds only
/// `per_worker_carry` = 4 food = 200 biomass < one body. It used to deliver a flat 0 ("too lean to
/// raid"); it kills ONE, carries the pack's ~200 biomass (≈ 25%), and wastes ~600. "Too lean" now
/// means only `delivered_food == 0` (no surplus), which a genuinely at-floor herd still is.
///
/// # Why the quarry is a 800-biomass **fowl** and not the mammoth it was
///
/// This fixture is about **carry**, and since the take resolves through the fight
/// (`docs/plan_hunt_through_combat.md` §4) a real mammoth cannot reach it: the crew that can bring
/// one down (63, `ceil(500 / (20 − 12))`) can always carry it, and that is true of **every shipped
/// species** at the spear tier — `durability/(attack − defense)` exceeds `body/per_worker_carry`
/// across the whole roster. So the species is one whose fight is trivially won (Wild Fowl, `defense
/// 0`, `durability 2` — one hunter brings down ten) carrying a synthetic big body, which leaves the
/// **pack** as the only binding term. That is deliberate: the property under test is `max(1,
/// carryable)`, and a fixture that let the fight bind would stop testing it.
#[test]
fn a_small_party_on_a_big_animal_delivers_a_partial_with_waste() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin(); // pack = workers × per_worker_carry (4 food/worker = 200 biomass)
    const MAMMOTH_BODY: f32 = 800.0; // 16 food; a 1-worker pack (200 biomass) seats 0 whole
    const MAMMOTH_K: f32 = 15600.0;
    const MAMMOTH_R: f32 = 0.04;
    /// A quarry a lone hunter can put down without the fight ever binding — see the doc above.
    const UNGUARDED_QUARRY: &str = "Wild Fowl";
    let ppb = fauna.hunt.provisions_per_biomass; // 0.02
    let pack_biomass = cfg.hunt.per_worker_carry / ppb; // 200 biomass for one worker
    let body_food = MAMMOTH_BODY * ppb; // 16 food

    // Standing surplus above K/2 ≈ 3213 biomass ≈ 4 whole bodies — NOT lean.
    let herd = wild_herd_of(
        UNGUARDED_QUARRY,
        11013.0,
        MAMMOTH_K,
        MAMMOTH_BODY,
        MAMMOTH_R,
    );
    let f = hunt_trip_forecast(
        1,
        &herd,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    println!(
        "[partial] 1-worker big body: killed {} animals, delivered {:.2} / wasted {:.2} food over {:?} turns",
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
    let lean = hunt_trip_forecast(
        1,
        &at_floor,
        0.5,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
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
    let party = spawn_hunt_party(&mut app, home, away, &id, 0.5);

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
        for policy in [0.5, 0.3, 0.15] {
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
                hunt_trip_forecast(
                    PARTY_WORKERS,
                    herd,
                    policy,
                    NO_FILL_TARGET,
                    &fauna,
                    &labor,
                    &cfg,
                    &hunting_party(),
                )
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
/// *config lever*, and a designer raising it must not silently break the client's preview. At 2.0
/// the Surplus/Sustain ceiling on a
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
    reveal_herds(app, herd_ids);
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

        // **THE CLIENT COMPOSES THE CEILING FROM THE PER-BIOMASS VECTOR** — the four ceiling rows
        // are retired, because four rows cannot answer a continuous dial
        // (`docs/plan_harvest_floor.md` §5). The exported rate must be the species' own, so the
        // curve the client draws is the sim's arithmetic and not an approximation of it.
        {
            let live_yield = {
                let registry = app.world.resource::<HerdRegistry>();
                herd_hunt_yield(registry.find(id).expect("herd present"), &fauna)
            };
            assert!(
                (exported.provisions_per_biomass - live_yield.provisions_per_biomass).abs() < 1e-6
                    && (exported.trade_per_biomass - live_yield.trade_goods_per_biomass).abs()
                        < 1e-6,
                "{case}: {id}: the exported per-biomass vector must be the species' own"
            );
        }
        // Every floor a Hunt assignment accepts.
        for policy in SWEPT_FLOORS {
            // A composed ceiling must never promise more than the source is standing there
            // holding. **Inherent** rather than a clamp someone has to remember: it is `B − floor·K`
            // with `floor >= 0`. Composed and bounded against the SAME snapshot's numbers — the live
            // herd moves under the sweep as each staffing takes from it, so mixing the two would
            // compare two different turns.
            let ceiling = (exported.biomass - policy * exported.carrying_capacity).max(0.0)
                * exported.provisions_per_biomass;
            assert!(
                ceiling <= exported.biomass * exported.provisions_per_biomass + TAKE_ABS_EPSILON,
                "{case}: {id} floor {policy}: composed ceiling {ceiling} exceeds the biomass the \
                 same snapshot published"
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
                        &hunting_party(),
                        output_multiplier,
                        workers,
                        policy,
                        NO_IMPROVEMENT_UNDERWAY,
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
                    NO_IMPROVEMENT_UNDERWAY,
                    labor.hunt.per_worker_biomass_capacity,
                    &hunting_party(),
                    &fauna,
                    &LadderConfig::builtin(),
                    f32::INFINITY,
                    // The preview pins `forecast == actual`, so the retreat draw is held fixed —
                    // every species here ships `wariness 0`, making it an identity anyway.
                    0,
                )
                .take;
                let sim_rate = herd_hunt_yield(&herd, &fauna)
                    .apply(take.carried, output_multiplier)
                    .provisions;

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
    // The harvest floor makes that unreachable **by construction**, and by the strongest available
    // argument: every stance's ceiling is now `max(0, B − floor·K) × dip`, which is `≤ B` for any
    // floor `≥ 0` and any dip `≤ 1` — and it **cannot read `r` at all**, the growth rate having been
    // removed from the take path's signature.
    //
    // The pass is kept (retargeted from "the clamp fires" to "nothing can make it need to fire"): it
    // still sweeps the whole preview==take matrix at an off-nominal lever, and it now pins the
    // stronger property. `assert_band_preview_matches_hunt_take` asserts the bound on every row.
    set_fauna_regrowth_rate(&mut app, CLAMP_BINDING_REGROWTH_RATE);
    {
        for policy in SWEPT_FLOORS {
            assert!(
                hunt_escapement_ceiling(policy, depleted_biomass, depleted_cap)
                    <= depleted_biomass,
                "{policy:?}: the escapement ceiling can never exceed the herd's own biomass, at any \
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
    let party = spawn_hunt_party(&mut app, home, pos, &id, 0.3);
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

// The IN-FLIGHT delivery forecast (`expedition_delivery` → the snapshot's
// `expeditionProjectedDelivery` / `expeditionEtaTurns` / `expeditionRecurring`). The in-flight twin of
// the pre-launch `huntTripEstimates`, pinned to a REAL driven party run — never to another forecast.
// ---------------------------------------------------------------------------------------------------

/// Run one real turn of the three systems, in pipeline order (Logistics regrow → Population
/// move → Population expedition step), matching what the forecast forward-simulates.
fn drive_expedition_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_expeditions);
}

/// Pin a big-game herd stationary, **freeze its K** (`fodder_per_biomass = 0` → a non-grazing herd
/// keeps its constant `carrying_capacity`, so the forecast's fixed-`K` clone can't drift from the live
/// run), and seed it full so it stands a large surplus. Returns `(id, position)`.
fn pin_frozen_full_big_herd(app: &mut App) -> (String, UVec2) {
    // A large fixed K (independent of which big-game species the map seeded first) so the surplus
    // above K/2 is worth several turns of hunting whatever the animal's body mass.
    const FROZEN_CAP: f32 = 2000.0;
    let id = pinned_game_herd(app, "big");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.fodder_per_biomass = 0.0;
    herd.carrying_capacity = FROZEN_CAP;
    herd.biomass = FROZEN_CAP;
    // The in-flight delivery forecast does not yet model hunt casualties (Predators Phase 0 — a
    // dangerous herd's real party shrinks below the casualty-free forecast; flagged for Phase 1+). So
    // retag to a harmless species (attack 0) while keeping the heavy frozen stock, exactly as the raid
    // forecast test does, so `delivered == projected` holds here on the delivery mechanics alone.
    herd.species = "Rabbit Warren".to_string();
    let pos = herd.position();
    (id, pos)
}

/// **THE in-flight forecast cannot lie — pinned to a real driven party run.** A live hunting party
/// mid-`Hunting` carries a partial pack; the snapshot must tell the client how much that delivery will
/// finally contain (`expeditionProjectedDelivery`) and roughly when it lands
/// (`expeditionEtaTurns`). We capture those two off the wire, then drive the REAL systems forward and
/// assert the home band actually receives that food, that many turns later. (Unbounded carry ⇒ the
/// raid is surplus-bound and runs several turns, so the party is genuinely mid-flight at capture;
/// frozen `K` ⇒ the forecast is exact.)
#[test]
fn in_flight_delivery_forecast_matches_a_real_party_run() {
    let mut app = build_headless_app();
    app.update();

    // Carry cap effectively unbounded → the raid completes only when the standing surplus is spent,
    // so it spans several turns and the party is genuinely mid-hunt (a partial pack) at capture.
    app.world
        .insert_resource(ExpeditionConfigHandle::new(unbounded_carry_config()));

    let (id, herd_pos) = pin_frozen_full_big_herd(&mut app);
    // A home band far from the herd (beyond comm + drop-off range → no early near-band delivery, so
    // the trip completes on surplus-spent exactly as the forecast assumes) but on the SAME ROW, so the
    // return travel matches the hex-distance ETA. Its larder starts empty.
    let home = spawn_home_band_same_row(&mut app, herd_pos);
    let party = spawn_hunt_party(&mut app, home, herd_pos, &id, 0.5);

    // Drive until the party is mid-Hunting with a partial pack (carried > 0), a couple turns in.
    let mut prehunt = 0;
    while !(phase(&app, party) == ExpeditionPhase::Hunting && carried(&app, party) > 0.0) {
        drive_expedition_turn(&mut app);
        prehunt += 1;
        assert!(
            prehunt < 6,
            "a surplus-bound raid should hunt for several turns"
        );
    }
    assert_eq!(
        phase(&app, party),
        ExpeditionPhase::Hunting,
        "the party must still be hunting when we capture its in-flight forecast"
    );

    // Read the in-flight forecast off the SHIPPED snapshot (the wire), not the helper directly.
    recapture_snapshot_in_place(&mut app.world);
    let (projected, eta, recurring) = {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let pstate = snapshot
            .populations
            .iter()
            .find(|p| p.entity == party.to_bits())
            .expect("the in-flight party is in the snapshot");
        assert!(pstate.is_expedition, "the party is an expedition");
        (
            pstate.expedition_projected_delivery,
            pstate.expedition_eta_turns,
            pstate.expedition_recurring,
        )
    };
    assert!(
        projected > 0.0,
        "a hunting party over a healthy herd projects a positive delivery (got {projected})"
    );
    assert!(
        eta > 0,
        "an in-flight Sustain party has a finite delivery ETA (got {eta})"
    );
    assert!(
        !recurring,
        "Sustain folds home after one trip — never recurring"
    );

    // Drive the REAL systems forward until the home band's larder receives the haul (Returning fold-
    // back deposits it), counting turns from capture.
    let horizon = expedition_config(&app).hunt.forecast_horizon_turns;
    let mut delivery_turn = None;
    let mut delivered = 0.0;
    for turn in 1..=horizon {
        drive_expedition_turn(&mut app);
        let home_food = carried(&app, home);
        if home_food > 0.0 {
            delivery_turn = Some(turn);
            delivered = home_food;
            break;
        }
    }
    let delivery_turn = delivery_turn.expect("the party delivers its haul within the horizon");

    // (a) The delivered food equals what the wire promised. Both accumulate on the same Scalar grid,
    // so a couple of quanta per hunt turn of slop is honest.
    let food_tolerance = 1.0;
    assert!(
        (delivered - projected).abs() <= food_tolerance,
        "delivered {delivered} != projected {projected} (in-flight forecast must not lie)"
    );

    // (b) The delivery lands ~E turns after capture. `eta_turns` is an APPROXIMATION by construction:
    // it counts the full walk home from the herd and does NOT subtract the comm range at which the
    // fold-back actually fires, so the real delivery is EARLIER by ~comm_range/speed turns (plus a
    // framing turn). Assert E is that honest, slightly-conservative upper bound.
    let cfg = expedition_config(&app);
    let comm_range = cfg.effective_comm_range();
    let speed = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_move_tiles_per_turn
        .max(1);
    let eta_slack = (comm_range.div_ceil(speed) + 2) as i64;
    let gap = eta as i64 - delivery_turn as i64;
    assert!(
        (0..=eta_slack).contains(&gap),
        "delivery landed on turn {delivery_turn}; ETA said {eta} (allowed slack {eta_slack})"
    );
}

/// The recurring flag on the wire: a **Deplete** party relaunches for repeated trips, so
/// `expeditionRecurring` must read `true` (Sustain reads `false`, pinned above). Guards the
/// `systems::raid_is_recurring` seam end-to-end through the snapshot.
#[test]
fn a_deplete_party_reports_recurring_on_the_wire() {
    let mut app = build_headless_app();
    app.update();

    let (id, herd_pos) = pin_frozen_full_big_herd(&mut app);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_hunt_party(&mut app, home, herd_pos, &id, 0.15);

    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let pstate = snapshot
        .populations
        .iter()
        .find(|p| p.entity == party.to_bits())
        .expect("the deplete party is in the snapshot");
    assert!(
        pstate.expedition_recurring,
        "a Deplete hunting party relaunches — expeditionRecurring must be true"
    );
}

// ---------------------------------------------------------------------------------------------------
// Regression: a JUST-LAUNCHED hunting party, still traveling toward a HEALTHY herd (NOT in reach),
// must project the SAME delivery the pre-launch `huntTripEstimates` promised for that (policy, party
// size). `expedition_delivery`'s Hunting/Outbound arm runs the forward-sim regardless of reach —
// travel only adds to the ETA — so a far party over a Thriving boar must still project a positive
// delivery, never the "herd lost / no surplus" 0 the client mislabels. Mirrors the live playtest
// where an in-flight party 8 tiles out read `expeditionProjectedDelivery == 0`.
// ---------------------------------------------------------------------------------------------------

/// Pin a big-game herd stationary as a **Wild Boar at `fraction`·K** with a frozen K (`fodder = 0`),
/// well above the Sustain floor (`K/2`), so both forecasts read the same fixed ecology. Returns
/// `(id, herd_pos)`.
fn pin_frozen_boar_herd(app: &mut App, fraction: f32) -> (String, UVec2) {
    let id = pinned_game_herd(app, "big");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.species = "Wild Boar".to_string();
    herd.body_mass = BOAR_BODY;
    herd.fodder_per_biomass = 0.0;
    herd.carrying_capacity = BOAR_K;
    herd.biomass = BOAR_K * fraction;
    herd.hunt_credit = 0.0;
    let pos = herd.position();
    (id, pos)
}

/// **The far/just-launched in-flight forecast must agree with the pre-launch estimate.** A 1-hunter
/// Sustain party, still 8+ tiles from a Thriving boar (NOT in reach), captured right after launch with
/// an empty pack. `expeditionProjectedDelivery` must be positive AND byte-equal to the herd's
/// `huntTripEstimates` deliveredFood for `(Sustain, 1)` — the two forecasts are the same code with
/// `initial_larder = carried = 0`, so a disagreement (the live 0) is the bug.
#[test]
fn a_far_just_launched_party_projects_the_estimate_delivery() {
    let mut app = build_headless_app();
    app.update();

    let (id, herd_pos) = pin_frozen_boar_herd(&mut app, 0.86);
    let home = spawn_home_band(&mut app, herd_pos);

    // Place the party FAR from the herd (well past `reach_tiles`), traveling toward it — the state a
    // party is in the turn it launches. Empty pack, Hunting phase, 1 worker.
    let width = app.world.resource::<TileRegistry>().width;
    let height = app.world.resource::<TileRegistry>().height;
    let far = UVec2::new(
        (herd_pos.x + width / 3) % width,
        (herd_pos.y + height / 3) % height,
    );
    let party = spawn_hunt_party_of(&mut app, home, far, &id, 0.5, BOAR_RAID_CREW);
    app.world
        .entity_mut(party)
        .insert(BandTravel { target: herd_pos });

    // Confirm the reproduction preconditions: the party is out of reach and carries nothing.
    let cfg = expedition_config(&app);
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let dist = core_sim::grid_utils::hex_distance_wrapped(far, herd_pos, width, wrap);
    assert!(
        dist > cfg.hunt.reach_tiles,
        "the party must be OUT of reach (dist {dist} <= reach {})",
        cfg.hunt.reach_tiles
    );
    assert_eq!(carried(&app, party), 0.0, "a just-launched party carries 0");
    assert_eq!(phase(&app, party), ExpeditionPhase::Hunting);

    // Capture the shipped snapshot WITHOUT advancing (the live capture is right after launch).
    // The target herd is far from home, so the fog filter would otherwise withhold it (see
    // `reveal_herds`) and the estimate the party's forecast is compared against would not ship.
    reveal_herds(&mut app, std::slice::from_ref(&id));
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;

    let pstate = snapshot
        .populations
        .iter()
        .find(|p| p.entity == party.to_bits())
        .expect("the in-flight party is in the snapshot");
    let projected = pstate.expedition_projected_delivery;
    let eta = pstate.expedition_eta_turns;

    // The invariant that encodes the user's contradiction: the far in-flight forecast and the
    // pre-launch estimate are the same forecast (carried == 0), so they must agree.
    let herd_state = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the target herd is in the snapshot");
    let estimate = herd_state
        .hunt_trip_estimates
        .iter()
        .find(|e| e.floor == 0.5 && e.party_workers == BOAR_RAID_CREW)
        .expect("a (Sustain, BOAR_RAID_CREW) huntTripEstimate row")
        .delivered_food;
    assert!(
        estimate > 0.0,
        "a Thriving boar at 0.86·K offers surplus — the pre-launch estimate must be positive"
    );
    assert!(
        projected > 0.0,
        "a hunting party over a healthy herd projects a positive delivery even far from it \
         (got {projected}; estimate says {estimate})"
    );
    assert!(
        (projected - estimate).abs() <= 1.0e-4,
        "far in-flight projected {projected} != pre-launch estimate {estimate} \
         (the two forecasts must agree)"
    );
    assert!(
        eta > 0,
        "a far Sustain party has a finite delivery ETA (travel + hunt + walk home)"
    );
}

/// **The only way `expeditionProjectedDelivery` reads 0 over a HEALTHY herd is if the party's target
/// is NOT that herd.** A party whose stored `fauna_id` no longer resolves (it went extinct / was
/// replaced, while a *different* healthy boar sits on the map) hits `expedition_delivery`'s herd-lost
/// branch → `projected_food = carried = 0`. Meanwhile the healthy boar the player sees still exports
/// positive `huntTripEstimates`. This is the live contradiction reproduced: a real, legitimate 0 that
/// belongs to a DIFFERENT herd than the one displayed on the tile — a client disambiguation problem,
/// not a forecast bug. (Also rules out the "workers == 0" suspect: the party has a full worker count.)
#[test]
fn a_lost_target_herd_projects_zero_while_a_healthy_boar_still_estimates_positive() {
    let mut app = build_headless_app();
    app.update();

    // The healthy boar the player is looking at (positive estimates).
    let (healthy, healthy_pos) = pin_frozen_boar_herd(&mut app, 0.86);
    let home = spawn_home_band(&mut app, healthy_pos);

    // A party targeting a DIFFERENT herd id that does not (any longer) resolve in the registry — the
    // lost/replaced target. Positioned far, empty pack, Hunting.
    let width = app.world.resource::<TileRegistry>().width;
    let height = app.world.resource::<TileRegistry>().height;
    let far = UVec2::new(
        (healthy_pos.x + width / 3) % width,
        (healthy_pos.y + height / 3) % height,
    );
    let party = spawn_hunt_party_of(&mut app, home, far, "game_gone", 0.5, BOAR_RAID_CREW);
    app.world
        .entity_mut(party)
        .insert(BandTravel { target: far });

    // The party genuinely has workers — the 0 is NOT the cap-0 early return.
    let workers = available_workers(app.world.get::<PopulationCohort>(party).unwrap().working);
    assert_eq!(
        workers, BOAR_RAID_CREW,
        "the party carries a real worker count"
    );

    reveal_herds(&mut app, std::slice::from_ref(&healthy));
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;

    let pstate = snapshot
        .populations
        .iter()
        .find(|p| p.entity == party.to_bits())
        .expect("the party is in the snapshot");
    assert_eq!(
        pstate.expedition_projected_delivery, 0.0,
        "a party whose target herd is lost projects the food it carries (0) — the live symptom"
    );

    // The healthy boar the player sees STILL offers surplus in its estimates: the two herds are
    // distinct, and only the client can tell the player which one the party is actually chasing.
    let healthy_state = snapshot
        .herds
        .iter()
        .find(|h| h.id == healthy)
        .expect("the healthy boar is in the snapshot");
    let healthy_estimate = healthy_state
        .hunt_trip_estimates
        .iter()
        .find(|e| e.floor == 0.5 && e.party_workers == BOAR_RAID_CREW)
        .expect("a (Sustain, BOAR_RAID_CREW) estimate for the healthy boar")
        .delivered_food;
    assert!(
        healthy_estimate > 0.0,
        "the boar on the tile is healthy — its estimate is positive while the party's target's is 0"
    );
}

/// Home-band distance from the herd for the near-band drop-off test: **inside**
/// `hunt.drop_off_within_tiles` (3), so the near-band gate fires, **and** within
/// `effective_comm_range()` (2) of the party — which spawns *at* the herd — so the drop-off deposits
/// from where the party is standing.
///
/// **Three distinct radii, easily conflated** (this const's doc claimed the wrong one before): the
/// take radius `hunt.reach_tiles` (1, party → herd), the drop-off gate `hunt.drop_off_within_tiles`
/// (3, herd → home band), and the delivery proximity `effective_comm_range()` (2, party → home band,
/// what `near_home` in `advance_expeditions` tests). Only the last two are geometry this test
/// depends on.
///
/// The test does **not** depend on a carry-in: `near_home` is already true on turn 1, so no walking
/// happens between the kill and the deposit. What it pins is that the party **survives** the
/// drop-off and resumes hunting, which is independent of how far the load travels.
const NEAR_BAND_TILES: u32 = 2;

/// A pack a `PARTY_WORKERS` party needs **several turns** to load past the drop-off's worthwhile-load
/// bar (`hunt.min_deliver_fraction` × cap), so a near-band delivery is a genuine **partial** — the
/// only regime in which the drop-off gate is distinguishable from trip completion. Neither shipped
/// value works here: at `per_worker_carry` 0.8 the pack (3.2 food) holds barely one turn's conversion
/// (4 hunters × `per_worker_biomass_capacity` 40 = 160 biomass ≈ 3.2 food), so the raid always ends on
/// `full`; and `unbounded_carry_config`'s 1e6 puts *half* the pack out of reach, so the gate never
/// fires at all.
const DROP_OFF_PER_WORKER_CARRY: f32 = 2.4;

/// The shipped expedition config with the multi-turn pack above.
fn drop_off_pack_config() -> Arc<ExpeditionConfig> {
    let mut cfg = (*ExpeditionConfig::builtin()).clone();
    cfg.hunt.per_worker_carry = DROP_OFF_PER_WORKER_CARRY;
    Arc::new(cfg)
}

/// A home band `tiles_away` tiles along the herd's **row** (so the row offset *is* the hex distance),
/// putting the herd inside the hunt drop-off radius — the near-band geometry every other trip test
/// deliberately avoids (see `spawn_home_band`). Its larder starts empty, so every gain in it is
/// delivered haul.
fn spawn_home_band_near_herd(
    app: &mut App,
    herd_pos: UVec2,
    tiles_away: u32,
) -> bevy::prelude::Entity {
    let width = app.world.resource::<TileRegistry>().width;
    let near = UVec2::new((herd_pos.x + tiles_away) % width, herd_pos.y);
    let tile = tile_at(app, near);
    app.world.spawn((cohort(tile, 10), ResidentBand)).id()
}

/// **A raid does not end because its quarry strolled past camp** (issue #441). A hunting party whose
/// herd wanders within `hunt.drop_off_within_tiles` of the home band used to reach `done` on the
/// near-band gate: it delivered a *partial* pack, folded home and **despawned** with the herd's
/// standing surplus still on the hoof — the trip cancelled by a coincidence of geography. The gate is
/// a **drop-off**: deliver, then resume hunting. The trip ends only on a full pack or the surplus
/// spent.
///
/// Pins all three halves — the delivery happens, the party **survives** it still on the job, and the
/// trip's total haul strictly exceeds the one partial load the old code came home with. This is the
/// `near_band_gate == true` path, which both expedition suites otherwise park the band far away to
/// avoid.
#[test]
fn a_raid_keeps_hunting_when_the_herd_wanders_near_the_band() {
    let mut app = build_headless_app();
    app.update();
    app.world
        .insert_resource(ExpeditionConfigHandle::new(drop_off_pack_config()));

    let (id, herd_pos) = pin_frozen_boar_herd(&mut app, 1.0);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        // Retag to a harmless species (attack 0), as the forecast pins do: casualties would shrink the
        // party mid-trip and move `cap`/`min_deliver` under the assertions below.
        herd.species = "Rabbit Warren".to_string();
        // The documented boar `r` — a pinned herd keeps its ORIGINAL species' growth rate (retagging
        // does not change it), and a fast breeder would regrow its surplus as fast as the raid takes
        // it, so the trip would never reach `surplus_spent` inside the horizon.
        herd.regrowth_rate = BOAR_R;
    }
    let home = spawn_home_band_near_herd(&mut app, herd_pos, NEAR_BAND_TILES);
    let party = spawn_hunt_party(&mut app, home, herd_pos, &id, 0.5);

    // The geometry this test exists for: the herd is inside the drop-off radius, and the party (which
    // spawned at the herd) is inside comm range of the band.
    let cfg = expedition_config(&app);
    assert!(
        NEAR_BAND_TILES <= cfg.hunt.drop_off_within_tiles,
        "the fixture must put the herd INSIDE the drop-off radius ({NEAR_BAND_TILES} tiles, radius {})",
        cfg.hunt.drop_off_within_tiles
    );
    assert!(
        NEAR_BAND_TILES <= cfg.effective_comm_range(),
        "the band must sit within comm range ({}) of the party's kill site, which is what puts it in \
         delivery range from where it stands ({NEAR_BAND_TILES} tiles) — retuning `comm_range_tiles` \
         below that turns this fixture into a carry-in, which `NEAR_BAND_TILES`'s doc denies",
        cfg.effective_comm_range()
    );

    let pack_cap = PARTY_WORKERS as f32 * cfg.hunt.per_worker_carry;
    let food_per_animal = BOAR_BODY
        * app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .hunt
            .provisions_per_biomass;
    let sustain_floor = BOAR_K * 0.5;

    // Drive the REAL systems until the raid ends (the party despawns on its fold-back), watching the
    // home larder for each drop-off and the pack for the fill that would legitimately end the trip.
    let mut first_drop_off = None;
    let mut peak_carried = 0.0_f32;
    let mut despawn_turn = None;
    for turn in 1..=cfg.hunt.forecast_horizon_turns {
        drive_expedition_turn(&mut app);
        let alive = app.world.get::<Expedition>(party).is_some();
        let larder = carried(&app, home);
        if first_drop_off.is_none() && larder > 0.0 {
            // **The regression**: on the old rule the near-band gate fed `done`, so the party folded
            // home and despawned on the very turn it delivered this partial load.
            assert!(
                alive,
                "turn {turn}: the party must SURVIVE its near-band drop-off of {larder} food — \
                 a despawn here is the #441 bug (a raid ended by geography)"
            );
            let phase_now = phase(&app, party);
            assert!(
                matches!(
                    phase_now,
                    ExpeditionPhase::Hunting | ExpeditionPhase::Delivering
                ),
                "turn {turn}: after a drop-off the party is still on the job, not folding home \
                 (phase {phase_now:?})"
            );
            first_drop_off = Some(larder);
        }
        if alive {
            peak_carried = peak_carried.max(carried(&app, party));
        } else {
            despawn_turn = Some(turn);
            break;
        }
    }
    let first_drop_off =
        first_drop_off.expect("a near-band party delivers its worthwhile partial load");
    let despawn_turn = despawn_turn.expect("the raid ends within the forecast horizon");
    let delivered_total = carried(&app, home);
    let leftover = herd_biomass(&app, &id);
    println!(
        "[near-band drop-off] first drop-off {first_drop_off} food, {delivered_total} total over \
         {despawn_turn} turns (pack {pack_cap}, peak carried {peak_carried}, herd left {leftover} \
         vs Sustain floor {sustain_floor})"
    );

    // The point of the fix: the committed party kept raiding, so the trip delivered STRICTLY MORE
    // than the single partial load it used to come home with.
    assert!(
        delivered_total > first_drop_off,
        "the raid must deliver more than its first drop-off ({delivered_total} vs \
         {first_drop_off}) — the party resumed hunting instead of ending the trip"
    );

    // …and it ended the RIGHT way: the herd is within one body of its Sustain floor (the standing
    // surplus is spent) or the pack filled. Never on the drop-off gate.
    let surplus_spent = leftover - sustain_floor < BOAR_BODY;
    let pack_filled = peak_carried >= pack_cap - food_per_animal;
    assert!(
        surplus_spent || pack_filled,
        "the raid must end on a spent surplus (herd left {leftover}, floor {sustain_floor}) or a \
         full pack (peak carried {peak_carried} of {pack_cap})"
    );

    // The drop-off narrates as a `Delivering` drop-off line, not a fold-back — the party reported in
    // and went back out.
    let dropped_off = app.world.resource::<CommandEventLog>().iter().any(|e| {
        e.detail
            .as_deref()
            .is_some_and(|d| d.contains("status=delivered"))
    });
    assert!(
        dropped_off,
        "a near-band drop-off pushes the Delivering feed line (status=delivered)"
    );
}

// ---------------------------------------------------------------------------------------------------
// THE FILL TARGET (`docs/plan_hunt_through_combat.md` §5.2) — the party-side twin of the floor.
//
// The defect it exists to fix (§5.1) is precisely a number that LOOKED like a lever and was not: the
// pack is measured in carry and the take in reach, so `turns_to_fill = per_worker_carry /
// (engage_rate × body_mass × provisions_per_biomass)` and party size cancels out of it entirely.
// Every test below is therefore paired — a lever that moves the trip, beside the invariance it exists
// to escape, beside the identity that makes it safe to land.
// ---------------------------------------------------------------------------------------------------

/// §5.1's own fixture, and the shape of the playtest report: a **Wild Fowl** flock (body `0.13`,
/// `engage_rate` `10`). Its ceiling is the roster's lowest, so an untargeted raid on it takes
/// `0.8 / (10 × 0.13 × 0.02)` ≈ **31 hunting turns at every legal party size** — the "away ≈43 turns,
/// 31 hunting + 12 travel" a player reported with no control that moved it.
const FOWL_BODY: f32 = 0.13;
/// A flock standing far above what any legal party can clear, so the **herd** side never binds and
/// what is measured is purely the party's own stop.
const FOWL_K: f32 = 4000.0;
/// The flock's regrowth while it is not meant to be the binding term.
const FOWL_R: f32 = 0.10;
/// A fill target well under the pack — 100 fowl of the ~307 a single hunter's pack seats.
const SHORT_FILL_TARGET: u32 = 100;
/// A second, larger target: the pair is what pins that the number is a **dial**, not a switch.
const LONGER_FILL_TARGET: u32 = 200;
/// A target far above what any pack here can hold, so `raid_load` must hand the pack's capacity back
/// unchanged — the identity half.
const FILL_TARGET_ABOVE_ANY_PACK: u32 = 10_000;

/// **A target below capacity shortens the trip; a target at or above it is an EXACT identity.**
///
/// Both halves are the assertion (§10). The first alone would pass for a target that silently
/// shortened *every* raid; the second is what makes the slice safe to land, and it is asserted
/// field-for-field rather than on `turns_to_fill` alone — an identity that holds on the turn count
/// while the payload moved would be no identity at all.
#[test]
fn a_fill_target_below_capacity_shortens_the_trip_and_above_it_is_an_identity() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin();
    let flock = wild_herd_of("Wild Fowl", FOWL_K, FOWL_K, FOWL_BODY, FOWL_R);

    let untargeted = hunt_trip_forecast(
        PARTY_WORKERS,
        &flock,
        PEAK_FLOOR,
        NO_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let baseline_turns = untargeted
        .turns_to_fill
        .expect("an untargeted fowl raid fills its pack inside the horizon");
    // **Liveness**: the raid this is measured against actually delivers. A dead raid would satisfy
    // every ordering below by taking nothing at all.
    assert!(
        untargeted.delivered_food > 0.0 && untargeted.animals_taken > 0,
        "the untargeted baseline must be a real raid ({} food, {} animals)",
        untargeted.delivered_food,
        untargeted.animals_taken
    );
    assert_eq!(
        untargeted.bound,
        HuntTripBound::PackFull,
        "with no target the party-side stop is the pack itself"
    );

    let targeted = hunt_trip_forecast(
        PARTY_WORKERS,
        &flock,
        PEAK_FLOOR,
        SHORT_FILL_TARGET,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    let targeted_turns = targeted.turns_to_fill.expect("a targeted raid comes home");
    assert!(
        targeted_turns < baseline_turns,
        "a fill target of {SHORT_FILL_TARGET} must shorten the trip ({targeted_turns} vs \
         {baseline_turns} turns)"
    );
    assert_eq!(
        targeted.bound,
        HuntTripBound::FillTarget,
        "…and the forecast must name the target as what ended it"
    );
    assert_eq!(
        targeted.animals_taken, SHORT_FILL_TARGET,
        "…and the party comes home with the animals it was sent for"
    );

    // The identity half: a target no pack could reach leaves the raid byte-for-byte as it was.
    let over_target = hunt_trip_forecast(
        PARTY_WORKERS,
        &flock,
        PEAK_FLOOR,
        FILL_TARGET_ABOVE_ANY_PACK,
        &fauna,
        &labor,
        &cfg,
        &hunting_party(),
    );
    assert_eq!(
        (
            over_target.turns_to_fill,
            over_target.animals_taken,
            over_target.delivered_food,
            over_target.wasted_food,
            over_target.delivered_trade,
            over_target.first_turn_provisions,
            over_target.bound,
        ),
        (
            untargeted.turns_to_fill,
            untargeted.animals_taken,
            untargeted.delivered_food,
            untargeted.wasted_food,
            untargeted.delivered_trade,
            untargeted.first_turn_provisions,
            untargeted.bound,
        ),
        "a target at or above the pack's capacity must be EXACTLY the untargeted raid"
    );
}

/// **Trip length responds to the target — and, with no target, does NOT respond to party size.**
///
/// The pair is the whole point (§10). Two different targets must give two different trip lengths,
/// because the defect being fixed is a number that looked like a lever and was not; and the
/// invariance that *caused* that defect is still true, which is what the target exists to escape.
/// Party size is not inert in general — it moves the **payload** — so that is asserted too, or the
/// invariance half would read as "party size does nothing".
#[test]
fn trip_length_responds_to_the_fill_target_but_not_to_party_size_without_one() {
    let fauna = FaunaConfig::builtin();
    let labor = LaborConfig::builtin();
    let cfg = ExpeditionConfig::builtin();
    let flock = wild_herd_of("Wild Fowl", FOWL_K, FOWL_K, FOWL_BODY, FOWL_R);
    let turns_for = |workers: u32, target: u32| {
        hunt_trip_forecast(
            workers,
            &flock,
            PEAK_FLOOR,
            target,
            &fauna,
            &labor,
            &cfg,
            &hunting_party(),
        )
    };

    // (a) The lever: two targets, two trip lengths.
    let short = turns_for(PARTY_WORKERS, SHORT_FILL_TARGET);
    let longer = turns_for(PARTY_WORKERS, LONGER_FILL_TARGET);
    let short_turns = short.turns_to_fill.expect("the shorter target comes home");
    let longer_turns = longer.turns_to_fill.expect("the longer target comes home");
    assert!(
        short_turns < longer_turns,
        "asking for {LONGER_FILL_TARGET} fowl must take strictly longer than asking for \
         {SHORT_FILL_TARGET} ({short_turns} vs {longer_turns} turns)"
    );
    assert!(
        short.animals_taken > 0 && longer.animals_taken > short.animals_taken,
        "…and the longer trip must actually bring back more ({} then {})",
        short.animals_taken,
        longer.animals_taken
    );

    // (b) The invariance that caused the bug: with NO target, every legal party size spends the same
    // number of turns, because the pack and the take both scale with the crew.
    let mut baseline: Option<u32> = None;
    let mut smallest_payload = 0u32;
    let mut largest_payload = 0u32;
    for workers in 2..=cfg.max_party_size {
        let f = turns_for(workers, NO_FILL_TARGET);
        let turns = f
            .turns_to_fill
            .expect("an untargeted fowl raid fills its pack");
        match baseline {
            None => {
                baseline = Some(turns);
                smallest_payload = f.animals_taken;
            }
            Some(first) => assert_eq!(
                turns, first,
                "party size must NOT move an untargeted raid's length ({workers} hunters took \
                 {turns} turns, 2 took {first}) — that is §5.1's defect, and the fill target is the \
                 escape from it, not a repair of it"
            ),
        }
        largest_payload = f.animals_taken;
    }
    // **Liveness**: party size is not inert — it moves the payload, just never the trip length.
    assert!(
        largest_payload > smallest_payload && smallest_payload > 0,
        "a bigger party must bring back more fowl ({smallest_payload} then {largest_payload})"
    );
}

/// A stationary herd pinned to a chosen species, capacity, stock and regrowth, with **`K` frozen**
/// (`fodder_per_biomass = 0` → a non-grazing herd keeps its constant `carrying_capacity`) so the
/// live run and the forecast's clone cannot drift apart. Returns `(id, position)`.
///
/// The generalisation of [`pin_frozen_full_big_herd`], which pins one particular full herd; the bound
/// tests need three *different* ecologies off the same seam.
fn pin_frozen_herd(
    app: &mut App,
    species: &str,
    body_mass: f32,
    cap: f32,
    biomass: f32,
    regrowth_rate: f32,
) -> (String, UVec2) {
    let id = pinned_game_herd(app, "big");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.fodder_per_biomass = 0.0;
    herd.species = species.to_string();
    // **The body mass has to move with the species.** The two are read from different places — the
    // engagement rate off the species' display name, the body off the herd's own field — so a retag
    // that left the seeded big-game body behind would quote a fill target in *mammoth* biomass and
    // the target would never bind.
    herd.body_mass = body_mass;
    herd.carrying_capacity = cap;
    herd.biomass = biomass;
    herd.regrowth_rate = regrowth_rate;
    let pos = herd.position();
    (id, pos)
}

/// The bound the wire reports for a live party, read off the **exported snapshot** — never off the
/// in-process forecast, which is the value the client cannot see.
fn exported_trip_bound(app: &mut App, party: bevy::prelude::Entity) -> String {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let pstate = snapshot
        .populations
        .iter()
        .find(|p| p.entity == party.to_bits())
        .expect("the party is in the snapshot");
    assert!(pstate.is_expedition, "the party is an expedition");
    pstate.expedition_trip_bound.clone()
}

/// The herd's live biomass and the floor it is being raided to, so a test can say which side of the
/// completion actually fired.
fn floor_biomass_of(app: &App, id: &str, floor: f32) -> f32 {
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("the herd is alive");
    floor * herd.carrying_capacity
}

/// Drive the real systems until the party stops `Hunting`, returning the turn it left on.
/// `None` = it was still hunting after `limit` turns.
fn drive_until_hunt_ends(app: &mut App, party: bevy::prelude::Entity, limit: u32) -> Option<u32> {
    for turn in 1..=limit {
        drive_expedition_turn(app);
        if phase(app, party) != ExpeditionPhase::Hunting {
            return Some(turn);
        }
    }
    None
}

/// **The exported bound names the stop that ACTUALLY ended the raid** — a fill-target raid, a
/// floor-bound raid, and one that reaches neither inside the horizon.
///
/// This is the readout the design turns on (§5.2): *"you come home on your fill target in 4 turns;
/// the herd never reaches the floor"* against *"you reach the floor in 2 turns with the pack a third
/// full"*. A turn count alone cannot distinguish them, so each case asserts the **name** on the wire
/// **and** the world state the named stop implies once the real systems have run.
#[test]
fn the_exported_bound_names_the_stop_that_ends_the_raid() {
    // (a) FILL-TARGET bound: a full flock the party could raid for 31 turns, told to come home with
    // `SHORT_FILL_TARGET` fowl.
    {
        let mut app = build_headless_app();
        app.update();
        let (id, herd_pos) =
            pin_frozen_herd(&mut app, "Wild Fowl", FOWL_BODY, FOWL_K, FOWL_K, FOWL_R);
        let home = spawn_home_band_same_row(&mut app, herd_pos);
        let party = spawn_hunt_party_targeting(
            &mut app,
            home,
            herd_pos,
            &id,
            PEAK_FLOOR,
            PARTY_WORKERS,
            SHORT_FILL_TARGET,
        );
        assert_eq!(
            exported_trip_bound(&mut app, party),
            "fill_target",
            "a raid told to stop at {SHORT_FILL_TARGET} fowl comes home on its target"
        );
        let left_on = drive_until_hunt_ends(&mut app, party, 31)
            .expect("a target-bound raid comes home well inside the untargeted 31 turns");
        assert!(
            left_on < 31,
            "…and does so before the untargeted raid would have (left on turn {left_on})"
        );
        // The named bound's world-state consequence: the herd never came near its floor, so nothing
        // but the target can have ended this trip.
        let floor_biomass = floor_biomass_of(&app, &id, PEAK_FLOOR);
        let biomass = herd_biomass(&app, &id);
        assert!(
            biomass > floor_biomass * 1.5,
            "the flock must be nowhere near its floor ({biomass} vs floor {floor_biomass})"
        );
    }

    // (b) FLOOR bound: the same flock standing barely above its floor, with regrowth off, so the
    // standing surplus is spent long before any pack fills.
    {
        const NO_REGROWTH: f32 = 0.0;
        let mut app = build_headless_app();
        app.update();
        let lean = FOWL_K * PEAK_FLOOR * 1.02;
        let (id, herd_pos) =
            pin_frozen_herd(&mut app, "Wild Fowl", FOWL_BODY, FOWL_K, lean, NO_REGROWTH);
        let home = spawn_home_band_same_row(&mut app, herd_pos);
        let party = spawn_hunt_party_targeting(
            &mut app,
            home,
            herd_pos,
            &id,
            PEAK_FLOOR,
            PARTY_WORKERS,
            FILL_TARGET_ABOVE_ANY_PACK,
        );
        assert_eq!(
            exported_trip_bound(&mut app, party),
            "floor",
            "a raid on a flock at its floor comes home because the herd ran out, not the pack"
        );
        drive_until_hunt_ends(&mut app, party, 31).expect("a floor-bound raid comes home");
        let floor_biomass = floor_biomass_of(&app, &id, PEAK_FLOOR);
        let biomass = herd_biomass(&app, &id);
        assert!(
            biomass - floor_biomass < FOWL_BODY,
            "the flock must be drawn to within one body of its floor ({biomass} vs floor \
             {floor_biomass})"
        );
        // Liveness — it really raided; a raid that took nothing would also sit at the floor.
        assert!(
            carried(&app, party) > 0.0 || carried(&app, home) > 0.0,
            "a floor-bound raid still brings its partial load home"
        );
    }

    // (c) HORIZON: a lone hunter with an effectively unbounded pack on a fast-breeding warren whose
    // regrowth outruns what one party can reach. Neither stop is ever hit.
    {
        const WARREN_BODY: f32 = 0.27;
        const WARREN_K: f32 = 2000.0;
        const WARREN_R: f32 = 0.05;
        const LONE_HUNTER: u32 = 1;
        let mut app = build_headless_app();
        app.update();
        app.world
            .insert_resource(ExpeditionConfigHandle::new(unbounded_carry_config()));
        let (id, herd_pos) = pin_frozen_herd(
            &mut app,
            "Rabbit Warren",
            WARREN_BODY,
            WARREN_K,
            WARREN_K,
            WARREN_R,
        );
        let home = spawn_home_band_same_row(&mut app, herd_pos);
        let party = spawn_hunt_party_targeting(
            &mut app,
            home,
            herd_pos,
            &id,
            PEAK_FLOOR,
            LONE_HUNTER,
            NO_FILL_TARGET,
        );
        assert_eq!(
            exported_trip_bound(&mut app, party),
            "horizon",
            "a raid that can neither fill nor exhaust must say so, not name a stop"
        );
        let horizon = expedition_config(&app).hunt.forecast_horizon_turns;
        assert!(
            drive_until_hunt_ends(&mut app, party, horizon).is_none(),
            "…and the real party must still be hunting when the horizon runs out"
        );
        // Liveness — it is hunting, not stalled: the warren has really been drawn on.
        assert!(
            herd_biomass(&app, &id) < WARREN_K,
            "a party that hunts for {horizon} turns must have taken something"
        );
    }
}

/// **Every pre-launch estimate row names its bound, and none of them names the fill target.**
///
/// The table is **band-agnostic and untargeted** — it samples floor × party size, and a fill target
/// is chosen at launch — so `"fill_target"` is unreachable here by construction. Pinned both ways:
/// the field is populated on every row (an empty string would be a silently unwritten column that no
/// client could branch on), and it never claims a target the row was not simulated with.
#[test]
fn every_pre_launch_estimate_row_names_an_untargeted_bound() {
    let mut app = build_headless_app();
    app.update();
    let (id, _) = pin_frozen_herd(&mut app, "Wild Fowl", FOWL_BODY, FOWL_K, FOWL_K, FOWL_R);
    reveal_herds(&mut app, std::slice::from_ref(&id));
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd_state = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the flock is in the snapshot");
    assert!(
        !herd_state.hunt_trip_estimates.is_empty(),
        "a huntable flock publishes a trip table — otherwise this test asserts nothing"
    );
    for row in &herd_state.hunt_trip_estimates {
        assert!(
            !row.bound.is_empty(),
            "every row must name the stop that ended it (floor {}, {} workers)",
            row.floor,
            row.party_workers
        );
        assert_ne!(
            row.bound, "fill_target",
            "the pre-launch table is UNTARGETED — no row may claim a fill target it was not \
             simulated with (floor {}, {} workers)",
            row.floor, row.party_workers
        );
    }
}
