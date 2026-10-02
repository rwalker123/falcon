//! **Food spoilage by keeping class** (#706, `docs/plan_civilization_steps.md` §Step 5 → "Only food
//! the band cannot eat in time rots").
//!
//! A band eats its fastest-rotting food first, so a unit waits about *larder ÷ need* turns before it
//! is eaten, and it rots only if that wait is longer than its class's shelf life. No per-unit age is
//! tracked: the rule is a **line**, not a share. Sorting the classes by shelf life, class *k* rots by
//! however much the stock of it and every faster class runs past `need × shelf_life_k`:
//!
//! ```text
//! line_k     = need × shelf_life_k
//! rot_k      = min(stock_k, max(0, cumulative_k − line_k))   cumulative_k = Σ post-rot stock of
//! cumulative = cumulative_{k-1} + (stock_k − rot_k)            every faster class + stock_k
//! ```
//!
//! A band with a small surplus stays under every line and never sees rot, and the line scales with
//! the band: sixty people hold twice what thirty hold before anything spoils.
//!
//! A **caravan's pack** rots by its walk instead ([`rots_in_transit`]): a class whose shelf life is
//! shorter than the walk home is lost on the way, entirely.
//!
//! The rule is pure ([`larder_rot`]); [`rot_band_larders`] is the one system that applies it, once a
//! turn right after the meal and before the turn's income lands.

use bevy::prelude::{Query, Res, With};

use crate::components::{FoodMix, PopulationCohort, ResidentBand};
use crate::demographics_config::{DemographicsConfigHandle, KeepingConfig};
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};

/// **What rots out of `food` this turn, per class** — the line rule above, on a band whose people
/// need `need` food a turn. Pure: the caller subtracts it. A class the table does not carry never
/// rots (it has no shelf life to run past); a band that needs nothing (`need ≤ 0`) has every line at
/// zero, so everything it holds rots — there is nobody to eat it.
pub fn larder_rot(food: &FoodMix, need: f32, keeping: &KeepingConfig) -> FoodMix {
    let need = need.max(0.0);
    let mut rot = FoodMix::default();
    let mut cumulative = scalar_zero();
    for class in keeping.by_shelf_life() {
        let stock = food.get(&class.id);
        if stock <= scalar_zero() {
            continue;
        }
        let line = scalar_from_f32(need * class.shelf_life_turns);
        let over = (cumulative + stock - line).max(scalar_zero());
        let rotted = over.min(stock);
        rot.add(&class.id, rotted);
        cumulative += stock - rotted;
    }
    rot
}

/// **The larder a turn of rot would leave** — `food` less [`larder_rot`], as one total. The runway's
/// first-turn correction (`snapshot::population`): a larder above its lines must not read a runway
/// it cannot keep.
pub fn larder_after_rot(food: &FoodMix, need: f32, keeping: &KeepingConfig) -> Scalar {
    food.total() - larder_rot(food, need, keeping).total()
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
/// take lands, so the line is measured against the food the band carries into the turn. It also
/// **resets** [`PopulationCohort::last_food_spoiled`] for the turn (to this rot); the labor pass adds
/// any caravan transit rot on top.
///
/// `With<ResidentBand>`: a detached party's pack does not rot in this slice — it carries its
/// composition through every move and lands home in the band's larder, where it rots by this rule.
pub fn rot_band_larders(
    demographics: Res<DemographicsConfigHandle>,
    mut cohorts: Query<&mut PopulationCohort, With<ResidentBand>>,
) {
    let config = demographics.get();
    for mut cohort in cohorts.iter_mut() {
        let cohort = &mut *cohort;
        let rot = larder_rot(cohort.stores.food(), cohort.last_food_need, &config.keeping);
        let mut spoiled = scalar_zero();
        for (class, amount) in rot.iter() {
            spoiled += cohort.stores.take_food_class(class, amount);
        }
        cohort.last_food_spoiled = spoiled.to_f32();
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
    const NEED: f32 = 5.0;

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

    fn mix(rows: &[(&str, f32)]) -> FoodMix {
        let mut mix = FoodMix::default();
        for (class, amount) in rows {
            mix.add(class, scalar_from_f32(*amount));
        }
        mix
    }

    fn close(a: Scalar, b: f32) -> bool {
        (a.to_f32() - b).abs() < 1e-3
    }

    #[test]
    fn a_small_surplus_never_rots() {
        // Two turns of flesh, one of greens, a season of grain — every class under its line.
        let food = mix(&[(FLESH, NEED * 2.0), (FRESH, NEED), (DRY, NEED * 10.0)]);
        assert!(larder_rot(&food, NEED, &keeping()).is_empty());
    }

    #[test]
    fn a_larder_above_the_flesh_line_rots_exactly_the_excess() {
        let excess = 7.0;
        let food = mix(&[(FLESH, NEED * FLESH_SHELF + excess)]);
        let rot = larder_rot(&food, NEED, &keeping());
        assert!(close(rot.get(FLESH), excess), "rot {:?}", rot);
        assert!(close(rot.total(), excess));
    }

    #[test]
    fn faster_classes_count_against_a_slower_line() {
        // Flesh sits exactly on its own line, so it does not rot; but the band eats it first, so the
        // greens wait behind it and run past THEIR line by flesh + greens − need × 8.
        let flesh = NEED * FLESH_SHELF;
        let fresh = NEED * FRESH_SHELF - flesh + 3.0;
        let food = mix(&[(FLESH, flesh), (FRESH, fresh)]);
        let rot = larder_rot(&food, NEED, &keeping());
        assert!(close(rot.get(FLESH), 0.0));
        assert!(close(rot.get(FRESH), 3.0), "rot {:?}", rot);
    }

    #[test]
    fn the_cumulative_uses_the_post_rot_stock() {
        // Flesh far past its line rots down to the line; the greens behind it are then measured
        // against the flesh that SURVIVED, not the flesh that was there.
        let food = mix(&[
            (FLESH, 100.0),
            (FRESH, NEED * FRESH_SHELF - NEED * FLESH_SHELF),
        ]);
        let rot = larder_rot(&food, NEED, &keeping());
        assert!(close(rot.get(FLESH), 100.0 - NEED * FLESH_SHELF));
        assert!(close(rot.get(FRESH), 0.0), "rot {:?}", rot);
    }

    #[test]
    fn a_bigger_band_holds_more_before_rot() {
        let food = mix(&[(FLESH, 30.0)]);
        let small = larder_rot(&food, NEED, &keeping()).total();
        let large = larder_rot(&food, NEED * 2.0, &keeping()).total();
        assert!(small > scalar_zero(), "the small band is over its line");
        assert!(
            large < small,
            "twice the people hold twice the food before it rots"
        );
        assert!(close(large, (30.0 - NEED * 2.0 * FLESH_SHELF).max(0.0)));
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
