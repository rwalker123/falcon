//! **The minimal TOE — two kits, two tiers, one cliff** (`docs/plan_hunt_through_combat.md` §4.8,
//! `docs/plan_early_game_labor.md` → "Equipment / TOE").
//!
//! Pinned against **real turns through the real systems and the real snapshot export**, because the
//! two halves of this slice have deliberately different shapes and must not be allowed to hide in one
//! aggregate:
//!
//! - **The attack half is a provable IDENTITY.** Nothing reads a hunter's `attack` for the take
//!   today, so equipping or expiring the hunting kit must move **no** number the turn produces — the
//!   fight still fields the intrinsic `person` profile until the slice that resolves the kill through
//!   `combat::resolve_fight`. What it *does* move is the exported `hunterAttack`, which is the whole
//!   point of landing it now: `max(0, attack − defense)` needs `attack` to be a real number.
//! - **The carry half is a stated BALANCE CHANGE.** `per_worker_biomass_capacity` is live in every
//!   hunt take, so a dry carry kit genuinely hauls less.
//!
//! Every ordering assertion here is paired with a liveness assertion: *both* tiers are shown
//! reachable and productive, never just "the equipped one is bigger".

use bevy::{math::UVec2, prelude::Entity};
use core_sim::{
    available_workers, build_headless_app, run_turn, BandEquipment, CommandEventKind,
    CommandEventLog, CreaturesConfig, EquipmentConfig, Herd, HerdRegistry, LaborAllocation,
    LaborAssignment, LaborConfig, LaborTarget, PopulationCohort, SimulationConfig, SizeClass,
    SnapshotHistory, Tile,
};
use sim_schema::state::PopulationCohortState;

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own. Same
/// seed as the other food-ledger integration tests, for no reason beyond reproducibility.
const SEED: u64 = 119_304_647;
/// **Red Deer** — a herbivore with `ferocity 0.15`, so the Phase-0 hunt-danger fight genuinely fires
/// (which is what gives the attack-half identity something to be identical *about*) without the
/// aurochs-scale casualties that would churn the working-age bracket between turns.
const DEER: &str = "Red Deer";
const HERD_ID: &str = "toe_deer";
/// Standing stock far above anything a band can take, so the **escapement** never binds and the take
/// is decided by the crew — which is where the carry tier lives.
const HERD_BIOMASS: f32 = 6_000.0;
/// `Red Deer` body mass (`fauna_config.json`), the quantum the take is rounded to.
const DEER_BODY_MASS: f32 = 15.0;
const DEER_REGROWTH: f32 = 0.1;
const DEER_FODDER_PER_BIOMASS: f32 = 0.05;
/// A shallow floor: the hunt is allowed to draw the herd down, so nothing about escapement is under
/// test here.
const SHALLOW_FLOOR: f32 = 0.05;
/// Enough turns for wear to accumulate visibly, few enough that the herd cannot roam past the leash.
const RUN_TURNS: u32 = 4;
/// Exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
const EPSILON: f32 = 1e-3;

// ---------------------------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------------------------

/// A booted world with its one resident band hunting a fat deer herd standing on its camp.
/// `kit` is the band's starting wear, which is the only thing these tests vary.
fn hunting_world(kit: BandEquipment) -> (bevy::prelude::App, Entity) {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();

    let (band, tile_entity, workers) = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        let (e, c) = q.iter(&app.world).next().expect("a starting band");
        (e, c.current_tile, available_workers(c.working))
    };
    let band_pos = app
        .world
        .get::<Tile>(tile_entity)
        .expect("band tile")
        .position;

    seat_deer(&mut app, band_pos);
    app.world.entity_mut(band).insert((
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: SHALLOW_FLOOR,
                },
                workers: workers.max(1),
                improvement: None,
            }],
            ..Default::default()
        },
        kit,
    ));
    (app, band)
}

