//! **Food spoilage by keeping class and AGE** (#706, `docs/plan_civilization_steps.md` §Step 5).
//!
//! Food rots at the END of its shelf life, not before. The larder holds each keeping class as
//! **lots with an age** ([`FoodMix`]); every turn, right after the meal, [`rot_band_larders`] ages
//! every lot by one and a lot whose age has reached its class's `shelf_life_turns` rots **whole**.
//! Nothing rots earlier, so anyone who joins the band in the meantime — a birth, a party coming
//! home, a band merging — can eat it, and the larder shows food that really exists until it goes
//! off. A kill that lands on turn *T* (age 0) is there for the meals of *T+1 … T+shelf* and rots in
//! the rot pass of *T+shelf*. The meal takes the fastest-rotting class first and the oldest lot
//! first within it.
//!
//! A **caravan's pack** keeps counting while it is carried: a pack whose walk is at least a
//! class's shelf life is lost on the way, entirely ([`rots_in_transit`]); a pack that survives
//! lands aged by its walk, so its shelf life is counted from the kill and not from the landing.
//!
//! [`rot_band_larders`] is the one system that rots a larder, once a turn right after the meal and
//! before the turn's income lands. [`rot_ahead`] is the same rule walked forward for the forecasts.
//!
//! **A store lengthens a shelf life** (#708): a band that holds a drying rack keeps a class longer
//! than its base. That is [`HeldPreservation`] — resolved once per band — read through
//! [`band_shelf_life`], the one seam. **A storage pit is not a preservation the band holds** (#709):
//! it is a second larder with an owner (`crate::fixed_store`), and the food IN it keeps the pit's
//! shelf life while the same food in the band's larder keeps its base.
//!
//! **Rot also teaches** (#707): a surplus that sat until it spoiled is the practice that teaches
//! [`STORAGE_KNOWLEDGE`], paid through the same knowledge ledger the intensification ladder uses
//! ([`storage_practice`]). Only the LARDER's rot teaches; a caravan pack lost on its walk is
//! distance, not a surplus that sat.

use bevy::prelude::{Query, Res, ResMut, With};
use std::collections::BTreeMap;

use crate::components::{BandId, FoodMix, PopulationCohort, ResidentBand};
use crate::demographics_config::{DemographicsConfigHandle, KeepingConfig};
use crate::intensification::{LadderConfig, LadderConfigHandle, LadderKnowledge, SourceKind};
use crate::resources::{DiscoveryProgressLedger, SimulationConfig, TileRegistry};
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};
use crate::supply_network_config::SupplyNetworkConfigHandle;
use crate::tile_improvement::TileImprovementRegistry;

/// **THE PRESERVATIONS A BAND HOLDS RIGHT NOW** (#708) — per keeping class, the longest shelf life
/// any rung it holds gives it. Empty for a band holding nothing, which is every band at the start
/// and every caller that has no band to ask about.
///
/// It is resolved **once per band per system run** ([`PreservationScene::held_by`]) and handed to
/// [`band_shelf_life`], so no per-lot lookup walks the improvements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeldPreservation {
    lives: BTreeMap<String, f32>,
}

impl HeldPreservation {
    /// A band that holds nothing: every class keeps its base life.
    pub fn none() -> Self {
        Self::default()
    }

    /// The longest preserved life this band holds for `class`, or `None` where it holds none.
    pub fn life_of(&self, class: &str) -> Option<f32> {
        self.lives.get(class).copied()
    }

    /// Hold a preservation: the class keeps `turns` if that is longer than what is already held.
    pub fn raise(&mut self, class: &str, turns: f32) {
        let held = self.lives.entry(class.to_string()).or_insert(turns);
        *held = held.max(turns);
    }

    /// Does this band hold no preservation at all?
    pub fn is_empty(&self) -> bool {
        self.lives.is_empty()
    }
}

/// **EVERYTHING A PRESERVATION IS RESOLVED AGAINST** — the keeping table, the ladder, the
/// improvements and the supply network's reach — so a caller builds it once and asks it per band.
/// The reach and grid are read by `crate::fixed_store` to find a band's own stores.
pub struct PreservationScene<'a> {
    pub keeping: &'a KeepingConfig,
    pub ladder: &'a LadderConfig,
    pub improvements: &'a TileImprovementRegistry,
    /// `supply_network_config.reach_tiles` - how near a band has to be to a store it owns to deposit
    /// into it and draw from it (`crate::fixed_store`).
    pub reach_tiles: u32,
    /// The grid's width and whether it wraps horizontally, for the hex distance.
    pub grid: (u32, bool),
}

