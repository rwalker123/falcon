//! **The minimal TOE — three kits, one job each, one cliff apiece**
//! (`docs/plan_hunt_through_combat.md` §4.8, `docs/plan_early_game_labor.md` → "Equipment / TOE").
//!
//! Pinned against **real turns through the real systems and the real snapshot export**, because the
//! three halves of this slice have deliberately different shapes and must not be allowed to hide in
//! one aggregate:
//!
//! - **Spears decide whether a take happens at all.** The kill resolves through
//!   `combat::resolve_fight`, so `max(0, attack − defense)` is the gate: a spear-armed band takes Red
//!   Deer and a bare-handed one takes **nothing** from them, at any headcount.
//! - **The sled decides how much of the kill comes home.** `hunt.per_worker_biomass_capacity` is live
//!   in every hunt take, so a sledless party genuinely hauls less — and, on a big enough body, leaves
//!   meat on the range as `wasted`.
//! - **Baskets decide how much a gatherer gathers.** Before §4.8's "one kit, one job" correction the
//!   forage web had no kit at all: the kit called *baskets* raised the *hunt's* haul, which is a
//!   physical nonsense (you drag a carcass; you do not put it in a container).
//!
//! **Every ordering assertion here is paired with a liveness assertion** on *both* sides of the tier,
//! never just "the equipped one is bigger". And the split itself is pinned by two cross-checks —
//! baskets must not touch the hunt, the sled must not touch foraging — because that is exactly the
//! confusion this slice corrects.

use bevy::{math::UVec2, prelude::Entity};
use core_sim::{
    available_workers, build_headless_app, run_turn, BandEquipment, CommandEventKind,
    CommandEventLog, CreaturesConfig, EffectTier, EquipmentConfig, EquipmentStat,
    FaunaConfigHandle, ForageRegistry, Herd, HerdRegistry, LaborAllocation, LaborAssignment,
    LaborConfig, LaborTarget, PopulationCohort, SimulationConfig, SizeClass, SnapshotHistory, Tile,
};
use sim_schema::state::PopulationCohortState;

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own. Same
/// seed as the other food-ledger integration tests, for no reason beyond reproducibility.
const SEED: u64 = 119_304_647;
/// **Red Deer** — a herbivore with `ferocity 0.15`, so the Phase-0 hunt-danger fight genuinely fires
/// without the aurochs-scale casualties that would churn the working-age bracket between turns.
const DEER: &str = "Red Deer";
const HERD_ID: &str = "toe_deer";
/// Standing stock far above anything a band can take, so the **escapement** never binds and the take
/// is decided by the crew — which is where the carry tier lives.
const HERD_BIOMASS: f32 = 6_000.0;
/// `Red Deer` body mass (`fauna_config.json`), the quantum the take is rounded to.
const DEER_BODY_MASS: f32 = 15.0;
const DEER_REGROWTH: f32 = 0.1;
const DEER_FODDER_PER_BIOMASS: f32 = 0.05;
/// **Wild Horses** — the *sled* half's quarry, and it has to be a different animal since slice 4.
///
/// §4.6's per-hunter-turn ceiling is `min(engage_rate, (attack − defense) / durability) × body_mass`,
/// and the sled tier only decides a take when that ceiling sits **above** the sledless rate (12) and
/// **below** the kitted one (40) — otherwise one bound or the other binds on both tiers and the two
/// hauls are identical. A Red Deer's ceiling is `min(1, 0.76) × 15 = 11.4`, under *both* rates; a
/// Wild Horse's is `min(0.5, 0.514) × 40 = 20.0`, squarely between them. That is the regime the sled
/// is a lever in, so that is where it is measured.
const HORSE: &str = "Wild Horses";
/// `Wild Horses` body mass (`fauna_config.json`).
const HORSE_BODY_MASS: f32 = 40.0;

/// **A body one small party can bring down but cannot carry** — the regime `AnimalTake::wasted` lives
/// in, and the one slice 4 made unreachable at the shipped tier (any crew that could make the kill
/// could carry it). Waste needs `workers × per_worker_carry < body_mass`, because
/// `quantise_animal_take`'s `max(1, carryable)` is the only site that kills more than it hauls. With
/// [`WASTE_CREW`] hunters: kitted collects `2 × 40 = 80 ≥ 50`, so nothing is left; sledless collects
/// `2 × 12 = 24 < 50`, so one body goes down and less than half of it comes home.
///
/// Seated on the `Red Deer` fight dials (the herd carries its own `body_mass`), so the *fight* is the
/// same on both sides and the only thing that varies is the haul.
const WASTE_BODY_MASS: f32 = 50.0;
/// Small enough that the sledless crew cannot seat one body, big enough that the fight still puts one
/// down (`2 hunters × (20 − 1) = 38` damage against a 25-durability body).
const WASTE_CREW: u32 = 2;

