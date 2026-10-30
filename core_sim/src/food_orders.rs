//! ⛔ **THE CHIEF'S FOOD AND FODDER ORDERS** (#860, `docs/plan_labor_at_scale.md` -> "Band orders - the
//! chief"). The food order keeps N turns of food on hand, the fodder order N turns of hay for the
//! band's pens; both are carried out by forage and hunt rows the chief places, ranks and floors.
//!
//! This is the forage/hunt twin of [`crate::keep_on_hand::manage_dig_rows`], and it writes the same
//! seam: a row's [`KeepShare`] on [`LaborAllocation::keep_shares`], which `staffing` reads. Where the
//! two differ:
//!
//! - **Currency.** A material order is in units; these are in **turns** (`target`, `onHand`) over a
//!   per-turn **draw** (`PopulationCohort::last_food_need`, `LaborAllocation::last_fodder_need`). The
//!   want is `draw + (target - turns) * draw / keep_on_hand_refill_turns` below the target and **the
//!   draw itself at or above it** - the band eats every turn, so the keep-up crew still works.
//! - **Which sources.** Every known forage patch and huntable herd in work range, each probed through
//!   the staffing's own reading ([`crate::staffing::probe_source`]) so a herd the band's kit cannot
//!   wound is no candidate. Ranked by food (or fodder) per hand at the floor, then the shortest walk,
//!   then the source key; each is given a share up to what it gives at the floor without being drawn
//!   down, best first, until the want is covered.
//! - **The floor.** Best (`DEFAULT_ESCAPEMENT_FLOOR`) below the target, the Learning floor
//!   (`band_orders.learning_floor`) at or above it. Never lower than Best.
//! - **Whole basket.** A chief's forage row has an empty take selection.
//! - **One patch, one order.** A candidate belongs to the order whose product it gives more of per
//!   hand (fodder only when strictly more). The orders are filled in Priority order (food first on a
//!   tie); what the other rows - the other order's and the player's - brought in of this order's
//!   product last turn (`last_yields`) is subtracted from this order's want first.
//! - **Improvements stay.** A row on a tended patch, a field, a tamed herd or a pen is never retired
//!   by the chief, even at a zero share, so its keepers keep coming; a pinned row ([`source_pinned`])
//!   neither moves nor goes. The chief never starts a build.
//! - **Rows are the chief's or the player's one by one.** [`LaborAllocation::order_rows`] lists the
//!   rows the chief works; a row absent from it is the player's whoever owns the kind - the chief
//!   leaves it alone and counts what it brings in.

use bevy::math::UVec2;

use crate::components::{
    BuildSource, KeepShare, LaborAllocation, LaborTarget, SourcePriority, TakeSelection,
    DEFAULT_ESCAPEMENT_FLOOR,
};
use crate::fauna::Herd;
use crate::keep_on_hand::{is_row_kind, source_key, DigSiteInputs, FODDER_KIND, FOOD_KIND};
use crate::staffing::probe_source;

/// What the food and fodder orders read of the band beyond its allocation.
pub struct FoodInputs<'a> {
    /// The staffing's sources and the band's gear and camp (shared with the dig pass).
    pub site: &'a DigSiteInputs<'a>,
    /// Whether the band's people have discovered the tile (fog, or no fog).
    pub tile_known: &'a dyn Fn(UVec2) -> bool,
    /// Whether the band can see the herd now.
    pub herd_known: &'a dyn Fn(&Herd) -> bool,
    /// The band's readings the two orders are judged on.
    pub readings: OrderReadings,
}

/// **THE BAND'S FOOD AND FODDER STOCKS AND PER-TURN NEEDS**: the figures the sim already keeps - the
/// larder (`stores`, `FOOD`) over what the people eat a turn (`PopulationCohort::last_food_need`), the
/// hay store (`FODDER`) over what the pens need beyond their pasture
/// (`LaborAllocation::last_fodder_need`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrderReadings {
    pub food_stock: f32,
    pub food_need: f32,
    pub fodder_stock: f32,
}

impl OrderReadings {
    /// The per-turn draw the `kind` order keeps up with, in the kind's units.
    pub fn draw(&self, kind: &str, allocation: &LaborAllocation) -> f32 {
        match kind {
            FOOD_KIND => self.food_need,
            FODDER_KIND => allocation.last_fodder_need,
            material => allocation.material_draw(material),
        }
    }

