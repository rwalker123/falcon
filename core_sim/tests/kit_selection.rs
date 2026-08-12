//! **Kit selection** — a party is sent out with a *named* kit from `equipment.json`'s roster
//! instead of implicitly using whatever the band owns.
//!
//! The whole design rests on one claim: **a kit is a MASK over the three predicates that already
//! existed**, so choosing one of the two working kits is bit-for-bit the shipped game. The first
//! test in this file is that claim; everything else is what the mask buys once it can be empty.
//!
//! The `none` kit is not a sentinel and nothing here treats it as one — it is an ordinary roster
//! entry whose `uses` list happens to be empty, which is why it reads false at every predicate and,
//! **because wear rides those same predicates**, spends no durability. That pairing is the one this
//! file exists to guard: a bare-handed comparison that consumed the very kit it is being compared
//! against would not be a comparison.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_expeditions, advance_herds, advance_labor_allocation, advance_tick, build_headless_app,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, BandEquipment,
    EffectTier, EquipmentConfig, EquipmentConfigHandle, EquipmentStat, Expedition,
    ExpeditionMission, ExpeditionPhase, FactionId, FaunaConfigHandle, GenerationId, HerdRegistry,
    HerdTelemetry, KitChoice, KitJob, LaborAllocation, LaborAssignment, LaborTarget, LocalStore,
    MoraleCause, PopulationCohort, ResidentBand, SimulationConfig, SnapshotHistory, SourceYield,
    StartingUnit, TileRegistry, WearQuantum, DEFAULT_ESCAPEMENT_FLOOR,
};

/// The crew every fixture in this file staffs, so two arms are only ever comparable to each other.
const CREW: u32 = 4;

/// **The one roster entry whose three appended tiers all differ from the job default each would
/// otherwise resolve to** — pen 40 (the equipped rate) where the hunt default falls back to 12,
/// vantage 1 where the scout default is 2, and no weapon at all where the warrior default carries
/// clubs. Named here so the in-flight-party fixture says *why* it picks this kit.
const HUSBANDRY_KIT: &str = "husbandry";

/// **A quarry that cannot fight back** (`combat.attack 0`) and is light enough that a small crew
/// engages several animals a turn — so a take is a real number at both tiers rather than a run of
/// all-or-nothing draws. The same species `denial_raid.rs` measures on, for the same reason.
const HARMLESS_QUARRY: &str = "Rabbit Warren";

/// **A quarry past the trap's `max_body_mass` and carrying real `defense`** — the other side of the
/// per-quarry default. A snare cannot hold a Red Deer, so `traps` grants no attack at all and the
/// party falls back to the bare hand's `1` against `defense 1`: the gate refuses the hunt and the
/// kit scores zero.
const DEFENDED_QUARRY: &str = "Red Deer";

/// The roster entry a small, wary quarry scores best against — named so the assertions read as the
/// claim rather than as a string.
const TRAPPING_KIT: &str = "trapping";

/// The id every fixture renames its herd to. Pinned because the live retreat draws from
/// `retreat_seed(map_seed, tick, herd_id, workers)` and the campaign's herd ids vary run to run.
const PINNED_HERD_ID: &str = "kit_fixture_herd";

/// Standing stock and body for the pinned herd — stated here so a test says what it measures
/// against rather than inheriting the roster's numbers by accident. Light bodies and a big stock:
/// the crew is engagement-bound at both kit tiers, so the *carry* tier is what separates them.
const HERD_BODY_MASS: f32 = 1.0;
const HERD_CAPACITY: f32 = 4000.0;

/// A world with the roster's authored wariness held at `0`, so a two-arm comparison measures the
/// kit rather than two different draws.
fn placid_world() -> App {
    let mut app = build_headless_app();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    // **Fog off**, so the wire fixtures read the herd they pinned rather than whatever the starting
    // band happens to be able to see. `herd_snapshot_entries` filters on live visibility, and a
    // fixture that pins a herd across the map would otherwise be asserting about fog.
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.update();
    app
}

/// **A world running the roster's AUTHORED wariness** — the opposite fixture to [`placid_world`],
/// for the tests whose whole subject is the retreat the trap avoids. Holding wariness at `0` there
/// would delete the term the per-quarry default turns on.
fn wary_world() -> App {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.update();
    app
}

/// Pin one stationary herd to its tile under `id`, re-badged as `species`. Unlike [`pin_herd`] it
/// leaves the roster's own body mass and stock alone — the per-quarry default is scored off the
/// **species row**, so a fixture that overrode `body_mass` would be measuring nothing.
fn pin_herd_of(app: &mut App, species: &str, id: &str) -> String {
    let picked = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .map(|h| h.id.clone())
            .expect("the campaign map seeds enough stationary game groups")
    };
    // `body_mass` and the ladder ceiling are CACHED on the `Herd` at spawn, so a re-badge that left
    // them behind would leave a herd claiming one species and behaving as another — which the pen
    // fixture below would hit as a silently-refused `corral_at`.
    let (body_mass, husbandry_ceiling) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let def = fauna
            .species_by_display(species)
            .expect("the roster ships this species");
        (def.body_mass, def.husbandry_ceiling)
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == picked).unwrap();
        herd.id = id.to_string();
        herd.route = vec![herd.current_pos];
        herd.step_index = 0;
        herd.species = species.to_string();
        herd.body_mass = body_mass;
        herd.husbandry_ceiling = husbandry_ceiling;
        herd.fodder_per_biomass = 0.0;
    }
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<HerdTelemetry>().entries = entries;
    id.to_string()
}

/// `HerdTelemetryState.defaultKitId` read off the **encoded envelope**, through the client's own
/// accessor chain — a field that never reached the codec still passes an in-process assertion.
///
/// **Encoded here rather than through `StoredSnapshot::encode_flat`.** This asserts on frame
/// *content*, and encoding the entry's snapshot directly says so: `encode_flat` is a read of stored
/// bytes whose header carries whatever sequence number the entry was published under, which is a
/// concern this fixture has no stake in.
fn published_default_kit(app: &App, herd_id: &str) -> String {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let herds = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list");
    let herd = herds
        .iter()
        .find(|herd| herd.id() == Some(herd_id))
        .unwrap_or_else(|| panic!("the pinned herd '{herd_id}' is on the wire"));
    herd.defaultKitId()
        .expect("every herd publishes the kit its sheet opens on")
        .to_string()
}

fn equipment(app: &App) -> std::sync::Arc<EquipmentConfig> {
    app.world.resource::<EquipmentConfigHandle>().get()
}

fn kit(app: &App, id: &str) -> KitChoice {
    equipment(app)
        .kit(id)
        .unwrap_or_else(|| panic!("the roster ships '{id}'"))
}

/// Pin a stationary herd to one tile with the fixture's own species, body and stock.
fn pin_herd(app: &mut App) -> (String, UVec2) {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .map(|h| h.id.clone())
            .expect("the campaign map seeds at least one stationary game group")
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.id = PINNED_HERD_ID.to_string();
    herd.route = vec![herd.current_pos];
    herd.step_index = 0;
    herd.species = HARMLESS_QUARRY.to_string();
    herd.body_mass = HERD_BODY_MASS;
    herd.carrying_capacity = HERD_CAPACITY;
    herd.biomass = HERD_CAPACITY;
    herd.regrowth_rate = 0.10;
    // The herd draws nothing from the pasture layer, so `advance_herds` cannot recompute `K` out
    // from under a two-arm comparison.
    herd.fodder_per_biomass = 0.0;
    let pos = herd.position();
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<HerdTelemetry>().entries = entries;
    (PINNED_HERD_ID.to_string(), pos)
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
        last_turn_transfer_received: 0.0,
        last_turn_transfer_sent: 0.0,
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

/// A **resident band** standing on the herd's tile, staffed onto a hunt under `kit`.
fn spawn_hunting_band(
    app: &mut App,
    pos: UVec2,
    fauna_id: &str,
    kit: Option<KitChoice>,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    app.world
        .spawn((
            cohort(tile, CREW),
            ResidentBand,
            // **Outfitted for the crew it staffs.** A spawn stocks a party's worth, and the one-unit
            // reference ledger would arm one of these four and send three out bare-handed — a
            // fixture about a shortfall, not about the kit these tests measure.
            BandEquipment::start_stocked_for(&EquipmentConfig::builtin(), CREW as f32),
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                    },
                    workers: CREW,
                    improvement: None,
                    kit,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// A detached party already in the `Hunting` phase on the herd's tile, sent out with `kit`.
fn spawn_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    fauna_id: &str,
    kit: KitChoice,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    // The name a launched party would carry, resolved off the registry as `outfit_raiding_party` does.
    // Display-only — every mechanic here resolves the herd through `fauna_id`.
    let target_species = app
        .world
        .resource::<HerdRegistry>()
        .find(fauna_id)
        .map(|herd| herd.species.clone())
        .unwrap_or_default();
    app.world
        .spawn((
            cohort(tile, CREW),
            LaborAllocation::default(),
            // Outfitted, for the reason the resident band above is.
            BandEquipment::start_stocked_for(&EquipmentConfig::builtin(), CREW as f32),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.to_string(),
                    target_species,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit,
                cargo: LocalStore::new(),
            },
        ))
        .id()
}

/// A home band placed far from the herd, so no near-band drop-off interferes with the raid's cycle.
fn spawn_home_band(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let far = UVec2::new(
        (herd_pos.x + width / 3) % width,
        (herd_pos.y + height / 3) % height,
    );
    let tile = tile_at(app, far);
    app.world
        .spawn((
            cohort(tile, 20),
            ResidentBand,
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
        ))
        .id()
}

fn wear_of(app: &App, entity: bevy::prelude::Entity) -> BandEquipment {
    app.world
        .get::<BandEquipment>(entity)
        .expect("the fixture spawned a wear ledger")
        .clone()
}

fn first_yield(app: &App, band: bevy::prelude::Entity) -> SourceYield {
    app.world
        .get::<LaborAllocation>(band)
        .and_then(|allocation| allocation.last_yields.first().cloned())
        .expect("the band's one assignment has a telemetry row")
}

/// One turn as a resident hunt sees it: the herd's ecology, then the band's take, then the tick.
fn drive_local_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_labor_allocation);
    app.world.run_system_once(advance_tick);
}

