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

use sim_schema::CraftOfferState;

use crate::{
    components::{BandBench, BandEquipment, BuildSource, LaborAllocation, LaborTarget},
    equipment_config::{
        EffectTier, EquipmentConfig, EquipmentEffect, EquipmentStat, ItemDefinition, KitJob,
    },
    intensification::NO_BUILD_GEAR,
    materials_config::MaterialsConfig,
    recipes_config::{RecipeDef, RecipesConfig},
    systems::{bench_tiers, rate_per_turn},
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
        /// The id of the kit the row CHOSE (its own pick, else its job's default) - what attributes
        /// the row's lines to one kit when several share a job and an item (`sled`).
        kit: String,
    },
    /// **A queued build behind the head**, by its place in the build queue (`1` is next) and its
    /// source. The head's own claim is [`SupplySource::Pool`] of the builders.
    BuildQueue { position: u32, source: BuildSource },
}

impl SupplySource {
    /// The take-row key for a labor row.
    pub fn take_row(target: &LaborTarget, kit: &str) -> Self {
        SupplySource::Take {
            job: target.kind(),
            source: BuildSource::of(target),
            kit: kit.to_string(),
        }
    }

    /// **THE BUILD QUEUE PLACE THIS SOURCE IS A CLAIM FOR**, `None` for a non-build consumer: the
    /// builders' pool is the head job (position 0) and a [`Self::BuildQueue`] line carries its own.
    fn build_position(&self) -> Option<u32> {
        match self {
            SupplySource::Pool(KitJob::Builders) => Some(HEAD_BUILD_POSITION),
            SupplySource::BuildQueue { position, .. } => Some(*position),
            _ => None,
        }
    }

    /// **Whether a missing unit here costs build or keeping WORK.** The pools build and keep; a site
    /// crew keeps. A take row's kit buys attack, carry or reach — food, not work.
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
    /// Every consumer short of the item, in the order the lines arrived. **Empty for an item that is
    /// only coming up** (see [`Self::upcoming_count`]).
    pub sources: Vec<SuggestionSourceLine>,
    /// **The share of [`Self::count`] that is a forecast** (#778): units that will wear out before
    /// the bench could replace them, netted against the queue after the current shortfall is.
    /// `count - upcoming_count` is what is short today.
    pub upcoming_count: u32,
    /// Turns until the first forecast shortage starts (fractional): when the loss that leaves a
    /// consumer without lands, not when the unit in hand runs out. `0` with no forecast.
    pub expires_in_turns: f32,
    /// The bench's lead time for one more unit, in whole turns (`>= 1`). `0` with no forecast.
    pub lead_turns: u32,
    /// **What the suggestion is called** - an item's own name, a material's title. Resolved by the
    /// capture; empty off it.
    pub display_name: String,
    /// **Why it is suggested, when the reason is not a kit going without** - a build-material
    /// suggestion states the job it feeds (`for Corral Wild Boar`). Empty for a kit suggestion, whose
    /// reason is its sources' `workers_without`.
    pub reason: String,
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
                source: SupplySource::take_row(
                    &assignment.target,
                    assignment.kit_choice(equipment).id(),
                ),
                item: item.to_string(),
                required,
                filled,
            })
        });
    let mut lines: Vec<ToolShortfallLine> = pools.chain(sites).chain(takes).collect();
    lines.extend(queued_build_lines(allocation, &lines, equipment, wear));
    lines
}

/// **THE KEEPING JOBS A SITE CREW KEEPS ITS SOURCE UNDER** — a site's tools are the keeping kits'
/// (`tillage` for a patch, `hurdling` for a herd, the road kits for a road, the deposit branches'
/// for a working), so a [`SupplySource::Site`] line is attributed to a kit through its source's
/// keeping job rather than through a labor row (a keeping job has no row of its own).
fn site_keeping_jobs(source: &BuildSource) -> &'static [KitJob] {
    match source {
        BuildSource::Patch(_) => &[KitJob::Agriculture],
        BuildSource::Herd(_) => &[KitJob::Husbandry],
        BuildSource::Road(_) | BuildSource::Tile { .. } | BuildSource::Band { .. } => {
            &[KitJob::Roadwork]
        }
        BuildSource::Deposit { .. } => &[KitJob::Quarrywork, KitJob::Extraction],
    }
}

impl SupplySource {
    /// **WHETHER THIS CONSUMER IS ONE OF A KIT'S JOBS** — the attribution [`band_kit_needs`] counts a
    /// unit by. A queued build is a forecast of a later job's claim, never a consumer holding a unit
    /// today, so it serves no job.
    fn serves_any(&self, jobs: &[KitJob]) -> bool {
        match self {
            SupplySource::Pool(job) => jobs.contains(job),
            SupplySource::Take { job, .. } => jobs.iter().any(|kit_job| kit_job.as_str() == *job),
            SupplySource::Site(source) => site_keeping_jobs(source)
                .iter()
                .any(|job| jobs.contains(job)),
            SupplySource::BuildQueue { .. } => false,
        }
    }
}

