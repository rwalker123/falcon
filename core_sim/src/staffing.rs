//! ⛔ **THE SIM STAFFS EVERY SOURCE FROM THE PLAYER'S FLOOR** (#871, `docs/plan_labor_at_scale.md`
//! "Decided").
//!
//! The player states the **source**, its **floor**, its **kit** and an optional **Max**; this module
//! decides the **crew**, every turn, with no smoothing. A row's crew is the sum of three kinds of
//! head, staffed in three passes across the whole band:
//!
//! 1. **Hold, kitted** — every row (highest [`SourcePriority`] first, ties dealt as below) gets its
//!    **keepers** first, then **kitted** heads up to the crew that takes what regrows at the floor
//!    (`T_hold`). If the band's units run out the row stays short: there is no bare fallback here.
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
    BandEquipment, BandId, LaborAllocation, LaborTarget, SourcePriority, StaffedCrew,
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

/// What the reach pass is aiming at on one row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReachTarget {
    /// Nothing to draw down — the row is below its floor, or keeps and takes nothing.
    Nothing,
    /// Draw the stock down at this take per turn (already strictly above the peak regrowth).
    Take(f32),
    /// A mined seam with no regrowth, until keep-on-hand stone (#874): there is no regrowth to
    /// out-take, so the target is the player's Max in heads — this many **take hands**.
    Heads(u32),
}

/// One staffed row, as the passes see it.
pub struct StaffRowInput<'a> {
    /// The player's rank for the row ([`SourcePriority`]) — the order passes serve rows in.
    pub priority: SourcePriority,
    /// The player's Max: the most heads the sim may put on the row. `None` is no max.
    pub max: Option<u32>,
    /// The keeping hands the row owes its site, staffed first.
    pub keepers: u32,
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

/// The heads the row needs in total (take hands, keepers excluded) to reach `target` with `armed`
/// of them kitted — rounded up once. `None` where no crew reaches it.
fn take_hands_needed(row: &StaffRowInput<'_>, target: f32, armed: u32) -> Option<u32> {
    if target <= NO_TAKE_WANTED {
        return Some(NOBODY);
    }
    (row.hands)(target, armed).map(|hands| (hands.max(0.0).ceil() as u32).max(AT_LEAST_ONE_HAND))
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
    staff_passes(rows, pool, shelf, Passes::All)
}

/// [`staff_band`] over the passes `passes` names.
fn staff_passes(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
    passes: Passes,
) -> Vec<RowStaffing> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(rows[i].priority), i));

    let mut out: Vec<RowStaffing> = vec![StaffedCrew::default(); rows.len()];
    let pool_left = pool;
    let cap_left: Vec<u32> = rows.iter().map(cap_of).collect();

    // ## What each row would aim at ignoring the pool and the other rows — published after the Max.
    for &i in &order {
        let row = &rows[i];
        let kmax = kitted_capacity(row, NOBODY, shelf);
        let keepers = row.keepers.min(cap_of(row));
        let hold = take_hands_needed(row, row.hold_take, kmax).unwrap_or(NOBODY);
        let reach = match row.reach {
            ReachTarget::Nothing => NOBODY,
            ReachTarget::Take(target) => take_hands_needed(row, target, kmax).unwrap_or(NOBODY),
            ReachTarget::Heads(heads) => heads,
        };
        out[i].hold_crew = keepers.saturating_add(hold).min(cap_of(row));
        out[i].reach_crew = keepers.saturating_add(reach.max(hold)).min(cap_of(row));
        let useful = useful_hands(row, kmax);
        out[i].useful_crew = keepers
            .saturating_add(useful.max(reach).max(hold))
            .min(cap_of(row));
    }

    let mut ledger = Ledger {
        out,
        pool_left,
        cap_left,
    };
    let groups = priority_groups(rows, &order);

    // ## Pass 1 — keepers, then kitted heads up to the hold crew; one head at a time within a group.
    for group in &groups {
        deal_round_robin(group, |i| ledger.deal_keeper(&rows[i], i));
        deal_kitted(rows, group, &mut ledger, shelf, |row, kmax| {
            take_hands_needed(row, row.hold_take, kmax).map_or(NOBODY, |hands| hands)
        });
    }

    // ## Pass 2 — kitted heads up to the reach crew, from whatever units and heads remain.
    for group in &groups {
        deal_kitted(rows, group, &mut ledger, shelf, |row, kmax| {
            match row.reach {
                ReachTarget::Nothing => NOBODY,
                ReachTarget::Take(target) => {
                    take_hands_needed(row, target, kmax).map_or(NOBODY, |hands| hands)
                }
                ReachTarget::Heads(heads) => heads,
            }
        });
    }

    if passes == Passes::Steady {
        return ledger.out;
    }

    // ## Pass 3 — every row up to its USEFUL crew: the kit's units still unspent first (a kit never
    // sits on the shelf while someone works bare), then bare hands where one adds value.
    for group in &groups {
        deal_kitted(rows, group, &mut ledger, shelf, useful_hands);
        let mut needed: Vec<Option<u32>> = vec![None; rows.len()];
        deal_round_robin(group, |i| {
            let row = &rows[i];
            if !row.bare_can_work {
                return false;
            }
            let kitted = ledger.out[i].kitted;
            let useful = *needed[i].get_or_insert_with(|| useful_hands(row, kitted));
            ledger.deal_head(i, kitted + ledger.out[i].bare < useful, HeadKind::Bare)
        });
    }
    ledger.out
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
    out: Vec<RowStaffing>,
    pool_left: u32,
    cap_left: Vec<u32>,
}