/// One turn as a detached party sees it.
fn drive_party_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_expeditions);
    app.world.run_system_once(advance_tick);
}

// ---------------------------------------------------------------------------------------------
// THE LOAD-BEARING CLAIM: the mask is a NO-OP for the two working kits
// ---------------------------------------------------------------------------------------------

/// **Naming the job's kit and naming nothing are the same order, bit for bit.** The roster's whole
/// safety argument is that `big_game` masks in exactly the components the hunt path used to consult
/// unconditionally — so a band that names it must take, waste, wear and forecast *identically* to
/// one that names nothing at all.
///
/// Driven through a real `advance_labor_allocation` rather than compared at the config seam
/// (`equipment_config::tests::the_two_shipped_kits_reproduce_the_pre_roster_predicates` does that),
/// because the mask is threaded through the take, the telemetry **and** the wear charge, and only a
/// resolved turn exercises all three at once.
#[test]
fn naming_the_jobs_own_kit_reproduces_the_default_take_bit_for_bit() {
    let run = |named: bool| {
        let mut app = placid_world();
        let (id, pos) = pin_herd(&mut app);
        let chosen = named.then(|| kit(&app, "big_game"));
        let band = spawn_hunting_band(&mut app, pos, &id, chosen);
        for _ in 0..4 {
            drive_local_turn(&mut app);
        }
        let biomass = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .map(|herd| herd.biomass);
        (first_yield(&app, band), wear_of(&app, band), biomass)
    };

    let (named_yield, named_wear, named_biomass) = run(true);
    let (default_yield, default_wear, default_biomass) = run(false);

    assert_eq!(
        named_yield, default_yield,
        "the mask is a no-op for the hunt's own kit — the resolved yield row must be identical"
    );
    assert_eq!(
        named_wear, default_wear,
        "…and so must the durability it spent, since wear rides the same predicate as the tier"
    );
    assert_eq!(
        named_biomass, default_biomass,
        "…and so must the herd it left behind"
    );
    // Liveness: a comparison of two zeros would pass while the feature was dead.
    assert!(
        named_yield.actual > 0.0 && named_wear.wear_of("spears") > 0.0,
        "the fixture must actually hunt and actually wear its spears: {named_yield:?} {named_wear:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// WHAT AN EMPTY MASK BUYS
// ---------------------------------------------------------------------------------------------

/// **A crew sent out with no kit runs at the unequipped tiers and spends NOTHING.**
///
/// The two halves are one claim, not two: wear is gated on the same effective predicate that chose
/// the tier, so a crew that is not using a component cannot be charged for it. Get that wrong and a
/// bare-handed comparison silently consumes the kit it is being compared against — the player pays
/// for the experiment they ran to decide not to.
#[test]
fn a_crew_with_no_kit_takes_less_and_spends_no_durability_on_any_component() {
    let run = |kit_id: &str| {
        let mut app = placid_world();
        let (id, pos) = pin_herd(&mut app);
        let chosen = kit(&app, kit_id);
        let band = spawn_hunting_band(&mut app, pos, &id, Some(chosen));
        for _ in 0..4 {
            drive_local_turn(&mut app);
        }
        (first_yield(&app, band), wear_of(&app, band))
    };

    let (kitted_yield, kitted_wear) = run("big_game");
    let (bare_yield, bare_wear) = run("none");

    assert_eq!(
        bare_wear,
        BandEquipment::start_stocked_for(&EquipmentConfig::builtin(), CREW as f32),
        "a crew using no component spends no durability on ANY of them — this is the pairing that \
         makes a bare-handed comparison free to run (compared against the ledger the fixture band \
         is OUTFITTED with, which is a party's worth)"
    );
    assert!(
        kitted_wear.wear_of("spears") > 0.0 && kitted_wear.wear_of("sled") > 0.0,
        "…and the kitted arm beside it must genuinely wear both, or the assertion above is vacuous: \
         {kitted_wear:?}"
    );
    assert!(
        bare_yield.actual < kitted_yield.actual,
        "a bare-handed crew hauls the sled's unequipped rate and fights at the person's own attack, \
         so it must bring home less: {bare_yield:?} vs {kitted_yield:?}"
    );
}

/// **A gather crew's kit is the BASKET's, and nothing else's.** The forage arm charges its own
/// quantum, so a `none` gather must leave the baskets untouched while a `gathering` one wears them
/// — the one-kit-one-job split, expressed through the mask.
///
/// **Both arms run in ONE world, on ONE patch, with the patch reset between them.** The map seed is
/// entropy by default, so two `build_headless_app()` worlds seed different flora on different
/// ground and a two-world comparison would be measuring the roll rather than the kit.
#[test]
fn a_gather_crew_wears_only_the_baskets_and_a_kitless_one_wears_nothing() {
    let mut app = placid_world();
    // Any food-bearing tile the band can stand on and work.
    let (tile_pos, tile_entity) = {
        let registry = app.world.resource::<TileRegistry>();
        let mut found = None;
        'outer: for y in 0..registry.height {
            for x in 0..registry.width {
                if let Some(entity) = registry.index(x, y) {
                    if app.world.get::<core_sim::FoodModuleTag>(entity).is_some() {
                        found = Some((UVec2::new(x, y), entity));
                        break 'outer;
                    }
                }
            }
        }
        found.expect("the campaign map seeds at least one food module")
    };
    // The patch as the world seeded it — restored between the arms so both work the same ground.
    let pristine = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patch(tile_pos)
        .cloned();

    let arm = |app: &mut App, kit_id: &str| {
        if let Some(patch) = pristine.clone() {
            app.world
                .resource_mut::<core_sim::ForageRegistry>()
                .patches
                .insert(tile_pos, patch);
        }
        let chosen = kit(app, kit_id);
        let band = app
            .world
            .spawn((
                cohort(tile_entity, CREW),
                ResidentBand,
                BandEquipment::start_stocked(&EquipmentConfig::builtin()),
                LaborAllocation {
                    assignments: vec![LaborAssignment {
                        target: LaborTarget::Forage {
                            tile: tile_pos,
                            floor: DEFAULT_ESCAPEMENT_FLOOR,
                            species: None,
                        },
                        workers: CREW,
                        improvement: None,
                        kit: Some(chosen),
                    }],
                    ..Default::default()
                },
            ))
            .id();
        drive_local_turn(app);
        let result = (first_yield(app, band), wear_of(app, band));
        // The arms are sequential on one patch, so each band leaves before the next arrives.
        app.world.despawn(band);
        result
    };

    let (kitted_yield, kitted_wear) = arm(&mut app, "gathering");
    let (bare_yield, bare_wear) = arm(&mut app, "none");

    assert!(
        kitted_wear.wear_of("baskets") > 0.0,
        "a kitted gather wears its baskets: {kitted_wear:?}"
    );
    assert_eq!(
        kitted_wear.wear_of("spears"),
        0.0,
        "…and nothing else — a gather blunts no spears"
    );
    assert_eq!(kitted_wear.wear_of("sled"), 0.0, "…and drags no sled");
    assert_eq!(
        bare_wear,
        BandEquipment::start_stocked(&EquipmentConfig::builtin()),
        "a crew gathering by hand wears nothing at all"
    );
    assert!(
        bare_yield.actual < kitted_yield.actual,
        "two cupped hands bring back less than a basketful: {bare_yield:?} vs {kitted_yield:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// THE PARTY CARRIES ITS CHOICE
// ---------------------------------------------------------------------------------------------

/// **A party's kit is resolved once at launch and does not re-resolve against the band's stock.**
///
/// The failure this pins is specific and silent: a party sent out with `none` that re-read the
/// band's components each turn would quietly re-arm itself, and the raid the player sent out
/// bare-handed would come home having hunted at attack 20. So the home band's kit is destroyed
/// *under* the party mid-raid, and the party's take must not move.
#[test]
fn a_partys_kit_survives_the_home_bands_stock_changing_under_it() {
    let mut app = placid_world();
    let (id, pos) = pin_herd(&mut app);
    let home = spawn_home_band(&mut app, pos);
    let bare = kit(&app, "none");
    let party = spawn_party(&mut app, home, pos, &id, bare);

    // Two turns with the home band fully kitted, then destroy its kit and run two more.
    for _ in 0..2 {
        drive_party_turn(&mut app);
    }
    let before = wear_of(&app, party);
    {
        let cfg = equipment(&app);
        let mut band_kit = app
            .world
            .get_mut::<BandEquipment>(home)
            .expect("the home band carries a wear ledger");
        run_dry(&mut band_kit, &cfg, "spears");
        run_dry(&mut band_kit, &cfg, "sled");
        run_dry(&mut band_kit, &cfg, "baskets");
    }
    for _ in 0..2 {
        drive_party_turn(&mut app);
    }

    assert_eq!(
        app.world
            .get::<Expedition>(party)
            .expect("the party is still in the field")
            .kit
            .id(),
        "none",
        "the party keeps the kit it was sent with"
    );
    assert_eq!(
        wear_of(&app, party),
        before,
        "a party using no component spends none, whatever the band it came off is holding"
    );
}

/// **A party's own wear still moves it, which is what makes the choice a mask and not a freeze.** A
/// `big_game` party whose spears run dry steps down to the bare-handed tier exactly as it always
/// did; what the roster fixed in place is *which components it reaches for*, not their condition.
#[test]
fn a_kitted_partys_own_wear_still_steps_it_down() {
    let mut app = placid_world();
    let (id, pos) = pin_herd(&mut app);
    let home = spawn_home_band(&mut app, pos);
    let kitted = kit(&app, "big_game");
    let party = spawn_party(&mut app, home, pos, &id, kitted);

    drive_party_turn(&mut app);
    let fresh_take = wear_of(&app, party).wear_of("spears");
    assert!(fresh_take > 0.0, "a kitted party blunts its spears");

    // Run the party's own spears to the cliff and give it another turn.
    {
        let cfg = equipment(&app);
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(party)
            .expect("the party carries a wear ledger");
        run_dry(&mut wear, &cfg, "spears");
    }
    let sled_before = wear_of(&app, party).wear_of("sled");
    drive_party_turn(&mut app);
    let after = wear_of(&app, party);
    assert_eq!(
        after.count_of("spears"),
        0,
        "the party's spears are gone — a batch that runs out is removed, not kept at zero"
    );
    assert_eq!(
        after.wear_of("spears"),
        0.0,
        "spent spears are not charged again — the predicate that chose the tier gates the charge, \
         and there is no ledger left to run past its own durability"
    );
    assert!(
        after.wear_of("sled") >= sled_before,
        "…while the sled, which is still serving, goes on being charged for what it hauls"
    );
}

// ---------------------------------------------------------------------------------------------
// THE WIRE
// ---------------------------------------------------------------------------------------------

/// **The roster reaches the client, tiers and carried items and all.**
///
/// This used to also assert the two estimate tables' `*_kit_id` disclaimers — the fields that told a
/// client *"these rows were priced at the hunt default, so refuse to show them for any other
/// selection"*. Both the tables and their disclaimers are retired: a client now **asks**
/// (`crate::forecast_query`) and names the kit in the question, so there is no mismatch left to
/// warn about. What still has to reach the wire is the picker's own data, which is this.
#[test]
fn the_published_snapshot_carries_the_kit_roster() {
    let mut app = placid_world();
    let (id, _pos) = pin_herd(&mut app);
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;

    let cfg = equipment(&app);
    let expected_hunt_default = cfg.default_kit_id(KitJob::Hunt);
    assert_eq!(snapshot.default_hunt_kit_id, expected_hunt_default);
    assert_eq!(
        snapshot.default_forage_kit_id,
        cfg.default_kit_id(KitJob::Forage)
    );
    assert_eq!(
        snapshot.kits.len(),
        cfg.kits().len(),
        "every roster entry is published — the picker's list is the config's list"
    );

    // The tiers ride the roster so the client renders real numbers without a second TOE table, and
    // `none`'s are the unequipped ones.
    let bare = snapshot
        .kits
        .iter()
        .find(|option| option.id == "none")
        .expect("the roster ships `none`");
    assert_eq!(
        bare.hunt_carry_per_worker_biomass,
        unequipped_carry(&cfg, "sled")
    );
    assert_eq!(
        bare.forage_carry_per_worker_biomass,
        unequipped_carry(&cfg, "baskets")
    );
    let kitted = snapshot
        .kits
        .iter()
        .find(|option| option.id == "big_game")
        .expect("the roster ships `big_game`");
    assert_eq!(kitted.attack, equipped_attack(&cfg));
    assert!(kitted.jobs.iter().any(|job| job == "hunt"));

    // **WHICH ITEMS EACH KIT CARRIES**, in config order. Without it a durability readout has to
    // infer the gear from the tiers, which is how a Trapping party came to be quoted the SPEARS'
    // condition.
    assert_eq!(
        kitted.item_ids,
        cfg.kit_definition("big_game")
            .expect("the roster ships `big_game`")
            .uses,
        "the kit publishes its `uses` list verbatim"
    );
    assert!(
        bare.item_ids.is_empty(),
        "`none` carries nothing, and says so with an empty list rather than an absent field"
    );

    // The pinned herd still reaches the wire — the herd row outlived the two tables that used to
    // hang off it, and it still publishes the kit its compose sheet opens on. On this fixture that
    // is the job default: `placid_world` holds the roster's wariness at `0`, which is exactly the
    // term the trap's `dispersion 0` acts on, so a warren ties with the spear and a tie keeps the
    // job default. That coincidence is asserted, not assumed.
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the pinned herd is on the wire");
    assert_eq!(
        herd.default_kit_id, expected_hunt_default,
        "with no retreat to avoid, the trap has nothing on the spear and the job default stands"
    );
}

/// **THE DEFECT: fresh traps and dry spears must not reprice the Trapping kit to the bare hand.**
///
/// The band wears its **spears** out and never touches its **traps**. Under `big_game` (which uses
/// spears) its attack must fall to the bare hand's; under `trapping` (which uses traps) it must stay
/// at the kitted tier. Those are two different answers for one band in one frame, and the pairing is
/// the whole test: asserting only the `trapping` row would pass on a sim that had stopped stepping
/// tiers down at all.
///
/// # Why the sim has to answer this instead of the client
///
/// Stepping a tier down needs the **axis → item** mapping, and it is per kit: `big_game` supplies
/// `attack` from `spears`, `trapping` supplies it from `traps`. `KitOption.itemIds` says what a kit
/// carries, not what each item is *for*, and no rule over that list recovers it — set-cover and
/// positional order both mis-assign, "any item live" keeps a kit at full tier with its weapon dry,
/// and "all items dry" keeps it at full tier with only the sled left. The client was guessing with a
/// hardcoded table, and this is the case the guess got wrong.
#[test]
fn a_bands_published_tiers_step_down_per_kit_by_which_item_that_kit_actually_uses() {
    let mut app = placid_world();
    let cfg = equipment(&app);

    // Wear the SPEARS to nothing and leave the traps untouched. `wear_item` charges per the item's
    // own quantum, so this is the durability cliff reached the way play reaches it.
    let band = app
        .world
        .query_filtered::<bevy::prelude::Entity, bevy::prelude::With<core_sim::ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the placid world spawns a resident band");
    {
        // **Uses, not condition points.** `wear_item` charges `uses × the item's own quantum`, so
        // the kills that empty the spears are `starting_durability / wear amount` — derived from
        // config rather than written as a number, or a retune of either dial silently stops this
        // fixture reaching the cliff it is about.
        let spears = cfg
            .item("spears")
            .expect("the shipped roster carries spears");
        let kills_to_expiry =
            spears.default_tier().starting_durability / spears.headline_wear().amount;
        let mut equipment_ledger = app
            .world
            .get_mut::<core_sim::BandEquipment>(band)
            .expect("a spawned band is kitted");
        // **Every unit** — a spawn stocks a party's worth, so one unit's kills leave the rest of
        // the stock standing and the kit would still be live.
        while equipment_ledger.count_of("spears") > 0 {
            equipment_ledger.wear_item(&cfg, "spears", WearQuantum::Strike, kills_to_expiry);
        }
    }
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let cohort = snapshot
        .populations
        .iter()
        .find(|p| p.entity == band.to_bits())
        .expect("the band is on the wire");

    let tiers = |kit_id: &str| {
        cohort
            .kit_tiers
            .iter()
            .find(|row| row.kit_id == kit_id)
            .unwrap_or_else(|| panic!("the band publishes tiers for `{kit_id}`"))
            .clone()
    };

    assert_eq!(
        cohort.kit_tiers.len(),
        cfg.kits().len(),
        "every offered kit gets a row — the picker's list and this list are the same list"
    );

    let bare = tiers("none").attack;
    let trapping = tiers("trapping").attack;
    let big_game = tiers("big_game").attack;

    assert!(
        trapping > bare,
        "the TRAPS are untouched, so the trapping kit keeps its attack ({trapping} vs bare {bare})          — this is the reported defect: guessing repriced it to the bare hand"
    );
    assert_eq!(
        big_game, bare,
        "…and the SPEARS are dry, so the big-game kit is on the bare hand's attack — the pairing          that stops this passing on a sim which never steps a tier down"
    );

    // The haul tier is supplied by the SLED, which BOTH kits use and neither wore, so it must be
    // untouched on both. A step-down keyed on "any item in the kit is dry" would drop it here.
    assert_eq!(
        tiers("big_game").hunt_carry_per_worker_biomass,
        tiers("trapping").hunt_carry_per_worker_biomass,
        "the sled is shared and unworn, so the haul tier cannot differ between the two hunt kits"
    );
    assert!(
        tiers("big_game").hunt_carry_per_worker_biomass
            > tiers("none").hunt_carry_per_worker_biomass,
        "…and it is still the SLEDDED rate, not the sledless one"
    );
}

/// The axes [`published_kit_tiers`] reads off one **encoded** `BandKitTiers` row.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PublishedKitRow {
    pen_carry_per_worker_biomass: f32,
    scout_vantage_range: f32,
    hunt_carry_per_worker_biomass: f32,
}

/// One band's whole `kitTiers` table, **off the encoded envelope**, keyed by kit id.
///
/// Encoded from the ring entry's *snapshot* rather than through `StoredSnapshot::encode_flat` for
/// the reason [`published_default_kit`] states: the claim here is about frame content, and
/// `encode_flat` is a read of stored bytes carrying a stored sequence number.
fn published_kit_tiers(
    app: &App,
    band: bevy::prelude::Entity,
) -> std::collections::BTreeMap<String, PublishedKitRow> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire");
    cohort
        .kitTiers()
        .expect("the band publishes a tier row per roster kit")
        .iter()
        .map(|row| {
            (
                row.kitId()
                    .expect("every published row names its kit")
                    .to_string(),
                PublishedKitRow {
                    pen_carry_per_worker_biomass: row.penCarryPerWorkerBiomass(),
                    scout_vantage_range: row.scoutVantageRange(),
                    hunt_carry_per_worker_biomass: row.huntCarryPerWorkerBiomass(),
                },
            )
        })
        .collect()
}

/// **Wear one item to its durability cliff**, in the item's own use quantum — **every unit of it**,
/// because a spawn stocks a party's worth and one unit's life leaves the rest of the stock standing.
///
/// `wear_item` charges `uses × the item's own wear amount`, so the count that empties **one unit** is
/// `starting_durability / wear.amount` — derived from config rather than written as a number, or a
/// retune of either dial silently stops a fixture reaching the cliff it is about. It serves the most
/// worn live batch, so charging that count once per unit owned empties the ledger.
fn wear_to_the_cliff(app: &mut App, band: bevy::prelude::Entity, item_id: &str) {
    let cfg = equipment(app);
    let item = cfg
        .item(item_id)
        .unwrap_or_else(|| panic!("the shipped roster carries '{item_id}'"));
    let uses_to_expiry = item.default_tier().starting_durability / item.headline_wear().amount;
    let mut ledger = app
        .world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band is kitted");
    while ledger.count_of(item_id) > 0 {
        ledger.wear_item(
            &cfg,
            item_id,
            cfg.item(item_id)
                .expect("the fixture names a roster item")
                .headline_wear()
                .per,
            uses_to_expiry,
        );
    }
}

/// **THE PEN AND THE VANTAGE STEP DOWN PER KIT TOO — the twin of the test above for the two axes
/// `BandKitTiers` did not carry.**
///
/// Those two rode the wire per band only at that band's **job default**
/// (`PopulationCohortState.penCarryPerWorkerBiomass` / `scoutVantageRange`), so a picker asking what
/// the kit *under the cursor* would grant had nothing to read and fell back to the ROSTER's **fresh**
/// tier. The two live readings that produced: a pen compose sheet quoting **40 per keeper** while the
/// sim collected **12** with the handling gear dry, and a Scout role card quoting **2 tiles** of sight
/// while `calculate_visibility` revealed at **1**. Both wrong in the reassuring direction.
///
/// The band wears its **handling gear** and its **wayfinding gear** out and touches nothing else, so
/// each axis is asserted three ways: the kit that supplies it steps down, a kit that supplies it not
/// at all is unmoved, and the **sled** both hunt kits share keeps its haul tier — which is what a
/// naive "any item in this kit is dry" rule would break.
///
/// **Every assertion is paired against the FRESH reading of the same row**, taken before the wear:
/// *"the pen rate is 12"* passes on a table that publishes 12 for everything, and *"it is unmoved"*
/// passes on a table that never moved at all.
///
/// Read off the **encoded envelope** — a field that never reached the codec still satisfies an
/// in-process assertion.
#[test]
fn a_bands_published_pen_and_vantage_tiers_step_down_per_kit_at_the_item_that_supplies_them() {
    let mut app = placid_world();
    let band = app
        .world
        .query_filtered::<bevy::prelude::Entity, bevy::prelude::With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the placid world spawns a resident band");
    recapture_snapshot_in_place(&mut app.world);
    let fresh = published_kit_tiers(&app, band);

    let row = |table: &std::collections::BTreeMap<String, PublishedKitRow>, kit_id: &str| {
        *table
            .get(kit_id)
            .unwrap_or_else(|| panic!("the band publishes tiers for `{kit_id}`"))
    };

    // LIVENESS, before anything is worn: the two axes genuinely vary by kit on this roster, or every
    // equality below would be the trivial truth about a table of one number.
    assert!(
        row(&fresh, HUSBANDRY_KIT).pen_carry_per_worker_biomass
            > row(&fresh, "big_game").pen_carry_per_worker_biomass,
        "only the handling gear supplies `pen_carry`, so a fresh husbandry kit must out-collect a \
         fresh stalking kit at the pen"
    );
    assert!(
        row(&fresh, "wayfinding").scout_vantage_range > row(&fresh, "none").scout_vantage_range,
        "only the wayfinding gear supplies the vantage's reach, so a fresh scout kit must see \
         further than no kit at all"
    );

    wear_to_the_cliff(&mut app, band, "husbandry_gear");
    wear_to_the_cliff(&mut app, band, "wayfinding");
    recapture_snapshot_in_place(&mut app.world);
    let worn = published_kit_tiers(&app, band);

    // --- the PEN -------------------------------------------------------------------------------
    assert!(
        row(&worn, HUSBANDRY_KIT).pen_carry_per_worker_biomass
            < row(&fresh, HUSBANDRY_KIT).pen_carry_per_worker_biomass,
        "the handling gear is dry, so the husbandry kit's published pen rate must fall — this is \
         the readout that quoted 40 per keeper against a sim collecting 12"
    );
    assert_eq!(
        row(&worn, HUSBANDRY_KIT).pen_carry_per_worker_biomass,
        row(&fresh, "big_game").pen_carry_per_worker_biomass,
        "…all the way to the bare rate, which is what a kit with no handling gear reads at every \
         state of wear"
    );
    assert_eq!(
        row(&worn, "big_game").pen_carry_per_worker_biomass,
        row(&fresh, "big_game").pen_carry_per_worker_biomass,
        "a kit that carries no handling gear is UNMOVED by wearing it out — the pairing that stops \
         this passing on a sim which steps every kit down together"
    );
    assert_eq!(
        row(&worn, HUSBANDRY_KIT).hunt_carry_per_worker_biomass,
        row(&fresh, HUSBANDRY_KIT).hunt_carry_per_worker_biomass,
        "the SLED beside the handling gear is untouched, so the husbandry kit keeps its haul tier — \
         a step-down keyed on `any item in this kit is dry` would drop it"
    );

    // --- the VANTAGE ---------------------------------------------------------------------------
    assert!(
        row(&worn, "wayfinding").scout_vantage_range
            < row(&fresh, "wayfinding").scout_vantage_range,
        "the wayfinding gear is dry, so the scout kit's published reach must fall — this is the \
         readout that quoted 2 tiles of sight against a reveal at 1"
    );
    assert_eq!(
        row(&worn, "wayfinding").scout_vantage_range,
        row(&fresh, "none").scout_vantage_range,
        "…all the way to the unaided reach, which is what a kit with no wayfinding gear reads"
    );
    assert_eq!(
        row(&worn, "big_game").scout_vantage_range,
        row(&fresh, "big_game").scout_vantage_range,
        "a kit that carries no wayfinding gear is UNMOVED by wearing it out"
    );
}

// ---------------------------------------------------------------------------------------------
// THE QUARRY'S OWN DEFAULT KIT
// ---------------------------------------------------------------------------------------------

/// **A warren wants the trap and a deer wants the spear, and the wire says so.**
///
/// `default_kits.hunt` is one id for the whole job, so a Rabbit Warren's compose sheet used to open
/// on the Stalking kit — which works, and is ~4× worse than Trapping, because a rabbit's
/// `wariness 0.75` loses a spear party three animals in four while the trap's `dispersion 0` keeps
/// all of them. The default is now **derived**: every hunt kit is scored against the species with
/// §4.6's own per-hunter-turn take.
///
/// Asserted off the **encoded envelope**, not the in-process struct: a field that never reached the
/// codec still passes an in-process assertion.
///
/// **The liveness half is the `assert_ne!`.** "The rabbit reads `trapping`" is satisfiable by a
/// scorer that has stopped scoring and answers a constant, so the two rows are also asserted to
/// *differ* — which is the whole claim, since one job default cannot differ from itself.
#[test]
fn a_warren_defaults_to_the_trap_and_a_deer_to_the_spear_on_the_wire() {
    let mut app = wary_world();
    let warren = pin_herd_of(&mut app, HARMLESS_QUARRY, "warren_fixture_herd");
    let deer = pin_herd_of(&mut app, DEFENDED_QUARRY, "deer_fixture_herd");
    recapture_snapshot_in_place(&mut app.world);

    let job_default = equipment(&app).default_kit_id(KitJob::Hunt).to_string();
    let warren_kit = published_default_kit(&app, &warren);
    let deer_kit = published_default_kit(&app, &deer);

    assert_eq!(
        warren_kit, TRAPPING_KIT,
        "a rabbit is small enough for the snare to hold and wary enough for the spear to scatter"
    );
    assert_eq!(
        deer_kit, job_default,
        "a Red Deer is past the trap's `max_body_mass`, so the snare scores zero and the job \
         default stands"
    );
    assert_ne!(
        warren_kit, deer_kit,
        "LIVENESS: a scorer that answered a constant would pass every equality above"
    );
}

/// **A CORRALLED herd wants the handling gear, and a wild one of the same species does not.**
///
/// The score is a function of the *species*, so it answers the same kit for a warren on the range
/// and a warren in a pen — and a pen has no fight stage for it to score, so it never could answer
/// otherwise. A corralled Rabbit Warren therefore published `trapping`, a kit whose contribution at
/// a pen is nil: a pen is collected on `EquipmentStat::PenCarry`, which only the husbandry kit
/// supplies. It is a **source-axis** question, the same one the picker's greying and
/// `KitRoster.priced_source` answer, and `fauna::herd_default_hunt_kit` answers it by asking the
/// roster which hunt kit supplies the stat rather than by naming one.
///
/// **The two herds are compared to EACH OTHER**, which is the liveness half: one species cannot
/// differ from itself, so a resolver that had stopped reading the herd — or that answered a
/// constant — fails the `assert_ne!` however plausible each equality looks alone.
///
/// Read off the **encoded envelope** through [`published_default_kit`], which also asserts both
/// estimate tables name that same kit — so the pen row's `huntTripEstimatesKitId` /
/// `denialEstimatesKitId` are pinned to the herd's own default here too.
#[test]
fn a_corralled_herd_defaults_to_the_pen_kit_and_a_wild_one_of_the_same_species_does_not() {
    let mut app = wary_world();
    let ranging = pin_herd_of(&mut app, HARMLESS_QUARRY, "ranging_warren_herd");
    let penned = pin_herd_of(&mut app, HARMLESS_QUARRY, "penned_warren_herd");
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == penned)
            .expect("the pen fixture herd is in the registry");
        let anchor = herd.current_pos;
        assert!(
            herd.corral_at(anchor),
            "the warren's husbandry ceiling allows a pen — `corral_at` refusing means the fixture \
             re-badged a species that cannot be penned"
        );
    }
    // The capture reads the DISPLAY list, so the pen has to be republished or the wire still
    // describes the herd as it stood before it was fenced.
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<HerdTelemetry>().entries = entries;
    recapture_snapshot_in_place(&mut app.world);

    let ranging_kit = published_default_kit(&app, &ranging);
    let penned_kit = published_default_kit(&app, &penned);

    assert_eq!(
        penned_kit, HUSBANDRY_KIT,
        "a pen is collected on `PenCarry`, and the handling gear is the only kit that supplies it"
    );
    assert_eq!(
        ranging_kit, TRAPPING_KIT,
        "the same animal on the range is still a scoring question, and the snare still wins it"
    );
    assert_ne!(
        penned_kit, ranging_kit,
        "LIVENESS: one species cannot differ from itself, so a resolver blind to the pen — or one \
         answering a constant — fails here even though each equality above looks plausible alone"
    );
}

