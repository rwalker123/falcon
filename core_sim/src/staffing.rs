//! ⛔ **THE SIM STAFFS EVERY SOURCE FROM THE PLAYER'S FLOOR** (#871, `docs/plan_labor_at_scale.md`
//! "Decided").
//!
//! The player states the **source**, its **floor**, its **kit** and an optional **Max**; this module
//! decides the **crew**, every turn, with no smoothing. A row's crew is the sum of three kinds of
//! head, staffed in three passes across the whole band:
//!
//! 0. **Scouts, off the top** — the band's scouting range is an explicit order, so its scouts are
//!    dealt first (kitted, then bare) out of the whole pool and shelf; everything below staffs from
//!    what is left.
//! 1. **Keepers, then each group's builders and hold crews** — every row at every priority gets its
//!    **keepers** first (keeping an improvement from sliding down its ladder is the one claim that
//!    outranks everything). Then, per priority group High to Low, the group's builds get their
//!    first builder and the group's rows get **kitted** heads up to the crew that takes what
//!    regrows at the floor (`T_hold`) - a hold crew is a food take, so a Normal row's never beats a
//!    High build. If the band's units run out the row stays short: there is no bare fallback here,
//!    except for the **kit step** and a chief's FOOD order (pass 1c). The kit step runs after pass 1b and
//!    before pass 1c: a workable bench order that makes an equipment item some source row is short of
//!    (the row's useful crew at full kit, under its Max and tile room, exceeds the heads the shelf
//!    still arms), or the bench tool that bounds the material such an item is made from, is crewed
//!    first - up to `crafters_per_item x the units the rows lack` for the kit, to its own cap for the
//!    tool - kitted where the shelf can arm a crafter, else bare, in bench rank order. Missing kit is
//!    made before anyone works bare. The food-first guard ([`staff_band_food_first`]) re-deals without
//!    the step when the larder would run short with the crafters at work.
//!    Then, for a chief's FOOD order (pass 1c): once every group's kitted hold heads are dealt
//!    and before any row's reach, its rows' hold crews are filled with bare hands where a bare hand
//!    takes something, so another order's kitted refill never takes the crew that keeps food coming in.
//!    A crew is sized `ceil(keep_hands + gather hands)`, rounded ONCE: the keeper keeps first and
//!    gathers with what is left, so the fraction of a hand of keeping rides inside a gatherer's head.
//! 2. **Reach, kitted** — with heads and units remaining, kitted heads up to the crew that draws the
//!    stock *down* toward the floor (`T_reach`, strictly above the largest regrowth between the floor
//!    and today's stock).
//! 3. **Up to the useful crew** — in priority order, every row is filled to the crew past which its
//!    take stops rising (the room above the floor, clear-it-now): first with whatever units of its
//!    kit are still unspent (a kit never sits on the shelf while someone works bare), then with bare
//!    heads where an unarmed worker adds value (never on quarry a bare hand cannot wound).
//!
//! Whatever is left is idle. The Max caps **every** pass: no pass puts a row's total above it.
//!
//! # Builds join the crew (#872)
//!
//! A declared build is a **build input** beside the row inputs (`StaffRowInput::is_build`), staffed
//! from the same pool and shelf: in pass 1, per priority group, a build that can progress gets
//! **one builder** ahead of that group's hold crews; then, per priority group in passes 2 and 3,
//! the group's builds are dealt **before** its sources reach toward their floor, up to
//! their cap (the rung's `max_builders`, cut by the room the tile left). A bare-handed builder
//! always contributes, so a build is a pass-3 destination. A tile's room is shared by every row and
//! build on it and by the other bands (`tile_worker_cap`).
//!
//! **The band's own builds are filled one at a time** (`builds_before`, the gate in
//! `Ledger::deal_head`): the fill order is priority group High to Low, then the least work still owed
//! (`StaffRowInput::build_work_left`), then nearest to camp, then row index, and a build is dealt a
//! head in any pass only when every build before it is at its cap or can take nobody. Levy builds
//! keep the levy step's own deal.
//!
//! **Within a priority group every pass deals one head at a time, round-robin**, nearest row to camp
//! first (ties by row index), until no row in the group wants another, the pool is empty or (for a
//! kitted head) the shelf cannot arm one more; only then does the next lower group get that pass.
//! Distance orders a round and never excludes a row.
//!
//! # Two halves, so the passes can be tested without a world
//!
//! [`staff_band`] is a pure function over [`StaffRowInput`]s — every web's arithmetic arrives as a
//! closure (`hands`: the take hands that reach a target take with the first `k` of them armed) so the
//! passes are one algorithm for forage, hunt and extraction alike. [`staff_rows`] builds those
//! inputs from the world through the **same** primitives the walk takes with
//! ([`crate::take_claims::ClaimSources`], `forage_crew_take_room`, `hunt_crew_needed`,
//! `deposit_crew_needed`, `KitCoverage::hands_to_reach`), and is callable from the turn and from a
//! command handler alike, which is how the snapshot shows the new crew the moment the player acts.
//!
//! # What is deliberately approximate
//!
//! - **Gear reserved for the standing pools and for site keeping** is not known at staffing time
//!   (it is settled later in the same turn), so the ledger is the band's whole live stock.
//! - **A source two bands work** sizes each band's keepers against the whole bill, not its crew
//!   share ([`crate::systems`] `SiteCrews`): the share is a function of the crews being decided.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::UVec2;

use crate::components::{
    BandEquipment, BandId, BuildCrew, BuildSource, LaborAllocation, LaborTarget, SourcePriority,
    StaffedCrew,
};
use crate::equipment_config::{KitChoice, KitCoverage, Quarry};
use crate::fauna::PartyResolution;
use crate::take_claims::{declared_on, planned_keep_hands, ClaimSources};

/// **HOW FAR ABOVE THE PEAK REGROWTH A CREW MUST TAKE TO DRAW A STOCK DOWN** — a relative margin,
/// because the target is *strictly* above the largest one-turn regrowth between the floor and
/// today's stock: a crew that takes exactly the peak holds the stock where it is. The client's
/// `crew_that_reaches` reaches the same place with `floor(peak ÷ per_worker) + 1` hands.
pub const REACH_STRICTLY_ABOVE_PEAK: f32 = 0.001;

/// **NO TAKE IS WANTED** — a source with nothing workable above its floor, a tile improvement or a
/// road pool (keepers only).
const NO_TAKE_WANTED: f32 = 0.0;

/// **NOBODY** — the empty crew.
const NOBODY: u32 = 0;

/// **ANY TAKE AT ALL NEEDS A HAND** - a source bounded by neither carry nor reach (a pen is a
/// larder, its engagement unbounded) reports no crew for a target, but nothing is taken by nobody.
const AT_LEAST_ONE_HAND: u32 = 1;

/// **NO HANDS** - the empty fractional crew.
const NO_HANDS: f32 = 0.0;

/// **NO FOOD** - a row whose take is not food (a dig, a tile or band improvement, a build).
const NO_FOOD: f32 = 0.0;

/// **WHAT THE BAND'S LARDER LOOKS LIKE TO THE FOOD-FIRST CHECK**: the food it holds and what its
/// people need each turn (`PopulationCohort::last_food_need`). A need of nothing switches the check
/// off, so a world with no people (a probe, a fixture) never trips it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FoodLedger {
    /// The food in the larder now.
    pub larder: f32,
    /// What the people eat each turn: the cohort's `food_demand` at its current brackets.
    pub need: f32,
    /// **The food a turn that is not a sim-staffed source row**: the band's steady pooling net
    /// (`snapshot::pooling_steady_nets`, the standing term the published runway adds) and each need
    /// row at its `sustainable` rate, as `FoodFlow` reads it.
    pub standing: f32,
}

/// **THE BAND'S LARDER AS THE FOOD-FIRST CHECK READS IT**: the food held, this turn's meal demand
/// (`food_demand` over the current brackets and the party the band feeds - the runway's own need),
/// and the standing income that is not a staffed row. Spoilage is deliberately not in it.
pub(crate) fn food_ledger_of(
    cohort: &crate::components::PopulationCohort,
    allocation: &LaborAllocation,
    demographics: &crate::demographics_config::DemographicsConfig,
    pooling_steady: f32,
) -> FoodLedger {
    let need = crate::systems::food_demand(
        cohort.children,
        cohort.working + cohort.fed_party_workers,
        cohort.elders,
        &demographics.consumption,
    )
    .to_f32();
    let need_rows: f32 = allocation
        .assignments
        .iter()
        .zip(&allocation.last_yields)
        .filter(|(row, _)| row.is_need_row())
        .map(|(_, yields)| yields.sustainable)
        .sum();
    FoodLedger {
        larder: cohort.stores.get(crate::components::FOOD).to_f32(),
        need,
        standing: pooling_steady + need_rows,
    }
}

/// **THE STEADY POOLING NET OF ONE BAND, read off a world** - the command path's twin of the turn's
/// per-network pre-pass (`snapshot::pooling_steady_nets`), `0` in no network or with no membership.
fn pooling_steady_of(world: &bevy::prelude::World, band: bevy::prelude::Entity) -> f32 {
    let (Some(membership), Some(supply), Some(demographics)) = (
        world.get_resource::<crate::supply::SupplyNetworkMembership>(),
        world.get_resource::<crate::supply_network_config::SupplyNetworkConfigHandle>(),
        world.get_resource::<crate::demographics_config::DemographicsConfigHandle>(),
    ) else {
        return NO_FOOD;
    };
    let bands: Vec<(
        bevy::prelude::Entity,
        &crate::components::PopulationCohort,
        Option<&LaborAllocation>,
    )> = world
        .iter_entities()
        .filter_map(|entity| {
            Some((
                entity.id(),
                entity.get::<crate::components::PopulationCohort>()?,
                entity.get::<LaborAllocation>(),
            ))
        })
        .collect();
    crate::snapshot::population::pooling_steady_nets(
        bands.into_iter(),
        membership,
        &demographics.get(),
        &supply.get(),
    )
    .get(&band)
    .copied()
    .unwrap_or(NO_FOOD)
}

impl FoodLedger {
    /// No larder to read: the check is off.
    pub const OFF: FoodLedger = FoodLedger {
        larder: 0.0,
        need: 0.0,
        standing: 0.0,
    };
}

/// **WHAT A BUILD IS WORTH TO THE LARDER WHEN IT FINISHES**: the work it still owes, what one builder
/// banks a turn, and the food a turn the finished rung adds. Plain values the world half resolves
/// ([`build_inputs`]); a build with no food gain carries `None` instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildFinish {
    /// Work units still owed on the climb.
    pub work_remaining: f32,
    /// Work units one builder banks a turn.
    pub work_per_builder: f32,
    /// Food a turn the finished rung adds, from the turn after it completes.
    pub gain: f32,
}

/// What the reach pass is aiming at on one row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReachTarget {
    /// Nothing to draw down — the row is below its floor, or keeps and takes nothing.
    Nothing,
    /// Draw the stock down at this take per turn (already strictly above the peak regrowth).
    Take(f32),
    /// A hand-crewed mined seam with no regrowth: there is no regrowth to out-take, so the target is
    /// the player's Max in heads - this many **take hands**. (A leader's keep-on-hand order sizes a
    /// mined seam by its deficit instead.)
    Heads(u32),
}

/// A hunt row's sustained take and the hands closure that reaches it with no room bound
/// ([`StaffRowInput::sustained`]).
pub struct SustainedTake<'a> {
    /// The regrowth at the floor, in biomass a turn.
    pub take: f32,
    /// [`StaffRowInput::hands`] with the room bound lifted.
    pub hands: Box<dyn Fn(f32, u32) -> Option<f32> + 'a>,
}

/// One staffed row, as the passes see it.
pub struct StaffRowInput<'a> {
    /// The player's rank for the row ([`SourcePriority`]) — the order passes serve rows in.
    pub priority: SourcePriority,
    /// The player's Max: the most heads the sim may put on the row. `None` is no max.
    pub max: Option<u32>,
    /// **The keeping hands the row owes its site, FRACTIONAL** - the turn's keeping takes exactly this
    /// much of the crew's work and the rest gathers, so the passes round `keep_hands + gather hands`
    /// once rather than ceiling the keeping and the gathering apart.
    pub keep_hands: f32,
    /// `T_hold` — the take per turn that holds the source at its floor (what regrows there).
    pub hold_take: f32,
    /// `T_reach` — what the reach pass aims at.
    pub reach: ReachTarget,
    /// **THE USEFUL TAKE** - the room above the floor (clear-it-now): the take past which more hands
    /// add nothing. Pass 3 staffs a row up to the crew that reaches it.
    pub useful_take: f32,
    /// The kit's items the band can still arm with, each with the people one unit arms.
    pub kit_items: Vec<(Arc<str>, u32)>,
    /// **The take hands that reach `target` take per turn when the first `k` of them are armed**
    /// and the rest bare. `None` where no such crew reaches it at all (a party that cannot wound the
    /// quarry). Fractional — the passes round once.
    pub hands: Box<dyn Fn(f32, u32) -> Option<f32> + 'a>,
    /// Whether an unarmed hand adds anything to the row's take (forage: a bare rate above nothing;
    /// hunt: a bare hunter can wound the quarry).
    pub bare_can_work: bool,
    /// Hex distance from camp to the row's site (a hunt row's herd at its current tile); Roadwork
    /// has no single site and reads 0. Orders the heads of a priority group, nearest first.
    pub distance: u32,
    /// **This input is a declared BUILD, not a source row** (#872). Its `max` is the most builders
    /// it can take (zero when it cannot progress), its `keepers` and takes are nothing, and a bare
    /// hand always contributes.
    pub is_build: bool,
    /// **This input is a crafting-bench ORDER** (#873): it joins the passes like a build - dealt with
    /// its group's builds ahead of the group's sources - but has no hold crew (never pass 1) and
    /// opens pass 2 with one crafter on every workable order. Its `max` is the order's crew cap, its
    /// kit is the bench tool (one unit arms one crafter) and a bare hand works only where the
    /// material has a `hand_working` rate.
    pub is_bench: bool,
    /// A build whose rung asks for no tool: every builder counts as kitted.
    pub kitless: bool,
    /// The tile the row's or build's hands stand on, when it is tile-bound; its room is shared with
    /// every other row and build on it and with the other bands.
    pub tile: Option<UVec2>,
    /// **A ROW THE CHIEF'S FOOD ORDER KEEPS UP**: its hold crew is dealt bare hands (where a bare
    /// hand takes something) in pass 1c, ahead of every row's kitted reach heads.
    pub food_keep_up: bool,
    /// **THE ROW'S TAKE HANDS ARRIVE AS A WHOLE CREW** (a hunt: haul, reach and fight terms are each
    /// ceiled by [`crate::fauna::hunt_crew_for_take`]). Such a crew's last head has slack by
    /// construction, so the fraction of a hand of keeping rides in it exactly as it rides in a
    /// fractional gatherer's head: only the whole keepers add heads.
    pub whole_hands: bool,
    /// **THE SUSTAINED TAKE A HOLD CREW IS SIZED ON, where it is not the room.** A hunt row's hold
    /// crew takes the regrowth at the floor every turn, which can be more than the stock above the
    /// floor this turn (`hold_take` is bounded by that room). The keeping's fit in a hunter's slack
    /// is asked against this take with the room bound lifted, so a hand whose leftover reach cannot
    /// take the regrowth is not counted as holding it. `None` for every row that sizes on its own
    /// `hold_take`.
    pub sustained: Option<SustainedTake<'a>>,
    /// **Food a unit of this row's take is worth** (provisions per biomass); [`NO_FOOD`] where the
    /// take is not food. Read by the food-first check ([`band_runs_short`]) to price a crew.
    pub food_per_take: f32,
    /// **What finishing this build adds to the larder** (a build input only).
    pub finish: Option<BuildFinish>,
    /// **A LEVY ROW OF THE BAND'S TRIBE** (#862): skipped in passes 0 and 1, dealt in the levy step
    /// and after it from the levy budget.
    pub levy: bool,
    /// **WHAT A BENCH ORDER MAKES** (a bench input only): read by the kit step, which crews the
    /// orders that make the equipment the source rows are short of, and by the food-first check's
    /// credit for the kit such an order finishes.
    pub bench_kit: Option<crate::systems::BenchKit>,
    /// **EVERY ITEM OF THE ROW'S CHOSEN KIT, live or not**, each with the people one unit arms. Read
    /// only by the kit step, so a band whose baskets have all worn out (or never had any) is still
    /// seen to be short of them; arming ([`Self::kit_items`]) is unchanged and lists live items only.
    pub kit_wants: Vec<(Arc<str>, u32)>,
    /// **WORK STILL OWED ON A BUILD'S CLIMB** (a build input only; `0` elsewhere): the sum of the legs
    /// the build's turn estimate dates (`systems::declared_build_work_remaining`). Orders the band's
    /// own builds, least left first, so the one closest to done is filled first.
    pub build_work_left: f32,
}

/// What the passes decided for one row, in the order the inputs came.
pub type RowStaffing = StaffedCrew;

/// The most heads a row may hold, given its Max.
fn cap_of(row: &StaffRowInput<'_>) -> u32 {
    row.max.unwrap_or(u32::MAX)
}

/// Whole units of `item` a row holding `heads` kitted heads has consumed.
fn units_for(heads: u32, people_per_unit: u32) -> u32 {
    heads.div_ceil(people_per_unit.max(1))
}

/// **THE MOST KITTED HEADS THE ROW COULD HOLD** — the heads it holds already plus what the units
/// still on the shelf arm, the scarcest item deciding (a kit is claimed whole).
fn kitted_capacity(row: &StaffRowInput<'_>, held: u32, shelf: &BTreeMap<Arc<str>, u32>) -> u32 {
    if row.kitless {
        return u32::MAX;
    }
    row.kit_items
        .iter()
        .map(|(item, people)| {
            let consumed = units_for(held, *people);
            let left = shelf.get(item).copied().unwrap_or(NOBODY);
            (consumed + left).saturating_mul((*people).max(1))
        })
        .min()
        .unwrap_or(NOBODY)
}

/// Move a row from `held` to `target` kitted heads, taking the difference off the shelf.
fn arm(row: &StaffRowInput<'_>, held: u32, target: u32, shelf: &mut BTreeMap<Arc<str>, u32>) {
    for (item, people) in &row.kit_items {
        let delta = units_for(target, *people).saturating_sub(units_for(held, *people));
        if let Some(units) = shelf.get_mut(item) {
            *units = units.saturating_sub(delta);
        }
    }
}

/// **The take hands, fractional,** that reach `target` with `armed` of them kitted; nothing where no
/// take is wanted or no crew reaches it.
fn gather_hands(row: &StaffRowInput<'_>, target: f32, armed: u32) -> f32 {
    if target <= NO_TAKE_WANTED {
        return NO_HANDS;
    }
    // A source bounded by neither carry nor reach reports no crew for a target, but nothing is taken
    // by nobody: a wanted take is at least one hand.
    (row.hands)(target, armed).map_or(NO_HANDS, |hands| {
        if hands > NO_HANDS {
            hands
        } else {
            AT_LEAST_ONE_HAND as f32
        }
    })
}

/// **THE WHOLE-HEAD KEEPERS OF A ROW** - the heads whose turn is entirely keeping.
fn whole_keepers(row: &StaffRowInput<'_>) -> u32 {
    row.keep_hands.max(NO_HANDS).floor() as u32
}

/// Whether the keeping leaves a fraction of a hand over, which rides inside a gatherer's head.
fn keeps_a_fraction(row: &StaffRowInput<'_>) -> bool {
    row.keep_hands > whole_keepers(row) as f32
}

/// **A ROW THAT HOLDS A TAKE** carries the keeping's fractional remainder in a gatherer's head; a row
/// with no hold take (a tile or band improvement, a source at its floor) gives that remainder a
/// keeper's head of its own so the improvement is still kept.
fn remainder_rides_with_a_gatherer(row: &StaffRowInput<'_>) -> bool {
    row.hold_take > NO_TAKE_WANTED
}

/// **THE HEADS A CREW NEEDS IN ALL**, keeping and gathering, rounded up ONCE: the keeper keeps first
/// and gathers with what is left, exactly as the turn splits it.
///
/// Where the gatherers' hands are a whole crew already ([`StaffRowInput::whole_hands`]) the keeping's
/// fraction rides inside their slack and only the whole keepers add heads.
/// The fraction rides only where it FITS: the whole crew, with the fractional keeping taken out of
/// its work, must still reach `target` with `armed` of them kitted (asked of the row's own `hands`
/// closure at the take that crew would need to make up the keeping). Otherwise one head is added.
/// **DOES THE KEEPING'S FRACTION RIDE IN A WHOLE CREW'S SLACK?** `crew` whole hunters, with `fraction`
/// of a hand taken out of their work, must still reach `target` with `armed` of them kitted (asked of
/// the row's own `hands` closure at the take the crew would need to make up the keeping). The one rule
/// [`crew_heads`] sizes a hunt crew by and [`crew_take_capacity`] prices it by.
fn keeping_fraction_fits(
    row: &StaffRowInput<'_>,
    crew: f32,
    fraction: f32,
    target: f32,
    armed: u32,
) -> bool {
    keeping_fraction_fits_by(&*row.hands, crew, fraction, target, armed)
}

/// [`keeping_fraction_fits`] asked of a given hands closure.
fn keeping_fraction_fits_by(
    hands: &dyn Fn(f32, u32) -> Option<f32>,
    crew: f32,
    fraction: f32,
    target: f32,
    armed: u32,
) -> bool {
    fraction <= NO_HANDS || {
        let working = crew - fraction;
        working > NO_HANDS && hands(target * crew / working, armed).is_none_or(|need| need <= crew)
    }
}

fn crew_heads(row: &StaffRowInput<'_>, gather: f32, target: f32, armed: u32) -> u32 {
    crew_heads_by(row, gather, target, armed, &*row.hands)
}

/// [`crew_heads`] with the keeping's fit asked of `fit_hands`.
fn crew_heads_by(
    row: &StaffRowInput<'_>,
    gather: f32,
    target: f32,
    armed: u32,
    fit_hands: &dyn Fn(f32, u32) -> Option<f32>,
) -> u32 {
    if row.whole_hands && gather > NO_HANDS {
        let whole = whole_keepers(row);
        let crew = gather.ceil();
        let fraction = row.keep_hands.max(NO_HANDS) - whole as f32;
        let fits = keeping_fraction_fits_by(fit_hands, crew, fraction, target, armed);
        return whole + crew as u32 + u32::from(!fits);
    }
    (row.keep_hands.max(NO_HANDS) + gather).ceil() as u32
}

/// The take the reach pass aims at, where it is a take (a `Heads` reach is whole heads, not a take).
fn reach_take(row: &StaffRowInput<'_>) -> f32 {
    match row.reach {
        ReachTarget::Take(target) => target,
        ReachTarget::Nothing | ReachTarget::Heads(_) => NO_TAKE_WANTED,
    }
}

/// The crew that holds the source at its floor, keepers included. A row with a sustained take (a
/// hunt) is sized on it: the hands that take the regrowth at the floor, the keeping's fraction
/// riding in a hunter's slack only where the slack reaches that regrowth.
fn hold_heads(row: &StaffRowInput<'_>, armed: u32) -> u32 {
    let heads = match &row.sustained {
        Some(sustained) if sustained.take > row.hold_take => {
            let gather = (sustained.hands)(sustained.take, armed).map_or(NO_HANDS, f32::ceil);
            crew_heads_by(row, gather, sustained.take, armed, &*sustained.hands)
        }
        _ => crew_heads(
            row,
            gather_hands(row, row.hold_take, armed),
            row.hold_take,
            armed,
        ),
    };
    if row.hold_take > NO_TAKE_WANTED {
        heads.max(AT_LEAST_ONE_HAND)
    } else {
        heads
    }
}

/// The take hands the reach pass aims at.
fn reach_gather(row: &StaffRowInput<'_>, armed: u32) -> f32 {
    match row.reach {
        ReachTarget::Nothing => NO_HANDS,
        ReachTarget::Take(target) => gather_hands(row, target, armed),
        ReachTarget::Heads(heads) => heads as f32,
    }
}

/// The crew that draws the stock down toward the floor, keepers included.
fn reach_heads(row: &StaffRowInput<'_>, armed: u32) -> u32 {
    let hold = gather_hands(row, row.hold_take, armed);
    crew_heads(
        row,
        reach_gather(row, armed).max(hold),
        reach_take(row).max(row.hold_take),
        armed,
    )
    .max(hold_heads(row, armed))
}

/// The crew past which the take stops rising, keepers included.
fn useful_heads(row: &StaffRowInput<'_>, armed: u32) -> u32 {
    let hold = gather_hands(row, row.hold_take, armed);
    let useful = match row.reach {
        ReachTarget::Heads(heads) => heads as f32,
        _ => gather_hands(row, row.useful_take, armed),
    };
    crew_heads(
        row,
        useful.max(reach_gather(row, armed)).max(hold),
        row.useful_take.max(reach_take(row)).max(row.hold_take),
        armed,
    )
    .max(hold_heads(row, armed))
}

/// Which passes a staffing run deals.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Passes {
    /// Passes 1 and 2 only: the keepers and the hold and reach crews - the hands that keep working
    /// once a source sits at its floor. The drawdown (pass 3) is left out.
    Steady,
    /// All three passes: the crews the band really works this turn.
    All,
}

/// **THE THREE PASSES** — see the module doc. `pool` is the hands the sim may staff;
/// `shelf` the band's live units per item (taken off as rows are armed).
pub fn staff_band(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
) -> Vec<RowStaffing> {
    staff_passes(rows, pool, shelf, Passes::All, &BTreeMap::new())
}

/// **THE BAND'S SCOUTING RANGE AS THE PASSES SEE IT** (#875) - the tiles past the local range the
/// player asked to see, what one scout of each kind adds, and the wayfinding items a kitted scout
/// is armed with. Plain values, like [`StaffRowInput`]: the world half resolves them
/// ([`scout_plan`]).
#[derive(Debug, Clone, Default)]
pub struct ScoutPlan {
    /// **A tribe member's levy** (#862): the budget its tribe may take from the band's free hands.
    /// `Some` for every band in a tribe - its scouts and its levy rows are dealt after pass 1, from
    /// the budget - and `None` for a band in no tribe (scouts in pass 0, no levy rows).
    pub levy: Option<LevyPlan>,
    /// **The share of the tribe's scouting edge this band covers** (`UNSTRETCHED` for a band in no
    /// tribe): the stretch needs `stretch x range` scout-tiles and the reach the crew buys is
    /// divided by it. `0` = no scouts.
    pub stretch: f32,
    /// Tiles past the local range the band should see. `0` = no scouts.
    pub range: u32,
    /// Tiles one **kitted** scout adds (the scout row's kit at its equipped vantage tier).
    pub kitted_tiles: u32,
    /// Tiles one **bare** scout adds (the bare vantage tier).
    pub bare_tiles: u32,
    /// The scout row's kit items the band can still arm with, each with the people one unit arms.
    /// Empty for kit `none`, so no scout is kitted.
    pub kit_items: Vec<(Arc<str>, u32)>,
}

/// **A TRIBE'S LEVY ON ONE MEMBER BAND** (#862): the budget is `floor(share x factor x hands_left)`
/// where `hands_left` is the pool after pass 1 - at least one head while `share > 0` and a hand is
/// left. `share` is the tribe's ceiling, `factor` the share of its orders the tribe leader's reach
/// carries to this member ([`crate::leadership::tribe_orders_carried_out_for_member`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LevyPlan {
    pub share: f64,
    pub factor: f64,
    /// A budget already struck (the builds-last deal's second half spends what the first left).
    pub fixed_budget: Option<u32>,
    /// The tiles of the tribe's road works: a declared build on `BuildSource::Road` of one of these
    /// is the tribe's levy build (a road has no labor row to say so), dealt in the levy step.
    pub road_tiles: Vec<UVec2>,
}

impl LevyPlan {
    /// Whether a declared build is the tribe's road build.
    pub fn levies_road_build(&self, source: &BuildSource) -> bool {
        matches!(source, BuildSource::Road(tile) if self.road_tiles.contains(tile))
    }
}

/// The levy budget: `floor(share x factor x hands_left)`, at least one head while the ceiling allows
/// any and a hand is left, never more than the hands left.
pub fn levy_budget(share: f64, factor: f64, hands_left: u32) -> u32 {
    if share <= NO_SHARE || hands_left == NOBODY {
        return NOBODY;
    }
    ((share * factor * f64::from(hands_left)).floor() as u32)
        .max(AT_LEAST_ONE_HAND)
        .min(hands_left)
}

/// A ceiling that takes nothing.
const NO_SHARE: f64 = 0.0;

/// **WHO THE SCOUT STEP PLACED AND HOW FAR THEY REACH** - the Scout row's crew and the tiles past
/// the local range its heads see.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScoutCrew {
    pub kitted: u32,
    pub bare: u32,
    /// `kitted x kitted_tiles + bare x bare_tiles`: every tile the crew's sight covers, which can
    /// pass the range when the last scout's sight overshoots it.
    pub reach: u32,
}

impl ScoutCrew {
    /// Every scout placed.
    pub fn total(&self) -> u32 {
        self.kitted + self.bare
    }
}

/// **THE SCOUT STEP** - kitted scouts first (the shelf can arm them, up to the heads the range
/// needs at the kitted tier), then bare ones for what is left of the range, each out of the pool
/// that remains. A tier that rounds to no tiles contributes nobody, so the step cannot loop on it.
fn deal_scouts(
    plan: &ScoutPlan,
    pool_left: &mut u32,
    budget: Option<&mut u32>,
    shelf: &mut BTreeMap<Arc<str>, u32>,
) -> ScoutCrew {
    // A member enclosed by the others (stretch 0) watches no edge of its own: no scouts.
    if plan.range == NOBODY || plan.stretch <= NO_STRETCH {
        return ScoutCrew::default();
    }
    let allowed = budget.as_deref().copied().unwrap_or(u32::MAX);
    // The stretch needs `stretch x range` scout-tiles (a lone band's `1.0` is the range itself).
    let needed = (plan.stretch * plan.range as f32).ceil() as u32;
    let arming = scout_arming_input(plan);
    let kitted_wanted = if plan.kitted_tiles == NOBODY {
        NOBODY
    } else {
        needed.div_ceil(plan.kitted_tiles)
    };
    let kitted = kitted_capacity(&arming, NOBODY, shelf)
        .min(kitted_wanted)
        .min(*pool_left)
        .min(allowed);
    arm(&arming, NOBODY, kitted, shelf);
    *pool_left -= kitted;
    let still_to_reach = needed.saturating_sub(kitted.saturating_mul(plan.kitted_tiles));
    let bare_wanted = if plan.bare_tiles == NOBODY {
        NOBODY
    } else {
        still_to_reach.div_ceil(plan.bare_tiles)
    };
    let bare = bare_wanted.min(*pool_left).min(allowed - kitted);
    *pool_left -= bare;
    if let Some(budget) = budget {
        *budget -= kitted + bare;
    }
    let seen = kitted
        .saturating_mul(plan.kitted_tiles)
        .saturating_add(bare.saturating_mul(plan.bare_tiles));
    // The reach this band's own stretch buys. At or past the scout-tiles its stretch needs, the
    // reach is the range plus the raw surplus (a lone band's overshoot); short of them, the
    // scout-tiles spread over the share of the edge (always under the range, so the two meet).
    let reach = if seen >= needed {
        plan.range + (seen - needed)
    } else {
        (seen as f32 / plan.stretch).floor() as u32
    };
    ScoutCrew {
        kitted,
        bare,
        reach,
    }
}