/// A shallow floor: the hunt is allowed to draw the herd down, so nothing about escapement is under
/// test here.
const SHALLOW_FLOOR: f32 = 0.05;
/// **Strip the patch** — the gather's floor, chosen so the *crew* is the binding term on both basket
/// tiers rather than the escapement ceiling, which is what makes the two tiers comparable.
const STRIP_THE_PATCH: f32 = 0.0;
/// Enough turns for wear to accumulate visibly, few enough that the herd cannot roam past the leash.
const RUN_TURNS: u32 = 4;
/// One turn is enough for every gather assertion here: the patch is drawn down as it is worked, so a
/// longer run measures regrowth rather than the crew's throughput.
const GATHER_TURNS: u32 = 1;
/// Exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
const EPSILON: f32 = 1e-3;

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/// A booted world with its one resident band hunting a fat deer herd standing on its camp.
/// `kit` is the band's starting wear, which is the only thing these tests vary.
fn hunting_world(kit: BandEquipment) -> (bevy::prelude::App, Entity) {
    hunting_world_of(DEER, DEER_BODY_MASS, None, kit)
}

/// [`hunting_world`] against a named quarry — the fight's dials are resolved off the species
/// (`FaunaConfig::quarry_fight_for`), so a test about which *bound* binds has to say which animal it
/// means. `crew` overrides the head-count where the binding term is `workers × per_worker_carry`.
fn hunting_world_of(
    species: &str,
    body_mass: f32,
    crew: Option<u32>,
    kit: BandEquipment,
) -> (bevy::prelude::App, Entity) {
    let (mut app, band, workers, band_pos) = booted_band();
    seat_quarry(&mut app, band_pos, species, body_mass);
    app.world.entity_mut(band).insert((
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: SHALLOW_FLOOR,
                },
                workers: crew.unwrap_or(workers).max(1),
                improvement: None,
                kit: None,
            }],
            ..Default::default()
        },
        kit,
    ));
    (app, band)
}

/// The same world with the band staffing **Forage** on a patch it is standing on or beside — the
/// basket kit's half of the split. No herd is seated at all, so nothing on this path can wear a sled.
fn gathering_world(kit: BandEquipment) -> (bevy::prelude::App, Entity) {
    let (mut app, band, workers, band_pos) = booted_band();
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .filter(|p| p.x.abs_diff(band_pos.x).max(p.y.abs_diff(band_pos.y)) <= 1)
        .min_by_key(|p| (p.y, p.x))
        .expect("the starting band must sit on or beside a forage patch");
    app.world.entity_mut(band).insert((
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: patch,
                    floor: STRIP_THE_PATCH,
                    species: None,
                },
                workers: workers.max(1),
                improvement: None,
                kit: None,
            }],
            ..Default::default()
        },
        kit,
    ));
    (app, band)
}

/// The same world with the band staffing **Scout** — it works every turn and marches, and neither
/// kills nor gathers. The control for "wear is charged for use, not for turns elapsed".
fn scouting_world(kit: BandEquipment) -> (bevy::prelude::App, Entity) {
    let (mut app, band) = hunting_world(kit);
    let workers = available_workers(app.world.get::<PopulationCohort>(band).unwrap().working);
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Scout,
            workers: workers.max(1),
            improvement: None,
            kit: None,
        }],
        ..Default::default()
    });
    (app, band)
}

/// `(app, band, available workers, band tile position)` — the shared opening of every fixture above.
fn booted_band() -> (bevy::prelude::App, Entity, u32, UVec2) {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    // **This suite pins the KIT, so the retreat stage is held at its identity.** Its hunt cases
    // compare two worlds that differ only in equipment; slice 7's authored `combat.wariness`
    // (`docs/plan_hunt_through_combat.md` §3.1) makes each world draw its own retreat, and a
    // "sledless hauls strictly less" ordering then turns on which side drew better. See
    // `FaunaConfig::without_retreat`.
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
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
    (app, band, workers, band_pos)
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

/// The exported `wasted_yield` of the band's one assignment — meat killed and left on the range for a
/// hunt, stock left standing for a gather.
fn exported_waste(app: &bevy::prelude::App, band: Entity) -> f32 {
    exported(app, band)
        .labor_assignments
        .first()
        .expect("the band has exactly one assignment")
        .wasted_yield
}

fn herd_biomass(app: &bevy::prelude::App) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the seeded herd")
        .biomass
}

