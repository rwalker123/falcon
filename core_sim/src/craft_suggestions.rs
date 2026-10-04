//! **What to make next, ranked by who is going without** (`docs/plan_crafting_and_materials.md` §7,
//! "Suggestions").
//!
//! A pure function over a band's own state — the tool lines its consumers settled this turn, its
//! bench queue, and the two configs — so the crafting panel, the AI's Craft specialist and auto-craft
//! read **one** list rather than each deriving their own.
//!
//! # The score is workers going without
//!
//! A missing hoe costs a gardener work; a missing spear costs a hunter attack and carry, which is
//! food. No single unit of *output* can rank those against each other, but a worker without the
//! tool their job needs means the same thing for both — so that is the score. Ties break by item id,
//! so the order is stable.
//!
//! # The sources are the consumers that draw on the band's stock each turn
//!
//! - **the standing pools** — [`LaborAllocation::last_pool_toe`] (`poolToe` on the wire);
//! - **the site crews** — [`LaborAllocation::last_keeping_issued`], each line carrying its claim
//!   beside what the settlement issued (`upkeepToe` on the wire);
//! - **the take rows** — each row's kit lines (`kitToe` on the wire), handed in by the capture that
//!   settles them.
//!
//! **A detached party is not a source**: it carries the kit it left with and is never resupplied, so
//! nothing crafted now reaches it. Its cohort works no rows, and the capture publishes it no list.
//!
//! **A spent unit counts; a worn one does not.** Every line reads `required − filled`, and `filled`
//! is what the settlement issued from **stock** — a unit that expired is gone from stock and so was
//! not issued, while a worn unit performs at full strength until it expires and is issued like any
//! other. Nothing here reads condition.
//!
//! # The count is the WHOLE shortfall, less what is already queued
//!
//! Never capped by what the band can afford to make now — the shortfall is what later becomes *"go
//! and fetch wood"*, and capping it to today's stock would hide exactly that gap. What is already on
//! the bench's queue for the item is netted out, and a suggestion netted to zero is dropped, so a
//! click does not keep re-offering the same spears. Whether the item can be made at all is the craft
//! offer's question, not this one's: an item no recipe makes is still suggested.

use std::collections::BTreeMap;

use crate::{
    components::{BandBench, BuildSource, LaborAllocation, LaborTarget},
    equipment_config::{
        EffectTier, EquipmentConfig, EquipmentEffect, EquipmentStat, ItemDefinition, KitJob,
    },
    intensification::NO_BUILD_GEAR,
    recipes_config::RecipesConfig,
};

/// **WHO WENT WITHOUT** — the consumer a [`ToolShortfallLine`] belongs to, keyed so a reader can join
/// it to the row it renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplySource {
    /// A standing pool — Roadwork or the builders — by its job.
    Pool(KitJob),
    /// A site crew keeping its source.
    Site(BuildSource),
    /// A take row. `job` is [`LaborTarget::kind`]; `source` is the source it works, `None` for a
    /// band-wide role (scout, warrior).
    Take {
        job: &'static str,
        source: Option<BuildSource>,
    },
}

impl SupplySource {
    /// The take-row key for a labor row.
    pub fn take_row(target: &LaborTarget) -> Self {
        SupplySource::Take {
            job: target.kind(),
            source: BuildSource::of(target),
        }
    }

    /// **Whether a missing unit here costs build or keeping WORK.** The pools build and keep; a site
    /// crew keeps. A take row's kit buys attack, carry or reach — food, not work.
    fn adds_work(&self) -> bool {
        match self {
            SupplySource::Pool(_) | SupplySource::Site(_) => true,
            SupplySource::Take { .. } => false,
        }
    }
}

/// **ONE CONSUMER'S CLAIM ON ONE ITEM, AS THE TURN SETTLED IT** — the scorer's input.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolShortfallLine {
    pub source: SupplySource,
    /// The `equipment.json` item id.
    pub item: String,
    /// Units the consumer's hands required.
    pub required: f32,
    /// Units the settlement handed it. Whole units, so it may exceed [`Self::required`]; a shortfall
    /// is `(required − filled).max(0)`, never a bare subtraction.
    pub filled: f32,
}