/// The same world with the band staffing **Scout** instead — it works every turn and marches, and
/// kills nothing. The control for "wear is charged for use, not for turns elapsed".
fn scouting_world(kit: BandEquipment) -> (bevy::prelude::App, Entity) {
    let (mut app, band) = hunting_world(kit);
    let workers = available_workers(app.world.get::<PopulationCohort>(band).unwrap().working);
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Scout,
            workers: workers.max(1),
            improvement: None,
        }],
        ..Default::default()
    });
    (app, band)
}

fn seat_deer(app: &mut bevy::prelude::App, pos: UVec2) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            HERD_ID.to_string(),
            DEER.to_string(),
            SizeClass::Big,
            vec![pos],
            HERD_BIOMASS,
            HERD_BIOMASS,
            DEER_FODDER_PER_BIOMASS,
            DEER_REGROWTH,
            DEER_BODY_MASS,
        ));
}

/// The band's row of the **exported snapshot** — the numbers the client actually reads.
fn exported(app: &bevy::prelude::App, band: Entity) -> PopulationCohortState {
    app.world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .clone()
        .expect("a snapshot was captured")
        .populations
        .iter()
        .find(|c| c.entity == band.to_bits())
        .expect("the resident band is exported")
        .clone()
}

fn herd_biomass(app: &bevy::prelude::App) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the seeded herd")
        .biomass
}

fn kit_of(app: &bevy::prelude::App, band: Entity) -> BandEquipment {
    *app.world.get::<BandEquipment>(band).expect("band kit")
}

fn equipment() -> std::sync::Arc<EquipmentConfig> {
    EquipmentConfig::builtin()
}

/// A kit worn exactly to its limit — the first spent state, since `equipped` is *strictly below*
/// `starting_durability`.
fn dry_carry() -> BandEquipment {
    BandEquipment {
        carry_wear: equipment().carry_kit.starting_durability,
        ..Default::default()
    }
}