fn kit_of(app: &bevy::prelude::App, band: Entity) -> BandEquipment {
    app.world
        .get::<BandEquipment>(band)
        .expect("band kit")
        .clone()
}

/// The shipped item ids. The sim resolves stats and quanta and never spells an item; a *test* has to
/// name one to drive it dry, so the names live here in one place.
const SPEARS: &str = "spears";
const SLED: &str = "sled";
const BASKETS: &str = "baskets";

/// A ledger with one item worn exactly to its limit — the first spent state, since an item is
/// equipped while its wear is *strictly below* `starting_durability`.
fn dry(item: &str) -> BandEquipment {
    let config = equipment();
    let durability = config
        .item(item)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"))
        .starting_durability;
    let mut ledger = BandEquipment::default();
    ledger.restore_wear(item, durability);
    ledger
}

fn equipment() -> std::sync::Arc<EquipmentConfig> {
    EquipmentConfig::builtin()
}

fn dry_sled() -> BandEquipment {
    dry(SLED)
}

fn dry_baskets() -> BandEquipment {
    dry(BASKETS)
}

fn dry_hunting() -> BandEquipment {
    dry(SPEARS)
}

// ---------------------------------------------------------------------------------------------
// The band starts stocked
// ---------------------------------------------------------------------------------------------

/// **Start-stocked, and the wire says so.** A freshly generated band carries a full kit on all three
/// roles and reports the *equipped* tier on all three axes — which is the shipped opening, unchanged
/// by this slice.
#[test]
fn a_fresh_band_is_kitted_and_publishes_all_three_equipped_tiers() {
    let (mut app, band, _, _) = booted_band();
    // The component is really there — `spawn_profile_population` must insert it, or every band's
    // wear would be silently discarded and the durability model would be inert.
    assert_eq!(
        app.world.get::<BandEquipment>(band).cloned(),
        Some(BandEquipment::default()),
        "a spawned band starts with an UNWORN kit"
    );

    run_turn(&mut app);
    let cohort = exported(&app, band);
    let equipment = equipment();
    let labor = LaborConfig::builtin();
    assert_eq!(
        published_condition(&cohort, SPEARS),
        equipment.item(SPEARS).expect("spears").starting_durability
    );
    assert_eq!(
        published_condition(&cohort, SLED),
        equipment.item(SLED).expect("sled").starting_durability
    );
    assert_eq!(
        published_condition(&cohort, BASKETS),
        equipment
            .item(BASKETS)
            .expect("baskets")
            .starting_durability
    );
    assert_eq!(
        cohort.hunter_attack,
        equipped_attack(&equipment),
        "a kitted hunter fights at the spear tier"
    );
    assert_eq!(
        cohort.hunt_carry_per_worker_biomass, labor.hunt.per_worker_biomass_capacity,
        "a sledded crew hauls at the shipped labor_config rate"
    );
    assert_eq!(
        cohort.forage_carry_per_worker_biomass, labor.forage.per_worker_biomass_capacity,
        "a basket-carrying crew gathers at the shipped labor_config rate"
    );
}

// ---------------------------------------------------------------------------------------------
// The sled — the HUNT's carry
// ---------------------------------------------------------------------------------------------