impl SupplySource {
    /// **WHETHER THIS CLAIM IS ON A KIT** - the attribution [`KitNeedItem::claimed`] and
    /// [`KitNeedItem::short`] count by. A take row is on the kit it CHOSE; a pool and a site crew are
    /// on a kit through their job (a site through its keeping job); a queued build is on the kit of
    /// the builders' job that carries its item. The caller has already matched the line's item to the
    /// kit's `uses`.
    fn is_on_kit(&self, kit: &crate::equipment_config::KitDefinition) -> bool {
        match self {
            SupplySource::Take { kit: chosen, .. } => *chosen == kit.id,
            SupplySource::BuildQueue { .. } => kit.jobs.contains(&KitJob::Builders),
            other => other.serves_any(&kit.jobs),
        }
    }
}

/// **ONE ITEM OF A KIT, AS THE BAND STANDS ON IT** — a row of [`KitNeed::items`].
#[derive(Debug, Clone, PartialEq)]
pub struct KitNeedItem {
    /// The `equipment.json` item id.
    pub item: String,
    /// How many people one unit equips (the item definition's own).
    pub workers_per_unit: u32,
    /// Units that serve THIS kit's jobs: the units the settlement issued to those jobs' consumers,
    /// plus the spare units nobody holds. A unit another job holds never counts.
    pub have: u32,
    /// Units the bench's queue already owes (and the finished ones parked for delivery).
    pub queued: u32,
    /// Whole units of this item parked on the bench's finished list (made this turn, stocked at
    /// the top of the next). A pure readout: `queued` already includes them.
    pub finished: u32,
    /// Units the claims ON THIS KIT asked for: `Σ required` over the shortfall lines
    /// [`SupplySource::is_on_kit`] attributes to it (a kit nobody's row chose claims `0`).
    pub claimed: f32,
    /// Whole units still missing on those lines BEFORE the bench's queue nets them - the figure
    /// Make Next starts from ([`whole_shortfall`] over the same lines), so the ledger and the
    /// suggestion cannot disagree. The reader nets `queued`.
    pub short: u32,
}

/// **ONE KIT'S STANDING WITH THE BAND** — the facts the client's *Make a whole kit* picker is
/// computed from. See [`band_kit_needs`].
#[derive(Debug, Clone, PartialEq)]
pub struct KitNeed {
    pub kit_id: String,
    pub display_name: String,
    pub jobs: Vec<KitJob>,
    /// People currently on the kit's jobs ([`LaborAllocation::workers_on_job`] summed over them).
    pub people: u32,
    pub items: Vec<KitNeedItem>,
}

/// **A SUMMED `f32` LINE READ AS WHOLE UNITS** — the settlement issues whole units, so a sum is a
/// whole number up to rounding; [`WHOLE_UNIT_TOLERANCE`] keeps `1.9999` from reading as one.
fn whole_units(units: f32) -> u32 {
    (units.max(NOTHING_MISSING) + WHOLE_UNIT_TOLERANCE).floor() as u32
}

/// **A SUMMED MISSING-UNIT FIGURE AS THE WHOLE UNITS TO MAKE** - `ceil(missing - tolerance)`, floored
/// at zero. The ONE formula under both Make Next's count (before the queue nets it) and the kit
/// ledger's `short`.
fn whole_shortfall(missing: f32) -> f32 {
    (missing - WHOLE_UNIT_TOLERANCE).ceil().max(NOTHING_TO_MAKE)
}