impl PreservationScene<'_> {
    /// **WHAT `band` PRESERVES WITH**, for the entries of the keeping table whose rung is held
    /// (fully — an `on_completion` rung half built is not held): a **band**-kind rung is held by the
    /// band itself. A **tile**-kind rung (a storage pit) is never held by a band: the food in the
    /// pit keeps its life, the band's larder does not (`crate::fixed_store`, #709).
    ///
    /// Which kind a rung is, is the ladder's declaration; nothing here names a branch.
    pub fn held_by(&self, band: Option<BandId>) -> HeldPreservation {
        let mut held = HeldPreservation::none();
        for entry in &self.keeping.preservation {
            let rung = entry.rung;
            let holds = match self.ladder.source_kind(rung.branch) {
                Some(SourceKind::Band) => band.is_some_and(|band| {
                    self.improvements
                        .bands()
                        .get(band, rung.branch)
                        .is_some_and(|held| self.ladder.at_or_above(held.held_rung(), rung))
                }),
                _ => false,
            };
            if holds {
                held.raise(&entry.class, entry.shelf_life_turns);
            }
        }
        held
    }
}

/// **HOW LONG FOOD OF `class` KEEPS FOR A BAND, in turns** — `None` for a class the table does not
/// carry (it never rots). **THE ONE SEAM every reading of a band's shelf life goes through**: the
/// larder's rot ([`rot_band_larders`]), the forecast walk ([`rot_ahead`]) and the hunting crew's meal
/// (`hunt_by_need::hunt_meal`, [`keeps_no_longer_than`]) all call it with the band's own
/// [`HeldPreservation`], so they cannot disagree about how long a kill lasts.
///
/// **The longest of the base and every preservation the band holds** (#708): a band with a drying
/// rack keeps its `flesh` to the rack's life, one near its people's pit keeps its `dry` to the pit's,
/// and the rot rule and the meal a hunting crew is sized to lengthen together.
///
/// ⛔ **A CARAVAN PACK ON THE WALK IS NOT A BAND'S FOOD, and uses [`base_shelf_life`] instead**
/// ([`rots_in_transit`], `work_party::TransitRot`, the landing in `systems::labor`): it is away from
/// the camp and from the pit, so neither can keep it. A lot the larder holds keeps counting its age
/// when the band leaves a store behind, so dry food older than the base rots at once — intended.
pub fn band_shelf_life(
    keeping: &KeepingConfig,
    held: &HeldPreservation,
    class: &str,
) -> Option<f32> {
    let base = keeping.shelf_life(class)?;
    Some(
        held.life_of(class)
            .map_or(base, |preserved| preserved.max(base)),
    )
}

/// **HOW LONG FOOD OF `class` KEEPS BEFORE ANY STORE HELPS** — the class's own `shelf_life_turns`.
/// What a caravan pack on the walk keeps for: it is away from the camp's rack and from the pit.
pub fn base_shelf_life(keeping: &KeepingConfig, class: &str) -> Option<f32> {
    keeping.shelf_life(class)
}

/// **DOES FOOD OF `class` KEEP NO LONGER THAN FOOD OF `kill_class`?** — the test for whether a stored
/// class is "the food this kill replaces" (`hunt_by_need`): a class that keeps longer is a reserve
/// the kill does not stand in for. Both lives are read through [`band_shelf_life`], so the answer
/// moves with preservation; a class the table does not carry never rots and counts as keeping
/// longest.
pub fn keeps_no_longer_than(
    keeping: &KeepingConfig,
    held: &HeldPreservation,
    class: &str,
    kill_class: &str,
) -> bool {
    let life = |id: &str| band_shelf_life(keeping, held, id).unwrap_or(f32::INFINITY);
    life(class) <= life(kill_class)
}

/// The discovery-ledger id of the storage lesson. Taught by rot rather than by a rung or a bench,
/// so it is coded here beside the system that teaches it ([`rot_band_larders`]) and resolved by
/// `intensification::discovery_id_for`.
pub const STORAGE_DISCOVERY_ID: u32 = 2018;

/// The name of the storage lesson in `intensification_ladder.json` (`knowledge.lesson_costs`) and
/// in `start_profile_knowledge_tags.json`.
pub const STORAGE_KNOWLEDGE: &str = "storage";

/// **The larder time at which eating a lot counts as KEPT** (#708): the lot has sat in the band's
/// own larder `storage_kept_age_turns` turns. It reads the lot's larder clock (`FoodBatch::stored`),
/// never its spoilage `age`, which a caravan pack's walk has already advanced. A lot entering the
/// larder on turn `T` has `stored` `0` at meal `T+1` (the meal comes before the rot pass that
/// advances it) and `1` at meal `T+2`: so "kept N turns" is a larder time of `N - 1`. The ladder
/// validates `storage_kept_age_turns >= 1`, so this never underflows.
pub fn kept_from_stored(knowledge: &LadderKnowledge) -> u32 {
    knowledge.storage_kept_age_turns.saturating_sub(1)
}

