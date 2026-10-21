//! ⛔ **THE SIM STAFFS EVERY SOURCE FROM THE PLAYER'S FLOOR** (#871, `docs/plan_labor_at_scale.md`
//! "Decided").
//!
//! The player states the **source**, its **floor**, its **kit** and an optional **Max**; this module
//! decides the **crew**, every turn, with no smoothing. A row's crew is the sum of three kinds of
//! head, staffed in three passes across the whole band:
//!
//! 1. **Hold, kitted** — every row (highest [`SourcePriority`] first, ties in row order) gets its
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

/// **THE THREE PASSES** — see the module doc. `pool` is the hands the sim may staff;
/// `shelf` the band's live units per item (taken off as rows are armed).
pub fn staff_band(
    rows: &[StaffRowInput<'_>],
    pool: u32,
    shelf: &mut BTreeMap<Arc<str>, u32>,
) -> Vec<RowStaffing> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(rows[i].priority), i));

    let mut out: Vec<RowStaffing> = vec![StaffedCrew::default(); rows.len()];
    let mut pool_left = pool;
    let mut cap_left: Vec<u32> = rows.iter().map(cap_of).collect();

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

    // ## Pass 1 — keepers, then kitted heads up to the hold crew.
    for &i in &order {
        let row = &rows[i];
        let keepers = row.keepers.min(cap_left[i]).min(pool_left);
        out[i].keepers = keepers;
        pool_left -= keepers;
        cap_left[i] -= keepers;

        let kmax = kitted_capacity(row, out[i].kitted, shelf);
        let want = take_hands_needed(row, row.hold_take, kmax).map_or(NOBODY, |hands| hands);
        let kitted = want.min(kmax).min(cap_left[i]).min(pool_left);
        arm(row, out[i].kitted, kitted, shelf);
        out[i].kitted = kitted;
        pool_left -= kitted;
        cap_left[i] -= kitted;
    }

    // ## Pass 2 — kitted heads up to the reach crew, from whatever units and heads remain.
    for &i in &order {
        let row = &rows[i];
        let kmax = kitted_capacity(row, out[i].kitted, shelf);
        let want = match row.reach {
            ReachTarget::Nothing => NOBODY,
            ReachTarget::Take(target) => {
                take_hands_needed(row, target, kmax).map_or(NOBODY, |hands| hands)
            }
            ReachTarget::Heads(heads) => heads,
        };
        let target = want.min(kmax).min(out[i].kitted + cap_left[i]);
        let target = target.min(out[i].kitted + pool_left).max(out[i].kitted);
        let added = target - out[i].kitted;
        arm(row, out[i].kitted, target, shelf);
        out[i].kitted = target;
        pool_left -= added;
        cap_left[i] -= added;
    }

    // ## Pass 3 — every row up to its USEFUL crew: the kit's units still unspent first (a kit never
    // sits on the shelf while someone works bare), then bare hands where one adds value.
    for &i in &order {
        let row = &rows[i];
        let kmax = kitted_capacity(row, out[i].kitted, shelf);
        let useful = useful_hands(row, kmax);
        let target = useful.min(kmax).max(out[i].kitted);
        let target = target.min(out[i].kitted + cap_left[i]);
        let target = target.min(out[i].kitted + pool_left);
        let added = target - out[i].kitted;
        arm(row, out[i].kitted, target, shelf);
        out[i].kitted = target;
        pool_left -= added;
        cap_left[i] -= added;

        if !row.bare_can_work {
            continue;
        }
        let needed = useful_hands(row, out[i].kitted);
        let bare = needed
            .saturating_sub(out[i].kitted)
            .min(cap_left[i])
            .min(pool_left);
        out[i].bare = bare;
        pool_left -= bare;
        cap_left[i] -= bare;
    }
    out
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
    let sources = world.sources;
    let equipment = sources.equipment;
    let crew_bound = available.max(1);
    let crew_float = crew_bound as f32;

    let mut shelf: BTreeMap<Arc<str>, u32> = BTreeMap::new();
    let mut rows: Vec<StaffRowInput<'a>> = Vec::new();
    let mut targets: Vec<LaborTarget> = Vec::new();
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
    for item_list in &items {
        for (item, _) in item_list {
            shelf
                .entry(Arc::clone(item))
                .or_insert_with(|| band_kit.live_units(item, equipment));
        }
    }

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
        };
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
    }
    let staffed = staff_band(&rows, pool, &mut shelf);
    targets.into_iter().zip(staffed).collect()
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
    input.bare_can_work = crate::fauna::hunt_fight_hands(&herd, fauna, 1.0, &bare_party).is_some();
    let base = sources.labor.hunt.per_worker_biomass_capacity;
    let bare_carry =
        equipment.hunt_per_worker_biomass_capacity(base, &equipment.no_kit(), band_kit);
    let drives = sources.drives_known;
    let hands_herd = herd.clone();
    input.hands = Box::new(move |target, armed| {
        let cov = coverage(armed);
        let party = party_of(&cov, &hands_herd);
        let stay = party.stay_fraction(crate::fauna::herd_wariness(&hands_herd, fauna));
        let haul = crate::fauna::hunt_crew_needed(
            target,
            &hands_herd,
            fauna,
            &cov,
            |crew_kit| equipment.hunt_per_worker_biomass_capacity(base, crew_kit, band_kit),
            bare_carry,
            stay,
            drives,
        ) as f32;
        let fight = crate::fauna::hunt_fight_hands(&hands_herd, fauna, target, &party)?;
        Some(haul.max(fight))
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

/// **RESTAFF ONE BAND OUTSIDE THE TURN** — what a command handler calls after it changed an input
/// the passes read (a floor, a kit, a Max, a priority, a stopped row), so the snapshot shows the new
/// crew the moment the player acts. It reads the world the way the labor pass does, regrow-first
/// ([`crate::fauna::ProjectionStart::BeforeRegrowth`]), and writes only the band's staffed rows; the next turn
/// restaffs from the live state and is the authority.
pub fn restaff_band(world: &mut bevy::prelude::World, band: bevy::prelude::Entity) {
    let Some(cohort) = world.get::<crate::components::PopulationCohort>(band) else {
        return;
    };
    let faction = cohort.faction;
    let available = crate::components::available_workers(cohort.working);
    let Some(allocation) = world.get::<LaborAllocation>(band).cloned() else {
        return;
    };
    let bench_workers = world
        .get::<crate::components::BandBench>(band)
        .map_or(NOBODY, |bench| bench.workers);
    let band_id = world.get::<BandId>(band).copied();
    let pool = staffable_pool(&allocation, available, bench_workers);
    let staffed = {
        let world_ref: &bevy::prelude::World = world;
        let equipment = world_ref
            .get_resource::<crate::EquipmentConfigHandle>()
            .map_or_else(
                crate::equipment_config::EquipmentConfig::builtin,
                crate::EquipmentConfigHandle::get,
            );
        let band_kit = world_ref
            .get::<BandEquipment>(band)
            .cloned()
            .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment, available as f32));
        let no_improvements = crate::tile_improvement::TileImprovementRegistry::default();
        let no_roads = crate::routes::RoadRegistry::default();
        crate::take_claims::with_world_sources(world_ref, faction, |sources| {
            let staffing_world = StaffingWorld {
                sources,
                tile_improvements: world_ref
                    .get_resource::<crate::tile_improvement::TileImprovementRegistry>()
                    .unwrap_or(&no_improvements),
                roads: world_ref
                    .get_resource::<crate::routes::RoadRegistry>()
                    .unwrap_or(&no_roads),
                band: band_id,
            };
            staff_rows(&staffing_world, &allocation, &band_kit, pool, available)
        })
    };
    if let Some(mut allocation) = world.get_mut::<LaborAllocation>(band) {
        allocation.apply_staffing(staffed, NOBODY);
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
        }
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
}
