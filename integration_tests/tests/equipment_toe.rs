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
    LaborConfig, LaborTarget, MaterialsConfig, PopulationCohort, RecipesConfig, SimulationConfig,
    SizeClass, SnapshotHistory, Tile,
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
/// The scouting TOE — the one item a band that neither hunts nor gathers can still spend.
const WAYFINDING: &str = "wayfinding";
/// The warrior TOE.
const CLUBS: &str = "clubs";
/// The husbandry TOE — hurdles, halters, vessels.
const HUSBANDRY_GEAR: &str = "husbandry_gear";
/// The passive device — snares, nets, weirs, one item.
const THE_PASSIVE_DEVICE: &str = "traps";

/// **How many workers every fixture ledger in this file is outfitted for.**
///
/// A spawn stocks a *party's worth* of each item (`equipment.md` → "the partly-equipped party"), and
/// a fixture that stocked one unit would hand its band one armed hunter and the rest bare hands —
/// which is a test about running short of units, not about the tier it means to measure. Comfortably
/// above the largest crew any world here staffs, so coverage always reaches the whole party.
const OUTFITTED_PARTY_WORKERS: f32 = 64.0;

/// **A fully outfitted ledger** — what "the band is kitted" means in this file.
fn outfitted() -> BandEquipment {
    BandEquipment::start_stocked_for(&equipment(), OUTFITTED_PARTY_WORKERS)
}

/// **An outfitted ledger holding only `units` of `item`** — a band short of exactly one thing, which
/// is the state `EquipmentConfig::coverage` divides a crew over.
fn short_of(item: &str, units: u32) -> BandEquipment {
    let config = equipment();
    let mut ledger = outfitted();
    ledger.restore_batches(item, Vec::new());
    let tier = config
        .item(item)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"))
        .default_tier()
        .id
        .clone();
    ledger.stock(item, units, &tier, None);
    ledger
}

/// An outfitted ledger with one item **used up** — the first spent state, reached the only way
/// the sim reaches it: by charging the item's own quantum until nothing is left.
fn dry(item: &str) -> BandEquipment {
    let config = equipment();
    let mut ledger = outfitted();
    run_dry(&mut ledger, &config, item);
    ledger
}

/// **Run an item out, by USING it.** Charges the item's own use quantum a batch at a time until the
/// band owns no unit of it with condition left.
fn run_dry(ledger: &mut BandEquipment, config: &EquipmentConfig, item: &str) {
    let def = config
        .item(item)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"));
    let uses = def.default_tier().starting_durability / def.headline_wear().amount;
    while ledger.remaining(item, config) > 0.0 {
        ledger.wear_item(
            config,
            item,
            config
                .item(item)
                .expect("the fixture names a roster item")
                .headline_wear()
                .per,
            uses,
        );
    }
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
    let (mut app, band, workers, _) = booted_band();
    // The component is really there — `spawn_profile_population` must insert it, or every band's
    // wear would be silently discarded and the durability model would be inert.
    // **`start_stocked_owned`, not `start_stocked`** — a spawn stamps each batch with the grade a
    // bare-handed craft of that item comes out at, so the ledger names a quality rather than leaving
    // a blank the panel cannot tell from a missing chip. Counts, tiers and wear are the shipped
    // opening's, unchanged; only the label is new.
    //
    // **Sized to the band's own workers.** A spawn stocks `ceil(workers × start_stock_fraction)`
    // units of each item, so the ledger is only reproducible from the head count the band actually
    // has — which is also what the coverage assertion below turns on.
    assert_eq!(
        app.world.get::<BandEquipment>(band).cloned(),
        Some(BandEquipment::start_stocked_owned(
            &EquipmentConfig::builtin(),
            &RecipesConfig::builtin(),
            &MaterialsConfig::builtin(),
            workers as f32,
        )),
        "a spawned band starts with an UNWORN kit"
    );

    run_turn(&mut app);
    let cohort = exported(&app, band);
    let equipment = equipment();

    assert_eq!(
        published_condition(&cohort, SPEARS),
        equipment
            .item(SPEARS)
            .expect("spears")
            .default_tier()
            .starting_durability
    );
    assert_eq!(
        published_condition(&cohort, SLED),
        equipment
            .item(SLED)
            .expect("sled")
            .default_tier()
            .starting_durability
    );
    assert_eq!(
        published_condition(&cohort, BASKETS),
        equipment
            .item(BASKETS)
            .expect("baskets")
            .default_tier()
            .starting_durability
    );
    assert_eq!(
        cohort.hunter_attack,
        equipped_attack(&equipment),
        "a kitted hunter fights at the spear tier"
    );
    assert_eq!(
        cohort.hunt_carry_per_worker_biomass,
        equipped_carry(&equipment, SLED),
        "a sledded crew hauls at the sled tier's own rate"
    );
    assert_eq!(
        cohort.forage_carry_per_worker_biomass,
        equipped_carry(&equipment, BASKETS),
        "a basket-carrying crew gathers at the baskets' own tier"
    );
}

