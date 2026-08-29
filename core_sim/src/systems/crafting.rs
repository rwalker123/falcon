//! **The bench** — one job at a time, per band (`docs/plan_crafting_and_materials.md` §5).
//!
//! Five steps, in this order and for these reasons:
//!
//! 1. **No recipe ⇒ nothing.** An idle bench is not a state that costs anything.
//! 2. **Nothing drawn ⇒ draw.** Withdraw each input's `amount × craft_material_efficiency`
//!    worst-first ([`LocalStore::take_material`]), record the exact reading on the recipe's `reads`
//!    axis **and what each row actually cost the store**, and **fix the grade there**. A short draw
//!    withdraws **nothing** — the turn is a no-op, not a half-spent pile.
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
    components::{
        BandBench, BandEquipment, BatchGrade, DrawnInputs, DrawnMaterial, LocalStore,
        PopulationCohort,
    },
    crafting::{craft_discovery_id, HAND_WORKING_MATERIAL_EFFICIENCY},
    equipment_config::{EquipmentConfigHandle, EquipmentStat},
    intensification::{knows, LadderConfigHandle, LadderKnowledge},
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

/// **Draw a pass's inputs and fix the grade, or draw nothing at all.**
///
/// The availability test runs over **every** input before a single unit moves, which is what makes
/// *"a short draw withdraws nothing"* true rather than "withdraws until it runs out". `None` is a
/// short draw: the store is untouched and the bench stays undrawn.
///
/// The returned pile carries **what the store actually lost**, per input row and in the recipe's own
/// order. The recipe's stated amount is not that number — the tool's material efficiency sits
/// between them — and the readout that names what a cleared job destroys has to name the real one.
fn draw_pass(
    store: &mut LocalStore,
    recipe: &RecipeDef,
    tiers: &BenchTiers,
    materials: &MaterialsConfig,
) -> Option<DrawnInputs> {
    let efficiency = tiers.material_efficiency;
    if !pass_is_affordable(store, recipe, tiers) {
        return None;
    }
    let mut reading = None;
    let mut withdrawn = Vec::with_capacity(recipe.inputs.len());
    for input in &recipe.inputs {
        let axis = spend_axis(input, materials);
        let draws = store.take_material(&input.material, axis, required(input.amount, efficiency));
        if let Some(read) = input.reads.as_deref() {
            reading = weighted_reading(&draws, read);
        }
        withdrawn.push(DrawnMaterial {
            material: input.material.clone(),
            // **Summed over the batches the draw touched**, because worst-first spends a row across
            // as many piles as it needs to fill it.
            amount: draws
                .iter()
                .map(|draw| draw.amount)
                .fold(scalar_zero(), |total, amount| total + amount),
        });
    }
    Some(fix_grade(
        recipe,
        reading,
        withdrawn,
        tiers.quality_ceiling,
        materials,
    ))
}

/// **Can the store pay for one pass right now?** — the availability half of [`draw_pass`], factored
/// out because the *projection* ([`bench_material_rate`]) has to ask the same question the draw will
/// ask and a second reading of it would be free to drift from the one that actually spends.
fn pass_is_affordable(store: &LocalStore, recipe: &RecipeDef, tiers: &BenchTiers) -> bool {
    recipe.inputs.iter().all(|input| {
        store.material_total(&input.material) >= required(input.amount, tiers.material_efficiency)
    })
}

