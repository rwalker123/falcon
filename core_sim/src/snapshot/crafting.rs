//! **Crafting on the wire** — the band's stock, its bench, its offers and its equipment life
//! (`docs/plan_crafting_and_materials.md` §7).
//!
//! # The sim resolves the refusal; the client renders it
//!
//! There is no *"you cannot craft that"* branch in the sim's **resolution** — every refusal is a
//! zero (a zero bench rate, a zero draw, a zero crew), exactly as `max(0, attack − defense)` refuses
//! a hunt. But a bench that silently does nothing is not an answer, so this module **publishes the
//! refusal as words**: one [`CraftOfferState`] per recipe, always, carrying a resolved `reason` and
//! a `severity`.
//!
//! **"Not needed yet" is a different string and a different severity from a shortage.** One is a
//! shrug and the other is a problem, and a client deriving both from a boolean cannot tell them
//! apart. That is the whole reason the reason is published rather than the inputs to it — the same
//! rule `kitTiers` exists to enforce.
//!
//! # What it costs
//!
//! `craftOffers` is bands × recipes, so everything that is a function of the **recipe alone** is
//! resolved once per capture into a [`CraftOfferPlan`] and the per-band pass only asks the band's
//! own questions: does it know the craft, has it the tool, has it the pile. The per-band pass is
//! then one [`BenchTiers`] per *material* (three, memoized) plus one store total per input row —
//! never a re-walk of the item table or the recipe book. `equipment.md` records capture going from
//! 49.51 ms to 3.15 ms when the estimate tables were retired; this must not give that back.

use std::collections::BTreeMap;

use sim_runtime::{
    BenchState, CharacteristicBandState, CharacteristicReadingState, CraftKnowledgeState,
    CraftOfferState, DrawnInputState, EquipmentBatchState, MaterialBatchState, MaterialDefState,
    MaterialShortfallState, RecipeDefState, RecipeInputState, RecipeOutputState,
};

use crate::{
    components::{BandBench, BandEquipment, LocalStore},
    crafting::{craft_discovery_id, title_from_id},
    equipment_config::{EquipmentConfig, EquipmentStat, WearQuantum},
    intensification::knows,
    materials_config::MaterialsConfig,
    orders::FactionId,
    recipes_config::{RecipeDef, RecipesConfig},
    resources::DiscoveryProgressLedger,
    scalar::scalar_from_f32,
    systems::{bench_tiers, preview_grade, rate_per_turn, BenchTiers},
};

/// The three severities a craft offer may carry. Opaque presentation tokens the sim never branches
/// on — named here so the one vocabulary has one home.
const SEVERITY_DANGER: &str = "danger";
const SEVERITY_NEUTRAL: &str = "neutral";
const SEVERITY_GOOD: &str = "good";

/// The three severities a life readout may carry. A separate vocabulary from the offers' on
/// purpose: a life bar is a fuel gauge and a craft offer is an invitation, so `good` would mean
/// nothing on one and `healthy` nothing on the other.
const LIFE_HEALTHY: &str = "healthy";
const LIFE_WARN: &str = "warn";
const LIFE_DANGER: &str = "danger";

/// **How two refusals read as one.** *"No crucible · no ore"* — the design's own separator.
const REASON_JOIN: &str = " · ";

/// A recipe row that makes something a party carries.
const GROUP_KIT: &str = "kit";
/// A recipe row that makes a bench tool — a thing that stretches one material.
const GROUP_TOOL: &str = "tool";
/// A recipe row that makes stock rather than kit.
const GROUP_STOCK: &str = "stock";

/// **The row is buildable and the band's stock of the thing is untouched** — a shrug, not a
/// shortage, and the panel dims it. Spelled once here because it is a *contract string*: a client
/// that saw a shortage's wording here would style a non-problem as a problem.
const REASON_NOT_NEEDED: &str = "Not needed yet";

/// **The bench has a job and nobody on it.** A contract string like [`REASON_NOT_NEEDED`], and for
/// the same reason: it is the *normal* state one click after **Make** — the player staffs the bench,
/// so this reads at [`SEVERITY_NEUTRAL`] while every other bench refusal is a problem.
const REASON_NO_CREW: &str = "No one at the bench";

/// **Nothing is blocking**, so there is no severity to state either. Spelled once so the empty
/// `blocked_reason` and the empty `blocked_severity` cannot drift apart.
const SEVERITY_NONE: &str = "";

/// **The life wording for an item the band has worn out.** Distinct from [`LIFE_NEVER_MADE`]
/// because both read `count 0` — a batch that runs out of units is removed — and *"your sled broke"*
/// is not the same sentence as *"you have never had a sled"*.
const LIFE_WORN_OUT: &str = "Worn out";
/// See [`LIFE_WORN_OUT`]. The band has never held one of these at all.
const LIFE_NEVER_MADE: &str = "Never made";
/// **Owned and unworn.** The end of the "sorted by urgency" list.
const LIFE_UNTOUCHED: &str = "Untouched";

/// Below this many quanta a life reads `~1 <singular> left` rather than `0 <plural> left` — the one
/// rung where the readout says *one*. A stock with any life left has at least *some* use in it, and
/// `0 kills left` on an item that still works is a lie in the alarming direction.
const APPROXIMATELY_ONE: f32 = 1.0;