    /// What the band holds of the kind, in the kind's own units.
    pub fn stock(&self, kind: &str) -> Option<f32> {
        match kind {
            FOOD_KIND => Some(self.food_stock),
            FODDER_KIND => Some(self.fodder_stock),
            _ => None,
        }
    }

    /// Turns of cover: stock over the per-turn draw, `0` where nothing is drawn.
    pub fn turns(&self, kind: &str, allocation: &LaborAllocation) -> f32 {
        let draw = self.draw(kind, allocation);
        match self.stock(kind) {
            Some(stock) if draw > NOTHING => stock / draw,
            _ => NOTHING,
        }
    }
}

const NOTHING: f32 = 0.0;

/// One forage patch or herd the chief could work.
struct Candidate {
    target: LaborTarget,
    key: String,
    tile: UVec2,
    herd: String,
    distance: u32,
    /// `Some(FODDER_KIND)` when the source gives more fodder than food per hand, else food.
    order: &'static str,
    /// The product's units per hand at the floor.
    units_per_hand: f32,
    /// The product's units the source gives a turn at the floor without being drawn down.
    sustainable_units: f32,
    /// The product's units per biomass.
    rate: f32,
}

/// Whether a forage or hunt row may neither move nor go: its party is out, a walk of its cargo is on
/// the road, or a build is declared on its source.
pub fn source_pinned(allocation: &LaborAllocation, target: &LaborTarget) -> bool {
    let party_out = allocation
        .assignments
        .iter()
        .any(|row| row.target.same_source(target) && row.party.is_some());
    let cargo_walking = allocation
        .homeward
        .iter()
        .any(|walk| walk.target.same_source(target));
    let build_declared = allocation
        .builds
        .iter()
        .any(|build| match (&build.source, target) {
            (BuildSource::Patch(at), LaborTarget::Forage { tile, .. }) => at == tile,
            (BuildSource::Herd(id), LaborTarget::Hunt { fauna_id, .. }) => id == fauna_id,
            _ => false,
        });
    party_out || cargo_walking || build_declared
}

fn candidate_of(
    inputs: &FoodInputs<'_>,
    target: LaborTarget,
    tile: UVec2,
    herd: String,
    crew: f32,
) -> Option<Candidate> {
    let sources = inputs.site.sources;
    let probe = probe_source(sources, inputs.site.band_kit, &target, crew)?;
    if !probe.viable {
        return None;
    }
    let food_per_hand = probe.per_hand * probe.food_rate;
    let fodder_per_hand = probe.per_hand * probe.fodder_rate;
    let (order, units_per_hand, rate) = if fodder_per_hand > food_per_hand {
        (FODDER_KIND, fodder_per_hand, probe.fodder_rate)
    } else if probe.food_rate > NOTHING {
        (FOOD_KIND, food_per_hand, probe.food_rate)
    } else {
        return None;
    };
    Some(Candidate {
        key: source_key(&target)?,
        distance: inputs.site.camp.map_or(0, |camp| camp.distance_to(tile)),
        target,
        tile,
        herd,
        order,
        units_per_hand,
        sustainable_units: probe.sustainable * rate,
        rate,
    })
}

fn forage_target(tile: UVec2, floor: f32) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

fn hunt_target(id: &str, floor: f32) -> LaborTarget {
    LaborTarget::Hunt {
        fauna_id: id.to_string(),
        floor,
        move_with_herd: false,
    }
}