fn dry_hunting() -> BandEquipment {
    BandEquipment {
        hunting_wear: equipment().hunting_kit.starting_durability,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// The band starts stocked
// ---------------------------------------------------------------------------------------------

/// **Start-stocked, and the wire says so.** A freshly generated band carries a full kit on both roles
/// and reports the *equipped* tier on both axes — which is the shipped opening, unchanged by this
/// slice.
#[test]
fn a_fresh_band_is_kitted_and_publishes_both_equipped_tiers() {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();
    let band = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        q.iter(&app.world).next().expect("a starting band").0
    };
    // The component is really there — `spawn_profile_population` must insert it, or every band's
    // wear would be silently discarded and the durability model would be inert.
    assert_eq!(
        app.world.get::<BandEquipment>(band).copied(),
        Some(BandEquipment::default()),
        "a spawned band starts with an UNWORN kit"
    );

    run_turn(&mut app);
    let cohort = exported(&app, band);
    let equipment = equipment();
    assert_eq!(
        cohort.hunting_kit_durability,
        equipment.hunting_kit.starting_durability
    );
    assert_eq!(
        cohort.carry_kit_durability,
        equipment.carry_kit.starting_durability
    );
    assert_eq!(
        cohort.hunter_attack, equipment.hunting_kit.equipped_attack,
        "a kitted hunter fights at the spear tier"
    );
    assert_eq!(
        cohort.carry_per_worker_biomass,
        LaborConfig::builtin().hunt.per_worker_biomass_capacity,
        "a kitted crew hauls at the shipped labor_config rate"
    );
}

// ---------------------------------------------------------------------------------------------
// The carry half — a stated balance change
// ---------------------------------------------------------------------------------------------

/// **Both carry tiers are reachable AND productive, and the dry one hauls less.** The liveness half
/// matters as much as the ordering: a dry band must still hunt (you can always take *some*), so a
/// zero take on either side would pass a naive "equipped > dry" check while meaning the model broke.
#[test]
fn both_carry_tiers_are_live_and_a_dry_kit_hauls_less() {
    let (mut kitted, kitted_band) = hunting_world(BandEquipment::default());
    let (mut dry, dry_band) = hunting_world(dry_carry());

    run_turn(&mut kitted);
    run_turn(&mut dry);

    let kitted_row = exported(&kitted, kitted_band);
    let dry_row = exported(&dry, dry_band);
    // **The biomass actually hauled home**, read off the carry kit's own wear — `carry_wear` is
    // charged exactly `wear_per_biomass_carried × carried`, so it inverts to the haul with no
    // arithmetic of our own. (The herd's raw biomass delta cannot be used: its ecological `K` is
    // recomputed from the range every turn, so most of the movement is the clamp, not the hunt.)
    let per_biomass = equipment().carry_kit.wear_per_biomass_carried;
    let kitted_haul = kit_of(&kitted, kitted_band).carry_wear / per_biomass;
    // The dry band started at its limit, so only the wear ADDED this turn is its haul.
    let dry_haul = (kit_of(&dry, dry_band).carry_wear - dry_carry().carry_wear) / per_biomass;

    assert!(
        kitted_haul > 0.0 && kitted_row.food_income > 0.0,
        "the equipped tier must actually bring game home (haul={kitted_haul}, \
         income={})",
        kitted_row.food_income
    );
    assert!(
        dry_haul > 0.0 && dry_row.food_income > 0.0,
        "the UNEQUIPPED tier must STILL bring game home — a dry band is a worse hunter, not a \
         non-hunter (haul={dry_haul}, income={})",
        dry_row.food_income
    );
    assert!(
        dry_haul < kitted_haul,
        "a dry carry kit must haul strictly less biomass: dry={dry_haul} vs kitted={kitted_haul}"
    );
    assert!(
        dry_row.food_income < kitted_row.food_income,
        "...and the player sees it as less food: dry={} vs kitted={}",
        dry_row.food_income,
        kitted_row.food_income
    );

    // ...and the wire carries both tiers explicitly, so the client never re-derives them.
    let equipment = equipment();
    assert_eq!(
        kitted_row.carry_per_worker_biomass,
        LaborConfig::builtin().hunt.per_worker_biomass_capacity
    );
    assert_eq!(
        dry_row.carry_per_worker_biomass,
        equipment.carry_kit.unequipped_per_worker_biomass_capacity
    );
    assert!(kitted_row.carry_kit_durability > 0.0);
    assert_eq!(
        dry_row.carry_kit_durability, 0.0,
        "a spent kit reads exactly 0 remaining, never negative"
    );
}

/// **The cliff is a CLIFF.** Performance is *flat* right up to expiry and then steps down — a
/// gradual taper would pass a "kit matters" test and is the wrong model (durability and performance
/// are orthogonal axes). Asserted on the **exported** rate across a wear sweep, and on both kits.
#[test]
fn the_durability_cliff_is_a_step_not_a_taper() {
    let equipment = equipment();
    let hunting_limit = equipment.hunting_kit.starting_durability;
    let carry_limit = equipment.carry_kit.starting_durability;
    // Fractions of each kit's life, from brand new to one hair short of spent.
    let almost_spent = [0.0, 0.25, 0.5, 0.75, 0.999];

    let mut carry_rates = Vec::new();
    let mut attacks = Vec::new();
    for fraction in almost_spent {
        // **A SCOUTING band, deliberately.** The sweep is about the *shape* of the tier function, so
        // the turn must not itself add wear — a hunting turn at the 0.999 sample would tip the kit
        // over the limit and the sweep would measure the cliff instead of the flat.
        let (mut app, band) = scouting_world(BandEquipment {
            hunting_wear: hunting_limit * fraction,
            carry_wear: carry_limit * fraction,
        });
        run_turn(&mut app);
        let row = exported(&app, band);
        carry_rates.push(row.carry_per_worker_biomass);
        attacks.push(row.hunter_attack);
    }
    // Flat — every reading below expiry is the SAME number, not a decreasing series.
    for (i, rate) in carry_rates.iter().enumerate() {
        assert_eq!(
            *rate, carry_rates[0],
            "carry performance must be flat until expiry (wear fraction {} read {rate} vs {})",
            almost_spent[i], carry_rates[0]
        );
        assert_eq!(
            attacks[i], attacks[0],
            "attack must be flat until expiry (wear fraction {})",
            almost_spent[i]
        );
    }

    // ...then one step down at the limit, on both axes.
    let (mut spent_app, spent_band) = scouting_world(BandEquipment {
        hunting_wear: hunting_limit,
        carry_wear: carry_limit,
    });
    run_turn(&mut spent_app);
    let spent = exported(&spent_app, spent_band);
    assert!(
        spent.carry_per_worker_biomass < carry_rates[0],
        "the carry role must STEP DOWN at expiry: {} -> {}",
        carry_rates[0],
        spent.carry_per_worker_biomass
    );
    assert!(
        spent.hunter_attack < attacks[0],
        "the hunting role must STEP DOWN at expiry: {} -> {}",
        attacks[0],
        spent.hunter_attack
    );
    assert_eq!(
        spent.hunter_attack,
        CreaturesConfig::builtin().person().attack,
        "the step lands exactly on the BARE-HANDED tier, not on some interpolated value"
    );
    assert_eq!(
        spent.carry_per_worker_biomass, equipment.carry_kit.unequipped_per_worker_biomass_capacity,
        "the step lands exactly on the unequipped carry tier"
    );
}

// ---------------------------------------------------------------------------------------------
// Wear is charged for USE, never for turns
// ---------------------------------------------------------------------------------------------

/// **A party that kills nothing loses no durability, however long it works.** `plan_denial_raid.md`
/// §1.2 depends on this: a turn-based clock would charge an idle march the same as a slaughter, which
/// would make denial free. Same world, same turn count, different *work*.
#[test]
fn wear_is_charged_for_kills_not_for_turns_elapsed() {
    let (mut hunting, hunting_band) = hunting_world(BandEquipment::default());
    let (mut scouting, scouting_band) = scouting_world(BandEquipment::default());
    for _ in 0..RUN_TURNS {
        run_turn(&mut hunting);
        run_turn(&mut scouting);
    }

    let hunted = kit_of(&hunting, hunting_band);
    let scouted = kit_of(&scouting, scouting_band);

    // Liveness: the hunting band really did wear its kit down over the same span.
    assert!(
        hunted.hunting_wear > 0.0 && hunted.carry_wear > 0.0,
        "a hunting band must wear BOTH kits over {RUN_TURNS} turns (got {hunted:?})"
    );
    // The discriminating half: identical turn count, zero kills, zero wear — on BOTH kits.
    assert_eq!(
        scouted,
        BandEquipment::default(),
        "a band that killed nothing for {RUN_TURNS} turns must lose no durability at all"
    );

    // ...and the hunting kit's wear is an exact whole number of kills, never a per-turn drip.
    let per_kill = equipment().hunting_kit.wear_per_kill;
    let kills = hunted.hunting_wear / per_kill;
    assert!(
        (kills - kills.round()).abs() < EPSILON,
        "hunting wear must be an exact multiple of wear_per_kill ({per_kill}): {} => {kills} kills",
        hunted.hunting_wear
    );
    assert!(
        kills >= 1.0,
        "the run has to have killed something for this to mean anything (got {kills})"
    );
}

/// **Running dry drops the role and it STAYS dropped** — there is no replenishment path in this
/// slice, so wear is monotonically non-decreasing and the unequipped tier is absorbing.
#[test]
fn a_kit_run_dry_stays_dry() {
    let equipment = equipment();
    let carry_limit = equipment.carry_kit.starting_durability;
    // One kill's worth of carry life left: this run crosses the cliff mid-flight.
    let almost = carry_limit - equipment.carry_kit.wear_per_biomass_carried * DEER_BODY_MASS;
    let (mut app, band) = hunting_world(BandEquipment {
        carry_wear: almost,
        ..Default::default()
    });

    let mut wear_series = Vec::new();
    let mut rate_series = Vec::new();
    for _ in 0..RUN_TURNS * 2 {
        run_turn(&mut app);
        wear_series.push(kit_of(&app, band).carry_wear);
        rate_series.push(exported(&app, band).carry_per_worker_biomass);
    }

    // Monotonic: nothing in this slice ever gives condition back.
    for pair in wear_series.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "wear must never decrease (no replenishment exists): {pair:?}"
        );
    }
    // The cliff was actually crossed (liveness), and once crossed it is absorbing.
    let unequipped = equipment.carry_kit.unequipped_per_worker_biomass_capacity;
    let first_dry = rate_series
        .iter()
        .position(|rate| *rate == unequipped)
        .expect("the run must actually cross the cliff — otherwise this test proves nothing");
    assert!(
        rate_series[first_dry..]
            .iter()
            .all(|rate| *rate == unequipped),
        "once dry the role stays on the unequipped tier: {rate_series:?}"
    );
    assert!(
        wear_series.last().copied().unwrap_or_default() >= carry_limit,
        "the kit ends the run spent"
    );
}