/// **Both hunt-carry tiers are reachable AND productive, and the sledless one hauls less.** The
/// liveness half matters as much as the ordering: a sledless band must still hunt (you can always
/// drag *something*), so a zero take on either side would pass a naive "equipped > dry" check while
/// meaning the model broke.
#[test]
fn both_hunt_carry_tiers_are_live_and_a_sledless_party_hauls_less() {
    let (mut kitted, kitted_band) =
        hunting_world_of(HORSE, HORSE_BODY_MASS, None, BandEquipment::default());
    let (mut dry, dry_band) = hunting_world_of(HORSE, HORSE_BODY_MASS, None, dry_sled());

    run_turn(&mut kitted);
    run_turn(&mut dry);

    let kitted_row = exported(&kitted, kitted_band);
    let dry_row = exported(&dry, dry_band);
    // **The biomass actually hauled home**, read off the sled's own wear — `sled_wear` is charged
    // exactly `wear_per_biomass_hauled × carried`, so it inverts to the haul with no arithmetic of
    // our own. (The herd's raw biomass delta cannot be used: its ecological `K` is recomputed from
    // the range every turn, so most of the movement is the clamp, not the hunt.)
    //
    // **The trick works on the KITTED arm only, and that is a property of the model rather than of
    // the fixture.** Since kit selection, wear is gated on the *same* effective predicate that chose
    // the tier (`equipment.md` → "Wear rides the SAME predicate"), so a band whose sled is spent is
    // dragging by hand and is charged nothing more: its `sled_wear` sits at the limit forever. The
    // dry arm's haul is therefore read off `food_income`, which is `carried × provisions_per_biomass
    // × output_multiplier` — strictly monotone in the haul, so the ordering below is the same claim.
    let per_biomass = equipment().item(SLED).expect("sled").wear.amount;
    let kitted_haul = kit_of(&kitted, kitted_band).wear_of(SLED) / per_biomass;

    assert!(
        kitted_haul > 0.0 && kitted_row.food_income > 0.0,
        "the equipped tier must actually bring game home (haul={kitted_haul}, income={})",
        kitted_row.food_income
    );
    assert!(
        dry_row.food_income > 0.0,
        "the UNEQUIPPED tier must STILL bring game home — a sledless band is a worse hauler, not a \
         non-hunter (income={})",
        dry_row.food_income
    );
    assert_eq!(
        kit_of(&dry, dry_band).wear_of(SLED),
        dry_sled().wear_of(SLED),
        "a spent sled is not dragged, so it is not charged either — the wear gate is the same \
         predicate that put this band on the unequipped tier"
    );
    assert!(
        dry_row.food_income < kitted_row.food_income,
        "a sledless party hauls strictly less, and the player sees it as less food: dry={} vs \
         kitted={}",
        dry_row.food_income,
        kitted_row.food_income
    );

    // ...and the wire carries both tiers explicitly, so the client never re-derives them.
    let equipment = equipment();
    assert_eq!(
        kitted_row.hunt_carry_per_worker_biomass,
        LaborConfig::builtin().hunt.per_worker_biomass_capacity
    );
    assert_eq!(
        dry_row.hunt_carry_per_worker_biomass,
        unequipped_carry(&equipment, SLED)
    );
    assert!(published_condition(&kitted_row, SLED) > 0.0);
    assert_eq!(
        published_condition(&dry_row, SLED),
        0.0,
        "a spent kit reads exactly 0 remaining, never negative"
    );
}

/// **A party that cannot haul its kill leaves the rest on the range — and that is the sledless case's
/// whole mechanic** (§4.8: "the sledless hunt needs no new mechanic"). `AnimalTake::wasted` has always
/// expressed it and the client already displays it; what slice 4 did was make it *unreachable* at the
/// shipped tier, because any crew that could make the kill could also carry it. A lower sledless carry
/// puts it back.
///
/// Asserted on **both** sides: the sledless party wastes something, and the equipped party on the
/// *same* quarry with the *same* crew wastes nothing — so the reading is about the sled and not about
/// the fixture being too small in general. Both must still have killed, or "wasted 0" would be the
/// trivial truth about a party that never engaged.
#[test]
fn a_sledless_party_wastes_the_kill_it_cannot_carry() {
    let (mut kitted, kitted_band) = hunting_world_of(
        DEER,
        WASTE_BODY_MASS,
        Some(WASTE_CREW),
        BandEquipment::default(),
    );
    let (mut dry, dry_band) = hunting_world_of(DEER, WASTE_BODY_MASS, Some(WASTE_CREW), dry_sled());

    run_turn(&mut kitted);
    run_turn(&mut dry);

    // Liveness on both sides: each party actually put a body on the ground this turn. Read off the
    // hunting kit, which is charged per animal killed.
    assert!(
        kit_of(&kitted, kitted_band).wear_of(SPEARS) > 0.0,
        "the equipped party must have killed, or 'it wasted nothing' is vacuous"
    );
    assert!(
        kit_of(&dry, dry_band).wear_of(SPEARS) > 0.0,
        "the sledless party must have killed, or its waste is not a haul failure"
    );

    let kitted_waste = exported_waste(&kitted, kitted_band);
    let dry_waste = exported_waste(&dry, dry_band);
    assert_eq!(
        kitted_waste, 0.0,
        "a sledded party seats the whole body: nothing should be left on the range (got \
         {kitted_waste})"
    );
    assert!(
        dry_waste > 0.0,
        "a sledless party kills more than it hauls — the meat it leaves is the shortfall's whole \
         mechanic (got {dry_waste})"
    );
}

