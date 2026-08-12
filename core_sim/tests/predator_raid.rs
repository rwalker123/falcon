//! Predators Phase 1b — the raid trigger + the Warrior role's first live consumer
//! (`docs/plan_predators.md`).
//!
//! A carnivore with `aggression > 0` within `predators.raid_radius` of a **resident** band raids its
//! camp; the band is defended by its **Warriors**. `advance_predator_raids` builds a fight (a single
//! representative of the pack vs the band's Warriors + exposed populace), resolves it through the
//! neutral combat subsystem, and applies **only the band-side** working-age casualties. These tests
//! drive the **real** system against **real cohort brackets** and the **real feed log**.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};
use bevy::MinimalPlugins;

use core_sim::{
    advance_predator_raids, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, ForageRegistry,
    GenerationId, GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand,
    SimulationConfig, SimulationTick, SizeClass, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, SourceYield, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, TileRegistry, WellbeingConfigHandle, FOOD,
};

/// The shipped carnivore — `Grey Wolf Pack` (`attack 3`, `aggression 0.6`), so its raid attack is
/// `3 × 0.6 = 1.8 > 0`: it raids.
const WOLF: &str = "Grey Wolf Pack";
/// A herbivore (`diet herbivore`, `aggression 0`) — it never raids.
const DEER: &str = "Red Deer";
/// The shipped wolf's `combat.durability` (`fauna_config.json`) — how much damage a pack soaks before
/// it goes down (`docs/plan_hunt_through_combat.md` §4.2). Restated here so the hand-built fight below
/// is the roster's wolf and not a neutral stand-in.
const WOLF_DURABILITY: f32 = 20.0;
/// The shipped `person` row's `combat.durability` (`creatures.json`), for the same reason.
const PERSON_DURABILITY: f32 = 20.0;

/// Build a real earthlike world (so tiles exist for the band position lookup), then **clear** the
/// worldgen herds and resident bands so each test controls exactly the actors on the map. Returns the
/// app, a guaranteed land position, and that tile's entity.
fn arena() -> (App, UVec2, Entity) {
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
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);

    // A guaranteed land position (a real herd spawned on it) → its tile entity.
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| h.position())
        .next()
        .expect("the world seeded at least one herd");
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the land tile resolves");

    // Clear the worldgen actors so the test is the sole author of the herds and bands on the map.
    app.world.resource_mut::<HerdRegistry>().herds.clear();
    let existing: Vec<Entity> = app
        .world
        .query_filtered::<Entity, With<PopulationCohort>>()
        .iter(&app.world)
        .collect();
    for entity in existing {
        app.world.despawn(entity);
    }

    (app, pos, tile)
}

/// Seat a hand-built **stationary** herd of `species` at `pos`. Only its species + position matter to
/// the raid path; biomass/cap/r/body are inert here.
fn seat(app: &mut App, id: &str, species: &str, pos: UVec2) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            id.to_string(),
            species.to_string(),
            SizeClass::Small,
            vec![pos],
            120.0,
            120.0,
            0.0,
            0.15,
            30.0,
        ));
}

/// Spawn a **resident** band on `tile` with `working` working-age adults and `warriors` assigned to
/// the Warrior role.
fn resident_band(app: &mut App, tile: Entity, working: u32, warriors: u32) -> Entity {
    let assignments = if warriors > 0 {
        vec![LaborAssignment {
            target: LaborTarget::Warrior,
            workers: warriors,
            improvement: None,
            kit: None,
        }]
    } else {
        Vec::new()
    };
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: working,
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
            },
            LaborAllocation {
                assignments,
                ..Default::default()
            },
            ResidentBand,
        ))
        .id()
}

fn working_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .working
        .to_f32()
}

/// Give a band **this turn's food income** and a larder to draw it from. `advance_labor_allocation`
/// normally credits `last_yields[i].actual` to the larder earlier in the Population stage; these
/// isolated raid tests fake both so the raid path has a real income to forfeit and a real larder to
/// debit. One `Forage` yield row of `income`, and `larder` provisions in the store.
fn seed_income_and_larder(app: &mut App, band: Entity, income: f32, larder: f32) {
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.stores.set(FOOD, scalar_from_f32(larder));
    }
    let mut alloc = app.world.get_mut::<LaborAllocation>(band).unwrap();
    alloc.last_yields = vec![SourceYield {
        actual: income,
        ..SourceYield::ZERO
    }];
}