/// **The margin lever is live, and a narrow win keeps the job default.**
///
/// Without a margin the published default flips on a trivial retune and the player watches their
/// compose sheet move for reasons they cannot see. The lever is measured against the roster's own
/// **narrowest genuine win** rather than a literal ratio, so a retune moves the test's threshold
/// with the game instead of failing it.
#[test]
fn a_narrow_win_keeps_the_job_default_until_the_margin_lets_it_through() {
    let fauna = core_sim::FaunaConfig::builtin();
    let person = core_sim::CreaturesConfig::builtin().person();
    let shipped = EquipmentConfig::builtin();
    let job_default = shipped.default_kit(KitJob::Hunt);
    let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());

    // The narrowest win on the shipped roster: the species where some hunt kit beats the job
    // default by the *smallest* positive factor. Found rather than named, so this test cannot go
    // stale against a roster edit.
    let score = |cfg: &EquipmentConfig, kit: &KitChoice, species: &core_sim::SpeciesDef| {
        core_sim::per_hunter_take_biomass(
            cfg.hunter_profile_against(person, kit, &fresh, species.body_mass),
            cfg.dispersion(kit, &fresh),
            species,
        )
    };
    let (narrowest, ratio) = fauna
        .species
        .values()
        .filter_map(|species| {
            let baseline = score(&shipped, &job_default, species);
            if baseline <= 0.0 {
                return None;
            }
            let best = shipped
                .kits_for_job(KitJob::Hunt)
                .map(|kit| score(&shipped, &kit, species))
                .fold(0.0f32, f32::max);
            (best > baseline).then(|| (species.clone(), best / baseline))
        })
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).expect("scores are finite"))
        .expect("some quarry on the shipped roster wants a kit other than the job default");
    assert!(
        ratio > 1.0,
        "LIVENESS: the narrowest win is a real win, not a tie"
    );

    let with_margin = |margin: f32| {
        let mut cfg = (*shipped).clone();
        cfg.quarry_default_kit_margin = margin;
        core_sim::quarry_default_hunt_kit(&cfg, person, &narrowest)
            .id()
            .to_string()
    };
    // A margin the win clears, and one it does not. `- 1.0` because the lever is a fraction of the
    // default's own score, not a multiple of it.
    let clears = (ratio - 1.0) * 0.5;
    let blocks = (ratio - 1.0) * 2.0;
    assert_ne!(
        with_margin(clears),
        job_default.id(),
        "below the win, the better kit takes the slot"
    );
    assert_eq!(
        with_margin(blocks),
        job_default.id(),
        "above it, a near-tie keeps the job default rather than flipping the published answer"
    );
}