/// A stretch that covers none of the tribe's edge.
const NO_STRETCH: f32 = 0.0;

/// The scout row seen as an input that only arms: `kitted_capacity` and `arm` read nothing else.
fn scout_arming_input(plan: &ScoutPlan) -> StaffRowInput<'static> {
    StaffRowInput {
        priority: SourcePriority::default(),
        max: None,
        keep_hands: NO_HANDS,
        hold_take: NO_TAKE_WANTED,
        reach: ReachTarget::Nothing,
        useful_take: NO_TAKE_WANTED,
        kit_items: plan.kit_items.clone(),
        hands: Box::new(|_, _| Some(0.0)),
        bare_can_work: false,
        distance: NOBODY,
        is_build: false,
        is_bench: false,
        kitless: false,
        tile: None,
        food_keep_up: false,
        whole_hands: false,
        sustained: None,
        food_per_take: NO_FOOD,
        finish: None,
        levy: false,
        bench_kit: None,
        kit_wants: Vec::new(),
        build_work_left: NO_HANDS,
    }
}

/// **THE FOOD THE ROW'S CREW BRINGS HOME A TURN, at its best** - the largest take (biomass) the row's
/// own `hands` closure says `crew` take hands reach with `armed` of them kitted, capped at the room
/// above the floor, priced at the row's food rate. Found by bisecting the closure, so every web (a
/// basket's linear hands, a hunt's whole crews) answers through the one function it is staffed by.
fn crew_take_capacity(row: &StaffRowInput<'_>, crew: &StaffedCrew) -> f32 {
    let hands = if row.whole_hands {
        // A hunt's take hands are a WHOLE crew: only the whole keepers come off it, and the keeping's
        // fraction rides in the hunters' slack where it fits (as `crew_heads` deals it), else it cost
        // one head of its own.
        let whole = whole_keepers(row);
        let heads = crew.total().saturating_sub(whole) as f32;
        let fraction = row.keep_hands.max(NO_HANDS) - whole as f32;
        if keeping_fraction_fits(row, heads, fraction, row.useful_take, crew.kitted) {
            heads
        } else {
            (heads - 1.0).max(NO_HANDS)
        }
    } else {
        (crew.total() as f32 - row.keep_hands.max(NO_HANDS)).max(NO_HANDS)
    };
    if hands <= NO_HANDS || row.useful_take <= NO_TAKE_WANTED {
        return NO_TAKE_WANTED;
    }
    let reaches = |target: f32| (row.hands)(target, crew.kitted).is_some_and(|need| need <= hands);
    if reaches(row.useful_take) {
        return row.useful_take;
    }
    let (mut low, mut high) = (NO_TAKE_WANTED, row.useful_take);
    for _ in 0..TAKE_BISECTION_STEPS {
        let mid = 0.5 * (low + high);
        if reaches(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// **HOW FINELY A CREW'S TAKE IS BISECTED** - the interval halves this many times, which is far
/// below a hundredth of a unit of food on any row.
const TAKE_BISECTION_STEPS: u32 = 24;

/// **WOULD THE BAND MISS A MEAL IN THE NEXT `window` TURNS AT THESE CREWS?** The larder is walked in
/// the order the runway reads it: the meal is checked and eaten first, then the turn's income lands.
/// Income is each food row's take at its crew (the room above its floor drawn down, the regrowth at
/// the floor refilling it), plus - from the turn after it completes at the crew it was dealt - the
/// food a finishing build adds ([`BuildFinish`]).
pub fn band_runs_short(
    rows: &[StaffRowInput<'_>],
    crews: &[StaffedCrew],
    ledger: FoodLedger,
    window: u32,
) -> bool {
    if window == 0 || ledger.need <= NO_FOOD {
        return false;
    }
    let mut room: Vec<f32> = rows.iter().map(|row| row.useful_take).collect();
    let mut capacity: Vec<f32> = rows
        .iter()
        .zip(crews)
        .map(|(row, crew)| {
            if row.food_per_take > NO_FOOD && !row.is_build {
                crew_take_capacity(row, crew)
            } else {
                NO_TAKE_WANTED
            }
        })
        .collect();
    let finishes: Vec<(u32, f32)> = rows
        .iter()
        .zip(crews)
        .filter_map(|(row, crew)| {
            let finish = row.finish?;
            let builders = crew.total();
            let per_turn = builders as f32 * finish.work_per_builder;
            (row.is_build && per_turn > NO_HANDS && finish.gain > NO_FOOD).then(|| {
                (
                    (finish.work_remaining / per_turn).ceil() as u32,
                    finish.gain,
                )
            })
        })
        .collect();
    let mut arrivals = kit_arrivals(rows, crews);
    let mut raised: Vec<u32> = vec![NOBODY; rows.len()];
    let mut larder = ledger.larder;
    for turn in 0..window {
        if larder < ledger.need {
            return true;
        }
        for arrival in arrivals
            .iter_mut()
            .filter(|a| !a.applied && turn >= a.finished_in)
        {
            arrival.applied = true;
            arm_short_rows(rows, crews, arrival, &mut raised, &mut capacity);
        }
        let mut income = NO_FOOD;
        for (i, row) in rows.iter().enumerate() {
            if capacity[i] <= NO_TAKE_WANTED {
                continue;
            }
            let take = capacity[i].min(room[i].max(NO_TAKE_WANTED));
            income += take * row.food_per_take;
            room[i] = room[i] - take + row.hold_take;
        }
        income += ledger.standing;
        income += finishes
            .iter()
            .filter(|(finished_in, _)| turn >= *finished_in)
            .map(|(_, gain)| gain)
            .sum::<f32>();
        larder = larder - ledger.need + income;
    }
    false
}

/// **A KIT ORDER'S FINISHED UNITS, as the food-first walk sees them**: when the order dealt `crews`
/// finishes its remaining work, how many units of which item it hands the rows. A tool order earns
/// none (the walk is conservative about it).
struct KitArrival {
    /// The turn (0-based, as the walk counts) from which the units are in the rows' hands.
    finished_in: u32,
    item: Arc<str>,
    units: u32,
    applied: bool,
}

/// The kit orders (an order making equipment, not a bench tool) the deal crewed, and when each
/// finishes: `ceil(work remaining / per-turn progress at the dealt crew)`, the per-turn progress
/// being the bench's own order rate (`Σ hands x craft speed x progress per worker-turn`).
fn kit_arrivals(rows: &[StaffRowInput<'_>], crews: &[StaffedCrew]) -> Vec<KitArrival> {
    rows.iter()
        .zip(crews)
        .filter_map(|(row, crew)| {
            let kit = row.bench_kit.as_ref()?;
            let credit = kit.credit.as_ref()?;
            if !row.is_bench || !kit.tool_serves.is_empty() || crew.total() == NOBODY {
                return None;
            }
            let per_turn = crew.kitted as f32 * credit.rate_per_tool_crafter
                + crew.bare as f32 * credit.rate_per_hand_crafter;
            (per_turn > NO_HANDS).then(|| KitArrival {
                finished_in: (credit.work_remaining / per_turn).ceil() as u32,
                item: Arc::clone(&credit.item),
                units: credit.units.floor() as u32,
                applied: false,
            })
        })
        .collect()
}

/// **THE ROWS SHORT OF A FINISHED ITEM TAKE AS IF THAT MANY MORE OF THEIR HEADS WERE ARMED**, High
/// priority and nearest first, never above the crew they were dealt; the capacity is re-struck
/// through the row's own `hands` closure ([`crew_take_capacity`]). Only a row whose kit is that one
/// item is credited: a row that needs a second item the shelf may lack is not promised its heads.
fn arm_short_rows(
    rows: &[StaffRowInput<'_>],
    crews: &[StaffedCrew],
    arrival: &KitArrival,
    raised: &mut [u32],
    capacity: &mut [f32],
) {
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|&i| {
            let row = &rows[i];
            !row.is_build
                && !row.is_bench
                && row.food_per_take > NO_FOOD
                && matches!(row.kit_items.as_slice(), [(item, _)] if *item == arrival.item)
        })
        .collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(rows[i].priority), rows[i].distance, i));
    let mut units_left = arrival.units;
    for i in order {
        let people = rows[i].kit_items[0].1.max(1);
        let unarmed = crews[i]
            .total()
            .saturating_sub(crews[i].kitted + raised[i] + crews[i].keepers);
        let heads = unarmed.min(units_left.saturating_mul(people));
        if heads == NOBODY {
            continue;
        }
        units_left -= units_for(heads, people).min(units_left);
        raised[i] += heads;
        let mut credited = crews[i];
        credited.kitted += raised[i];
        credited.bare = credited.bare.saturating_sub(raised[i]);
        capacity[i] = crew_take_capacity(&rows[i], &credited);
    }
}

/// **THE THREE PASSES, FOOD FIRST WHEN THE BAND WOULD GO HUNGRY.** The deal runs in today's order
/// (builds ahead of their group's hold crews). If the larder walked forward `window` turns at the
/// crews it produced comes up short ([`band_runs_short`]) and a build holds builders, the band is
/// re-dealt with every source row and bench order before any build: they get the whole pool and
/// shelf, the builds only what they leave, and the keepers (pass 1a) still go first on every row.
/// The last value is whether the second deal was the one returned.
pub fn staff_band_food_first(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    tile_room: &BTreeMap<TileKey, u32>,
    scouts: &ScoutPlan,
    ledger: FoodLedger,
    window: u32,
) -> (Vec<RowStaffing>, ScoutCrew, bool) {
    let (dealt, choice) =
        deal_band_food_first(rows, pool, shelf, tile_room, scouts, ledger, window);
    (dealt.out, dealt.scout, choice.fed_first)
}

/// **WHICH DEAL THE FOOD-FIRST GUARD CHOSE.**
struct FoodFirstChoice {
    /// When the builds were dealt in the deal returned.
    builds: BuildsDealt,
    /// Whether the kit step ran in it.
    kit: KitStep,
    /// Whether the band was re-dealt with builds last because builders were dealt and the larder
    /// would have run short.
    fed_first: bool,
}

/// [`staff_band_food_first`] with the bench's readings ([`Dealt`]).
///
/// **The guard's deals, in order:** A (kit step on, builds in turn); if builders were dealt and A
/// runs short, B (kit step on, builds last); if the last deal tried dealt any kit crafter and still
/// runs short, C (kit step off, builds last). The last deal tried is returned when none is fine.
fn deal_band_food_first(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    tile_room: &BTreeMap<TileKey, u32>,
    scouts: &ScoutPlan,
    ledger: FoodLedger,
    window: u32,
) -> (Dealt, FoodFirstChoice) {
    let deal = |when: BuildsDealt, kit: KitStep| {
        let mut dealt_shelf = shelf.clone();
        let dealt = deal_builds(
            rows,
            pool,
            &mut dealt_shelf,
            tile_room,
            scouts,
            EVERY_HIGH_OPENER,
            when,
            kit,
        );
        (dealt, dealt_shelf)
    };
    let (mut dealt, mut dealt_shelf) = deal(BuildsDealt::InTurn, KitStep::On);
    let mut choice = FoodFirstChoice {
        builds: BuildsDealt::InTurn,
        kit: KitStep::On,
        fed_first: false,
    };
    let builders_dealt = rows
        .iter()
        .zip(&dealt.out)
        .any(|(row, crew)| is_a_build(row) && crew.total() > NOBODY);
    let mut runs_short = band_runs_short(rows, &dealt.out, ledger, window);
    if runs_short && builders_dealt {
        (dealt, dealt_shelf) = deal(BuildsDealt::Last, KitStep::On);
        choice = FoodFirstChoice {
            builds: BuildsDealt::Last,
            kit: KitStep::On,
            fed_first: true,
        };
        runs_short = band_runs_short(rows, &dealt.out, ledger, window);
    }
    if runs_short && dealt.kit_crafters > NOBODY {
        (dealt, dealt_shelf) = deal(BuildsDealt::Last, KitStep::Off);
        choice = FoodFirstChoice {
            builds: BuildsDealt::Last,
            kit: KitStep::Off,
            fed_first: builders_dealt,
        };
    }
    *shelf = dealt_shelf;
    (dealt, choice)
}

/// **WHETHER THE KIT STEP RUNS** (between pass 1b and pass 1c): crafters for the orders that make
/// the equipment the source rows are short of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KitStep {
    On,
    Off,
}

/// **WHEN THE BUILDS ARE DEALT**: in their groups' turn (the ordinary passes), or after every source
/// row and bench order has been dealt (a band feeding itself first).
#[derive(Clone, Copy, PartialEq, Eq)]
enum BuildsDealt {
    InTurn,
    Last,
}

/// A declared build (a bench order joins the passes like a build but is not one).
fn is_a_build(row: &StaffRowInput<'_>) -> bool {
    row.is_build && !row.is_bench
}

/// **THE PASSES, WITH THE BUILDS DEALT `when`.** `Last` deals every other input first over the whole
/// pool, shelf and tile room, then the builds over what is left; the bench's readings are the first
/// deal's (no builder was dealt before pass 2).
#[allow(clippy::too_many_arguments)] // the passes' own inputs plus when to deal the builds
fn deal_builds(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    tile_room: &BTreeMap<TileKey, u32>,
    scouts: &ScoutPlan,
    opener_limit: usize,
    when: BuildsDealt,
    kit_step: KitStep,
) -> Dealt {
    if when == BuildsDealt::InTurn {
        return staff_passes_counted(
            rows,
            pool,
            shelf,
            Passes::All,
            tile_room,
            scouts,
            opener_limit,
            Inputs::Every,
            kit_step,
        );
    }
    let mut dealt = staff_passes_counted(
        rows,
        pool,
        shelf,
        Passes::All,
        tile_room,
        scouts,
        opener_limit,
        Inputs::AllButBuilds,
        kit_step,
    );
    let spent: u32 = dealt.out.iter().map(StaffedCrew::total).sum::<u32>() + dealt.scout.total();
    let mut room_left = tile_room.clone();
    for (row, crew) in rows.iter().zip(&dealt.out) {
        if let Some(tile) = row.tile {
            if let Some(room) = room_left.get_mut(&tile_key(tile)) {
                *room = room.saturating_sub(crew.total());
            }
        }
    }
    // The builds' deal spends what the first deal left of the levy budget.
    let levy_for_builds = ScoutPlan {
        levy: scouts.levy.as_ref().map(|plan| LevyPlan {
            fixed_budget: Some(dealt.levy_left),
            ..plan.clone()
        }),
        stretch: UNSTRETCHED_PLAN,
        ..ScoutPlan::default()
    };
    let built = staff_passes_counted(
        rows,
        pool.saturating_sub(spent),
        shelf,
        Passes::All,
        &room_left,
        &levy_for_builds,
        opener_limit,
        Inputs::BuildsOnly,
        kit_step,
    );
    for (i, row) in rows.iter().enumerate() {
        if is_a_build(row) {
            dealt.out[i] = built.out[i];
        }
    }
    dealt
}

/// The stretch a plan that deals no scouts carries.
const UNSTRETCHED_PLAN: f32 = crate::components::UNSTRETCHED;

/// **WHICH INPUTS A DEAL STAFFS**; the rest are left at nobody.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inputs {
    Every,
    AllButBuilds,
    BuildsOnly,
}

impl Inputs {
    fn deals(self, row: &StaffRowInput<'_>) -> bool {
        match self {
            Inputs::Every => true,
            Inputs::AllButBuilds => !is_a_build(row),
            Inputs::BuildsOnly => is_a_build(row),
        }
    }
}

/// [`staff_band`] over the passes `passes` names.
fn staff_passes(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    passes: Passes,
    tile_room: &BTreeMap<TileKey, u32>,
) -> Vec<RowStaffing> {
    staff_passes_scouting(rows, pool, shelf, passes, tile_room, &ScoutPlan::default()).0
}

/// **NO LIMIT ON HIGH BENCH OPENERS** - the real staffing deals every High order its first crafter;
/// the counterfactual that names who took a hold crew's hand runs with fewer.
const EVERY_HIGH_OPENER: usize = usize::MAX;

/// [`staff_passes`] with the band's scouting range: the scouts are placed **first**, before pass 1a's
/// keepers, out of the whole pool and shelf; every pass then staffs from what the scouts left.
fn staff_passes_scouting(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    passes: Passes,
    tile_room: &BTreeMap<TileKey, u32>,
    scouts: &ScoutPlan,
) -> (Vec<RowStaffing>, ScoutCrew) {
    let dealt = staff_passes_counted(
        rows,
        pool,
        shelf,
        passes,
        tile_room,
        scouts,
        EVERY_HIGH_OPENER,
        Inputs::Every,
        KitStep::On,
    );
    (dealt.out, dealt.scout)
}

/// **WHAT THE PASSES DEALT**, with the readings the bench's publishing needs.
struct Dealt {
    out: Vec<RowStaffing>,
    scout: ScoutCrew,
    /// Where the hands went before pass 2 opened.
    first_call: crate::components::FirstCall,
    /// Every input's crew total as pass 1 left it (before pass 2 opened).
    pass_one: Vec<u32>,
    /// The High bench orders (input indices) that took a first crafter in pass 1, in bench order.
    openers: Vec<usize>,
    /// What is left of the levy budget (0 with no levy).
    levy_left: u32,
    /// The crafters the kit step seated.
    kit_crafters: u32,
}

/// One crafter on bench order `i`: kitted (holding the tool) when the shelf can arm one, else bare
/// where a bare hand works. Whether a head was seated. The ONE opener, used by the High order's
/// pass-1 crafter and by pass 2's opener on every other workable order.
fn deal_bench_opener(
    rows: &[StaffRowInput<'_>],
    ledger: &mut Ledger,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    i: usize,
) -> bool {
    deal_bench_head(rows, ledger, shelf, i)
}

/// One more crafter on bench order `i`: kitted (holding the tool) when the shelf can arm one more,
/// else bare where a bare hand works. The head choice the opener and the kit step share.
fn deal_bench_head(
    rows: &[StaffRowInput<'_>],
    ledger: &mut Ledger,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    i: usize,
) -> bool {
    let row = &rows[i];
    let held = ledger.out[i].kitted;
    if kitted_capacity(row, held, shelf) > held {
        let dealt = ledger.deal_head(i, true, HeadKind::Kitted);
        if dealt {
            arm(row, held, held + 1, shelf);
        }
        dealt
    } else if row.bare_can_work {
        ledger.deal_head(i, true, HeadKind::Bare)
    } else {
        false
    }
}

/// **EVERY HEAD ARMED** - the shelf the kit step sizes a row's full-kit crew against.
const EVERY_HEAD_ARMED: u32 = u32::MAX;

/// **WHAT THE SOURCE ROWS WANT ARMED, AND WHAT THE SHELF CANNOT ARM**, at this point in the deal.
/// A row wants kit up to its REACH crew (pass 2 is the pass that arms heads: kitted heads up to
/// `max(hold, reach)`; pass 3's useful crew is filled bare by design), under its Max and its tile's
/// room. Per item: `wants` = the units those crews would hold, `holds` = the units its kitted heads
/// already hold, `left` = the units still on the shelf.
///
/// Returns `(lacks, reserved)`: the units per item the rows lack (`wants - holds - left`; items nobody
/// lacks are absent), and the **heads to keep out of the kit step** - per item `min(left, wants -
/// holds) x people per unit`, summed - so every shelf unit a row wants armed still has a hand to arm
/// it in pass 2. The kit step only takes hands the shelf cannot arm. Levy rows, builds and bench
/// orders are not source rows.
fn kit_lacks(
    rows: &[StaffRowInput<'_>],
    order: &[usize],
    ledger: &Ledger,
    shelf: &BTreeMap<Arc<str>, u32>,
) -> (BTreeMap<Arc<str>, u32>, u32) {
    // item -> (units wanted, units held, people one unit arms)
    let mut wanted: BTreeMap<Arc<str>, (u32, u32, u32)> = BTreeMap::new();
    for &i in order {
        let row = &rows[i];
        if row.is_build || row.is_bench || row.levy {
            continue;
        }
        let crew = &ledger.out[i];
        let room = row
            .tile
            .and_then(|tile| ledger.tile_room.get(&tile_key(tile)).copied())
            .unwrap_or(u32::MAX);
        let limit = |heads: u32| {
            heads
                .min(cap_of(row))
                .min(crew.total().saturating_add(room))
                .saturating_sub(crew.keepers)
        };
        let reach_crew = |armed: u32| reach_heads(row, armed).max(hold_heads(row, armed));
        let live_heads = limit(reach_crew(EVERY_HEAD_ARMED));
        // An item the band holds no live unit of cannot be priced through the row's coverage (a dead
        // item covers nobody): the row is taken to want its bare reach crew in it, and at least one
        // head where the row has any take at all (a hunt bare hands cannot wound still wants spears).
        let dead_heads = limit(reach_crew(NOBODY).max(u32::from(row.useful_take > NO_TAKE_WANTED)));
        for (item, people) in &row.kit_wants {
            let live = row.kit_items.iter().any(|(held, _)| held == item);
            let entry = wanted.entry(Arc::clone(item)).or_insert((0, 0, *people));
            if live {
                entry.0 += units_for(live_heads, *people);
                entry.1 += units_for(crew.kitted, *people);
            } else {
                entry.0 += units_for(dead_heads, *people);
            }
        }
    }
    let mut reserved = NOBODY;
    let lacks = wanted
        .into_iter()
        .filter_map(|(item, (wants, holds, people))| {
            let left = shelf.get(&item).copied().unwrap_or(NOBODY);
            let unarmed = wants.saturating_sub(holds);
            reserved = reserved.saturating_add(left.min(unarmed).saturating_mul(people.max(1)));
            let lack = unarmed.saturating_sub(left);
            (lack > NOBODY).then_some((item, lack))
        })
        .collect();
    (lacks, reserved)
}

/// **HOW MANY CRAFTERS A KIT ORDER IS WORTH**, `NOBODY` for an order that is no kit order. An order
/// making an item the rows lack is crewed up to `crafters_per_item` x the units lacked; an order making
/// the bench tool that bounds the material such an item is made from, up to its own cap (a tool nobody
/// holds lengthens the wait for the kit).
fn kit_crafters_wanted(row: &StaffRowInput<'_>, lacks: &BTreeMap<Arc<str>, u32>) -> u32 {
    let Some(kit) = &row.bench_kit else {
        return NOBODY;
    };
    let units_short: u32 = kit.makes.iter().filter_map(|item| lacks.get(item)).sum();
    if units_short > NOBODY {
        return kit.crafters_per_item.max(1).saturating_mul(units_short);
    }
    if kit.tool_serves.iter().any(|item| lacks.contains_key(item)) {
        return cap_of(row);
    }
    NOBODY
}

/// [`staff_passes_scouting`] that also reports where the hands went before pass 2 opened.
/// `opener_limit` caps how many High bench orders take their first crafter in pass 1
/// ([`EVERY_HIGH_OPENER`] for the real staffing). `inputs` names the rows dealt; the rest stay at
/// nobody.
#[allow(clippy::too_many_arguments)] // the pass's own inputs plus which of them to deal
fn staff_passes_counted(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    passes: Passes,
    tile_room: &BTreeMap<TileKey, u32>,
    scouts: &ScoutPlan,
    opener_limit: usize,
    inputs: Inputs,
    kit_step: KitStep,
) -> Dealt {
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|&i| inputs.deals(&rows[i]))
        .collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(rows[i].priority), i));

    let mut out: Vec<RowStaffing> = vec![StaffedCrew::default(); rows.len()];
    let pool_left = pool;
    let cap_left: Vec<u32> = rows.iter().map(cap_of).collect();

    // ## What each row would aim at ignoring the pool and the other rows — published after the Max
    // and after the room its tile has left this band (`tile_room`): a tile never holds more than
    // that, so no target names a head the tile cannot seat. `tile_short` is what the room cut off.
    for &i in &order {
        let row = &rows[i];
        let kmax = kitted_capacity(row, NOBODY, shelf);
        let room = row
            .tile
            .and_then(|tile| tile_room.get(&tile_key(tile)).copied())
            .unwrap_or(u32::MAX);
        let max_cap = cap_of(row);
        let hold = hold_heads(row, kmax).min(max_cap);
        let reach = reach_heads(row, kmax).max(hold).min(max_cap);
        let useful = useful_heads(row, kmax).max(reach).min(max_cap);
        out[i].hold_crew = hold.min(room);
        out[i].reach_crew = reach.min(room).max(out[i].hold_crew);
        out[i].useful_crew = useful.min(room).max(out[i].reach_crew);
        out[i].tile_short = hold.max(reach).saturating_sub(out[i].reach_crew);
    }

    let mut ledger = Ledger {
        levy: rows.iter().map(|row| row.levy).collect(),
        levy_budget: NOBODY,
        out,
        pool_left,
        cap_left,
        tiles: rows.iter().map(|row| row.tile.map(tile_key)).collect(),
        tile_room: tile_room.clone(),
        builds_before: builds_before(rows, &order),
    };
    let groups = priority_groups(rows, &order);
    // Each priority group split into its builds and its sources: a group deals its builds first.
    let split: Vec<(Vec<usize>, Vec<usize>)> = groups
        .iter()
        .map(|group| {
            group
                .iter()
                .partition(|&&i| rows[i].is_build || rows[i].is_bench)
        })
        .collect();

    // ## The scouts come off the top - before every keeper, builder and crew. The scouting range is
    // an explicit order of its own (one row per band, not a priority group), so it is dealt first
    // out of the whole pool and shelf and every pass below staffs from what is left.
    // A tribe member's scouts wait for the levy step, after pass 1.
    let mut scout_crew = if scouts.levy.is_some() {
        ScoutCrew::default()
    } else {
        deal_scouts(scouts, &mut ledger.pool_left, None, shelf)
    };
    let mut first_call = crate::components::FirstCall {
        scouting: pool - ledger.pool_left,
        pool,
        ..Default::default()
    };
    let after_scouts = ledger.pool_left;

    // **A levy row is not dealt in pass 1** (#862): the keep-up heads of the band's own rows come
    // first, and the tribe takes its share of what is left in the levy step below.
    let split_first: Vec<(Vec<usize>, Vec<usize>)> = split
        .iter()
        .map(|(builds, sources)| {
            (
                builds.iter().copied().filter(|&i| !rows[i].levy).collect(),
                sources.iter().copied().filter(|&i| !rows[i].levy).collect(),
            )
        })
        .collect();

    // ## Pass 1a — the keepers on every row at every priority, High to Low: the ground the band
    // already has never rots to pay for anything else (a hold crew is a food take, not rot
    // protection). A row's whole-head keepers go first, round-robin within a group; then the head
    // that carries the keeping's fractional remainder.
    for (_, sources) in &split_first {
        deal_keeping(rows, &mut ledger, shelf, sources);
    }
    first_call.keeping = after_scouts - ledger.pool_left;
    let after_keepers = ledger.pool_left;
    // The keeper-typed heads each row holds from here on; every later want is the row's crew less
    // these.
    let mut kept: Vec<u32> = ledger.out.iter().map(|crew| crew.keepers).collect();

    // ## Pass 1b — each priority group in turn, High to Low: its builds' first builder (the player
    // started the build, so the game tries to staff it; kitted if the shelf can arm one, else bare),
    // THEN the group's kitted heads up to the hold crew. A Normal row's hold crew therefore never
    // beats a High build, and a Low build still beats the Low hold crews.
    let mut openers: Vec<usize> = Vec::new();
    for (group, (builds, sources)) in groups.iter().zip(&split_first) {
        if passes == Passes::All {
            for &i in builds.iter().filter(|&&i| !rows[i].is_bench) {
                let row = &rows[i];
                let kitted = kitted_capacity(row, NOBODY, shelf) >= 1;
                if ledger.deal_head(
                    i,
                    true,
                    if kitted {
                        HeadKind::Kitted
                    } else {
                        HeadKind::Bare
                    },
                ) && kitted
                {
                    arm(row, NOBODY, 1, shelf);
                }
            }
            // **A High bench order's FIRST crafter**, after the group's builders and ahead of EVERY
            // hold crew (the High group is dealt first, so no group's hold crew precedes it): the
            // player's way to say "work this one". Queue order, one each.
            if rows[group[0]].priority == SourcePriority::High {
                let mut high: Vec<usize> = builds
                    .iter()
                    .copied()
                    .filter(|&i| rows[i].is_bench)
                    .collect();
                high.sort_unstable();
                for i in high {
                    if openers.len() < opener_limit
                        && deal_bench_opener(rows, &mut ledger, shelf, i)
                    {
                        openers.push(i);
                    }
                }
            }
        }
        deal_kitted(rows, sources, &mut ledger, shelf, |i, row, kmax| {
            hold_heads(row, kmax).saturating_sub(kept[i])
        });
    }

    // ## The kit step - after pass 1b and before pass 1c: a source row short of an equipment item the
    // bench can make gets that order its crafters before any row's bare hold crew is dealt, so the
    // kit is made rather than worked around. A kit order is a workable order making an item some
    // source row is short of, or the bench tool that bounds the material such an item is made from;
    // dealt in bench rank order, kitted crafters where the shelf can arm one, else bare where a bare
    // hand works.
    let mut kit_crafters = NOBODY;
    if passes == Passes::All && kit_step == KitStep::On {
        let (lacks, reserved) = kit_lacks(rows, &order, &ledger, shelf);
        if !lacks.is_empty() {
            for &i in order.iter().filter(|&&i| rows[i].is_bench) {
                let want = kit_crafters_wanted(&rows[i], &lacks);
                // Only hands the shelf cannot arm: the heads for the units already on the shelf
                // stay in the pool for pass 2's kitted reach heads.
                while ledger.out[i].total() < want
                    && ledger.pool_left > reserved
                    && deal_bench_head(rows, &mut ledger, shelf, i)
                {
                    kit_crafters += 1;
                }
            }
        }
    }

    // ## Pass 1c — the chief's FOOD order keeps its rows' hold crews up with bare hands, after every
    // group's kitted hold heads (pass 1b) and before ANY row's pass 2: another order's kitted refill
    // never takes the crew that keeps food coming in. Priority High to Low, round-robin nearest
    // first, only on a web where a bare hand takes something, up to the hold crew under the Max.
    for (_, sources) in &split_first {
        let keepers_of_food: Vec<usize> = sources
            .iter()
            .copied()
            .filter(|&i| rows[i].food_keep_up && rows[i].bare_can_work)
            .collect();
        // The target is re-struck at the units the shelf can still arm (`kitted_capacity`), as
        // `deal_kitted` strikes its own, so a head dealt kitted never leaves a bare-sized target.
        let mut memo: Vec<Option<(u32, u32)>> = vec![None; rows.len()];
        deal_round_robin(&keepers_of_food, |i| {
            let row = &rows[i];
            let held = ledger.out[i].kitted;
            let kmax = kitted_capacity(row, held, shelf);
            let hold = food_target(&mut memo[i], kmax, || {
                hold_heads(row, kmax).saturating_sub(kept[i])
            });
            let wanted = held + ledger.out[i].bare < hold;
            deal_food_head(rows, &mut ledger, shelf, i, wanted)
        });
    }

    // What passes 1b and 1c dealt is the builds' first builders and the rows' hold crews.
    first_call.building = (0..rows.len())
        .filter(|&i| rows[i].is_build && !rows[i].is_bench)
        .map(|i| ledger.out[i].total())
        .sum();
    first_call.crafting = openers.len() as u32 + kit_crafters;
    first_call.holding = (after_keepers - ledger.pool_left)
        .saturating_sub(first_call.building)
        .saturating_sub(first_call.crafting);
    let pass_one: Vec<u32> = ledger.out.iter().map(StaffedCrew::total).collect();

    // ## The levy step (#862) - between pass 1 and pass 2. The tribe's budget is struck over the
    // hands pass 1 left: first the member's scouts, then each priority group's levy rows' keepers,
    // first builder and hold crew, every head counted against the budget. Passes 2 and 3 deal the
    // levy rows like any other, from what is left of it.
    if let Some(plan) = &scouts.levy {
        let hands_left = ledger.pool_left;
        ledger.levy_budget = match plan.fixed_budget {
            Some(budget) => budget.min(hands_left),
            None => levy_budget(plan.share, plan.factor, hands_left),
        };
        let before_scouts = ledger.pool_left;
        let mut budget = ledger.levy_budget;
        scout_crew = deal_scouts(scouts, &mut ledger.pool_left, Some(&mut budget), shelf);
        ledger.levy_budget = budget;
        first_call.scouting = before_scouts - ledger.pool_left;
        for (builds, sources) in &split {
            let levy_sources: Vec<usize> =
                sources.iter().copied().filter(|&i| rows[i].levy).collect();
            let levy_builds: Vec<usize> =
                builds.iter().copied().filter(|&i| rows[i].levy).collect();
            deal_keeping(rows, &mut ledger, shelf, &levy_sources);
            if passes == Passes::All {
                for &i in levy_builds.iter().filter(|&&i| !rows[i].is_bench) {
                    let row = &rows[i];
                    let kitted = kitted_capacity(row, NOBODY, shelf) >= 1;
                    let kind = if kitted {
                        HeadKind::Kitted
                    } else {
                        HeadKind::Bare
                    };
                    if ledger.deal_head(i, true, kind) && kitted {
                        arm(row, NOBODY, 1, shelf);
                    }
                }
            }
            let levy_kept: Vec<u32> = ledger.out.iter().map(|crew| crew.keepers).collect();
            deal_kitted(rows, &levy_sources, &mut ledger, shelf, |i, row, kmax| {
                hold_heads(row, kmax).saturating_sub(levy_kept[i])
            });
        }
        kept = ledger.out.iter().map(|crew| crew.keepers).collect();
    }

    // ## Pass 2 opens with ONE crafter on every workable bench order that has none (a High order's
    // was dealt in pass 1), at any priority (High to Low, then queue order), before any group's
    // pass-2 dealing. A Low order still gets its first crafter, and the tool goes to the higher
    // priority because that order is dealt first.
    if passes == Passes::All {
        for &i in order.iter().filter(|&&i| rows[i].is_bench) {
            if ledger.out[i].total() == NOBODY {
                deal_bench_opener(rows, &mut ledger, shelf, i);
            }
        }
    }

    // ## Pass 2 — kitted heads up to the reach crew, from whatever units and heads remain. A group's
    // builds go first, up to their cap.
    //
    // **The chief's FOOD order's refill is dealt first in its own group** (still after every higher
    // group's pass 2, so a High stone refill goes before a Normal food refill): bare heads up to the
    // row's reach crew, round-robin nearest first, only where a bare hand takes something. Player
    // rows and the other orders keep kitted-first.
    for (builds, sources) in &split {
        if passes == Passes::All {
            deal_kitted(rows, builds, &mut ledger, shelf, |_, row, _| cap_of(row));
        }
        let food_refill: Vec<usize> = sources
            .iter()
            .copied()
            .filter(|&i| rows[i].food_keep_up && rows[i].bare_can_work)
            .collect();
        let mut memo: Vec<Option<(u32, u32)>> = vec![None; rows.len()];
        deal_round_robin(&food_refill, |i| {
            let row = &rows[i];
            let held = ledger.out[i].kitted;
            let kmax = kitted_capacity(row, held, shelf);
            let reach = food_target(&mut memo[i], kmax, || {
                reach_heads(row, kmax)
                    .max(hold_heads(row, kmax))
                    .saturating_sub(kept[i])
            });
            let wanted = held + ledger.out[i].bare < reach;
            deal_food_head(rows, &mut ledger, shelf, i, wanted)
        });
        deal_kitted(rows, sources, &mut ledger, shelf, |i, row, kmax| {
            reach_heads(row, kmax)
                .max(hold_heads(row, kmax))
                .saturating_sub(kept[i])
        });
    }

    if passes == Passes::Steady {
        return Dealt {
            out: ledger.out,
            scout: scout_crew,
            first_call,
            pass_one,
            openers,
            levy_left: ledger.levy_budget,
            kit_crafters,
        };
    }

    // ## Pass 3 — every row up to its USEFUL crew: the kit's units still unspent first (a kit never
    // sits on the shelf while someone works bare), then bare hands where one adds value. A group's
    // builds first: a bare-handed builder always contributes.
    for (builds, sources) in &split {
        deal_kitted(rows, builds, &mut ledger, shelf, |_, row, _| cap_of(row));
        deal_round_robin(builds, |i| {
            ledger.deal_head(i, rows[i].bare_can_work, HeadKind::Bare)
        });
        deal_kitted(rows, sources, &mut ledger, shelf, |i, row, kmax| {
            useful_heads(row, kmax).saturating_sub(kept[i])
        });
        let mut needed: Vec<Option<u32>> = vec![None; rows.len()];
        deal_round_robin(sources, |i| {
            let row = &rows[i];
            if !row.bare_can_work {
                return false;
            }
            let kitted = ledger.out[i].kitted;
            let useful =
                *needed[i].get_or_insert_with(|| useful_heads(row, kitted).saturating_sub(kept[i]));
            ledger.deal_head(i, kitted + ledger.out[i].bare < useful, HeadKind::Bare)
        });
    }
    // The published reach and useful crews state the crew actually dealt: bare hands past the
    // basket supply are less effective than the kit the targets were struck at, so the row can
    // field more heads than the kitted aim named. The deal itself is unchanged.
    for crew in &mut ledger.out {
        let dealt = crew.total();
        crew.reach_crew = crew.reach_crew.max(dealt);
        crew.useful_crew = crew.useful_crew.max(crew.reach_crew);
    }
    Dealt {
        levy_left: ledger.levy_budget,
        out: ledger.out,
        scout: scout_crew,
        first_call,
        pass_one,
        openers,
        kit_crafters,
    }
}

/// Which segment of a row's crew a head joins.
#[derive(Clone, Copy)]
enum HeadKind {
    Keeper,
    Kitted,
    Bare,
}

/// The passes' running state: each row's crew so far, the pool and each row's room under its Max.
struct Ledger {
    /// Which inputs are levy rows (dealt from [`Self::levy_budget`]).
    levy: Vec<bool>,
    /// The levy heads still to deal (every head on a levy row counts against it).
    levy_budget: u32,
    out: Vec<RowStaffing>,
    pool_left: u32,
    cap_left: Vec<u32>,
    /// The tile each input's hands stand on, and the room each tile has left for this band.
    tiles: Vec<Option<TileKey>>,
    tile_room: BTreeMap<TileKey, u32>,
    /// **THE FILL GATE**: for each of the band's own builds, the builds ahead of it in
    /// [`builds_before`]'s order (empty for every other input). A build is dealt a head only when
    /// every one of them [`Self::build_is_full`].
    builds_before: Vec<Vec<usize>>,
}

impl Ledger {
    /// **WHETHER BUILD `j` CAN TAKE NO MORE BUILDERS**: at its cap (the Max the passes deal it to,
    /// already cut by the room its tile has left) or unable to take any at all (a cap of nothing: a
    /// refusing gate, a materials stall, a finished climb).
    fn build_is_full(&self, j: usize) -> bool {
        self.cap_left[j] == NOBODY
            || self.tiles[j]
                .and_then(|tile| self.tile_room.get(&tile))
                .is_some_and(|room| *room == NOBODY)
    }

    /// Seat one head on row `i` when `wanted` and the pool and the Max allow it.
    fn deal_head(&mut self, i: usize, wanted: bool, kind: HeadKind) -> bool {
        if !wanted || self.pool_left == NOBODY || self.cap_left[i] == NOBODY {
            return false;
        }
        if !self.builds_before[i].iter().all(|&j| self.build_is_full(j)) {
            return false;
        }
        if self.levy[i] && self.levy_budget == NOBODY {
            return false;
        }
        // The tile holds only so many hands, across every row and build on it (and, through the
        // room it started with, every other band). A tile with no entry is unbounded.
        if let Some(room) = self.tiles[i].and_then(|tile| self.tile_room.get_mut(&tile)) {
            if *room == NOBODY {
                return false;
            }
            *room -= 1;
        }
        match kind {
            HeadKind::Keeper => self.out[i].keepers += 1,
            HeadKind::Kitted => self.out[i].kitted += 1,
            HeadKind::Bare => self.out[i].bare += 1,
        }
        self.pool_left -= 1;
        self.cap_left[i] -= 1;
        if self.levy[i] {
            self.levy_budget -= 1;
        }
        true
    }

    /// One keeper, while the row still owes its site one.
    fn deal_keeper(&mut self, row: &StaffRowInput<'_>, i: usize) -> bool {
        let owed = self.out[i].keepers < whole_keepers(row);
        self.deal_head(i, owed, HeadKind::Keeper)
    }
}

/// **THE KEEPERS OF A GROUP'S ROWS**: every whole keeper round-robin, then the head that carries
/// each row's fractional remainder (a kitted gatherer if the shelf can arm one, else a bare one where
/// a bare hand works, else a keeper).
fn deal_keeping(
    rows: &[StaffRowInput<'_>],
    ledger: &mut Ledger,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    sources: &[usize],
) {
    deal_round_robin(sources, |i| ledger.deal_keeper(&rows[i], i));
    for &i in sources {
        let row = &rows[i];
        if !keeps_a_fraction(row) {
            continue;
        }
        let held = ledger.out[i].kitted;
        let kind = if !remainder_rides_with_a_gatherer(row) {
            HeadKind::Keeper
        } else if kitted_capacity(row, held, shelf) > held {
            HeadKind::Kitted
        } else if row.bare_can_work {
            HeadKind::Bare
        } else {
            HeadKind::Keeper
        };
        if ledger.deal_head(i, true, kind) && matches!(kind, HeadKind::Kitted) {
            arm(row, held, held + 1, shelf);
        }
    }
}

/// **ONE HEAD ON A CHIEF'S FOOD ROW** (pass 1c's hold crew and pass 2's refill): kitted, arming the
/// row's live kit items from the shelf exactly as [`deal_kitted`] arms, when the shelf can arm one
/// more; else bare. The caller has already checked a bare hand takes something on the row. `wanted`
/// is whether the row's crew target is still short.
fn deal_food_head(
    rows: &[StaffRowInput<'_>],
    ledger: &mut Ledger,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    i: usize,
    wanted: bool,
) -> bool {
    let row = &rows[i];
    let held = ledger.out[i].kitted;
    if wanted && kitted_capacity(row, held, shelf) > held {
        if ledger.deal_head(i, true, HeadKind::Kitted) {
            arm(row, held, held + 1, shelf);
            return true;
        }
        return false;
    }
    ledger.deal_head(i, wanted, HeadKind::Bare)
}

/// A chief's food row's crew target at the kitted capacity `kmax`, remembered per `kmax` so the
/// target is struck once per shelf reading (the memo `deal_kitted` keeps).
fn food_target(memo: &mut Option<(u32, u32)>, kmax: u32, strike: impl FnOnce() -> u32) -> u32 {
    match *memo {
        Some((at, target)) if at == kmax => target,
        _ => {
            let target = strike();
            *memo = Some((kmax, target));
            target
        }
    }
}

/// **A ROUND OF ONE HEAD PER ROW**, repeated until a whole round deals nothing. `deal_one` seats at
/// most one head on a row and says whether it did.
fn deal_round_robin(group: &[usize], mut deal_one: impl FnMut(usize) -> bool) {
    loop {
        let mut dealt = false;
        for &i in group {
            dealt |= deal_one(i);
        }
        if !dealt {
            break;
        }
    }
}

/// **KITTED HEADS, ONE AT A TIME ACROSS A GROUP.** A row takes another while it holds fewer than
/// `want_of` (its target at the units the shelf could still arm it with) and one more head is armed
/// by the shelf as it stands. The want is re-read as the shelf drains, remembered per shelf reading.
fn deal_kitted(
    rows: &[StaffRowInput<'_>],
    group: &[usize],
    ledger: &mut Ledger,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    want_of: impl Fn(usize, &StaffRowInput<'_>, u32) -> u32,
) {
    let mut memo: Vec<Option<(u32, u32)>> = vec![None; rows.len()];
    deal_round_robin(group, |i| {
        let row = &rows[i];
        let held = ledger.out[i].kitted;
        let gatherers = held + ledger.out[i].bare;
        let kmax = kitted_capacity(row, held, shelf);
        let want = match memo[i] {
            Some((at, want)) if at == kmax => want,
            _ => {
                let want = want_of(i, row, kmax);
                memo[i] = Some((kmax, want));
                want
            }
        };
        if !ledger.deal_head(i, held < kmax && gatherers < want, HeadKind::Kitted) {
            return false;
        }
        arm(row, held, held + 1, shelf);
        true
    });
}

/// **THE ORDER THE BAND'S OWN BUILDS ARE FILLED IN**, one build at a time: priority group High to
/// Low, then the least work still owed on the climb (closest to done first), then nearest to camp,
/// then row index. Returns, per input, the builds ahead of it in that order (empty for an input that
/// is not one of the band's own builds: a source, a bench order, a levy build - the tribe's levy
/// step deals those).
fn builds_before(rows: &[StaffRowInput<'_>], order: &[usize]) -> Vec<Vec<usize>> {
    let mut own: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| is_a_build(&rows[i]) && !rows[i].levy)
        .collect();
    own.sort_by(|&a, &b| {
        let (x, y) = (&rows[a], &rows[b]);
        y.priority
            .cmp(&x.priority)
            .then(x.build_work_left.total_cmp(&y.build_work_left))
            .then(x.distance.cmp(&y.distance))
            .then(a.cmp(&b))
    });
    let mut before = vec![Vec::new(); rows.len()];
    for (place, &i) in own.iter().enumerate() {
        before[i] = own[..place].to_vec();
    }
    before
}

/// The rows of `order` (priority High to Low, ties by row index) split by priority, each group
/// ordered **nearest to camp first**, ties by row index — the order a round deals in.
fn priority_groups(rows: &[StaffRowInput<'_>], order: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &i in order {
        match groups.last_mut() {
            Some(group) if rows[group[0]].priority == rows[i].priority => group.push(i),
            _ => groups.push(vec![i]),
        }
    }
    for group in &mut groups {
        group.sort_by_key(|&i| (rows[i].distance, i));
    }
    groups
}

// ---------------------------------------------------------------------------------------------
// The world half
// ---------------------------------------------------------------------------------------------

/// **WHAT STAFFING READS BESIDE THE CLAIM SOURCES** — the tile-improvement and road registries (a
/// tile row's and the Roadwork pool's keepers), and the band they are staffed for.
pub struct StaffingWorld<'a> {
    pub sources: &'a ClaimSources<'a>,
    pub tile_improvements: &'a crate::tile_improvement::TileImprovementRegistry,
    pub roads: &'a crate::routes::RoadRegistry,
    pub band: Option<BandId>,
    /// **The tile the band stands on** and the map it stands on, which a row's distance is measured
    /// from. `None` reads every row at distance 0.
    pub camp: Option<CampSite>,
    /// **The hands every OTHER band has on each tile** — what a tile's `tile_worker_cap` is shared
    /// with. In the turn it is the room earlier bands left (bands are staffed in a deterministic
    /// order); from a command it is every other band's current placements.
    pub tile_hands_elsewhere: &'a BTreeMap<TileKey, u32>,
    /// **Whether a declared build can make progress this turn** — not complete, not gated, not stalled
    /// on materials. A build that cannot gets no builders.
    pub build_can_progress: &'a dyn Fn(&crate::components::BuildDeclaration) -> bool,
    /// **The work a declared build still owes** on its whole climb - the figure its turn estimate is
    /// struck from. Orders the band's builds, closest to done first.
    pub build_work_remaining: &'a dyn Fn(&crate::components::BuildDeclaration) -> f32,
    /// **WHAT THE BAND'S STORE HOLDS OF A MATERIAL** - the "on hand" a keep-on-hand target is
    /// judged against (`LocalStore::material_total`: the store only, not loads at the site).
    pub on_hand: &'a dyn Fn(&str) -> f32,
    /// **The band's larder and need**, for the food-first check. [`FoodLedger::OFF`] reads nothing.
    pub food: FoodLedger,
    /// **THE TRIBE'S LEVY ON THIS BAND** (#862): `Some` for a band in a tribe (see [`LevyPlan`]).
    pub levy: Option<LevyPlan>,
    /// **THE BENCH'S ORDERS** as the passes need them ([`crate::systems::bench_plan`]): which are
    /// workable, their crew caps and their tool.
    pub bench: &'a crate::systems::BenchPlan,
}

/// **A TILE AS A MAP KEY** — `(y, x)`, row-major like every registry keyed by tile (`UVec2` is not
/// `Ord`), so iteration is an order and not an accident.
pub type TileKey = (u32, u32);

/// The map key of a tile position.
pub fn tile_key(tile: UVec2) -> TileKey {
    (tile.y, tile.x)
}

/// **THE TILE A DECLARED BUILD'S BUILDERS STAND ON** — its site, `None` for a band improvement (it
/// sits on the band, not on a tile of the map).
pub fn build_site_tile(source: &BuildSource, herds: &crate::fauna::HerdRegistry) -> Option<UVec2> {
    match source {
        BuildSource::Patch(tile) | BuildSource::Road(tile) => Some(*tile),
        BuildSource::Deposit { tile, .. } | BuildSource::Tile { tile, .. } => Some(*tile),
        BuildSource::Herd(id) => herds.find(id).map(|herd| herd.position()),
        BuildSource::Band { .. } => None,
    }
}

/// **THE HANDS ONE BAND HAS ON EACH TILE** — its source rows' crews and its builds' builders,
/// summed per tile. What every other band's staffing subtracts from a tile's room.
pub fn tile_hands_of(
    allocation: &LaborAllocation,
    herds: &crate::fauna::HerdRegistry,
) -> BTreeMap<TileKey, u32> {
    let mut hands: BTreeMap<TileKey, u32> = BTreeMap::new();
    for row in &allocation.assignments {
        if let Some(tile) = crate::systems::party_source_position(&row.target, herds) {
            *hands.entry(tile_key(tile)).or_insert(NOBODY) += row.workers;
        }
    }
    for (source, crew) in &allocation.build_crews {
        if let Some(tile) = build_site_tile(source, herds) {
            *hands.entry(tile_key(tile)).or_insert(NOBODY) += crew.total();
        }
    }
    hands
}

/// Where the band's camp is, and the grid the hex distance wraps over.
#[derive(Debug, Clone, Copy)]
pub struct CampSite {
    pub position: UVec2,
    pub grid_width: u32,
    pub wrap_horizontal: bool,
}

impl CampSite {
    /// Hex steps from camp to `site`, the metric the work-party walk is measured in.
    pub(crate) fn distance_to(&self, site: UVec2) -> u32 {
        crate::grid_utils::hex_distance_wrapped(
            self.position,
            site,
            self.grid_width,
            self.wrap_horizontal,
        )
    }
}

/// **THE HANDS THE SIM MAY STAFF** — the working people less the hands still walking home
/// and every crew the player set by hand (Warrior, Builders and a need row's muster). The Scout
/// row is the sim's: it is dealt from this pool FIRST (`deal_scouts`, before pass 1a), off the top, ahead of the hold crews.
pub fn staffable_pool(allocation: &LaborAllocation, available: u32) -> u32 {
    let player_set: u32 = allocation
        .assignments
        .iter()
        .filter(|row| !row.target.is_sim_staffed())
        .map(|row| row.workers)
        .sum();
    available
        .saturating_sub(allocation.walking_home())
        .saturating_sub(player_set)
}

/// **IS THIS ROW A CAMP-KILL HUNT?** - a hunt row whose herd, at its current tile, stands within
/// `band_work_range` of the camp (`work_party::party_begins_past`, the threshold that decides
/// whether a party posts). Such a row hauls nothing, so it is held to the kit items that bind a kill
/// ([`crate::components::LaborAssignment::binding_kit`]). `false` with no camp to measure from.
fn is_camp_kill(world: &StaffingWorld<'_>, target: &LaborTarget) -> bool {
    world.camp.as_ref().is_some_and(|camp| {
        crate::hunt_by_need::camp_kill_hunt(
            target,
            world.sources.herds,
            Some(camp.position),
            world.sources.labor,
            (camp.grid_width, camp.wrap_horizontal),
        )
    })
}

/// The kit's items still serving the band, each with the people one unit arms.
fn live_kit_items(
    kit: &KitChoice,
    band_kit: &BandEquipment,
    world: &StaffingWorld<'_>,
) -> Vec<(Arc<str>, u32)> {
    let equipment = world.sources.equipment;
    kit.uses()
        .filter(|item| kit.item_live(item, band_kit, equipment))
        .filter_map(|item| {
            let def = equipment.item(item)?;
            Some((Arc::<str>::from(item), def.workers_per_unit))
        })
        .collect()
}

/// **EVERY ITEM OF A KIT**, live or not, each with the people one unit arms.
fn all_kit_items(
    kit: &KitChoice,
    equipment: &crate::equipment_config::EquipmentConfig,
) -> Vec<(Arc<str>, u32)> {
    kit.uses()
        .filter_map(|item| {
            let def = equipment.item(item)?;
            Some((Arc::<str>::from(item), def.workers_per_unit))
        })
        .collect()
}

/// A coverage closure: the crews when the first `k` heads are armed and the rest bare.
fn coverage_with<'a>(
    equipment: &'a crate::equipment_config::EquipmentConfig,
    kit: KitChoice,
    band_kit: &'a BandEquipment,
    items: Vec<(Arc<str>, u32)>,
    crew: f32,
) -> impl Fn(u32) -> KitCoverage + 'a {
    move |armed: u32| {
        equipment.coverage_from_units(&kit, crew, band_kit, |item| {
            items
                .iter()
                .find(|(id, _)| id.as_ref() == item)
                .map_or(0.0, |(_, people)| units_for(armed, *people) as f32)
        })
    }
}

/// The keepers a tile improvement owes.
fn tile_keepers(
    world: &StaffingWorld<'_>,
    band_kit: &BandEquipment,
    target: &LaborTarget,
    crew: u32,
) -> f32 {
    let LaborTarget::Tile { tile, branch } = target else {
        return 0.0;
    };
    world
        .tile_improvements
        .get(*tile, *branch)
        .map_or(0.0, |improvement| {
            crate::systems::prospective_keep_hands(
                world.sources.equipment,
                band_kit,
                improvement.held_rung(),
                improvement.keeping_basis(world.sources.ladder),
                crew,
            )
        })
}

/// The keepers a band improvement owes: the tile kind's reading, off the band's own record.
fn band_keepers(
    world: &StaffingWorld<'_>,
    band_kit: &BandEquipment,
    target: &LaborTarget,
    crew: u32,
) -> f32 {
    let LaborTarget::Band { band, branch } = target else {
        return 0.0;
    };
    world
        .tile_improvements
        .bands()
        .get(*band, *branch)
        .map_or(0.0, |improvement| {
            crate::systems::prospective_keep_hands(
                world.sources.equipment,
                band_kit,
                improvement.held_rung(),
                improvement.keeping_basis(world.sources.ladder),
                crew,
            )
        })
}

/// The keepers the Roadwork pool owes: its kept roads' bills at the as-if-equipped rate, summed.
fn roadwork_keepers(world: &StaffingWorld<'_>, band_kit: &BandEquipment, crew: u32) -> f32 {
    let Some(band) = world.band else {
        return 0.0;
    };
    world
        .roads
        .kept_by(band)
        .map(|(tile, road)| {
            let remoteness = road.keeper_remoteness;
            let measure = (world.sources.ground_of)(tile).map_or(0.0, |ground| {
                crate::routes::road_upkeep_measure(ground.terrain, remoteness)
            });
            crate::systems::prospective_keep_hands(
                world.sources.equipment,
                band_kit,
                road.held_rung(),
                crate::routes::road_keeping_basis(road, measure, world.sources.ladder),
                crew,
            )
        })
        .sum()
}

/// The staffed rows of an allocation as the passes see them, with the kit each resolved to.
struct StaffInputs<'a> {
    /// The staffed source rows, then the declared builds (`rows[targets.len()..]`).
    rows: Vec<StaffRowInput<'a>>,
    targets: Vec<LaborTarget>,
    /// Each staffed row's resolved kit, in `rows` order.
    kits: Vec<KitChoice>,
    /// Whether each staffed row is a camp-kill hunt ([`is_camp_kill`]), in `rows` order.
    camp_kills: Vec<bool>,
    /// The declared builds, in the order they follow the source rows.
    builds: Vec<BuildMeta>,
    /// The room each referenced tile has for this band: the cap less the other bands' hands.
    tile_room: BTreeMap<TileKey, u32>,
    /// The band's scouting range (`range` 0 when it has no Scout row).
    scouts: ScoutPlan,
    /// The queue index of each bench order input, in the order they follow the builds in `rows`.
    bench_orders: Vec<usize>,
}

/// What a build input carries beyond the passes' view of it.
struct BuildMeta {
    source: BuildSource,
    /// The effective cap published beside the crew.
    cap: u32,
}

/// **WHAT STAFFING DECIDED FOR A BAND** — the crew of every staffed source row and of every
/// declared build.
#[derive(Debug, Clone, Default)]
pub struct StaffedBand {
    pub rows: Vec<(LaborTarget, StaffedCrew)>,
    pub builds: Vec<(BuildSource, BuildCrew)>,
    /// The Scout row's crew and the tiles past the local range it reaches (default with no Scout
    /// row). Kept apart from `rows` so the settle trim, which takes hands off source rows and
    /// builds, can never leave `reach` describing scouts it removed.
    pub scout: ScoutCrew,
    /// **Every bench order's staffing**, one per queue index: workable orders carry what the passes
    /// dealt, the rest nothing and a cap of zero.
    pub bench: Vec<BenchStaffed>,
    /// Where the hands went before pass 2 opened (what a crewless bench order waited behind).
    pub first_call: crate::components::FirstCall,
    /// Which High bench order took a take row's hold-crew hands (empty when none did).
    pub hold_losses: Vec<crate::components::HoldLoss>,
}

/// **WHAT THE PASSES DECIDED FOR ONE BENCH ORDER.**
#[derive(Debug, Clone, Copy)]
pub struct BenchStaffed {
    /// The order's queue index.
    pub order: usize,
    /// Who the passes seated on it: the crafters holding the tool and the ones working by hand.
    pub crew: crate::components::BenchCrew,
    /// The cap the passes used.
    pub cap: u32,
    /// The order's rank.
    pub priority: SourcePriority,
}

impl StaffedBand {
    /// **Every crafter the staffing seated on the bench.**
    pub fn bench_total(&self) -> u32 {
        self.bench.iter().map(|order| order.crew.total()).sum()
    }

    /// **WRITE THE BENCH'S CREWS ONTO THE BENCH** - each order's crew and cap, restamped every
    /// staffing. The turn, the command restaff and the preview's copy all go through it.
    pub fn apply_bench(&self, bench: &mut crate::components::BandBench) {
        bench.first_call = self.first_call;
        bench.hold_losses = self.hold_losses.clone();
        for staffed in &self.bench {
            if let Some(order) = bench.orders.get_mut(staffed.order) {
                order.crew = staffed.crew;
                order.crew_cap = staffed.cap;
            }
        }
    }

    /// **WRITE THE WHOLE STAFFING ONTO AN ALLOCATION** - the staffed rows, the Scout row's crew and
    /// reach, and the builds' crews. The one place the turn, the command restaff and the preview's
    /// copy all apply a staffing, so they cannot differ.
    pub fn apply_to(self, allocation: &mut LaborAllocation, lost_people: u32) {
        allocation.apply_staffing(self.rows, lost_people);
        allocation.apply_scouting(self.scout);
        allocation.apply_build_crews(self.builds);
    }
}

/// **STAFF EVERY STAFFED ROW AND EVERY DECLARED BUILD OF `allocation`** — the world half of the
/// passes. `pool` is [`staffable_pool`]; `available` the band's whole working crew, the bound keepers
/// are sized against. Returns each staffed row's crew keyed by its target, for
/// [`LaborAllocation::apply_staffing`], and each build's crew keyed by its source, for
/// [`LaborAllocation::apply_build_crews`].
pub fn staff_rows<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    pool: u32,
    available: u32,
) -> StaffedBand {
    let equipment = world.sources.equipment;
    let inputs = build_staff_inputs(world, allocation, band_kit, available);
    let mut shelf: BTreeMap<Arc<str>, u32> = BTreeMap::new();
    for (item, _) in inputs
        .rows
        .iter()
        .flat_map(|row| row.kit_items.iter())
        .chain(inputs.scouts.kit_items.iter())
    {
        shelf
            .entry(Arc::clone(item))
            .or_insert_with(|| band_kit.live_units(item, equipment));
    }
    let opening_shelf = shelf.clone();
    let (
        Dealt {
            out: staffed,
            scout,
            first_call,
            pass_one,
            openers,
            levy_left: _,
            kit_crafters: _,
        },
        choice,
    ) = deal_band_food_first(
        &inputs.rows,
        pool,
        &mut shelf,
        &inputs.tile_room,
        &inputs.scouts,
        world.food,
        world.sources.labor.food_first_window_turns,
    );
    let fed_first = choice.fed_first;
    let source_rows = inputs.targets.len();
    let build_end = source_rows + inputs.builds.len();
    // **WHICH HIGH ORDER TOOK A HOLD CREW'S HANDS**: only when an opener took a pass-1 hand, rerun
    // pass 1 with one opener fewer at a time; a row's loss to opener `j` is what its pass-1 crew
    // gained when that opener was withheld.
    let mut hold_losses: Vec<crate::components::HoldLoss> = Vec::new();
    let mut with_openers = pass_one;
    for (taken, &opener) in openers.iter().enumerate().rev() {
        let without = deal_builds(
            &inputs.rows,
            pool,
            &mut opening_shelf.clone(),
            &inputs.tile_room,
            &inputs.scouts,
            taken,
            choice.builds,
            choice.kit,
        )
        .pass_one;
        for (row, target) in inputs.targets.iter().enumerate() {
            let lost = without[row].saturating_sub(with_openers[row]);
            if lost > NOBODY {
                hold_losses.push(crate::components::HoldLoss {
                    target: target.clone(),
                    order: inputs.bench_orders[opener - build_end],
                    hands: lost,
                });
            }
        }
        with_openers = without;
    }
    // Bench order, then row order: the counterfactual walked the openers last to first.
    hold_losses.sort_by_key(|loss| loss.order);
    let mut staffed = staffed;
    for ((crew, camp_kill), (row, target)) in staffed
        .iter_mut()
        .zip(&inputs.camp_kills)
        .zip(inputs.rows.iter().zip(&inputs.targets))
    {
        crew.camp_kill = *camp_kill;
        // Only a take row has a bare rate to be idle: a tile, band or roadwork row is not asked a
        // take kit at all.
        crew.bare_idle = !row.bare_can_work
            && matches!(
                target,
                LaborTarget::Forage { .. } | LaborTarget::Hunt { .. } | LaborTarget::Extract { .. }
            );
    }
    // The hands every input holds on each tile once the passes are done: what is left of a tile's
    // room is what a build there could still take beyond its crew.
    let mut tile_held: BTreeMap<TileKey, u32> = BTreeMap::new();
    for (row, crew) in inputs.rows.iter().zip(&staffed) {
        if let Some(tile) = row.tile {
            *tile_held.entry(tile_key(tile)).or_default() += crew.total();
        }
    }
    let builds = inputs
        .builds
        .iter()
        .zip(&staffed[source_rows..])
        .zip(&inputs.rows[source_rows..])
        .map(|((meta, crew), row)| {
            // **The cap is the rung's, cut by the room the tile has left** (the target the passes deal
            // a build to): a build on a full tile reads its crew as its cap.
            let room_left = row
                .tile
                .map(tile_key)
                .and_then(|key| {
                    inputs.tile_room.get(&key).map(|room| {
                        room.saturating_sub(tile_held.get(&key).copied().unwrap_or(NOBODY))
                    })
                })
                .unwrap_or(u32::MAX);
            (
                meta.source.clone(),
                BuildCrew {
                    kitted: crew.kitted,
                    bare: crew.bare,
                    cap: meta.cap.min(crew.total().saturating_add(room_left)),
                    // Only a build that would have held builders in the builds-first deal (a cap above
                    // nobody and a climb that can progress) is waiting on the food.
                    fed_first: fed_first
                        && crew.total() == NOBODY
                        && row.max.is_some_and(|max| max > NOBODY),
                },
            )
        })
        .collect();
    let mut bench: Vec<BenchStaffed> = (0..world.bench.orders.len())
        .map(|order| BenchStaffed {
            order,
            crew: crate::components::BenchCrew::default(),
            cap: 0,
            priority: world.bench.orders[order].priority,
        })
        .collect();
    for (offset, &order) in inputs.bench_orders.iter().enumerate() {
        let crew = &staffed[build_end + offset];
        bench[order].crew = crate::components::BenchCrew {
            at_tool: crew.kitted,
            by_hand: crew.bare,
        };
        bench[order].cap = world.bench.orders[order].crew_cap;
    }
    StaffedBand {
        rows: inputs
            .targets
            .into_iter()
            .zip(staffed)
            .take(source_rows)
            .collect(),
        builds,
        scout,
        bench,
        first_call,
        hold_losses,
    }
}