/// **WHAT THE BENCH WILL BANK, PER TURN, PER MATERIAL** — the forward half of a band's
/// `material_upkeep_income` (`docs/plan_standing_upkeep.md` §2.7), and the **one producer** of it:
/// the wire's `material_upkeep_income` row and [`crate::systems::labor`]'s material-shortfall Alert
/// both read this, because a row and an event that summed the inflow apart would be free to
/// disagree about whether a band is running out.
///
/// A bench finishes one item when its meter crosses the recipe's `work`, so its per-turn output is
/// `rate_per_turn / work × the output's amount` — struck through the same
/// [`rate_per_turn`] [`advance_crafting`] accrues with, so the projection and the accrual cannot
/// describe different benches. `passes` is deliberately fractional: a bench two turns from an item
/// is making half of one a turn, which is exactly what a *rate* means.
///
/// **Only MATERIAL outputs count** — a bench making a sled adds nothing to a material ledger.
///
/// # ⛔ EMPTY UNLESS THE BENCH WOULD ACTUALLY BANK THIS TURN
///
/// [`advance_crafting`] banks nothing for a bench that has drawn no pile and cannot draw one, so a
/// rate published for it is income that never arrives. That is not a corner: on the shipped roster
/// `hurdles` have **no producer but a bench**, and a band with `hurdles` queued and no `wood` banks
/// zero for ever while the ledger read ~0.29/turn — which drove `material_upkeep_income` above
/// `material_upkeep_need`, printed a runway of `∞` and left the caret untinted in exactly the state
/// the standing bill exists to announce. The three gates, in the order [`advance_crafting`] applies
/// them: a recipe the book still carries, a crew (`rate_per_turn > 0`), and **a pile drawn or
/// affordable**.
pub fn bench_material_rate(
    bench: Option<&BandBench>,
    store: &LocalStore,
    recipes: &crate::recipes_config::RecipesConfig,
    materials: &MaterialsConfig,
    equipment: &crate::equipment_config::EquipmentConfig,
    wear: &BandEquipment,
) -> std::collections::BTreeMap<String, f32> {
    let mut rates = std::collections::BTreeMap::new();
    let Some(bench) = bench else {
        return rates;
    };
    let Some(recipe_id) = bench.recipe_id.as_deref() else {
        return rates;
    };
    // A recipe the book no longer carries — the bench stalls rather than clearing itself
    // ([`advance_crafting`]), and a stalled bench makes nothing.
    let Some(recipe) = recipes.recipe(recipe_id) else {
        return rates;
    };
    let Some(material) = recipe.bench_material() else {
        return rates;
    };
    let tiers = bench_tiers(material, materials, equipment, wear);
    let rate = rate_per_turn(bench.workers, &recipes.crafting, tiers.speed);
    if recipe.work <= 0.0 || rate <= 0.0 {
        return rates;
    }
    // **A pile already cut keeps the bench running** even if the store could not fund a *second*
    // pass — `advance_crafting` gates the draw, not the progress.
    if bench.drawn.is_none() && !pass_is_affordable(store, recipe, &tiers) {
        return rates;
    }
    let passes = rate / recipe.work;
    for output in &recipe.outputs {
        if let Some(material) = output.material_id() {
            *rates.entry(material.to_string()).or_insert(0.0) += output.amount * passes;
        }
    }
    rates
}

/// **What a bench accrues in one turn** — `workers × progress_per_worker_turn × craft_speed`.
///
/// **One authority**, because the sim applies it and the wire publishes it: a readout that recomputed
/// the product beside this one would be free to drop the `craft_speed` term, and that term is exactly
/// what makes a *worker-turn* not a worker's turn (bare-handed organics work at `0.5`, so two
/// crafters deliver one unit a turn, not two).
///
/// `0` is the whole refusal: no crew, or a material with no tool and no bare-handed rate.
pub fn rate_per_turn(
    workers: u32,
    crafting: &crate::recipes_config::CraftingTuning,
    speed: f32,
) -> f32 {
    workers as f32 * crafting.progress_per_worker_turn * speed
}

/// **Which axis a row is SPENT worst-first on** — the read axis where the row names one, and the
/// material's own **first declared axis** everywhere else.
///
/// Either way the poor stock goes before the good, which is the only ordering that does not silently
/// burn the player's best hide on the first thing they make. The fallback decides only *which pile*,
/// never how much, so it has to be deterministic rather than right.
///
/// ⛔ **`reads` IS OVERLOADED, and this is the half that survives everywhere.** It names the grade's
/// axis *and* this spend order. Deleting `reads` from a recipe to stop it quoting a grade would
/// silently move the spend order onto the material's first declared axis — for `hurdles` that is
/// hide by `toughness` instead of `suppleness`, a different pile eaten. The grade is gated in
/// [`RecipeDef::grade_for`] for exactly that reason.
fn spend_axis<'a>(
    input: &'a crate::recipes_config::RecipeInput,
    materials: &'a MaterialsConfig,
) -> &'a str {
    input
        .reads
        .as_deref()
        .or_else(|| spend_axis_for(&input.material, materials))
        .unwrap_or_default()
}

/// **The amount-weighted average of a set of draws on one axis** — the number the grade is selected
/// from. `None` when nothing came out, which is what an empty store gives.
fn weighted_reading(draws: &[crate::components::MaterialDraw], axis: &str) -> Option<f32> {
    let mut amount = scalar_zero();
    let mut weighted = 0.0f32;
    for draw in draws {
        let reading = draw.characteristics.get(axis).copied().unwrap_or_default();
        weighted += reading * draw.amount.to_f32();
        amount += draw.amount;
    }
    (amount > scalar_zero()).then(|| weighted / amount.to_f32())
}