/// **ONE CONSUMER GOING WITHOUT A SUGGESTED ITEM** — a row of [`CraftSuggestion::sources`].
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionSourceLine {
    pub source: SupplySource,
    /// `(required − filled)`, in units of the item.
    pub missing_units: f32,
    /// `missing_units × workers_per_unit` — the people who went without.
    pub workers_without: f32,
    /// `workers_without × build_work` where the consumer builds or keeps
    /// ([`SupplySource::adds_work`]), else [`NO_WORK_RECOVERED`].
    pub work_per_turn: f32,
}

/// **ONE SUGGESTION** — an item, how many to make, and who is going without it.
#[derive(Debug, Clone, PartialEq)]
pub struct CraftSuggestion {
    /// The `equipment.json` item id.
    pub item: String,
    /// Units to make: the whole shortfall, less what the queue already owes. Always `>= 1`.
    pub count: u32,
    /// The score — [`SuggestionSourceLine::workers_without`] summed over [`Self::sources`].
    pub workers_without: f32,
    /// [`SuggestionSourceLine::work_per_turn`] summed over [`Self::sources`].
    pub work_per_turn: f32,
    /// Every consumer short of the item, in the order the lines arrived.
    pub sources: Vec<SuggestionSourceLine>,
}

/// **A LINE THAT IS NOT SHORT** — the boundary at and below which a consumer went without nothing.
const NOTHING_MISSING: f32 = 0.0;

/// **WHAT A CONSUMER THAT ADDS NO WORK RECOVERS** — a take row's kit, or an item declaring no
/// `build_work`.
pub const NO_WORK_RECOVERED: f32 = 0.0;

/// **THE `workers_per_unit` OF AN ITEM THE TABLE DOES NOT CARRY** — the item definition's own
/// default (one person holds one unit), for a line naming an id a config edit has since dropped.
const ONE_WORKER_PER_UNIT: u32 = 1;

/// **HOW FAR ABOVE A WHOLE NUMBER A SUMMED SHORTFALL MAY SIT AND STILL BE THAT NUMBER** — the summed
/// `f32` lines carry rounding, and `ceil(1.000001)` would ask for a second tool nobody is short of.
/// A thousandth of a unit is far below any requirement a hand can generate.
const WHOLE_UNIT_TOLERANCE: f32 = 1e-3;

/// **NOTHING LEFT TO SUGGEST** — a netted count at or below this drops the suggestion.
const NOTHING_TO_MAKE: f32 = 0.0;

/// **THE LINES A BAND'S OWN CONSUMERS SETTLED THIS TURN** — the pools and the site crews off the
/// band's allocation, and the take rows' kit lines the caller hands in as
/// `(row index, item, required, filled)` (the capture settles those; the allocation does not carry
/// them).
///
/// **Call it only for a resident band.** A detached party is not a source — see the module docs.
pub fn band_tool_shortfall_lines<'a>(
    allocation: &LaborAllocation,
    take_rows: impl IntoIterator<Item = (usize, &'a str, f32, f32)>,
) -> Vec<ToolShortfallLine> {
    let pools = allocation
        .last_pool_toe
        .iter()
        .map(|line| ToolShortfallLine {
            source: SupplySource::Pool(line.pool),
            item: line.item.clone(),
            required: line.required,
            filled: line.filled,
        });
    let sites = allocation
        .last_keeping_issued
        .iter()
        .map(|issue| ToolShortfallLine {
            source: SupplySource::Site(issue.source.clone()),
            item: issue.item.clone(),
            required: issue.required,
            filled: issue.units,
        });
    let takes = take_rows
        .into_iter()
        .filter_map(|(row, item, required, filled)| {
            let assignment = allocation.assignments.get(row)?;
            Some(ToolShortfallLine {
                source: SupplySource::take_row(&assignment.target),
                item: item.to_string(),
                required,
                filled,
            })
        });
    pools.chain(sites).chain(takes).collect()
}