/// **STAFF THE ROWS, THEN MAKE SURE NO HAND IS COMMITTED TWICE** - [`staff_rows`] sizes the pool as if
/// every hand a staffed row holds were free, but the hands a far party loses (and the rejoining
/// walkers a smaller row can no longer take back) leave the row and WALK HOME, and a walking hand is
/// not free for the near row that was just given it. Applying the crews to a copy shows what is left
/// committed once the walks exist; while that is more than the band has, one hand comes off the
/// lowest-priority row that can lose it without shrinking a standing party (which would only cut
/// more hands loose). The near row waits for the hands to arrive, and nothing is committed twice.
///
/// `lost_people` is the head count the band no longer has (discarded off the cut walks, never walked
/// home), exactly as [`LaborAllocation::apply_staffing`] takes it.
pub fn staff_rows_settled<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    pool: u32,
    available: u32,
    lost_people: u32,
) -> StaffedBand {
    let mut staffed = staff_rows(world, allocation, band_kit, pool, available);
    let levied_roads: &[UVec2] = world
        .levy
        .as_ref()
        .map_or(&[], |plan| plan.road_tiles.as_slice());
    loop {
        let mut after = allocation.clone();
        staffed.clone().apply_to(&mut after, lost_people);
        let committed = after.assigned_total() + staffed.bench_total() + after.walking_home();
        if committed <= available {
            return staffed;
        }
        if !trim_one_hand(allocation, levied_roads, &mut staffed) {
            return staffed;
        }
    }
}

