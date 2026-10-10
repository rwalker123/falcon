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
//!   settles them;
//! - **the queued builds behind the head** — [`LaborAllocation::last_queued_build_toe`]: what each
//!   later job in the build queue will ask of the builders' hands. The head's own claim is the
//!   builders' `last_pool_toe` line; the later jobs are walked **in queue order against the stock
//!   left after the turn's settlement** (owned units, less every unit the turn issued to a pool, a
//!   site crew or a take row), each job taking its tools **cumulatively** — a tool an earlier job
//!   takes is not there for a later one. A line is a [`SupplySource::BuildQueue`] carrying the
//!   job's queue position and source.
//!
//! # The order
//!
//! 1. every **non-build** shortage (a pool other than the builders, a site crew, a take row), by
//!    workers going without;
//! 2. then the **current build** — the head job's builders line, as build position 0;
//! 3. then the **later jobs, in queue order**, ties by item id.
//!
//! An item short in both places is **one** suggestion: its count is the total and it ranks at its
//! earliest position.
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
    components::{BandBench, BandEquipment, BuildSource, LaborAllocation, LaborTarget},
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
    /// **A queued build behind the head**, by its place in the build queue (`1` is next) and its
    /// source. The head's own claim is [`SupplySource::Pool`] of the builders.
    BuildQueue { position: u32, source: BuildSource },
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
    /// **THE BUILD QUEUE PLACE THIS SOURCE IS A CLAIM FOR**, `None` for a non-build consumer: the
    /// builders' pool is the head job (position 0) and a [`Self::BuildQueue`] line carries its own.
    fn build_position(&self) -> Option<u32> {
        match self {
            SupplySource::Pool(KitJob::Builders) => Some(HEAD_BUILD_POSITION),
            SupplySource::BuildQueue { position, .. } => Some(*position),
            _ => None,
        }
    }

    fn adds_work(&self) -> bool {
        match self {
            SupplySource::Pool(_) | SupplySource::Site(_) | SupplySource::BuildQueue { .. } => true,
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
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
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
    let mut lines: Vec<ToolShortfallLine> = pools.chain(sites).chain(takes).collect();
    lines.extend(queued_build_lines(allocation, &lines, equipment, wear));
    lines
}

/// **A ROW THAT CLAIMS NONE OF AN ITEM WRITES NO KIT LINE FOR IT** — `required` is never `0` on the
/// wire, so a reader may divide by it.
const NOTHING_CLAIMED: f32 = 0.0;

/// **ONE TAKE ROW'S CLAIM ON ONE KIT ITEM** — units `required` against the units the settlement
/// handed the row (`filled`). The wire's `kitToe` line, before it is worded.
#[derive(Debug, Clone, PartialEq)]
pub struct KitToeLine {
    pub item_id: String,
    pub required: f32,
    pub filled: f32,
}

/// **ONE TAKE ROW'S GEAR, SETTLED** — the kit it carries, how far the band's units cover it, and its
/// per-item claim lines (a standing pool writes none: its tools are settled with the keeping).
#[derive(Debug, Clone)]
pub struct TakeRowGear {
    pub kit: crate::equipment_config::KitChoice,
    pub coverage: crate::equipment_config::KitCoverage,
    pub toe: Vec<KitToeLine>,
}

/// **EVERY ROW'S GEAR, struck once for the snapshot and for auto-craft alike** — index-aligned with
/// `allocation.assignments`. The kit is spread over the take hands (a keeper carries the keeping
/// tools, never the take kit, `docs/plan_site_crews.md` §2.3) and cut from the band's share of the
/// ledger ([`LaborAllocation::item_budget`]), settled on `claims`.
pub fn take_row_gear(
    config: &EquipmentConfig,
    allocation: &LaborAllocation,
    claims: &crate::take_claims::RowClaims,
    kit: &crate::components::BandEquipment,
) -> Vec<TakeRowGear> {
    let budget = allocation.item_budget(config, &claims.claims);
    allocation
        .assignments
        .iter()
        .enumerate()
        .map(|(i, assignment)| {
            let workers =
                crate::take_claims::take_hands(assignment.workers as f32, claims.keep_hands[i]);
            let row_kit = if assignment.target.is_standing_pool() {
                config.no_kit()
            } else {
                assignment.kit_choice(config)
            };
            let share = budget.share_for_source(&assignment.target, kit, config);
            let coverage = config.coverage_from_units(&row_kit, workers, kit, &share);
            // **WHICH OF THE KIT'S ITEMS ARE SHORT, BY NAME** — per item, the units the row
            // claimed (its take hands that would take something with the kit, never its head
            // count) beside the units the settlement handed it. A row that claims nothing writes
            // no line.
            let claim = claims.claims[i];
            let toe = row_kit
                .uses()
                .filter_map(|item| {
                    let per_unit = config
                        .item(item)
                        .map_or(ONE_WORKER_PER_UNIT, |def| def.workers_per_unit)
                        as f32;
                    let required = claim / per_unit;
                    (required > NOTHING_CLAIMED).then(|| KitToeLine {
                        item_id: item.to_string(),
                        required,
                        filled: share(item),
                    })
                })
                .collect();
            TakeRowGear {
                kit: row_kit,
                coverage,
                toe,
            }
        })
        .collect()
}

/// **THE QUEUED BUILDS' LINES** — each later job in the build queue walked against the stock left
/// after this turn's settlement, **cumulatively**: the band's owned live units of an item
/// ([`BandEquipment::live_units`], the reading the settlement itself uses) less every unit the turn
/// issued of it (`settled` — the pools' and sites' and take rows' `filled`, the head's builders line
/// included), then less what each earlier queued job took. A job's line is `required` against what it
/// could take from that remainder.
///
/// Jobs are walked in queue order, items within a job in id order; nothing here re-derives the
/// settlement.
fn queued_build_lines(
    allocation: &LaborAllocation,
    settled: &[ToolShortfallLine],
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> Vec<ToolShortfallLine> {
    let mut queued: Vec<&crate::components::QueuedBuildToe> =
        allocation.last_queued_build_toe.iter().collect();
    queued.sort_by(|a, b| (a.position, &a.item).cmp(&(b.position, &b.item)));
    let mut left: BTreeMap<&str, f32> = BTreeMap::new();
    queued
        .into_iter()
        .map(|line| {
            let stock = left.entry(line.item.as_str()).or_insert_with(|| {
                let issued: f32 = settled
                    .iter()
                    .filter(|settled| settled.item == line.item)
                    .map(|settled| settled.filled)
                    .sum();
                (wear.live_units(&line.item, equipment) as f32 - issued).max(NOTHING_MISSING)
            });
            let filled = line.required.min(*stock);
            *stock -= filled;
            ToolShortfallLine {
                source: SupplySource::BuildQueue {
                    position: line.position,
                    source: line.source.clone(),
                },
                item: line.item.clone(),
                required: line.required,
                filled,
            }
        })
        .collect()
}

/// **A BAND'S RANKED SUGGESTIONS, from its settled gear** — the one function the snapshot's
/// `craftSuggestions` and auto-craft both call, so the list the panel shows and the list auto-craft
/// works down cannot differ. `take_rows` are `(row, item, required, filled)` off [`TakeRowGear::toe`].
pub fn band_suggestions<'a>(
    allocation: &LaborAllocation,
    take_rows: impl IntoIterator<Item = (usize, &'a str, f32, f32)>,
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> Vec<CraftSuggestion> {
    let lines = band_tool_shortfall_lines(allocation, take_rows, equipment, wear);
    craft_suggestions(&lines, bench, recipes, equipment)
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
    // **The order** (module docs): every non-build shortage first, by workers going without; then
    // the build — the head's builders line as position 0, the queued jobs behind it in queue order.
    // An item short in both places ranks at its earliest position. Ties by item id, which the
    // `BTreeMap` walk already put in order — a stable sort keeps it.
    suggestions.sort_by(|a, b| {
        rank_of(a)
            .partial_cmp(&rank_of(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions
}

/// **WHERE A SUGGESTION RANKS** — `(tier, key)`, smaller first. Tier 0 is any non-build shortage,
/// keyed by the people going without it there (negated, so more first); tier 1 is build-only, keyed
/// by the earliest build position it is short at.
fn rank_of(suggestion: &CraftSuggestion) -> (u8, f32) {
    let non_build: f32 = suggestion
        .sources
        .iter()
        .filter(|line| line.source.build_position().is_none())
        .map(|line| line.workers_without)
        .sum();
    if suggestion
        .sources
        .iter()
        .any(|line| line.source.build_position().is_none())
    {
        return (NON_BUILD_TIER, -non_build);
    }
    let earliest = suggestion
        .sources
        .iter()
        .filter_map(|line| line.source.build_position())
        .min()
        .unwrap_or(HEAD_BUILD_POSITION);
    (BUILD_TIER, earliest as f32)
}

/// The tier of a suggestion short of something a non-build consumer needs.
const NON_BUILD_TIER: u8 = 0;
/// The tier of a suggestion short only for the build queue.
const BUILD_TIER: u8 = 1;
/// The head job's place in the build queue.
const HEAD_BUILD_POSITION: u32 = 0;

/// **UNITS OF `item` ALREADY COMING FROM THE BENCH** — what the queue still owes, `(count − made) ×
/// the output amount` over every order whose recipe makes the item, **plus** what the bench has
/// already made and parked on [`BandBench::finished`] for delivery at the top of the next turn.
///
/// ⛔ **The parked units count because the shortfall lines do not yet know about them.** The labor
/// pass settles the band's tools *before* the bench runs, so on the turn an item is finished the
/// lines still read it missing while the order has already counted it as made — netting the queue
/// alone would raise the suggestion by exactly the item just made (and keep it there when the order
/// popped). Next turn the item is in the store, issued, and gone from both.
///
/// An order whose recipe the book no longer carries makes nothing and nets nothing.
pub fn queued_units(bench: Option<&BandBench>, recipes: &RecipesConfig, item: &str) -> f32 {
    let Some(bench) = bench else {
        return NOTHING_TO_MAKE;
    };
    let owed: f32 = bench
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
        .sum();
    let parked: f32 = bench
        .finished
        .iter()
        .filter(|batch| batch.item == item)
        .map(|batch| batch.count as f32)
        .sum();
    owed + parked
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
                SupplySource::Pool(KitJob::Roadwork),
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

    // ---- the queued builds behind the head --------------------------------------------------

    use crate::components::{PoolToeLine, QueuedBuildToe};

    const CROOK: &str = "crook";
    const PLAIN_TIER: &str = "plain";

    fn herd(id: &str) -> BuildSource {
        BuildSource::Herd(id.to_string())
    }

    fn stocked(item: &str, count: u32) -> BandEquipment {
        let mut wear = BandEquipment::default();
        if count > 0 {
            wear.stock(item, count, PLAIN_TIER, None);
        }
        wear
    }

    fn queued(position: u32, source: BuildSource, item: &str, required: f32) -> QueuedBuildToe {
        QueuedBuildToe {
            position,
            source,
            item: item.to_string(),
            required,
        }
    }

    /// The builders' settled line for the head job.
    fn head(item: &str, required: f32, filled: f32) -> PoolToeLine {
        PoolToeLine {
            pool: KitJob::Builders,
            item: item.to_string(),
            required,
            filled,
        }
    }

    fn suggest(
        allocation: &LaborAllocation,
        take_rows: Vec<(usize, &str, f32, f32)>,
        wear: &BandEquipment,
    ) -> Vec<CraftSuggestion> {
        let (recipes, equipment) = configs();
        let lines = band_tool_shortfall_lines(allocation, take_rows, &equipment, wear);
        craft_suggestions(&lines, None, &recipes, &equipment)
    }

    fn queue_positions(suggestion: &CraftSuggestion) -> Vec<u32> {
        suggestion
            .sources
            .iter()
            .filter_map(|line| line.source.build_position())
            .collect()
    }

    /// **TWO CROOKS HELD, TWO TAME JOBS NEEDING TWO EACH: THE HEAD IS COVERED, THE SECOND IS SHORT 2.**
    /// The head job's line is its settled builders claim (required 2, filled 2); the second job walks
    /// the stock left after the settlement, which is none.
    #[test]
    fn a_covered_head_leaves_nothing_for_the_second_tame_job() {
        let allocation = LaborAllocation {
            last_pool_toe: vec![head(CROOK, 2.0, 2.0)],
            last_queued_build_toe: vec![queued(1, herd("boar"), CROOK, 2.0)],
            ..Default::default()
        };
        let ranked = suggest(&allocation, Vec::new(), &stocked(CROOK, 2));
        assert_eq!(ranked.len(), 1, "only the second job is short: {ranked:?}");
        assert_eq!((ranked[0].item.as_str(), ranked[0].count), (CROOK, 2));
        assert!(matches!(
            ranked[0].sources[0].source,
            SupplySource::BuildQueue { position: 1, .. }
        ));
        assert_eq!(ranked[0].sources[0].workers_without, 2.0);
    }

    /// **TAME, CULTIVATE, TAME WITH 2 CROOKS AND 0 HOES: job 2 is short 2 hoes, job 3 short 2
    /// crooks** — the first Tame took the crooks, so the third finds none. In queue order.
    #[test]
    fn jobs_take_their_tools_cumulatively_in_queue_order() {
        let allocation = LaborAllocation {
            last_pool_toe: vec![head(CROOK, 2.0, 2.0)],
            last_queued_build_toe: vec![
                queued(1, BuildSource::Patch(PATCH), HOES, 2.0),
                queued(2, herd("boar"), CROOK, 2.0),
            ],
            ..Default::default()
        };
        let ranked = suggest(&allocation, Vec::new(), &stocked(CROOK, 2));
        let order: Vec<(&str, u32, Vec<u32>)> = ranked
            .iter()
            .map(|s| (s.item.as_str(), s.count, queue_positions(s)))
            .collect();
        assert_eq!(
            order,
            vec![(HOES, 2, vec![1]), (CROOK, 2, vec![2])],
            "hoes for job 2, then crooks for job 3, each in its place in the queue"
        );
    }

    /// **A LATER JOB TAKES FROM WHAT AN EARLIER QUEUED JOB LEFT** - not from the whole stock. Four
    /// crooks held and the head takes none of them: the first queued job takes two, the second two,
    /// the third finds none.
    #[test]
    fn each_queued_job_takes_from_what_the_ones_before_it_left() {
        let allocation = LaborAllocation {
            last_queued_build_toe: vec![
                queued(1, herd("a"), CROOK, 2.0),
                queued(2, herd("b"), CROOK, 2.0),
                queued(3, herd("c"), CROOK, 2.0),
            ],
            ..Default::default()
        };
        let ranked = suggest(&allocation, Vec::new(), &stocked(CROOK, 4));
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].count, 2, "only the third job is short");
        assert_eq!(queue_positions(&ranked[0]), vec![3]);
    }

    /// **A CURRENT NON-BUILD SHORTAGE OUTRANKS EVERY BUILD ONE, THE HEAD INCLUDED**, even when the head
    /// job has more people going without; the head then ranks before the later jobs.
    #[test]
    fn non_build_shortages_rank_above_the_head_job_and_the_head_above_the_queue() {
        let (recipes, equipment) = configs();
        let lines = [
            line(
                SupplySource::BuildQueue {
                    position: 1,
                    source: BuildSource::Patch(PATCH),
                },
                HOES,
                9.0,
                0.0,
            ),
            line(SupplySource::Pool(KitJob::Builders), CROOK, 5.0, 0.0),
            line(take("deer"), SPEARS, 1.0, 0.0),
        ];
        let ranked = craft_suggestions(&lines, None, &recipes, &equipment);
        let order: Vec<&str> = ranked.iter().map(|s| s.item.as_str()).collect();
        assert_eq!(
            order,
            vec![SPEARS, CROOK, HOES],
            "one hunter without a spear beats five builders without crooks; the head beats the queue \
             even though the queued job is 9 short"
        );
    }

    /// **AN ITEM SHORT NOW AND FOR A LATER JOB IS ONE SUGGESTION**: the total count, ranked at its
    /// earliest position (here with the non-build shortages, ahead of a build-only item).
    #[test]
    fn an_item_short_now_and_later_is_one_suggestion_at_its_earliest_position() {
        let allocation = LaborAllocation {
            last_keeping_issued: vec![crate::components::KeepingIssue {
                source: BuildSource::Patch(PATCH),
                item: HOES.to_string(),
                units: 0.0,
                required: 1.0,
            }],
            last_pool_toe: vec![head(CROOK, 3.0, 0.0)],
            last_queued_build_toe: vec![queued(1, BuildSource::Patch(UVec2::new(8, 8)), HOES, 2.0)],
            ..Default::default()
        };
        let ranked = suggest(&allocation, Vec::new(), &BandEquipment::default());
        let order: Vec<(&str, u32)> = ranked.iter().map(|s| (s.item.as_str(), s.count)).collect();
        assert_eq!(
            order,
            vec![(HOES, 3), (CROOK, 3)],
            "hoes: 1 now + 2 later = one suggestion of 3, ahead of the build-only crooks even though \
             the crooks have more people without"
        );
        assert_eq!(
            ranked[0].sources.len(),
            2,
            "both lines ride the one suggestion"
        );
    }
}