fn larder_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32()
}

fn raid_forfeit_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .unwrap()
        .last_raid_forfeit
}

/// Whether at least one `predator_raid` feed line exists for faction 0.
fn has_raid_line(app: &App) -> bool {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .any(|e| e.kind.as_str() == "predator_raid")
}

/// **An unguarded band bleeds.** A wolf (aggression 0.6) on the band's own tile, 0 warriors → the band
/// loses working-age population AND a `predator_raid` feed line is pushed.
#[test]
fn an_unguarded_band_bleeds_and_narrates() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let band = resident_band(&mut app, tile, 30, 0);

    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);
    let after = working_of(&app, band);

    assert!(
        after < before,
        "an unguarded band raided by a wolf must lose working-age people: {before} -> {after}"
    );
    assert!(
        has_raid_line(&app),
        "a casualty-causing raid must push a predator_raid feed line"
    );
}

/// **Warriors cut losses.** The same pack + band size, but with warriors assigned, loses strictly
/// FEWER working-age people (the warriors add power, thinning the enemy-relative loss ratio). Two
/// bands, one turn — deliberately in the unclamped regime (30 working-age, a few warriors).
#[test]
fn warriors_cut_a_bands_losses() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let unguarded = resident_band(&mut app, tile, 30, 0);
    let guarded = resident_band(&mut app, tile, 30, 5);

    let unguarded_before = working_of(&app, unguarded);
    let guarded_before = working_of(&app, guarded);
    app.world.run_system_once(advance_predator_raids);
    let unguarded_loss = unguarded_before - working_of(&app, unguarded);
    let guarded_loss = guarded_before - working_of(&app, guarded);

    assert!(
        unguarded_loss > 0.0,
        "the unguarded band must take losses to compare against: {unguarded_loss}"
    );
    assert!(
        guarded_loss < unguarded_loss,
        "warriors must cut the band's losses: guarded {guarded_loss} vs unguarded {unguarded_loss}"
    );
}

/// **No raid without a trigger — (a) a herbivore in range causes nothing.** A deer (herbivore,
/// aggression 0) on the band's tile is not a raider: no casualties, no feed line.
#[test]
fn a_herbivore_never_raids() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "game_deer", DEER, pos);
    let band = resident_band(&mut app, tile, 30, 0);

    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);
    assert_eq!(
        before,
        working_of(&app, band),
        "a herbivore must not raid a band's camp"
    );
    assert!(
        !has_raid_line(&app),
        "a herbivore must push no predator_raid feed line"
    );
}

/// **No raid without a trigger — (b) a carnivore OUT of `raid_radius` causes nothing.** A wolf placed
/// far from the band (well beyond the raid reach) never reaches the camp.
#[test]
fn a_distant_carnivore_never_raids() {
    let (mut app, pos, tile) = arena();
    // `raid_radius` defaults to 2; place the wolf ~10 tiles away on the same row.
    let far = UVec2::new(pos.x + 10, pos.y);
    seat(&mut app, "pred_wolf", WOLF, far);
    let band = resident_band(&mut app, tile, 30, 0);

    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);
    assert_eq!(
        before,
        working_of(&app, band),
        "a carnivore out of raid_radius must not raid the camp"
    );
    assert!(
        !has_raid_line(&app),
        "an out-of-range carnivore must push no predator_raid feed line"
    );
}