/// **Wear does not enter the score — a band worn to dry sees the same default.**
///
/// The default is a property of *quarry × roster*, a per-world constant per herd, so it cannot
/// reshuffle under the player as their spears run out. Scoring at the live tier would do exactly
/// that: a dry `big_game` party falls to the bare hand's `attack 1`, which on a `defense 0` warren
/// is a 20× cut — enough to flip any margin.
#[test]
fn a_herds_default_kit_does_not_move_when_the_band_wears_its_kit_to_dry() {
    let mut app = wary_world();
    let warren = pin_herd_of(&mut app, HARMLESS_QUARRY, "warren_fixture_herd");
    recapture_snapshot_in_place(&mut app.world);
    let fresh = published_default_kit(&app, &warren);

    // Run EVERY item in the table dry on the band that is working the herd, so no kit can be
    // preferred merely because the band happens to have kept one item.
    let tile = tile_at(
        &app,
        app.world
            .resource::<HerdRegistry>()
            .find(&warren)
            .expect("pinned")
            .position(),
    );
    let band = app
        .world
        .spawn((
            cohort(tile, CREW),
            ResidentBand,
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
            LaborAllocation::default(),
        ))
        .id();
    {
        let cfg = equipment(&app);
        let items: Vec<String> = cfg.items().map(|(id, _)| id.to_string()).collect();
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(band)
            .expect("the band carries a wear ledger");
        for id in items {
            run_dry(&mut wear, &cfg, &id);
        }
    }
    recapture_snapshot_in_place(&mut app.world);

    assert_eq!(
        published_default_kit(&app, &warren),
        fresh,
        "the published default is scored at the FRESH tier, so a dry band does not move it"
    );
    assert_eq!(
        fresh, TRAPPING_KIT,
        "LIVENESS: the unchanged value is the real answer, not a default that never resolved"
    );
}