/// Take ONE hand off the lowest-priority row or build that can lose it (see the ties below), or
/// answer `false` when nothing can give one.
fn trim_one_hand(
    allocation: &LaborAllocation,
    levied_roads: &[UVec2],
    staffed: &mut StaffedBand,
) -> bool {
    let standing = |target: &LaborTarget| -> u32 {
        allocation
            .assignments
            .iter()
            .find(|row| row.target.same_source(target))
            .map_or(NOBODY, |row| {
                row.party
                    .as_ref()
                    .map_or(NOBODY, |party| party.workers)
                    .max(allocation.rejoining_to(target))
            })
    };
    let priority_of = |target: &LaborTarget| {
        allocation
            .assignments
            .iter()
            .find(|row| row.target.same_source(target))
            .map_or(SourcePriority::default(), |row| row.priority)
    };
    // The lowest-priority hand that can go: a row's hand above the party it must keep, or a
    // builder (builders stand on no party). Builds rank by their site's Priority.
    let build_priority_of = |source: &BuildSource| build_site_priority(allocation, source);
    // Ties at one priority: the rows' surplus goes before a build's builders. The passes deal
    // a group's builds their first head before its hold crews, so the trim runs in reverse and
    // the build's builders are the last hands of the group to go. Within one kind the later
    // index goes first (the dealing's own reverse). Indices are never compared across kinds.
    // A levy head is trimmed before the band's own at equal priority.
    let levy_row = |target: &LaborTarget| {
        allocation
            .assignments
            .iter()
            .any(|row| row.for_tribe.is_some() && row.target.same_source(target))
    };
    let levy_build = |source: &BuildSource| {
        matches!(source, BuildSource::Road(tile) if levied_roads.contains(tile))
            || allocation
                .assignments
                .iter()
                .any(|row| row.for_tribe.is_some() && source.names(&row.target))
    };
    let row_pick = staffed
        .rows
        .iter()
        .enumerate()
        .filter(|(_, (target, crew))| crew.total() > standing(target))
        .map(|(index, (target, crew))| {
            // A chief's food row down to its hold crew is the last of the rows to lose a hand: pass
            // 1c dealt those heads ahead of every other row's reach.
            let protected = allocation.keeps_food_up(target) && crew.total() <= crew.hold_crew;
            (
                protected,
                priority_of(target),
                std::cmp::Reverse(levy_row(target)),
                std::cmp::Reverse(index),
                index,
            )
        })
        .min();
    let build_pick = staffed
        .builds
        .iter()
        .enumerate()
        .filter(|(_, (_, crew))| crew.total() > NOBODY)
        .map(|(index, (source, crew))| {
            // Builds are filled one at a time, so the build filled last holds the fewest builders:
            // at equal priority the thinnest crew gives its hand first, then the later index.
            (
                build_priority_of(source),
                std::cmp::Reverse(levy_build(source)),
                crew.total(),
                std::cmp::Reverse(index),
                index,
            )
        })
        .min();
    // **A bench order's heads trim exactly as a build's of the same priority** (later index first).
    let bench_pick = staffed
        .bench
        .iter()
        .enumerate()
        .filter(|(_, order)| order.crew.total() > NOBODY)
        .map(|(index, order)| (order.priority, std::cmp::Reverse(index), index))
        .min();
    // The build-like candidate: the lower priority of a build and a bench order; a tie takes the bench.
    // Its last field says whether it is a levy build (a bench order never is).
    let buildish = match (build_pick, bench_pick) {
        (None, None) => None,
        (Some((priority, levied, _, _, at)), None) => Some((priority, false, at, levied.0)),
        (None, Some((priority, _, at))) => Some((priority, true, at, false)),
        (Some((build_priority, levied, _, _, build_at)), Some((bench_priority, _, bench_at))) => {
            if bench_priority <= build_priority {
                Some((bench_priority, true, bench_at, false))
            } else {
                Some((build_priority, false, build_at, levied.0))
            }
        }
    };
    let take_buildish = |staffed: &mut StaffedBand, at: usize, is_bench: bool| {
        if is_bench {
            take_one_bench_hand(&mut staffed.bench[at].crew);
        } else {
            take_one_builder(&mut staffed.builds[at].1);
        }
    };
    match (row_pick, buildish) {
        (None, None) => return false,
        (Some((_, _, _, _, at)), None) => take_one_row_hand(&mut staffed.rows[at].1),
        (None, Some((_, is_bench, at, _))) => take_buildish(staffed, at, is_bench),
        (Some(row), Some(build)) => {
            // At equal priority any levy head (row or build) goes before any of the band's own; between
            // two heads of one side the rows' surplus goes before a build's builders.
            let row_levied = row.2 .0;
            let build_levied = build.3;
            let own_row_against_levy_build = !row_levied && build_levied;
            if row.1 < build.0 || (row.1 == build.0 && !own_row_against_levy_build) {
                take_one_row_hand(&mut staffed.rows[row.4].1);
            } else {
                take_buildish(staffed, build.2, build.1);
            }
        }
    }
    true
}

/// **THE RANK OF A DECLARED BUILD'S SITE** - the row working the source, the Roadwork row for a
/// road, the band improvement's row when it has one, else Normal. A build has no mark of its own.
/// The one reading the staffing passes, the settle trim and the labor pass all share.
pub fn build_site_priority(allocation: &LaborAllocation, source: &BuildSource) -> SourcePriority {
    allocation
        .assignments
        .iter()
        .find(|row| match source {
            BuildSource::Road(_) => matches!(row.target, LaborTarget::Roadwork),
            _ => source.names(&row.target),
        })
        .map_or(SourcePriority::default(), |row| row.priority)
}

/// Take one hand off a staffed row's crew: bare first, then kitted, then a keeper.
fn take_one_row_hand(crew: &mut StaffedCrew) {
    if crew.bare > NOBODY {
        crew.bare -= 1;
    } else if crew.kitted > NOBODY {
        crew.kitted -= 1;
    } else {
        crew.keepers -= 1;
    }
}

/// Take one crafter off a bench order's crew: bare first, then the tool-holders.
fn take_one_bench_hand(crew: &mut crate::components::BenchCrew) {
    if crew.by_hand > NOBODY {
        crew.by_hand -= 1;
    } else {
        crew.at_tool = crew.at_tool.saturating_sub(1);
    }
}

/// Take one builder off a build's crew: bare first, then kitted.
fn take_one_builder(crew: &mut BuildCrew) {
    if crew.bare > NOBODY {
        crew.bare -= 1;
    } else {
        crew.kitted -= 1;
    }
}

/// **THE NUMBER OF UNITS OF EACH ITEM ON THE SHELF WHEN THE BAND'S WORK IS PRICED AT "AS MANY AS IT
/// WANTS"** — far above any crew a band fields, so no row is ever short of kit in that run, yet small
/// enough that unit and head arithmetic in `u32` and `f32` stays exact.
pub const UNLIMITED_SHELF_UNITS: u32 = 1 << 20;

/// One kit the band's work wants, in the unit the outfit picker orders in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitWant {
    /// The `equipment.json` kit id.
    pub kit: String,
    /// Whole kits the rows using it would arm.
    pub count: u32,
    /// The highest rank among the rows wanting it.
    pub priority: SourcePriority,
    /// The nearest wanting row's distance from camp (orders equal ranks).
    pub distance: u32,
}

/// **WHAT THE BAND'S WORK WANTS TO BE OUTFITTED WITH**, and the crews that run produced.
#[derive(Debug, Clone, Default)]
pub struct WorkKitPlan {
    pub wants: Vec<KitWant>,
    /// Every staffed row's keepers and hold and reach crews (passes 1 and 2) in the unlimited-shelf
    /// run - the crews the wants are read off - keyed by its target.
    pub crews: Vec<(LaborTarget, StaffedCrew)>,
}

/// **RUN THE HOLD AND REACH PASSES AGAINST AN UNLIMITED SHELF** over the band's real rows, pool,
/// Maxes, floors and priorities, and read off the kitted heads each row gets from passes 1 and 2 -
/// the crews that last. Pass 3 (the drawdown that pulls a stock down to its floor in a turn) adds no
/// want: its hands work with whatever kit is left, or bare. The band's own ledger is replaced by one
/// holding [`UNLIMITED_SHELF_UNITS`] fresh units of every item any row's kit references, so the
/// answer does not move as the player picks an outfit, only as the work changes. A row's heads
/// become whole kits (`units_for`: a kit is claimed whole, the scarcest item deciding) and are
/// summed over the rows using that kit.
pub fn work_kit_plan<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    pool: u32,
    available: u32,
) -> WorkKitPlan {
    let equipment = world.sources.equipment;
    let mut unlimited = BandEquipment::default();
    for row in allocation
        .assignments
        .iter()
        .filter(|row| row.target.is_sim_staffed())
    {
        for item in row.kit_choice(equipment).uses() {
            if unlimited.count_of(item) != NOBODY {
                continue;
            }
            if let Some(def) = equipment.item(item) {
                unlimited.stock(item, UNLIMITED_SHELF_UNITS, &def.default_tier().id, None);
            }
        }
    }
    let inputs = build_staff_inputs(world, allocation, &unlimited, available);
    let mut shelf: BTreeMap<Arc<str>, u32> = BTreeMap::new();
    for row in &inputs.rows {
        for (item, _) in &row.kit_items {
            shelf.insert(Arc::clone(item), UNLIMITED_SHELF_UNITS);
        }
    }
    for (item, _) in &inputs.scouts.kit_items {
        shelf.insert(Arc::clone(item), UNLIMITED_SHELF_UNITS);
    }
    let (staffed, scout) = staff_passes_scouting(
        &inputs.rows,
        pool,
        &mut shelf,
        Passes::Steady,
        &inputs.tile_room,
        &inputs.scouts,
    );
    let mut wants: BTreeMap<String, KitWant> = BTreeMap::new();
    // The Scout row wants one kit per kitted scout its range needs, ranked at its own Priority.
    if let Some(row) = allocation
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Scout))
    {
        let kits_needed = inputs
            .scouts
            .kit_items
            .iter()
            .map(|(_, people)| units_for(scout.kitted, *people))
            .max()
            .unwrap_or(NOBODY);
        if kits_needed > NOBODY {
            let kit = row.kit_choice(equipment);
            let want = wants
                .entry(kit.id().to_string())
                .or_insert_with(|| KitWant {
                    kit: kit.id().to_string(),
                    count: NOBODY,
                    priority: row.priority,
                    distance: NOBODY,
                });
            want.count = want.count.saturating_add(kits_needed);
            want.priority = want.priority.max(row.priority);
            want.distance = NOBODY;
        }
    }
    for ((row, kit), crew) in inputs.rows.iter().zip(&inputs.kits).zip(&staffed) {
        let kits_needed = row
            .kit_items
            .iter()
            .map(|(_, people)| units_for(crew.kitted, *people))
            .max()
            .unwrap_or(NOBODY);
        if kits_needed == NOBODY {
            continue;
        }
        let want = wants
            .entry(kit.id().to_string())
            .or_insert_with(|| KitWant {
                kit: kit.id().to_string(),
                count: NOBODY,
                priority: row.priority,
                distance: row.distance,
            });
        want.count = want.count.saturating_add(kits_needed);
        want.priority = want.priority.max(row.priority);
        want.distance = want.distance.min(row.distance);
    }
    let mut wants: Vec<KitWant> = wants.into_values().collect();
    wants.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.distance.cmp(&b.distance))
            .then_with(|| a.kit.cmp(&b.kit))
    });
    WorkKitPlan {
        wants,
        crews: inputs.targets.into_iter().zip(staffed).collect(),
    }
}

/// The staffed rows of `allocation` as the passes read them, over `band_kit`.
fn build_staff_inputs<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    available: u32,
) -> StaffInputs<'a> {
    let sources = world.sources;
    let equipment = sources.equipment;
    let crew_bound = available.max(1);
    let crew_float = crew_bound as f32;

    let mut rows: Vec<StaffRowInput<'a>> = Vec::new();
    let mut targets: Vec<LaborTarget> = Vec::new();
    let mut staffed_kits: Vec<KitChoice> = Vec::new();
    let mut staffed_camp_kills: Vec<bool> = Vec::new();
    // **THE ONE KIT A ROW IS HELD TO** - a camp-kill hunt's is narrowed to the items that bind a
    // kill, read off the herd's tile as the passes see it, not off a stamp from an earlier staffing.
    let camp_kills: Vec<bool> = allocation
        .assignments
        .iter()
        .map(|row| is_camp_kill(world, &row.target))
        .collect();
    let kits: Vec<KitChoice> = allocation
        .assignments
        .iter()
        .zip(&camp_kills)
        .map(|(row, camp_kill)| {
            let kit = row.kit_choice(equipment);
            if *camp_kill {
                equipment.camp_kill_kit(&kit)
            } else {
                kit
            }
        })
        .collect();
    let items: Vec<Vec<(Arc<str>, u32)>> = allocation
        .assignments
        .iter()
        .zip(&kits)
        .map(|(row, kit)| {
            if row.target.is_staffed() {
                live_kit_items(kit, band_kit, world)
            } else {
                Vec::new()
            }
        })
        .collect();

    let wanted: Vec<Vec<(Arc<str>, u32)>> = allocation
        .assignments
        .iter()
        .zip(&kits)
        .map(|(row, kit)| {
            if row.target.is_staffed() {
                all_kit_items(kit, equipment)
            } else {
                Vec::new()
            }
        })
        .collect();

    for (idx, row) in allocation.assignments.iter().enumerate() {
        if !row.target.is_staffed() {
            continue;
        }
        let kit = &kits[idx];
        let kit_items = items[idx].clone();
        let kit_wants = wanted[idx].clone();
        let target = row.target.clone();
        let declared = declared_on(allocation, &target);
        let mut input = StaffRowInput {
            priority: row.priority,
            max: row.max_workers,
            keep_hands: NO_HANDS,
            hold_take: NO_TAKE_WANTED,
            useful_take: NO_TAKE_WANTED,
            reach: ReachTarget::Nothing,
            kit_items,
            hands: Box::new(|_, _| Some(0.0)),
            bare_can_work: false,
            distance: NOBODY,
            is_build: false,
            is_bench: false,
            kitless: false,
            tile: crate::systems::party_source_position(&target, sources.herds),
            food_keep_up: allocation.keeps_food_up(&target),
            whole_hands: false,
            sustained: None,
            food_per_take: NO_FOOD,
            finish: None,
            levy: row.for_tribe.is_some(),
            bench_kit: None,
            kit_wants,
            build_work_left: NO_HANDS,
        };
        if let (Some(camp), Some(site)) = (
            &world.camp,
            crate::systems::party_source_position(&target, sources.herds),
        ) {
            input.distance = camp.distance_to(site);
        }
        match &target {
            LaborTarget::Forage {
                tile,
                floor,
                take_species,
                ..
            } => {
                input.keep_hands =
                    planned_keep_hands(sources, band_kit, declared, &target, crew_bound);
                forage_input(
                    world,
                    band_kit,
                    kit,
                    *tile,
                    *floor,
                    take_species,
                    crew_float,
                    &mut input,
                );
                if allocation.row_order(&target).is_some() {
                    apply_chief_share(
                        &mut input,
                        allocation
                            .keep_share(allocation.row_order(&target).unwrap_or_default(), *tile),
                    );
                }
            }
            LaborTarget::Hunt {
                fauna_id, floor, ..
            } => {
                input.keep_hands =
                    planned_keep_hands(sources, band_kit, declared, &target, crew_bound);
                hunt_input(
                    world, band_kit, kit, fauna_id, *floor, crew_float, &mut input,
                );
                if allocation.row_order(&target).is_some() {
                    apply_chief_share(&mut input, allocation.keep_share_of_herd(fauna_id));
                }
            }
            LaborTarget::Extract {
                tile,
                material,
                floor,
            } => {
                input.keep_hands =
                    planned_keep_hands(sources, band_kit, declared, &target, crew_bound);
                extract_input(
                    world, allocation, band_kit, kit, *tile, material, *floor, crew_float,
                    &mut input,
                );
            }
            LaborTarget::Tile { .. } => {
                input.keep_hands = tile_keepers(world, band_kit, &target, crew_bound);
            }
            LaborTarget::Band { .. } => {
                // It stands at the camp: distance 0, and the row is never a work party.
                input.keep_hands = band_keepers(world, band_kit, &target, crew_bound);
            }
            LaborTarget::Roadwork => {
                input.keep_hands = roadwork_keepers(world, band_kit, crew_bound);
            }
            LaborTarget::Scout | LaborTarget::Warrior => continue,
        }
        rows.push(input);
        targets.push(target);
        staffed_kits.push(kit.clone());
        staffed_camp_kills.push(camp_kills[idx]);
    }
    let builds = build_inputs(world, allocation, band_kit, &mut rows);
    let bench_orders = bench_inputs(world, &mut rows);
    // The room each referenced tile has for this band: the cap less what the other bands hold.
    let tile_cap = sources.labor.tile_worker_cap;
    let mut tile_room: BTreeMap<TileKey, u32> = BTreeMap::new();
    for tile in rows.iter().filter_map(|row| row.tile).map(tile_key) {
        tile_room.entry(tile).or_insert_with(|| {
            tile_cap.saturating_sub(
                world
                    .tile_hands_elsewhere
                    .get(&tile)
                    .copied()
                    .unwrap_or(NOBODY),
            )
        });
    }
    // A build's published cap is also bounded by the room its tile started with.
    let builds = builds
        .into_iter()
        .zip(rows.iter().skip(targets.len()))
        .map(|(mut meta, row)| {
            if let Some(room) = row.tile.and_then(|tile| tile_room.get(&tile_key(tile))) {
                meta.cap = meta.cap.min(*room);
            }
            meta
        })
        .collect();
    StaffInputs {
        rows,
        targets,
        kits: staffed_kits,
        camp_kills: staffed_camp_kills,
        builds,
        tile_room,
        scouts: scout_plan(world, allocation, band_kit),
        bench_orders,
    }
}

/// **THE BENCH'S WORKABLE ORDERS AS PASS INPUTS** (#873), in rank order (priority High to Low, then
/// queue order). Each carries its own priority, a Max of its crew cap, the bench tool as a one-person
/// kit and distance 0 (the bench stands at camp). Returns each input's queue index.
fn bench_inputs<'a>(world: &StaffingWorld<'a>, rows: &mut Vec<StaffRowInput<'a>>) -> Vec<usize> {
    let mut queue_indices = Vec::new();
    for &index in &world.bench.rank {
        let plan = &world.bench.orders[index];
        if !plan.workable {
            continue;
        }
        rows.push(StaffRowInput {
            priority: plan.priority,
            max: Some(plan.crew_cap),
            keep_hands: NO_HANDS,
            hold_take: NO_TAKE_WANTED,
            useful_take: NO_TAKE_WANTED,
            reach: ReachTarget::Nothing,
            kit_items: plan
                .tool_item
                .iter()
                .map(|item| (Arc::clone(item), ONE_CRAFTER_PER_TOOL))
                .collect(),
            hands: Box::new(|_, _| Some(0.0)),
            bare_can_work: plan.bare_can_work,
            distance: NOBODY,
            is_build: false,
            is_bench: true,
            whole_hands: false,
            sustained: None,
            food_per_take: NO_FOOD,
            finish: None,
            levy: false,
            bench_kit: Some(plan.kit.clone()),
            kit_wants: Vec::new(),
            build_work_left: NO_HANDS,
            kitless: false,
            tile: None,
            food_keep_up: false,
        });
        queue_indices.push(index);
    }
    queue_indices
}

/// **ONE BENCH TOOL ARMS ONE CRAFTER** - a tool is held by one hand at a time.
const ONE_CRAFTER_PER_TOOL: u32 = 1;

/// **THE BAND'S SCOUTING RANGE, RESOLVED** (#875) - the Scout row's range, what a kitted and a bare
/// scout each add (the equipped and the bare vantage tier, both through
/// [`crate::equipment_config::EquipmentConfig::scout_vantage_range`]) and the wayfinding items the
/// band can still arm a scout with. The default plan (`range` 0) when there is no Scout row.
fn scout_plan(
    world: &StaffingWorld<'_>,
    allocation: &LaborAllocation,
    band_kit: &BandEquipment,
) -> ScoutPlan {
    let Some(row) = allocation
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Scout))
    else {
        return ScoutPlan {
            levy: world.levy.clone(),
            stretch: crate::components::UNSTRETCHED,
            ..ScoutPlan::default()
        };
    };
    let equipment = world.sources.equipment;
    let equipped = world.sources.labor.scout.vantage_range as f32;
    let kit = row.kit_choice(equipment);
    let tiles = |kit: &KitChoice| {
        equipment
            .scout_vantage_range(equipped, kit, band_kit)
            .max(0.0)
            .round() as u32
    };
    ScoutPlan {
        levy: world.levy.clone(),
        stretch: row.scouting_stretch,
        range: row.scouting_range,
        kitted_tiles: tiles(&kit),
        bare_tiles: tiles(&equipment.no_kit()),
        kit_items: live_kit_items(&kit, band_kit, world),
    }
}

/// **THE DECLARED BUILDS AS BUILD INPUTS**, pushed after the source rows. Each is ranked by its
/// site's one Priority, sits at its site's distance from camp, is capped by the in-flight rung's
/// `max_builders` (zero when it cannot progress) and is armed from the rung's build tools.
fn build_inputs<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    rows: &mut Vec<StaffRowInput<'a>>,
) -> Vec<BuildMeta> {
    let sources = world.sources;
    let equipment = sources.equipment;
    let ladder = sources.ladder;
    let mut metas = Vec::new();
    for declaration in &allocation.builds {
        let source = &declaration.source;
        let rung = crate::systems::build_rung_in_flight(
            declaration,
            sources.forage,
            sources.herds,
            world.roads,
            world.tile_improvements,
            sources.deposits,
            ladder,
        );
        let rung_cap = ladder
            .rung_by_id(rung)
            .and_then(|def| def.build.as_ref())
            .map_or(NOBODY, |build| build.max_builders);
        let priority = build_site_priority(allocation, source);
        // A build on a site the tribe levies is the tribe's: dealt in the levy step, and capped by
        // the carrying row's Max (the overflow shortfall) when it has one.
        let levy_row = allocation
            .assignments
            .iter()
            .find(|row| row.for_tribe.is_some() && source.names(&row.target));
        let levy_site = levy_row.is_some()
            || world
                .levy
                .as_ref()
                .is_some_and(|plan| plan.levies_road_build(source));
        let levy_cap = levy_row.and_then(|row| row.max_workers);
        let tile = build_site_tile(source, sources.herds);
        let distance = match (&world.camp, tile) {
            (Some(camp), Some(site)) => camp.distance_to(site),
            _ => NOBODY,
        };
        let toe = equipment.pool_toe(rung.branch, Some(&rung.wire_key()));
        let kit_items = live_kit_items(toe.kit(), band_kit, world);
        let can_progress = rung_cap > NOBODY && (world.build_can_progress)(declaration);
        let work_left = (world.build_work_remaining)(declaration);
        let finish = patch_build_finish(world, declaration)
            .or_else(|| herd_build_finish(world, declaration));
        rows.push(StaffRowInput {
            priority,
            max: Some(if can_progress {
                levy_cap.map_or(rung_cap, |cap| cap.min(rung_cap))
            } else {
                NOBODY
            }),
            keep_hands: NO_HANDS,
            hold_take: NO_TAKE_WANTED,
            useful_take: NO_TAKE_WANTED,
            reach: ReachTarget::Nothing,
            kit_items,
            hands: Box::new(|_, _| Some(0.0)),
            bare_can_work: true,
            distance,
            is_build: true,
            is_bench: false,
            kitless: toe.tools().is_empty(),
            tile,
            food_keep_up: false,
            whole_hands: false,
            sustained: None,
            food_per_take: NO_FOOD,
            finish,
            levy: levy_site,
            bench_kit: None,
            kit_wants: Vec::new(),
            build_work_left: work_left,
        });
        metas.push(BuildMeta {
            source: source.clone(),
            cap: rung_cap,
        });
    }
    metas
}

/// **WHAT A PATCH BUILD ADDS TO THE LARDER WHEN IT FINISHES**: the work its climb still owes
/// ([`crate::forage::patch_build_legs`], at the Field's own price), what one bare builder banks a
/// turn, and the food a turn the destination rung pays above the rung the patch holds
/// ([`crate::forage::rung_payoff`] - the quote the wire's `rungQuotes[].foodYield` is struck from).
/// `None` for any other source, and for a climb that adds nothing: a herd's rung is not counted, so
/// the check errs toward feeding the band first.
fn patch_build_finish(
    world: &StaffingWorld<'_>,
    declaration: &crate::components::BuildDeclaration,
) -> Option<BuildFinish> {
    let BuildSource::Patch(tile) = &declaration.source else {
        return None;
    };
    let sources = world.sources;
    let ladder = sources.ladder;
    let patch = sources.patch(*tile)?;
    let destination = declaration.declared.destination(ladder);
    let composition: Vec<crate::flora_config::FloraShare> = (sources.ground_of)(*tile)
        .map(|ground| {
            crate::forage::tile_flora_composition(
                sources.flora,
                &sources.labor.forage,
                &ground,
                sources.map_seed,
            )
            .into_owned()
        })
        .unwrap_or_default();
    let price = crate::forage::patch_field_cost_multiplier(
        &patch,
        &composition,
        sources.flora,
        &sources.labor.forage,
        ladder,
    );
    let work_remaining: f32 = crate::forage::patch_build_legs(&patch, destination, ladder, price)
        .iter()
        .map(|leg| leg.work_remaining)
        .sum();
    let payoff_at = |rung: crate::forage::PlantPayoff| {
        crate::forage::rung_payoff(
            &patch,
            &composition,
            &sources.labor.forage,
            sources.flora,
            NO_OUTPUT_BONUS,
            rung,
        )
    };
    let finished = crate::forage::PlantPayoff::of(ladder.rung_by_id(destination)?.trait_set());
    let held = crate::forage::PlantPayoff::of(patch.standing().traits);
    let gain = payoff_at(finished) - payoff_at(held);
    (work_remaining > NO_HANDS && gain > NO_FOOD).then_some(BuildFinish {
        work_remaining,
        work_per_builder: crate::intensification::PER_WORKER_OUTPUT,
        gain,
    })
}