impl Ledger {
    /// Seat one head on row `i` when `wanted` and the pool and the Max allow it.
    fn deal_head(&mut self, i: usize, wanted: bool, kind: HeadKind) -> bool {
        if !wanted || self.pool_left == NOBODY || self.cap_left[i] == NOBODY {
            return false;
        }
        match kind {
            HeadKind::Keeper => self.out[i].keepers += 1,
            HeadKind::Kitted => self.out[i].kitted += 1,
            HeadKind::Bare => self.out[i].bare += 1,
        }
        self.pool_left -= 1;
        self.cap_left[i] -= 1;
        true
    }

    /// One keeper, while the row still owes its site one.
    fn deal_keeper(&mut self, row: &StaffRowInput<'_>, i: usize) -> bool {
        let owed = self.out[i].keepers < row.keepers;
        self.deal_head(i, owed, HeadKind::Keeper)
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
    want_of: impl Fn(&StaffRowInput<'_>, u32) -> u32,
) {
    let mut memo: Vec<Option<(u32, u32)>> = vec![None; rows.len()];
    deal_round_robin(group, |i| {
        let row = &rows[i];
        let held = ledger.out[i].kitted;
        let kmax = kitted_capacity(row, held, shelf);
        let want = match memo[i] {
            Some((at, want)) if at == kmax => want,
            _ => {
                let want = want_of(row, kmax);
                memo[i] = Some((kmax, want));
                want
            }
        };
        if !ledger.deal_head(i, held < want.min(kmax), HeadKind::Kitted) {
            return false;
        }
        arm(row, held, held + 1, shelf);
        true
    });
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

/// **THE USEFUL CREW'S TAKE HANDS** - the heads that reach the row's useful take with `armed` of
/// them kitted (a mined seam's useful crew is its Max in heads).
fn useful_hands(row: &StaffRowInput<'_>, armed: u32) -> u32 {
    match row.reach {
        ReachTarget::Heads(heads) => heads,
        _ => take_hands_needed(row, row.useful_take, armed).unwrap_or(NOBODY),
    }
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
    fn distance_to(&self, site: UVec2) -> u32 {
        crate::grid_utils::hex_distance_wrapped(
            self.position,
            site,
            self.grid_width,
            self.wrap_horizontal,
        )
    }
}

/// **THE HANDS THE SIM MAY STAFF** — the working people less the bench, the hands still walking home
/// and every crew the player set by hand (Scout, Warrior, Builders and a need row's muster).
pub fn staffable_pool(allocation: &LaborAllocation, available: u32, bench_workers: u32) -> u32 {
    let player_set: u32 = allocation
        .assignments
        .iter()
        .filter(|row| !row.target.is_staffed())
        .map(|row| row.workers)
        .sum();
    available
        .saturating_sub(bench_workers)
        .saturating_sub(allocation.walking_home())
        .saturating_sub(player_set)
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
    rows: Vec<StaffRowInput<'a>>,
    targets: Vec<LaborTarget>,
    /// Each staffed row's resolved kit, in `rows` order.
    kits: Vec<KitChoice>,
}

/// **STAFF EVERY STAFFED ROW OF `allocation`** — the world half of the passes. `pool` is
/// [`staffable_pool`]; `available` the band's whole working crew, the bound keepers are sized
/// against. Returns each staffed row's crew keyed by its target, for
/// [`LaborAllocation::apply_staffing`].
pub fn staff_rows<'a>(
    world: &'a StaffingWorld<'a>,
    allocation: &LaborAllocation,
    band_kit: &'a BandEquipment,
    pool: u32,
    available: u32,
) -> Vec<(LaborTarget, StaffedCrew)> {
    let equipment = world.sources.equipment;
    let inputs = build_staff_inputs(world, allocation, band_kit, available);
    let mut shelf: BTreeMap<Arc<str>, u32> = BTreeMap::new();
    for row in &inputs.rows {
        for (item, _) in &row.kit_items {
            shelf
                .entry(Arc::clone(item))
                .or_insert_with(|| band_kit.live_units(item, equipment));
        }
    }
    let staffed = staff_band(&inputs.rows, pool, &mut shelf);
    inputs.targets.into_iter().zip(staffed).collect()
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
        .filter(|row| row.target.is_staffed())
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
    let staffed = staff_passes(&inputs.rows, pool, &mut shelf, Passes::Steady);
    let mut wants: BTreeMap<String, KitWant> = BTreeMap::new();
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
    let kits: Vec<KitChoice> = allocation
        .assignments
        .iter()
        .map(|row| row.kit_choice(equipment))
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

    for (idx, row) in allocation.assignments.iter().enumerate() {
        if !row.target.is_staffed() {
            continue;
        }
        let kit = &kits[idx];
        let kit_items = items[idx].clone();
        let target = row.target.clone();
        let declared = declared_on(allocation, &target);
        let mut input = StaffRowInput {
            priority: row.priority,
            max: row.max_workers,
            keepers: 0,
            hold_take: NO_TAKE_WANTED,
            useful_take: NO_TAKE_WANTED,
            reach: ReachTarget::Nothing,
            kit_items,
            hands: Box::new(|_, _| Some(0.0)),
            bare_can_work: false,
            distance: NOBODY,
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
                input.keepers = planned_keep_hands(sources, band_kit, declared, &target, crew_bound)
                    .ceil() as u32;
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
            }
            LaborTarget::Hunt {
                fauna_id, floor, ..
            } => {
                input.keepers = planned_keep_hands(sources, band_kit, declared, &target, crew_bound)
                    .ceil() as u32;
                hunt_input(
                    world, band_kit, kit, fauna_id, *floor, crew_float, &mut input,
                );
            }
            LaborTarget::Extract {
                tile,
                material,
                floor,
            } => {
                input.keepers = planned_keep_hands(sources, band_kit, declared, &target, crew_bound)
                    .ceil() as u32;
                extract_input(
                    world, band_kit, kit, *tile, material, *floor, crew_float, &mut input,
                );
            }
            LaborTarget::Tile { .. } => {
                input.keepers = tile_keepers(world, band_kit, &target, crew_bound).ceil() as u32;
            }
            LaborTarget::Roadwork => {
                input.keepers = roadwork_keepers(world, band_kit, crew_bound).ceil() as u32;
            }
            LaborTarget::Scout | LaborTarget::Warrior | LaborTarget::Builders => continue,
        }
        rows.push(input);
        targets.push(target);
        staffed_kits.push(kit.clone());
    }
    StaffInputs {
        rows,
        targets,
        kits: staffed_kits,
    }
}

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
    let Some(herd) = sources.herd(fauna_id) else {
        return;
    };
    let herd = herd.into_owned();
    let equipment = sources.equipment;
    let fauna = sources.fauna;
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
    let hands_herd = herd.clone();
    input.hands = Box::new(move |target, armed| {
        let cov = coverage(armed);
        let party = party_of(&cov, &hands_herd);
        crate::fauna::hunt_crew_for_take(
            target,
            room,
            &hands_herd,
            fauna,
            &cov,
            |crew_kit| equipment.hunt_per_worker_biomass_capacity(base, crew_kit, band_kit),
            bare_carry,
            &party,
            drives,
        )
        .map(|crew| crew as f32)
    });
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
    input.useful_take = room;
    input.reach = ReachTarget::Take(reach_target(peak, room));
}