/// **The designer surface's catalogue is the config the sim runs — it ROUND-TRIPS.**
///
/// `SubsistenceSection.equipmentConfigJson` is this schema's one deliberate blob: the Workbench
/// prints the TOE configuration key by key, so a dial added to `equipment.json` appears with no
/// client edit and no schema edit. That only holds if what is published is *the config*, not a
/// lossy projection of it — and the way it silently stops holding is a field that **serializes
/// under a different name than it deserializes**, which no compiler and no equality check on the
/// live struct would notice.
///
/// So the assertion is on the **published string**, taken off the encoded envelope rather than the
/// in-process snapshot, fed back through `EquipmentConfig::from_json_str` — which validates — and
/// compared against the config the sim is running. A renamed field fails at the parse (`missing
/// field`); a re-pointed one fails on the values below.
#[test]
fn the_published_equipment_config_json_round_trips_to_the_config_the_sim_runs() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = placid_world();
    recapture_snapshot_in_place(&mut app.world);

    let bytes = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .encode_flat();
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let published = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.equipmentConfigJson())
        .expect("the subsistence section carries the designer catalogue");
    assert!(
        !published.is_empty(),
        "an empty string is the serialization-failed reading — the catalogue never reached the wire"
    );

    // `from_json_str` validates, so a parse that succeeds is also a config the sim would boot on.
    let parsed = EquipmentConfig::from_json_str(published)
        .expect("the published catalogue parses back as an EquipmentConfig, and validates");
    let live = equipment(&app);

    // The ITEM TABLE, compared WHOLE rather than field by field. The three named blocks this
    // replaced (`hunting_kit` / `sled_kit` / `basket_kit`) could be listed by hand because their
    // fields were fixed; an item's are not — `effects` is a list, an effect may carry mass bounds,
    // and the next item adds an axis. A hand-listed comparison would go stale silently, which is
    // the exact failure this whole field exists to prevent.
    assert!(
        !live.items.is_empty(),
        "the item table is non-empty, or comparing it says nothing"
    );
    assert_eq!(
        parsed.items, live.items,
        "every item's durability, wear quantum and effects survive the round trip"
    );

    // The roster, in file order — and each entry compared through the MASK it resolves to, so the
    // `uses` list is asserted as the thing it means rather than as a list of names.
    assert_eq!(
        parsed.kits().len(),
        live.kits().len(),
        "every roster entry survives the round trip"
    );
    for (round_tripped, shipped) in parsed.kits().iter().zip(live.kits()) {
        assert_eq!(round_tripped.id, shipped.id);
        assert_eq!(round_tripped.display_name, shipped.display_name);
        assert_eq!(round_tripped.jobs, shipped.jobs);
        assert_eq!(
            parsed.kit(&shipped.id),
            live.kit(&shipped.id),
            "kit `{}` resolves to the same component mask on both sides",
            shipped.id
        );
    }
    assert_eq!(
        parsed.default_kit_id(KitJob::Hunt),
        live.default_kit_id(KitJob::Hunt)
    );
    assert_eq!(
        parsed.default_kit_id(KitJob::Forage),
        live.default_kit_id(KitJob::Forage)
    );

    // The file's `_comment*` keys are not struct fields, so serializing the STRUCT is what keeps
    // them off the wire — the designer page prints dials, not prose.
    assert!(
        !published.contains("_comment"),
        "the catalogue is the struct the sim runs, not the text of `equipment.json`"
    );
}