/// **Practice one band's larder is worth this turn**, in the ladder's practice units - the ONE place
/// storage practice is computed. Both terms are measured against the band's OWN need, so band size
/// cancels, over the same `full_turn = need x storage_lesson_rot_turns`:
///
/// ```text
/// rot_practice  = learn_rate x min(rotted / full_turn, 1)
/// kept_practice = learn_rate x storage_kept_weight x min(kept / full_turn, 1)
/// total         = min(rot_practice + kept_practice, learn_rate)
/// ```
///
/// `kept` is the food the meal ate from lots that sat in the larder at least `storage_kept_age_turns`
/// ([`kept_from_stored`]). Zero when the band needs nothing or neither term has anything.
pub fn storage_practice(rotted: f32, kept: f32, need: f32, knowledge: &LadderKnowledge) -> f32 {
    if need <= 0.0 {
        return 0.0;
    }
    let full_turn = need * knowledge.storage_lesson_rot_turns;
    let share = |amount: f32| (amount.max(0.0) / full_turn).min(1.0);
    let rot_practice = knowledge.learn_rate * share(rotted);
    let kept_practice = knowledge.learn_rate * knowledge.storage_kept_weight * share(kept);
    (rot_practice + kept_practice).min(knowledge.learn_rate)
}

/// **How much of `food` will rot before it is eaten**, on a band that eats `need` a turn and takes
/// no income — the larder walked forward through the same meal-then-rot turns the sim runs. The
/// runway's first-turn correction (`snapshot::population`): a larder holding food that will expire
/// uneaten must not read a runway it cannot keep.
///
/// The walk is bounded by the longest shelf life in the table (past it every aged lot has expired),
/// and a class the table does not carry never rots. `held` is the band's own preservation, so the
/// forecast rots what the turns will rot.
pub fn rot_ahead(
    food: &FoodMix,
    need: f32,
    keeping: &KeepingConfig,
    held: &HeldPreservation,
) -> Scalar {
    let ration = scalar_from_f32(need.max(0.0));
    let order = keeping.eat_order();
    let horizon = keeping
        .classes
        .iter()
        .filter_map(|class| band_shelf_life(keeping, held, &class.id))
        .map(|life| life.ceil() as u32)
        .max()
        .unwrap_or(0);
    let mut stock = food.clone();
    let mut rotted = scalar_zero();
    for _ in 0..=horizon {
        if stock.is_empty() {
            break;
        }
        stock.eat(ration, &order);
        rotted += stock.age_and_expire(|class| band_shelf_life(keeping, held, class));
    }
    rotted
}

/// **[`rot_ahead`] OVER A LARDER AND THE STORES BEHIND IT** (#709) - the same meal-then-rot walk, the
/// band eating its larder first and its stores last, each rotting at its own shelf lives: the
/// larder at `held`, the stores at `store_held` (the in-store lives, [`band_shelf_life`] taking the
/// longer of base and store). Returns everything that rots uneaten, larder and stores together.
pub fn rot_ahead_with_stores(
    larder: &FoodMix,
    stores: &FoodMix,
    need: f32,
    keeping: &KeepingConfig,
    held: &HeldPreservation,
    store_held: &HeldPreservation,
) -> Scalar {
    let ration = scalar_from_f32(need.max(0.0));
    let order = keeping.eat_order();
    let horizon = keeping
        .classes
        .iter()
        .flat_map(|class| {
            [
                band_shelf_life(keeping, held, &class.id),
                band_shelf_life(keeping, store_held, &class.id),
            ]
        })
        .flatten()
        .map(|life| life.ceil() as u32)
        .max()
        .unwrap_or(0);
    let (mut larder, mut stores) = (larder.clone(), stores.clone());
    let mut rotted = scalar_zero();
    for _ in 0..=horizon {
        if larder.is_empty() && stores.is_empty() {
            break;
        }
        let eaten = larder.eat(ration, &order);
        stores.eat(ration - eaten, &order);
        rotted += larder.age_and_expire(|class| band_shelf_life(keeping, held, class));
        rotted += stores.age_and_expire(|class| band_shelf_life(keeping, store_held, class));
    }
    rotted
}

/// **Does a pack of `class` rot on a walk of `walk_turns`?** — a shelf life no longer than the walk
/// home (the larder expires a lot at `age >= shelf`, and a pack's walk is its age). A walk of no length (a local row, a road the whole way) never rots anything, and a class
/// the table does not carry never rots. **The BASE shelf life**: a pack on the walk is away from the
/// camp's rack and from the pit ([`band_shelf_life`]).
pub fn rots_in_transit(class: &str, walk_turns: u32, keeping: &KeepingConfig) -> bool {
    base_shelf_life(keeping, class).is_some_and(|shelf| shelf <= walk_turns as f32)
}