/// **Everything about a recipe that does not depend on the band** — resolved once per capture and
/// read by every band, because `craftOffers` is bands × recipes and the recipe half of that product
/// is a constant.
pub(crate) struct CraftOfferPlan<'a> {
    id: &'a str,
    recipe: &'a RecipeDef,
    group: &'a str,
    output_item: Option<&'a str>,
    /// The material the bench works — whose tool is consulted and whose bare-handed rate applies.
    bench_material: Option<&'a str>,
    /// `(item id, display name)` of the tool that bounds [`Self::bench_material`], when the roster
    /// has one. It is what the *"No loom"* refusal names.
    tool: Option<(&'a str, &'a str)>,
    /// **The material this recipe's OUTPUT bounds**, when the output is a bench tool — what the
    /// tool *stretches*, which is deliberately never what it is *made from*. It is what the unlock
    /// sentence is about.
    bounds_material: Option<&'a str>,
    /// The material's own word, capitalized — *Hide*, *Fibre*, *Bone*.
    material_label: String,
}

/// Resolve the recipe-only half of every offer, in book order. One pass per capture.
pub(crate) fn plan_craft_offers<'a>(
    recipes: &'a RecipesConfig,
    equipment: &'a EquipmentConfig,
) -> Vec<CraftOfferPlan<'a>> {
    recipes
        .recipes()
        .map(|(id, recipe)| {
            let output_item = recipe.output_equipment_id();
            let bench_material = recipe.bench_material();
            CraftOfferPlan {
                id,
                recipe,
                group: group_of(recipe, equipment),
                output_item,
                bench_material,
                tool: bench_material.and_then(|material| {
                    equipment
                        .bench_tool_for(material)
                        .map(|(tool_id, _)| (tool_id, recipes.item_display_name(tool_id)))
                }),
                bounds_material: output_item
                    .and_then(|item| equipment.item(item))
                    .and_then(|def| def.bounds_material()),
                material_label: bench_material.map(title_from_id).unwrap_or_default(),
            }
        })
        .collect()
}

/// **Which of the ledger's three groups a recipe belongs in.** Derived, never authored: a recipe
/// that makes an item with a `bounds_material` *is* a tool row, and one that makes only a material
/// *is* a stock row. A `group` key in `recipes.json` would be a second statement of both.
fn group_of(recipe: &RecipeDef, equipment: &EquipmentConfig) -> &'static str {
    match recipe.output_equipment_id() {
        Some(item) => match equipment.item(item).and_then(|def| def.bounds_material()) {
            Some(_) => GROUP_TOOL,
            None => GROUP_KIT,
        },
        None => GROUP_STOCK,
    }
}

/// **The per-band inputs the whole crafting readout is resolved against**, bundled so the resolution
/// happens in one place rather than at the capture site — the same shape `BandKitLevers` has.
pub(crate) struct BandCraftInputs<'a> {
    pub(crate) materials: &'a MaterialsConfig,
    pub(crate) equipment: &'a EquipmentConfig,
    pub(crate) plans: &'a [CraftOfferPlan<'a>],
    /// Which crafts this band's faction knows, by craft id — resolved once per faction per capture
    /// rather than per recipe, because a discovery lookup is a map probe and there are ten recipes
    /// naming three crafts.
    pub(crate) known_crafts: &'a BTreeMap<String, bool>,
    /// The book's bench dials — `progress_per_worker_turn` is the one term of the bench's accrual
    /// that lives on no recipe row, carried here so [`bench_state`] can publish the whole product
    /// through the same [`crate::systems::rate_per_turn`] the bench applies.
    pub(crate) crafting: &'a crate::recipes_config::CraftingTuning,
    /// **The reference job an equipment life gauge is quoted against**, in work units — the ladder's
    /// [`crate::intensification::REFERENCE_BUILD_RUNG`] `work_cost`, resolved once per capture. See
    /// [`quantum_units_per_noun`] for why a build's wear cannot be counted in its own units.
    pub(crate) reference_build_cost: f32,
}

/// **The whole crafting half of one cohort's row**, resolved together because the four readouts
/// share the band's store, its ledger and its bench tiers — resolving them apart would walk the same
/// three structures four times.
pub(crate) struct BandCraftState {
    pub(crate) material_batches: Vec<MaterialBatchState>,
    pub(crate) bench: BenchState,
    pub(crate) craft_offers: Vec<CraftOfferState>,
    pub(crate) equipment_batches: Vec<EquipmentBatchState>,
}

pub(crate) fn band_craft_state(
    store: &LocalStore,
    bench: Option<&BandBench>,
    wear: &BandEquipment,
    inputs: &BandCraftInputs<'_>,
) -> BandCraftState {
    // **One `BenchTiers` per MATERIAL, not per recipe.** The tiers are a function of (material,
    // band), so ten recipes over three materials resolve three times.
    let mut tiers_by_material: BTreeMap<&str, BenchTiers> = BTreeMap::new();
    for plan in inputs.plans {
        if let Some(material) = plan.bench_material {
            tiers_by_material
                .entry(material)
                .or_insert_with(|| bench_tiers(material, inputs.materials, inputs.equipment, wear));
        }
    }
    let running = bench.and_then(|bench| bench.recipe_id.as_deref());
    let craft_offers = inputs
        .plans
        .iter()
        .map(|plan| {
            let tiers = plan
                .bench_material
                .and_then(|material| tiers_by_material.get(material).copied())
                .unwrap_or(NO_BENCH_TIERS);
            craft_offer(plan, &tiers, store, wear, inputs, running)
        })
        .collect();
    BandCraftState {
        material_batches: material_batches(store, inputs.materials),
        bench: bench_state(bench, store, inputs, &tiers_by_material),
        craft_offers,
        equipment_batches: equipment_batches(wear, inputs.equipment, inputs.reference_build_cost),
    }
}

/// **What a recipe with no bench material resolves to: nothing works.** `validate` makes a recipe
/// with no inputs unrepresentable, so this is only reachable through a config edit under a running
/// world — and a zero rate is the right answer there, because it is the same answer the bench gives.
const NO_BENCH_TIERS: BenchTiers = BenchTiers {
    speed: 0.0,
    quality_ceiling: 0.0,
    material_efficiency: crate::crafting::HAND_WORKING_MATERIAL_EFFICIENCY,
    tooled: false,
};