/// **Every labor row publishes the kit it is priced at, resolved** — the sim never ships
/// "unspecified". A crew that named nothing reads the job's default; a band-wide role, which has no
/// kit axis at all, reads `""` rather than a kit it is not using.
#[test]
fn every_labor_row_publishes_the_kit_it_is_priced_at() {
    let mut app = placid_world();
    let (id, pos) = pin_herd(&mut app);
    let tile = tile_at(&app, pos);
    let band = app
        .world
        .spawn((
            cohort(tile, CREW * 2),
            ResidentBand,
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
            LaborAllocation {
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Hunt {
                            fauna_id: id.clone(),
                            floor: DEFAULT_ESCAPEMENT_FLOOR,
                        },
                        workers: CREW,
                        improvement: None,
                        // Named nothing — the wire must still say which kit it is working under.
                        kit: None,
                    },
                    LaborAssignment {
                        target: LaborTarget::Scout,
                        workers: CREW,
                        improvement: None,
                        kit: None,
                    },
                ],
                ..Default::default()
            },
        ))
        .id();
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let cohort_state = snapshot
        .populations
        .iter()
        .find(|state| state.entity == band.to_bits())
        .expect("the fixture band is on the wire");

    let hunt_row = cohort_state
        .labor_assignments
        .iter()
        .find(|row| row.kind == "hunt")
        .expect("the hunt row is published");
    assert_eq!(
        hunt_row.kit_id,
        equipment(&app).default_kit_id(KitJob::Hunt),
        "an unnamed crew publishes the job's default, not an empty string"
    );

    let scout_row = cohort_state
        .labor_assignments
        .iter()
        .find(|row| row.kind == "scout")
        .expect("the scout row is published");
    assert_eq!(
        scout_row.kit_id,
        equipment(&app).default_kit_id(KitJob::Scout),
        "a band-wide role publishes its own job's default now — it used to publish `\"\"`, because \
         Scout and Warrior had no kit axis at all until the roster gained gear for them"
    );
}

/// **`PopulationCohortState.kitId` answers for the two HUNT tiers, and a client must not read the
/// forage tier against it.**
///
/// A resident band resolves `hunt_carry_per_worker_biomass`/`hunter_attack` through the **hunt**
/// job's default and `forage_carry_per_worker_biomass` through the **forage** job's — two different
/// kits — while the row publishes only the first. The `.fbs` used to describe the field as *"which
/// kit the three tiers above are quoted at"*, which is true of two of the three: a client pairing
/// `forageCarryPerWorkerBiomass` with `kits[kitId]` reads its gathering rate off `big_game`, a kit
/// that carries no basket component at all.
///
/// This test pins the divergence as a **fact of the wire** rather than the narrowed wording as a
/// promise — the numbers are what a client would actually mis-pair — and pairs it with the
/// in-flight-party case, where a party's single kit makes the two genuinely coincide.
#[test]
fn a_resident_bands_published_kit_answers_for_the_hunt_tiers_only() {
    let mut app = placid_world();
    let (_id, pos) = pin_herd(&mut app);
    let tile = tile_at(&app, pos);
    let band = app
        .world
        .spawn((
            cohort(tile, CREW),
            ResidentBand,
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
        ))
        .id();
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let state = snapshot
        .populations
        .iter()
        .find(|state| state.entity == band.to_bits())
        .expect("the fixture band is on the wire");

    let cfg = equipment(&app);
    assert_eq!(
        state.kit_id,
        cfg.default_kit_id(KitJob::Hunt),
        "a resident band's row is quoted at the HUNT job's default"
    );
    // The trap, stated in numbers: the roster entry the field names does not carry this band's
    // gathering rate, so looking one up against the other reads the wrong tier.
    let named = snapshot
        .kits
        .iter()
        .find(|option| option.id == state.kit_id)
        .expect("the published kit id names a real roster entry");
    assert_ne!(
        named.forage_carry_per_worker_biomass, state.forage_carry_per_worker_biomass,
        "if these agreed the field's narrowed scope would be untestable — and the whole reason the \
         `.fbs` now says the forage tier is NOT quoted at this id is that they do not"
    );
    assert_eq!(
        state.forage_carry_per_worker_biomass,
        snapshot
            .kits
            .iter()
            .find(|option| option.id == cfg.default_kit_id(KitJob::Forage))
            .expect("the forage default is on the roster")
            .forage_carry_per_worker_biomass,
        "…and the tier it IS quoted at is the FORAGE job's default, which rides the wire as \
         `default_forage_kit_id`"
    );
}

/// **Each of the three appended tiers answers for its OWN job's default** — the direct twin of the
/// test above, for `penCarryPerWorkerBiomass` / `scoutVantageRange` / `warriorAttack`.
///
/// The roster gave husbandry gear, wayfinding gear and clubs a kit each, so a resident band now
/// resolves **four** different kits across one cohort row: the hunt default for the two hunt tiers
/// *and the pen* (a pen is worked from a Hunt row), the forage default for the gather tier, and the
/// scout and warrior defaults for the two band-wide roles. Only the first rides the wire, as
/// `kitId`.
///
/// **It fails if someone resolves all of them through `kitId`.** The scout and warrior asserts are
/// each paired with an `assert_ne!` against the kit `kitId` actually names — a wayfinding vantage
/// against `big_game`'s bare 1 tile, a club's `attack 6` against the stalking kit's spears at 20 —
/// so the numbers a client would mis-pair are what the test compares, not the wording.
#[test]
fn a_resident_bands_appended_tiers_each_answer_for_their_own_jobs_default() {
    let mut app = placid_world();
    let (_id, pos) = pin_herd(&mut app);
    let tile = tile_at(&app, pos);
    let band = app
        .world
        .spawn((
            cohort(tile, CREW),
            ResidentBand,
            BandEquipment::start_stocked(&EquipmentConfig::builtin()),
        ))
        .id();
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let state = snapshot
        .populations
        .iter()
        .find(|state| state.entity == band.to_bits())
        .expect("the fixture band is on the wire");

    let cfg = equipment(&app);
    let roster = |id: &str| {
        snapshot
            .kits
            .iter()
            .find(|option| option.id == id)
            .unwrap_or_else(|| panic!("'{id}' is on the published roster"))
    };
    let named = roster(&state.kit_id);

    // --- the pen: quoted at the HUNT default, which IS what `kitId` names ---------------------
    assert_eq!(
        state.pen_carry_per_worker_biomass,
        roster(cfg.default_kit_id(KitJob::Hunt)).pen_carry_per_worker_biomass,
        "a pen is worked from a Hunt row, so its tier is the hunt job's default — the one job \
         `kitId` does answer for"
    );
    // …and it is a real resolution rather than the equipped rate handed back unchanged: some kit on
    // the roster grants a *different* pen tier, so the hunt default's is genuinely a resolved one.
    assert!(
        snapshot
            .kits
            .iter()
            .any(|option| option.pen_carry_per_worker_biomass
                != state.pen_carry_per_worker_biomass),
        "if every kit granted the same pen tier this assertion could not tell a resolution from a \
         constant"
    );

    // --- the vantage: quoted at the SCOUT default, NOT at `kitId` -------------------------------
    assert_eq!(
        state.scout_vantage_range,
        roster(cfg.default_kit_id(KitJob::Scout)).scout_vantage_range,
        "the vantage tier is the SCOUT job's default, which rides the wire as `default_scout_kit_id`"
    );
    assert_ne!(
        state.scout_vantage_range, named.scout_vantage_range,
        "…and reading it against `kitId` would quote the hunt kit's bare vantage instead — which is \
         exactly the mis-pairing the `.fbs` note on `kitId` exists to prevent"
    );

    // --- the warrior: quoted at the WARRIOR default, NOT at `kitId` -----------------------------
    assert_eq!(
        state.warrior_attack,
        roster(cfg.default_kit_id(KitJob::Warrior)).attack,
        "a warrior fights at the WARRIOR job's default, which rides the wire as \
         `default_warrior_kit_id`"
    );
    assert_ne!(
        state.warrior_attack, named.attack,
        "…and reading it against `kitId` would arm the band's defenders with the hunt kit's spears"
    );
    assert_ne!(
        state.warrior_attack, state.hunter_attack,
        "`attack` is one stat resolved through two different kits, so the two rows are two numbers \
         on the same band — a readout must not render one as the other"
    );
}