/// **EVERY ORDERABLE KIT'S STANDING, from the lines the turn settled** — one [`KitNeed`] per roster
/// kit that carries something (the shipped `none` carries nothing and is not orderable), in config
/// order.
///
/// - `people` is the head count on the kit's jobs.
/// - `have` per item is the units issued to **this kit's jobs' consumers** (`Σ filled` over the
///   lines [`SupplySource::serves_any`] attributes to them) **plus the spare units** — owned live
///   units less everything the settlement issued to anyone, floored at zero. **A unit held by
///   another job never counts**: a sled in a woodcutter's hands does not equip a hunter, so counting
///   it would tell the picker nobody needs a sled while the hunters walk without one. A queued
///   build's line is a forecast, so it never counts as a unit held (though it is still part of what
///   is spoken for when the spare is struck).
/// - `queued` is the one [`queued_units`] the suggestions net by.
pub fn band_kit_needs(
    allocation: &LaborAllocation,
    lines: &[ToolShortfallLine],
    hunt_turn: Option<&[ToolShortfallLine]>,
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> Vec<KitNeed> {
    equipment
        .kits()
        .iter()
        .filter(|kit| !kit.uses.is_empty())
        .map(|kit| {
            let items = kit
                .uses
                .iter()
                .map(|item| {
                    let to_these_jobs: f32 = lines
                        .iter()
                        .filter(|line| line.item == *item && line.source.serves_any(&kit.jobs))
                        .map(|line| line.filled)
                        .sum();
                    let to_anyone: f32 = lines
                        .iter()
                        .filter(|line| line.item == *item)
                        .map(|line| line.filled)
                        .sum();
                    let spare = wear
                        .live_units(item, equipment)
                        .saturating_sub(whole_units(to_anyone));
                    // **Claimed and short, off the passes Make Next reads**: per pass the lines on
                    // this kit for this item; the pass with the larger shortfall wins, ties to the
                    // earlier (`suggestions_over`'s rule).
                    let (claimed, missing) = std::iter::once(lines)
                        .chain(hunt_turn)
                        .map(|pass| {
                            let on_kit = || {
                                pass.iter()
                                    .filter(|line| line.item == *item && line.source.is_on_kit(kit))
                            };
                            let claimed: f32 = on_kit().map(|line| line.required).sum();
                            let missing: f32 = on_kit()
                                .map(|line| (line.required - line.filled).max(NOTHING_MISSING))
                                .sum();
                            (claimed, missing)
                        })
                        .fold(None::<(f32, f32)>, |best, next| match best {
                            Some(held) if held.1 >= next.1 => Some(held),
                            _ => Some(next),
                        })
                        .unwrap_or((NOTHING_MISSING, NOTHING_MISSING));
                    KitNeedItem {
                        claimed,
                        short: whole_shortfall(missing) as u32,
                        item: item.clone(),
                        workers_per_unit: equipment
                            .item(item)
                            .map_or(ONE_WORKER_PER_UNIT, |def| def.workers_per_unit),
                        have: whole_units(to_these_jobs).saturating_add(spare),
                        queued: whole_units(queued_units(bench, recipes, item)),
                        finished: whole_units(parked_units(bench, item)),
                    }
                })
                .collect();
            KitNeed {
                kit_id: kit.id.clone(),
                display_name: kit.display_name.clone(),
                jobs: kit.jobs.clone(),
                people: kit
                    .jobs
                    .iter()
                    .map(|job| allocation.workers_on_job(*job))
                    .sum(),
                items,
            }
        })
        .collect()
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
            let workers = crate::take_claims::take_hands(
                allocation.hands_present_on(assignment) as f32,
                claims.keep_hands[i],
            );
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
            // **`required` is the ask, `filled` what the passes granted** (#871).
            let claim = claims.asks.get(i).copied().unwrap_or(claims.claims[i]);
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
///
/// `hunt_turn` is the second reading of the band's take rows, on the turn a hunting-by-need crew goes
/// out (`snapshot::population::hunt_turn_lines`): a need row stands at zero hands, so only the
/// mustered pass sees the kit its crew would claim. Each item's shortfall is the larger of the two
/// readings ([`craft_suggestions_over`]), so a kit a hunt crew is short of is suggested - and
/// auto-craft, reading this same list, queues it. `None` is a band with no hunt going out.
pub fn band_suggestions<'a>(
    allocation: &LaborAllocation,
    take_rows: impl IntoIterator<Item = (usize, &'a str, f32, f32)>,
    hunt_turn: Option<&[ToolShortfallLine]>,
    bench: Option<&BandBench>,
    configs: SuggestionConfigs<'_>,
    wear: &BandEquipment,
    forecast: Option<ForecastInputs<'_>>,
) -> Vec<CraftSuggestion> {
    let SuggestionConfigs { recipes, equipment } = configs;
    let lines = band_tool_shortfall_lines(allocation, take_rows, equipment, wear);
    let passes: Vec<&[ToolShortfallLine]> =
        std::iter::once(lines.as_slice()).chain(hunt_turn).collect();
    let upcoming = forecast.map_or_else(BTreeMap::new, |inputs| {
        wear_forecast(
            &item_needs(&passes),
            bench,
            recipes,
            equipment,
            wear,
            &inputs,
        )
    });
    suggestions_over(&passes, bench, recipes, equipment, &upcoming)
}

/// **RANK WHAT TO MAKE NEXT** - see the module docs for every rule.
///
/// `bench` is the band's bench, whose queue nets the counts; `None` nets nothing. This is the list
/// of what is short **today**; [`band_suggestions`] adds what is coming up.
pub fn craft_suggestions(
    lines: &[ToolShortfallLine],
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
) -> Vec<CraftSuggestion> {
    craft_suggestions_over(&[lines], bench, recipes, equipment)
}

/// **RANK WHAT TO MAKE NEXT OVER SEVERAL READINGS OF THE SAME BAND** - the standing turn's lines and
/// the turn the hunt goes out (`hunt_by_need`: a need row stands at zero hands, so only a mustered
/// pass sees the kit its crew would claim). **Each item's shortfall is the larger of the passes, never
/// their sum**: they describe one band on two different days, and the tools made serve both. The
/// sources of the pass that wins are the ones listed. Ties go to the earlier pass.
pub fn craft_suggestions_over(
    passes: &[&[ToolShortfallLine]],
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
) -> Vec<CraftSuggestion> {
    suggestions_over(passes, bench, recipes, equipment, &BTreeMap::new())
}

fn suggestions_over(
    passes: &[&[ToolShortfallLine]],
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
    upcoming: &WearForecast,
) -> Vec<CraftSuggestion> {
    let mut by_item: BTreeMap<&str, Vec<SuggestionSourceLine>> = BTreeMap::new();
    for pass in passes {
        for (item, sources) in group_missing(pass, equipment) {
            let missing = |lines: &[SuggestionSourceLine]| -> f32 {
                lines.iter().map(|line| line.missing_units).sum()
            };
            match by_item.get(item) {
                Some(held) if missing(held) >= missing(&sources) => {}
                _ => {
                    by_item.insert(item, sources);
                }
            }
        }
    }
    suggestions_from(by_item, bench, recipes, upcoming)
}

/// One pass's missing units per item, as source lines.
fn group_missing<'a>(
    lines: &'a [ToolShortfallLine],
    equipment: &EquipmentConfig,
) -> BTreeMap<&'a str, Vec<SuggestionSourceLine>> {
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

    by_item
}