/// **THE LARDER ROT, ONCE A TURN** — right after `simulate_population`'s meal and before the turn's
/// take lands: every lot ages a turn and a lot at its shelf life rots whole. It also **resets**
/// [`PopulationCohort::last_food_spoiled`] for the turn (to this rot); the labor pass adds any
/// caravan transit rot on top. A band that needs nothing rots nothing early: food nobody eats simply
/// sits until it expires.
///
/// `With<ResidentBand>`: a detached party's pack does not rot in this slice — it carries its lots
/// and their ages through every move and lands home in the band's larder, where they rot by this
/// rule.
///
/// **A band rots at ITS shelf life** (#708): its [`HeldPreservation`] is resolved once per band
/// here, off the improvements as the turn found them, and handed to the one seam.
///
/// **Rot teaches storage** (#707): the larder rot this pass computes — and only that, before the
/// labor pass adds caravan transit rot — is credited to the faction's [`STORAGE_KNOWLEDGE`] lesson
/// once per band per turn ([`storage_practice`], priced by `LadderKnowledge::ledger_credit`).
#[allow(clippy::too_many_arguments)] // a Bevy system: one parameter per resource it reads
pub fn rot_band_larders(
    demographics: Res<DemographicsConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    mut ledger: ResMut<DiscoveryProgressLedger>,
    improvements: Option<Res<TileImprovementRegistry>>,
    supply: Option<Res<SupplyNetworkConfigHandle>>,
    tile_registry: Option<Res<TileRegistry>>,
    sim_config: Option<Res<SimulationConfig>>,
    mut cohorts: Query<(&mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    let config = demographics.get();
    let ladder = ladder.get();
    let supply = supply.map(|handle| handle.get());
    // A hand-built world may lack a resource; with none of them there is nothing to preserve with,
    // and every band rots at its base life.
    let scene = match (
        improvements.as_deref(),
        supply.as_deref(),
        tile_registry.as_deref(),
    ) {
        (Some(improvements), Some(supply), Some(tile_registry)) => Some(PreservationScene {
            keeping: &config.keeping,
            ladder: &ladder,
            improvements,
            reach_tiles: supply.reach_tiles,
            grid: (
                tile_registry.width,
                sim_config
                    .as_deref()
                    .is_some_and(|config| config.map_topology.wrap_horizontal),
            ),
        }),
        _ => None,
    };
    for (mut cohort, band_id) in cohorts.iter_mut() {
        let held = scene.as_ref().map_or_else(HeldPreservation::none, |scene| {
            scene.held_by(band_id.copied())
        });
        let rotted = cohort
            .stores
            .age_food(|class| band_shelf_life(&config.keeping, &held, class));
        cohort.last_food_spoiled = rotted.to_f32();
        let practice = storage_practice(
            rotted.to_f32(),
            cohort.last_food_kept,
            cohort.last_food_need,
            &ladder.knowledge,
        );
        if let Some(credit) = ladder.knowledge.ledger_credit(STORAGE_KNOWLEDGE, practice) {
            if credit > 0.0 {
                ledger.add_progress(
                    cohort.faction,
                    STORAGE_DISCOVERY_ID,
                    scalar_from_f32(credit),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::LocalStore;
    use crate::demographics_config::{DemographicsConfig, KeepingClass};

    const FLESH: &str = "flesh";
    const FRESH: &str = "fresh_plant";
    const DRY: &str = "dry";
    /// Shelf lives the tests state for themselves, so a re-tune of the shipped table cannot move
    /// the arithmetic under them.
    const FLESH_SHELF: f32 = 4.0;
    const FRESH_SHELF: f32 = 8.0;
    const DRY_SHELF: f32 = 60.0;

    fn keeping() -> KeepingConfig {
        KeepingConfig {
            classes: vec![
                KeepingClass {
                    id: DRY.to_string(),
                    shelf_life_turns: DRY_SHELF,
                },
                KeepingClass {
                    id: FLESH.to_string(),
                    shelf_life_turns: FLESH_SHELF,
                },
                KeepingClass {
                    id: FRESH.to_string(),
                    shelf_life_turns: FRESH_SHELF,
                },
            ],
            startup_class: DRY.to_string(),
            plant_fallback_class: FRESH.to_string(),
            kill_fallback_class: FLESH.to_string(),
            preservation: Vec::new(),
            fixed_stores: Vec::new(),
        }
    }

    fn close(a: Scalar, b: f32) -> bool {
        (a.to_f32() - b).abs() < 1e-3
    }

    /// One turn the way the sim runs it: the meal, then the rot pass. Returns what rotted.
    fn turn(store: &mut crate::components::LocalStore, need: f32) -> Scalar {
        store.eat_food(scalar_from_f32(need), &keeping().eat_order());
        store.age_food(|class| keeping().shelf_life(class))
    }

    #[test]
    fn a_kill_is_eaten_down_and_rots_whole_at_the_end_of_its_shelf_life() {
        // A 48-flesh kill on a band needing 2: the larder reads 46, 44, 42 over the first three
        // meals and nothing rots until the fourth rot pass, which takes what is left. (The retired
        // line rule cut it to need × shelf = 8 on the very first turn.)
        const KILL: f32 = 48.0;
        const BAND_NEED: f32 = 2.0;
        let mut store = crate::components::LocalStore::new();
        store.add_food(FLESH, scalar_from_f32(KILL));
        for (turn_number, expected) in [(1, 46.0), (2, 44.0), (3, 42.0)] {
            let rotted = turn(&mut store, BAND_NEED);
            assert!(close(rotted, 0.0), "turn {turn_number}: nothing rots early");
            assert!(
                close(store.food().total(), expected),
                "turn {turn_number}: larder {:?}",
                store.food().total()
            );
        }
        let rotted = turn(&mut store, BAND_NEED);
        assert!(
            close(rotted, KILL - BAND_NEED * FLESH_SHELF),
            "the fourth rot pass takes the remaining {rotted:?}"
        );
        assert!(store.food().is_empty());
    }

    #[test]
    fn a_band_that_needs_nothing_keeps_its_food_until_it_expires() {
        let mut store = crate::components::LocalStore::new();
        store.add_food(FLESH, scalar_from_f32(10.0));
        for _ in 1..FLESH_SHELF as u32 {
            assert!(close(turn(&mut store, 0.0), 0.0));
        }
        assert!(close(store.food().total(), 10.0));
        assert!(close(turn(&mut store, 0.0), 10.0), "it rots at its end");
    }

    #[test]
    fn someone_arriving_mid_window_eats_from_the_kill() {
        // The same kill, but the band grows to need 22 a turn after the second meal.
        let mut store = crate::components::LocalStore::new();
        store.add_food(FLESH, scalar_from_f32(48.0));
        turn(&mut store, 2.0);
        turn(&mut store, 2.0);
        turn(&mut store, 22.0);
        let rotted = turn(&mut store, 22.0);
        assert!(
            close(rotted, 0.0),
            "the bigger band ate it before it expired"
        );
        assert!(store.food().is_empty(), "left {:?}", store.food().total());
    }

    #[test]
    fn the_meal_takes_the_oldest_lot_first_and_the_fastest_class_first() {
        let mut food = FoodMix::default();
        food.add_aged(FLESH, scalar_from_f32(3.0), 2);
        food.add_aged(FLESH, scalar_from_f32(3.0), 0);
        food.add(FRESH, scalar_from_f32(3.0));
        let mut store = crate::components::LocalStore::new();
        store.add_food_mix(&food);
        store.eat_food(scalar_from_f32(4.0), &keeping().eat_order());
        let left: Vec<(String, u32, f32)> = store
            .food()
            .batches()
            .map(|(class, age, amount)| (class.to_string(), age, amount.to_f32()))
            .collect();
        assert_eq!(
            left,
            vec![(FLESH.to_string(), 0, 2.0), (FRESH.to_string(), 0, 3.0)],
            "the aged flesh lot went first, then the fresher one; the greens wait"
        );
    }

    #[test]
    fn a_proportional_split_keeps_ages_so_both_halves_expire_together() {
        let mut food = FoodMix::default();
        food.add_aged(FLESH, scalar_from_f32(10.0), 3);
        food.add(FLESH, scalar_from_f32(10.0));
        let mut stays = crate::components::LocalStore::new();
        stays.add_food_mix(&food);
        let moves = stays.take_food_mix(scalar_from_f32(8.0));
        assert!(close(moves.total(), 8.0));
        let ages = |mix: &FoodMix| -> Vec<u32> { mix.batches().map(|(_, age, _)| age).collect() };
        assert_eq!(ages(&moves), vec![3, 0]);
        assert_eq!(ages(stays.food()), vec![3, 0]);
        let mut left = stays;
        let mut gone = crate::components::LocalStore::new();
        gone.add_food_mix(&moves);
        let (a, b) = (
            left.age_food(|c| keeping().shelf_life(c)),
            gone.age_food(|c| keeping().shelf_life(c)),
        );
        assert!(
            a > scalar_zero() && b > scalar_zero(),
            "both halves hold the aged lot"
        );
        assert!(left.food().get(FLESH) > scalar_zero());
    }

    #[test]
    fn a_pack_lands_aged_by_its_walk_and_expires_that_much_sooner() {
        const WALK: u32 = 2;
        let turns_until_it_rots = |age: u32| {
            let mut store = crate::components::LocalStore::new();
            store.add_food_aged(FLESH, scalar_from_f32(5.0), age);
            (1..)
                .find(|_| turn(&mut store, 0.0) > scalar_zero())
                .expect("it rots")
        };
        assert_eq!(turns_until_it_rots(0), FLESH_SHELF as u32);
        assert_eq!(turns_until_it_rots(WALK), FLESH_SHELF as u32 - WALK);
    }

    #[test]
    fn merging_keeps_the_incoming_ages_and_merges_equal_ones() {
        let mut a = FoodMix::default();
        a.add_aged(FLESH, scalar_from_f32(1.0), 2);
        let mut b = FoodMix::default();
        b.add_aged(FLESH, scalar_from_f32(2.0), 2);
        b.add(FLESH, scalar_from_f32(4.0));
        a.merge(&b);
        let lots: Vec<(u32, f32)> = a.batches().map(|(_, age, n)| (age, n.to_f32())).collect();
        assert_eq!(lots, vec![(2, 3.0), (0, 4.0)]);
    }

    /// **Larder time rides every move.** A lot that sat a turn keeps that turn through a merge, a
    /// proportional split and the serialized form, and does not merge with a same-age lot that has
    /// not sat.
    #[test]
    fn larder_time_survives_merges_splits_and_never_merges_with_a_fresher_lot() {
        let mut sat = FoodMix::default();
        sat.add(DRY, scalar_from_f32(10.0));
        sat.age_and_expire(|c| keeping().shelf_life(c));
        // A pack that walked one turn lands now: same spoilage age, no larder time.
        let mut landed = FoodMix::default();
        landed.add_aged(DRY, scalar_from_f32(10.0), 1);
        sat.merge(&landed);
        assert_eq!(sat.batches().count(), 2, "equal age, different larder time");

        let half = sat.proportional(scalar_from_f32(10.0));
        let mut kept = scalar_zero();
        let mut mix = half.clone();
        let mut again = FoodMix::default();
        again.merge(&mix);
        assert_eq!(again, half);
        let (_, k) = mix.eat_measuring_kept(scalar_from_f32(100.0), &keeping().eat_order(), 1);
        kept += k;
        assert!(close(kept, 5.0), "only the half that sat counts: {kept:?}");

        let json = serde_json::to_string(&sat).expect("serializes");
        let back: FoodMix = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, sat);
    }

    #[test]
    fn the_forecast_is_what_the_turns_actually_rot() {
        // A mixed, mixed-age larder: the forward walk must name exactly what the turns then rot.
        let mut food = FoodMix::default();
        food.add_aged(FLESH, scalar_from_f32(30.0), 1);
        food.add(FLESH, scalar_from_f32(20.0));
        food.add(FRESH, scalar_from_f32(25.0));
        food.add(DRY, scalar_from_f32(40.0));
        for need in [0.0, 2.0, 5.0, 12.0, 60.0] {
            let predicted = rot_ahead(&food, need, &keeping(), &HeldPreservation::none());
            let mut store = crate::components::LocalStore::new();
            store.add_food_mix(&food);
            let mut actual = scalar_zero();
            for _ in 0..=DRY_SHELF as u32 {
                actual += turn(&mut store, need);
            }
            assert!(
                close(predicted, actual.to_f32()),
                "need {need}: predicted {predicted:?} vs actual {actual:?}"
            );
        }
    }

    #[test]
    fn the_batches_survive_a_checkpoint_round_trip() {
        let mut food = FoodMix::default();
        food.add_aged(FLESH, scalar_from_f32(3.0), 2);
        food.add(FLESH, scalar_from_f32(1.5));
        food.add(DRY, scalar_from_f32(9.0));
        let json = serde_json::to_string(&food).expect("serializes");
        let back: FoodMix = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, food);
        assert_eq!(back.batches().count(), 3);
    }

    #[test]
    fn eating_draws_the_fastest_rotting_class_first() {
        let mut store = crate::components::LocalStore::new();
        store.add_food(DRY, scalar_from_f32(10.0));
        store.add_food(FLESH, scalar_from_f32(3.0));
        store.add_food(FRESH, scalar_from_f32(4.0));
        let eaten = store.eat_food(scalar_from_f32(5.0), &keeping().eat_order());
        assert!(close(eaten, 5.0));
        assert!(close(store.food().get(FLESH), 0.0), "flesh goes first");
        assert!(close(store.food().get(FRESH), 2.0), "then the greens");
        assert!(close(store.food().get(DRY), 10.0), "the grain waits");
    }

    #[test]
    fn a_walk_longer_than_a_shelf_life_rots_that_class_and_no_other() {
        let keeping = keeping();
        assert!(rots_in_transit(FLESH, FLESH_SHELF as u32 + 1, &keeping));
        // A walk of exactly the shelf life is lost too: the larder expires a lot at `age >= shelf`.
        assert!(rots_in_transit(FLESH, FLESH_SHELF as u32, &keeping));
        assert!(!rots_in_transit(FLESH, FLESH_SHELF as u32 - 1, &keeping));
        assert!(!rots_in_transit(DRY, FLESH_SHELF as u32 + 1, &keeping));
        assert!(!rots_in_transit(
            FLESH,
            crate::work_party::NO_WALK,
            &keeping
        ));
    }

    #[test]
    fn the_shipped_table_eats_flesh_then_greens_then_grain() {
        let config = DemographicsConfig::default();
        assert_eq!(
            config.keeping.eat_order(),
            vec![FLESH.to_string(), FRESH.to_string(), DRY.to_string()]
        );
    }

    /// The ladder pace the storage tests state for themselves, so a re-tune of the shipped dials
    /// cannot move the arithmetic under them.
    const LEARN_RATE: f32 = 1.0;
    /// Turns of need that count as one full turn of practice (the shipped dial's value).
    const ROT_TURNS: f32 = 1.0;
    /// Two band sizes' per-turn need and the share of it that rots, for the size-cancels pin.
    const SMALL_NEED: f32 = 30.0;
    const BIG_NEED: f32 = 60.0;
    const HALF: f32 = 0.5;
    /// Rot far past one full turn's need (a whole kill expiring at once), which must still pay no
    /// more than one turn of practice.
    const MANY_TURNS_OF_NEED: f32 = 10.0;

    fn knowledge() -> LadderKnowledge {
        let mut knowledge = crate::intensification::LadderConfig::builtin()
            .knowledge
            .clone();
        knowledge.learn_rate = LEARN_RATE;
        knowledge.storage_lesson_rot_turns = ROT_TURNS;
        knowledge
    }

    /// Rot's practice alone: no kept food.
    fn rot_only(rotted: f32, need: f32, knowledge: &LadderKnowledge) -> f32 {
        storage_practice(rotted, NOTHING_KEPT, need, knowledge)
    }

    /// Kept food's practice alone: nothing rotted.
    fn kept_only(kept: f32, need: f32, knowledge: &LadderKnowledge) -> f32 {
        storage_practice(NOTHING_ROTTED, kept, need, knowledge)
    }

    const NOTHING_KEPT: f32 = 0.0;
    const NOTHING_ROTTED: f32 = 0.0;
    /// The shipped weight, stated so a re-tune of the shipped table cannot move the arithmetic.
    const KEPT_WEIGHT: f32 = 0.25;
    /// The shipped threshold, stated for the same reason.
    const KEPT_AGE_TURNS: u32 = 2;
    /// Rot this share of a full turn, with a full turn of kept food beside it: past the cap.
    const NINE_TENTHS: f32 = 0.9;

    #[test]
    fn no_rot_teaches_nothing() {
        assert_eq!(rot_only(0.0, SMALL_NEED, &knowledge()), 0.0);
    }

    #[test]
    fn a_band_that_needs_nothing_learns_nothing_from_its_rot() {
        assert_eq!(rot_only(SMALL_NEED, 0.0, &knowledge()), 0.0);
    }

    #[test]
    fn a_turns_need_rotting_is_a_full_turn_of_practice_and_more_is_capped() {
        let k = knowledge();
        assert_eq!(rot_only(SMALL_NEED * ROT_TURNS, SMALL_NEED, &k), LEARN_RATE);
        assert_eq!(
            rot_only(SMALL_NEED * MANY_TURNS_OF_NEED, SMALL_NEED, &k),
            LEARN_RATE
        );
    }

    #[test]
    fn half_a_turns_need_rotting_is_half_the_practice() {
        let practice = rot_only(SMALL_NEED * HALF * ROT_TURNS, SMALL_NEED, &knowledge());
        assert!((practice - LEARN_RATE * HALF).abs() < 1e-6, "{practice}");
    }

    #[test]
    fn band_size_cancels() {
        let k = knowledge();
        assert_eq!(
            rot_only(SMALL_NEED * HALF, SMALL_NEED, &k),
            rot_only(BIG_NEED * HALF, BIG_NEED, &k)
        );
        assert_eq!(
            kept_only(SMALL_NEED * HALF, SMALL_NEED, &k),
            kept_only(BIG_NEED * HALF, BIG_NEED, &k)
        );
    }

    /// **Kept food pays the weight's share of rot's practice** - a full turn of need eaten kept is
    /// `weight x learn_rate`, and more is capped there.
    #[test]
    fn a_turns_need_of_kept_food_pays_the_weight_of_a_full_turn() {
        let k = knowledge();
        let full = SMALL_NEED * ROT_TURNS;
        assert!((kept_only(full, SMALL_NEED, &k) - LEARN_RATE * KEPT_WEIGHT).abs() < 1e-6);
        assert!(
            (kept_only(full * MANY_TURNS_OF_NEED, SMALL_NEED, &k) - LEARN_RATE * KEPT_WEIGHT).abs()
                < 1e-6,
            "kept food past a full turn adds nothing"
        );
        assert!(kept_only(full, SMALL_NEED, &k) < rot_only(full, SMALL_NEED, &k));
    }

    /// **Rot and kept food co-occurring are capped at ONE full turn** of practice.
    #[test]
    fn rot_and_kept_food_together_cap_at_one_full_turn() {
        let k = knowledge();
        let full = SMALL_NEED * ROT_TURNS;
        // Rot alone is already a full turn: kept food adds nothing.
        assert_eq!(storage_practice(full, full, SMALL_NEED, &k), LEARN_RATE);
        // Half a turn of rot and a full turn of kept food: the sum, under the cap.
        let sum = storage_practice(full * HALF, full, SMALL_NEED, &k);
        assert!(
            (sum - LEARN_RATE * (HALF + KEPT_WEIGHT)).abs() < 1e-6,
            "{sum}"
        );
        // Nine tenths of rot and a full turn of kept food would sum past the cap.
        let capped = storage_practice(full * NINE_TENTHS, full, SMALL_NEED, &k);
        assert_eq!(capped, LEARN_RATE);
    }

    /// **THE BOUNDARY, THROUGH THE REAL MEAL-THEN-AGE ORDER.** A lot landed on turn `T` has stored
    /// age 0 at meal `T+1` (fresh), and age 1 at meal `T+2` (kept at the shipped threshold of 2).
    #[test]
    fn a_lot_is_kept_from_the_second_meal_after_it_landed() {
        let k = knowledge();
        assert_eq!(k.storage_kept_age_turns, KEPT_AGE_TURNS);
        let from_age = kept_from_stored(&k);
        let order = keeping().eat_order();
        let ration = scalar_from_f32(1.0);
        let mut store = LocalStore::default();
        // Landed on turn T (after that turn's meal and rot).
        store.add_food("dry", scalar_from_f32(10.0));

        // Meal T+1: the lot has not aged yet.
        let (eaten, kept) = store.eat_food_measuring_kept(ration, &order, from_age);
        assert_eq!(eaten, ration);
        assert_eq!(kept, scalar_zero(), "eaten at the first meal: fresh");
        store.age_food(|_| Some(f32::INFINITY));

        // Meal T+2: one turn of age.
        let (_, kept) = store.eat_food_measuring_kept(ration, &order, from_age);
        assert_eq!(kept, ration, "eaten at the second meal: kept");
    }

    /// **Food eaten fresh teaches nothing**, however much of it there is.
    #[test]
    fn food_eaten_fresh_pays_no_practice() {
        let k = knowledge();
        let order = keeping().eat_order();
        let mut store = LocalStore::default();
        store.add_food("flesh", scalar_from_f32(10.0));
        let (_, kept) =
            store.eat_food_measuring_kept(scalar_from_f32(5.0), &order, kept_from_stored(&k));
        assert_eq!(
            storage_practice(NOTHING_ROTTED, kept.to_f32(), SMALL_NEED, &k),
            0.0
        );
    }

    /// **The meal reads the oldest lot first**, so the kept share is the lot that has sat in the
    /// larder, not a share of the class.
    #[test]
    fn a_mixed_age_meal_counts_only_the_lots_that_sat_as_kept() {
        let k = knowledge();
        let order = keeping().eat_order();
        let mut store = LocalStore::default();
        store.add_food("dry", scalar_from_f32(2.0));
        for _ in 0..3 {
            store.age_food(|_| Some(f32::INFINITY));
        }
        store.add_food("dry", scalar_from_f32(5.0));
        let (eaten, kept) =
            store.eat_food_measuring_kept(scalar_from_f32(4.0), &order, kept_from_stored(&k));
        assert_eq!(eaten, scalar_from_f32(4.0));
        assert_eq!(kept, scalar_from_f32(2.0));
    }
}