// ---------------------------------------------------------------------------------------------
// Baskets — the FORAGE web's carry
// ---------------------------------------------------------------------------------------------

/// **Both gather tiers are reachable AND productive, and bare hands gather far less.** The forage
/// web's twin of the sled test above, and the half that did not exist before §4.8: `baskets` used to
/// raise the hunt's haul while `forage.per_worker_biomass_capacity` sat untouched by any kit.
#[test]
fn both_gather_tiers_are_live_and_bare_hands_gather_less() {
    let (mut kitted, kitted_band) = gathering_world(BandEquipment::default());
    let (mut bare, bare_band) = gathering_world(dry_baskets());

    for _ in 0..GATHER_TURNS {
        run_turn(&mut kitted);
        run_turn(&mut bare);
    }

    let kitted_row = exported(&kitted, kitted_band);
    let bare_row = exported(&bare, bare_band);
    // The biomass actually gathered, inverted from the basket kit's own wear — the same trick the
    // sled test uses, with the same restriction to the kitted arm and for the same reason: a crew
    // with no baskets left is gathering by hand and is charged nothing more.
    let per_biomass = equipment().item(BASKETS).expect("baskets").wear.amount;
    let kitted_take = kit_of(&kitted, kitted_band).wear_of(BASKETS) / per_biomass;

    assert!(
        kitted_take > 0.0 && kitted_row.food_income > 0.0,
        "the equipped tier must actually bring food home (take={kitted_take}, income={})",
        kitted_row.food_income
    );
    assert!(
        bare_row.food_income > 0.0,
        "the UNEQUIPPED tier must STILL gather — bare hands is a handful, not nothing (income={})",
        bare_row.food_income
    );
    assert_eq!(
        kit_of(&bare, bare_band).wear_of(BASKETS),
        dry_baskets().wear_of(BASKETS),
        "spent baskets are not carried, so they are not charged either"
    );
    assert!(
        bare_row.food_income < kitted_row.food_income,
        "a bare-handed crew gathers strictly less, and the player sees it as less food: bare={} vs \
         kitted={}",
        bare_row.food_income,
        kitted_row.food_income
    );

    // ...and the wire carries the forage tier as its OWN field. A client that read the hunt's number
    // here would be repeating the very defect §4.8 corrected.
    let equipment = equipment();
    assert_eq!(
        kitted_row.forage_carry_per_worker_biomass,
        LaborConfig::builtin().forage.per_worker_biomass_capacity
    );
    assert_eq!(
        bare_row.forage_carry_per_worker_biomass,
        unequipped_carry(&equipment, BASKETS)
    );
    assert!(published_condition(&kitted_row, BASKETS) > 0.0);
    assert_eq!(
        published_condition(&bare_row, BASKETS),
        0.0,
        "a spent kit reads exactly 0 remaining, never negative"
    );
}

// ---------------------------------------------------------------------------------------------
// One kit, one job — the cross-checks
// ---------------------------------------------------------------------------------------------

/// **Baskets do not touch the hunt.** The cross-check that would have caught the original defect: a
/// band whose baskets are gone must hunt *exactly* as a fully-kitted one, because dragging a carcass
/// is a transport problem and no container helps.
///
/// Liveness on both sides: the shared take is non-zero (or "identical" is a statement about two
/// zeros), and the basket tier really is live on the wire in the same frame — so this cannot pass by
/// the basket kit having quietly stopped working altogether.
#[test]
fn baskets_do_not_touch_the_hunt() {
    let (mut kitted, kitted_band) =
        hunting_world_of(HORSE, HORSE_BODY_MASS, None, BandEquipment::default());
    let (mut basketless, basketless_band) =
        hunting_world_of(HORSE, HORSE_BODY_MASS, None, dry_baskets());

    run_turn(&mut kitted);
    run_turn(&mut basketless);

    let kitted_row = exported(&kitted, kitted_band);
    let basketless_row = exported(&basketless, basketless_band);
    assert!(
        kitted_row.food_income > 0.0,
        "the hunt must pay for this comparison to mean anything"
    );
    assert!(
        basketless_row.forage_carry_per_worker_biomass < kitted_row.forage_carry_per_worker_biomass,
        "the basketless band must really be on the lower GATHER tier in this same frame, or the \
         equality below is vacuous"
    );
    assert_eq!(
        basketless_row.hunt_carry_per_worker_biomass, kitted_row.hunt_carry_per_worker_biomass,
        "a dry basket must not move the hunt's haul rate"
    );
    assert!(
        (basketless_row.food_income - kitted_row.food_income).abs() < EPSILON,
        "a dry basket must not change what a hunt brings home: {} vs {}",
        basketless_row.food_income,
        kitted_row.food_income
    );
    assert_eq!(
        kit_of(&basketless, basketless_band).wear_of(BASKETS),
        dry_baskets().wear_of(BASKETS),
        "a hunting turn gathers nothing, so it charges no basket wear at all"
    );
}