/// Extraction: wood regrows and follows the general rule; stone (no regrowth) holds nothing and its
/// reach is the Max in heads.
#[allow(clippy::too_many_arguments)] // the row's own inputs
fn extract_input<'a>(
    world: &'a StaffingWorld<'a>,
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
    if ground_rate <= crate::extraction_config::NEVER_RENEWS {
        // A seam that never renews holds nothing; its reach is the player's Max in heads.
        let wanted = input
            .max
            .map_or(NOBODY, |max| max.saturating_sub(input.keepers));
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

/// Where `band` is camped, read the way the labor pass reads it; `None` where the world has no map.
fn camp_of(world: &bevy::prelude::World, band: bevy::prelude::Entity) -> Option<CampSite> {
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

/// Run `body` over a band's staffing context: the world as the labor pass reads it, regrow-first,
/// the band's allocation, its gear, the pool and the working crew. `None` where the band has no
/// cohort or allocation.
fn with_band_staffing<R>(
    world: &bevy::prelude::World,
    band: bevy::prelude::Entity,
    body: impl FnOnce(&StaffingWorld<'_>, &LaborAllocation, &BandEquipment, u32, u32) -> R,
) -> Option<R> {
    let cohort = world.get::<crate::components::PopulationCohort>(band)?;
    let faction = cohort.faction;
    let available = crate::components::available_workers(cohort.working);
    let allocation = world.get::<LaborAllocation>(band)?.clone();
    let bench_workers = world
        .get::<crate::components::BandBench>(band)
        .map_or(NOBODY, |bench| bench.workers);
    let band_id = world.get::<BandId>(band).copied();
    let camp = camp_of(world, band);
    let pool = staffable_pool(&allocation, available, bench_workers);
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
    Some(crate::take_claims::with_world_sources(
        world,
        faction,
        |sources| {
            let staffing_world = StaffingWorld {
                sources,
                tile_improvements: world
                    .get_resource::<crate::tile_improvement::TileImprovementRegistry>()
                    .unwrap_or(&no_improvements),
                roads: world
                    .get_resource::<crate::routes::RoadRegistry>()
                    .unwrap_or(&no_roads),
                band: band_id,
                camp,
            };
            body(&staffing_world, &allocation, &band_kit, pool, available)
        },
    ))
}

/// **RESTAFF ONE BAND OUTSIDE THE TURN** — what a command handler calls after it changed an input
/// the passes read (a floor, a kit, a Max, a priority, a stopped row), so the snapshot shows the new
/// crew the moment the player acts. It reads the world the way the labor pass does, regrow-first
/// ([`crate::fauna::ProjectionStart::BeforeRegrowth`]), and writes only the band's staffed rows; the next turn
/// restaffs from the live state and is the authority. While the band's outfitting window is open it
/// also refreshes the kit its work wants ([`WorkKitWants`]).
pub fn restaff_band(world: &mut bevy::prelude::World, band: bevy::prelude::Entity) {
    let Some(staffed) = with_band_staffing(
        world,
        band,
        |staffing_world, allocation, kit, pool, available| {
            staff_rows(staffing_world, allocation, kit, pool, available)
        },
    ) else {
        return;
    };
    if let Some(mut allocation) = world.get_mut::<LaborAllocation>(band) {
        allocation.apply_staffing(staffed, NOBODY);
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
        StaffRowInput {
            priority,
            max,
            keepers,
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
        let steady = staff_passes(&rows, POOL, &mut steady_shelf, Passes::Steady);
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
}
