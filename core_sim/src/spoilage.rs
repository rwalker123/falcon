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
//! A **caravan's pack** keeps counting while it is carried: a pack whose walk is longer than a
//! class's shelf life is lost on the way, entirely ([`rots_in_transit`]); a pack that survives
//! lands aged by its walk, so its shelf life is counted from the kill and not from the landing.
//!
//! [`rot_band_larders`] is the one system that rots a larder, once a turn right after the meal and
//! before the turn's income lands. [`rot_ahead`] is the same rule walked forward for the forecasts.

use bevy::prelude::{Query, Res, With};

use crate::components::{FoodMix, PopulationCohort, ResidentBand};
use crate::demographics_config::{DemographicsConfigHandle, KeepingConfig};
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};

/// **How much of `food` will rot before it is eaten**, on a band that eats `need` a turn and takes
/// no income — the larder walked forward through the same meal-then-rot turns the sim runs. The
/// runway's first-turn correction (`snapshot::population`): a larder holding food that will expire
/// uneaten must not read a runway it cannot keep.
///
/// The walk is bounded by the longest shelf life in the table (past it every aged lot has expired),
/// and a class the table does not carry never rots.
pub fn rot_ahead(food: &FoodMix, need: f32, keeping: &KeepingConfig) -> Scalar {
    let ration = scalar_from_f32(need.max(0.0));
    let order = keeping.eat_order();
    let horizon = keeping
        .classes
        .iter()
        .map(|class| class.shelf_life_turns.ceil() as u32)
        .max()
        .unwrap_or(0);
    let mut stock = food.clone();
    let mut rotted = scalar_zero();
    for _ in 0..=horizon {
        if stock.is_empty() {
            break;
        }
        stock.eat(ration, &order);
        rotted += stock.age_and_expire(|class| keeping.shelf_life(class));
    }
    rotted
}

/// **Does a pack of `class` rot on a walk of `walk_turns`?** — a shelf life shorter than the walk
/// home. A walk of no length (a local row, a road the whole way) never rots anything, and a class
/// the table does not carry never rots.
pub fn rots_in_transit(class: &str, walk_turns: u32, keeping: &KeepingConfig) -> bool {
    keeping
        .shelf_life(class)
        .is_some_and(|shelf| shelf < walk_turns as f32)
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
pub fn rot_band_larders(
    demographics: Res<DemographicsConfigHandle>,
    mut cohorts: Query<&mut PopulationCohort, With<ResidentBand>>,
) {
    let config = demographics.get();
    for mut cohort in cohorts.iter_mut() {
        let rotted = cohort
            .stores
            .age_food(|class| config.keeping.shelf_life(class));
        cohort.last_food_spoiled = rotted.to_f32();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn the_forecast_is_what_the_turns_actually_rot() {
        // A mixed, mixed-age larder: the forward walk must name exactly what the turns then rot.
        let mut food = FoodMix::default();
        food.add_aged(FLESH, scalar_from_f32(30.0), 1);
        food.add(FLESH, scalar_from_f32(20.0));
        food.add(FRESH, scalar_from_f32(25.0));
        food.add(DRY, scalar_from_f32(40.0));
        for need in [0.0, 2.0, 5.0, 12.0, 60.0] {
            let predicted = rot_ahead(&food, need, &keeping());
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
        assert!(!rots_in_transit(FLESH, FLESH_SHELF as u32, &keeping));
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
}
