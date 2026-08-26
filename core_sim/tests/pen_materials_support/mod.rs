//! **A PILE OF HURDLES A PEN BUILD CAN EAT** — the one fixture helper the material half of the
//! standing upkeep made every animal-ladder harness need (`docs/plan_standing_upkeep.md` §4.9
//! item 12).
//!
//! The `animal:pen` rung declares a **material pile** beside its `work_cost` and a **material rate**
//! beside its `work_per_turn`, and a build's accrual is scaled by the fraction of the pile the band's
//! store can pay for. So a fixture measuring the pen's **pacing** has to state its store, exactly as
//! a plant fixture has to state its gathering site: a bare `LocalStore` covers none of the pile, the
//! coverage is `0`, and the harness measures a stall it staged itself rather than the ladder.
//!
//! Shared rather than copied because the alternative is one drifting copy per test binary, and the
//! reading has to be the recipe's own or the batch a fixture holds is not the batch a bench makes.

// Justified per `.github/copilot-instructions.md`: this module is compiled WHOLE into each test
// binary that includes it, and each uses only the entry point its suite needs — same idiom and
// rationale as `telling_support`.
#![allow(dead_code)]

use bevy::prelude::Entity;
use bevy::prelude::World;
use core_sim::{scalar_from_f32, LocalStore, MaterialsConfig, PopulationCohort, RecipesConfig};

/// The material the `animal:pen` rung eats, on both its build pile and its upkeep rate.
pub const PEN_MATERIAL: &str = "hurdles";

/// Enough that neither the pile nor the fence's own mending is ever the binding constraint — a
/// fixture measuring the ladder must not accidentally measure the store.
const AMPLE_HURDLES: f32 = 1_000.0;

/// A `LocalStore` holding [`AMPLE_HURDLES`], at the reading the shipped recipe stamps its output
/// with — so the batch is the one a bench would have made.
pub fn stocked_with_pen_materials() -> LocalStore {
    let materials = MaterialsConfig::builtin();
    let recipes = RecipesConfig::builtin();
    let characteristics = recipes
        .recipes()
        .find_map(|(_, recipe)| {
            recipe
                .outputs
                .iter()
                .find(|output| output.material_id() == Some(PEN_MATERIAL))
                .map(|output| output.characteristics.clone())
        })
        .expect("the shipped book makes the pen's material");
    let band = materials
        .band_key(PEN_MATERIAL, &characteristics)
        .expect("the shipped roster rates the pen's material");
    let mut store = LocalStore::new();
    store.deposit_material(
        PEN_MATERIAL,
        band,
        scalar_from_f32(AMPLE_HURDLES),
        &characteristics,
    );
    store
}

/// [`stocked_with_pen_materials`] applied to a band already spawned — for a harness whose cohort is
/// built by a shared helper it does not own.
pub fn stock_pen_materials(world: &mut World, band: Entity) {
    let stocked = stocked_with_pen_materials();
    let mut cohort = world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band exists");
    for (id, batches) in stocked.materials() {
        for (key, batch) in batches {
            cohort
                .stores
                .deposit_material(id, key.clone(), batch.amount, &batch.characteristics);
        }
    }
}