/// **WHAT A HERD BUILD ADDS TO THE LARDER WHEN IT FINISHES**: the work the destination rung still
/// owes on this herd (its price on this species, less what the herd's position has banked into it),
/// what one bare builder banks a turn, and the food a turn the destination's block pays above the
/// block the herd holds - the sustained MSY the wire's `rungQuotes[].foodYield` is struck from
/// ([`crate::fauna::herd_msy_biomass_at`] at the species' hunt yield). A ring or an output
/// commitment names a rung the herd already holds and so adds nothing. A deposit, road, tile or band
/// build yields no food, and carries no finish.
fn herd_build_finish(
    world: &StaffingWorld<'_>,
    declaration: &crate::components::BuildDeclaration,
) -> Option<BuildFinish> {
    let BuildSource::Herd(id) = &declaration.source else {
        return None;
    };
    let sources = world.sources;
    let (ladder, fauna) = (sources.ladder, sources.fauna);
    let herd = sources.herd(id)?;
    let destination = declaration.declared.destination(ladder);
    let rung = ladder.rung_by_id(destination)?;
    let cost = rung.build_cost(match rung.cost_scale {
        crate::intensification::CostScale::SpeciesTaming => {
            fauna.taming_cost_multiplier_for(&herd.species)
        }
        _ => crate::intensification::RUNG_COST_UNSCALED,
    })?;
    let work_remaining = (cost - herd.rung_work_done(destination, ladder)).max(NO_HANDS);
    let yield_per_biomass = crate::fauna::herd_hunt_yield(&herd, fauna).provisions_per_biomass;
    let msy_at = |payoff| crate::fauna::herd_msy_biomass_at(&herd, fauna, payoff);
    let finished = crate::fauna::HerdPayoff::of(rung.trait_set());
    let held = crate::fauna::HerdPayoff::of(herd.standing().traits);
    let gain = (msy_at(finished) - msy_at(held)) * yield_per_biomass * NO_OUTPUT_BONUS;
    (work_remaining > NO_HANDS && gain > NO_FOOD).then_some(BuildFinish {
        work_remaining,
        work_per_builder: crate::intensification::PER_WORKER_OUTPUT,
        gain,
    })
}

/// **THE PRODUCTIVITY MULTIPLIER A RUNG QUOTE IS STRUCK AT** - the identity: the check prices a band's
/// own crews, and the discontent modifier is the same on both sides of the comparison.
const NO_OUTPUT_BONUS: f32 = 1.0;

/// Forage: a patch's regrowth at the floor and the peak between the floor and today's stock, turned
/// into hands over the basket rates.
#[allow(clippy::too_many_arguments)] // the row's own inputs
fn forage_input<'a>(
    world: &'a StaffingWorld<'a>,
    band_kit: &'a BandEquipment,
    kit: &KitChoice,
    tile: UVec2,
    floor: f32,
    take_species: &crate::components::TakeSelection,
    crew: f32,
    input: &mut StaffRowInput<'a>,
) {
    let sources = world.sources;
    let Some(patch) = sources.patch(tile) else {
        return;
    };
    let composition: Vec<crate::flora_config::FloraShare> = (sources.ground_of)(tile)
        .map(|ground| {
            crate::forage::tile_flora_composition(
                sources.flora,
                &sources.labor.forage,
                &ground,
                sources.map_seed,
            )
            .into_owned()
        })
        .unwrap_or_default();
    let room = crate::forage::crew_take_room(
        &patch,
        &composition,
        sources.flora,
        &sources.labor.forage,
        take_species,
        floor,
    );
    input.food_per_take = crate::forage::patch_provisions_per_biomass_taking(
        &patch,
        &composition,
        sources.flora,
        &sources.labor.forage,
        take_species,
    );
    let equipment = sources.equipment;
    let season = (sources.season_of)(tile);
    let base = sources.labor.forage.per_worker_biomass_capacity;
    let per_hand = move |crew_kit: &KitChoice| {
        crate::forage::forage_per_worker_biomass(
            equipment.forage_per_worker_biomass_capacity(base, crew_kit, band_kit),
            season,
        )
    };
    let bare = per_hand(&equipment.no_kit());
    input.bare_can_work = bare > 0.0;
    // **GROUND THAT REFUSES GATHERING HAS NO TAKE TO STAFF.** A zero season is the gather seam's
    // answer for wild ground off a gathering site (`forage::gather_season`), so the row asks for no
    // take hands: hold, reach and useful takes stay at nothing and only the keepers (and any build on
    // the patch, dealt as a build) are dealt.
    if season <= crate::forage::NO_FORAGE_SEASON {
        input.bare_can_work = false;
        return;
    }
    let items = input.kit_items.clone();
    let coverage = coverage_with(equipment, kit.clone(), band_kit, items, crew);
    input.hands =
        Box::new(move |target, armed| Some(coverage(armed).hands_to_reach(target, per_hand, bare)));
    if room <= NO_TAKE_WANTED {
        return;
    }
    let share = crate::forage::selected_biomass_share(
        &crate::forage::patch_composition(
            &patch,
            &composition,
            sources.flora,
            &sources.labor.forage,
        ),
        take_species,
    );
    let ecology = crate::forage::patch_ecology(&patch, &sources.labor.forage);
    let capacity = patch.carrying_capacity;
    let reseed = sources.labor.forage.reseed_floor_fraction;
    let growth = |stock: f32| {
        crate::fauna::reseeding_logistic_regrowth(stock, capacity, ecology.regrowth_rate, reseed)
            - stock
    };
    let (low, high) = crate::fauna::floor_reach_band(floor, patch.biomass, capacity);
    let hold = growth(floor * capacity).max(0.0) * share;
    let peak = crate::fauna::peak_regrowth_between(capacity, low, high, growth) * share;
    input.hold_take = hold.min(room);
    input.useful_take = room;
    input.reach = ReachTarget::Take(reach_target(peak, room));
}

/// **A CHIEF'S FORAGE OR HUNT ROW IS STAFFED FROM ITS SHARE OF THE ORDER'S WANT** (#860): the hold and
/// reach the managing pass set aside (biomass), never past the room the row's floor leaves. A row the
/// pass gave no share (a pinned or improved row off the set, or at a zero want) is staffed to nobody -
/// its keepers still come.
fn apply_chief_share(input: &mut StaffRowInput<'_>, share: Option<crate::components::KeepShare>) {
    let room = input.useful_take;
    let (hold, take) = share.map_or((NO_TAKE_WANTED, NO_TAKE_WANTED), |share| {
        (share.hold, share.take)
    });
    input.hold_take = hold.min(room);
    input.useful_take = take.min(room);
    input.reach = if take <= NO_TAKE_WANTED {
        ReachTarget::Nothing
    } else {
        ReachTarget::Take(take.min(room))
    };
}

/// `T_reach`: strictly above the peak regrowth, never past the room, and the whole room where
/// nothing regrows to out-take.
fn reach_target(peak: f32, room: f32) -> f32 {
    if peak <= NO_TAKE_WANTED {
        room
    } else {
        (peak * (1.0 + REACH_STRICTLY_ABOVE_PEAK)).min(room)
    }
}

/// Hunt (and pens): the herd's regrowth at the floor on the meat side, the fight and the haul turned
/// into hands over the partly-armed party.
fn hunt_input<'a>(
    world: &'a StaffingWorld<'a>,
    band_kit: &'a BandEquipment,
    kit: &KitChoice,
    fauna_id: &str,
    floor: f32,
    crew: f32,
    input: &mut StaffRowInput<'a>,
) {
    let sources = world.sources;
    input.whole_hands = true;
    let Some(herd) = sources.herd(fauna_id) else {
        return;
    };
    let herd = herd.into_owned();
    let equipment = sources.equipment;
    let fauna = sources.fauna;
    input.food_per_take = crate::fauna::herd_hunt_yield(&herd, fauna).provisions_per_biomass;
    let room = crate::fauna::hunt_crew_room(&herd, fauna, floor);
    let items = input.kit_items.clone();
    let coverage = coverage_with(equipment, kit.clone(), band_kit, items, crew);
    let party_of = move |cov: &KitCoverage, herd: &crate::fauna::Herd| {
        PartyResolution {
            equipment,
            coverage: cov,
            wear: band_kit,
            intrinsic: sources.person,
            tuning: sources.combat.tuning(),
            hunt_injury_damage_per_animal: sources.combat.hunt_injury_damage_per_animal,
            drives_known: sources.drives_known,
        }
        .party_against(Quarry::Mass(herd.body_mass))
    };
    let bare_party = party_of(&coverage(NOBODY), &herd);
    input.bare_can_work = !matches!(
        crate::fauna::FightRate::of(
            &bare_party,
            crate::fauna::herd_fight_stage(&herd, fauna).as_ref()
        ),
        crate::fauna::FightRate::CannotWound
    );
    let base = sources.labor.hunt.per_worker_biomass_capacity;
    let bare_carry =
        equipment.hunt_per_worker_biomass_capacity(base, &equipment.no_kit(), band_kit);
    let drives = sources.drives_known;
    // The hands closure for a room bound: the take sizes on the room, the sustained take's fit on none.
    let coverage = std::rc::Rc::new(coverage);
    let party_of = std::rc::Rc::new(party_of);
    let hands_within = |room_bound: f32| -> Box<dyn Fn(f32, u32) -> Option<f32> + 'a> {
        let (coverage, party_of) = (coverage.clone(), party_of.clone());
        let hands_herd = herd.clone();
        let no_kit_carry = bare_carry;
        Box::new(move |target, armed| {
            let cov = coverage(armed);
            let party = party_of(&cov, &hands_herd);
            crate::fauna::hunt_crew_for_take(
                target,
                room_bound,
                &hands_herd,
                fauna,
                &cov,
                |crew_kit| equipment.hunt_per_worker_biomass_capacity(base, crew_kit, band_kit),
                no_kit_carry,
                &party,
                drives,
            )
            .map(|crew| crew as f32)
        })
    };
    input.hands = hands_within(room);
    if room <= NO_TAKE_WANTED {
        return;
    }
    let capacity = crate::fauna::herd_capacity(&herd, fauna);
    let ecology = crate::fauna::herd_ecology(&herd, fauna);
    let meat = herd.meat_take_fraction();
    let growth = |stock: f32| crate::fauna::regrowth_delta_at(&herd, stock, capacity, &ecology);
    let (low, high) = crate::fauna::floor_reach_band(floor, herd.biomass, capacity);
    let hold = growth(floor * capacity).max(0.0) * meat;
    let peak = crate::fauna::peak_regrowth_between(capacity, low, high, growth) * meat;
    input.hold_take = hold.min(room);
    // **Only a KEPT herd sizes its hold crew on the regrowth** (the keeping's fraction rides a hunter's
    // slack only where the slack reaches it). A wild herd keeps nobody and takes its room this turn:
    // sizing it on a regrowth its room cannot yield dealt bare hands to a row that takes nothing.
    if hold > room && input.keep_hands > NO_HANDS {
        input.sustained = Some(SustainedTake {
            take: hold,
            hands: hands_within(f32::INFINITY),
        });
    }
    input.useful_take = room;
    input.reach = ReachTarget::Take(reach_target(peak, room));
}

/// Extraction: wood regrows and follows the general rule; stone (no regrowth) holds nothing and its
/// reach is the Max in heads.
#[allow(clippy::too_many_arguments)] // the row's own inputs
fn extract_input<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    kit: &KitChoice,
    tile: UVec2,
    material: &str,
    floor: f32,
    crew: f32,
    input: &mut StaffRowInput<'a>,
) {
    let sources = world.sources;
    let Some((working, ground)) = sources.working(tile, material) else {
        return;
    };
    let equipment = sources.equipment;
    let config = sources.extraction;
    let ladder = sources.ladder;
    let payoff = crate::extraction::deposit_payoff(working.standing(), ladder);
    let capacity = crate::extraction::tile_deposit_capacity(config, material, &ground);
    let ground_rate = crate::extraction::tile_deposit_regrowth(config, material, &ground);
    let room =
        crate::extraction::deposit_reachable(working.stock, capacity, ground_rate, &payoff, floor);
    let held = working.standing().held;
    let carry = crate::extraction::DepositCarry::of(sources.labor, sources.materials, material);
    let items = input.kit_items.clone();
    let coverage = coverage_with(equipment, kit.clone(), band_kit, items, crew);
    input.bare_can_work = payoff.yield_per_worker_turn > 0.0;
    let hands_payoff = payoff;
    input.hands = Box::new(move |target, armed| {
        Some(crate::extraction::deposit_crew_needed(
            target,
            held,
            &hands_payoff,
            &coverage(armed),
            equipment,
            band_kit,
            carry.as_ref(),
            0.0,
        ) as f32)
    });
    if room <= crate::extraction::DEPOSIT_EMPTY {
        return;
    }
    let listed = allocation
        .row_order(&LaborTarget::Extract {
            tile,
            material: material.to_string(),
            floor,
        })
        .is_some_and(|kind| kind == material && allocation.chief_owns(material));
    if listed {
        // **A MATERIAL A CHIEF KEEPS ON HAND IS STAFFED FROM ITS ROW'S SHARE OF THE WANT** (#860,
        // #874): the managing pass split the want (the draw plus the deficit spread over
        // `keep_on_hand_refill_turns`) across the sites, each up to what it gives at the order's
        // floor without drawing it down. At or above the target the shares are zero and nobody digs
        // (the row's keepers still come, `keep_hands`); a row off the set has no share and is staffed
        // to nobody. The order's priority ranks the row among the band's work.
        let Some(share) = allocation.keep_share(material, tile) else {
            return;
        };
        if share.take <= NO_TAKE_WANTED {
            return;
        }
        input.hold_take = share.hold.min(room);
        input.useful_take = share.take.min(room);
        input.reach = ReachTarget::Take(share.take.min(room));
        return;
    }
    if ground_rate <= crate::extraction_config::NEVER_RENEWS {
        // A seam that never renews holds nothing; crewed by hand, its reach is the player's Max in heads.
        let wanted = input.max.map_or(NOBODY, |max| {
            max.saturating_sub(input.keep_hands.ceil() as u32)
        });
        input.reach = ReachTarget::Heads(wanted);
        return;
    }
    let rate = ground_rate * payoff.regrowth_multiplier;
    let seed = config.seed_fraction;
    let growth =
        |stock: f32| crate::extraction::deposit_regrowth(stock, capacity, rate, seed) - stock;
    let floor_stock =
        crate::extraction::deposit_effective_floor(capacity, ground_rate, &payoff, floor);
    let (low, high) = (floor_stock, working.stock.max(floor_stock));
    let hold = growth(floor_stock).max(0.0);
    let peak = crate::fauna::peak_regrowth_between(capacity, low, high, growth);
    input.hold_take = hold.min(room);
    input.useful_take = room;
    input.reach = ReachTarget::Take(reach_target(peak, room));
}

/// **WHAT A FORAGE OR HUNT SOURCE WOULD GIVE A CHIEF'S ROW** - the staffing's own reading of the
/// source ([`forage_input`] / [`hunt_input`], so the answer is the one the passes will use), probed
/// before any row exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceProbe {
    /// Whether anyone the band can field takes anything from the source at the floor: there is room
    /// above it and a crew (kitted as the band is) reaches the take. A herd the kit cannot wound is
    /// not viable.
    pub viable: bool,
    /// What the source gives a turn at the floor without being drawn down, in biomass.
    pub sustainable: f32,
    /// The most the chief may ask of the source a turn, in biomass: its regrowth at the floor
    /// ([`Self::sustainable`]) plus the stock standing above the floor spread over the refill turns.
    /// The floor stays the floor - the chief never draws the source below it.
    pub cap: f32,
    /// What one hand of the best-kitted crew takes a turn (biomass), at the [`Self::cap`] take - the
    /// rate the band's kit and carry really give a worker on this source.
    pub per_hand: f32,
    /// Food units a unit of biomass is worth (`0` where the source pays none).
    pub food_rate: f32,
    /// Fodder units a unit of biomass is worth (`0` where the source pays none).
    pub fodder_rate: f32,
}

/// Probe the forage or hunt source `target` names at its own floor for a crew of up to `crew`
/// hands armed as far as the band's gear goes. `None` for any other target and for a source that is
/// gone.
pub(crate) fn probe_source(
    sources: &ClaimSources<'_>,
    band_kit: &BandEquipment,
    target: &LaborTarget,
    crew: f32,
    refill_turns: f32,
) -> Option<SourceProbe> {
    let tile_improvements = crate::tile_improvement::TileImprovementRegistry::default();
    let roads = crate::routes::RoadRegistry::default();
    let elsewhere: BTreeMap<TileKey, u32> = BTreeMap::new();
    let can_progress = |_: &crate::components::BuildDeclaration| false;
    let work_remaining = |_: &crate::components::BuildDeclaration| NO_HANDS;
    let on_hand = |_: &str| 0.0_f32;
    let no_bench = crate::systems::BenchPlan::default();
    let world = StaffingWorld {
        sources,
        tile_improvements: &tile_improvements,
        roads: &roads,
        band: None,
        camp: None,
        tile_hands_elsewhere: &elsewhere,
        build_can_progress: &can_progress,
        build_work_remaining: &work_remaining,
        on_hand: &on_hand,
        food: FoodLedger::OFF,
        levy: None,
        bench: &no_bench,
    };
    let kit = sources.equipment.default_kit(target.kit_job());
    let kit_items = live_kit_items(&kit, band_kit, &world);
    let mut input = StaffRowInput {
        priority: SourcePriority::default(),
        max: None,
        keep_hands: NO_HANDS,
        hold_take: NO_TAKE_WANTED,
        useful_take: NO_TAKE_WANTED,
        reach: ReachTarget::Nothing,
        kit_items,
        hands: Box::new(|_, _| Some(0.0)),
        bare_can_work: false,
        distance: NOBODY,
        is_build: false,
        is_bench: false,
        kitless: false,
        tile: None,
        food_keep_up: false,
        whole_hands: false,
        sustained: None,
        food_per_take: NO_FOOD,
        finish: None,
        levy: false,
        bench_kit: None,
        kit_wants: Vec::new(),
        build_work_left: NO_HANDS,
    };
    let (food_rate, fodder_rate) = match target {
        LaborTarget::Forage {
            tile,
            floor,
            take_species,
            ..
        } => {
            let patch = sources.patch(*tile)?;
            let composition: Vec<crate::flora_config::FloraShare> = (sources.ground_of)(*tile)
                .map(|ground| {
                    crate::forage::tile_flora_composition(
                        sources.flora,
                        &sources.labor.forage,
                        &ground,
                        sources.map_seed,
                    )
                    .into_owned()
                })
                .unwrap_or_default();
            let rates = (
                crate::forage::patch_provisions_per_biomass(
                    &patch,
                    &composition,
                    sources.flora,
                    &sources.labor.forage,
                ),
                crate::forage::patch_fodder_per_biomass(
                    &patch,
                    &composition,
                    sources.flora,
                    &sources.labor.forage,
                ),
            );
            forage_input(
                &world,
                band_kit,
                &kit,
                *tile,
                *floor,
                take_species,
                crew,
                &mut input,
            );
            rates
        }
        LaborTarget::Hunt {
            fauna_id, floor, ..
        } => {
            let herd = sources.herd(fauna_id)?;
            let food = sources
                .fauna
                .hunt_yield_for(&herd.species)
                .provisions_per_biomass;
            hunt_input(&world, band_kit, &kit, fauna_id, *floor, crew, &mut input);
            (food, 0.0)
        }
        _ => return None,
    };
    let sustainable = input.hold_take.max(NO_TAKE_WANTED);
    // The stock above the floor (`useful_take` is the room the floor leaves) is spread over the refill
    // turns; the floor itself is never drawn below.
    let cap = (sustainable + input.useful_take.max(NO_TAKE_WANTED) / refill_turns.max(1.0))
        .min(sustainable.max(input.useful_take));
    let armed = crew as u32;
    let hands = (input.hands)(cap, armed);
    let reaches = cap > NO_TAKE_WANTED && hands.is_some();
    let per_hand = hands
        .filter(|hands| *hands > NO_HANDS)
        .map_or(NO_TAKE_WANTED, |hands| {
            cap / hands.max(AT_LEAST_ONE_HAND as f32)
        });
    Some(SourceProbe {
        viable: reaches && input.useful_take > NO_TAKE_WANTED,
        sustainable,
        cap,
        per_hand,
        food_rate,
        fodder_rate,
    })
}

/// Where `band` is camped, read the way the labor pass reads it; `None` where the world has no map.
/// The tribe's levy plan for `band` as the command path reads it (`None` in no tribe).
fn levy_of_band(world: &bevy::prelude::World, band: bevy::prelude::Entity) -> Option<LevyPlan> {
    let id = *world.get::<BandId>(band)?;
    let registry = world.get_resource::<crate::tribe::TribeRegistry>()?;
    let config = world.get_resource::<crate::leadership_config::LeadershipConfigHandle>()?;
    let own_leader = world
        .get::<crate::leadership::BandLeadership>(band)
        .is_some_and(|leadership| leadership.leader.is_some());
    let ladder = world.get_resource::<crate::LadderConfigHandle>()?.get();
    crate::tribe_levy::levy_plan(registry, &config.get(), &ladder, id, own_leader)
}

pub(crate) fn camp_of(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
) -> Option<CampSite> {
    let cohort = world.get::<crate::components::PopulationCohort>(band)?;
    let position = world
        .get::<crate::components::Tile>(cohort.current_tile)?
        .position;
    let grid_width = world
        .get_resource::<crate::resources::TileRegistry>()?
        .width;
    let wrap_horizontal = world
        .get_resource::<crate::resources::SimulationConfig>()?
        .map_topology
        .wrap_horizontal;
    Some(CampSite {
        position,
        grid_width,
        wrap_horizontal,
    })
}

/// **WHETHER A BUILD OF THE BAND'S OWN IS STILL SHORT OF ITS BUILDERS AND COULD TAKE THEM** - read off
/// the band's last staffing: a declared build (not one on a site the tribe levies) whose published
/// crew is under its cap and which can make progress this turn (no refusing gate, no materials stall).
/// A build the food-first look-ahead pulled its builders off counts: it is short and could take them.
/// The chief waits on it before he offers another build.
pub(crate) fn band_has_a_build_short_of_builders(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
) -> bool {
    with_band_staffing(world, band, |staffing, allocation, _, _, _| {
        allocation.builds.iter().any(|declaration| {
            let levied = staffing
                .levy
                .as_ref()
                .is_some_and(|plan| plan.levies_road_build(&declaration.source))
                || allocation
                    .assignments
                    .iter()
                    .any(|row| row.for_tribe.is_some() && declaration.source.names(&row.target));
            !levied
                && allocation
                    .build_crews
                    .iter()
                    .find(|(source, _)| *source == declaration.source)
                    .is_some_and(|(_, crew)| crew.total() < crew.cap)
                && (staffing.build_can_progress)(declaration)
        })
    })
    .unwrap_or(false)
}

/// **WHETHER THE BAND'S DECLARED BUILD ON `source` CAN MAKE PROGRESS** (open to builders: no refusing
/// gate, no materials stall, not already standing), by the one predicate staffing deals builders
/// through. `false` where the band declares no build there.
pub(crate) fn band_build_can_progress(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
    source: &BuildSource,
) -> bool {
    with_band_staffing(world, band, |staffing, allocation, _, _, _| {
        allocation
            .builds
            .iter()
            .find(|declaration| declaration.source == *source)
            .is_some_and(|declaration| (staffing.build_can_progress)(declaration))
    })
    .unwrap_or(false)
}

/// Run `body` over a band's staffing context: the world as the labor pass reads it, regrow-first,
/// the band's allocation, its gear, the pool and the working crew. `None` where the band has no
/// cohort or allocation.
fn with_band_staffing<R>(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
    body: impl FnOnce(&StaffingWorld<'_>, &LaborAllocation, &BandEquipment, u32, u32) -> R,
) -> Option<R> {
    let allocation = world.get::<LaborAllocation>(band)?.clone();
    with_band_staffing_over(world, band, allocation, body)
}

/// [`with_band_staffing`] over an allocation the caller supplies instead of the one the band holds -
/// the seam the compose sheet's dry run (`staffing_preview`) staffs a COPY through. Everything but
/// the allocation is read from the world exactly as the command path reads it.
pub(crate) fn with_band_staffing_over<R>(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
    allocation: LaborAllocation,
    body: impl FnOnce(&StaffingWorld<'_>, &LaborAllocation, &BandEquipment, u32, u32) -> R,
) -> Option<R> {
    let cohort = world.get::<crate::components::PopulationCohort>(band)?;
    let faction = cohort.faction;
    let available = crate::components::available_workers(cohort.working);
    let band_id = world.get::<BandId>(band).copied();
    let camp = camp_of(world, band);
    let pool = staffable_pool(&allocation, available);
    let equipment = world
        .get_resource::<crate::EquipmentConfigHandle>()
        .map_or_else(
            crate::equipment_config::EquipmentConfig::builtin,
            crate::EquipmentConfigHandle::get,
        );
    let band_kit = world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment, available as f32));
    let no_improvements = crate::tile_improvement::TileImprovementRegistry::default();
    let no_roads = crate::routes::RoadRegistry::default();
    let no_herds = crate::fauna::HerdRegistry::default();
    let herds = world
        .get_resource::<crate::fauna::HerdRegistry>()
        .unwrap_or(&no_herds);
    // Every OTHER band's current placements share a tile's room with this one.
    let mut elsewhere: BTreeMap<TileKey, u32> = BTreeMap::new();
    for (other, other_allocation) in world
        .iter_entities()
        .filter_map(|entity| Some((entity.id(), entity.get::<LaborAllocation>()?)))
    {
        if other == band {
            continue;
        }
        for (tile, hands) in tile_hands_of(other_allocation, herds) {
            *elsewhere.entry(tile).or_insert(NOBODY) += hands;
        }
    }
    let no_discovery = crate::resources::DiscoveryProgressLedger::default();
    let no_sites = crate::resources::FoodSiteRegistry::default();
    let no_belief = crate::belief::BeliefRegistry::default();
    let stores = &cohort.stores;
    Some(crate::take_claims::with_world_sources(
        world,
        faction,
        |sources| {
            let tile_improvements = world
                .get_resource::<crate::tile_improvement::TileImprovementRegistry>()
                .unwrap_or(&no_improvements);
            let roads = world
                .get_resource::<crate::routes::RoadRegistry>()
                .unwrap_or(&no_roads);
            let grid = world
                .get_resource::<crate::resources::TileRegistry>()
                .map_or((0, 0), |tiles| (tiles.width, tiles.height));
            let wrap_horizontal = world
                .get_resource::<crate::resources::SimulationConfig>()
                .is_some_and(|config| config.map_topology.wrap_horizontal);
            let progress_context = crate::systems::BuildProgressContext {
                sources,
                roads,
                tile_improvements,
                band_id,
                faction,
                discovery: world
                    .get_resource::<crate::resources::DiscoveryProgressLedger>()
                    .unwrap_or(&no_discovery),
                knowledge_threshold: sources.ladder.knowledge.completion_threshold,
                food_sites: world
                    .get_resource::<crate::resources::FoodSiteRegistry>()
                    .unwrap_or(&no_sites),
                grid,
                wrap_horizontal,
                stores,
                belief: world
                    .get_resource::<crate::belief::BeliefRegistry>()
                    .unwrap_or(&no_belief),
            };
            let can_progress = |declaration: &crate::components::BuildDeclaration| {
                crate::systems::declared_build_can_progress(
                    &progress_context,
                    &allocation,
                    declaration,
                )
            };
            let work_remaining = |declaration: &crate::components::BuildDeclaration| {
                crate::systems::declared_build_work_remaining(&progress_context, declaration)
            };
            let on_hand = |material: &str| stores.material_total(material).to_f32();
            let recipes = world
                .get_resource::<crate::recipes_config::RecipesConfigHandle>()
                .map_or_else(
                    crate::recipes_config::RecipesConfig::builtin,
                    crate::recipes_config::RecipesConfigHandle::get,
                );
            let bench_plan = world
                .get::<crate::components::BandBench>(band)
                .map(|bench| {
                    crate::systems::bench_plan(
                        bench,
                        stores,
                        &recipes,
                        sources.materials,
                        sources.equipment,
                        &band_kit,
                    )
                })
                .unwrap_or_default();
            let staffing_world = StaffingWorld {
                sources,
                tile_improvements,
                roads,
                band: band_id,
                camp,
                tile_hands_elsewhere: &elsewhere,
                build_can_progress: &can_progress,
                build_work_remaining: &work_remaining,
                on_hand: &on_hand,
                food: world
                    .get_resource::<crate::demographics_config::DemographicsConfigHandle>()
                    .map_or(FoodLedger::OFF, |demographics| {
                        food_ledger_of(
                            cohort,
                            &allocation,
                            &demographics.get(),
                            pooling_steady_of(world, band),
                        )
                    }),
                levy: levy_of_band(world, band),
                bench: &bench_plan,
            };
            body(&staffing_world, &allocation, &band_kit, pool, available)
        },
    ))
}