// ---------------------------------------------------------------------------------------------
// The attack half — a provable identity
// ---------------------------------------------------------------------------------------------

/// **The attack half moves NOTHING the turn produces — yet.** Nothing reads a hunter's `attack` for
/// the take today (the Phase-0 hunt-danger fight still fields the intrinsic `person` profile), so a
/// band with a spent hunting kit and a band with a full one must resolve a hunt *identically*: same
/// take, same food income, same casualties.
///
/// The paired liveness assertion is the reason this ships at all: the exported `hunterAttack`
/// **does** differ, 20 against 1, which is the number the fight's `max(0, attack − defense)` gate will
/// be resolved through when the kill moves into `combat::resolve_fight`.
#[test]
fn the_attack_tier_is_published_but_inert_in_the_take() {
    let (mut kitted, kitted_band) = hunting_world(BandEquipment::default());
    let (mut bare, bare_band) = hunting_world(dry_hunting());

    for _ in 0..RUN_TURNS {
        run_turn(&mut kitted);
        run_turn(&mut bare);
    }

    let kitted_row = exported(&kitted, kitted_band);
    let bare_row = exported(&bare, bare_band);

    // The identity: every number the turn produced is the same.
    assert_eq!(
        herd_biomass(&kitted),
        herd_biomass(&bare),
        "the take must not depend on the hunting kit yet — slice 4 owns the fight"
    );
    assert_eq!(kitted_row.food_income, bare_row.food_income);
    assert_eq!(
        kitted
            .world
            .get::<PopulationCohort>(kitted_band)
            .unwrap()
            .working,
        bare.world
            .get::<PopulationCohort>(bare_band)
            .unwrap()
            .working,
        "hunt-danger casualties must not depend on the hunting kit yet"
    );
    // ...and the carry kit wore identically too, since the take did.
    assert_eq!(
        kit_of(&kitted, kitted_band).carry_wear,
        kit_of(&bare, bare_band).carry_wear
    );

    // Liveness on the other side: the tiers really are different, and the fight really did happen
    // (a hunt that never engaged anything would make the identity above vacuous).
    let equipment = equipment();
    assert_eq!(
        kitted_row.hunter_attack,
        equipment.hunting_kit.equipped_attack
    );
    assert_eq!(
        bare_row.hunter_attack,
        CreaturesConfig::builtin().person().attack
    );
    assert!(
        bare_row.hunter_attack < kitted_row.hunter_attack,
        "the spear must be the larger number, or the gate opens the wrong way"
    );
    // ...and the danger fight really fired in both worlds, so "casualties are identical" is a
    // statement about a fight that happened rather than about one that never started. (The cohort's
    // `working` bracket cannot carry this: births move it too, and on a fat deer herd they outrun
    // the losses.)
    assert!(
        hunt_danger_fired(&kitted) && hunt_danger_fired(&bare),
        "the deer's ferocity must actually turn the hunt into a fight in BOTH worlds"
    );
}

/// Did the Phase-0 hunt-danger adapter resolve a fight this run? Read off the command feed, which is
/// the only place a casualty-causing hunt announces itself.
fn hunt_danger_fired(app: &bevy::prelude::App) -> bool {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .any(|entry| entry.kind == CommandEventKind::HuntDanger)
}