/// **Aggression scales lethality.** A higher-aggression carnivore inflicts strictly more casualties
/// than a lower-aggression one, all else equal — mirroring the Phase-0 ferocity-scaling test, asserted
/// via a direct `resolve_fight` on the raid's own two-contingent band composition (the adapter feeds
/// the resolver `attack × aggression`, so the two differ only in that product).
#[test]
fn aggression_scales_raid_lethality() {
    use core_sim::{
        resolve_fight, CombatStats, CombatTuning, Contingent, ContingentId, FightPayload, Force,
        ForceId, Posture, RangeBand,
    };

    // The raid's exact band side: a few Warriors (attack 1) + the exposed unarmed folk (attack 0),
    // against a single pack representative at `attack × aggression`.
    let band_killed_at = |aggression: f32| -> f32 {
        let payload = FightPayload {
            sides: vec![
                Force {
                    id: ForceId(0),
                    posture: Posture::Aggressor,
                    contingents: vec![Contingent {
                        kind: ContingentId::from("Grey Wolf Pack"),
                        count: 1.0,
                        profile: CombatStats {
                            attack: 3.0 * aggression, // the adapter's `attack × aggression`
                            defense: 3.0,
                            durability: WOLF_DURABILITY,
                            range: RangeBand::Melee,
                            wariness: 0.0,
                        },
                    }],
                },
                Force {
                    id: ForceId(1),
                    posture: Posture::Defender,
                    contingents: vec![
                        Contingent {
                            kind: ContingentId::from("warrior"),
                            count: 3.0,
                            profile: CombatStats {
                                attack: 1.0,
                                defense: 1.0,
                                durability: PERSON_DURABILITY,
                                range: RangeBand::Melee,
                                wariness: 0.0,
                            },
                        },
                        Contingent {
                            kind: ContingentId::from("person"),
                            count: 4.0,
                            profile: CombatStats {
                                attack: 0.0,
                                defense: 1.0,
                                durability: PERSON_DURABILITY,
                                range: RangeBand::Melee,
                                wariness: 0.0,
                            },
                        },
                    ],
                },
            ],
            terrain: vec![],
            seed: 0,
        };
        resolve_fight(&payload, &CombatTuning::default())
            .results
            .iter()
            .filter(|r| r.force == ForceId(1))
            .map(|r| r.killed)
            .sum()
    };

    let mild = band_killed_at(0.2);
    let fierce = band_killed_at(0.6);
    assert!(
        fierce > mild,
        "a more aggressive carnivore must inflict strictly more casualties: mild {mild}, fierce {fierce}"
    );
}

/// **A casualty-causing raid on a WORKING band forfeits food** (Predators Phase 3). The band earned
/// `income` this turn; the raid debits `raid_yield_forfeit_fraction` (0.25) of it from the larder and
/// reports the actual take in `last_raid_forfeit`.
#[test]
fn a_raid_forfeits_a_fraction_of_a_working_bands_income() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let band = resident_band(&mut app, tile, 30, 0);
    // Ample larder so the whole forfeit is payable; a real income to forfeit a slice of.
    let income = 40.0;
    let larder = 500.0;
    seed_income_and_larder(&mut app, band, income, larder);

    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);

    assert!(
        working_of(&app, band) < before,
        "the raid must cause casualties (the trigger for the forfeit)"
    );
    // Default `raid_yield_forfeit_fraction` is 0.25.
    let expected = 0.25 * income;
    let forfeit = raid_forfeit_of(&app, band);
    assert!(
        (forfeit - expected).abs() < 1e-3,
        "a raid forfeits 0.25 × income: expected {expected}, got {forfeit}"
    );
    // The larder was actually debited by exactly that amount.
    assert!(
        (larder_of(&app, band) - (larder - expected)).abs() < 1e-3,
        "the forfeit is a REAL larder debit"
    );
}

/// **An IDLE raided band forfeits nothing** — it had no income to lose, so a raid costs it only people.
#[test]
fn an_idle_raided_band_forfeits_no_food() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let band = resident_band(&mut app, tile, 30, 0);
    // No income row at all, but a full larder — the forfeit must still be zero (0.25 × 0 = 0).
    let larder = 500.0;
    seed_income_and_larder(&mut app, band, 0.0, larder);

    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);

    assert!(
        working_of(&app, band) < before,
        "an idle band still loses PEOPLE to the raid"
    );
    assert_eq!(
        raid_forfeit_of(&app, band),
        0.0,
        "a band with no income forfeits no food"
    );
    assert!(
        (larder_of(&app, band) - larder).abs() < 1e-3,
        "an idle band's larder is untouched by the raid"
    );
}