/// **What the band HAS, per rating** — amount, the exact per-axis reading, and the band that reading
/// falls in. Both, because the band is the merge key and the panel's word while the exact value is
/// what crafting reads.
fn material_batches(store: &LocalStore, materials: &MaterialsConfig) -> Vec<MaterialBatchState> {
    store
        .materials()
        .flat_map(|(material, batches)| {
            batches.values().map(move |batch| MaterialBatchState {
                material_id: material.to_string(),
                amount: batch.amount.to_f32(),
                // **In the material's DECLARED axis order**, not the batch map's — the order is part
                // of the material's contract and a readout that sorted alphabetically would put
                // `suppleness` before `toughness` on a hide and call it the first axis.
                readings: materials
                    .material(material)
                    .map(|def| def.characteristics.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|axis| {
                        let value = batch.characteristics.get(axis).copied().unwrap_or_default();
                        CharacteristicReadingState {
                            axis: axis.clone(),
                            value,
                            band_name: materials
                                .band_name(materials.band_index(value))
                                .unwrap_or_default()
                                .to_string(),
                        }
                    })
                    .collect(),
                variety_name: materials
                    .nearest_variety(material, &batch.characteristics)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

/// **A DRAWN job's shortfalls block nothing** — the pile is already withdrawn and in hand, so what
/// the store is short of cannot stop the item in flight; it speaks to the *next* draw, which is the
/// ledger's own offer row for that recipe. It is the rule [`bench_state`]'s `output_grade` follows
/// one field down: **this row is about the item being made**.
///
/// The shortfall does not move to the bench in a softer form — it leaves the bench entirely, because
/// a fact published in two places is one that can disagree with itself.
/// [`BenchState::shortfalls`] keeps being populated either way: it is honest data about the next
/// draw, and the client does not render it as blocking.
const NOTHING_SHORT_STOPS_A_DRAWN_PILE: &[MaterialShortfallState] = &[];

/// **What is on the bench, and why it is not moving.**
///
/// An idle bench publishes an all-default row rather than nothing, because *"idle"* and *"blocked"*
/// are different states and a client must be able to tell them apart without a second field.
///
/// **Once the pile is drawn only two things can stop the job**: nobody at the bench, and a zero
/// craft rate (the bounding tool wore out mid-craft on a material that cannot be worked bare-handed
/// — the one genuine *"a running job is stopped"* case). See [`NOTHING_SHORT_STOPS_A_DRAWN_PILE`].
fn bench_state(
    bench: Option<&BandBench>,
    store: &LocalStore,
    inputs: &BandCraftInputs<'_>,
    tiers_by_material: &BTreeMap<&str, BenchTiers>,
) -> BenchState {
    let Some(bench) = bench else {
        return BenchState::default();
    };
    let Some(recipe_id) = bench.recipe_id.as_deref() else {
        return BenchState::default();
    };
    let Some(plan) = inputs.plans.iter().find(|plan| plan.id == recipe_id) else {
        // A recipe the book no longer carries — reachable only through a config edit under a running
        // world, and the bench stalls rather than clearing itself. The row states the id so the
        // player sees what their bench is stuck on.
        return BenchState {
            recipe_id: recipe_id.to_string(),
            workers: bench.workers,
            blocked_reason: format!("Recipe '{recipe_id}' is not in the book"),
            blocked_severity: SEVERITY_DANGER.to_string(),
            ..BenchState::default()
        };
    };
    let tiers = plan
        .bench_material
        .and_then(|material| tiers_by_material.get(material).copied())
        .unwrap_or(NO_BENCH_TIERS);
    let shortfalls = shortfalls_for(plan.recipe, &tiers, store);
    // **The bench's blocked reason is the offer's refusal plus the crew's.** A bench with a full
    // pile and nobody on it is stopped too, and that is not a craft-offer question — the offer
    // answers *"could this be made"*, not *"is anyone making it"*.
    //
    // **Except a shortage, once the pile is DRAWN** — see [`NOTHING_SHORT_STOPS_A_DRAWN_PILE`].
    let blocking = if bench.drawn.is_some() {
        NOTHING_SHORT_STOPS_A_DRAWN_PILE
    } else {
        shortfalls.as_slice()
    };
    let mut reasons = refusal_reasons(plan, &tiers, blocking, inputs);
    // **A fault reads differently from a prompt, and only the sim can tell them apart.** Everything
    // `refusal_reasons` returns is something gone wrong — a shortage, an unlearned craft, a tool
    // that cannot work the material — so it carries the alarm. A bench merely waiting for its crew
    // is the *expected* state after a Make, because the player is the one who staffs it, so it is an
    // instruction instead. A joined reason takes the alarm if any half of it does.
    let mut severity = if reasons.is_empty() {
        SEVERITY_NONE
    } else {
        SEVERITY_DANGER
    };
    if bench.workers == 0 {
        reasons.push(REASON_NO_CREW.to_string());
        if severity == SEVERITY_NONE {
            severity = SEVERITY_NEUTRAL;
        }
    }
    BenchState {
        recipe_id: recipe_id.to_string(),
        display_name: plan.recipe.display_name.clone(),
        workers: bench.workers,
        progress: bench.progress.to_f32(),
        work: plan.recipe.work,
        teaches: plan.recipe.craft.clone(),
        blocked_reason: reasons.join(REASON_JOIN),
        blocked_severity: severity.to_string(),
        shortfalls,
        items_completed: bench.items_completed,
        drawn: bench.drawn.is_some(),
        // **The grade the pile in flight FIXED**, not the one the next draw would pick — the two
        // differ the moment the store changes under a running job, and this row is about the item
        // being made.
        output_grade: bench
            .drawn
            .as_ref()
            .and_then(|drawn| drawn.grade.clone())
            .unwrap_or_default(),
        // **The accrual itself, not its parts.** `craft_speed` is the tool-or-bare-hand join, and a
        // client that multiplied `workers × work` would read a bare-handed organic's `0.5` as `1.0`
        // and promise a finish in half the turns it takes. Resolved through the same helper
        // `advance_crafting` accrues with, so the published rate is the applied rate.
        rate_per_turn: rate_per_turn(bench.workers, inputs.crafting, tiers.speed),
        // **What the store already lost for the job in flight** — the withdrawn amounts, not the
        // recipe's stated inputs, so a clear or a swap can name what it destroys. Empty on an
        // undrawn bench, which is the honest answer: nothing has been cut yet.
        drawn_inputs: bench
            .drawn
            .as_ref()
            .map(|drawn| {
                drawn
                    .withdrawn
                    .iter()
                    .map(|input| DrawnInputState {
                        material_id: input.material.clone(),
                        amount: input.amount.to_f32(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// **What one pass of `recipe` is short**, after the tool's material efficiency. Empty when the pile
/// is there — a published row always has `short > 0`.
fn shortfalls_for(
    recipe: &RecipeDef,
    tiers: &BenchTiers,
    store: &LocalStore,
) -> Vec<MaterialShortfallState> {
    recipe
        .inputs
        .iter()
        .filter_map(|input| {
            let required = scalar_from_f32(input.amount * tiers.material_efficiency);
            let held = store.material_total(&input.material);
            (held < required).then(|| MaterialShortfallState {
                material_id: input.material.clone(),
                required: required.to_f32(),
                held: held.to_f32(),
                short: (required - held).to_f32(),
            })
        })
        .collect()
}

/// **Every reason this band cannot make a pass right now**, in the order the design lists them:
/// the craft, then the tool, then the material. Empty means it can.
fn refusal_reasons(
    plan: &CraftOfferPlan<'_>,
    tiers: &BenchTiers,
    shortfalls: &[MaterialShortfallState],
    inputs: &BandCraftInputs<'_>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    for craft in &plan.recipe.requires_knowledge {
        if !inputs.known_crafts.get(craft).copied().unwrap_or(false) {
            reasons.push(format!("Needs {}", title_from_id(craft)));
        }
    }
    // **A zero rate IS the refusal** — no tool and a material that cannot be worked bare-handed.
    // The sentence names the tool, because *"you cannot craft that"* tells the player nothing they
    // can act on and *"no loom"* tells them exactly what to build.
    if tiers.speed <= 0.0 {
        reasons.push(match plan.tool {
            Some((_, display)) => format!("No {}", display.to_lowercase()),
            None => format!("{} cannot be worked", plan.material_label),
        });
    }
    for shortfall in shortfalls {
        // **The NUMBER, never "cannot craft".** One decimal, because a material accumulates
        // fractionally and *"short 5 bone"* on a band holding 4.9 would be a lie by a whole unit.
        reasons.push(format!(
            "Short {:.1} {}",
            shortfall.short, shortfall.material_id
        ));
    }
    reasons
}

/// **One recipe, offered or refused, for one band.**
fn craft_offer(
    plan: &CraftOfferPlan<'_>,
    tiers: &BenchTiers,
    store: &LocalStore,
    wear: &BandEquipment,
    inputs: &BandCraftInputs<'_>,
    running: Option<&str>,
) -> CraftOfferState {
    let shortfalls = shortfalls_for(plan.recipe, tiers, store);
    let refusals = refusal_reasons(plan, tiers, &shortfalls, inputs);
    let available = refusals.is_empty();
    let output_grade =
        preview_grade(store, plan.recipe, tiers, inputs.materials).unwrap_or_default();
    // **The head and the cell are resolved together**, because the note is only news relative to the
    // head — the two disagreeing is the whole readout. The head is resolved *first* because the
    // invitation quotes what the tier being made would unlock.
    let craftable = craftable_tier(plan, inputs);
    let (output_tier_name, output_tier_rank) = craftable
        .map(|(id, rank)| (id.to_string(), rank))
        .unwrap_or_default();
    let (reason, severity) = if available {
        invitation(
            plan,
            tiers,
            wear,
            &output_grade,
            inputs,
            craftable.map(|(id, _)| id),
        )
    } else {
        (refusals.join(REASON_JOIN), SEVERITY_DANGER)
    };
    CraftOfferState {
        recipe_id: plan.id.to_string(),
        display_name: plan.recipe.display_name.clone(),
        group: plan.group.to_string(),
        output_item_id: plan.output_item.unwrap_or_default().to_string(),
        available,
        reason,
        severity: severity.to_string(),
        shortfalls,
        output_grade,
        on_bench: running == Some(plan.id),
        output_tier_name,
        output_tier_rank,
        owned_note: craftable
            .map(|(_, rank)| owned_note(plan, wear, rank, inputs))
            .unwrap_or_default(),
    }
}

/// **The tier a craft would produce right now, and its rank in the item's own list** — the ledger's
/// group head. `None` for a recipe that makes a material rather than an item, which has no tier to
/// be grouped under.
///
/// It is resolved **per band** rather than in the [`CraftOfferPlan`] because `craftable_tier` reads
/// what the *faction* knows, and the plan is a per-capture constant. The walk is over one item's
/// tiers — one on the shipped roster — so it costs nothing.
fn craftable_tier<'a>(
    plan: &CraftOfferPlan<'a>,
    inputs: &BandCraftInputs<'a>,
) -> Option<(&'a str, u32)> {
    let def = inputs.equipment.item(plan.output_item?)?;
    let known = |craft: &str| inputs.known_crafts.get(craft).copied().unwrap_or(false);
    let tier = def.craftable_tier(known);
    let rank = def.tiers.iter().position(|row| row.id == tier.id)?;
    Some((tier.id.as_str(), rank as u32))
}

/// **WHAT THE BAND CARRIES, SAID ONLY WHEN IT IS NEWS** — `""` whenever nothing the band holds is
/// older than what it could now make, which is every row on the shipped one-tier roster.
///
/// Two sentences, and they answer different questions:
///
/// - **units in hand at an older tier** → `carrying flint · poor`. Several such batches name the
///   **worst** grade, because naming the best is the one a player would be told about last — a row
///   that flattered its stock would be telling them the opposite of what they need to act on.
/// - **no units at all, and a set retired at an older tier** → `last flint set wore out`. The tier is
///   read out of `BandEquipment::retired_tiers_of`, which `wear_item` keys by the tier of the unit it
///   destroyed — **never inferred from `craftable_tier`'s neighbour**. With iron beside bronze and
///   flint, *"the rank below what I can now make"* names bronze for a flint set that actually wore
///   out, and a published string asserting the wrong tier is worse than saying nothing. Of several
///   retired tiers it names the **highest-ranked one still below** what the band can now make: that
///   is the set it lost most recently.
fn owned_note(
    plan: &CraftOfferPlan<'_>,
    wear: &BandEquipment,
    craftable_rank: u32,
    inputs: &BandCraftInputs<'_>,
) -> String {
    let Some(item) = plan.output_item else {
        return String::new();
    };
    let Some(def) = inputs.equipment.item(item) else {
        return String::new();
    };
    let rank_of = |tier: &str| def.tiers.iter().position(|row| row.id == tier);
    // **Worst grade first**, by the band it names; an ungraded batch is a start-stocked unit, which
    // makes no quality claim at all and therefore sorts ahead of every one that does.
    let carried = wear
        .batches_of(item)
        .iter()
        .filter_map(|batch| rank_of(&batch.tier).map(|rank| (rank, batch)))
        .filter(|(rank, _)| (*rank as u32) < craftable_rank)
        .min_by_key(|(rank, batch)| {
            let grade = batch
                .grade
                .as_ref()
                .and_then(|grade| inputs.materials.band_index_of(&grade.id));
            (grade, *rank)
        });
    if let Some((rank, batch)) = carried {
        let tier = tier_word(&def.tiers[rank].id);
        return match batch.grade.as_ref().filter(|grade| !grade.id.is_empty()) {
            Some(grade) => format!("carrying {tier}{REASON_JOIN}{}", grade.id),
            None => format!("carrying {tier}"),
        };
    }
    if wear.count_of(item) > 0 {
        return String::new();
    }
    // **The tier that actually wore out**, newest-lost first — not the neighbour of what the band can
    // now make.
    let lost = wear
        .retired_tiers_of(item)
        .filter(|(_, count)| *count > 0)
        .filter_map(|(tier, _)| rank_of(tier))
        .filter(|rank| (*rank as u32) < craftable_rank)
        .max();
    match lost {
        Some(rank) => format!("last {} set wore out", tier_word(&def.tiers[rank].id)),
        None => String::new(),
    }
}

/// **THE BAND A TOOL WOULD UNLOCK — its own `craft_quality_ceiling`, never the top of the ladder.**
///
/// Resolved off the tier the bench would actually make (`made_tier`, the faction's `craftable_tier`),
/// through the **same** `band_index` lookup `fix_grade` runs, so the invitation names exactly the
/// grade the bench will then produce. Quoting the top band instead is right only while every tool
/// happens to declare `0.90`: a tool with a ceiling of `0.70` would advertise `excellent` work while
/// capping the band at `good` — the panel promising a grade the bench cannot make.
///
/// `None` for a recipe that makes no item, or a tool declaring no ceiling at all — *present effects
/// apply, absent ones do not*.
fn unlocked_band<'a>(
    plan: &CraftOfferPlan<'_>,
    made_tier: Option<&str>,
    inputs: &BandCraftInputs<'a>,
) -> Option<&'a str> {
    let def = inputs.equipment.item(plan.output_item?)?;
    let tier = made_tier.map_or_else(|| def.default_tier(), |id| def.tier_or_default(id));
    let ceiling = def.tier_craft_stat(tier, EquipmentStat::CraftQualityCeiling)?;
    inputs
        .materials
        .band_name(inputs.materials.band_index(ceiling))
}

/// **A tier's player-facing word, lowercased for mid-sentence use.** Resolved through
/// [`title_from_id`] like every other id in this model — `equipment.json` authors no display name,
/// and a second spelling of `flint` here is a second thing to keep in step.
fn tier_word(tier: &str) -> String {
    title_from_id(tier).to_lowercase()
}

/// **What a buildable row says.** Three rungs, most specific first:
///
/// 1. **A first tool** — `Unlocks excellent fibre work`, `good`. The one row on the panel that is an
///    opportunity rather than a chore, and it is only true once: the band owns none of it yet. The
///    adjective is **the band THIS tool's own `craft_quality_ceiling` falls in** ([`unlocked_band`]),
///    resolved through the same lookup the draw uses to fix a grade — so the invitation and the
///    outcome cannot disagree.
/// 2. **Nothing to replace** — `Not needed yet`, `neutral` and dimmed. A shrug, and deliberately a
///    different string and severity from a shortage.
/// 3. **Otherwise** — what the pile and the bench would make of it, `Hide + tanning frame →
///    excellent`, with the tool's material saving appended when it has one.
fn invitation(
    plan: &CraftOfferPlan<'_>,
    tiers: &BenchTiers,
    wear: &BandEquipment,
    output_grade: &str,
    inputs: &BandCraftInputs<'_>,
    made_tier: Option<&str>,
) -> (String, &'static str) {
    let owned = plan
        .output_item
        .map(|item| wear.count_of(item))
        .unwrap_or(0);
    if plan.group == GROUP_TOOL && owned == 0 {
        let material = plan.bounds_material.unwrap_or_default();
        let band = unlocked_band(plan, made_tier, inputs).unwrap_or_default();
        return (format!("Unlocks {band} {material} work"), SEVERITY_GOOD);
    }
    // **A shrug, not a shortage.** The band holds units of the thing and has spent nothing on them,
    // so there is nothing to replace — a different string AND a different severity from a refusal,
    // which is the whole reason the reason is published.
    if owned > 0 && wear.wear_of(plan.output_item.unwrap_or_default()) <= 0.0 {
        return (REASON_NOT_NEEDED.to_string(), SEVERITY_NEUTRAL);
    }
    let mut reason = plan.material_label.clone();
    match (plan.tool, tiers.tooled) {
        (Some((_, display)), true) => reason.push_str(&format!(" + {}", display.to_lowercase())),
        (Some((_, display)), false) => reason.push_str(&format!(", no {}", display.to_lowercase())),
        (None, _) => {}
    }
    if !output_grade.is_empty() {
        reason.push_str(&format!(" → {output_grade}"));
    }
    // **What the tool saves, as the number it saves.** `material_efficiency` is the *fraction
    // consumed*, so a `0.8` tool reads `−20%` — the saving, not the fraction, because the row is
    // about what the player gets for having built the thing.
    if tiers.tooled && tiers.material_efficiency < 1.0 {
        let saving: f32 = ((1.0 - tiers.material_efficiency) * 100.0).round();
        reason.push_str(REASON_JOIN);
        reason.push_str(&format!(
            "{} costs \u{2212}{saving:.0}%",
            plan.material_label
        ));
    }
    (reason, SEVERITY_NEUTRAL)
}

/// **HOW MANY OF A QUANTUM'S OWN UNITS ONE OF ITS NOUNS IS** — the divisor that turns a raw
/// condition-over-wear count into the sentence a player can act on.
///
/// **`1.0` for every quantum whose unit IS its noun**: a kill is a kill, a hauled biomass is a hauled
/// biomass, a finished craft is a craft. [`WearQuantum::BuildProgress`] is the one exception, and it
/// became one when improvements were priced in work (`docs/plan_unit_costed_work.md` §6.3): its unit
/// is a **work unit** — one worker-turn at the food peak — while a player holding a hoe thinks in
/// *builds*. So it is quoted against the **reference job**
/// ([`crate::intensification::REFERENCE_BUILD_RUNG`], the `plant:tended` rung's own `work_cost`) and
/// its noun reads *"gardens' worth"*.
///
/// **The reference is the LADDER's, resolved here rather than carried as a literal**, so a retune of
/// the rung moves the readout with it — and it lives at the readout rather than on `WearQuantum`
/// because that table knows nothing about rungs.
fn quantum_units_per_noun(quantum: WearQuantum, reference_build_cost: f32) -> f32 {
    match quantum {
        WearQuantum::BuildProgress => reference_build_cost,
        WearQuantum::Strike
        | WearQuantum::BiomassHauled
        | WearQuantum::BiomassGathered
        | WearQuantum::BiomassCollected
        | WearQuantum::TileRevealed
        | WearQuantum::ItemCrafted => ONE_UNIT_IS_ITS_OWN_NOUN,
    }
}

/// The divisor of a quantum whose unit already *is* the thing the readout counts.
const ONE_UNIT_IS_ITS_OWN_NOUN: f32 = 1.0;

/// **What the band owns of each item, and how much life is in it.**
///
/// One row per **batch**, plus one `count: 0` row for every config item the band owns none of — so
/// the ledger always has a row per item and one can never simply go missing. Config-driven for the
/// same reason `kitItemConditions` is.
fn equipment_batches(
    wear: &BandEquipment,
    equipment: &EquipmentConfig,
    reference_build_cost: f32,
) -> Vec<EquipmentBatchState> {
    equipment
        .items()
        .flat_map(|(item, def)| {
            let batches = wear.batches_of(item);
            if batches.is_empty() {
                // **Worn out and never made are different states**, and this is the only place they
                // can be told apart: both read `count 0`, because a batch that runs out of units is
                // removed from the ledger entirely.
                let retired = wear.retired_of(item);
                return vec![EquipmentBatchState {
                    item_id: item.to_string(),
                    quantum_noun: def.headline_wear().per.noun().to_string(),
                    life: if retired > 0 {
                        LIFE_WORN_OUT.to_string()
                    } else {
                        LIFE_NEVER_MADE.to_string()
                    },
                    life_severity: if retired > 0 {
                        LIFE_DANGER.to_string()
                    } else {
                        LIFE_WARN.to_string()
                    },
                    ..EquipmentBatchState::default()
                }];
            }
            batches
                .iter()
                .map(|batch| {
                    let tier = def.tier_or_default(&batch.tier);
                    // **A batch of `count` holds `count × starting_durability` of life**, spent one
                    // unit at a time — so the quanta left is the whole batch's, not the serving
                    // unit's. That is what makes it a fuel gauge for a stockpile rather than for one
                    // spear.
                    let condition_left =
                        (tier.starting_durability * batch.count as f32 - batch.wear).max(0.0);
                    // **The wear rate is per QUANTUM UNIT; the readout counts NOUNS**, so the
                    // divisor carries both (`quantum_units_per_noun` — `1` for every quantum but
                    // the build's, whose unit is a work unit and whose noun is a whole garden).
                    let per_noun = def.headline_wear().amount
                        * quantum_units_per_noun(def.headline_wear().per, reference_build_cost);
                    let quanta_left = if per_noun > 0.0 {
                        condition_left / per_noun
                    } else {
                        0.0
                    };
                    let full_unit_quanta = if per_noun > 0.0 {
                        tier.starting_durability / per_noun
                    } else {
                        0.0
                    };
                    let fraction = if full_unit_quanta > 0.0 {
                        quanta_left / full_unit_quanta
                    } else {
                        0.0
                    };
                    EquipmentBatchState {
                        item_id: item.to_string(),
                        tier_id: batch.tier.clone(),
                        grade: batch
                            .grade
                            .as_ref()
                            .map(|grade| grade.id.clone())
                            .unwrap_or_default(),
                        count: batch.count,
                        remaining: (tier.starting_durability - batch.wear).max(0.0),
                        quanta_left,
                        quantum_noun: def.headline_wear().per.noun().to_string(),
                        life: life_wording(batch.wear, quanta_left, def.headline_wear().per),
                        life_severity: life_severity(fraction, equipment).to_string(),
                    }
                })
                .collect()
        })
        .collect()
}

/// **The row's wording, in the item's own use quanta and never in percent.**
///
/// An unworn batch reads `Untouched` rather than a count, because the count is not the point when
/// nothing has been spent — *"sorted by urgency, untouched last"* is the panel's rule and this is
/// the word it sorts on.
fn life_wording(worn: f32, quanta_left: f32, quantum: WearQuantum) -> String {
    if worn <= 0.0 {
        return LIFE_UNTOUCHED.to_string();
    }
    if quanta_left < APPROXIMATELY_ONE {
        return format!("~1 {} left", quantum.singular_noun());
    }
    format!("{:.0} {} left", quanta_left, quantum.noun())
}

/// The bar's colour, off the two seams in `equipment.json`'s `life_readout`. See
/// [`crate::equipment_config::LifeReadoutConfig`] for why they are fractions of one fresh unit.
fn life_severity(fraction: f32, equipment: &EquipmentConfig) -> &'static str {
    if fraction < equipment.life_readout.danger_fraction {
        LIFE_DANGER
    } else if fraction < equipment.life_readout.warn_fraction {
        LIFE_WARN
    } else {
        LIFE_HEALTHY
    }
}

/// **Which crafts a faction knows**, by craft id — resolved once per faction per capture. The offer
/// pass asks it ten times per band, and a discovery lookup per ask would be ten map probes for three
/// distinct answers.
pub(crate) fn known_crafts(
    materials: &MaterialsConfig,
    discovery: &DiscoveryProgressLedger,
    faction: FactionId,
    threshold: f32,
) -> BTreeMap<String, bool> {
    crate::crafting::crafts_declared_by(materials)
        .into_iter()
        .map(|craft| {
            let known = craft_discovery_id(craft)
                .is_some_and(|id| knows(discovery, faction, id, threshold));
            (craft.to_string(), known)
        })
        .collect()
}

/// **The materials catalogue** — a per-world constant, published so the panel names a material's
/// craft and axes without a second copy of the table.
pub(crate) fn material_catalogue(
    materials: &MaterialsConfig,
    equipment: &EquipmentConfig,
) -> Vec<MaterialDefState> {
    materials
        .materials()
        .map(|(id, def)| MaterialDefState {
            id: id.to_string(),
            // `""` = **nothing works this material yet** (the `displayName` convention), never a
            // craft the client should look up. See `MaterialDef::craft`.
            craft: def.craft.clone().unwrap_or_default(),
            axes: def.characteristics.clone(),
            hand_workable: def.is_hand_workable(),
            hand_working_rate: def.hand_working_rate(),
            hand_working_quality_ceiling: def.hand_working.map_or(0.0, |hand| hand.quality_ceiling),
            tool_item_id: equipment
                .bench_tool_for(id)
                .map(|(tool, _)| tool.to_string())
                .unwrap_or_default(),
        })
        .collect()
}

/// **The rating vocabulary, once for the world** — the legend, not the lookup: every published
/// reading already carries its own band name.
pub(crate) fn characteristic_band_catalogue(
    materials: &MaterialsConfig,
) -> Vec<CharacteristicBandState> {
    materials
        .characteristic_bands
        .iter()
        .map(|band| CharacteristicBandState {
            name: band.name.clone(),
            from: band.from,
        })
        .collect()
}

/// **The recipe book** — a per-world constant. The band-relative half (can it be made, what would it
/// come out at, what is missing) is each cohort's own `craft_offers`.
pub(crate) fn recipe_catalogue(
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
) -> Vec<RecipeDefState> {
    recipes
        .recipes()
        .map(|(id, recipe)| RecipeDefState {
            id: id.to_string(),
            display_name: recipe.display_name.clone(),
            craft: recipe.craft.clone(),
            group: group_of(recipe, equipment).to_string(),
            work: recipe.work,
            requires_knowledge: recipe.requires_knowledge.clone(),
            inputs: recipe
                .inputs
                .iter()
                .map(|input| RecipeInputState {
                    material_id: input.material.clone(),
                    amount: input.amount,
                    reads_axis: input.reads.clone().unwrap_or_default(),
                })
                .collect(),
            outputs: recipe
                .outputs
                .iter()
                .map(|output| RecipeOutputState {
                    equipment_id: output.equipment_id().unwrap_or_default().to_string(),
                    material_id: output.material_id().unwrap_or_default().to_string(),
                    amount: output.amount,
                })
                .collect(),
        })
        .collect()
}

/// **Per faction, per craft.** Every faction the ledger carries times every craft the materials
/// table declares — the same shape `snapshot_intensification_knowledge` publishes the ladder's five
/// in, and for the same reason: a client reads its own faction's rows off the list.
pub(crate) fn craft_knowledge_states(
    materials: &MaterialsConfig,
    discovery: &DiscoveryProgressLedger,
    threshold: f32,
) -> Vec<CraftKnowledgeState> {
    let mut factions: Vec<u32> = discovery.progress.keys().map(|faction| faction.0).collect();
    factions.sort_unstable();
    factions.dedup();
    let crafts = crate::crafting::crafts_declared_by(materials);
    factions
        .into_iter()
        .flat_map(|faction_id| {
            let faction = FactionId(faction_id);
            crafts.iter().filter_map(move |craft| {
                let id = craft_discovery_id(craft)?;
                Some(CraftKnowledgeState {
                    faction: faction_id,
                    craft_id: (*craft).to_string(),
                    display_name: title_from_id(craft),
                    known: knows(discovery, faction, id, threshold),
                    progress: discovery.get_progress(faction, id).to_f32(),
                    completion_threshold: threshold,
                })
            })
        })
        .collect()
}

#[cfg(test)]
/// **The builtin crafting levers as a `'static` bundle** — what every `population_state` fixture in
/// this crate passes, so a fixture asserting on the food ledger does not have to assemble a recipe
/// book to get one. Nothing is known: none of the three crafts ships known, which is the opening
/// state of every campaign.
pub(crate) fn builtin_craft_inputs() -> &'static BandCraftInputs<'static> {
    use std::sync::OnceLock;
    static MATERIALS: OnceLock<std::sync::Arc<MaterialsConfig>> = OnceLock::new();
    static RECIPES: OnceLock<std::sync::Arc<RecipesConfig>> = OnceLock::new();
    static EQUIPMENT: OnceLock<std::sync::Arc<EquipmentConfig>> = OnceLock::new();
    static PLANS: OnceLock<Vec<CraftOfferPlan<'static>>> = OnceLock::new();
    static KNOWN: OnceLock<BTreeMap<String, bool>> = OnceLock::new();
    static LADDER: OnceLock<std::sync::Arc<crate::intensification::LadderConfig>> = OnceLock::new();
    static INPUTS: OnceLock<BandCraftInputs<'static>> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let materials: &'static MaterialsConfig = MATERIALS.get_or_init(MaterialsConfig::builtin);
        let recipes: &'static RecipesConfig = RECIPES.get_or_init(RecipesConfig::builtin);
        let equipment: &'static EquipmentConfig = EQUIPMENT.get_or_init(EquipmentConfig::builtin);
        BandCraftInputs {
            materials,
            equipment,
            plans: PLANS.get_or_init(|| plan_craft_offers(recipes, equipment)),
            known_crafts: KNOWN.get_or_init(BTreeMap::new),
            crafting: &recipes.crafting,
            reference_build_cost: LADDER
                .get_or_init(crate::intensification::LadderConfig::builtin)
                .reference_build_cost(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment_config::WearQuantum;

    /// **THE BUILD QUANTUM IS THE ONE WHOSE UNIT IS NOT ITS NOUN**, and this pins the divisor that
    /// turns one into the other — `quantum_units_per_noun`'s only non-identity arm.
    ///
    /// **It is unreachable from the shipped roster today**, which is exactly why it is pinned here
    /// rather than left to a wire test: `husbandry_gear` is the only item that wears on a build and
    /// it declares `biomass_collected` first, so no published `equipmentBatches` row currently
    /// divides by anything. The arm goes live the day an item **leads** with `build_progress` (issue
    /// #539's hoe), and an untested conversion would quote that item's whole life in bare work units.
    #[test]
    fn the_build_quanta_are_quoted_against_the_ladders_reference_job() {
        let reference = crate::intensification::LadderConfig::builtin().reference_build_cost();
        assert!(
            reference > 0.0,
            "fixture: the ladder must price its reference job, or the divisor is meaningless"
        );
        assert_eq!(
            quantum_units_per_noun(WearQuantum::BuildProgress, reference),
            reference,
            "a build's noun is the REFERENCE JOB, so its divisor is that job's own work cost — a \
             work unit means nothing to a player holding a hoe"
        );
        for quantum in [
            WearQuantum::Strike,
            WearQuantum::BiomassHauled,
            WearQuantum::BiomassGathered,
            WearQuantum::BiomassCollected,
            WearQuantum::TileRevealed,
            WearQuantum::ItemCrafted,
        ] {
            assert_eq!(
                quantum_units_per_noun(quantum, reference),
                ONE_UNIT_IS_ITS_OWN_NOUN,
                "{quantum:?} counts in its own unit — a kill is a kill"
            );
        }
    }

    /// **THE SHIPPED HANDLING GEAR HEADLINES THE SLAUGHTER, NOT THE BUILD** — `wear[0]` is the
    /// entry a life gauge quotes (`ItemDefinition::headline_wear`), and the order is the model.
    ///
    /// Pinned because it is a **readout** decision a config reorder would silently make: the gauge
    /// would flip from *"≈2500 biomass butchered"* to *"≈12 gardens' worth"*, quoting a keeper's kit
    /// in the plant web's reference job. `docs/plan_unit_costed_work.md` §6.3 assumed this item
    /// already led with the build; it never has.
    #[test]
    fn the_handling_gears_life_gauge_reads_in_butchered_biomass() {
        let equipment = EquipmentConfig::builtin();
        let gear = equipment
            .item("husbandry_gear")
            .expect("the shipped roster carries the handling gear");
        assert_eq!(
            gear.headline_wear().per,
            WearQuantum::BiomassCollected,
            "the handling gear's gauge reads in butchered biomass"
        );
        assert!(
            gear.wears_on(WearQuantum::BuildProgress),
            "fixture: it must still wear on the build, or the ordering claim is vacuous"
        );
        assert_eq!(
            quantum_units_per_noun(
                gear.headline_wear().per,
                crate::intensification::LadderConfig::builtin().reference_build_cost()
            ),
            ONE_UNIT_IS_ITS_OWN_NOUN,
            "so its published `quantaLeft` is a raw biomass count, unconverted"
        );
    }
}