/// **The sled does not touch foraging.** The mirror of the test above: a band with no sled must gather
/// exactly as a fully-kitted one, because a drag harness does not help you hold more berries.
#[test]
fn the_sled_does_not_touch_foraging() {
    let (mut kitted, kitted_band) = gathering_world(BandEquipment::default());
    let (mut sledless, sledless_band) = gathering_world(dry_sled());

    for _ in 0..GATHER_TURNS {
        run_turn(&mut kitted);
        run_turn(&mut sledless);
    }

    let kitted_row = exported(&kitted, kitted_band);
    let sledless_row = exported(&sledless, sledless_band);
    assert!(
        kitted_row.food_income > 0.0,
        "the gather must pay for this comparison to mean anything"
    );
    assert!(
        sledless_row.hunt_carry_per_worker_biomass < kitted_row.hunt_carry_per_worker_biomass,
        "the sledless band must really be on the lower HUNT tier in this same frame, or the \
         equality below is vacuous"
    );
    assert_eq!(
        sledless_row.forage_carry_per_worker_biomass, kitted_row.forage_carry_per_worker_biomass,
        "a dry sled must not move the gather rate"
    );
    assert!(
        (sledless_row.food_income - kitted_row.food_income).abs() < EPSILON,
        "a dry sled must not change what a gather brings home: {} vs {}",
        sledless_row.food_income,
        kitted_row.food_income
    );
    assert_eq!(
        kit_of(&sledless, sledless_band).wear_of(SLED),
        dry_sled().wear_of(SLED),
        "a gathering turn hauls no carcass, so it charges no sled wear at all"
    );
}