/// **RANK WHAT TO MAKE NEXT** — see the module docs for every rule.
///
/// `bench` is the band's bench, whose queue nets the counts; `None` nets nothing.
pub fn craft_suggestions(
    lines: &[ToolShortfallLine],
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
) -> Vec<CraftSuggestion> {
    let mut by_item: BTreeMap<&str, Vec<SuggestionSourceLine>> = BTreeMap::new();
    for line in lines {
        let missing_units = (line.required - line.filled).max(NOTHING_MISSING);
        if missing_units <= NOTHING_MISSING {
            continue;
        }
        let def = equipment.item(&line.item);
        let per_unit = def.map_or(ONE_WORKER_PER_UNIT, |def| def.workers_per_unit) as f32;
        let workers_without = missing_units * per_unit;
        let work_per_turn = if line.source.adds_work() {
            workers_without * def.map_or(NO_BUILD_GEAR, build_work_per_worker)
        } else {
            NO_WORK_RECOVERED
        };
        by_item
            .entry(line.item.as_str())
            .or_default()
            .push(SuggestionSourceLine {
                source: line.source.clone(),
                missing_units,
                workers_without,
                work_per_turn,
            });
    }

    let mut suggestions: Vec<CraftSuggestion> = by_item
        .into_iter()
        .filter_map(|(item, sources)| {
            let missing: f32 = sources.iter().map(|line| line.missing_units).sum();
            let whole_shortfall = (missing - WHOLE_UNIT_TOLERANCE).ceil();
            let netted = (whole_shortfall - queued_units(bench, recipes, item)).ceil();
            if netted <= NOTHING_TO_MAKE {
                return None;
            }
            Some(CraftSuggestion {
                item: item.to_string(),
                count: netted as u32,
                workers_without: sources.iter().map(|line| line.workers_without).sum(),
                work_per_turn: sources.iter().map(|line| line.work_per_turn).sum(),
                sources,
            })
        })
        .collect();
    // **Most people going without first; ties by item id**, which the `BTreeMap` walk already put in
    // order — a stable sort keeps it.
    suggestions.sort_by(|a, b| b.workers_without.total_cmp(&a.workers_without));
    suggestions
}

/// **UNITS OF `item` THE BENCH'S QUEUE STILL OWES** — `(count − made) × the output amount`, summed
/// over every order whose recipe makes the item. An order whose recipe the book no longer carries
/// makes nothing and nets nothing.
pub fn queued_units(bench: Option<&BandBench>, recipes: &RecipesConfig, item: &str) -> f32 {
    let Some(bench) = bench else {
        return NOTHING_TO_MAKE;
    };
    bench
        .orders
        .iter()
        .filter_map(|order| {
            let recipe = recipes.recipe(&order.recipe_id)?;
            let per_pass: f32 = recipe
                .outputs
                .iter()
                .filter(|output| output.equipment_id() == Some(item))
                .map(|output| output.amount)
                .sum();
            Some(order.remaining() as f32 * per_pass)
        })
        .sum()
}

