//! **The minimal TOE — two kits, two tiers, one cliff** (`docs/plan_hunt_through_combat.md` §4.8,
//! `docs/plan_early_game_labor.md` → "Equipment / TOE").
//!
//! Pinned against **real turns through the real systems and the real snapshot export**, because the
//! two halves of this slice have deliberately different shapes and must not be allowed to hide in one
//! aggregate:
//!
//! - **The attack half is now LIVE in the take** (`docs/plan_hunt_through_combat.md` §4, slice 4).
//!   The kill resolves through `combat::resolve_fight`, so `max(0, attack − defense)` decides whether
//!   a band can hurt its quarry at all: a spear-armed band takes Red Deer and a bare-handed one takes
//!   **nothing** from them, at any headcount (§4.8 — "everything from a gazelle upward is
//!   untouchable"). It shipped as a provable identity one slice earlier; that is now inverted, and
//!   the test below asserts the difference rather than the sameness.
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
/// **Wild Horses** — the *carry* half's quarry, and it has to be a different animal since slice 4.
///
/// §4.6's per-hunter-turn ceiling is `min(engage_rate, (attack − defense) / durability) × body_mass`,
/// and the carry tier only decides a take when that ceiling sits **above** the dry rate (12) and
/// **below** the kitted one (40) — otherwise one bound or the other binds on both tiers and the two
/// hauls are identical. A Red Deer's ceiling is `min(1, 0.76) × 15 = 11.4`, under *both* rates; a
/// Wild Horse's is `min(0.5, 0.514) × 40 = 20.0`, squarely between them. That is the regime the carry
/// kit is a lever in, so that is where it is measured.
const HORSE: &str = "Wild Horses";
/// `Wild Horses` body mass (`fauna_config.json`).
const HORSE_BODY_MASS: f32 = 40.0;

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
    hunting_world_of(DEER, DEER_BODY_MASS, kit)
}

/// [`hunting_world`] against a named quarry — the fight's dials are resolved off the species
/// (`FaunaConfig::quarry_fight_for`), so a test about which *bound* binds has to say which animal it
/// means.
fn hunting_world_of(
    species: &str,
    body_mass: f32,
    kit: BandEquipment,
) -> (bevy::prelude::App, Entity) {
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

    seat_quarry(&mut app, band_pos, species, body_mass);
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

fn seat_quarry(app: &mut bevy::prelude::App, pos: UVec2, species: &str, body_mass: f32) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            HERD_ID.to_string(),
            species.to_string(),
            SizeClass::Big,
            vec![pos],
            HERD_BIOMASS,
            HERD_BIOMASS,
            DEER_FODDER_PER_BIOMASS,
            DEER_REGROWTH,
            body_mass,
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
    let (mut kitted, kitted_band) =
        hunting_world_of(HORSE, HORSE_BODY_MASS, BandEquipment::default());
    let (mut dry, dry_band) = hunting_world_of(HORSE, HORSE_BODY_MASS, dry_carry());

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

/// **The attack half DECIDES the take** (`docs/plan_hunt_through_combat.md` §4, slice 4) — the
/// inversion of what this test asserted one slice ago, when nothing read a hunter's `attack` and the
/// two tiers were a provable identity.
///
/// The kill is now `combat::resolve_fight`'s enemy losses, so §4.2's gate `max(0, attack − defense)`
/// runs the whole hunt: against a Red Deer's `defense 1` a spear-armed hunter deals `19` and brings
/// down `19/25` of a body a turn, while a bare-handed one deals **exactly zero** — so the kitted band
/// eats and the dry band takes **nothing at all**. That is §4.8 in one number: *"everything from a
/// gazelle upward is untouchable"* without spears, and *"without kit you are a trapper, not a
/// hunter"*.
///
/// **Both halves are asserted.** A bare-handed take of zero is only meaningful beside a kitted take
/// that is real, or the fixture could be measuring a herd nobody hunted.
#[test]
fn the_attack_tier_decides_the_take() {
    let (mut kitted, kitted_band) = hunting_world(BandEquipment::default());
    let (mut bare, bare_band) = hunting_world(dry_hunting());

    for _ in 0..RUN_TURNS {
        run_turn(&mut kitted);
        run_turn(&mut bare);
    }

    let kitted_row = exported(&kitted, kitted_band);
    let bare_row = exported(&bare, bare_band);

    // **The kitted band really hunted** — the liveness half, without which the zeros below prove
    // nothing.
    assert!(
        kitted_row.food_income > 0.0,
        "a spear-armed band must bring deer home (income={})",
        kitted_row.food_income
    );
    assert!(
        kit_of(&kitted, kitted_band).hunting_wear > 0.0,
        "the hunting kit is charged per animal killed, so a real take must have worn it"
    );

    // **The bare-handed band took nothing** — `attack 1` against `defense 1` is `max(0, 0)`, the hard
    // gate, so no animal ever went down however long the band hunted.
    assert_eq!(
        bare_row.food_income, 0.0,
        "a bare-handed band cannot take a Red Deer at all — the gate is exact, not merely small"
    );
    assert_eq!(
        kit_of(&bare, bare_band).hunting_wear,
        dry_hunting().hunting_wear,
        "no kills, so no hunting-kit wear beyond what the band started spent"
    );
    assert_eq!(
        kit_of(&bare, bare_band).carry_wear,
        0.0,
        "nothing was hauled, so the carry kit is untouched — wear tracks USE"
    );

    // The herd shows it from the other side: the kitted band drew it down, the bare one did not.
    assert!(
        herd_biomass(&kitted) < herd_biomass(&bare),
        "the take depends on the hunting kit: kitted left {} standing, bare left {}",
        herd_biomass(&kitted),
        herd_biomass(&bare)
    );

    // The tiers really are different on the wire, which is the number the gate is resolved through.
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
    // **And the gate cuts BOTH ways** — a property worth pinning here because it was not true one
    // slice ago. A Red Deer swings at `attack 0.8 × ferocity 0.15 = 0.12`, which does not clear a
    // human's `defense 1`, so it costs the party *nothing* — where the retired power-ratio model gave
    // every positive attack some casualties. Deer are now safe to hunt, and only the roster's three
    // real fighters (mammoth, aurochs, wolf) clear a person's defense at all.
    assert!(
        !hunt_danger_fired(&kitted) && !hunt_danger_fired(&bare),
        "a Red Deer cannot hurt a human: 0.8 × 0.15 = 0.12 is below `person.defense` 1, so the gate \
         gives it exactly zero — no casualties, no feed line"
    );
}

/// Did the hunt-danger path resolve casualties this run? Read off the command feed, which is the only
/// place a casualty-causing hunt announces itself.
fn hunt_danger_fired(app: &bevy::prelude::App) -> bool {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .any(|entry| entry.kind == CommandEventKind::HuntDanger)
}