/// **Each kit wears on its OWN quantum, and only on it** (`docs/plan_denial_raid.md` §1.2 — wear
/// tracks USE). A band that hunts but does not gather must finish the run with whole baskets, and one
/// that gathers but does not hunt with a whole sled. Both directions, with the used kit's wear
/// asserted positive so the untouched one is a real absence rather than a dead system.
#[test]
fn the_sled_and_the_baskets_wear_on_different_quanta() {
    let (mut hunting, hunting_band) = hunting_world(BandEquipment::default());
    let (mut gathering, gathering_band) = gathering_world(BandEquipment::default());
    for _ in 0..GATHER_TURNS {
        run_turn(&mut hunting);
        run_turn(&mut gathering);
    }

    let hunted = kit_of(&hunting, hunting_band);
    let gathered = kit_of(&gathering, gathering_band);
    assert!(
        hunted.wear_of(SLED) > 0.0,
        "a hunting band drags carcasses home, so its sled must wear: {hunted:?}"
    );
    assert_eq!(
        hunted.wear_of(BASKETS),
        0.0,
        "...and it gathered nothing, so its baskets are untouched: {hunted:?}"
    );
    assert!(
        gathered.wear_of(BASKETS) > 0.0,
        "a gathering band fills baskets, so they must wear: {gathered:?}"
    );
    assert_eq!(
        gathered.wear_of(SLED),
        0.0,
        "...and it hauled no carcass, so its sled is untouched: {gathered:?}"
    );
    assert_eq!(
        gathered.wear_of(SPEARS),
        0.0,
        "a gathering band killed nothing, so its spears are untouched: {gathered:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The cliff, per kit
// ---------------------------------------------------------------------------------------------

/// **The cliff is a CLIFF, on every kit independently.** Performance is *flat* right up to expiry and
/// then steps down — a gradual taper would pass a "kit matters" test and is the wrong model
/// (durability and performance are orthogonal axes). Asserted on the **exported** tiers across a wear
/// sweep, on all three kits at once.
#[test]
fn the_durability_cliff_is_a_step_not_a_taper() {
    let equipment = equipment();
    let hunting_limit = equipment.item(SPEARS).expect("spears").starting_durability;
    let sled_limit = equipment.item(SLED).expect("sled").starting_durability;
    let basket_limit = equipment
        .item(BASKETS)
        .expect("baskets")
        .starting_durability;
    // Fractions of each kit's life, from brand new to one hair short of spent.
    let almost_spent = [0.0, 0.25, 0.5, 0.75, 0.999];

    let mut hunt_rates = Vec::new();
    let mut gather_rates = Vec::new();
    let mut attacks = Vec::new();
    for fraction in almost_spent {
        // **A SCOUTING band, deliberately.** The sweep is about the *shape* of the tier function, so
        // the turn must not itself add wear — a working turn at the 0.999 sample would tip a kit over
        // its limit and the sweep would measure the cliff instead of the flat.
        let (mut app, band) = scouting_world(worn(&[
            (SPEARS, hunting_limit * fraction),
            (SLED, sled_limit * fraction),
            (BASKETS, basket_limit * fraction),
        ]));
        run_turn(&mut app);
        let row = exported(&app, band);
        hunt_rates.push(row.hunt_carry_per_worker_biomass);
        gather_rates.push(row.forage_carry_per_worker_biomass);
        attacks.push(row.hunter_attack);
    }
    // Flat — every reading below expiry is the SAME number, not a decreasing series.
    for (i, fraction) in almost_spent.iter().enumerate() {
        assert_eq!(
            hunt_rates[i], hunt_rates[0],
            "the hunt's carry must be flat until expiry (wear fraction {fraction})"
        );
        assert_eq!(
            gather_rates[i], gather_rates[0],
            "the gather's carry must be flat until expiry (wear fraction {fraction})"
        );
        assert_eq!(
            attacks[i], attacks[0],
            "attack must be flat until expiry (wear fraction {fraction})"
        );
    }

    // ...then one step down at the limit, on all three axes.
    let (mut spent_app, spent_band) = scouting_world(worn(&[
        (SPEARS, hunting_limit),
        (SLED, sled_limit),
        (BASKETS, basket_limit),
    ]));
    run_turn(&mut spent_app);
    let spent = exported(&spent_app, spent_band);
    assert!(
        spent.hunt_carry_per_worker_biomass < hunt_rates[0],
        "the hunt's carry must STEP DOWN at expiry: {} -> {}",
        hunt_rates[0],
        spent.hunt_carry_per_worker_biomass
    );
    assert!(
        spent.forage_carry_per_worker_biomass < gather_rates[0],
        "the gather's carry must STEP DOWN at expiry: {} -> {}",
        gather_rates[0],
        spent.forage_carry_per_worker_biomass
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
        spent.hunt_carry_per_worker_biomass,
        unequipped_carry(&equipment, SLED),
        "the step lands exactly on the sledless tier"
    );
    assert_eq!(
        spent.forage_carry_per_worker_biomass,
        unequipped_carry(&equipment, BASKETS),
        "the step lands exactly on the bare-handed gather tier"
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
        hunted.wear_of(SPEARS) > 0.0 && hunted.wear_of(SLED) > 0.0,
        "a hunting band must wear its spears AND its sled over {RUN_TURNS} turns (got {hunted:?})"
    );
    // The discriminating half: identical turn count, zero kills, zero wear — on ALL THREE kits.
    assert_eq!(
        scouted,
        BandEquipment::default(),
        "a band that killed and gathered nothing for {RUN_TURNS} turns must lose no durability at all"
    );

    // ...and the hunting kit's wear is an exact whole number of kills, never a per-turn drip.
    let per_kill = equipment().item(SPEARS).expect("spears").wear.amount;
    let kills = hunted.wear_of(SPEARS) / per_kill;
    assert!(
        (kills - kills.round()).abs() < EPSILON,
        "hunting wear must be an exact multiple of wear_per_kill ({per_kill}): {} => {kills} kills",
        hunted.wear_of(SPEARS)
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
    let sled_limit = equipment.item(SLED).expect("sled").starting_durability;
    // One deer's worth of sled life left: this run crosses the cliff mid-flight.
    let almost = sled_limit - equipment.item(SLED).expect("sled").wear.amount * DEER_BODY_MASS;
    let (mut app, band) = hunting_world(worn(&[(SLED, almost)]));

    let mut wear_series = Vec::new();
    let mut rate_series = Vec::new();
    for _ in 0..RUN_TURNS * 2 {
        run_turn(&mut app);
        wear_series.push(kit_of(&app, band).wear_of(SLED));
        rate_series.push(exported(&app, band).hunt_carry_per_worker_biomass);
    }

    // Monotonic: nothing in this slice ever gives condition back.
    for pair in wear_series.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "wear must never decrease (no replenishment exists): {pair:?}"
        );
    }
    // The cliff was actually crossed (liveness), and once crossed it is absorbing.
    let unequipped = unequipped_carry(&equipment, SLED);
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
        wear_series.last().copied().unwrap_or_default() >= sled_limit,
        "the kit ends the run spent"
    );
}