/// **The grade a draw WOULD fix, without drawing** — what a published craft offer quotes.
///
/// It runs the same two steps the bench runs and in the same order — the store's own worst-first
/// spend order ([`LocalStore::preview_take_material`]), then `min(reading, ceiling)` against the
/// recipe's seams — so the number on the panel is the number the next completion is stamped with.
/// Anything less shared would let the two drift, and a grade that changes the moment you press Make
/// is worse than no grade at all.
///
/// `None` for a recipe that reads nothing, and for a band whose store holds none of the read
/// material: an offer that is short quotes no grade rather than quoting the grade of nothing.
///
/// **Also `None` for a recipe whose outputs are all materials**, via [`RecipeDef::grade_for`] — the
/// offer would otherwise quote a grade the bench never stamps, because a material batch has no
/// grade field. Such a recipe still declares `reads`, and it still means something: `reads` is
/// **overloaded**, naming both the axis a grade is read from *and* the axis the input is spent
/// worst-first on ([`spend_axis`]). Only the second half applies here, so the gate lives on the
/// grade and not on the config.
pub fn preview_grade(
    store: &LocalStore,
    recipe: &RecipeDef,
    tiers: &BenchTiers,
    materials: &MaterialsConfig,
) -> Option<String> {
    let input = recipe.reading_input()?;
    let axis = input.reads.as_deref()?;
    let draws = store.preview_take_material(
        &input.material,
        spend_axis(input, materials),
        required(input.amount, tiers.material_efficiency),
    );
    let reading = weighted_reading(&draws, axis)?;
    recipe
        .grade_for(reading.min(tiers.quality_ceiling), materials)
        .map(str::to_string)
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
    let ladder = ladder_handle.get();
    let practice_per_item = ladder.knowledge.craft_lesson_per_item;
    let knowledge_threshold = ladder.knowledge.completion_threshold;

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

        // ⛔ **AN UNSTAFFED BENCH DRAWS NOTHING.** The draw runs *before* the workers term is used,
        // so a bench at zero crew would keep withdrawing materials for a pass it can never work — a
        // famine quietly draining the material store into an idle bench. The crew can now reach zero
        // without the job ending (`LaborAllocation::normalize` stalls a bench rather than clearing
        // it), so this is a state the sim reaches and not a defensive check.
        //
        // **It gates the DRAW, not the pile.** A bench that had already drawn keeps what it cut —
        // the materials are the player's and the job is still theirs — and simply banks no progress,
        // which falls out of `rate_per_turn(0, …)` on its own.
        if bench.drawn.is_none() && bench.workers > AN_IDLE_BENCH {
            bench.drawn = draw_pass(&mut cohort.stores, recipe, &tiers, &materials);
        }
        // Nothing drawn ⇒ nothing to work on. Not a branch on "can this be crafted": the pile is
        // simply not there yet.
        if bench.drawn.is_none() {
            continue;
        }

        let accrued = scalar_from_f32(rate_per_turn(bench.workers, &recipes.crafting, tiers.speed));
        bench.progress += accrued;
        if bench.progress < scalar_from_f32(recipe.work) {
            continue;
        }

        // **The tier a craft comes out at is the best this faction knows** — resolved here, at the
        // moment of delivery, off the same ledger and the same completion threshold `set_bench`
        // gates a recipe on, so one reading of "does this people know that craft" serves both.
        let known = |craft: &str| {
            craft_discovery_id(craft)
                .is_some_and(|id| knows(&discovery, faction, id, knowledge_threshold))
        };
        let drawn_grade = bench.drawn.as_ref().and_then(|drawn| drawn.grade.clone());
        emit_outputs(
            recipe,
            drawn_grade.as_deref(),
            &mut cohort.stores,
            &mut wear,
            &materials,
            &equipment,
            &known,
        );
        // **THE SAME QUANTUM, CHARGED ONCE, SIDE BY SIDE.** Splitting these across two sites is how
        // a tool that lasts 25 items ends up teaching a craft in 30.
        if tiers.tooled {
            if let Some((tool_id, _)) = equipment.bench_tool_for(material) {
                let tool_id = tool_id.to_string();
                wear.wear_item(
                    &equipment,
                    &tool_id,
                    crate::equipment_config::WearQuantum::ItemCrafted,
                    ONE_ITEM,
                );
            }
        }
        credit_craft_lesson(
            &recipe.craft,
            practice_per_item * ONE_ITEM,
            &ladder.knowledge,
            faction,
            &mut discovery,
        );

        bench.items_completed = bench.items_completed.saturating_add(1);
        // **The grade the batch just delivered carries.** It was a readout with no reader until the
        // count slice; it is now the same string every batch of that craft is stamped with.
        bench.last_output_grade = drawn_grade;
        // **The overflow is not carried.** Progress past `work` was done on an item whose materials
        // have not been drawn yet, so there is nothing for it to have been spent on — the same
        // shape as the ladder's `crew_scale`, where over-crewing buys nothing.
        bench.progress = scalar_zero();
        bench.drawn = draw_pass(&mut cohort.stores, recipe, &tiers, &materials);
    }
}