/// **KEEP THE CHIEF'S FOOD AND FODDER ROWS WHERE HIS ORDERS SAY** - run right after
/// [`crate::keep_on_hand::manage_dig_rows`], before staffing, by the turn and by every command
/// restaff. See the module documentation.
pub fn manage_food_orders(
    allocation: &mut LaborAllocation,
    inputs: &FoodInputs<'_>,
    has_leader: bool,
) {
    if !has_leader {
        return;
    }
    let sources = inputs.site.sources;
    let refill = sources.labor.keep_on_hand_refill_turns as f32;
    let work_range = sources.labor.band_work_range;
    let crew = sources.labor.tile_worker_cap as f32;
    let best = DEFAULT_ESCAPEMENT_FLOOR;

    // Every candidate source, in a stable order.
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut tiles: Vec<UVec2> = sources.forage.patches.keys().copied().collect();
    tiles.sort_by_key(|tile| (tile.y, tile.x));
    for tile in tiles {
        let within = inputs
            .site
            .camp
            .is_none_or(|camp| camp.distance_to(tile) <= work_range);
        if !within || !(inputs.tile_known)(tile) {
            continue;
        }
        let Some(_) = sources.forage.patch(tile) else {
            continue;
        };
        if let Some(candidate) =
            candidate_of(inputs, forage_target(tile, best), tile, String::new(), crew)
        {
            candidates.push(candidate);
        }
    }
    for herd in sources.herds.entries() {
        let within = inputs
            .site
            .camp
            .is_none_or(|camp| camp.distance_to(herd.position()) <= work_range);
        let herbivore = sources
            .fauna
            .species_by_display(&herd.species)
            .is_some_and(|def| def.diet == crate::fauna_config::Diet::Herbivore);
        if !within || !herbivore || !(inputs.herd_known)(herd) {
            continue;
        }
        if let Some(candidate) = candidate_of(
            inputs,
            hunt_target(&herd.id, best),
            herd.position(),
            herd.id.clone(),
            crew,
        ) {
            candidates.push(candidate);
        }
    }

    // Adoption: a chief's arrival or a whole-kind claim lists the band's existing rows under the
    // order their product belongs to.
    let adopting: Vec<String> = allocation
        .adopt_pending
        .iter()
        .filter(|kind| is_row_kind(kind) && allocation.chief_owns(kind))
        .cloned()
        .collect();
    if !adopting.is_empty() {
        let rows: Vec<LaborTarget> = allocation
            .assignments
            .iter()
            .map(|row| row.target.clone())
            .collect();
        for target in rows {
            if matches!(target, LaborTarget::Extract { .. }) {
                continue;
            }
            let Some(key) = source_key(&target) else {
                continue;
            };
            if allocation.order_rows.contains_key(&key) {
                continue;
            }
            let order = candidates
                .iter()
                .find(|candidate| candidate.key == key)
                .map_or(FOOD_KIND, |candidate| candidate.order);
            if adopting.iter().any(|kind| kind == order) {
                allocation.order_rows.insert(key, order.to_string());
            }
        }
        for kind in &adopting {
            allocation.adopt_pending.remove(kind);
        }
    }

    // The orders, in Priority order, food first on a tie.
    let mut orders: Vec<(&'static str, SourcePriority)> = [FOOD_KIND, FODDER_KIND]
        .into_iter()
        .filter(|kind| allocation.chief_owns(kind))
        .filter_map(|kind| Some((kind, allocation.order_for(kind)?.priority)))
        .collect();
    orders.sort_by_key(|order| std::cmp::Reverse(order.1));

    for (kind, priority) in orders {
        let order = *allocation.order_for(kind).expect("a chief-owned order");
        let draw = inputs.readings.draw(kind, allocation);
        let turns = inputs.readings.turns(kind, allocation);
        let target_turns = order.target as f32;
        let below = turns < target_turns;
        let want = if draw > NOTHING {
            let deficit_units = if below {
                (target_turns - turns) * draw
            } else {
                NOTHING
            };
            draw + deficit_units / refill
        } else {
            NOTHING
        };
        let floor = if below {
            best
        } else {
            sources.labor.band_orders.learning_floor
        };

        // What every row that is not this order's chief row brought in of this product LAST TURN
        // (`last_yields`, the one-turn lag `material_draw` accepts): a row the band was too short of
        // hands to staff brought in nothing, and counting its site's sustainable take would leave the
        // other order short.
        let mut incoming = NOTHING;
        for (index, row) in allocation.assignments.iter().enumerate() {
            if matches!(row.target, LaborTarget::Extract { .. }) {
                continue;
            }
            let Some(key) = source_key(&row.target) else {
                continue;
            };
            if allocation.order_rows.get(&key).map(String::as_str) == Some(kind) {
                continue;
            }
            let Some(yields) = allocation.last_yields.get(index) else {
                continue;
            };
            incoming += if kind == FOOD_KIND {
                yields.actual
            } else {
                yields.fodder
            };
        }
        let mut want_left = (want - incoming).max(NOTHING);
        let mut draw_left = draw.min(want_left);

        let mut ranked: Vec<&Candidate> = candidates
            .iter()
            .filter(|candidate| candidate.order == kind)
            .filter(|candidate| {
                // A source the player already works (or the other order does) is counted above.
                !allocation.assignments.iter().any(|row| {
                    source_key(&row.target).is_some_and(|key| key == candidate.key)
                        && allocation
                            .order_rows
                            .get(&candidate.key)
                            .map(String::as_str)
                            != Some(kind)
                })
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.units_per_hand
                .partial_cmp(&a.units_per_hand)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.distance.cmp(&b.distance))
                .then(a.key.cmp(&b.key))
        });
        let mut set: Vec<&Candidate> = Vec::new();
        let mut shares: Vec<(String, KeepShare)> = Vec::new();
        for candidate in ranked {
            if want_left <= NOTHING {
                break;
            }
            let units = candidate.sustainable_units.min(want_left);
            let hold_units = units.min(draw_left);
            shares.push((
                kind.to_string(),
                KeepShare {
                    tile: candidate.tile,
                    herd: candidate.herd.clone(),
                    hold: hold_units / candidate.rate.max(f32::EPSILON),
                    take: units / candidate.rate.max(f32::EPSILON),
                },
            ));
            want_left -= units;
            draw_left -= hold_units;
            set.push(candidate);
        }
        allocation.keep_shares.extend(shares);

        // The chief's existing rows of this order: in the set they stay, improvements and pinned
        // rows stay at no share, the rest go.
        let listed: Vec<String> = allocation
            .order_rows
            .iter()
            .filter(|(_, listed)| listed.as_str() == kind)
            .map(|(key, _)| key.clone())
            .collect();
        for key in listed {
            let Some(target) = allocation
                .assignments
                .iter()
                .find(|row| source_key(&row.target).is_some_and(|held| held == key))
                .map(|row| row.target.clone())
            else {
                allocation.order_rows.remove(&key);
                continue;
            };
            let in_set = set.iter().any(|candidate| candidate.key == key);
            let improvement = match &target {
                LaborTarget::Forage { tile, .. } => sources
                    .forage
                    .patch(*tile)
                    .is_some_and(|patch| patch.ladder_position() > NOTHING),
                LaborTarget::Hunt { fauna_id, .. } => sources
                    .herds
                    .find(fauna_id)
                    .is_some_and(|herd| herd.is_corralled() || herd.ladder_position() > NOTHING),
                _ => false,
            };
            if !in_set && !improvement && !source_pinned(allocation, &target) {
                allocation.drop_source_row(&target);
                allocation.order_rows.remove(&key);
            }
        }
        // A row for each set site that lacks one.
        for candidate in &set {
            let held = allocation
                .assignments
                .iter()
                .any(|row| source_key(&row.target).is_some_and(|key| key == candidate.key));
            if !held {
                let mut target = candidate.target.clone();
                set_floor(&mut target, floor);
                allocation.set_staffed_row(target, None, None);
            }
            allocation
                .order_rows
                .entry(candidate.key.clone())
                .or_insert_with(|| kind.to_string());
        }
        // The chief's rows: the order's priority and floor, the whole basket, no Max.
        let mine: Vec<String> = allocation
            .order_rows
            .iter()
            .filter(|(_, listed)| listed.as_str() == kind)
            .map(|(key, _)| key.clone())
            .collect();
        for row in allocation.assignments.iter_mut() {
            let Some(key) = source_key(&row.target) else {
                continue;
            };
            if !mine.contains(&key) {
                continue;
            }
            set_floor(&mut row.target, floor);
            if let LaborTarget::Forage { take_species, .. } = &mut row.target {
                *take_species = TakeSelection::EVERYTHING;
            }
            row.priority = priority;
            row.max_workers = None;
        }
        // Published: what the order's sources give a turn at the floor.
        let total: f32 = candidates
            .iter()
            .filter(|candidate| candidate.order == kind)
            .map(|candidate| candidate.sustainable_units)
            .sum();
        allocation
            .keep_sustainable
            .insert(kind.to_string(), total + incoming);
        if let Some(best_site) = set.first() {
            allocation
                .last_keep_site
                .insert(kind.to_string(), best_site.tile);
        }
    }
}

fn set_floor(target: &mut LaborTarget, new_floor: f32) {
    match target {
        LaborTarget::Forage { floor, .. } | LaborTarget::Hunt { floor, .. } => *floor = new_floor,
        _ => {}
    }
}