/// The basket twin of [`a_kit_run_dry_stays_dry`] — the gather's cliff is crossed by *gathering*, on
/// its own quantum, and is absorbing in the same way.
#[test]
fn baskets_run_dry_on_their_own_quantum_and_stay_dry() {
    let equipment = equipment();
    let basket_limit = equipment
        .item(BASKETS)
        .expect("baskets")
        .starting_durability;
    // A single turn's gathering is enough to tip this over: the crew's throughput is
    // `workers × 8`, and one unit of biomass costs `wear_per_biomass_gathered`.
    let almost = basket_limit - equipment.item(BASKETS).expect("baskets").wear.amount;
    let (mut app, band) = gathering_world(worn(&[(BASKETS, almost)]));

    let mut wear_series = Vec::new();
    let mut rate_series = Vec::new();
    for _ in 0..RUN_TURNS {
        run_turn(&mut app);
        wear_series.push(kit_of(&app, band).wear_of(BASKETS));
        rate_series.push(exported(&app, band).forage_carry_per_worker_biomass);
    }

    for pair in wear_series.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "wear must never decrease (no replenishment exists): {pair:?}"
        );
    }
    let unequipped = unequipped_carry(&equipment, BASKETS);
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
        wear_series.last().copied().unwrap_or_default() >= basket_limit,
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
        kit_of(&kitted, kitted_band).wear_of(SPEARS) > 0.0,
        "the hunting kit is charged per animal killed, so a real take must have worn it"
    );

    // **The bare-handed band took nothing** — `attack 1` against `defense 1` is `max(0, 0)`, the hard
    // gate, so no animal ever went down however long the band hunted.
    assert_eq!(
        bare_row.food_income, 0.0,
        "a bare-handed band cannot take a Red Deer at all — the gate is exact, not merely small"
    );
    assert_eq!(
        kit_of(&bare, bare_band).wear_of(SPEARS),
        dry_hunting().wear_of(SPEARS),
        "no kills, so no hunting-kit wear beyond what the band started spent"
    );
    assert_eq!(
        kit_of(&bare, bare_band).wear_of(SLED),
        0.0,
        "nothing was hauled, so the sled is untouched — wear tracks USE"
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
    assert_eq!(kitted_row.hunter_attack, equipped_attack(&equipment));
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

/// The **equipped** `attack` spears grant — read off the item's effect list rather than a named
/// config field, so the test follows the model instead of restating it.
fn equipped_attack(config: &EquipmentConfig) -> f32 {
    match config
        .item(SPEARS)
        .and_then(|item| item.effect(EquipmentStat::Attack))
    {
        Some(EffectTier::Equipped(value)) => value,
        other => panic!("spears must declare an equipped attack, got {other:?}"),
    }
}

/// The **unequipped** carry rate a carry item declares — the tier a party without it falls back to.
fn unequipped_carry(config: &EquipmentConfig, item: &str) -> f32 {
    let stat = if item == SLED {
        EquipmentStat::HuntCarry
    } else {
        EquipmentStat::ForageCarry
    };
    match config.item(item).and_then(|def| def.effect(stat)) {
        Some(EffectTier::Unequipped(value)) => value,
        other => panic!("'{item}' must declare an unequipped {stat:?}, got {other:?}"),
    }
}

/// **One item's published remaining condition**, pulled out of the cohort's `kit_item_conditions`
/// list by id.
///
/// The wire used to carry three fixed floats (`huntingKitDurability` and friends); it now carries a
/// row per item, because the item table is config and a fixed field set could not have carried the
/// trapping kit's `traps`. A missing row is a **failure**, not a zero — a zero reads as *dry*, which
/// is a real and very different state from *the server never published this item*.
fn published_condition(cohort: &PopulationCohortState, item: &str) -> f32 {
    cohort
        .kit_item_conditions
        .iter()
        .find(|row| row.item_id == item)
        .unwrap_or_else(|| {
            panic!("the wire must publish a condition row for '{item}'; a missing row would read as dry")
        })
        .remaining
}

/// A ledger with the given wear already spent on each named item — the fixture's way of standing a
/// band at an arbitrary point on its durability curve.
fn worn(entries: &[(&str, f32)]) -> BandEquipment {
    let mut ledger = BandEquipment::default();
    for (item, wear) in entries {
        ledger.restore_wear(item, *wear);
    }
    ledger
}