/// **The forfeit is capped at the available larder** — it is a `LocalStore::take`, so it can never go
/// negative even when `0.25 × income` exceeds what the band holds.
#[test]
fn the_forfeit_is_capped_at_the_larder() {
    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let band = resident_band(&mut app, tile, 30, 0);
    // Big income (forfeit would be 0.25 × 100 = 25), but the larder holds only 2.
    let income = 100.0;
    let thin_larder = 2.0;
    seed_income_and_larder(&mut app, band, income, thin_larder);

    app.world.run_system_once(advance_predator_raids);

    let forfeit = raid_forfeit_of(&app, band);
    assert!(
        (forfeit - thin_larder).abs() < 1e-3,
        "the forfeit is capped at the larder: expected {thin_larder}, got {forfeit}"
    );
    assert!(
        larder_of(&app, band) <= 1e-3,
        "the larder is drained to (a floored) zero, never negative"
    );
}

// The `raid_yield_forfeit_fraction` **config-rejection** (out of `[0, 1]`) is pinned next to its
// sibling predator-config bounds in `fauna_config.rs`'s unit tests
// (`validate_rejects_an_out_of_range_raid_yield_forfeit_fraction`), where the `reject` /
// `assert_rejects_field` JSON harness lives — `FaunaConfig::builtin()` returns an `Arc`, so a bad
// value can't be assigned onto it here.

// ---------------------------------------------------------------------------------------------
// #520 — the warrior line is only as armed as the band's club stock
// ---------------------------------------------------------------------------------------------

/// The warrior TOE. The sim never spells an item; a test standing a band up short of one has to.
const CLUBS: &str = "clubs";

/// Give `band` exactly `clubs` clubs, unworn, at the tier that ships known.
fn arm_with_clubs(app: &mut App, band: Entity, clubs: u32) {
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let tier = equipment
        .item(CLUBS)
        .expect("the shipped roster carries clubs")
        .default_tier()
        .id
        .clone();
    let mut ledger = core_sim::BandEquipment::default();
    ledger.stock(CLUBS, clubs, &tier, None);
    app.world.entity_mut(band).insert(ledger);
}

/// **A BAND WITH THREE CLUBS AND EIGHT WARRIORS STANDS THREE ARMED DEFENDERS UP, NOT EIGHT**
/// (issue #520).
///
/// `advance_predator_raids` builds its warrior contingents from
/// `EquipmentConfig::coverage(&warrior_kit, warrior_count, wear)` — the same seam the hunt divides a
/// party with — so the clubs reach whoever they reach and the rest defend at the bare hand's `1`.
/// This asserts the composition through those very seams, then drives a **real raid** on the same
/// band so the wiring is exercised end to end.
///
/// > **The band's own casualties do not move, and that is the placeholder resolver, not this
/// > change.** `combat::resolve_fight` sizes a side's losses from the *enemy's* power over its own
/// > **head count**; a defender's `attack` reaches only the enemy's losses (discarded here) and
/// > `force_power`, which `docs/plan_predators.md` Phase 1b leaves as a placeholder. So what the
/// > partly-clubbed warrior line changes today is what the pack takes, not what the camp loses.
#[test]
fn a_band_short_of_clubs_stands_up_an_armed_line_and_a_bare_one() {
    const WARRIORS: u32 = 8;
    const CLUBS_OWNED: u32 = 3;

    let (mut app, pos, tile) = arena();
    seat(&mut app, "pred_wolf", WOLF, pos);
    let band = resident_band(&mut app, tile, 30, WARRIORS);
    arm_with_clubs(&mut app, band, CLUBS_OWNED);

    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let person = app
        .world
        .resource::<core_sim::CreaturesConfigHandle>()
        .get()
        .person();
    let wear = app
        .world
        .get::<core_sim::BandEquipment>(band)
        .expect("the band was just armed")
        .clone();
    let warrior_kit = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band has an allocation")
        .kit_on(&LaborTarget::Warrior, &equipment);

    let coverage = equipment.coverage(&warrior_kit, WARRIORS as f32, &wear);
    assert_eq!(
        coverage.crews().len(),
        2,
        "three clubs across eight warriors is two lines: {coverage:?}"
    );
    assert_eq!(coverage.crews()[0].workers, CLUBS_OWNED as f32);
    assert_eq!(
        coverage.crews()[1].workers,
        (WARRIORS - CLUBS_OWNED) as f32,
        "the five the clubs did not reach still turn out"
    );
    let armed = equipment.warrior_profile(person, &coverage.crews()[0].kit, &wear);
    let bare = equipment.warrior_profile(person, &coverage.crews()[1].kit, &wear);
    assert!(
        armed.attack > bare.attack,
        "the clubbed line swings the club's tier and the other the bare hand's ({armed:?} vs \
         {bare:?})"
    );
    assert_eq!(
        bare.attack, person.attack,
        "and the unarmed line is exactly the `person` roster row — no borrowed club"
    );

    // ...and the real raid resolves against that band without incident.
    let before = working_of(&app, band);
    app.world.run_system_once(advance_predator_raids);
    assert!(
        working_of(&app, band) < before,
        "liveness — the pack really raided this band, so the composition above was the one used"
    );
}