/// **An in-flight party carries ONE kit, so every tier on its row is quoted at it.** The resident
/// band's four-way split above is a property of a *band* holding one kit per assignment; a detached
/// party decided its kit at launch and fights, hauls, keeps and scouts at that one tier — which is
/// what makes `kitId` a complete answer for a party and a partial one for a band.
#[test]
fn an_in_flight_partys_appended_tiers_are_all_quoted_at_the_kit_it_was_sent_with() {
    let mut app = placid_world();
    let (fauna_id, pos) = pin_herd(&mut app);
    let home = spawn_home_band(&mut app, pos);
    // **The husbandry kit, chosen because all three of its appended tiers differ from the job
    // default each would otherwise resolve to** — pen 40 against the hunt default's bare 12,
    // vantage 1 against the scout default's 2, and no weapon at all (`attack` 1) against the
    // warrior default's clubs at 6. So a resolution that reached for a job default instead of the
    // party's own kit fails all three asserts, not one.
    let sent_with = kit(&app, HUSBANDRY_KIT);
    let party = spawn_party(&mut app, home, pos, &fauna_id, sent_with.clone());
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let state = snapshot
        .populations
        .iter()
        .find(|state| state.entity == party.to_bits())
        .expect("the party is on the wire");
    let named = snapshot
        .kits
        .iter()
        .find(|option| option.id == state.kit_id)
        .expect("the published kit id names a real roster entry");

    assert_eq!(
        state.kit_id,
        sent_with.id(),
        "a party's row names the kit it was SENT OUT WITH, not any job's default"
    );
    assert_eq!(
        state.pen_carry_per_worker_biomass,
        named.pen_carry_per_worker_biomass
    );
    assert_eq!(state.scout_vantage_range, named.scout_vantage_range);
    assert_eq!(state.warrior_attack, named.attack);
    // Liveness: the three above are the party's kit's numbers, and they are NOT the job defaults'.
    let cfg = equipment(&app);
    let default_of = |job| {
        let id = cfg.default_kit_id(job).to_string();
        snapshot
            .kits
            .iter()
            .find(|option| option.id == id)
            .unwrap_or_else(|| panic!("the {job:?} default is on the roster"))
    };
    assert_ne!(
        state.pen_carry_per_worker_biomass,
        default_of(KitJob::Hunt).pen_carry_per_worker_biomass
    );
    assert_ne!(
        state.scout_vantage_range,
        default_of(KitJob::Scout).scout_vantage_range
    );
    assert_ne!(state.warrior_attack, default_of(KitJob::Warrior).attack);
}

/// **The shipped durability of one item**, by id — the tests read dials off the item table now that
/// `equipment.json` has one, rather than off three named blocks.
fn item_durability(cfg: &EquipmentConfig, id: &str) -> f32 {
    cfg.item(id)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{id}'"))
        .default_tier()
        .starting_durability
}

/// **Run an item out, by USING it** — the only way the sim reduces condition, so a fixture that
/// wants a dry item spends it rather than writing a number into the ledger. Charges the item's own
/// quantum a batch at a time until nothing of it is left.
fn run_dry(ledger: &mut BandEquipment, cfg: &EquipmentConfig, id: &str) {
    let uses = item_durability(cfg, id)
        / cfg
            .item(id)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{id}'"))
            .headline_wear()
            .amount;
    while ledger.remaining(id, cfg) > 0.0 {
        ledger.wear_item(
            cfg,
            id,
            cfg.item(id)
                .expect("the fixture names a roster item")
                .headline_wear()
                .per,
            uses,
        );
    }
}

/// The **unequipped** carry rate a carry item declares — the tier a party without it falls back to.
fn unequipped_carry(cfg: &EquipmentConfig, id: &str) -> f32 {
    let stat = if id == "sled" {
        EquipmentStat::HuntCarry
    } else {
        EquipmentStat::ForageCarry
    };
    // The bare-handed side of a carry lives in `labor_config.json` since quality tiers landed —
    // the item's own tier declares the *equipped* value, which is what a fixture must not read here.
    let _ = (cfg, stat);
    let labor = core_sim::LaborConfig::builtin();
    if id == "sled" {
        labor.hunt.per_worker_biomass_capacity
    } else {
        labor.forage.per_worker_biomass_capacity
    }
}

/// The **equipped** `attack` a speared party fights at.
fn equipped_attack(cfg: &EquipmentConfig) -> f32 {
    match cfg.item("spears").and_then(|item| {
        item.default_tier()
            .effects
            .iter()
            .find(|effect| effect.stat == EquipmentStat::Attack)
            .map(|effect| effect.tier)
    }) {
        Some(EffectTier::Equipped(value)) => value,
        other => panic!("spears must declare an equipped attack, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// #520 on the wire — the composed party has to be EXPRESSIBLE
// ---------------------------------------------------------------------------------------------

/// **A resident band staffing GATHERERS, holding only `baskets_owned` baskets.**
///
/// The forage job is the one that had no head count on the wire, so this is the fixture the
/// denominator is measured on. It never runs a turn — the published pair is resolved from the
/// allocation and the ledger, so the patch only has to exist for the assignment to name.
fn spawn_gathering_band(app: &mut App, baskets_owned: u32) -> (bevy::prelude::Entity, UVec2) {
    let tile_pos = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .min_by_key(|p| (p.y, p.x))
        .expect("the campaign map seeds forage patches");
    let tile_entity = tile_at(app, tile_pos);
    let mut wear = BandEquipment::start_stocked_for(&EquipmentConfig::builtin(), CREW as f32);
    let tier = EquipmentConfig::builtin()
        .item("baskets")
        .expect("the roster ships baskets")
        .default_tier()
        .id
        .clone();
    wear.restore_batches("baskets", Vec::new());
    wear.stock("baskets", baskets_owned, &tier, None);
    let band = app
        .world
        .spawn((
            cohort(tile_entity, CREW),
            ResidentBand,
            wear,
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: tile_pos,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                    },
                    workers: CREW,
                    improvement: None,
                    kit: None,
                }],
                ..Default::default()
            },
        ))
        .id();
    (band, tile_pos)
}

/// One `BandKitCrew` row, read off the **encoded** envelope.
#[derive(Debug, Clone, PartialEq)]
struct PublishedCrew {
    workers: f32,
    hunter_attack: f32,
    item_ids: Vec<String>,
}

/// The three things this suite reads off one band's **encoded** cohort row — the crews, the flat
/// `hunterAttack` beside them, and one item's `workersHolding`.
///
/// Encoded from the ring entry's *snapshot* rather than through `StoredSnapshot::encode_flat`, for
/// the reason [`published_kit_tiers`] states.
fn published_hunt_composition(
    app: &App,
    band: bevy::prelude::Entity,
    item: &str,
) -> (Vec<PublishedCrew>, f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire");
    let crews = cohort
        .huntCrews()
        .expect("a band always publishes at least one hunt crew")
        .iter()
        .map(|row| PublishedCrew {
            workers: row.workers(),
            hunter_attack: row.hunterAttack(),
            item_ids: row
                .itemIds()
                .map(|ids| ids.iter().map(str::to_string).collect())
                .unwrap_or_default(),
        })
        .collect();
    let workers_holding = published_gear_pair(app, band, item).0;
    (crews, cohort.hunterAttack(), workers_holding)
}

