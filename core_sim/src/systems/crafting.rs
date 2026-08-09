//! **The bench** — one job at a time, per band (`docs/plan_crafting_and_materials.md` §5).
//!
//! Five steps, in this order and for these reasons:
//!
//! 1. **No recipe ⇒ nothing.** An idle bench is not a state that costs anything.
//! 2. **Nothing drawn ⇒ draw.** Withdraw each input's `amount × craft_material_efficiency`
//!    worst-first ([`LocalStore::take_material`]), record the exact reading on the recipe's `reads`
//!    axis, and **fix the grade there**. A short draw withdraws **nothing** — the turn is a no-op,
//!    not a half-spent pile.
//! 3. **Accrue** `workers × progress_per_worker_turn × craft_speed`, where `craft_speed` is the
//!    bounding tool's equipped value if the band has one, else the **material's** own
//!    `hand_working.rate`. **That is `0` for a material with no `hand_working`, which is how metal
//!    will refuse itself with no branch** — exactly as `max(0, attack − defense)` refuses a hunt.
//! 4. **On `progress >= work`**: emit the outputs, charge **one** [`WearQuantum::ItemCrafted`] on the
//!    bounding tool and **one** lesson of the recipe's craft. Same quantum, same count, one place —
//!    so the thing that consumes the tool and the thing that teaches the craft cannot drift.
//! 5. **Reset and re-draw.** The next pass's grade is fixed from the stock the band has *now*.
//!
//! **There is no "you cannot craft that" branch anywhere in here.** Every refusal the design names
//! is a zero: a zero rate (no tool, no bare-handed rate), a zero draw (short of material), a zero
//! crew. The *client* renders the reasoned refusal; the sim just does not move.

use bevy::prelude::*;

use crate::{
    components::{BandBench, BandEquipment, DrawnInputs, LocalStore, PopulationCohort},
    crafting::{craft_discovery_id, HAND_WORKING_MATERIAL_EFFICIENCY},
    equipment_config::{EquipmentConfigHandle, EquipmentStat},
    intensification::LadderConfigHandle,
    materials_config::{MaterialsConfig, MaterialsConfigHandle},
    orders::FactionId,
    recipes_config::{RecipeDef, RecipesConfigHandle},
    resources::DiscoveryProgressLedger,
    scalar::{scalar_from_f32, scalar_zero, Scalar},
};

/// **One item finished charges one wear and one lesson.** Named so the two charges read off the same
/// number at the same site rather than each carrying a literal `1.0` that could be retuned apart.
const ONE_ITEM: f32 = 1.0;

/// **The tiers a band's bench resolves for one material** — the tool's if it has a live one, the
/// material's own bare-handed readings otherwise.
///
/// **The unequipped side of all three is a property of the MATERIAL, never of the absent tool** (one
/// home per fact — a tool that is not there cannot declare anything), which is why this is resolved
/// here rather than through `EquipmentConfig::two_tier`: that fallback searches the whole item table
/// and would answer the loom's numbers for a band scraping a hide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchTiers {
    /// Progress multiplier. `0` when the material cannot be worked bare-handed and no tool is
    /// present — the whole refusal.
    pub speed: f32,
    /// The best reading the output can realize.
    pub quality_ceiling: f32,
    /// The fraction of the recipe's stated input amounts a draw actually consumes.
    pub material_efficiency: f32,
    /// Whether a live tool supplied any of the above — what the wear charge is gated on.
    pub tooled: bool,
}

/// Resolve [`BenchTiers`] for `material` against this band's ledger. Public to the crate so the
/// forecast/readout side can quote exactly what the bench will pay.
pub fn bench_tiers(
    material: &str,
    materials: &MaterialsConfig,
    equipment: &crate::equipment_config::EquipmentConfig,
    wear: &BandEquipment,
) -> BenchTiers {
    let hand = materials
        .material(material)
        .and_then(|def| def.hand_working);
    let bare = BenchTiers {
        speed: hand.map_or(0.0, |hand| hand.rate),
        quality_ceiling: hand.map_or(0.0, |hand| hand.quality_ceiling),
        material_efficiency: HAND_WORKING_MATERIAL_EFFICIENCY,
        tooled: false,
    };
    let Some(tool) = equipment.live_bench_tool(material, wear) else {
        return bare;
    };
    // **Present effects apply; absent ones do not** — the same "only declared values participate"
    // clause `KitChoice::multiplier` runs on. A speed-only tool leaves the ceiling and the
    // efficiency exactly where the bare hand had them, and that is fine: no land is better at
    // speed, so a speed-only tool never touches the move/stay decision.
    BenchTiers {
        speed: tool
            .craft_stat(EquipmentStat::CraftSpeed)
            .unwrap_or(bare.speed),
        quality_ceiling: tool
            .craft_stat(EquipmentStat::CraftQualityCeiling)
            .unwrap_or(bare.quality_ceiling),
        material_efficiency: tool
            .craft_stat(EquipmentStat::CraftMaterialEfficiency)
            .unwrap_or(bare.material_efficiency),
        tooled: true,
    }
}