/// **A CLUB IS CHARGED FOR THE BLOWS IT LANDED, NOT FOR THE RAID IT ATTENDED** (the strike quantum).
///
/// `WearQuantum::Fight` charged the whole warrior line one use per engagement, which billed a band
/// for weapons most of its warriors may never have swung. The club now shares `Strike` with the
/// spear: a line that clears the pack's `defense` is charged per blow, a line that cannot is charged
/// nothing, and **a band nobody raided still pays zero** — `docs/plan_denial_raid.md` §1.2 intact.
///
/// The three arms are compared against **each other** rather than against literals, because the
/// number moves with `combat_config` and with the pack's own `durability`.
#[test]
fn clubs_wear_per_blow_landed_and_an_unraided_band_pays_nothing() {
    const WARRIORS: u32 = 8;
    const FEW_CLUBS: u32 = 3;

    let club_wear_after_a_raid = |clubs: u32, seat_a_pack: bool| -> f32 {
        let (mut app, pos, tile) = arena();
        if seat_a_pack {
            seat(&mut app, "pred_wolf", WOLF, pos);
        }
        let band = resident_band(&mut app, tile, 30, WARRIORS);
        arm_with_clubs(&mut app, band, clubs);
        app.world.run_system_once(advance_predator_raids);
        app.world
            .get::<core_sim::BandEquipment>(band)
            .expect("the band was armed")
            .wear_of(CLUBS)
    };

    let fully_clubbed = club_wear_after_a_raid(WARRIORS, true);
    let partly_clubbed = club_wear_after_a_raid(FEW_CLUBS, true);
    let unraided = club_wear_after_a_raid(WARRIORS, false);

    assert_eq!(
        unraided, 0.0,
        "a band nobody raided swung nothing — wear is charged for USE, never for turns elapsed"
    );
    assert!(
        partly_clubbed > 0.0,
        "the clubbed line really landed blows: {partly_clubbed}"
    );
    assert!(
        partly_clubbed < fully_clubbed,
        "three clubs land fewer blows than eight, so they wear less: {partly_clubbed} vs \
         {fully_clubbed} — a per-ENGAGEMENT charge would read the same for both"
    );
}

/// **Determinism.** Two identical raid setups leave bit-identical casualties (the resolver is pure and
/// the seed is derived, not random).
#[test]
fn a_raid_is_deterministic() {
    let run = || -> f32 {
        let (mut app, pos, tile) = arena();
        seat(&mut app, "pred_wolf", WOLF, pos);
        let band = resident_band(&mut app, tile, 30, 2);
        let before = working_of(&app, band);
        app.world.run_system_once(advance_predator_raids);
        before - working_of(&app, band)
    };
    assert_eq!(
        run(),
        run(),
        "a raid must be bit-identical across identical runs"
    );
}