/// **NOBODY AT THE BENCH** — the crew below which it may not draw. Named rather than a bare `0`
/// because the test is *"is anyone working this job"* and not a comparison with a magnitude.
const AN_IDLE_BENCH: u32 = 0;

/// **The grade, fixed here and never again.** The `characteristic_bands` rung `min(drawn reading,
/// tool ceiling)` falls in — excellent flax with no loom still makes a `good` basket, because the
/// bare hand's ceiling is what the band is capped at.
fn fix_grade(
    recipe: &RecipeDef,
    reading: Option<f32>,
    withdrawn: Vec<DrawnMaterial>,
    ceiling: f32,
    materials: &MaterialsConfig,
) -> DrawnInputs {
    let grade = reading
        .map(|reading| reading.min(ceiling))
        .and_then(|capped| recipe.grade_for(capped, materials))
        .map(str::to_string);
    DrawnInputs {
        reading,
        grade,
        withdrawn,
    }
}

/// Deliver one pass's outputs.
///
/// **Equipment lands as a NEW BATCH** ([`BandEquipment::stock`]) carrying `amount` units, the tier
/// the faction can reach, and the grade the draw fixed. It is never merged into a batch already
/// standing: *"the next ten are their own batch"* is what keeps a fresh craft from averaging into a
/// half-spent pile.
///
/// **`RecipeOutput::amount` is honoured** — a pass of a recipe that makes three makes three. It used
/// to deliver exactly one pass's worth of *condition* however many the row named, which a
/// count-bearing ledger has no excuse for.
fn emit_outputs(
    recipe: &RecipeDef,
    drawn_grade: Option<&str>,
    store: &mut LocalStore,
    wear: &mut BandEquipment,
    materials: &MaterialsConfig,
    equipment: &crate::equipment_config::EquipmentConfig,
    known: &dyn Fn(&str) -> bool,
) {
    for output in &recipe.outputs {
        if let Some(item) = output.equipment_id() {
            let Some(def) = equipment.item(item) else {
                continue;
            };
            // **The best tier this faction can reach**, which is the default one until something
            // gated is learned — so the shipped opening makes exactly what it always made.
            let tier = def.craftable_tier(known).id.clone();
            // **The grade's absolutes are copied HERE and carried on the batch**, which is what
            // makes "fixed at craft time and never moves" structural: a recipe retuned under a
            // running world cannot re-grade a sled already in the band's hands.
            //
            // **The batch is stamped with the BAND whether or not the recipe declares it** — the
            // grade is a real property of the object, and a recipe that declares nothing there
            // simply hands it an empty effect list (inherited from the highest rung at or below it).
            let grade = drawn_grade.map(|id| BatchGrade {
                id: id.to_string(),
                effects: recipe.grade_effects_for(id, materials).to_vec(),
            });
            wear.stock(item, whole_units(output.amount), &tier, grade);
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

/// **How many whole units an equipment output row delivers.** `validate` rejects a non-whole
/// `amount` on an equipment row — a ledger that counts things cannot bank half a spear — so this is
/// a cast rather than a rounding policy, and a negative or non-finite amount (also rejected) reads
/// as none rather than wrapping.
fn whole_units(amount: f32) -> u32 {
    if amount.is_finite() && amount > 0.0 {
        amount as u32
    } else {
        0
    }
}

/// **Crafting is the fourth teacher**, and what is being made decides what is learned: the lesson is
/// the *recipe's* craft, never a fixed track.
///
/// `practice` is in **practice units** — the same currency a rung's lesson is paid in, one quantum
/// over (per *item finished* rather than per *turn worked*) — and it becomes ledger progress through
/// the one [`LadderKnowledge::ledger_credit`] divisor, so a bench and a rung cannot come to disagree
/// about what a lesson costs.
///
/// A craft the coded set cannot resolve — or one the ladder does not price — is skipped rather than
/// panicking; the loaders' cross-config checks (`LadderConfig::validate`'s lesson-cost coverage among
/// them) are what make that unreachable.
fn credit_craft_lesson(
    craft: &str,
    practice: f32,
    knowledge: &LadderKnowledge,
    faction: FactionId,
    discovery: &mut DiscoveryProgressLedger,
) {
    let Some(id) = craft_discovery_id(craft) else {
        return;
    };
    let Some(amount) = knowledge.ledger_credit(craft, practice) else {
        return;
    };
    discovery.add_progress(faction, id, scalar_from_f32(amount));
}
