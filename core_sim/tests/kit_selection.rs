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
    StartingUnit, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};

/// The crew every fixture in this file staffs, so two arms are only ever comparable to each other.
const CREW: u32 = 4;

/// **A quarry that cannot fight back** (`combat.attack 0`) and is light enough that a small crew
/// engages several animals a turn — so a take is a real number at both tiers rather than a run of
/// all-or-nothing draws. The same species `denial_raid.rs` measures on, for the same reason.
const HARMLESS_QUARRY: &str = "Rabbit Warren";

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
            BandEquipment::default(),
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
    app.world
        .spawn((
            cohort(tile, CREW),
            LaborAllocation::default(),
            BandEquipment::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.to_string(),
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
                kit,
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
        .spawn((cohort(tile, 20), ResidentBand, BandEquipment::default()))
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
        BandEquipment::default(),
        "a crew using no component spends no durability on ANY of them — this is the pairing that \
         makes a bare-handed comparison free to run"
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
                BandEquipment::default(),
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
        BandEquipment::default(),
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
        band_kit.restore_wear("spears", item_durability(&cfg, "spears"));
        band_kit.restore_wear("sled", item_durability(&cfg, "sled"));
        band_kit.restore_wear("baskets", item_durability(&cfg, "baskets"));
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
        let durability = item_durability(&equipment(&app), "spears");
        let mut wear = app
            .world
            .get_mut::<BandEquipment>(party)
            .expect("the party carries a wear ledger");
        wear.restore_wear("spears", durability);
    }
    let sled_before = wear_of(&app, party).wear_of("sled");
    drive_party_turn(&mut app);
    let after = wear_of(&app, party);
    assert_eq!(
        after.wear_of("spears"),
        item_durability(&equipment(&app), "spears"),
        "spent spears are not charged again — the predicate that chose the tier gates the charge"
    );
    assert!(
        after.wear_of("sled") >= sled_before,
        "…while the sled, which is still serving, goes on being charged for what it hauls"
    );
}

// ---------------------------------------------------------------------------------------------
// THE WIRE
// ---------------------------------------------------------------------------------------------

/// **The roster reaches the client, and both estimate tables SAY which kit they are quoted for.**
///
/// Neither `huntTripEstimates` nor `denialEstimates` is repriced per kit — they are ~95% of snapshot
/// capture and a kit axis multiplies them — so the field exists precisely so a client can refuse to
/// present a table as an answer for a selection it was not computed for. A missing or wrong id here
/// is the "a sheet quoting a kitted raid to a bare-handed party" defect, on the wire.
#[test]
fn the_published_snapshot_carries_the_roster_and_names_the_kit_the_tables_are_quoted_for() {
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

    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the pinned herd is on the wire");
    assert_eq!(
        herd.hunt_trip_estimates_kit_id, expected_hunt_default,
        "the hunt table names the kit it was priced at"
    );
    assert_eq!(
        herd.denial_estimates_kit_id, expected_hunt_default,
        "and so does the denial table"
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
            BandEquipment::default(),
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
        .spawn((cohort(tile, CREW), ResidentBand, BandEquipment::default()))
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

/// **The shipped durability of one item**, by id — the tests read dials off the item table now that
/// `equipment.json` has one, rather than off three named blocks.
fn item_durability(cfg: &EquipmentConfig, id: &str) -> f32 {
    cfg.item(id)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{id}'"))
        .starting_durability
}

/// The **unequipped** carry rate a carry item declares — the tier a party without it falls back to.
fn unequipped_carry(cfg: &EquipmentConfig, id: &str) -> f32 {
    let stat = if id == "sled" {
        EquipmentStat::HuntCarry
    } else {
        EquipmentStat::ForageCarry
    };
    match cfg.item(id).and_then(|item| item.effect(stat)) {
        Some(EffectTier::Unequipped(value)) => value,
        other => panic!("'{id}' must declare an unequipped {stat:?}, got {other:?}"),
    }
}

/// The **equipped** `attack` a speared party fights at.
fn equipped_attack(cfg: &EquipmentConfig) -> f32 {
    match cfg
        .item("spears")
        .and_then(|item| item.effect(EquipmentStat::Attack))
    {
        Some(EffectTier::Equipped(value)) => value,
        other => panic!("spears must declare an equipped attack, got {other:?}"),
    }
}