/// **THE BAND'S ALLOCATION WITH ITS KEEP-ON-HAND DIG ROWS PLACED** - [`crate::keep_on_hand::manage_dig_rows`]
/// run over a copy, read from the world the way a command restaff reads it. The caller writes it back
/// before staffing. `None` where the band has no cohort or allocation.
fn managed_dig_allocation(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
) -> Option<LaborAllocation> {
    let mut allocation = world.get::<LaborAllocation>(band)?.clone();
    let cohort = world.get::<crate::components::PopulationCohort>(band)?;
    let faction = cohort.faction;
    let camp = camp_of(world, band);
    let equipment = world
        .get_resource::<crate::EquipmentConfigHandle>()
        .map_or_else(
            crate::equipment_config::EquipmentConfig::builtin,
            crate::EquipmentConfigHandle::get,
        );
    let available = crate::components::available_workers(cohort.working);
    let band_kit = world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment, available as f32));
    let has_leader = world
        .get::<crate::leadership::BandLeadership>(band)
        .is_some_and(|leadership| leadership.leader.is_some());
    let visibility = world.get_resource::<crate::visibility::VisibilityLedger>();
    let fog_enabled = world
        .get_resource::<crate::resources::SimulationConfig>()
        .is_none_or(|config| config.fog_enabled);
    let registry = world.get_resource::<crate::resources::TileRegistry>()?;
    crate::take_claims::with_world_sources(world, faction, |sources| {
        let tiles = || {
            (0..registry.height).flat_map(move |y| {
                (0..registry.width).filter_map(move |x| {
                    registry
                        .index(x, y)
                        .and_then(|entity| world.get::<crate::components::Tile>(entity))
                })
            })
        };
        let candidates = |material: &str| {
            crate::keep_on_hand::discovered_deposit_tiles(
                sources.extraction,
                material,
                faction,
                visibility,
                fog_enabled,
                tiles(),
            )
        };
        let supply = world
            .resource::<crate::supply_network_config::SupplyNetworkConfigHandle>()
            .get();
        let access = crate::routes::road_access_in_world(world, faction, sources.ladder);
        let widest_route_reach = crate::routes::max_route_reach_tiles(sources.ladder);
        let geometry = (
            registry.width,
            registry.height,
            world
                .resource::<crate::resources::SimulationConfig>()
                .map_topology
                .wrap_horizontal,
        );
        let walk_to = |tile: UVec2| {
            camp.as_ref().and_then(|camp| {
                crate::work_party::resolve_walk(
                    camp.position,
                    tile,
                    sources.labor,
                    &supply,
                    world.resource::<crate::routes::RoadRegistry>(),
                    &access,
                    widest_route_reach,
                    geometry,
                )
            })
        };
        let inputs = crate::keep_on_hand::DigSiteInputs {
            sources,
            band_kit: &band_kit,
            camp: camp.as_ref(),
            walk_to: &walk_to,
        };
        let pool = staffable_pool(&allocation, available);
        let cohort_stores = &cohort.stores;
        let on_hand = |material: &str| cohort_stores.material_total(material).to_f32();
        let rivals = crate::source_holders::SourceHolders::from_world(world)
            .for_band(world.get::<BandId>(band).copied(), faction);
        crate::keep_on_hand::manage_dig_rows(
            &mut allocation,
            &inputs,
            has_leader,
            &candidates,
            &on_hand,
            &rivals,
        );
        let known_tile = |tile: UVec2| {
            !fog_enabled
                || visibility.is_some_and(|ledger| ledger.is_discovered(faction, tile.x, tile.y))
        };
        let known_herd = |herd: &crate::fauna::Herd| {
            let at = herd.position();
            !fog_enabled || visibility.is_some_and(|ledger| ledger.is_visible(faction, at.x, at.y))
        };
        let demographics = world
            .get_resource::<crate::demographics_config::DemographicsConfigHandle>()
            .map_or_else(
                crate::demographics_config::DemographicsConfig::builtin,
                crate::demographics_config::DemographicsConfigHandle::get,
            );
        crate::food_orders::manage_food_orders(
            &mut allocation,
            &crate::food_orders::FoodInputs {
                keeping: &demographics.keeping,
                site: &inputs,
                tile_known: &known_tile,
                herd_known: &known_herd,
                pool,
                readings: crate::food_orders::OrderReadings::of(
                    cohort,
                    crate::fixed_store::reachable_food_in_world(world, band),
                ),
            },
            has_leader,
            &rivals,
        );
    });
    Some(allocation)
}

/// **STAFF A COPY OF A BAND'S ALLOCATION THE WAY THE COMMAND PATH DOES** — `restaff_band`'s staffing
/// (`staff_rows_settled` with the band's bench and nobody lost, then `apply_staffing` and
/// `apply_build_crews`) run on `allocation` instead of the one the band holds, and returned rather
/// than written, with the crew the same passes gave each bench order. Nothing in the world changes.
/// `None` where the band has no cohort.
///
/// The one staffing the compose sheet's dry run (`staffing_preview`) and the command path share.
pub fn staff_allocation_copy(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
    allocation: LaborAllocation,
) -> Option<(LaborAllocation, Vec<BenchStaffed>)> {
    let staffed = with_band_staffing_over(
        world,
        band,
        allocation.clone(),
        |staffing_world, allocation, kit, pool, available| {
            staff_rows_settled(staffing_world, allocation, kit, pool, available, NOBODY)
        },
    )?;
    let bench = staffed.bench.clone();
    let mut staffed_allocation = allocation;
    staffed.apply_to(&mut staffed_allocation, NOBODY);
    Some((staffed_allocation, bench))
}

/// **RESTAFF ONE BAND OUTSIDE THE TURN** — what a command handler calls after it changed an input
/// the passes read (a floor, a kit, a Max, a priority, a stopped row), so the snapshot shows the new
/// crew the moment the player acts. It reads the world the way the labor pass does, regrow-first
/// ([`crate::fauna::ProjectionStart::BeforeRegrowth`]), and writes only the band's staffed rows; the next turn
/// restaffs from the live state and is the authority. While the band's outfitting window is open it
/// also refreshes the kit its work wants ([`WorkKitWants`]).
pub fn restaff_band(world: &mut bevy::prelude::World, band: bevy::prelude::Entity) {
    restaff_band_after_loss(world, band, NOBODY);
}

/// **THE STAFFED HANDS A BAND HOLDS PAST THE PEOPLE IT HAS** - what it commits (every row, the
/// bench and the walkers) less its working-age head count, capped at the hands the staffed rows
/// hold. This is the turn's `lost_to_staffed` for a command that removed people (a split) before
/// any shed ran: a hand-set row's excess is the next turn's shed to take, so it is not counted.
pub fn staffed_hands_lost(world: &bevy::prelude::World, band: bevy::prelude::Entity) -> u32 {
    let Some(allocation) = world.get::<LaborAllocation>(band) else {
        return NOBODY;
    };
    let Some(cohort) = world.get::<crate::components::PopulationCohort>(band) else {
        return NOBODY;
    };
    let bench_workers = world
        .get::<crate::components::BandBench>(band)
        .map_or(NOBODY, crate::components::BandBench::crew_total);
    let committed = allocation.assigned_total() + bench_workers + allocation.walking_home();
    let staffed_hands: u32 = allocation
        .assignments
        .iter()
        .filter(|row| row.target.is_sim_staffed())
        .map(|row| row.workers)
        .sum();
    committed
        .saturating_sub(crate::components::available_workers(cohort.working))
        .min(staffed_hands)
}

/// **RESTAFF ONE BAND OUTSIDE THE TURN, AFTER IT LOST PEOPLE** - [`restaff_band`] for a command that
/// removed people from the band (a split). `lost_people` is the head count of the staffed hands
/// that left with them: a far party's share of those is discarded where it stands, exactly as the
/// turn does, rather than walked home as hands the band no longer has (they would count against
/// the pool until they "arrived").
pub fn restaff_band_after_loss(
    world: &mut bevy::prelude::World,
    band: bevy::prelude::Entity,
    lost_people: u32,
) {
    restaff_band_inner(world, band, lost_people, ManagingPasses::Run);
}

/// **RESTAFF ONE BAND AT THE END OF THE TURN, AFTER A STEP FREED ITS HANDS** - the same staffing
/// [`restaff_band`] runs, minus the managing passes (`manage_dig_rows`, `manage_food_orders`). Those
/// passes may drop a managed row, and a row dropped now would take the turn's published yield with
/// it; the next turn's labour pass runs them at its head, so only the staffing is repeated here.
/// Called by `systems::restaff_freed_hands`.
pub fn restaff_band_end_of_turn(world: &mut bevy::prelude::World, band: bevy::prelude::Entity) {
    restaff_band_inner(world, band, NOBODY, ManagingPasses::Skip);
}

/// Whether a restaff re-runs the chief's managing passes before staffing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ManagingPasses {
    Run,
    Skip,
}

fn restaff_band_inner(
    world: &mut bevy::prelude::World,
    band: bevy::prelude::Entity,
    lost_people: u32,
    managing: ManagingPasses,
) {
    if managing == ManagingPasses::Run {
        if let Some(managed) = managed_dig_allocation(world, band) {
            if let Some(mut allocation) = world.get_mut::<LaborAllocation>(band) {
                *allocation = managed;
            }
        }
    }
    let Some(staffed) = with_band_staffing(
        world,
        band,
        |staffing_world, allocation, kit, pool, available| {
            staff_rows_settled(
                staffing_world,
                allocation,
                kit,
                pool,
                available,
                lost_people,
            )
        },
    ) else {
        return;
    };
    if let Some(mut bench) = world.get_mut::<crate::components::BandBench>(band) {
        staffed.apply_bench(&mut bench);
    }
    if let Some(mut allocation) = world.get_mut::<LaborAllocation>(band) {
        staffed.apply_to(&mut allocation, lost_people);
    }
    refresh_band_work_kit_wants(world, band);
}

/// **THE KIT THE BAND'S WORK WANTS, PER BAND WITH AN OPEN OUTFITTING WINDOW** - derived each time the
/// band restaffs (and at every recapture), never saved. A band whose window is closed has no entry.
#[derive(bevy::prelude::Resource, Debug, Clone, Default)]
pub struct WorkKitWants {
    by_band: BTreeMap<BandId, Vec<KitWant>>,
}

impl WorkKitWants {
    /// What `band`'s work wants, in picker order; empty when nothing is wanted or the window is closed.
    pub fn of(&self, band: BandId) -> &[KitWant] {
        self.by_band.get(&band).map_or(&[], Vec::as_slice)
    }
}

/// Recompute one band's wants while its window is open; drop them otherwise.
pub fn refresh_band_work_kit_wants(world: &mut bevy::prelude::World, band: bevy::prelude::Entity) {
    let Some(id) = world.get::<BandId>(band).copied() else {
        return;
    };
    let open = world
        .get_resource::<crate::starting_loadout::StartingLoadout>()
        .is_some_and(|windows| windows.is_open(id));
    let wants = if open {
        with_band_staffing(
            world,
            band,
            |staffing_world, allocation, _, pool, available| {
                work_kit_plan(staffing_world, allocation, pool, available).wants
            },
        )
    } else {
        None
    };
    let Some(mut store) = world.get_resource_mut::<WorkKitWants>() else {
        return;
    };
    match wants {
        Some(wants) if !wants.is_empty() => {
            store.by_band.insert(id, wants);
        }
        _ => {
            store.by_band.remove(&id);
        }
    }
}

/// The keepers and hold and reach crews (passes 1 and 2) the band's staffed rows get against an
/// unlimited shelf - the run the wants are read off. `None` where the band has no cohort or allocation.
pub fn unlimited_shelf_crews(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
) -> Option<Vec<(LaborTarget, StaffedCrew)>> {
    with_band_staffing(
        world,
        band,
        |staffing_world, allocation, _, pool, available| {
            work_kit_plan(staffing_world, allocation, pool, available).crews
        },
    )
}