/// Net each item's shortfall - and what is coming up - against the queue and rank the survivors.
///
/// **One suggestion per item.** Its count is today's whole shortfall plus the forecast's units, the
/// queue struck from the shortfall first and what is left of it from the forecast.
fn suggestions_from(
    shortages: BTreeMap<&str, Vec<SuggestionSourceLine>>,
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    upcoming: &WearForecast,
) -> Vec<CraftSuggestion> {
    let mut items: BTreeMap<&str, Vec<SuggestionSourceLine>> = BTreeMap::new();
    for (item, forecast) in upcoming {
        if forecast.units > 0 {
            items.entry(item.as_str()).or_default();
        }
    }
    items.extend(shortages);
    let mut suggestions: Vec<CraftSuggestion> = items
        .into_iter()
        .filter_map(|(item, sources)| {
            let missing: f32 = sources.iter().map(|line| line.missing_units).sum();
            let whole_shortfall = whole_shortfall(missing);
            let queued = queued_units(bench, recipes, item);
            let current = (whole_shortfall - queued).ceil().max(NOTHING_TO_MAKE);
            let forecast = upcoming.get(item);
            let coming = forecast.map_or(NOTHING_TO_MAKE, |forecast| forecast.units as f32);
            // What the queue owes beyond today's shortfall serves the forecast.
            let spare_queue = (queued - whole_shortfall).max(NOTHING_TO_MAKE);
            let coming = (coming - spare_queue).ceil().max(NOTHING_TO_MAKE);
            let count = current + coming;
            if count <= NOTHING_TO_MAKE {
                return None;
            }
            Some(CraftSuggestion {
                item: item.to_string(),
                count: count as u32,
                workers_without: sources.iter().map(|line| line.workers_without).sum(),
                work_per_turn: sources.iter().map(|line| line.work_per_turn).sum(),
                sources,
                upcoming_count: coming as u32,
                expires_in_turns: forecast.map_or(NO_FORECAST_TURNS, |f| f.expires_in_turns),
                lead_turns: forecast.map_or(NO_FORECAST_LEAD, |f| f.lead_turns),
                display_name: String::new(),
                reason: String::new(),
            })
        })
        .collect();
    // **The order** (module docs): every non-build shortage first, by workers going without; then
    // the build - the head's builders line as position 0, the queued jobs behind it in queue order;
    // then what is only coming up, soonest to run out first. An item short in both places ranks at
    // its earliest position. Ties by item id, which the `BTreeMap` walk already put in order - a
    // stable sort keeps it.
    suggestions.sort_by(|a, b| {
        rank_of(a)
            .partial_cmp(&rank_of(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions
}

/// **WHERE A SUGGESTION RANKS** - `(tier, key)`, smaller first. Tier 0 is any non-build shortage,
/// keyed by the people going without it there (negated, so more first); tier 1 is build-only, keyed
/// by the earliest build position it is short at; tier 2 is coming up only, keyed by the turns until
/// the unit in hand runs out.
fn rank_of(suggestion: &CraftSuggestion) -> (u8, f32) {
    if suggestion.sources.is_empty() {
        return (COMING_UP_TIER, suggestion.expires_in_turns);
    }
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
/// The tier of a suggestion that is only coming up: nothing is short of it today.
const COMING_UP_TIER: u8 = 2;
/// The head job's place in the build queue.
const HEAD_BUILD_POSITION: u32 = 0;
/// The `expiresInTurns` of an item with no wear forecast.
const NO_FORECAST_TURNS: f32 = 0.0;
/// The `leadTurns` of an item with no wear forecast.
const NO_FORECAST_LEAD: u32 = 0;

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
    owed + parked_units(Some(bench), item)
}

/// **UNITS OF `item` THE BENCH HAS FINISHED AND PARKED** on [`BandBench::finished`], stocked at the
/// top of the next turn. The one sum [`queued_units`] adds to what the queue owes and the readout
/// [`KitNeedItem::finished`] publishes, so the two cannot drift.
pub fn parked_units(bench: Option<&BandBench>, item: &str) -> f32 {
    bench.map_or(NOTHING_TO_MAKE, |bench| {
        bench
            .finished
            .iter()
            .filter(|batch| batch.item == item)
            .map(|batch| batch.count as f32)
            .sum()
    })
}

/// **WHAT THE CAPTURE KNOWS ABOUT A QUEUED BUILD THAT THIS MODULE CANNOT** - the goods it still has to
/// draw (the registries hold the source's progress) and the words naming the job.
pub struct BuildPileView<'a> {
    /// The goods the entry has **still to draw**, per material: the pile times the share of the climb
    /// left, so a build 64% done owes 36% of it.
    pub remaining: &'a dyn Fn(&crate::components::BuildQueueEntry) -> BTreeMap<String, f32>,
    /// The job in words, `Corral Wild Boar` - the suggestion's reason reads `for {label}`.
    pub label: &'a dyn Fn(&crate::components::BuildQueueEntry) -> String,
}

/// **THE STOCK MATERIAL A QUEUED BUILD DRAWS, SUGGESTED AS WHAT TO MAKE NEXT.** A queued build eats
/// goods as its work goes in (hurdles for a pen or a ring); a band short of them stalls, and "Make
/// next" has to say so. The queue is walked in order against the store: each entry's **remaining**
/// pile (`view.remaining`) is added to a running need and the part of it the stock does not cover is
/// that entry's shortfall, so a later job is short only of what the earlier ones leave. Every line is
/// a [`SupplySource::BuildQueue`] carrying its queue position and source - the head included, which
/// is what lets a reader name the job - and the suggestion's `reason` is the first short entry's
/// `for {job}`. Only a material some recipe makes is suggested - a raw one is fetched, not made - and
/// the count is whole **passes** of that recipe (`ceil(short / output per pass) x output per pass`),
/// less what the bench's queue already owes. A holding under
/// [`crate::components::MATERIAL_DUST_UNITS`] is nothing. The suggestions merge into the kit list in
/// [`merge_build_material_suggestions`], which keeps the module's one ranking.
pub fn build_material_suggestions(
    allocation: &LaborAllocation,
    store: &crate::components::LocalStore,
    recipes: &RecipesConfig,
    bench: Option<&BandBench>,
    view: &BuildPileView<'_>,
) -> Vec<CraftSuggestion> {
    use crate::components::MATERIAL_DUST_UNITS;
    let mut need: BTreeMap<String, f32> = BTreeMap::new();
    let mut counted: BTreeMap<String, f32> = BTreeMap::new();
    let mut lines: BTreeMap<String, Vec<SuggestionSourceLine>> = BTreeMap::new();
    let mut reasons: BTreeMap<String, String> = BTreeMap::new();
    for (position, entry) in allocation.build_queue.iter().enumerate() {
        for (material, pile) in (view.remaining)(entry) {
            let running = need.entry(material.clone()).or_insert(NOTHING_MISSING);
            *running += pile;
            let held = store.material_total(&material).to_f32();
            let held = if held < MATERIAL_DUST_UNITS {
                NOTHING_MISSING
            } else {
                held
            };
            let short_through = (*running - held).max(NOTHING_MISSING);
            let already = counted
                .insert(material.clone(), short_through)
                .unwrap_or(NOTHING_MISSING);
            let missing_units = short_through - already;
            if missing_units <= NOTHING_MISSING {
                continue;
            }
            reasons
                .entry(material.clone())
                .or_insert_with(|| format!("for {}", (view.label)(entry)));
            lines
                .entry(material)
                .or_default()
                .push(SuggestionSourceLine {
                    source: SupplySource::BuildQueue {
                        position: position as u32,
                        source: entry.source.clone(),
                    },
                    missing_units,
                    workers_without: NOTHING_MISSING,
                    work_per_turn: NO_WORK_RECOVERED,
                });
        }
    }
    lines
        .into_iter()
        .filter_map(|(material, sources)| {
            let per_pass: f32 = recipes
                .recipes()
                .filter(|(_, recipe)| recipe.row_key() == Some(material.as_str()))
                .find_map(|(_, recipe)| {
                    let amount: f32 = recipe
                        .outputs
                        .iter()
                        .filter(|output| output.material_id() == Some(material.as_str()))
                        .map(|output| output.amount)
                        .sum();
                    (amount > NOTHING_TO_MAKE).then_some(amount)
                })?;
            let missing: f32 = sources.iter().map(|line| line.missing_units).sum();
            let passes = ((missing - WHOLE_UNIT_TOLERANCE) / per_pass)
                .ceil()
                .max(NOTHING_TO_MAKE);
            let count = (passes * per_pass - queued_material_units(bench, recipes, &material))
                .ceil()
                .max(NOTHING_TO_MAKE);
            (count > NOTHING_TO_MAKE).then(|| CraftSuggestion {
                reason: reasons.get(&material).cloned().unwrap_or_default(),
                item: material,
                count: count as u32,
                workers_without: NOTHING_MISSING,
                work_per_turn: NO_WORK_RECOVERED,
                sources,
                upcoming_count: 0,
                expires_in_turns: NO_FORECAST_TURNS,
                lead_turns: NO_FORECAST_LEAD,
                display_name: String::new(),
            })
        })
        .collect()
}

/// **THE BUILD-MATERIAL SUGGESTIONS FOR A BAND, off the world's claim sources** - the one wrapper the
/// snapshot and auto-craft both call, so the list the panel shows and the list auto-craft works down
/// cannot differ. Supplies the view ([`BuildPileView`]) `build_material_suggestions` needs: each
/// entry's remaining pile and the words naming the job.
pub fn build_material_suggestions_in_world(
    allocation: &LaborAllocation,
    store: &crate::components::LocalStore,
    recipes: &RecipesConfig,
    bench: Option<&BandBench>,
    sources: &crate::take_claims::ClaimSources<'_>,
) -> Vec<CraftSuggestion> {
    build_material_suggestions(
        allocation,
        store,
        recipes,
        bench,
        &BuildPileView {
            remaining: &|entry| {
                crate::systems::remaining_build_materials(
                    entry,
                    sources.forage,
                    sources.herds,
                    sources.deposits,
                    sources.ladder,
                )
            },
            label: &|entry| {
                let job = match &entry.declared {
                    crate::components::BuildJob::Climb(rung) => sources
                        .ladder
                        .rung(*rung)
                        .verb
                        .as_deref()
                        .map(crate::crafting::title_from_id)
                        .unwrap_or_default(),
                    crate::components::BuildJob::ExtendPen => "Extend pen".to_string(),
                    crate::components::BuildJob::SetHerdOutput(_) => "Set herd output".to_string(),
                };
                let target = match &entry.source {
                    BuildSource::Herd(id) => sources
                        .herds
                        .find(id)
                        .map(|herd| herd.species.clone())
                        .unwrap_or_default(),
                    BuildSource::Patch(tile) => format!("({}, {})", tile.x, tile.y),
                    _ => String::new(),
                };
                format!("{job} {target}").trim().to_string()
            },
        },
    )
}

/// **WHAT THE BENCH'S QUEUE STILL OWES OF A MATERIAL** - `remaining × the recipe's output amount`,
/// over the orders whose recipe makes it. The material twin of [`queued_units`].
fn queued_material_units(
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    material: &str,
) -> f32 {
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
                .filter(|output| output.material_id() == Some(material))
                .map(|output| output.amount)
                .sum();
            Some(order.remaining() as f32 * per_pass)
        })
        .sum()
}

/// **MERGE THE BUILD-MATERIAL SUGGESTIONS INTO A KIT LIST** and re-rank, so the module has one order.
pub fn merge_build_material_suggestions(
    suggestions: &mut Vec<CraftSuggestion>,
    extra: Vec<CraftSuggestion>,
) {
    suggestions.extend(extra);
    suggestions.sort_by(|a, b| {
        rank_of(a)
            .partial_cmp(&rank_of(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// **THE TWO CONFIGS A SUGGESTION LIST IS RANKED AND NETTED WITH.**
#[derive(Clone, Copy)]
pub struct SuggestionConfigs<'a> {
    pub recipes: &'a RecipesConfig,
    pub equipment: &'a EquipmentConfig,
}

/// **WHAT A SUGGESTION'S FORECAST NEEDS BEYOND THE LINES** - the offers (the recipe the ledger marks
/// `suggested` for each item) and the materials table (to resolve the bench's rate per recipe).
#[derive(Clone, Copy)]
pub struct ForecastInputs<'a> {
    pub offers: &'a [CraftOfferState],
    pub materials: &'a MaterialsConfig,
}

/// **ONE ITEM'S WEAR FORECAST** (#778).
#[derive(Debug, Clone, PartialEq)]
pub struct UpcomingWear {
    /// Units that will wear out within the lead time and are not covered by a spare: the whole units
    /// short after the lead minus the whole units short now.
    pub units: u32,
    /// Turns until the first forecast shortage starts: the (fractional) turn the loss that leaves a
    /// consumer without lands, not when the unit in hand runs out - a spare absorbs that.
    pub expires_in_turns: f32,
    /// The bench's lead time for one more unit of the item, in whole turns.
    pub lead_turns: u32,
}

/// The forecast for every item that has one, by item id.
pub type WearForecast = BTreeMap<String, UpcomingWear>;

/// **THE CREW A LEAD TIME IS QUOTED AT WHEN NOBODY IS AT THE BENCH** - one crafter, not never. The
/// player staffs the bench and the sim never picks the crew, so an unstaffed bench is quoted at the
/// smallest crew that could make progress.
const UNSTAFFED_BENCH_QUOTE_CREW: u32 = 1;

/// **THE SHORTEST LEAD TIME** - a turn. Whatever the bench's rate, a unit is not made inside the
/// turn it was asked for.
const MIN_LEAD_TURNS: f32 = 1.0;

/// **EACH ITEM'S NEED: THE UNITS ITS CONSUMERS REQUIRE** - `Σ required` over its lines, the larger of
/// the passes (the same two readings [`craft_suggestions_over`] takes the larger of).
fn item_needs<'a>(passes: &[&'a [ToolShortfallLine]]) -> BTreeMap<&'a str, f32> {
    let mut needs: BTreeMap<&str, f32> = BTreeMap::new();
    for pass in passes {
        let mut sums: BTreeMap<&str, f32> = BTreeMap::new();
        for line in pass.iter() {
            *sums.entry(line.item.as_str()).or_default() += line.required;
        }
        for (item, sum) in sums {
            let held = needs.entry(item).or_default();
            *held = held.max(sum);
        }
    }
    needs
}

/// **THE BENCH'S RATE ON A RECIPE** - [`rate_per_turn`] at the bench's crew (one crafter when it has
/// none) through the recipe's bench material. `0` when no tool and no bare hand can work the
/// material, or the recipe has no bench material.
fn quoted_rate(
    bench: Option<&BandBench>,
    recipe: &RecipeDef,
    recipes: &RecipesConfig,
    inputs: &ForecastInputs<'_>,
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> f32 {
    let Some(material) = recipe.bench_material() else {
        return 0.0;
    };
    let crew = bench
        .map(|bench| bench.workers)
        .filter(|workers| *workers > 0)
        .unwrap_or(UNSTAFFED_BENCH_QUOTE_CREW);
    let tiers = bench_tiers(material, inputs.materials, equipment, wear);
    rate_per_turn(crew, &recipes.crafting, tiers.speed)
}

/// **TURNS THE BENCH NEEDS TO FINISH THE WORK ALREADY QUEUED** - each order's remaining passes times
/// its recipe's work, less the progress banked on the pass in flight, over that order's rate. An
/// order the bench cannot advance (rate `0`) adds nothing.
fn queued_lead_turns(
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    inputs: &ForecastInputs<'_>,
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> f32 {
    let Some(bench) = bench else {
        return 0.0;
    };
    bench
        .orders
        .iter()
        .filter_map(|order| {
            let recipe = recipes.recipe(&order.recipe_id)?;
            let rate = quoted_rate(Some(bench), recipe, recipes, inputs, equipment, wear);
            if rate <= 0.0 {
                return None;
            }
            let work = order.remaining() as f32 * recipe.work - order.progress.to_f32();
            Some(work.max(0.0) / rate)
        })
        .sum()
}

/// **THE WEAR FORECAST** (`docs/plan_crafting_and_materials.md` -> "Coming up"): for every item some
/// consumer needs, the units that will wear out before the bench could replace one.
///
/// The rate is the condition the band was charged per turn over its wear window
/// ([`BandEquipment::wear_rate`]); the horizon is the bench's lead time for one more unit; the loss
/// is struck by the ledger's own consumption step on a **clone** of it, a turn at a time, so the
/// forecast spends units in the order the turns will and the real ledger is never touched.
fn wear_forecast(
    needs: &BTreeMap<&str, f32>,
    bench: Option<&BandBench>,
    recipes: &RecipesConfig,
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
    inputs: &ForecastInputs<'_>,
) -> WearForecast {
    let queued_lead = queued_lead_turns(bench, recipes, inputs, equipment, wear);
    let mut forecast = WearForecast::new();
    for (&item, &need) in needs {
        let rate = wear.wear_rate(item);
        if need <= NOTHING_MISSING || rate <= 0.0 {
            continue;
        }
        let Some(recipe) = inputs
            .offers
            .iter()
            .find(|offer| offer.suggested && offer.output_item_id == item)
            .and_then(|offer| recipes.recipe(&offer.recipe_id))
        else {
            continue;
        };
        let per_pass: f32 = recipe
            .outputs
            .iter()
            .filter(|output| output.equipment_id() == Some(item))
            .map(|output| output.amount)
            .sum();
        let crafting_rate = quoted_rate(bench, recipe, recipes, inputs, equipment, wear);
        if per_pass <= 0.0 || crafting_rate <= 0.0 {
            continue;
        }
        let lead = queued_lead + recipe.work / per_pass / crafting_rate;
        let lead_turns = (lead - WHOLE_UNIT_TOLERANCE).ceil().max(MIN_LEAD_TURNS);
        let short = |live: u32| {
            (need - live as f32 - WHOLE_UNIT_TOLERANCE)
                .ceil()
                .max(NOTHING_MISSING)
        };
        let short_now = short(wear.live_units(item, equipment));
        let mut after = wear.clone();
        // **The turn the shortage really starts**, as a fractional turn: the first turn of the
        // clone's walk whose loss leaves more of `need` unheld than today, counted from the turns
        // before it plus the share of that turn the serving unit lasted.
        let mut shortage_starts: Option<f32> = None;
        for turn in 0..lead_turns as u32 {
            let left_in_hand = after.remaining(item, equipment);
            after.charge_condition(equipment, item, rate);
            let live = after.live_units(item, equipment);
            if shortage_starts.is_none() && short(live) > short_now {
                shortage_starts = Some(turn as f32 + (left_in_hand / rate).min(1.0));
            }
            // **Nothing further can change the result** once no live unit is left; the queue ahead
            // has no cap, so the walk must not run its whole length.
            if live == 0 {
                break;
            }
        }
        let units = short(after.live_units(item, equipment)) - short_now;
        let Some(expires_in_turns) = shortage_starts.filter(|_| units > NOTHING_MISSING) else {
            continue;
        };
        forecast.insert(
            item.to_string(),
            UpcomingWear {
                units: units as u32,
                expires_in_turns,
                lead_turns: lead_turns as u32,
            },
        );
    }
    forecast
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
    const SLED_RECIPE: &str = "sled";
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
            kit: "big_game".to_string(),
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

    /// **TWO READINGS OF ONE BAND TAKE THE LARGER SHORTFALL PER ITEM, NEVER THE SUM** - the standing
    /// turn and the turn the hunt goes out are the same people on different days, and a tool made
    /// serves both.
    #[test]
    fn the_larger_of_two_passes_wins_per_item_and_they_are_never_summed() {
        let (recipes, equipment) = configs();
        let standing = [line(take("boar"), SPEARS, 4.0, 2.0)];
        let hunt_turn = [
            line(take("boar"), SPEARS, 1.0, 0.0),
            line(take("mammoth"), SPEARS, 5.0, 2.0),
        ];
        let ranked = craft_suggestions_over(&[&standing, &hunt_turn], None, &recipes, &equipment);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].count, 4, "max(2, 1 + 3), not 2 + 4");
        assert_eq!(
            ranked[0].sources.len(),
            2,
            "the winning pass's sources are the ones listed"
        );

        let alone = craft_suggestions_over(&[&standing], None, &recipes, &equipment);
        assert_eq!(alone[0].count, 2, "control: the standing pass alone");
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

    // ---- KitNeed (#780) ------------------------------------------------------------------

    const STALKING_KIT: &str = "big_game";
    const SLED: &str = "sled";
    const HUNTERS: u32 = 6;

    fn extract(tile: UVec2) -> SupplySource {
        SupplySource::Take {
            job: "extract",
            source: Some(BuildSource::Deposit {
                tile,
                material: "wood".to_string(),
            }),
            kit: "woodcutting".to_string(),
        }
    }

    /// A band of six hunters.
    fn hunting_band() -> LaborAllocation {
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Hunt {
                fauna_id: "deer".to_string(),
                floor: 0.5,
                move_with_herd: false,
            },
            HUNTERS,
            HUNTERS,
            None,
        );
        allocation
    }

    /// A ledger holding `count` live units of `item`.
    fn holding(equipment: &EquipmentConfig, items: &[(&str, u32)]) -> BandEquipment {
        let mut wear = BandEquipment::default();
        for (item, count) in items {
            let tier = equipment.item(item).expect("a shipped item").default_tier();
            wear.stock(item, *count, &tier.id, None);
        }
        wear
    }

    fn stalking(needs: &[KitNeed]) -> &KitNeed {
        needs
            .iter()
            .find(|need| need.kit_id == STALKING_KIT)
            .expect("the stalking kit is orderable")
    }

    fn item_of<'a>(need: &'a KitNeed, item: &str) -> &'a KitNeedItem {
        need.items
            .iter()
            .find(|row| row.item == item)
            .expect("the kit uses the item")
    }

    /// **A sled in a woodcutter's hands does not equip a hunter.** Six hunters, six spears, one sled
    /// used by the hunters and one held by a woodcutter: the stalking kit's spears are all six, its
    /// sled count is the hunters' one - the woodcutter's is another job's.
    #[test]
    fn another_jobs_unit_never_counts_towards_a_kit() {
        let (recipes, equipment) = configs();
        let wear = holding(&equipment, &[(SPEARS, 6), (SLED, 2)]);
        let lines = [
            line(take("deer"), SPEARS, 6.0, 6.0),
            line(take("deer"), SLED, 1.0, 1.0),
            line(extract(PATCH), SLED, 1.0, 1.0),
        ];
        let needs = band_kit_needs(
            &hunting_band(),
            &lines,
            None,
            None,
            &recipes,
            &equipment,
            &wear,
        );
        let kit = stalking(&needs);
        assert_eq!(kit.people, HUNTERS, "the hunters are the kit's people");
        assert_eq!(
            item_of(kit, SPEARS).have,
            6,
            "six spears issued to the six hunters"
        );
        assert_eq!(
            item_of(kit, SLED).have,
            1,
            "the woodcutter's sled is not the hunters' (and no unit is spare)"
        );
        assert_eq!(
            item_of(kit, SLED).workers_per_unit,
            equipment.item(SLED).unwrap().workers_per_unit
        );
    }

    /// **A spare unit nobody holds counts; a queued build's forecast line never counts as held.**
    #[test]
    fn a_spare_unit_counts_and_a_queued_build_line_does_not() {
        let (recipes, equipment) = configs();
        let wear = holding(&equipment, &[(SLED, 3)]);
        let lines = [
            line(take("deer"), SLED, 1.0, 1.0),
            line(extract(PATCH), SLED, 1.0, 1.0),
            line(
                SupplySource::BuildQueue {
                    position: 1,
                    source: BuildSource::Patch(PATCH),
                },
                SLED,
                1.0,
                1.0,
            ),
        ];
        let needs = band_kit_needs(
            &hunting_band(),
            &lines,
            None,
            None,
            &recipes,
            &equipment,
            &wear,
        );
        // Three live sleds, all three spoken for (hunt, extract, the queued build's forecast).
        assert_eq!(item_of(stalking(&needs), SLED).have, 1);

        let spare = holding(&equipment, &[(SLED, 4)]);
        let needs = band_kit_needs(
            &hunting_band(),
            &lines,
            None,
            None,
            &recipes,
            &equipment,
            &spare,
        );
        assert_eq!(
            item_of(stalking(&needs), SLED).have,
            2,
            "the hunters' one plus the one spare sled"
        );
    }

    /// **`queued` is the one `queued_units` the suggestions net by, and `none` is not orderable.**
    #[test]
    fn queued_is_the_benchs_owed_units_and_the_empty_kit_is_not_listed() {
        let (recipes, equipment) = configs();
        let mut bench = BandBench::default();
        bench.enqueue(SLED_RECIPE, 3);
        let needs = band_kit_needs(
            &hunting_band(),
            &[],
            None,
            Some(&bench),
            &recipes,
            &equipment,
            &BandEquipment::default(),
        );
        assert_eq!(item_of(stalking(&needs), SLED).queued, 3);
        assert!(
            needs.iter().all(|need| need.kit_id != "none"),
            "a kit that carries nothing is not orderable"
        );
    }
}