/// **A SPAWNED BAND IS ARMED TO ITS LAST WORKER, AND HAS A RESERVE BEHIND THEM** (issue #520).
///
/// A unit of an item arms `workers_per_unit` people, so a spawn that stocked one unit would send the
/// shipped band out as **one** armed hunter and sixteen bare hands. The stock is
/// `ceil(workers × start_stock_fraction)` per item, and this asserts both halves of what that buys:
/// coverage is **uniform** at the band's full head count (nobody is short), and the surplus over the
/// head count is the opening reserve — what the first break spends instead of disarming someone.
#[test]
fn a_spawned_band_arms_every_worker_and_keeps_a_reserve() {
    let (app, band, workers, _) = booted_band();
    let equipment = equipment();
    let ledger = app
        .world
        .get::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger");

    assert!(workers > 1, "the shipped band is a party, not one person");
    for kit in equipment.kits() {
        let choice = equipment
            .kit(&kit.id)
            .expect("the roster resolves its own entry");
        let coverage = equipment.coverage(&choice, workers as f32, ledger);
        assert!(
            coverage.is_uniform(),
            "every worker on the `{}` kit holds the same thing at spawn: {coverage:?}",
            kit.id
        );
        for item in choice.uses() {
            assert_eq!(
                coverage.workers_holding(item),
                workers as f32,
                "`{item}` must reach all {workers} of the band's workers"
            );
        }
    }
    // The RESERVE — strictly more units than people, which is what `start_stock_fraction`'s
    // half-again buys: the first retirement comes out of stock rather than out of the line.
    assert!(
        ledger.live_units(SPEARS, &equipment) > workers,
        "a spawn stocks a reserve above the head count ({} units for {workers} workers)",
        ledger.live_units(SPEARS, &equipment)
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
    let (mut kitted, kitted_band) = hunting_world_of(HORSE, HORSE_BODY_MASS, None, outfitted());
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
    let per_biomass = equipment().item(SLED).expect("sled").headline_wear().amount;
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
        equipped_carry(&equipment, SLED)
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
    let (mut kitted, kitted_band) =
        hunting_world_of(DEER, WASTE_BODY_MASS, Some(WASTE_CREW), outfitted());
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
    let (mut kitted, kitted_band) = gathering_world(outfitted());
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
    let per_biomass = equipment()
        .item(BASKETS)
        .expect("baskets")
        .headline_wear()
        .amount;
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
        equipped_carry(&equipment, BASKETS)
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

/// **A BAND SHORT OF BASKETS GATHERS WITH THE BASKETS IT HAS** (issue #520) — strictly more than
/// bare hands and strictly less than a fully-basketed crew.
///
/// The plant web's half of *"gear covers people, not jobs"*: `advance_labor_allocation` divides the
/// gatherers by the baskets the band holds (`EquipmentConfig::coverage`) and pays the crew-weighted
/// mean of the two tiers, exactly as the hunt does with sleds. Before this, five baskets across
/// sixteen gatherers paid sixteen basketfuls.
#[test]
fn a_band_short_of_baskets_gathers_between_the_bare_and_the_basketed() {
    // Few enough that most of the shipped band's gatherers go without, so the middle reading cannot
    // round into either end.
    const BASKETS_OWNED: u32 = 2;

    let (mut kitted, kitted_band) = gathering_world(outfitted());
    let (mut partly, partly_band) = gathering_world(short_of(BASKETS, BASKETS_OWNED));
    let (mut bare, bare_band) = gathering_world(dry_baskets());

    for _ in 0..GATHER_TURNS {
        run_turn(&mut kitted);
        run_turn(&mut partly);
        run_turn(&mut bare);
    }

    let kitted_income = exported(&kitted, kitted_band).food_income;
    let partly_income = exported(&partly, partly_band).food_income;
    let bare_income = exported(&bare, bare_band).food_income;

    assert!(
        bare_income > 0.0,
        "bare hands are a handful, not nothing — or every ordering below is about zero"
    );
    assert!(
        bare_income < partly_income,
        "two baskets must beat none: bare={bare_income} vs partly={partly_income}"
    );
    assert!(
        partly_income < kitted_income,
        "...and two baskets must not gather like a full set: partly={partly_income} vs \
         kitted={kitted_income}"
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
    let (mut kitted, kitted_band) = hunting_world_of(HORSE, HORSE_BODY_MASS, None, outfitted());
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
    let (mut kitted, kitted_band) = gathering_world(outfitted());
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
    let (mut hunting, hunting_band) = hunting_world(outfitted());
    let (mut gathering, gathering_band) = gathering_world(outfitted());
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
    let hunting_limit = equipment
        .item(SPEARS)
        .expect("spears")
        .default_tier()
        .starting_durability;
    let sled_limit = equipment
        .item(SLED)
        .expect("sled")
        .default_tier()
        .starting_durability;
    let basket_limit = equipment
        .item(BASKETS)
        .expect("baskets")
        .default_tier()
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

/// **A party that swings at nothing loses no durability, however long it works.**
/// `plan_denial_raid.md` §1.2 depends on this: a turn-based clock would charge an idle march the same
/// as a slaughter, which would make denial free. Same world, same turn count, different *work*.
#[test]
fn wear_is_charged_for_work_not_for_turns_elapsed() {
    let (mut hunting, hunting_band) = hunting_world(outfitted());
    let (mut scouting, scouting_band) = scouting_world(outfitted());
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
    // The discriminating half: identical turn count, zero kills, zero wear on every item whose
    // quantum is a KILL or a BIOMASS.
    //
    // **It is no longer `outfitted()`, and what changed is honest rather than a
    // loosening.** A scouting band now carries wayfinding gear, whose quantum is *ground revealed
    // for the first time* — so a band that is out scouting really is using something up, and a
    // ledger that stayed empty would mean the scouting kit was inert. What §1.2 forbids is a TURN
    // clock, and this asserts exactly that: the three items a hunt and a gather wear are untouched
    // over the same span, and the one item that moved moved on work that was actually done.
    for item in [SPEARS, SLED, BASKETS] {
        assert_eq!(
            scouted.wear_of(item),
            0.0,
            "a band that killed and gathered nothing for {RUN_TURNS} turns must not wear {item} \
             (got {scouted:?})"
        );
    }
    assert!(
        scouted.wear_of(WAYFINDING) > 0.0,
        "...and its wayfinding gear must actually be paying for the ground it uncovered, or the \
         scouting kit is inert and the clause above asserts nothing (got {scouted:?})"
    );

    // ...and the hunting kit's wear is an exact whole number of kills, never a per-turn drip.
    let per_kill = equipment()
        .item(SPEARS)
        .expect("spears")
        .headline_wear()
        .amount;
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
    let sled_limit = equipment
        .item(SLED)
        .expect("sled")
        .default_tier()
        .starting_durability;
    // One deer's worth of sled life left: this run crosses the cliff mid-flight.
    let almost =
        sled_limit - equipment.item(SLED).expect("sled").headline_wear().amount * DEER_BODY_MASS;
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
    assert_eq!(
        wear_series.last().copied().unwrap_or_default(),
        0.0,
        "the kit ends the run spent — the last batch is gone, so there is no condition left to read"
    );
    let _ = sled_limit;
}

/// The basket twin of [`a_kit_run_dry_stays_dry`] — the gather's cliff is crossed by *gathering*, on
/// its own quantum, and is absorbing in the same way.
#[test]
fn baskets_run_dry_on_their_own_quantum_and_stay_dry() {
    let equipment = equipment();
    let basket_limit = equipment
        .item(BASKETS)
        .expect("baskets")
        .default_tier()
        .starting_durability;
    // A single turn's gathering is enough to tip this over: the crew's throughput is
    // `workers × 8`, and one unit of biomass costs `wear_per_biomass_gathered`.
    let almost = basket_limit
        - equipment
            .item(BASKETS)
            .expect("baskets")
            .headline_wear()
            .amount;
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
    assert_eq!(
        wear_series.last().copied().unwrap_or_default(),
        0.0,
        "the kit ends the run spent — the last batch is gone, so there is no condition left to read"
    );
    let _ = basket_limit;
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
    let (mut kitted, kitted_band) = hunting_world(outfitted());
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
    match config.item(SPEARS).and_then(|item| {
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

/// The **equipped** carry rate an item's default tier declares — what the material bought.
fn equipped_carry(config: &EquipmentConfig, item: &str) -> f32 {
    let stat = if item == SLED {
        EquipmentStat::HuntCarry
    } else {
        EquipmentStat::ForageCarry
    };
    config
        .item(item)
        .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"))
        .default_tier()
        .effects
        .iter()
        .find(|effect| effect.stat == stat)
        .map(|effect| effect.tier.value())
        .unwrap_or_else(|| panic!("'{item}' must declare an equipped {stat:?} on its tier"))
}

/// The **no-equipment** carry rate — the tier a party without the item falls back to. Since quality
/// tiers landed it lives in `labor_config.json`, because the *item's* tier declares the equipped
/// side; reading it off the item would find the number a kitted band gets.
fn unequipped_carry(_config: &EquipmentConfig, item: &str) -> f32 {
    let labor = core_sim::LaborConfig::builtin();
    if item == SLED {
        labor.hunt.per_worker_biomass_capacity
    } else {
        labor.forage.per_worker_biomass_capacity
    }
}

/// The **unequipped** value an item declares for a NAMED stat, on its SHARED effects — the pen's and
/// the vantage's side, whose equipped value lives elsewhere. Panics if the item declares the other
/// side, so a test cannot silently assert against the wrong side of a cliff.
fn unequipped_of(config: &EquipmentConfig, item: &str, stat: EquipmentStat) -> f32 {
    match config.item(item).and_then(|def| {
        def.effects
            .iter()
            .find(|effect| effect.stat == stat)
            .map(|effect| effect.tier)
    }) {
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

/// A start-stocked ledger with the given condition already **spent** on each named item — the
/// fixture's way of standing a band at an arbitrary point on its durability curve, reached the only
/// way the sim reaches it: by charging the item's own quantum.
fn worn(entries: &[(&str, f32)]) -> BandEquipment {
    let config = equipment();
    let mut ledger = outfitted();
    for (item, wear) in entries {
        let def = config
            .item(item)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"));
        // **Down to the LAST unit first.** A spawn stocks a party's worth, so a wear figure that
        // means *"this band is about to lose its sled"* has to spend the reserve before it can say
        // so — otherwise it names the condition of one unit among two dozen fresh ones.
        let uses_per_unit = def.default_tier().starting_durability / def.headline_wear().amount;
        while ledger.count_of(item) > 1 {
            ledger.wear_item(&config, item, def.headline_wear().per, uses_per_unit);
        }
        ledger.wear_item(
            &config,
            item,
            def.headline_wear().per,
            wear / def.headline_wear().amount,
        );
    }
    ledger
}

// ---------------------------------------------------------------------------------------------
// The expanded roster: scouting, warrior and husbandry (issue #492)
// ---------------------------------------------------------------------------------------------

/// **THE WAYFINDING KIT WEARS ON GROUND, NOT ON TURNS — and this is the half that proves the
/// quantum is not a clock in disguise.**
///
/// A band's own centre and its worked sources reveal fog too, and a parked band re-sees the same
/// ring every single turn. If `WearQuantum::TileRevealed` had been charged for *tiles seen* — or for
/// any reveal rather than a scout vantage's — a camp that never staffed a scout would still spend
/// wayfinding gear simply by existing, which is exactly the "an idle march costs the same as a
/// slaughter" failure `docs/plan_denial_raid.md` §1.2 forbids.
///
/// So: same world, same turn count, same fog being revealed by the band itself — **the difference is
/// solely whether anyone is staffing Scout**. Paired with the liveness half in
/// `wear_is_charged_for_work_not_for_turns_elapsed`, which asserts the scouting band's gear really
/// does run down, so this is not the trivial truth about a dead feature.
#[test]
fn only_a_staffed_scout_wears_the_wayfinding_kit() {
    let (mut scouting, scouting_band) = scouting_world(outfitted());
    // The hunting fixture staffs Hunt and nothing else, so its Scout head-count is zero and no
    // vantage is ever posted — while its band centre and its worked herd tile reveal fog every turn.
    let (mut unscouted, unscouted_band) = hunting_world(outfitted());
    for _ in 0..RUN_TURNS {
        run_turn(&mut scouting);
        run_turn(&mut unscouted);
    }

    assert!(
        kit_of(&scouting, scouting_band).wear_of(WAYFINDING) > 0.0,
        "liveness: a band staffing Scout must actually spend its wayfinding gear over {RUN_TURNS} turns"
    );
    assert_eq!(
        kit_of(&unscouted, unscouted_band).wear_of(WAYFINDING),
        0.0,
        "a band with nobody on Scout reveals fog from its own centre and its worked sources for \
         {RUN_TURNS} turns and must still spend NO wayfinding gear — the quantum is a scout's \
         first sightings, not a turn"
    );
}

/// **THE SCOUTING KIT DECIDES WHAT A VANTAGE MAKES OUT**, and running it dry steps the band down to
/// the item's own unequipped tier rather than blinding it.
///
/// The two tiers are `labor_config.json`'s `scout.vantage_range` (equipped — the shipped game has
/// always run kitted, so that number keeps its one home) and `wayfinding`'s declared unequipped
/// value. Asserted through the resolver rather than against literals, so a retune of either moves
/// the test with it; the ordering and the liveness of both sides are what is actually pinned.
#[test]
fn the_wayfinding_tier_steps_down_when_the_kit_runs_dry() {
    let equipment = equipment();
    let equipped_range = LaborConfig::builtin().scout.vantage_range as f32;
    let kit = equipment
        .kit("wayfinding")
        .expect("the shipped roster carries the wayfinding kit");

    let fresh = equipment.scout_vantage_range(equipped_range, &kit, &outfitted());
    let spent = equipment.scout_vantage_range(
        equipped_range,
        &kit,
        &worn(&[(
            WAYFINDING,
            equipment
                .item(WAYFINDING)
                .unwrap()
                .default_tier()
                .starting_durability,
        )]),
    );

    assert_eq!(
        fresh, equipped_range,
        "a fresh wayfinding kit posts vantages at labor_config's own range, which is where that \
         number lives"
    );
    assert_eq!(
        spent,
        unequipped_of(&equipment, WAYFINDING, EquipmentStat::ScoutVantageRange),
        "a dry wayfinding kit steps exactly onto the item's declared unequipped range"
    );
    assert!(
        spent < fresh,
        "...and that is a step DOWN: {spent} must be under {fresh}, or the kit buys nothing"
    );
    assert!(
        spent > 0.0,
        "...but never to blindness — a scout with no gear still sees their own tile's ring"
    );
}

/// **A PEN IS COLLECTED ON THE HUSBANDRY GEAR'S TIER, NOT THE SLED'S**, and the whole point of the
/// kit is that bringing the wrong tool costs you.
///
/// A sled drags a carcass in off the range; a pen stands at the camp, and what bounds a slaughter
/// there is handling gear. So `EquipmentStat::PenCarry` is a separate stat, the equipped side stays
/// `labor_config.hunt.per_worker_biomass_capacity` (the rate a pen has always collected at, keeping
/// its one home), and a crew that corralled its herd and stayed on the big-game kit collects at the
/// bare rate.
#[test]
fn only_the_husbandry_kit_collects_a_pen_at_the_shipped_rate() {
    let equipment = equipment();
    // The **baseline** a keeper without handling gear collects at; the equipped side is the hunt
    // haul's own tier, which `pen_per_worker_biomass_capacity` resolves internally so the number
    // keeps its one home.
    let baseline_rate = LaborConfig::builtin().hunt.per_worker_biomass_capacity;
    let fresh = outfitted();

    let husbandry = equipment
        .kit("husbandry")
        .expect("the shipped roster carries the husbandry kit");
    let big_game = equipment
        .kit("big_game")
        .expect("the shipped roster carries the big-game kit");

    assert_eq!(
        equipment.pen_per_worker_biomass_capacity(baseline_rate, &husbandry, &fresh),
        equipped_carry(&equipment, SLED),
        "a keeper with handling gear collects a pen at exactly the rate a pen has always collected \
         at — the hunt haul's own tier, shared rather than restated"
    );
    assert_eq!(
        equipment.pen_per_worker_biomass_capacity(baseline_rate, &big_game, &fresh),
        unequipped_of(&equipment, HUSBANDRY_GEAR, EquipmentStat::PenCarry),
        "a crew that brought spears and a sled to a pen collects at the bare rate — the sled is for \
         the range, and this is the decision the husbandry kit exists to make"
    );
    // ...and the two carry stats genuinely cannot reach each other: the husbandry kit still carries
    // a sled (a keeper hauls the meat home), so this is the discriminating direction.
    assert_eq!(
        equipment.hunt_per_worker_biomass_capacity(baseline_rate, &husbandry, &fresh),
        equipped_carry(&equipment, SLED),
        "the husbandry kit carries a sled too, so its RANGE haul is untouched"
    );
    assert!(
        equipment.pen_per_worker_biomass_capacity(baseline_rate, &big_game, &fresh)
            < equipped_carry(&equipment, SLED),
        "liveness: the bare pen rate must actually be lower, or both branches assert the same thing"
    );
}

/// **A PEN SHORT OF HANDLING GEAR COLLECTS AT THE MIX OF THE TWO TIERS** (issue #520) — the twin of
/// the basket test above, on the other stat.
///
/// Hurdles and halters cover keepers one unit at a time exactly as a spear covers a hunter, so
/// `advance_labor_allocation` and the assign-time seed both price a pen through
/// `coverage(...).weighted_rate(...)`. This asserts that very expression, because the file carries no
/// live corralled-herd world to run a pen through; the collection cap it feeds is `keepers × this`.
#[test]
fn a_pen_short_of_handling_gear_collects_between_the_bare_and_the_geared() {
    const KEEPERS: f32 = 8.0;
    const GEAR_OWNED: u32 = 2;

    let equipment = equipment();
    let baseline_rate = LaborConfig::builtin().hunt.per_worker_biomass_capacity;
    let husbandry = equipment
        .kit("husbandry")
        .expect("the shipped roster carries the husbandry kit");

    let pen_rate = |wear: &BandEquipment| {
        equipment
            .coverage(&husbandry, KEEPERS, wear)
            .weighted_rate(|kit| {
                equipment.pen_per_worker_biomass_capacity(baseline_rate, kit, wear)
            })
    };

    let geared = pen_rate(&outfitted());
    let partly = pen_rate(&short_of(HUSBANDRY_GEAR, GEAR_OWNED));
    let bare = pen_rate(&dry(HUSBANDRY_GEAR));

    assert_eq!(
        geared,
        equipped_carry(&equipment, SLED),
        "a fully geared pen crew collects exactly what a pen has always collected"
    );
    assert_eq!(
        bare,
        unequipped_of(&equipment, HUSBANDRY_GEAR, EquipmentStat::PenCarry),
        "and a crew with none of it collects at the bare rate"
    );
    assert!(
        bare < partly && partly < geared,
        "two sets of handling gear across {KEEPERS} keepers must land strictly between: \
         bare={bare} partly={partly} geared={geared}"
    );
    // The mix is the CREW-WEIGHTED mean, not an average of the two tiers — two keepers geared and
    // six not is a quarter of the way up, and that is what makes the cap `keepers × rate` correct.
    let expected =
        (GEAR_OWNED as f32 / KEEPERS) * geared + (1.0 - GEAR_OWNED as f32 / KEEPERS) * bare;
    assert!(
        (partly - expected).abs() < EPSILON,
        "the mix must be weighted by the crews' share: {partly} vs {expected}"
    );
}

/// **THE WARRIOR KIT ARMS THE CAMP, through the SAME `attack` stat and the same seam a spear does.**
///
/// A weapon is a weapon whichever role carries it — what keeps a club out of a hunt and a spear out
/// of a raid is the kit's `jobs` list, not a second stat. The unequipped side is `creatures.json`'s
/// `person.combat.attack`, exactly as it is for spears, so a band whose clubs are gone defends with
/// its hands.
#[test]
fn the_warrior_kit_swaps_the_defenders_attack_tier() {
    let equipment = equipment();
    let bare = CreaturesConfig::builtin().person();
    let fresh = outfitted();
    let warrior = equipment
        .kit("warrior")
        .expect("the shipped roster carries the warrior kit");

    let armed = equipment.warrior_profile(bare, &warrior, &fresh);
    let spent = equipment.warrior_profile(
        bare,
        &warrior,
        &worn(&[(
            CLUBS,
            equipment
                .item(CLUBS)
                .unwrap()
                .default_tier()
                .starting_durability,
        )]),
    );

    assert!(
        armed.attack > bare.attack,
        "clubs must beat the bare hand's {} (got {})",
        bare.attack,
        armed.attack
    );
    assert_eq!(
        spent.attack, bare.attack,
        "and a dry warrior kit steps exactly onto the person roster's own attack, not an \
         interpolated value"
    );
    // The rest of the profile is untouched — a weapon is a weapon, and armour is not this item.
    assert_eq!(armed.defense, bare.defense);
    assert_eq!(armed.durability, bare.durability);
}

/// **EVERY KIT CARRIES A QUANTUM THAT IS NOT A TURN**, and a JOB's gear is never charged on another
/// job's use.
///
/// The type already forbids a `turn` variant, so what this pins is the thing a config edit can still
/// get wrong: gear charged on a use its owner never made, which would let a band that only scouts
/// blunt a sled it never took out. Read off the shipped config, so authoring a kit onto the wrong
/// quantum fails here rather than in play.
///
/// > **A WEAPON SHARES ONE QUANTUM WITH EVERY OTHER WEAPON, deliberately.** `Kill` and `Fight`
/// > collapsed into `Strike` — what wears a spear and what wears a club is the same event, a blow
/// > landed — so the clubs are asserted *equal* to the spears here rather than distinct. **The kit
/// > mask is what keeps them apart**: `wear_kit` charges only items the crew's kit carries, and no
/// > kit carries both, so a hunt cannot blunt the camp's clubs. That is the guarantee this test
/// > used to get from distinct quanta and now gets from the mask.
#[test]
fn each_kit_wears_on_a_use_quantum_of_its_own_job() {
    let equipment = equipment();
    let quantum = |item: &str| {
        equipment
            .item(item)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"))
            .headline_wear()
            .per
    };
    assert_eq!(quantum(WAYFINDING), core_sim::WearQuantum::TileRevealed);
    assert_eq!(
        quantum(HUSBANDRY_GEAR),
        core_sim::WearQuantum::BiomassCollected
    );
    // Every weapon, and only a weapon, is charged per blow landed.
    for weapon in [SPEARS, THE_PASSIVE_DEVICE, CLUBS] {
        assert_eq!(
            quantum(weapon),
            core_sim::WearQuantum::Strike,
            "'{weapon}' is swung, so it wears per landed strike"
        );
    }
    // ...and no CARRY or non-weapon kit shares a quantum with any other job's.
    for (a, b) in [
        (SPEARS, SLED),
        (SPEARS, BASKETS),
        (SLED, BASKETS),
        (SLED, WAYFINDING),
        (SLED, HUSBANDRY_GEAR),
        (BASKETS, WAYFINDING),
        (BASKETS, HUSBANDRY_GEAR),
        (WAYFINDING, HUSBANDRY_GEAR),
    ] {
        assert_ne!(
            quantum(a),
            quantum(b),
            "'{b}' must not be charged on '{a}'s quantum — a band that only scouts would blunt \
             gear it never took out"
        );
    }
    // **The clubs cannot be reached from a hunt kit, which is what makes the shared quantum safe.**
    for kit_id in [
        "big_game",
        "trapping",
        "gathering",
        "husbandry",
        "wayfinding",
    ] {
        let kit = equipment
            .kit(kit_id)
            .unwrap_or_else(|| panic!("the shipped roster carries '{kit_id}'"));
        assert!(
            !kit.uses().any(|item| item == CLUBS),
            "'{kit_id}' must not carry clubs, or a hunt would charge the camp's weapons"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// COUNTS AND TIERS (the equipment-count slice)
// ---------------------------------------------------------------------------------------------

/// The tier every shipped item's one quality rung carries — the flint age.
const FLINT_TIER: &str = "flint";

/// **AN ABSENT ENTRY IS NOT OWNED — the invariant this slice inverted.**
///
/// It used to read as a *full* item, which was correct for exactly as long as nothing could make a
/// second spear: crafting can introduce an item a band has never had, and the old reading made that
/// state unrepresentable. So an empty ledger now resolves every **unequipped** tier, and a band that
/// owns nothing fights and hauls exactly as one carrying no kit does.
///
/// **Paired with the start-stocked reading**, because "an empty ledger is bare" passes just as well
/// on a sim that stopped resolving equipment at all.
#[test]
fn a_band_with_no_entry_for_an_item_resolves_the_unequipped_tier() {
    let equipment = equipment();
    let labor = LaborConfig::builtin();
    let intrinsic = CreaturesConfig::builtin().person();
    let owns_nothing = BandEquipment::default();
    let start_stocked = BandEquipment::start_stocked(&equipment);
    let big_game = equipment
        .kit("big_game")
        .expect("the roster ships big_game");
    let gathering = equipment
        .kit("gathering")
        .expect("the roster ships gathering");

    assert_eq!(
        equipment
            .hunter_profile_unbounded(intrinsic, &big_game, &owns_nothing)
            .attack,
        intrinsic.attack,
        "a band that owns no spears fights bare-handed, whatever kit it names"
    );
    assert_eq!(
        equipment.hunt_per_worker_biomass_capacity(
            labor.hunt.per_worker_biomass_capacity,
            &big_game,
            &owns_nothing
        ),
        labor.hunt.per_worker_biomass_capacity,
        "a band that owns no sled drags at the no-equipment baseline"
    );
    assert_eq!(
        equipment.forage_per_worker_biomass_capacity(
            labor.forage.per_worker_biomass_capacity,
            &gathering,
            &owns_nothing
        ),
        labor.forage.per_worker_biomass_capacity,
        "a band that owns no baskets gathers at the no-equipment baseline"
    );
    assert_eq!(
        owns_nothing.remaining(SPEARS, &equipment),
        0.0,
        "the wire reads NOT OWNED as no condition, never as a fresh item"
    );

    // ...and the same three readings on a start-stocked band are the equipped ones, or the above is
    // a statement about a sim that resolves nothing.
    assert_eq!(
        equipment
            .hunter_profile_unbounded(intrinsic, &big_game, &start_stocked)
            .attack,
        equipped_attack(&equipment)
    );
    assert_eq!(
        equipment.hunt_per_worker_biomass_capacity(
            labor.hunt.per_worker_biomass_capacity,
            &big_game,
            &start_stocked
        ),
        equipped_carry(&equipment, SLED)
    );
    assert_eq!(
        equipment.forage_per_worker_biomass_capacity(
            labor.forage.per_worker_biomass_capacity,
            &gathering,
            &start_stocked
        ),
        equipped_carry(&equipment, BASKETS)
    );
}

/// **THE REFERENCE LEDGER IS ONE UNIT, AND ONE UNIT IS THE SHIPPED LIFE.**
///
/// `BandEquipment::start_stocked` is the fresh *reference* ledger every quarry-scoring and
/// roster-quoting surface prices against, where only liveness is read — so it stocks one unit and
/// gains no head count. **A SPAWN is the other seam and stocks a party's worth**
/// (`start_stocked_owned`, sized by `start_stock_fraction`); that half is
/// `a_spawned_band_arms_every_worker_and_keeps_a_reserve`.
///
/// One unit is one item's `starting_durability` — the life the game has always had — asserted
/// against the literal use counts `equipment.md` records, so a retune of either dial fails here
/// rather than in play.
#[test]
fn the_reference_ledger_is_one_unit_and_the_shipped_lives_are_unchanged() {
    let equipment = equipment();
    let stocked = BandEquipment::start_stocked(&equipment);
    for item in [
        SPEARS,
        SLED,
        BASKETS,
        THE_PASSIVE_DEVICE,
        HUSBANDRY_GEAR,
        WAYFINDING,
        CLUBS,
    ] {
        assert_eq!(
            stocked.count_of(item),
            1,
            "the reference ledger holds exactly one '{item}' — it states liveness, not stock"
        );
        let def = equipment
            .item(item)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"));
        assert_eq!(
            stocked.remaining(item, &equipment),
            def.default_tier().starting_durability,
            "and it is unworn"
        );
    }

    // **The recorded lives, in the item's own quantum** (`equipment.md` → "Config files"). A batch
    // holds `count × starting_durability` of life, so stocking two would double every figure here.
    let uses = |item: &str| {
        let def = equipment
            .item(item)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{item}'"));
        def.default_tier().starting_durability / def.headline_wear().amount
    };
    assert_eq!(uses(SPEARS), 250.0, "250 kills of spears");
    assert_eq!(uses(SLED), 5000.0, "5000 biomass hauled");
    assert_eq!(uses(BASKETS), 2500.0, "2500 biomass gathered");
    assert_eq!(uses(THE_PASSIVE_DEVICE), 500.0, "500 kills of traps");
    assert_eq!(uses(HUSBANDRY_GEAR), 2500.0, "2500 biomass butchered");
    assert_eq!(uses(WAYFINDING), 2000.0, "2000 first sightings");
    assert_eq!(uses(CLUBS), 50.0, "50 raids fought");
}

/// **THE STOCK RUNS OUT ONE BATCH AT A TIME, and idle stock does not rot.**
///
/// Wear charges the most-worn LIVE batch first, so a band holding a half-spent unit and a fresh one
/// finishes the half-spent one before it touches the other — which is what makes a *"turns left"*
/// readout a real number instead of an average. And nothing at all charges a batch that did not go
/// out: the fresh unit's condition is untouched until the worn one is gone.
#[test]
fn wear_runs_the_stock_out_one_batch_at_a_time_and_idle_stock_does_not_rot() {
    let equipment = equipment();
    let def = equipment.item(SPEARS).expect("spears");
    let durability = def.default_tier().starting_durability;
    let uses_per_unit = durability / def.headline_wear().amount;

    let mut ledger = BandEquipment::start_stocked(&equipment);
    // Spend most of the spawned unit, then take delivery of a second batch of two.
    ledger.wear_item(
        &equipment,
        SPEARS,
        core_sim::WearQuantum::Strike,
        uses_per_unit * 0.8,
    );
    ledger.stock(SPEARS, 2, FLINT_TIER, None);
    assert_eq!(
        ledger.count_of(SPEARS),
        3,
        "one part-spent unit and two fresh"
    );
    let worn_remaining = ledger.remaining(SPEARS, &equipment);
    assert!(
        worn_remaining < durability,
        "the spawned unit is the worn one: {worn_remaining}"
    );

    // A charge that finishes the worn unit must not spill onto the fresh batch.
    ledger.wear_item(
        &equipment,
        SPEARS,
        core_sim::WearQuantum::Strike,
        uses_per_unit * 0.2,
    );
    assert_eq!(
        ledger.count_of(SPEARS),
        2,
        "the worn unit is gone, one batch at a time"
    );
    assert_eq!(
        ledger.remaining(SPEARS, &equipment),
        durability,
        "**IDLE STOCK DOES NOT ROT** — the fresh batch is untouched, so stockpiling ahead of a hard \
         season is a real strategy rather than a slow loss"
    );

    // And the fresh batch is really two items' worth of life, not one.
    ledger.wear_item(
        &equipment,
        SPEARS,
        core_sim::WearQuantum::Strike,
        uses_per_unit,
    );
    assert_eq!(ledger.count_of(SPEARS), 1, "one of the two is spent");
    ledger.wear_item(
        &equipment,
        SPEARS,
        core_sim::WearQuantum::Strike,
        uses_per_unit,
    );
    assert_eq!(ledger.count_of(SPEARS), 0, "and then the band is dry");
    assert_eq!(ledger.remaining(SPEARS, &equipment), 0.0);
}

/// **A TIER SWITCHES WHAT THE MATERIAL BOUGHT AND NOTHING ELSE.**
///
/// The shipped roster carries one tier per item on purpose — an unreachable tier is dead content the
/// Workbench publishes — so the switching is exercised by a **fixture**, exactly as the materials
/// table's `varieties` are. A bronze spear hits harder; it is still a thrown weapon with the same
/// `dispersion` and the same `exposure`, because those are the item's.
#[test]
fn a_tier_switches_an_items_attack_without_touching_its_shared_effects() {
    let mut json: serde_json::Value = serde_json::from_str(core_sim::BUILTIN_EQUIPMENT_CONFIG)
        .expect("the shipped table is json");
    // A second tier, gated on a craft the materials table declares, after the item's default.
    json["items"][SPEARS]["tiers"]
        .as_array_mut()
        .expect("spears declare a tier list")
        .push(serde_json::json!({
            "id": "bronze",
            "requires_knowledge": "bone_working",
            "starting_durability": 180.0,
            "effects": [{ "stat": "attack", "equipped": 34.0 }]
        }));
    let config =
        EquipmentConfig::from_json_str(&json.to_string()).expect("a second tier is a valid table");
    let def = config.item(SPEARS).expect("spears");

    let flint = def.default_tier();
    let bronze = def.tier("bronze").expect("the fixture added bronze");
    assert_eq!(flint.id, FLINT_TIER, "the FIRST tier is the default");
    assert_ne!(
        tier_attack(bronze),
        tier_attack(flint),
        "the tier is what the material bought"
    );
    assert_ne!(bronze.starting_durability, flint.starting_durability);

    // The shared effects are the ITEM's, so a band holding either tier resolves the same ones.
    let big_game = config.kit("big_game").expect("the roster ships big_game");
    let stocked = BandEquipment::start_stocked(&config);
    for tier in [FLINT_TIER, "bronze"] {
        let mut ledger = BandEquipment::default();
        ledger.stock(SPEARS, 1, tier, None);
        assert_eq!(
            config.dispersion(&big_game, &ledger),
            config.dispersion(&big_game, &stocked),
            "'{tier}' spears scare the herd exactly as much — dispersion is the item's"
        );
        assert_eq!(config.exposure(&big_game, &ledger), 1.0);
    }

    // ...and the attack a party resolves follows the batch's tier.
    let intrinsic = CreaturesConfig::builtin().person();
    let attack_at = |tier: &str| {
        let mut ledger = BandEquipment::default();
        ledger.stock(SPEARS, 1, tier, None);
        config
            .hunter_profile_unbounded(intrinsic, &big_game, &ledger)
            .attack
    };
    assert_eq!(attack_at(FLINT_TIER), tier_attack(flint));
    assert_eq!(attack_at("bronze"), tier_attack(bronze));
}

/// The `attack` a tier declares — panics if it declares none, so a fixture cannot assert against a
/// silently absent effect.
fn tier_attack(tier: &core_sim::EquipmentTier) -> f32 {
    tier.effects
        .iter()
        .find(|effect| effect.stat == EquipmentStat::Attack)
        .map(|effect| effect.tier.value())
        .expect("the tier must declare an attack")
}

// ---------------------------------------------------------------------------------------------
// MEASUREMENT — what the strike quantum costs, for issue #495's retune
// ---------------------------------------------------------------------------------------------

/// **REPORT-ONLY: how fast the shipped opening spends its spears, per landed blow.**
///
/// `WearQuantum::Strike` replaced `Kill`, and **strikes outnumber kills** — the shipped roster's
/// `wear.amount` was tuned against kills and is deliberately **not** retuned here (issue #495 owns
/// the balance pass). This harness is the input to that pass: it runs the shipped ~17-worker band on
/// Red Deer through real turns and reports the two numbers a retune needs — **charged strikes per
/// turn** and **turns per spear UNIT** — measured rather than derived, because the charge is
/// `strikes × absorbed/dealt` and the absorbed share depends on how much of the party's damage the
/// standing herd can take.
///
/// Asserts no bound, deliberately (the `fauna_migratory_representation` precedent): a floor on a
/// number the arc is still moving fails on a retune rather than on a regression. Run it with
/// `cargo test -p integration_tests --test equipment_toe -- --ignored --nocapture`.
#[test]
#[ignore = "report-only measurement for issue #495"]
fn report_the_strike_wear_the_shipped_opening_pays() {
    // Long enough to average the retreat's draw, short enough that no unit retires mid-run (a unit
    // is 250 strikes and the band lands at most one per hunter per turn), so the wear delta is a
    // clean strike count.
    const MEASURED_TURNS: u32 = 10;

    // **The SHIPPED opening, not this suite's calm one.** Every other fixture here holds the
    // roster's `wariness` at zero so the retreat cannot make a comparison turn on a draw; this one
    // must not, because the retreat is exactly what decides how much of the party's blow lands in a
    // body — and the band keeps its **own** spawn stock rather than the outsized fixture ledger.
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
    seat_quarry(&mut app, band_pos, DEER, DEER_BODY_MASS);
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: HERD_ID.to_string(),
                // The shipped default — the band holds the herd at its most productive biomass, so
                // the run measures a steady hunt rather than a herd being stripped to nothing.
                floor: core_sim::MSY_BIOMASS_FRACTION,
            },
            workers: workers.max(1),
            kit: None,
        }],
        ..Default::default()
    });

    let equipment = equipment();
    let spears = equipment.item(SPEARS).expect("spears ship");
    let per_strike = spears.headline_wear().amount;
    let unit_durability = spears.default_tier().starting_durability;

    let mut previous = kit_of(&app, band).wear_of(SPEARS);
    let mut strikes_per_turn = Vec::new();
    for _ in 0..MEASURED_TURNS {
        run_turn(&mut app);
        let now = kit_of(&app, band).wear_of(SPEARS);
        strikes_per_turn.push((now - previous) / per_strike);
        previous = now;
    }

    let total: f32 = strikes_per_turn.iter().sum();
    let mean = total / MEASURED_TURNS as f32;
    let strikes_per_unit = unit_durability / per_strike;
    println!("--- strike wear, shipped opening vs Red Deer ---");
    println!("per-turn charged strikes: {strikes_per_turn:?}");
    println!("mean charged strikes/turn: {mean:.2}");
    println!("strikes per spear unit:    {strikes_per_unit:.0}");
    println!(
        "turns per spear unit:      {:.1}",
        strikes_per_unit / mean.max(f32::EPSILON)
    );
    println!(
        "units the band holds:      {}",
        kit_of(&app, band).live_units(SPEARS, &equipment)
    );
    assert!(
        total > 0.0,
        "the band must actually be landing blows, or the report is about a dead sim"
    );
}