/// **One item's `(workersHolding, workersOnQuotedJob)` pair**, off the **encoded** envelope — read
/// together, because the two fields are one sentence and a test that read them apart could not
/// catch them describing two different jobs.
fn published_gear_pair(app: &App, band: bevy::prelude::Entity, item: &str) -> (f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire")
        .kitItemConditions()
        .expect("the band publishes a condition row per config item")
        .iter()
        .find(|row| row.itemId() == Some(item))
        .map(|row| (row.workersHolding(), row.workersOnQuotedJob()))
        .unwrap_or_else(|| panic!("the config carries '{item}', so it has a row"))
}

/// **A BAND SHORT OF SPEARS PUBLISHES ITS DIVISION** (issue #520) — two crews whose workers sum to
/// the hunt head count, the armed one at the equipped tier and the bare one at the `person` row's
/// intrinsic `1`.
///
/// The hunt gate `max(0, hunterAttack − defense)` is why one number per band is not enough: it
/// decides whether a species can be taken **at all**, and a mixed band's honest answer is *"these
/// can, those cannot"*. Read off the **encoded envelope** — a field that never reached the codec
/// still satisfies an in-process assertion.
#[test]
fn a_band_short_of_spears_publishes_one_hunt_crew_per_run() {
    /// Two of the four hunters hold a spear, so the two rows are the same size and neither can be
    /// mistaken for the whole party.
    const SPEARS_OWNED: u32 = 2;

    let mut app = placid_world();
    let (herd, pos) = pin_herd(&mut app);
    let band = spawn_hunting_band(&mut app, pos, &herd, None);
    {
        let cfg = equipment(&app);
        let tier = cfg
            .item("spears")
            .expect("the roster ships spears")
            .default_tier()
            .id
            .clone();
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(band)
            .expect("the fixture spawned a ledger");
        wear.restore_batches("spears", Vec::new());
        wear.stock("spears", SPEARS_OWNED, &tier, None);
    }
    recapture_snapshot_in_place(&mut app.world);

    let (crews, hunter_attack, spears_holding) = published_hunt_composition(&app, band, "spears");
    let cfg = equipment(&app);
    let intrinsic = core_sim::CreaturesConfig::builtin().person().attack;

    assert_eq!(
        crews.len(),
        2,
        "two loadouts, so two published rows: {crews:?}"
    );
    assert_eq!(
        crews.iter().map(|crew| crew.workers).sum::<f32>(),
        CREW as f32,
        "the rows must account for every hunter on the job, or a client cannot render a share"
    );
    assert_eq!(crews[0].workers, SPEARS_OWNED as f32);
    assert_eq!(
        crews[0].hunter_attack,
        equipped_attack(&cfg),
        "the best-equipped row comes first and carries the spear's tier"
    );
    assert!(
        crews[0].item_ids.iter().any(|id| id == "spears"),
        "…and says what it is holding: {crews:?}"
    );
    assert_eq!(crews[1].workers, (CREW - SPEARS_OWNED) as f32);
    assert_eq!(
        crews[1].hunter_attack, intrinsic,
        "the rest are on the `person` roster row's own attack — the gate's losing side"
    );
    assert!(
        !crews[1].item_ids.iter().any(|id| id == "spears"),
        "…and hold no spear: {crews:?}"
    );
    assert_eq!(
        hunter_attack, crews[0].hunter_attack,
        "`hunterAttack` keeps its meaning: the BEST-equipped crew's tier, which is exactly why a \
         client must read `huntCrews` for the rest of the party"
    );
    assert_eq!(
        spears_holding, SPEARS_OWNED as f32,
        "`workersHolding` counts the PEOPLE the spears reach, so a gear row reads '2 of 4' without \
         dividing anything"
    );
}

/// **A GEAR ROW CARRIES ITS OWN DENOMINATOR, on the jobs that are not the hunt** (issue #520).
///
/// `workersHolding` alone is only renderable where the wire also carries a head count, and only the
/// hunt does (`Σ huntCrews.workers`) — so a spears shortfall could be stated and a **basket's** could
/// not, which is the quiet half of the same reassuring-direction failure. `workersOnQuotedJob` is
/// that denominator, off the same coverage the numerator came from.
///
/// The band below staffs **gatherers** and holds fewer baskets than it has of them, so the pair is a
/// genuine shortfall on a job the hunt's head count says nothing about.
#[test]
fn a_gear_row_publishes_the_head_count_of_the_job_it_is_quoted_at() {
    /// Fewer than the crew, so the pair is a real *"2 of 4"* rather than a full set.
    const BASKETS_OWNED: u32 = 2;

    let mut app = placid_world();
    let (band, _patch) = spawn_gathering_band(&mut app, BASKETS_OWNED);
    recapture_snapshot_in_place(&mut app.world);

    let (holding, on_job) = published_gear_pair(&app, band, "baskets");
    assert_eq!(
        on_job, CREW as f32,
        "the denominator is the FORAGE job's head count — the hunt's says nothing about baskets"
    );
    assert_eq!(
        holding, BASKETS_OWNED as f32,
        "and the numerator is the gatherers the baskets reach, so the row reads '2 of 4'"
    );
    assert!(
        holding < on_job,
        "liveness: this band really is short, or the pair is the trivial truth about a full set"
    );

    // The HUNT row is unaffected and still answers at its own job — nobody is staffed on it here,
    // which is the other zero the schema separates.
    let (spears_holding, spears_on_job) = published_gear_pair(&app, band, "spears");
    assert_eq!(
        (spears_holding, spears_on_job),
        (0.0, 0.0),
        "nobody hunts in this band, so the spear row is `0 of 0` — not a shortfall"
    );
}

/// **A JOB NOBODY IS STAFFED ON PUBLISHES A ZERO DENOMINATOR, and the row is still there.**
///
/// `0 of 0` is *"nothing was needed"*; `0 of 4` is *"four people went without"*. Both are `0`
/// numerators, and a client that could not tell them apart would render a warning on every band that
/// simply is not gathering. Asserted against a real shortfall in the same world, so *"the row exists
/// and reads zero"* cannot pass on a sim that publishes zeros everywhere.
#[test]
fn a_job_nobody_is_staffed_on_publishes_a_zero_denominator_rather_than_an_absent_row() {
    let mut app = placid_world();
    let (herd, pos) = pin_herd(&mut app);
    let band = spawn_hunting_band(&mut app, pos, &herd, None);
    {
        // Strip the CLUBS so the warrior row would be a shortfall if anybody were on it.
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(band)
            .expect("the fixture spawned a ledger");
        wear.restore_batches("clubs", Vec::new());
    }
    recapture_snapshot_in_place(&mut app.world);

    // Nobody on Warrior: `0 of 0`, and the row is present.
    assert_eq!(
        published_gear_pair(&app, band, "clubs"),
        (0.0, 0.0),
        "a band with no warriors needed no clubs — `0 of 0`, and the row must still be published"
    );
    // Nobody on Forage either, though the band owns baskets: the denominator, not the stock, is
    // what makes this zero — which is exactly the distinction the pair exists to draw.
    let (baskets_holding, baskets_on_job) = published_gear_pair(&app, band, "baskets");
    assert_eq!((baskets_holding, baskets_on_job), (0.0, 0.0));
    assert!(
        app.world
            .get::<BandEquipment>(band)
            .expect("ledger")
            .count_of("baskets")
            > 0,
        "…and the band DOES own baskets, so the zero is about the staffing and not the stock"
    );
    // LIVENESS, same frame: the job this band IS staffed on publishes a real denominator.
    let (spears_holding, spears_on_job) = published_gear_pair(&app, band, "spears");
    assert_eq!(
        (spears_holding, spears_on_job),
        (CREW as f32, CREW as f32),
        "the hunt row is staffed and fully armed, so the pair is `4 of 4` — a zero everywhere would \
         pass every assertion above"
    );
}

/// **A UNIFORMLY-EQUIPPED BAND PUBLISHES EXACTLY ONE ROW, never an empty list** (issue #520).
///
/// The same rule `KitCoverage` follows sim-side: a client must not have to tell *"no crews"* from
/// *"one crew holding nothing"*. Paired with the test above — asserting "one row" alone would pass
/// on a sim that had stopped dividing anything at all.
#[test]
fn a_fully_armed_band_publishes_exactly_one_hunt_crew() {
    let mut app = placid_world();
    let (herd, pos) = pin_herd(&mut app);
    let band = spawn_hunting_band(&mut app, pos, &herd, None);
    recapture_snapshot_in_place(&mut app.world);

    let (crews, hunter_attack, spears_holding) = published_hunt_composition(&app, band, "spears");
    let cfg = equipment(&app);

    assert_eq!(
        crews.len(),
        1,
        "everybody holds the same thing, so there is one run — and it is a ROW, not an empty list: \
         {crews:?}"
    );
    assert_eq!(
        crews[0].workers, CREW as f32,
        "the one row carries the whole hunt head count"
    );
    assert_eq!(crews[0].hunter_attack, equipped_attack(&cfg));
    assert_eq!(
        hunter_attack, crews[0].hunter_attack,
        "`hunterAttack` is still the best crew's tier when there is only one crew"
    );
    assert_eq!(
        spears_holding, CREW as f32,
        "the spears reach every hunter — the reserve above the head count arms nobody extra"
    );
}