/// **How much of one input a pass consumes** — the recipe's stated amount after the tool's
/// efficiency. One helper, so the availability test and the withdrawal cannot disagree about the
/// number and leave the bench short mid-draw.
fn required(amount: f32, efficiency: f32) -> Scalar {
    scalar_from_f32(amount * efficiency)
}

/// **Draw a pass's inputs, or nothing at all.**
///
/// The availability test runs over **every** input before a single unit moves, which is what makes
/// *"a short draw withdraws nothing"* true rather than "withdraws until it runs out". Returns the
/// drawn reading on the recipe's `reads` axis (`None` for a recipe that reads nothing).
fn draw_inputs(
    store: &mut LocalStore,
    recipe: &RecipeDef,
    efficiency: f32,
    materials: &MaterialsConfig,
) -> Option<Option<f32>> {
    for input in &recipe.inputs {
        if store.material_total(&input.material) < required(input.amount, efficiency) {
            return None;
        }
    }
    let mut read_amount = scalar_zero();
    let mut read_weighted = 0.0f32;
    for input in &recipe.inputs {
        // **Worst-first on the axis this row is judged by** — the read axis where this row names
        // one, and the material's own **first declared axis** everywhere else. Either way the poor
        // stock is spent before the good, which is the only ordering that does not silently burn the
        // player's best hide on the first thing they make; the fallback only decides *which pile*,
        // never how much, and it has to be deterministic rather than right.
        let axis = input
            .reads
            .as_deref()
            .or_else(|| spend_axis_for(&input.material, materials))
            .unwrap_or_default();
        let draws = store.take_material(&input.material, axis, required(input.amount, efficiency));
        let Some(read) = input.reads.as_deref() else {
            continue;
        };
        for draw in draws {
            let reading = draw.characteristics.get(read).copied().unwrap_or_default();
            read_weighted += reading * draw.amount.to_f32();
            read_amount += draw.amount;
        }
    }
    let reading = (read_amount > scalar_zero()).then(|| read_weighted / read_amount.to_f32());
    Some(reading)
}

/// The material's first declared axis — the deterministic spend order for a row the recipe does not
/// read. `None` for a material the table does not carry, which `validate_against` makes unreachable.
fn spend_axis_for<'a>(material: &str, materials: &'a MaterialsConfig) -> Option<&'a str> {
    materials
        .material(material)?
        .characteristics
        .first()
        .map(String::as_str)
}