/// Recompute every open window's wants - the recapture hook.
pub fn refresh_work_kit_wants(world: &mut bevy::prelude::World) {
    if !world.contains_resource::<WorkKitWants>() {
        return;
    }
    if !world.contains_resource::<crate::starting_loadout::StartingLoadout>() {
        return;
    }
    let bands: Vec<(bevy::prelude::Entity, BandId)> = world
        .query::<(bevy::prelude::Entity, &BandId)>()
        .iter(world)
        .map(|(entity, id)| (entity, *id))
        .collect();
    let windows = world.resource::<crate::starting_loadout::StartingLoadout>();
    let open: Vec<bevy::prelude::Entity> = bands
        .into_iter()
        .filter(|(_, id)| windows.is_open(*id))
        .map(|(entity, _)| entity)
        .collect();
    let known: Vec<BandId> = world
        .resource::<WorkKitWants>()
        .by_band
        .keys()
        .copied()
        .collect();
    for id in known {
        let still_open = world
            .get_resource::<crate::starting_loadout::StartingLoadout>()
            .is_some_and(|windows| windows.is_open(id));
        if !still_open {
            world.resource_mut::<WorkKitWants>().by_band.remove(&id);
        }
    }
    for entity in open {
        refresh_band_work_kit_wants(world, entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // a test row builder: one argument per row input
    fn linear_row<'a>(
        priority: SourcePriority,
        max: Option<u32>,
        keepers: u32,
        hold: f32,
        reach: f32,
        armed_rate: f32,
        bare_rate: f32,
        kit_items: Vec<(Arc<str>, u32)>,
    ) -> StaffRowInput<'a> {
        let kit_wants = kit_items.clone();
        StaffRowInput {
            priority,
            max,
            keep_hands: keepers as f32,
            hold_take: hold,
            useful_take: reach.max(hold),
            reach: if reach > 0.0 {
                ReachTarget::Take(reach)
            } else {
                ReachTarget::Nothing
            },
            kit_items,
            hands: Box::new(move |target, armed| {
                let armed_take = armed as f32 * armed_rate;
                if target <= armed_take {
                    return Some(target / armed_rate);
                }
                if bare_rate <= 0.0 {
                    return Some(armed as f32);
                }
                Some(armed as f32 + (target - armed_take) / bare_rate)
            }),
            bare_can_work: bare_rate > 0.0,
            distance: 0,
            is_build: false,
            is_bench: false,
            kitless: false,
            tile: None,
            food_keep_up: false,
            whole_hands: false,
            sustained: None,
            food_per_take: NO_FOOD,
            finish: None,
            levy: false,
            bench_kit: None,
            kit_wants,
            build_work_left: NO_HANDS,
        }
    }

    fn at(mut row: StaffRowInput<'_>, distance: u32) -> StaffRowInput<'_> {
        row.distance = distance;
        row
    }

    /// A bare-handed row whose useful take needs `heads` bare hands (rate 1 a head).
    fn bare_row<'a>(priority: SourcePriority, heads: f32) -> StaffRowInput<'a> {
        linear_row(priority, None, 0, 0.0, heads, 4.0, 1.0, Vec::new())
    }

    fn crews(out: &[RowStaffing]) -> Vec<u32> {
        out.iter().map(RowStaffing::total).collect()
    }

    fn shelf(units: u32) -> BTreeMap<Arc<str>, u32> {
        BTreeMap::from([(Arc::from("spears"), units)])
    }

    fn spears() -> Vec<(Arc<str>, u32)> {
        vec![(Arc::from("spears"), 1)]
    }

    #[test]
    fn pass_three_staffs_up_to_the_useful_crew_with_the_kit_first_and_never_past_the_max() {
        // Hold 4 and reach 8 are one hand each at the armed rate 4, so the reach crew is 2; the
        // useful take is 20, which is 5 armed hands. Two spears sit on the shelf; the rest go bare
        // at rate 1 (so 12 more take hands to cover what two spears leave).
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            4.0,
            8.0,
            4.0,
            1.0,
            spears(),
        );
        row.useful_take = 20.0;
        let rows = vec![row];
        let mut units = shelf(2);
        let out = staff_band(&rows, 100, &mut units);
        assert_eq!(
            out[0].kitted, 2,
            "the unspent kit is used before anyone works bare"
        );
        assert_eq!(
            units[&Arc::<str>::from("spears")],
            0,
            "nothing is left on the shelf"
        );
        assert_eq!(
            out[0].bare, 12,
            "bare hands cover the rest of the useful take (20 - 2*4)"
        );
        assert_eq!(
            out[0].useful_crew, 14,
            "the published useful crew is the ceiling pass 3 filled"
        );

        let mut capped = linear_row(
            SourcePriority::Normal,
            Some(5),
            0,
            4.0,
            8.0,
            4.0,
            1.0,
            spears(),
        );
        capped.useful_take = 20.0;
        let mut units = shelf(2);
        let out = staff_band(&[capped], 100, &mut units);
        assert_eq!(out[0].kitted + out[0].bare, 5, "the Max caps pass 3");
    }

    #[test]
    fn the_steady_passes_kit_only_the_hold_and_reach_crew_not_the_drawdown() {
        // A source well above its floor: reach 8 is two armed hands at rate 4, the useful take 52 is
        // thirteen. On an unlimited shelf the full passes kit all thirteen; the steady passes kit
        // the two that keep working once the stock sits at its floor.
        const POOL: u32 = 40;
        const REACH_CREW: u32 = 2;
        const USEFUL_CREW: u32 = 13;
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            4.0,
            8.0,
            4.0,
            1.0,
            spears(),
        );
        row.useful_take = 52.0;
        let rows = vec![row];

        let mut steady_shelf = shelf(UNLIMITED_SHELF_UNITS);
        let steady = staff_passes(
            &rows,
            POOL,
            &mut steady_shelf,
            Passes::Steady,
            &BTreeMap::new(),
        );
        assert_eq!(steady[0].kitted, REACH_CREW, "the reach crew is the want");
        assert_eq!(steady[0].bare, 0, "the steady passes seat no bare hand");

        let mut full_shelf = shelf(UNLIMITED_SHELF_UNITS);
        let full = staff_band(&rows, POOL, &mut full_shelf);
        assert_eq!(
            full[0].kitted, USEFUL_CREW,
            "liveness: the drawdown kits the whole useful crew when the shelf allows"
        );
        assert!(steady[0].kitted < POOL && steady[0].kitted < full[0].kitted);
    }

    #[test]
    fn a_kitted_lower_priority_row_beats_a_bare_higher_priority_one() {
        // One spear. The High row is a hunt that needs 2 heads at the armed rate; the Low row also.
        let rows = vec![
            linear_row(SourcePriority::Low, None, 0, 4.0, 4.0, 4.0, 1.0, spears()),
            linear_row(SourcePriority::High, None, 0, 4.0, 4.0, 4.0, 1.0, spears()),
        ];
        let mut units = shelf(1);
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(out[1].kitted, 1, "the High row is armed first");
        assert_eq!(out[0].kitted, 0, "the Low row is left the bare pass");
        assert_eq!(
            out[0].bare, 4,
            "four bare hands reach four at a rate of one"
        );
    }

    #[test]
    fn a_rows_kitted_heads_come_before_another_rows_bare_ones() {
        // High row has no kit at all; Low row holds the only spear. Kitted-first across priorities:
        // the spear goes to the row that can use it in pass 1, not to a bare head of a higher row.
        let rows = vec![
            linear_row(
                SourcePriority::High,
                None,
                0,
                4.0,
                4.0,
                4.0,
                1.0,
                Vec::new(),
            ),
            linear_row(SourcePriority::Low, None, 0, 4.0, 4.0, 4.0, 1.0, spears()),
        ];
        let mut units = shelf(1);
        let out = staff_band(&rows, 3, &mut units);
        assert_eq!(out[1].kitted, 1);
        assert_eq!(out[0].kitted, 0);
        assert_eq!(
            out[0].bare, 2,
            "the High row's bare heads take what is left of the pool"
        );
    }

    /// A band short of gathering kit with sleds: a High stone refill (kitted heads only help it
    /// past a bare hand's one in four) and a Low food row whose hold crew is three bare hands.
    fn food_hold_and_stone_refill<'a>(food_kept_up: bool) -> Vec<StaffRowInput<'a>> {
        let sleds: Vec<(Arc<str>, u32)> = vec![(Arc::from("sleds"), 1)];
        let mut food = linear_row(SourcePriority::Low, None, 0, 3.0, 3.0, 1.0, 1.0, Vec::new());
        food.food_keep_up = food_kept_up;
        vec![
            linear_row(SourcePriority::High, None, 0, 0.0, 4.0, 1.0, 1.0, sleds),
            food,
        ]
    }

    #[test]
    fn a_chiefs_food_hold_is_dealt_bare_before_another_orders_kitted_refill() {
        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let kept = staff_band(&food_hold_and_stone_refill(true), 5, &mut sleds);
        assert_eq!(
            kept[1].bare, 3,
            "the food row's hold crew is filled bare first"
        );
        assert_eq!(kept[1].hold_crew, 3);
        assert_eq!(
            (kept[0].kitted, kept[0].bare),
            (2, 0),
            "stone gets only the pool the food hold left, kitted"
        );
        assert!(kept[0].kitted > 0 && kept[1].total() > 0, "liveness");
    }

    #[test]
    fn without_the_food_order_the_kitted_refill_still_goes_first() {
        // No leader, or food released to the player: the row is not `food_keep_up`.
        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let out = staff_band(&food_hold_and_stone_refill(false), 5, &mut sleds);
        assert_eq!(out[0].kitted, 3, "stone's kitted reach comes first");
        assert!(
            out[1].total() < out[1].hold_crew,
            "the food hold is left short: {} of {}",
            out[1].total(),
            out[1].hold_crew
        );
        assert!(out[0].kitted > 0 && out[1].total() > 0, "liveness");
    }

    /// A stone refill (kitted heads, sleds) and a food row with a one-hand hold and a four-hand
    /// refill, at the given priorities.
    fn food_refill_and_stone_refill<'a>(
        stone: SourcePriority,
        food: SourcePriority,
        food_kept_up: bool,
    ) -> Vec<StaffRowInput<'a>> {
        let sleds: Vec<(Arc<str>, u32)> = vec![(Arc::from("sleds"), 1)];
        let mut food = linear_row(food, None, 0, 1.0, 4.0, 1.0, 1.0, Vec::new());
        food.food_keep_up = food_kept_up;
        vec![linear_row(stone, None, 0, 0.0, 4.0, 1.0, 1.0, sleds), food]
    }

    #[test]
    fn a_chiefs_food_refill_is_dealt_bare_before_a_kitted_stone_refill_at_equal_priority() {
        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let rows =
            food_refill_and_stone_refill(SourcePriority::Normal, SourcePriority::Normal, true);
        let out = staff_band(&rows, 5, &mut sleds);
        assert_eq!(
            out[1].total(),
            4,
            "the food row's whole refill crew is filled"
        );
        assert_eq!(
            out[0].kitted, 1,
            "stone gets only what the food refill left"
        );

        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let rows =
            food_refill_and_stone_refill(SourcePriority::Normal, SourcePriority::Normal, false);
        let out = staff_band(&rows, 5, &mut sleds);
        assert_eq!(out[0].kitted, 3, "a player's food row keeps kitted-first");
        assert!(out[1].total() < 4, "its refill is left short");
        assert!(out[0].kitted > 0 && out[1].total() > 0, "liveness");
    }

    /// A chief's food row that takes baskets (a one-hand hold, a four-hand refill) beside a stone
    /// row that takes the same `item`, both Normal.
    fn food_and_stone_on_one_item<'a>(item: &str) -> Vec<StaffRowInput<'a>> {
        let kit: Vec<(Arc<str>, u32)> = vec![(Arc::from(item), 1)];
        let mut food = linear_row(
            SourcePriority::Normal,
            None,
            0,
            1.0,
            4.0,
            1.0,
            1.0,
            kit.clone(),
        );
        food.food_keep_up = true;
        vec![
            linear_row(SourcePriority::Normal, None, 0, 0.0, 4.0, 1.0, 1.0, kit),
            food,
        ]
    }

    /// **A CHIEF'S FOOD HEADS ARE KITTED WHERE THE SHELF CAN ARM THEM** (pass 1c's hold and pass 2's
    /// refill), bare only past the shelf; with nothing on the shelf they are bare as before.
    #[test]
    fn a_chiefs_food_heads_are_kitted_while_the_shelf_arms_them() {
        const BASKETS: u32 = 2;
        let mut rows = food_and_stone_on_one_item("baskets");
        rows.remove(0);
        let mut shelf = BTreeMap::from([(Arc::from("baskets"), BASKETS)]);
        let out = staff_band(&rows, 5, &mut shelf);
        assert_eq!(out[0].kitted, BASKETS, "every basket on the shelf is armed");
        assert!(
            out[0].bare > 0 && out[0].total() >= out[0].hold_crew,
            "bare only past the shelf: {:?}",
            out[0]
        );

        let mut empty = BTreeMap::from([(Arc::from("baskets"), 0)]);
        let out = staff_band(&rows, 5, &mut empty);
        assert_eq!(out[0].kitted, 0, "nothing on the shelf, nothing armed");
        assert!(out[0].bare > 0, "the food row is dealt bare as it was");
    }

    /// **THE FOOD REFILL'S TARGET FOLLOWS THE KIT IT DEALS**: a basket head takes twice what a bare
    /// head does, so with the shelf arming every head the reach crew is four, not the seven a
    /// target struck at one armed head would name.
    #[test]
    fn a_chiefs_food_target_is_struck_at_the_kit_the_shelf_arms() {
        const BASKETS: u32 = 10;
        const REACH: f32 = 8.0;
        const ARMED_RATE: f32 = 2.0;
        let kit: Vec<(Arc<str>, u32)> = vec![(Arc::from("baskets"), 1)];
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            1.0,
            REACH,
            ARMED_RATE,
            1.0,
            kit,
        );
        row.food_keep_up = true;
        let mut shelf = BTreeMap::from([(Arc::from("baskets"), BASKETS)]);
        let out = staff_band(&[row], 12, &mut shelf);
        let aimed = (REACH / ARMED_RATE) as u32;
        assert_eq!(out[0].total(), aimed, "no hand past the kitted reach crew");
        assert_eq!((out[0].kitted, out[0].bare), (aimed, 0));
    }

    /// **TWO ROWS ON ONE ITEM**: at equal priority the chief's food row arms first, so the stone row
    /// finds the shelf spent, and no unit is armed twice.
    #[test]
    fn a_chiefs_food_row_arms_a_shared_item_before_an_equal_stone_row() {
        const SLEDS: u32 = 2;
        let rows = food_and_stone_on_one_item("sleds");
        let mut shelf = BTreeMap::from([(Arc::from("sleds"), SLEDS)]);
        let out = staff_band(&rows, 5, &mut shelf);
        assert_eq!(out[1].kitted, SLEDS, "the food row arms both sleds");
        assert_eq!(out[0].kitted, 0, "the stone row finds none left");
        assert!(out[0].total() + out[1].total() > 0, "liveness");
    }

    #[test]
    fn a_high_stone_refill_still_goes_before_a_normal_food_refill() {
        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let rows = food_refill_and_stone_refill(SourcePriority::High, SourcePriority::Normal, true);
        let out = staff_band(&rows, 5, &mut sleds);
        assert_eq!(out[0].kitted, 3, "the High stone refill is dealt first");
        assert_eq!(out[1].total(), 2, "food gets what is left, hold crew first");
        assert!(
            out[1].total() >= out[1].hold_crew,
            "liveness: the hold is kept"
        );
    }

    #[test]
    fn a_food_hold_never_deals_bare_where_a_bare_hand_takes_nothing() {
        let mut rows = food_hold_and_stone_refill(true);
        // A hunt bare hands cannot wound: no bare head.
        rows[1] = linear_row(SourcePriority::Low, None, 0, 3.0, 3.0, 1.0, 0.0, Vec::new());
        rows[1].food_keep_up = true;
        let mut sleds = BTreeMap::from([(Arc::from("sleds"), 3)]);
        let out = staff_band(&rows, 5, &mut sleds);
        assert_eq!(out[1].bare, 0);
        assert!(out[0].kitted > 0, "liveness: stone still staffs");
    }

    /// A declared build as the staffing sees it: a bare hand always builds, the rung caps the
    /// hands, and `tile` is shared with whatever else stands there.
    fn a_build<'a>(
        priority: SourcePriority,
        rung_cap: u32,
        tile: Option<UVec2>,
    ) -> StaffRowInput<'a> {
        StaffRowInput {
            priority,
            max: Some(rung_cap),
            keep_hands: 0.0,
            hold_take: 0.0,
            useful_take: 0.0,
            reach: ReachTarget::Nothing,
            kit_items: Vec::new(),
            hands: Box::new(|_, _| Some(0.0)),
            bare_can_work: true,
            distance: 0,
            is_build: true,
            is_bench: false,
            kitless: true,
            tile,
            food_keep_up: false,
            whole_hands: false,
            sustained: None,
            food_per_take: NO_FOOD,
            finish: None,
            levy: false,
            bench_kit: None,
            kit_wants: Vec::new(),
            build_work_left: NO_HANDS,
        }
    }

    /// A bench order as a pass input: its own priority, a Max of its crew cap, the bench tool as a
    /// one-person kit and distance 0.
    fn a_bench_order<'a>(
        priority: SourcePriority,
        cap: u32,
        tool: Option<&str>,
    ) -> StaffRowInput<'a> {
        let mut row = a_build(priority, cap, None);
        row.is_build = false;
        row.is_bench = true;
        row.kitless = false;
        row.kit_items = tool
            .into_iter()
            .map(|item| (Arc::<str>::from(item), 1))
            .collect();
        row
    }

    /// **ONLY A HIGH ORDER'S FIRST CRAFTER JOINS PASS ONE**: it goes ahead of the hold crews, so a
    /// band whose pool only covers a hold crew still crews it with one; Normal and Low wait.
    #[test]
    fn only_a_high_bench_orders_first_crafter_joins_pass_one() {
        let rows = vec![
            holding(SourcePriority::Normal, 8.0, 0.0),
            a_bench_order(SourcePriority::High, 3, None),
        ];
        let out = staff_band(&rows, 2, &mut shelf(5));
        assert_eq!(out[1].total(), 1, "the High order's opener beats the hold");
        assert_eq!(out[0].total(), 1, "the hold crew gives up that hand");
        let rows = vec![
            holding(SourcePriority::Normal, 8.0, 0.0),
            a_bench_order(SourcePriority::Normal, 3, None),
        ];
        let out = staff_band(&rows, 2, &mut shelf(5));
        assert_eq!(out[0].total(), 2, "the hold crew took both hands");
        assert_eq!(out[1].total(), 0, "a Normal order waits behind a hold crew");
        let out = staff_band(&rows, 3, &mut shelf(5));
        assert_eq!(out[1].total(), 1, "liveness: a spare hand opens it");
    }

    /// **A High bench order takes the kitted crafter ahead of a Normal source's reach crew**, and its
    /// second crafter too while a Normal source's pass-2 heads wait.
    #[test]
    fn a_high_bench_order_takes_the_tool_ahead_of_a_normal_sources_reach_crew() {
        let tool: Vec<(Arc<str>, u32)> = vec![(Arc::from("loom"), 1)];
        let mut source = holding(SourcePriority::Normal, 0.0, 0.0);
        source.reach = ReachTarget::Take(16.0);
        source.kit_items = tool;
        let rows = vec![source, a_bench_order(SourcePriority::High, 2, Some("loom"))];
        let mut units: BTreeMap<Arc<str>, u32> = BTreeMap::new();
        units.insert(Arc::from("loom"), 2);
        let out = staff_band(&rows, 6, &mut units);
        assert_eq!(out[1].kitted, 2, "the High order holds both tools");
        assert_eq!(out[0].kitted, 0, "the Normal source's reach crew gets none");
    }

    /// A build with a priority, a cap and the work its climb still owes.
    fn a_build_owing<'a>(
        priority: SourcePriority,
        rung_cap: u32,
        work_left: f32,
        tile: Option<UVec2>,
    ) -> StaffRowInput<'a> {
        let mut build = a_build(priority, rung_cap, tile);
        build.build_work_left = work_left;
        build
    }

    /// **BUILDS ARE FILLED ONE AT A TIME**, priority group High to Low: the High build reaches its
    /// cap before the Low build gets a head, however few hands there are.
    #[test]
    fn a_high_build_fills_before_a_normal_build_starts() {
        const RUNG_CAP: u32 = 3;
        let rows = vec![
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 1.0, None),
            a_build_owing(SourcePriority::High, RUNG_CAP, 50.0, None),
        ];
        let out = staff_band(&rows, 2, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (0, 2));
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (1, RUNG_CAP));
    }

    /// **TWO NORMAL BUILDS, HANDS FOR FOUR: 3 AND 1; HANDS FOR THREE: 3 AND 0.**
    #[test]
    fn two_builds_fill_the_closer_one_to_its_cap_then_the_next() {
        const RUNG_CAP: u32 = 3;
        let rows = vec![
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 10.0, None),
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 10.0, None),
        ];
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (3, 1));
        let out = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (3, 0));
    }

    /// **THE BUILD CLOSEST TO DONE FILLS FIRST, WHATEVER ITS ROW INDEX**, and a tie in work goes to
    /// the nearer site, then the earlier row.
    #[test]
    fn the_build_closest_to_done_fills_first_regardless_of_row_order() {
        const RUNG_CAP: u32 = 3;
        let rows = vec![
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 40.0, None),
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 6.0, None),
        ];
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (1, RUNG_CAP));
        let mut far = a_build_owing(SourcePriority::Normal, RUNG_CAP, 6.0, None);
        far.distance = 5;
        let mut near = a_build_owing(SourcePriority::Normal, RUNG_CAP, 6.0, None);
        near.distance = 1;
        let rows = vec![far, near];
        let out = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (0, RUNG_CAP));
    }

    /// **A BUILD CAPPED BY ITS TILE'S ROOM LETS THE NEXT START**, and a build that can take nobody
    /// never holds the line.
    #[test]
    fn a_build_capped_by_tile_room_lets_the_next_one_start() {
        const RUNG_CAP: u32 = 4;
        const TILE_ROOM: u32 = 2;
        let site = UVec2::new(4, 7);
        let rows = vec![
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 1.0, Some(site)),
            a_build_owing(SourcePriority::Normal, RUNG_CAP, 2.0, None),
        ];
        let room = BTreeMap::from([(tile_key(site), TILE_ROOM)]);
        let out = staff_passes(&rows, 5, &mut shelf(0), Passes::All, &room);
        assert_eq!((out[0].total(), out[1].total()), (TILE_ROOM, 3));
    }

    /// **A RUNG CAPS ITS BUILDERS** however many hands the band has idle.
    #[test]
    fn a_rung_never_takes_more_builders_than_its_cap() {
        const RUNG_CAP: u32 = 2;
        let rows = vec![a_build(SourcePriority::Normal, RUNG_CAP, None)];
        let out = staff_band(&rows, 20, &mut shelf(0));
        assert_eq!(out[0].total(), RUNG_CAP);
    }

    /// A band of three: one bare-handed food row (a take of one food a unit, 100 of room, one
    /// regrowing a turn) and one build that would take every hand. `gain` is what the build adds
    /// to the larder a turn once it finishes (in one turn, at three builders).
    fn feeding_band<'a>(gain: Option<f32>) -> Vec<StaffRowInput<'a>> {
        let mut food = linear_row(
            SourcePriority::Normal,
            None,
            0,
            1.0,
            100.0,
            1.0,
            1.0,
            Vec::new(),
        );
        food.useful_take = 100.0;
        food.food_per_take = 1.0;
        let mut build = a_build(SourcePriority::Normal, 3, None);
        build.finish = gain.map(|gain| BuildFinish {
            work_remaining: 3.0,
            work_per_builder: 1.0,
            gain,
        });
        vec![food, build]
    }

    const FOOD_FIRST_WINDOW: u32 = 5;

    fn food_first(rows: &[StaffRowInput<'_>], larder: f32) -> (Vec<RowStaffing>, bool) {
        let (out, _, fed_first) = staff_band_food_first(
            rows,
            3,
            &mut shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder,
                need: 1.0,
                standing: 0.0,
            },
            FOOD_FIRST_WINDOW,
        );
        (out, fed_first)
    }

    /// **A BAND THAT WOULD STARVE WITH ITS BUILDER WORKING IS RE-STAFFED BUILDS LAST**: builds
    /// first gives the build all three hands and the food row none; the larder (2, need 1) empties
    /// inside the window, so every hand goes to food and the build waits.
    #[test]
    fn a_band_that_would_starve_is_restaffed_builds_last() {
        let rows = feeding_band(None);
        let (out, fed_first) = food_first(&rows, 2.0);
        assert!(fed_first);
        assert_eq!((out[0].total(), out[1].total()), (3, 0));
        // The control: the same rows at the ordinary order give the build everything.
        let plain = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!(
            plain[0].total(),
            0,
            "builds first leaves the food row nobody"
        );
    }

    /// **THE BUILDS-LAST DEAL FILLS ONE BUILD AT A TIME TOO**: the food row (Max 1) takes its hand,
    /// and the builds get what is left - the closer build to its cap, the other nothing.
    #[test]
    fn the_food_first_builds_last_deal_respects_the_fill_gate() {
        let mut rows = feeding_band(None);
        rows[0].max = Some(1);
        rows[1].build_work_left = 2.0;
        rows.push(a_build_owing(SourcePriority::Normal, 3, 9.0, None));
        let (out, _, fed_first) = staff_band_food_first(
            &rows,
            4,
            &mut shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder: 2.0,
                need: 1.0,
                standing: 0.0,
            },
            FOOD_FIRST_WINDOW,
        );
        assert!(fed_first, "liveness: the builds-last deal is the one run");
        assert_eq!(crews(&out), vec![1, 3, 0]);
    }

    /// **A BAND THAT IS SHORT BUT SURVIVES THE WINDOW KEEPS ITS BUILDER.**
    #[test]
    fn a_band_that_survives_the_window_keeps_its_builder() {
        let rows = feeding_band(None);
        let (out, fed_first) = food_first(&rows, 100.0);
        assert!(!fed_first);
        assert_eq!((out[0].total(), out[1].total()), (0, 3));
    }

    /// **A BUILD FINISHING INSIDE THE WINDOW COUNTS ITS GAIN**: a larder of 3 against a need of 1
    /// starves on the fourth meal without the build's finished yield and does not with it.
    #[test]
    fn a_build_finishing_in_the_window_counts_its_gain() {
        let (without, fed_without) = food_first(&feeding_band(None), 3.0);
        assert!(fed_without);
        assert_eq!(without[1].total(), 0);
        let (with, fed_with) = food_first(&feeding_band(Some(2.0)), 3.0);
        assert!(!fed_with);
        assert_eq!(with[1].total(), 3, "the finishing build keeps its builders");
    }

    /// A hunt-shaped row (whole crews, the Goosefen warren): one hunter takes `ONE_HUNTER_TAKE`, and
    /// the site's keeping is a sliver of a hand.
    fn a_warren_row<'a>(keeping: f32) -> StaffRowInput<'a> {
        const ONE_HUNTER_TAKE: f32 = 2.0;
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            ONE_HUNTER_TAKE / 2.0,
            // The room the stand can give is under one hunter's full take, so the keeping's sliver
            // rides in his slack.
            ONE_HUNTER_TAKE * 0.75,
            1.0,
            1.0,
            Vec::new(),
        );
        row.keep_hands = keeping;
        row.whole_hands = true;
        row.food_per_take = 1.0;
        row.hands = Box::new(|take, _| Some((take / ONE_HUNTER_TAKE).ceil().max(1.0)));
        row
    }

    /// **A PEN'S HOLD CREW TAKES THE REGROWTH AT THE FLOOR, NOT THIS TURN'S ROOM.** One keeper whose
    /// leftover reach (after the keeping's 0.54 of a hand) cannot take what the pen regrows each turn
    /// is not a hold crew: the keeping rides in his slack only where the slack reaches the regrowth.
    /// The control: a pen whose regrowth the slack does cover stays at one head, and a row with no
    /// sustained take is sized on its own hold take as before.
    #[test]
    fn a_pens_hold_crew_is_sized_on_the_regrowth_its_slack_must_reach() {
        const KEEPING: f32 = 0.542;
        const ONE_HUNTER_REACH: f32 = 80.0;
        const ROOM: f32 = 22.0;
        const REGROWTH_ONE_SLACK_CANNOT_TAKE: f32 = 46.6;
        const REGROWTH_ONE_SLACK_COVERS: f32 = 30.0;
        let reach = |bound: f32| -> Box<dyn Fn(f32, u32) -> Option<f32>> {
            Box::new(move |take, _| Some((take.min(bound) / ONE_HUNTER_REACH).ceil().max(1.0)))
        };
        let pen = |regrowth: f32| {
            let mut row = a_warren_row(KEEPING);
            row.hold_take = ROOM;
            row.hands = reach(ROOM);
            row.sustained = Some(SustainedTake {
                take: regrowth,
                hands: reach(f32::INFINITY),
            });
            row
        };
        assert_eq!(
            hold_heads(&pen(REGROWTH_ONE_SLACK_CANNOT_TAKE), 0),
            2,
            "one keeper's slack cannot take the regrowth: the keeping gets a head of its own"
        );
        assert_eq!(
            hold_heads(&pen(REGROWTH_ONE_SLACK_COVERS), 0),
            1,
            "the control: the slack covers the regrowth, so one head holds it"
        );
        let mut roomed = pen(REGROWTH_ONE_SLACK_CANNOT_TAKE);
        roomed.sustained = None;
        assert_eq!(
            hold_heads(&roomed, 0),
            1,
            "with no sustained take the row is sized on its own hold take"
        );
        // The reach and useful crews never fall under the hold crew.
        let row = pen(REGROWTH_ONE_SLACK_CANNOT_TAKE);
        assert!(reach_heads(&row, 0) >= hold_heads(&row, 0));
        assert!(useful_heads(&row, 0) >= hold_heads(&row, 0));
    }

    /// **A HUNT'S TAKE COUNTS IN THE FOOD-FIRST WALK THOUGH ITS KEEPING IS A SLIVER OF A HAND** (the
    /// Goosefen case: one rabbit hunter, 0.005 keeping). The row's `hands` closure answers WHOLE
    /// crews, so pricing the crew at `1 - 0.005` fractional hands made every target need more than the
    /// crew had and the hunt read as no food at all - food-first fired and the builder was pulled.
    #[test]
    fn a_hunt_with_a_sliver_of_keeping_counts_its_take_and_the_builder_stays() {
        const KEEPING: f32 = 0.005;
        const LARDER: f32 = 2.0;
        let rows = vec![
            a_warren_row(KEEPING),
            a_build(SourcePriority::Normal, 1, None),
        ];
        let (out, _, fed_first) = staff_band_food_first(
            &rows,
            2,
            &mut shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder: LARDER,
                need: 1.0,
                standing: 0.0,
            },
            FOOD_FIRST_WINDOW,
        );
        assert_eq!(out[0].total(), 1, "liveness: the hunter is staffed");
        assert_eq!(out[1].total(), 1, "the builder is staffed beside him");
        assert!(
            !fed_first,
            "one hunter's take covers the meal, so the band does not feed itself first"
        );
        // The control: no hunt row, the same larder and meal: the band does run short.
        let bare = vec![a_build(SourcePriority::Normal, 1, None)];
        let (_, _, short) = staff_band_food_first(
            &bare,
            2,
            &mut shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder: LARDER,
                need: 1.0,
                standing: 0.0,
            },
            FOOD_FIRST_WINDOW,
        );
        assert!(
            !short || bare.len() == 1,
            "no food row: nothing to feed from"
        );
    }

    /// **A HERD BUILD FINISHING INSIDE THE WINDOW KEEPS ITS BUILDER** (the Goosefen case: a warren
    /// going to Pastoral). The gain is the finished rung's sustained food above the held rung's,
    /// through the same `herd_msy_biomass_at` the forecast and `herd_build_finish` read.
    #[test]
    fn a_herd_build_finishing_in_the_window_keeps_its_builder() {
        let fauna = crate::fauna_config::FaunaConfig::builtin();
        let mut herd = crate::fauna::Herd::new(
            "game_warren".to_string(),
            "Rabbit Warren".to_string(),
            crate::fauna_config::SizeClass::Small,
            vec![UVec2::new(1, 1)],
            1000.0,
            1000.0,
            0.06,
            0.35,
            0.27,
            crate::shipped::root_standing(crate::shipped::animal()),
        );
        herd.husbandry_ceiling = Some(crate::shipped::animal_pen());
        let per_biomass = crate::fauna::herd_hunt_yield(&herd, &fauna).provisions_per_biomass;
        let msy = |payoff| crate::fauna::herd_msy_biomass_at(&herd, &fauna, payoff);
        let gain = (msy(crate::fauna::HerdPayoff::Pastoral) - msy(crate::fauna::HerdPayoff::Wild))
            * per_biomass;
        assert!(gain > 0.0, "liveness: taming must add food");
        // Scale the herd's gain so the larder (3 against a need of 1) starves without it.
        let scaled = gain.max(2.0) * 10.0;
        let (without, fed_without) = food_first(&feeding_band(None), 3.0);
        assert!(fed_without && without[1].total() == 0);
        let (with, fed_with) = food_first(&feeding_band(Some(scaled)), 3.0);
        assert!(!fed_with);
        assert_eq!(with[1].total(), 3);
    }

    /// **POOLING AND NEED ROWS ARE INCOME**: a band fed by its network (a standing net above its
    /// meal) does not read as starving, and keeps its builder.
    #[test]
    fn a_band_fed_by_its_network_keeps_its_builder() {
        let rows = feeding_band(None);
        let (out, _, fed_first) = staff_band_food_first(
            &rows,
            3,
            &mut shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder: 2.0,
                need: 1.0,
                standing: 1.0,
            },
            FOOD_FIRST_WINDOW,
        );
        assert!(!fed_first);
        assert_eq!(out[1].total(), 3);
    }

    /// **STABILITY**: the same state staffed twice gives the same answer.
    #[test]
    fn food_first_is_stable() {
        let rows = feeding_band(None);
        let first = food_first(&rows, 2.0);
        let second = food_first(&rows, 2.0);
        assert_eq!(crews(&first.0), crews(&second.0));
        assert_eq!(first.1, second.1);
    }

    /// **A BUILD THAT CANNOT PROGRESS TAKES NOBODY** (its cap is stated as zero).
    #[test]
    fn a_build_that_cannot_progress_takes_nobody() {
        let rows = vec![
            a_build(SourcePriority::High, 0, None),
            a_build(SourcePriority::Low, 2, None),
        ];
        let out = staff_band(&rows, 10, &mut shelf(0));
        assert_eq!((out[0].total(), out[1].total()), (0, 2));
    }

    /// **A TILE CAPS EVERYONE ON IT**: two builds on one tile share its room.
    #[test]
    fn two_builds_on_one_tile_share_its_room() {
        const TILE_ROOM: u32 = 3;
        let site = UVec2::new(4, 7);
        let rows = vec![
            a_build(SourcePriority::Normal, 4, Some(site)),
            a_build(SourcePriority::Normal, 4, Some(site)),
        ];
        let room = BTreeMap::from([(tile_key(site), TILE_ROOM)]);
        let out = staff_passes(&rows, 20, &mut shelf(0), Passes::All, &room);
        assert_eq!(
            (out[0].total(), out[1].total()),
            (TILE_ROOM, 0),
            "the first build takes the whole room and the second has none to start in"
        );
    }

    /// **A ROW'S CREW TARGETS NEVER EXCEED WHAT ITS TILE CAN HOLD**, and `tile_short` is the excess
    /// the room cut off.
    #[test]
    fn a_rows_targets_are_capped_at_its_tile_room_and_tile_short_is_the_excess() {
        const TILE_ROOM: u32 = 3;
        let site = UVec2::new(4, 7);
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            8.0,
            8.0,
            1.0,
            1.0,
            spears(),
        );
        row.tile = Some(site);
        let rows = vec![row];
        let uncapped = staff_band(&rows, 20, &mut shelf(10));
        assert!(
            uncapped[0].reach_crew > TILE_ROOM,
            "the fixture must aim past the room"
        );
        assert_eq!(uncapped[0].tile_short, 0, "no room entry, nothing cut");
        let room = BTreeMap::from([(tile_key(site), TILE_ROOM)]);
        let out = staff_passes(&rows, 20, &mut shelf(10), Passes::All, &room);
        assert!(out[0].reach_crew <= TILE_ROOM && out[0].useful_crew <= TILE_ROOM);
        assert!(out[0].hold_crew <= out[0].reach_crew);
        assert_eq!(
            out[0].tile_short,
            uncapped[0].hold_crew.max(uncapped[0].reach_crew) - out[0].reach_crew
        );
        assert!(out[0].tile_short > 0);
    }

    /// **WITHIN A PRIORITY GROUP A BUILD IS DEALT BEFORE A SOURCE REACHES TOWARD ITS FLOOR.**
    #[test]
    fn a_build_is_dealt_before_a_sources_reach_in_its_group() {
        const RUNG_CAP: u32 = 3;
        let mut source = linear_row(
            SourcePriority::Normal,
            None,
            0,
            0.0,
            40.0,
            4.0,
            1.0,
            Vec::new(),
        );
        source.useful_take = 40.0;
        let rows = vec![source, a_build(SourcePriority::Normal, RUNG_CAP, None)];
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!(out[1].total(), RUNG_CAP, "the build fills to its cap first");
        assert_eq!(out[0].total(), 1, "the source takes what the build left");
    }

    /// A row holding `hold` take per turn, one kitted hand taking `RATE` of it, bare hands 1.
    fn holding<'a>(priority: SourcePriority, hold: f32, keep_hands: f32) -> StaffRowInput<'a> {
        const RATE: f32 = 4.0;
        let mut row = linear_row(priority, None, 0, hold, hold, RATE, 1.0, spears());
        row.keep_hands = keep_hands;
        row
    }

    /// **A HOLD CREW IS A FOOD TAKE, NOT ROT PROTECTION**: a High build gets its builder ahead of a
    /// Normal row's hold crew, and the keeping's fraction no longer costs a whole extra head.
    #[test]
    fn a_high_build_gets_its_builder_before_a_normal_rows_hold_crew() {
        const POOL: u32 = 6;
        // The High forage row: 0.043 hands of keeping and 1.4 hands of gathering (5.6 at 4 a hand)
        // round ONCE to two heads; the Normal hunt holds four heads but only three kits are left.
        let rows = vec![
            holding(SourcePriority::High, 5.6, 0.043),
            holding(SourcePriority::Normal, 16.0, 0.0),
            a_build(SourcePriority::High, 3, None),
        ];
        let out = staff_band(&rows, POOL, &mut shelf(5));
        assert_eq!(
            (out[0].keepers, out[0].kitted + out[0].bare),
            (0, 2),
            "the fraction of a hand of keeping rides inside a gatherer's head"
        );
        assert_eq!(out[2].total(), 1, "the High build's builder comes next");
        assert_eq!(out[1].total(), 3, "the Normal hold crew takes what is left");
    }

    /// A Normal build does NOT beat a High row's hold crew; a Low build beats the Low hold crews but
    /// not the High and Normal ones.
    #[test]
    fn a_build_ranks_by_its_groups_priority_against_hold_crews() {
        let rows = vec![
            holding(SourcePriority::High, 12.0, 0.0),
            a_build(SourcePriority::Normal, 2, None),
        ];
        let out = staff_band(&rows, 3, &mut shelf(9));
        assert_eq!((out[0].total(), out[1].total()), (3, 0));

        let rows = vec![
            holding(SourcePriority::High, 4.0, 0.0),
            holding(SourcePriority::Normal, 4.0, 0.0),
            a_build(SourcePriority::Low, 2, None),
            holding(SourcePriority::Low, 16.0, 0.0),
        ];
        let out = staff_band(&rows, 3, &mut shelf(9));
        assert_eq!(
            crews(&out),
            vec![1, 1, 1, 0],
            "the Low build beats the Low hold crew, behind the High and Normal ones"
        );
    }

    /// An over-committed band: an empty Roadwork row, a forage row and a road build, the last two
    /// holding two heads, at the given Roadwork and forage Priorities. Trims one hand.
    fn trim_one_with(roadwork: SourcePriority, forage: SourcePriority) -> StaffedBand {
        const HANDS: u32 = 2;
        let forage_target = LaborTarget::Forage {
            tile: UVec2::new(1, 1),
            floor: 0.5,
            species: None,
            take_species: Default::default(),
        };
        let mut allocation = LaborAllocation::default();
        assert!(allocation.set_staffed_row(LaborTarget::Roadwork, None, None));
        assert!(allocation.set_staffed_row(forage_target.clone(), None, None));
        allocation.set_source_priority(&LaborTarget::Roadwork, roadwork);
        allocation.set_source_priority(&forage_target, forage);
        let crew = StaffedCrew {
            bare: HANDS,
            ..StaffedCrew::default()
        };
        let mut staffed = StaffedBand {
            bench: Vec::new(),
            first_call: Default::default(),
            hold_losses: Vec::new(),
            rows: vec![
                (LaborTarget::Roadwork, StaffedCrew::default()),
                (forage_target, crew),
            ],
            builds: vec![(
                BuildSource::Road(UVec2::new(3, 3)),
                BuildCrew {
                    kitted: 0,
                    bare: HANDS,
                    cap: HANDS,
                    fed_first: false,
                },
            )],
            scout: ScoutCrew::default(),
        };
        assert!(trim_one_hand(&allocation, &[], &mut staffed));
        staffed
    }

    /// **A ROAD BUILD TRIMS AT THE ROADWORK ROW'S PRIORITY**, not as Normal: with Roadwork High the
    /// Normal forage row gives the hand and the road builders keep theirs.
    #[test]
    fn a_high_roadwork_build_is_trimmed_after_a_normal_row() {
        let staffed = trim_one_with(SourcePriority::High, SourcePriority::Normal);
        assert_eq!(staffed.rows[1].1.total(), 1, "the Normal row gave the hand");
        assert_eq!(
            staffed.builds[0].1.bare, 2,
            "the High road keeps its builders"
        );
    }

    /// The mirror: with Roadwork Low, the road build gives the hand before the Normal row does.
    #[test]
    fn a_low_roadwork_build_is_trimmed_before_a_normal_row() {
        let staffed = trim_one_with(SourcePriority::Low, SourcePriority::Normal);
        assert_eq!(
            staffed.builds[0].1.bare, 1,
            "the Low road build gave the hand"
        );
        assert_eq!(staffed.rows[1].1.total(), 2);
    }

    /// At equal priority the rows' surplus goes before a build's builders.
    #[test]
    fn at_equal_priority_a_row_is_trimmed_before_the_build() {
        let staffed = trim_one_with(SourcePriority::Normal, SourcePriority::Normal);
        assert_eq!(staffed.builds[0].1.bare, 2, "the build keeps its builders");
        assert_eq!(staffed.rows[1].1.total(), 1, "the row gave the hand");
    }

    /// **THE KEEPING AND THE GATHERING ROUND ONCE**: 1.6 hands of keeping and 0.5 of gathering are
    /// ceil(2.1) = 3 heads, one of them a whole keeper.
    #[test]
    fn keeping_and_gathering_round_once() {
        let rows = vec![holding(SourcePriority::Normal, 2.0, 1.6)];
        let out = staff_band(&rows, 20, &mut shelf(10));
        assert_eq!(out[0].total(), 3);
        assert_eq!(out[0].keepers, 1);
        assert_eq!(out[0].keepers + out[0].kitted + out[0].bare, out[0].total());
        assert_eq!(out[0].hold_crew, 3);
    }

    /// A hunt-like row: its take hands arrive as a WHOLE crew (`ceil(target / rate)`), kitted with
    /// `item`, and no bare hand can work it.
    fn hunt_like<'a>(item: &str, take: f32, keep_hands: f32) -> StaffRowInput<'a> {
        const RATE: f32 = 0.6;
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            take,
            take,
            RATE,
            0.0,
            vec![(Arc::from(item), 1)],
        );
        row.hands = Box::new(|target, _| Some((target / RATE).ceil()));
        row.keep_hands = keep_hands;
        row.whole_hands = true;
        row
    }

    /// **A WHOLE HUNT CREW CARRIES THE KEEPING'S FRACTION IN ITS SLACK**: one hunter's take plus
    /// 0.0046 of a hand of keeping is one head, not `ceil(1.0046)` = two.
    #[test]
    fn a_whole_hunt_crew_carries_the_keeping_fraction_in_its_slack() {
        let rows = vec![hunt_like("traps", 0.5, 0.0046)];
        let mut units = BTreeMap::from([(Arc::from("traps"), 5)]);
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(out[0].total(), 1);
        assert_eq!(out[0].hold_crew, 1);
        assert_eq!(out[0].reach_crew, 1);
        assert_eq!(out[0].useful_crew, 1);
        // Whole keepers still come as heads of their own: 1.5 keepers + 1 hunter = 2 heads.
        let rows = vec![hunt_like("traps", 0.2, 1.5)];
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!((out[0].keepers, out[0].total()), (1, 2));
    }

    /// **THE FRACTION RIDES ONLY WHERE IT FITS**: a need just under a whole head leaves too little
    /// slack for half a hand of keeping, so the row gets the extra head.
    #[test]
    fn keeping_that_does_not_fit_the_slack_adds_a_head() {
        // 0.58 at 0.6 a hand is 0.97 of a hand: one hunter, 0.03 of slack.
        let rows = vec![hunt_like("traps", 0.58, 0.5)];
        let mut units = BTreeMap::from([(Arc::from("traps"), 5)]);
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(out[0].total(), 2);
        // 0.3 at 0.6 a hand is half a hand: the half a hand of keeping fits beside it.
        let rows = vec![hunt_like("traps", 0.3, 0.5)];
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(out[0].total(), 1);
    }

    /// **GOOSEFEN**: three workers, five baskets, two traps. The rabbit row's tiny keeping must not
    /// take a second hunter off the forage row's hands.
    #[test]
    fn a_tame_warrens_keeping_does_not_take_the_last_worker_from_food() {
        let mut forage = linear_row(
            SourcePriority::Normal,
            None,
            0,
            0.2,
            0.2,
            0.29,
            0.29,
            vec![(Arc::from("baskets"), 1)],
        );
        forage.useful_take = 3.0;
        // A second forage row held at its floor: nothing to take, nothing to keep.
        let held = linear_row(
            SourcePriority::Normal,
            None,
            0,
            0.0,
            0.0,
            0.29,
            0.29,
            vec![(Arc::from("baskets"), 1)],
        );
        let rows = vec![forage, held, hunt_like("traps", 0.5184, 0.0046)];
        let mut units = BTreeMap::from([(Arc::from("traps"), 2), (Arc::from("baskets"), 5)]);
        let out = staff_band(&rows, 3, &mut units);
        assert_eq!(out[0].total(), 2, "the forage row gets the other two hands");
        assert_eq!(out[2].total(), 1, "one rabbit hunter");
    }

    /// A row with nothing to take still keeps its improvement with a whole head.
    #[test]
    fn a_keeping_only_row_gets_a_whole_keeper_for_a_fraction() {
        let rows = vec![holding(SourcePriority::Normal, 0.0, 0.3)];
        let out = staff_band(&rows, 20, &mut shelf(10));
        assert_eq!((out[0].keepers, out[0].kitted, out[0].bare), (1, 0, 0));
    }

    #[test]
    fn the_max_caps_every_pass() {
        let rows = vec![linear_row(
            SourcePriority::Normal,
            Some(2),
            1,
            8.0,
            8.0,
            4.0,
            1.0,
            spears(),
        )];
        let mut units = shelf(5);
        let out = staff_band(&rows, 20, &mut units);
        assert_eq!(out[0].total(), 2);
        assert_eq!(out[0].keepers, 1);
        assert!(out[0].hold_crew <= 2 && out[0].reach_crew <= 2);
    }

    #[test]
    fn a_row_below_its_floor_gets_keepers_only() {
        let rows = vec![linear_row(
            SourcePriority::Normal,
            None,
            2,
            0.0,
            0.0,
            4.0,
            1.0,
            spears(),
        )];
        let mut units = shelf(5);
        let out = staff_band(&rows, 20, &mut units);
        assert_eq!((out[0].keepers, out[0].kitted, out[0].bare), (2, 0, 0));
    }

    #[test]
    fn a_hand_that_cannot_help_is_never_added() {
        // The quarry cannot be wounded bare-handed (deer): no bare heads, however large the pool.
        let mut row = linear_row(
            SourcePriority::Normal,
            None,
            0,
            4.0,
            4.0,
            4.0,
            0.0,
            spears(),
        );
        row.bare_can_work = false;
        let mut units = shelf(0);
        let out = staff_band(&[row], 50, &mut units);
        assert_eq!(out[0].bare, 0);
    }

    #[test]
    fn idle_is_what_the_passes_leave() {
        let rows = vec![linear_row(
            SourcePriority::Normal,
            None,
            0,
            2.0,
            2.0,
            1.0,
            1.0,
            Vec::new(),
        )];
        let mut units = shelf(0);
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(out[0].total(), 2);
        assert_eq!(10 - out[0].total(), 8, "the rest of the pool is idle");
    }

    #[test]
    fn the_pool_bounds_the_staffing_and_keepers_come_first() {
        let rows = vec![linear_row(
            SourcePriority::Normal,
            None,
            3,
            9.0,
            9.0,
            1.0,
            1.0,
            Vec::new(),
        )];
        let mut units = shelf(0);
        let out = staff_band(&rows, 4, &mut units);
        assert_eq!(out[0].keepers, 3);
        assert_eq!(out[0].total(), 4);
    }

    #[test]
    fn a_mined_seam_with_no_max_gets_nobody_and_with_a_max_gets_the_max() {
        let mut none = linear_row(
            SourcePriority::Normal,
            None,
            0,
            0.0,
            0.0,
            1.0,
            1.0,
            Vec::new(),
        );
        none.reach = ReachTarget::Heads(0);
        let mut capped = linear_row(
            SourcePriority::Normal,
            Some(3),
            0,
            0.0,
            0.0,
            1.0,
            1.0,
            Vec::new(),
        );
        capped.reach = ReachTarget::Heads(3);
        let mut units = shelf(0);
        let out = staff_band(&[none, capped], 10, &mut units);
        assert_eq!(out[0].total(), 0);
        assert_eq!(out[1].total(), 3);
    }

    #[test]
    fn equal_priority_bare_rows_split_the_heads_evenly() {
        let rows = vec![
            at(bare_row(SourcePriority::Normal, 8.0), 3),
            at(bare_row(SourcePriority::Normal, 8.0), 3),
        ];
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!(crews(&out), vec![2, 2]);
    }

    #[test]
    fn the_odd_head_goes_to_the_nearer_row_whatever_the_row_order() {
        // The far row is first in the input, so row index alone would hand it the odd head.
        let rows = vec![
            at(bare_row(SourcePriority::Normal, 8.0), 9),
            at(bare_row(SourcePriority::Normal, 8.0), 2),
        ];
        let out = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!(crews(&out), vec![1, 2], "the nearer row (index 1) gets two");
    }

    #[test]
    fn equal_distance_ties_go_to_the_earlier_row() {
        let rows = vec![
            at(bare_row(SourcePriority::Normal, 8.0), 4),
            at(bare_row(SourcePriority::Normal, 8.0), 4),
        ];
        let out = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!(crews(&out), vec![2, 1]);
    }

    #[test]
    fn a_max_on_one_row_hands_the_rest_to_the_other() {
        let mut capped = bare_row(SourcePriority::Normal, 8.0);
        capped.max = Some(1);
        let rows = vec![at(capped, 1), at(bare_row(SourcePriority::Normal, 8.0), 5)];
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!(crews(&out), vec![1, 3]);
    }

    #[test]
    fn a_high_row_fills_before_the_normal_rows_get_anything() {
        let rows = vec![
            at(bare_row(SourcePriority::Normal, 8.0), 1),
            at(bare_row(SourcePriority::High, 3.0), 7),
            at(bare_row(SourcePriority::Normal, 8.0), 2),
        ];
        let out = staff_band(&rows, 7, &mut shelf(0));
        assert_eq!(
            crews(&out),
            vec![2, 3, 2],
            "High takes its useful 3, the Normal pair splits the remaining 4"
        );
    }

    #[test]
    fn kitted_rows_share_the_shelf_one_unit_at_a_time() {
        // Two spears, two equal rows that would each use both: the shelf is split, not drained by one.
        let row = |distance| {
            at(
                linear_row(
                    SourcePriority::Normal,
                    None,
                    0,
                    8.0,
                    8.0,
                    4.0,
                    0.0,
                    spears(),
                ),
                distance,
            )
        };
        let rows = vec![row(6), row(1)];
        let mut units = shelf(2);
        let out = staff_band(&rows, 10, &mut units);
        assert_eq!(
            out.iter().map(|crew| crew.kitted).collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert_eq!(units[&Arc::<str>::from("spears")], 0);
    }

    #[test]
    fn a_far_row_is_dealt_every_round_just_after_the_nearer_ones() {
        let rows = vec![
            at(bare_row(SourcePriority::Normal, 8.0), 40),
            at(bare_row(SourcePriority::Normal, 8.0), 1),
            at(bare_row(SourcePriority::Normal, 8.0), 2),
        ];
        let out = staff_band(&rows, 6, &mut shelf(0));
        assert_eq!(crews(&out), vec![2, 2, 2]);
        let out = staff_band(&rows, 4, &mut shelf(0));
        assert_eq!(
            crews(&out),
            vec![1, 2, 1],
            "round two reaches the nearest row only"
        );
    }

    #[test]
    fn keepers_are_dealt_round_robin_before_the_hold_heads() {
        let keeper_row = |distance| {
            at(
                linear_row(
                    SourcePriority::Normal,
                    None,
                    2,
                    8.0,
                    8.0,
                    1.0,
                    1.0,
                    Vec::new(),
                ),
                distance,
            )
        };
        let rows = vec![keeper_row(5), keeper_row(1)];
        let out = staff_band(&rows, 3, &mut shelf(0));
        assert_eq!(
            out.iter().map(|crew| crew.keepers).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    /// Tiles one kitted scout adds in the scout tests (the equipped vantage tier).
    const KITTED_TILES: u32 = 2;
    /// Tiles one bare scout adds (the bare vantage tier).
    const BARE_TILES: u32 = 1;

    fn wayfinding() -> Vec<(Arc<str>, u32)> {
        vec![(Arc::from("wayfinding"), 1)]
    }

    fn wayfinding_shelf(units: u32) -> BTreeMap<Arc<str>, u32> {
        BTreeMap::from([(Arc::from("wayfinding"), units)])
    }

    fn scouting(range: u32, kit_items: Vec<(Arc<str>, u32)>) -> ScoutPlan {
        ScoutPlan {
            range,
            kitted_tiles: KITTED_TILES,
            bare_tiles: BARE_TILES,
            kit_items,
            levy: None,
            stretch: crate::components::UNSTRETCHED,
        }
    }

    fn scout_step(plan: &ScoutPlan, pool: u32, units: u32) -> ScoutCrew {
        let mut pool_left = pool;
        deal_scouts(plan, &mut pool_left, None, &mut wayfinding_shelf(units))
    }

    #[test]
    fn kitted_scouts_are_placed_before_bare_ones() {
        // Range 5 at 2 tiles a kitted scout wants three kitted; the shelf arms two (4 tiles) and one
        // bare scout covers the last tile.
        let crew = scout_step(&scouting(5, wayfinding()), 10, 2);
        assert_eq!((crew.kitted, crew.bare, crew.reach), (2, 1, 5));
    }

    #[test]
    fn the_reach_is_every_tile_the_scouts_see() {
        // Range 1 wants one kitted scout, whose 2-tile sight reaches 2, not the 1 asked for.
        let crew = scout_step(&scouting(1, wayfinding()), 10, 1);
        assert_eq!((crew.kitted, crew.bare, crew.reach), (1, 0, 2));
    }

    #[test]
    fn a_kit_of_none_places_only_bare_scouts() {
        let crew = scout_step(&scouting(4, Vec::new()), 10, 5);
        assert_eq!((crew.kitted, crew.bare, crew.reach), (0, 4, 4));
    }

    #[test]
    fn a_range_of_nothing_places_nobody() {
        let crew = scout_step(&scouting(0, wayfinding()), 10, 5);
        assert_eq!(crew, ScoutCrew::default());
    }

    #[test]
    fn the_reach_comes_up_short_when_people_run_out() {
        // Range 9, three people, one wayfinding kit: one kitted (2 tiles) + two bare (2 tiles).
        let crew = scout_step(&scouting(9, wayfinding()), 3, 1);
        assert_eq!((crew.kitted, crew.bare, crew.reach), (1, 2, 4));
    }

    #[test]
    fn a_tier_that_rounds_to_no_tiles_contributes_nobody() {
        let mut plan = scouting(3, wayfinding());
        plan.kitted_tiles = 0;
        plan.bare_tiles = 0;
        assert_eq!(scout_step(&plan, 10, 5), ScoutCrew::default());
    }

    fn kitted_hold_and_reach_row<'a>() -> StaffRowInput<'a> {
        // Hold crew 2, reach crew 6, one take hand per kitted head.
        linear_row(
            SourcePriority::Normal,
            None,
            0,
            2.0,
            6.0,
            1.0,
            0.0,
            spears(),
        )
    }

    fn staffed_with_scouts(pool: u32, range: u32) -> (Vec<RowStaffing>, ScoutCrew) {
        let rows = vec![kitted_hold_and_reach_row()];
        staff_passes_scouting(
            &rows,
            pool,
            &mut shelf(10),
            Passes::All,
            &BTreeMap::new(),
            &scouting(range, Vec::new()),
        )
    }

    #[test]
    fn a_short_band_still_staffs_the_full_range_and_the_hold_crew_goes_short() {
        // Two people, range 2 (two bare scouts): the scouts come off the top, so the hold crew
        // (which would have taken both) gets nobody.
        let (rows, scouts) = staffed_with_scouts(2, 2);
        assert_eq!((scouts.bare, scouts.reach), (2, 2));
        assert_eq!(rows[0].kitted, 0);
        let (rows, scouts) = staffed_with_scouts(2, 0);
        assert_eq!(scouts.total(), 0);
        assert_eq!(
            rows[0].kitted, 2,
            "liveness: without scouts the hold crew staffs"
        );
    }

    #[test]
    fn scouts_are_placed_before_the_keepers() {
        // A keeping-only row (two whole keepers) in a band of three with range 2: the scouts take
        // two people and the keepers get the one that is left.
        let rows = vec![linear_row(
            SourcePriority::Normal,
            None,
            2,
            2.0,
            2.0,
            1.0,
            0.0,
            Vec::new(),
        )];
        let (out, scouts) = staff_passes_scouting(
            &rows,
            3,
            &mut shelf(10),
            Passes::All,
            &BTreeMap::new(),
            &scouting(2, Vec::new()),
        );
        assert_eq!(scouts.bare, 2);
        assert_eq!(
            out[0].keepers, 1,
            "the keepers staff from what the scouts left"
        );
    }

    #[test]
    fn the_reach_crew_gets_what_the_scouts_left() {
        // Five people, range 2 (two bare scouts): scouts 2, hold 2, and one left for the reach crew.
        let (rows, scouts) = staffed_with_scouts(5, 2);
        assert_eq!(scouts.bare, 2);
        assert_eq!(rows[0].kitted, 3);
        let (rows, scouts) = staffed_with_scouts(5, 0);
        assert_eq!(scouts.total(), 0);
        assert_eq!(
            rows[0].kitted, 5,
            "liveness: without scouts the crew takes them"
        );
    }

    #[test]
    fn the_steady_passes_see_the_scouts_too() {
        let rows = vec![kitted_hold_and_reach_row()];
        let (_, scouts) = staff_passes_scouting(
            &rows,
            5,
            &mut shelf(10),
            Passes::Steady,
            &BTreeMap::new(),
            &scouting(2, Vec::new()),
        );
        assert_eq!(scouts.bare, 2);
    }

    // ---------------------------------------------------------------------------------------
    // A tribe levies its bands (#862): the levy step
    // ---------------------------------------------------------------------------------------

    fn levy_row_of<'a>(priority: SourcePriority, want: f32) -> StaffRowInput<'a> {
        let mut row = bare_row(priority, want);
        row.levy = true;
        row
    }

    fn deal_with_levy(
        rows: &[StaffRowInput<'_>],
        pool: u32,
        share: f64,
        factor: f64,
        scouts: ScoutPlan,
    ) -> Dealt {
        let plan = ScoutPlan {
            levy: Some(LevyPlan {
                share,
                factor,
                fixed_budget: None,
                road_tiles: Vec::new(),
            }),
            ..scouts
        };
        staff_passes_counted(
            rows,
            pool,
            &mut shelf(0),
            Passes::All,
            &BTreeMap::new(),
            &plan,
            EVERY_HIGH_OPENER,
            Inputs::Every,
            KitStep::On,
        )
    }

    #[test]
    fn the_levy_never_takes_a_keep_up_head() {
        // The band's own row keeps its site with every hand it has; nothing is left to levy.
        let mut own = bare_row(SourcePriority::Normal, 0.0);
        own.keep_hands = 4.0;
        let rows = vec![own, levy_row_of(SourcePriority::High, 8.0)];
        let dealt = deal_with_levy(&rows, 4, 1.0, 1.0, ScoutPlan::default());
        assert_eq!(dealt.out[0].total(), 4, "the keepers hold every hand");
        assert_eq!(
            dealt.out[1].total(),
            0,
            "the levy is dealt from what pass 1 left"
        );
    }

    #[test]
    fn the_ceiling_caps_the_levy_heads_and_leaves_the_bands_own_rows_the_rest() {
        let heads = |share: f64| {
            let rows = vec![
                levy_row_of(SourcePriority::Normal, 8.0),
                bare_row(SourcePriority::Low, 8.0),
            ];
            let dealt = deal_with_levy(&rows, 10, share, 1.0, ScoutPlan::default());
            (dealt.out[0].total(), dealt.out[1].total())
        };
        let (none, own_none) = heads(0.0);
        let (low, own_low) = heads(0.25);
        let (normal, own_normal) = heads(0.5);
        let (high, own_high) = heads(1.0);
        assert_eq!((none, low, normal, high), (0, 2, 5, 8));
        assert!(none < low && low < normal && normal < high);
        // The band's own pass-2/3 row takes what the levy left, up to its own want.
        assert_eq!((own_none, own_low, own_normal, own_high), (8, 8, 5, 2));
    }

    #[test]
    fn a_leaderless_members_budget_is_the_factor_scaled_share_and_never_below_one() {
        let rows = vec![levy_row_of(SourcePriority::Normal, 8.0)];
        let dealt = deal_with_levy(&rows, 10, 0.5, 0.5, ScoutPlan::default());
        assert_eq!(dealt.out[0].total(), 2, "floor(0.5 x 0.5 x 10)");
        let dealt = deal_with_levy(&rows, 3, 0.25, 0.25, ScoutPlan::default());
        assert_eq!(
            dealt.out[0].total(),
            1,
            "at least one head while the ceiling allows any"
        );
    }

    #[test]
    fn a_tribe_members_scouts_are_dealt_after_pass_one_within_the_budget_and_a_lone_band_deals_them_first(
    ) {
        let scouting = ScoutPlan {
            range: 4,
            kitted_tiles: 2,
            bare_tiles: 1,
            stretch: crate::components::UNSTRETCHED,
            ..ScoutPlan::default()
        };
        let mut own = bare_row(SourcePriority::Normal, 0.0);
        own.keep_hands = 3.0;
        // A tribe member: pass 1 keeps three of four hands, the budget (at least one) buys one scout.
        let dealt = deal_with_levy(&[own], 4, 0.5, 1.0, scouting.clone());
        assert_eq!(dealt.out[0].total(), 3, "the keepers came first");
        assert_eq!(dealt.scout.total(), 1, "one scout within the budget");
        assert_eq!(dealt.scout.reach, 1);
        // A band in no tribe: scouts come off the top, ahead of the keepers.
        let mut own = bare_row(SourcePriority::Normal, 0.0);
        own.keep_hands = 3.0;
        let alone = staff_passes_counted(
            &[own],
            4,
            &mut shelf(0),
            Passes::All,
            &BTreeMap::new(),
            &scouting,
            EVERY_HIGH_OPENER,
            Inputs::Every,
            KitStep::On,
        );
        assert_eq!(alone.scout.total(), 4, "pass 0 deals the whole range");
        assert_eq!(alone.out[0].total(), 0, "and the keepers get what is left");
    }

    #[test]
    fn a_stretch_scales_the_scout_tiles_needed_and_a_zero_stretch_needs_none() {
        let plan = |stretch: f32| ScoutPlan {
            range: 4,
            kitted_tiles: 2,
            bare_tiles: 1,
            stretch,
            ..ScoutPlan::default()
        };
        let step = |stretch: f32| {
            let mut pool_left = 10;
            deal_scouts(&plan(stretch), &mut pool_left, None, &mut shelf(0))
        };
        assert_eq!(step(1.0).total(), 4);
        assert_eq!(
            step(0.5).total(),
            2,
            "half the edge needs half the scout-tiles"
        );
        assert_eq!(
            step(0.5).reach,
            4,
            "and two bare scouts reach the range over their half"
        );
        assert_eq!(
            step(0.0),
            ScoutCrew::default(),
            "an enclosed member scouts nothing"
        );
    }

    /// A small stretch's one bare scout is one scout-tile of surplus over the tiles needed, not a
    /// reach inflated by `1 / stretch` past the range.
    #[test]
    fn a_small_stretchs_scout_reaches_the_range_not_the_range_over_the_stretch() {
        const SMALL_STRETCH: f32 = 0.1;
        let plan = ScoutPlan {
            range: 1,
            kitted_tiles: 2,
            bare_tiles: 1,
            stretch: SMALL_STRETCH,
            ..ScoutPlan::default()
        };
        let mut pool_left = 10;
        let crew = deal_scouts(&plan, &mut pool_left, None, &mut shelf(0));
        assert_eq!(crew.total(), 1, "liveness: one bare scout");
        assert_eq!(crew.reach, 1);
    }

    #[test]
    fn at_equal_priority_a_levy_head_is_trimmed_before_the_bands_own() {
        use crate::components::{LaborAllocation, LaborTarget};
        use crate::ladder_ids::BranchId;
        let branch = BranchId::new("food_stores");
        let levy_target = LaborTarget::Tile {
            tile: UVec2::new(1, 1),
            branch,
        };
        let own_target = LaborTarget::Tile {
            tile: UVec2::new(2, 2),
            branch,
        };
        let mut allocation = LaborAllocation::default();
        allocation.set_staffed_row(levy_target.clone(), None, None);
        allocation.set_staffed_row(own_target.clone(), None, None);
        allocation
            .assignments
            .iter_mut()
            .find(|row| row.target.same_source(&levy_target))
            .unwrap()
            .for_tribe = Some(crate::tribe::TribeId(0));
        let crew = StaffedCrew {
            bare: 1,
            ..StaffedCrew::default()
        };
        // The levy row is FIRST: on index alone the later (own) row would go first.
        let mut staffed = StaffedBand {
            rows: vec![(levy_target, crew), (own_target, crew)],
            ..StaffedBand::default()
        };
        assert!(trim_one_hand(&allocation, &[], &mut staffed));
        assert_eq!(staffed.rows[0].1.total(), 0, "the levy head went");
        assert_eq!(staffed.rows[1].1.total(), 1, "the band's own head stayed");
    }

    /// A levy build's builder goes before the band's own row hand at equal priority (the cross-kind
    /// comparison), though a row's surplus otherwise goes before a build's builders.
    #[test]
    fn at_equal_priority_a_levy_builder_is_trimmed_before_an_own_row_hand() {
        use crate::components::{LaborAllocation, LaborTarget};
        use crate::ladder_ids::BranchId;
        let branch = BranchId::new("food_stores");
        let levy_site = UVec2::new(1, 1);
        let levy_target = LaborTarget::Tile {
            tile: levy_site,
            branch,
        };
        let own_target = LaborTarget::Tile {
            tile: UVec2::new(2, 2),
            branch,
        };
        let mut allocation = LaborAllocation::default();
        allocation.set_staffed_row(levy_target.clone(), None, None);
        allocation.set_staffed_row(own_target.clone(), None, None);
        allocation
            .assignments
            .iter_mut()
            .find(|row| row.target.same_source(&levy_target))
            .unwrap()
            .for_tribe = Some(crate::tribe::TribeId(0));
        let row_crew = StaffedCrew {
            bare: 1,
            ..StaffedCrew::default()
        };
        let build_crew = BuildCrew {
            kitted: 0,
            bare: 1,
            cap: 1,
            fed_first: false,
        };
        let mut staffed = StaffedBand {
            rows: vec![(own_target, row_crew)],
            builds: vec![(
                BuildSource::Tile {
                    tile: levy_site,
                    branch,
                },
                build_crew,
            )],
            ..StaffedBand::default()
        };
        assert!(trim_one_hand(&allocation, &[], &mut staffed));
        assert_eq!(staffed.builds[0].1.total(), 0, "the levy builder went");
        assert_eq!(staffed.rows[0].1.total(), 1, "the band's own hand stayed");
    }

    // -----------------------------------------------------------------------------------------
    // The kit step
    // -----------------------------------------------------------------------------------------

    /// A forage row that kits with baskets: a kitted head takes 2, a bare one 1; its useful take is 8,
    /// so a full-kit crew is 4 heads.
    fn basket_row<'a>(priority: SourcePriority, hold: f32, max: Option<u32>) -> StaffRowInput<'a> {
        let mut row = linear_row(
            priority,
            max,
            0,
            hold,
            8.0,
            2.0,
            1.0,
            vec![(Arc::from("baskets"), 1)],
        );
        row.food_per_take = 1.0;
        row
    }

    fn arcs(items: &[&str]) -> Vec<Arc<str>> {
        items.iter().map(|item| Arc::from(*item)).collect()
    }

    /// A bench order that makes `makes` (and, as a tool order, serves `tool_serves`).
    fn a_kit_order<'a>(
        priority: SourcePriority,
        cap: u32,
        tool: Option<&str>,
        makes: &[&str],
        tool_serves: &[&str],
        per_item: u32,
        credit: Option<crate::systems::KitCredit>,
    ) -> StaffRowInput<'a> {
        let mut row = a_bench_order(priority, cap, tool);
        row.bench_kit = Some(crate::systems::BenchKit {
            makes: arcs(makes),
            tool_serves: arcs(tool_serves),
            crafters_per_item: per_item,
            credit,
        });
        row
    }

    fn kit_shelf(baskets: u32) -> BTreeMap<Arc<str>, u32> {
        BTreeMap::from([(Arc::from("baskets"), baskets)])
    }

    fn kit_deal(
        rows: &[StaffRowInput<'_>],
        pool: u32,
        shelf: BTreeMap<Arc<str>, u32>,
        kit_step: KitStep,
    ) -> Dealt {
        let mut shelf = shelf;
        staff_passes_counted(
            rows,
            pool,
            &mut shelf,
            Passes::All,
            &BTreeMap::new(),
            &ScoutPlan::default(),
            EVERY_HIGH_OPENER,
            Inputs::Every,
            kit_step,
        )
    }

    /// **THE KIT IS MADE BEFORE ANYONE WORKS BARE**: a food row short of baskets (one on the shelf for
    /// a full crew of four) has the basket order crewed ahead of its bare hold heads, by the units it
    /// lacks times the crafters an item takes; the same deal with the step off hands the row its bare
    /// hold heads first.
    #[test]
    fn crafters_are_dealt_to_the_kit_order_before_a_bare_hold_head_and_capped_by_what_is_short() {
        let order = |per_item| {
            a_kit_order(
                SourcePriority::Normal,
                10,
                None,
                &["baskets"],
                &[],
                per_item,
                None,
            )
        };
        let rows = |per_item| {
            let mut food = basket_row(SourcePriority::Normal, 4.0, None);
            food.food_keep_up = true;
            vec![food, order(per_item)]
        };
        let on = kit_deal(&rows(1), 5, kit_shelf(1), KitStep::On);
        assert_eq!(
            on.kit_crafters, 3,
            "three baskets lacking, one crafter each"
        );
        assert_eq!(on.out[1].total(), 3);
        assert_eq!(
            on.pass_one[0], 2,
            "the row's one basket and the single bare hold head the crafters left"
        );
        assert_eq!(on.first_call.crafting, 3);
        let off = kit_deal(&rows(1), 5, kit_shelf(1), KitStep::Off);
        assert_eq!(off.kit_crafters, 0);
        assert_eq!(
            (off.pass_one[0], off.pass_one[1]),
            (3, 0),
            "with the step off the bare hold heads came first and nobody crafted in pass one"
        );
        let wide = kit_deal(&rows(2), 10, kit_shelf(1), KitStep::On);
        assert_eq!(wide.kit_crafters, 6, "two crafters an item, three items");
    }

    /// **A LOOM ORDER AHEAD OF A SHORT BASKET ITEM IS CREWED TO ITS CAP IN THE KIT STEP**, ahead of
    /// the basket order it serves (bench rank order).
    #[test]
    fn a_tool_order_serving_a_short_item_is_crewed_to_its_cap() {
        let rows = vec![
            a_kit_order(
                SourcePriority::Normal,
                2,
                None,
                &["loom"],
                &["baskets"],
                2,
                None,
            ),
            a_kit_order(
                SourcePriority::Normal,
                4,
                Some("loom"),
                &["baskets"],
                &[],
                1,
                None,
            ),
            basket_row(SourcePriority::Normal, 1.0, None),
        ];
        let dealt = kit_deal(&rows, 8, kit_shelf(0), KitStep::On);
        assert_eq!(
            dealt.out[0].total(),
            2,
            "the loom order is crewed to its cap"
        );
        assert_eq!(dealt.out[1].total(), 4);
        assert_eq!(dealt.kit_crafters, 6);
    }

    /// **NO KIT STEP WHEN THE SHELF COVERS EVERY ROW, and a row at its Max is not short.**
    #[test]
    fn no_kit_step_when_the_shelf_covers_every_row_or_the_row_is_at_its_max() {
        let order = || a_kit_order(SourcePriority::Normal, 10, None, &["baskets"], &[], 1, None);
        let covered = vec![basket_row(SourcePriority::Normal, 1.0, None), order()];
        let dealt = kit_deal(&covered, 8, kit_shelf(4), KitStep::On);
        assert_eq!((dealt.kit_crafters, dealt.first_call.crafting), (0, 0));
        let at_max = vec![basket_row(SourcePriority::Normal, 1.0, Some(1)), order()];
        let dealt = kit_deal(&at_max, 8, kit_shelf(1), KitStep::On);
        assert_eq!(dealt.kit_crafters, 0, "one basket arms the row's only head");
        let short = vec![basket_row(SourcePriority::Normal, 1.0, None), order()];
        assert!(
            kit_deal(&short, 8, kit_shelf(1), KitStep::On).kit_crafters > 0,
            "liveness: the same row with fewer baskets than heads is short"
        );
    }

    /// The food-first fixture of the kit guard: a High basket row and a Normal basket order that
    /// finishes four baskets in one turn at the crew it is dealt (when it is credited).
    fn kit_guard_rows<'a>(credited: bool) -> Vec<StaffRowInput<'a>> {
        let mut food = basket_row(SourcePriority::High, 0.0, None);
        food.useful_take = 40.0;
        let credit = credited.then(|| crate::systems::KitCredit {
            item: Arc::from("baskets"),
            units: 4.0,
            work_remaining: 4.0,
            rate_per_tool_crafter: 2.0,
            rate_per_hand_crafter: 1.0,
        });
        vec![
            food,
            a_kit_order(
                SourcePriority::Normal,
                4,
                None,
                &["baskets"],
                &[],
                1,
                credit,
            ),
        ]
    }

    fn kit_guard(rows: &[StaffRowInput<'_>]) -> Vec<u32> {
        let (out, _, _) = staff_band_food_first(
            rows,
            6,
            &mut kit_shelf(0),
            &BTreeMap::new(),
            &ScoutPlan::default(),
            FoodLedger {
                larder: 6.0,
                need: 3.0,
                standing: 0.0,
            },
            FOOD_FIRST_WINDOW,
        );
        crews(&out)
    }

    /// **THE CREDIT**: the larder (6, need 3) runs short inside the window while the four crafters
    /// make the baskets and the food row's two bare hands take 2 a turn - unless the baskets, done in
    /// one turn, arm those two hands (4 a turn from then on). Credited, the kit deal stands; without
    /// the credit the band is re-dealt with the crafters behind the food crews (deal C).
    #[test]
    fn the_kit_a_crafter_will_finish_counts_toward_the_larder() {
        assert_eq!(
            kit_guard(&kit_guard_rows(true)),
            vec![2, 4],
            "the finishing baskets keep the kit crafters"
        );
        assert_eq!(
            kit_guard(&kit_guard_rows(false)),
            vec![5, 1],
            "without the credit the larder runs short and the crafters go behind the food crew"
        );
    }

    /// **A LARDER THAT RUNS SHORT WHILE THE CRAFTERS WORK FALLS BACK TO DEAL C** even with a credit
    /// that arrives too late (the crafters take five turns to finish).
    #[test]
    fn a_larder_the_late_kit_cannot_save_puts_the_crafters_behind_the_food_crews() {
        let mut rows = kit_guard_rows(true);
        if let Some(credit) = rows[1]
            .bench_kit
            .as_mut()
            .and_then(|kit| kit.credit.as_mut())
        {
            credit.work_remaining = 400.0;
        }
        assert_eq!(kit_guard(&rows), vec![5, 1]);
    }

    /// **A BAND WITH NO BASKET AT ALL IS SHORT OF BASKETS**: the row's kit lists no live unit (nothing
    /// to arm with), but its chosen kit still names baskets, so the basket order is crewed in the kit
    /// step ahead of the bare hold heads.
    #[test]
    fn a_band_holding_no_baskets_has_them_made_before_anyone_works_bare() {
        let mut food = basket_row(SourcePriority::Normal, 4.0, None);
        food.kit_items = Vec::new();
        food.food_keep_up = true;
        let rows = vec![
            food,
            a_kit_order(SourcePriority::Normal, 10, None, &["baskets"], &[], 1, None),
        ];
        let on = kit_deal(&rows, 12, kit_shelf(0), KitStep::On);
        assert_eq!(
            on.kit_crafters, 8,
            "the row's bare useful crew, a basket each"
        );
        assert_eq!(on.pass_one[1], 8, "crafters before any bare hold head");
        let off = kit_deal(&rows, 12, kit_shelf(0), KitStep::Off);
        assert_eq!(off.kit_crafters, 0);
    }

    /// **A ROW BARE HANDS CANNOT WORK, WITH NO SPEAR, IS SHORT OF SPEARS.**
    #[test]
    fn a_hunt_bare_hands_cannot_wound_with_no_spears_is_short_of_them() {
        let mut hunt = linear_row(
            SourcePriority::Normal,
            None,
            0,
            0.0,
            4.0,
            2.0,
            0.0,
            Vec::new(),
        );
        hunt.kit_wants = vec![(Arc::from("spears"), 1)];
        let rows = vec![
            hunt,
            a_kit_order(SourcePriority::Normal, 3, None, &["spears"], &[], 1, None),
        ];
        let dealt = kit_deal(&rows, 6, kit_shelf(0), KitStep::On);
        assert!(dealt.kit_crafters >= 1, "the spear order is crewed");
        assert_eq!(dealt.out[0].total(), 0, "bare hands work nothing there");
    }

    /// **THE KIT STEP ONLY TAKES HANDS THE SHELF CANNOT ARM**: a row whose reach crew wants four
    /// baskets, with three on the shelf, keeps the hands that arm the two the hold crew has not taken
    /// for pass 2. A pool of three has nothing to spare (the kit step deals no crafter); a pool of four spares one hand for the one basket the shelf
    /// lacks.
    #[test]
    fn the_kit_step_leaves_the_hands_that_arm_the_shelfs_units() {
        let rows = || {
            vec![
                basket_row(SourcePriority::Normal, 1.0, None),
                a_kit_order(SourcePriority::Normal, 10, None, &["baskets"], &[], 1, None),
            ]
        };
        let tight = kit_deal(&rows(), 3, kit_shelf(3), KitStep::On);
        assert_eq!(
            tight.kit_crafters, 0,
            "every spare hand is reserved to arm a shelf unit"
        );
        assert_eq!(
            tight.out[0].kitted, 2,
            "pass 2 armed what its opener left (the order's ordinary opener takes the third hand)"
        );
        let spare = kit_deal(&rows(), 4, kit_shelf(3), KitStep::On);
        assert_eq!(
            spare.kit_crafters, 1,
            "one basket lacking, one hand the shelf cannot use"
        );
        assert_eq!(
            spare.out[0].kitted, 3,
            "and the shelf's units are still all armed"
        );
    }

    /// **THE WANT IS SIZED ON THE REACH CREW, NOT THE USEFUL CREW**: pass 3's useful crew is filled
    /// bare by design, so a row whose reach is covered by the baskets it holds is not short of more.
    #[test]
    fn a_row_whose_reach_crew_is_armed_is_not_short_though_its_useful_crew_is_larger() {
        // Hold 1, reach 2 (one kitted head at 2 a head), useful 8 (four kitted heads).
        let mut food = basket_row(SourcePriority::Normal, 1.0, None);
        food.reach = ReachTarget::Take(2.0);
        let rows = vec![
            food,
            a_kit_order(SourcePriority::Normal, 10, None, &["baskets"], &[], 1, None),
        ];
        let dealt = kit_deal(&rows, 8, kit_shelf(1), KitStep::On);
        assert_eq!(dealt.kit_crafters, 0, "one basket arms the reach crew");
    }
}