/// **THE WORK ONE HOLDER OF THIS ITEM ADDS** — the `build_work` `equipped` value at the item's
/// default tier (the tier every reference rate resolves through), falling back to the item's shared
/// effects when the tier declares none: the tier layer beats the item layer, as everywhere else.
/// The largest across the branches it serves; [`NO_BUILD_GEAR`] for an item that declares none.
fn build_work_per_worker(def: &ItemDefinition) -> f32 {
    let is_build_work = |effect: &&EquipmentEffect| effect.stat == EquipmentStat::BuildWork;
    let tier = &def.default_tier().effects;
    let layer = if tier.iter().any(|effect| is_build_work(&effect)) {
        tier
    } else {
        &def.effects
    };
    layer
        .iter()
        .filter(is_build_work)
        .filter_map(|effect| match effect.tier {
            EffectTier::Equipped(value) => Some(value),
            EffectTier::Unequipped(_) => None,
        })
        .fold(NO_BUILD_GEAR, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::BenchOrder;
    use bevy::math::UVec2;

    const HOES: &str = "hoes";
    const SPEARS: &str = "spears";
    const HOES_RECIPE: &str = "hoes";
    const PATCH: UVec2 = UVec2::new(3, 4);

    fn configs() -> (
        std::sync::Arc<RecipesConfig>,
        std::sync::Arc<EquipmentConfig>,
    ) {
        (RecipesConfig::builtin(), EquipmentConfig::builtin())
    }

    fn line(source: SupplySource, item: &str, required: f32, filled: f32) -> ToolShortfallLine {
        ToolShortfallLine {
            source,
            item: item.to_string(),
            required,
            filled,
        }
    }

    fn take(fauna: &str) -> SupplySource {
        SupplySource::Take {
            job: "hunt",
            source: Some(BuildSource::Herd(fauna.to_string())),
        }
    }

    /// **More people going without ranks first, whatever the item** — and a tie goes to the item id.
    #[test]
    fn ranks_by_workers_going_without_then_by_item_id() {
        let (recipes, equipment) = configs();
        let lines = [
            line(
                SupplySource::Site(BuildSource::Patch(PATCH)),
                HOES,
                2.0,
                0.0,
            ),
            line(take("deer"), SPEARS, 3.0, 0.0),
            line(
                SupplySource::Pool(KitJob::Builders),
                "earthmoving",
                3.0,
                0.0,
            ),
        ];
        let ranked = craft_suggestions(&lines, None, &recipes, &equipment);
        let order: Vec<&str> = ranked.iter().map(|s| s.item.as_str()).collect();
        assert_eq!(
            order,
            vec!["earthmoving", SPEARS, HOES],
            "three short beats two, and the 3–3 tie goes to the earlier id"
        );
        assert_eq!(ranked[2].count, 2, "the whole shortfall, in units");
    }

    /// **Work a turn is stated only where the gear adds build or keeping work** — a spear's cost is
    /// food, so its row reads the people, not a work figure.
    #[test]
    fn work_recovered_is_build_work_on_pools_and_sites_and_nothing_on_a_take_row() {
        let (recipes, equipment) = configs();
        let hoe_work = build_work_per_worker(equipment.item(HOES).expect("shipped item"));
        assert!(hoe_work > NO_BUILD_GEAR, "the shipped hoe adds build work");
        let lines = [
            line(
                SupplySource::Site(BuildSource::Patch(PATCH)),
                HOES,
                2.0,
                0.0,
            ),
            line(take("deer"), SPEARS, 1.0, 0.0),
        ];
        let ranked = craft_suggestions(&lines, None, &recipes, &equipment);
        let hoes = ranked
            .iter()
            .find(|s| s.item == HOES)
            .expect("hoes suggested");
        let spears = ranked
            .iter()
            .find(|s| s.item == SPEARS)
            .expect("spears suggested");
        assert!((hoes.work_per_turn - 2.0 * hoe_work).abs() < f32::EPSILON);
        assert_eq!(spears.work_per_turn, NO_WORK_RECOVERED);
    }

    /// **What the queue already owes is netted, and a suggestion netted to zero drops off.** Paired:
    /// the same lines with nothing queued DO suggest, so a scorer that never suggested passes neither.
    #[test]
    fn the_queue_nets_the_count_and_a_netted_zero_drops() {
        let (recipes, equipment) = configs();
        let lines = [
            line(
                SupplySource::Site(BuildSource::Patch(PATCH)),
                HOES,
                2.0,
                0.0,
            ),
            line(
                SupplySource::Site(BuildSource::Patch(UVec2::new(5, 5))),
                HOES,
                1.0,
                0.0,
            ),
        ];
        let unqueued = craft_suggestions(&lines, None, &recipes, &equipment);
        assert_eq!(unqueued[0].count, 3, "nothing queued: the whole shortfall");

        let mut bench = BandBench::default();
        let mut started = BenchOrder::new(HOES_RECIPE, 3);
        started.made = 1;
        bench.orders.push(started);
        let netted = craft_suggestions(&lines, Some(&bench), &recipes, &equipment);
        assert_eq!(netted[0].count, 1, "two still owed by the queue: 3 − 2");

        bench.enqueue(HOES_RECIPE, 1);
        let covered = craft_suggestions(&lines, Some(&bench), &recipes, &equipment);
        assert!(
            covered.is_empty(),
            "the queue owes all three — nothing to suggest"
        );
    }

    /// **The count is the whole shortfall, never re-asked for float noise**, and an over-filled line
    /// (whole units may exceed a fractional claim) is not short at all.
    #[test]
    fn a_fractional_shortfall_rounds_up_and_an_over_filled_line_is_not_short() {
        let (recipes, equipment) = configs();
        let lines = [
            line(
                SupplySource::Pool(KitJob::Roadwork),
                "earthmoving",
                0.3,
                0.0,
            ),
            line(
                SupplySource::Pool(KitJob::Builders),
                "earthmoving",
                0.7,
                0.0,
            ),
            line(SupplySource::Pool(KitJob::Builders), HOES, 0.136, 1.0),
        ];
        let ranked = craft_suggestions(&lines, None, &recipes, &equipment);
        assert_eq!(ranked.len(), 1, "the over-filled hoe line is not short");
        assert_eq!(ranked[0].count, 1, "0.3 + 0.7 is one tool, not two");
        assert_eq!(ranked[0].sources.len(), 2, "both pools are named");
    }
}