/// **The bench, once per band per turn.**
///
/// Scheduled after `advance_labor_allocation` so it draws on the materials **this turn's** take
/// delivered, and after the labor pass that owns the worker pool the bench's crew came out of.
#[allow(clippy::too_many_arguments)]
pub fn advance_crafting(
    mut bands: Query<(
        &mut PopulationCohort,
        &mut BandBench,
        &mut BandEquipment,
        &crate::components::BandId,
    )>,
    materials_handle: Res<MaterialsConfigHandle>,
    recipes_handle: Res<RecipesConfigHandle>,
    equipment_handle: Res<EquipmentConfigHandle>,
    ladder_handle: Res<LadderConfigHandle>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
) {
    let materials = materials_handle.get();
    let recipes = recipes_handle.get();
    let equipment = equipment_handle.get();
    let lesson = ladder_handle.get().knowledge.lesson_per_crafted_item;

    for (mut cohort, mut bench, mut wear, _) in bands.iter_mut() {
        let Some(recipe_id) = bench.recipe_id.clone() else {
            continue;
        };
        // A recipe the book no longer carries can only arrive through a config edit under a running
        // world. The bench stalls rather than clearing itself: the player chose this job, and
        // silently emptying their bench is a worse answer than a job that makes no progress.
        let Some(recipe) = recipes.recipe(&recipe_id) else {
            continue;
        };
        let Some(material) = recipe.bench_material() else {
            continue;
        };
        let tiers = bench_tiers(material, &materials, &equipment, &wear);
        let faction = cohort.faction;

        if bench.drawn.is_none() {
            if let Some(reading) = draw_inputs(
                &mut cohort.stores,
                recipe,
                tiers.material_efficiency,
                &materials,
            ) {
                bench.drawn = Some(fix_grade(recipe, reading, tiers.quality_ceiling));
            }
        }
        // Nothing drawn ⇒ nothing to work on. Not a branch on "can this be crafted": the pile is
        // simply not there yet.
        if bench.drawn.is_none() {
            continue;
        }

        let accrued = scalar_from_f32(
            bench.workers as f32 * recipes.crafting.progress_per_worker_turn * tiers.speed,
        );
        bench.progress += accrued;
        if bench.progress < scalar_from_f32(recipe.work) {
            continue;
        }

        emit_outputs(recipe, &mut cohort.stores, &mut wear, &materials);
        // **THE SAME QUANTUM, CHARGED ONCE, SIDE BY SIDE.** Splitting these across two sites is how
        // a tool that lasts 25 items ends up teaching a craft in 30.
        if tiers.tooled {
            if let Some((tool_id, _)) = equipment.bench_tool_for(material) {
                let tool_id = tool_id.to_string();
                wear.wear_item(&equipment, &tool_id, ONE_ITEM);
            }
        }
        credit_craft_lesson(&recipe.craft, lesson * ONE_ITEM, faction, &mut discovery);

        bench.items_completed = bench.items_completed.saturating_add(1);
        bench.last_output_grade = bench.drawn.as_ref().and_then(|drawn| drawn.grade.clone());
        // **The overflow is not carried.** Progress past `work` was done on an item whose materials
        // have not been drawn yet, so there is nothing for it to have been spent on — the same
        // shape as the ladder's `crew_scale`, where over-crewing buys nothing.
        bench.progress = scalar_zero();
        bench.drawn = None;
        if let Some(reading) = draw_inputs(
            &mut cohort.stores,
            recipe,
            tiers.material_efficiency,
            &materials,
        ) {
            bench.drawn = Some(fix_grade(recipe, reading, tiers.quality_ceiling));
        }
    }
}

/// **The grade, fixed here and never again.** `min(drawn reading, tool ceiling)` against the
/// recipe's own seams — fine flax with no loom still makes a standard basket.
fn fix_grade(recipe: &RecipeDef, reading: Option<f32>, ceiling: f32) -> DrawnInputs {
    let grade = reading
        .map(|reading| reading.min(ceiling))
        .and_then(|capped| recipe.grade_for(capped))
        .map(str::to_string);
    DrawnInputs { reading, grade }
}

/// Deliver one pass's outputs.
///
/// **Equipment lands through the ONE seam this ledger has**
/// ([`BandEquipment::restock`]): the band names the item and it is unworn. Condition, not count —
/// see that method for what the equipment-count slice has to re-point.
fn emit_outputs(
    recipe: &RecipeDef,
    store: &mut LocalStore,
    wear: &mut BandEquipment,
    materials: &MaterialsConfig,
) {
    for output in &recipe.outputs {
        if let Some(item) = output.equipment_id() {
            wear.restock(item);
        }
        if let Some(material) = output.material_id() {
            let Some(band) = materials.band_key(material, &output.characteristics) else {
                continue;
            };
            store.deposit_material(
                material,
                band,
                scalar_from_f32(output.amount),
                &output.characteristics,
            );
        }
    }
}

/// **Crafting is the fourth teacher**, and what is being made decides what is learned: the lesson is
/// the *recipe's* craft, never a fixed track.
///
/// A craft the coded set cannot resolve is skipped rather than panicking; the loaders' cross-config
/// checks are what make that unreachable.
fn credit_craft_lesson(
    craft: &str,
    amount: f32,
    faction: FactionId,
    discovery: &mut DiscoveryProgressLedger,
) {
    let Some(id) = craft_discovery_id(craft) else {
        return;
    };
    discovery.add_progress(faction, id, scalar_from_f32(amount));
}
