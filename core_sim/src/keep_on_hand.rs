//! ⛔ **AN ORDERED MATERIAL IS KEPT ON HAND BY THE CHIEF, OR CREWED BY THE PLAYER** (#860 / #874,
//! `docs/plan_labor_at_scale.md` -> "Band orders - the chief").
//!
//! # ⛔ AN ORDER BELONGS TO A BAND WITH A LEADER
//!
//! Before a leader the player crews work directly. When a leader arrives, the first managing pass
//! gives the band an order for every kind `labor_config.band_orders.ordered_kinds` lists that it has
//! none for: owner [`OrderOwner::Chief`], at `starting_target[kind]`, priority Normal, and one feed
//! line per band says so ([`LaborAllocation::unannounced_takeover`], drained by
//! `leadership::announce_chief_takeover`). There is no "none": a target is always above zero, and who
//! works the kind is the order's owner. The chief's rows are the ones [`LaborAllocation::order_rows`] lists
//! under the order, for a material as for food - a row not listed is the player's whoever owns the
//! order, hand commands on it work, and what it brings in counts toward the order's want. A hand
//! command on a LISTED row is refused ([`order_refusal`]). *Crew it yourself* ([`release_order`])
//! hands every listed row to the player where it stands (one stand: [`release_row`]); *Hand it to the
//! chief* ([`claim_order`]) takes every row of the kind back over (one stand: [`claim_row`]). An order the player released stays the player's -
//! arrival only creates missing orders.
//!
//! The sim owns the chief's dig row - one per band per ordered material, on a site the game picks -
//! and staffs it from the deficit ([`crate::staffing`]'s `extract_input`).
//!
//! This module owns what is not staffing: the site the game picks ([`manage_dig_rows`]), the three
//! order transitions and the refusals the commands share.
//!
//! # The cargo rule: a row never moves or goes while anything of it is out
//!
//! A row is **pinned** ([`row_is_pinned`]) while its party is out, a walk carries its cargo home or a
//! build is declared on its site. A pinned row stays where it is until the next pass that finds it
//! unpinned moves or removes it. A release removes no row, so nothing is stranded there either.

use bevy::math::UVec2;

use crate::components::{
    BandEquipment, BandOrder, BuildSource, LaborAllocation, LaborTarget, OrderOwner,
    SourcePriority, DEFAULT_ESCAPEMENT_FLOOR, STRIP_IT_BARE,
};
use crate::extraction::{
    deposit_payoff, deposit_reachable, tile_deposit_capacity, tile_deposit_regrowth,
    working_default_kit, DEPOSIT_EMPTY, NO_DEPOSIT_GEAR,
};
use crate::extraction_config::{ExtractionConfig, NEVER_RENEWS, NO_DEPOSIT};
use crate::leadership::{BandLeadership, LeaderKind};
use crate::staffing::CampSite;
use crate::take_claims::ClaimSources;

/// The kind id of the food order: turns of food on hand over what the band eats in a turn.
pub const FOOD_KIND: &str = "food";
/// The kind id of the fodder order: turns of hay on hand over what the band's pens need.
pub const FODDER_KIND: &str = "fodder";

/// Whether `kind` is carried out by forage and hunt rows rather than by dig rows.
pub fn is_row_kind(kind: &str) -> bool {
    kind == FOOD_KIND || kind == FODDER_KIND
}

/// **A ROW'S IDENTITY** for [`LaborAllocation::order_rows`]: `f:<x>,<y>` for a forage patch,
/// `h:<herd id>` for a herd, `d:<material>:<x>,<y>` for a working (a dig row of one material on one
/// tile). `None` for any other row.
pub fn source_key(target: &LaborTarget) -> Option<String> {
    match target {
        LaborTarget::Forage { tile, .. } => Some(format!("f:{},{}", tile.x, tile.y)),
        LaborTarget::Hunt { fauna_id, .. } => Some(format!("h:{fauna_id}")),
        LaborTarget::Extract { tile, material, .. } => Some(dig_key(material, tile.x, tile.y)),
        _ => None,
    }
}

/// The [`source_key`] of the working of `material` at `(x, y)`.
pub fn dig_key(material: &str, x: u32, y: u32) -> String {
    format!("d:{material}:{x},{y}")
}

/// **WHO KEEPS A BAND'S ORDERS, AS THE REFUSALS AND THE FEED NAME THEM**: the chief by name, or the
/// band's elders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keeper {
    /// "Orrin" or "Bitterbrook's elders".
    pub subject: String,
    /// Whether the subject takes the plural verb ("keep", "take").
    pub plural: bool,
}

impl Keeper {
    /// The keeper of a band with a leader, `None` for a leaderless band. `band_name` is the band's
    /// own name (a council is named for it).
    pub fn of(leadership: Option<&BandLeadership>, band_name: &str) -> Option<Self> {
        let leader = leadership?.leader.as_ref()?;
        Some(match leader.kind {
            LeaderKind::Chief => Self {
                subject: leader.name.clone(),
                plural: false,
            },
            LeaderKind::ElderCouncil => Self {
                subject: format!("{band_name}'s elders"),
                plural: true,
            },
        })
    }

    /// `verb` conjugated for the subject: "keeps" for a chief, "keep" for the elders.
    fn verb(&self, singular: &str, plural: &str) -> String {
        if self.plural { plural } else { singular }.to_string()
    }
}

/// **THE REFUSAL FOR A HAND COMMAND ON A KIND A CHIEF KEEPS ON HAND** (`assign_labor extract`,
/// `stop_labor`, `work_priority`), naming the keeper. `None` when the band crews the kind by hand.
pub fn order_refusal(
    allocation: &LaborAllocation,
    kind: &str,
    keeper: Option<&Keeper>,
) -> Option<String> {
    if !allocation.chief_owns(kind) {
        return None;
    }
    let keeper = keeper?;
    Some(format!(
        "{} {} {kind} on hand - choose Crew it yourself to work it by hand.",
        keeper.subject,
        keeper.verb("keeps", "keep")
    ))
}

/// **THE REFUSAL FOR AN ORDER ON A BAND WITH NO LEADER.**
pub fn leaderless_refusal(kind: &str, band_label: &str) -> String {
    format!("{band_label} has no leader yet - crew {kind} by hand.")
}

/// **THE FEED LINE FOR A CHIEF'S ARRIVAL**: what he takes over and keeps. `taken` is the kind and
/// its opening target, for every kind the arrival created an order for.
pub fn takeover_line(keeper: &Keeper, band_name: &str, taken: &[(String, u32)]) -> String {
    // Food and fodder read in turns, a material as an amount; the turns come first.
    let mut ordered: Vec<&(String, u32)> = taken.iter().collect();
    ordered.sort_by_key(|(kind, _)| !is_row_kind(kind));
    let parts: Vec<String> = ordered
        .iter()
        .map(|(kind, target)| {
            if is_row_kind(kind) {
                format!("{target} turns of {kind}")
            } else {
                format!("{target} {kind}")
            }
        })
        .collect();
    let (last, rest) = parts
        .split_last()
        .map_or((String::new(), &[][..]), |(last, rest)| {
            (last.clone(), rest)
        });
    let keeps = if rest.is_empty() {
        last
    } else {
        format!("{} and {last}", rest.join(", "))
    };
    let keep = keeper.verb("keeps", "keep");
    if keeper.plural {
        format!(
            "{} take over the band's work: they {keep} {keeps} on hand.",
            keeper.subject
        )
    } else {
        format!(
            "{} is chief of {band_name} \u{2014} he takes over the band's work: he {keep} {keeps} on hand.",
            keeper.subject
        )
    }
}

/// **WHY A `band_order ... set` IS REFUSED**, or `None` when it stands: a kind that takes no order
/// and a target of zero (there is no "none" - the owner says who works the kind).
pub fn set_refusal(ordered: bool, kind: &str, target: u32) -> Option<String> {
    if !ordered {
        return Some(format!(
            "{kind} takes no order - it is crewed by hand with assign_labor."
        ));
    }
    (target == 0).then(|| {
        format!(
            "A {kind} order needs a target above zero - choose Crew it yourself to work it by hand."
        )
    })
}

/// **SET THE TARGET AND PRIORITY OF A KIND'S ORDER.** A kind with no order yet starts as the
/// chief's, and adopts the band's existing rows of it as his arrival would; an existing order keeps
/// its owner.
pub fn set_order(
    allocation: &mut LaborAllocation,
    kind: &str,
    target: u32,
    priority: SourcePriority,
) {
    if !allocation.orders.contains_key(kind) {
        allocation.adopt_pending.insert(kind.to_string());
    }
    allocation
        .orders
        .entry(kind.to_string())
        .and_modify(|order| {
            order.target = target;
            order.priority = priority;
        })
        .or_insert(BandOrder {
            target,
            priority,
            owner: OrderOwner::Chief,
        });
}

/// **CREW IT YOURSELF**: the order's owner becomes the player and the chief's rows of `kind` stay
/// where they stand and become the player's. Each row's Max is the crew it was staffed with (at
/// least one, so a never-renewing dig keeps the count the hand-crew rule needs); its priority is
/// kept. No row is removed, so the cargo rule is not involved. A no-op where the player already
/// owns it or no order exists.
pub fn release_order(allocation: &mut LaborAllocation, kind: &str) {
    let Some(order) = allocation.orders.get_mut(kind) else {
        return;
    };
    if order.owner == OrderOwner::Player {
        return;
    }
    order.owner = OrderOwner::Player;
    let keys: Vec<String> = allocation
        .order_rows
        .iter()
        .filter(|(_, listed)| listed.as_str() == kind)
        .map(|(key, _)| key.clone())
        .collect();
    for key in keys {
        release_row(allocation, &key);
    }
}

/// **CREW THIS ROW YOURSELF** (one row of any order, the kind staying the chief's): the row leaves
/// [`LaborAllocation::order_rows`], its Max is the crew it was staffed with (at least one), and its
/// floor and selection stay. A no-op for a row the chief does not list.
pub fn release_row(allocation: &mut LaborAllocation, key: &str) {
    if allocation.order_rows.remove(key).is_none() {
        return;
    }
    for row in allocation.assignments.iter_mut() {
        if source_key(&row.target).is_some_and(|held| held == key) {
            row.max_workers = Some(row.workers.max(MIN_RELEASED_CREW));
        }
    }
}

/// **HAND THIS ROW BACK TO THE CHIEF**: list the row under the `kind` order; the next managing pass
/// clears its Max and sets its floor and selection. A no-op where the band holds no such row.
pub fn claim_row(allocation: &mut LaborAllocation, key: &str, kind: &str) {
    let held = allocation
        .assignments
        .iter()
        .any(|row| source_key(&row.target).is_some_and(|held| held == key));
    if held {
        allocation
            .order_rows
            .insert(key.to_string(), kind.to_string());
    }
}

/// The fewest hands a released row's Max can name: a never-renewing dig is staffed by its count,
/// and a count of zero is no crew.
const MIN_RELEASED_CREW: u32 = 1;

/// **HAND IT TO THE CHIEF**: the order's owner becomes the chief at its current target (or
/// `starting_target` where it never had one) and the next managing pass takes over every row of the
/// kind exactly as it does on his arrival.
pub fn claim_order(allocation: &mut LaborAllocation, kind: &str, starting_target: u32) {
    allocation
        .orders
        .entry(kind.to_string())
        .and_modify(|order| order.owner = OrderOwner::Chief)
        .or_insert(BandOrder {
            target: starting_target,
            priority: SourcePriority::default(),
            owner: OrderOwner::Chief,
        });
    allocation.adopt_pending.insert(kind.to_string());
}

/// **THE FLOOR AN `extract` ROW CARRIES WHEN NOBODY NAMED ONE** - nothing is left standing on a finite
/// body (the player's floor protects regrowth, and rock has none), the shared default on a deposit
/// that renews.
fn dig_floor(ground_rate: f32) -> f32 {
    if ground_rate > NEVER_RENEWS {
        DEFAULT_ESCAPEMENT_FLOOR
    } else {
        STRIP_IT_BARE
    }
}

/// **THE DISCOVERED TILES THAT HOLD `material`** - the fog gate `deposit_states` publishes by.
pub fn discovered_deposit_tiles<'a>(
    config: &ExtractionConfig,
    material: &str,
    faction: crate::orders::FactionId,
    visibility: Option<&crate::visibility::VisibilityLedger>,
    fog_enabled: bool,
    tiles: impl Iterator<Item = &'a crate::components::Tile>,
) -> Vec<UVec2> {
    tiles
        .filter(|tile| tile_deposit_capacity(config, material, tile) > NO_DEPOSIT)
        .filter(|tile| {
            !fog_enabled
                || visibility.is_some_and(|ledger| {
                    ledger.is_discovered(faction, tile.position.x, tile.position.y)
                })
        })
        .map(|tile| tile.position)
        .collect()
}

/// **A SITE'S STANDING IN THE RANKING** - most per worker, then the shortest walk, then tile order.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SiteRank {
    tile: UVec2,
    per_hand: f32,
    distance: u32,
    /// The floor the chief works this site at: Best where it regrows, strip-it-bare where it never does.
    floor: f32,
    /// What the site gives a turn at that floor without drawing it down - unbounded for a deposit
    /// that never regrows.
    sustainable: f32,
}

impl SiteRank {
    /// Whether `self` ranks strictly ahead of `other`.
    fn beats(&self, other: &SiteRank) -> bool {
        if self.per_hand != other.per_hand {
            return self.per_hand > other.per_hand;
        }
        if self.distance != other.distance {
            return self.distance < other.distance;
        }
        (self.tile.y, self.tile.x) < (other.tile.y, other.tile.x)
    }
}

/// What a site that never regrows gives "without drawing it down": there is nothing to draw it
/// down from, so its share is whatever the want is.
const UNBOUNDED_SITE: f32 = f32::INFINITY;
/// No take.
const NO_TAKE: f32 = 0.0;

/// What the ranking reads: the claim sources, the band's gear and where it camps.
pub struct DigSiteInputs<'a> {
    pub sources: &'a ClaimSources<'a>,
    pub band_kit: &'a BandEquipment,
    pub camp: Option<&'a CampSite>,
}

/// The rank of digging `material` at `tile`, `None` where nothing is reachable there.
fn rank_site(inputs: &DigSiteInputs<'_>, tile: UVec2, material: &str) -> Option<SiteRank> {
    let sources = inputs.sources;
    let (working, ground) = sources.working(tile, material)?;
    let payoff = deposit_payoff(working.standing(), sources.ladder);
    let capacity = tile_deposit_capacity(sources.extraction, material, &ground);
    let ground_rate = tile_deposit_regrowth(sources.extraction, material, &ground);
    let reachable = deposit_reachable(
        working.stock,
        capacity,
        ground_rate,
        &payoff,
        dig_floor(ground_rate),
    );
    if reachable <= DEPOSIT_EMPTY {
        return None;
    }
    let equipment = sources.equipment;
    let rung = working.rung();
    let rung_key = rung.wire_key();
    let best_kit = equipment
        .deposit_kits_for(rung.branch)
        .iter()
        .filter(|kit| {
            kit.uses()
                .all(|item| kit.item_live(item, inputs.band_kit, equipment))
        })
        .map(|kit| {
            equipment.deposit_take_per_worker(kit, inputs.band_kit, rung.branch, Some(&rung_key))
        })
        .fold(NO_DEPOSIT_GEAR, f32::max);
    let floor = dig_floor(ground_rate);
    let sustainable = if ground_rate > NEVER_RENEWS {
        let rate = ground_rate * payoff.regrowth_multiplier;
        let floor_stock =
            crate::extraction::deposit_effective_floor(capacity, ground_rate, &payoff, floor);
        (crate::extraction::deposit_regrowth(
            floor_stock,
            capacity,
            rate,
            sources.extraction.seed_fraction,
        ) - floor_stock)
            .max(NO_TAKE)
    } else {
        UNBOUNDED_SITE
    };
    Some(SiteRank {
        tile,
        per_hand: payoff.yield_per_worker_turn + best_kit,
        distance: inputs.camp.map_or(0, |camp| camp.distance_to(tile)),
        floor,
        sustainable,
    })
}

/// **IS THE ROW PINNED** - its party is out, a walk carries its cargo home, or a build is declared on
/// its site. A pinned row neither moves nor goes.
fn row_is_pinned(allocation: &LaborAllocation, target: &LaborTarget) -> bool {
    let LaborTarget::Extract { tile, material, .. } = target else {
        return false;
    };
    let party_out = allocation
        .assignments
        .iter()
        .any(|row| row.target.same_source(target) && row.party.is_some());
    let cargo_walking = allocation
        .homeward
        .iter()
        .any(|walk| walk.target.same_source(target));
    let build_declared = allocation.builds.iter().any(|build| {
        matches!(
            &build.source,
            BuildSource::Deposit { tile: at, material: held } if at == tile && held == material
        )
    });
    party_out || cargo_walking || build_declared
}

/// **KEEP A CHIEF'S DIG ROWS WHERE HIS ORDERS SAY** - run before staffing, by the turn and by every
/// command restaff.
///
/// - **arrival**: a band with a leader gets an order (owner Chief, `starting_target`, Normal) for
///   every ordered kind it has none for, and the kinds are queued for the feed line.
/// - **per chief-owned material**: the order's **want** is the band's draw plus the deficit spread
///   over `keep_on_hand_refill_turns` (zero at or above the target). The candidate sites are ranked
///   ([`SiteRank`]: most per hand, then the shortest walk, then tile order) and each gets a share, in
///   that order, of up to what it gives a turn at the order's floor without drawing it down, until
///   the want is covered; the best site is always in the set, so a band at target keeps its row. A
///   deposit that never regrows gives without bound, so it ends with one row. A row is created on each
///   set site that lacks one, a row on a set site stays, and a row on any other site is retired. Every
///   row takes the order's priority, loses its Max and is worked at no lower than the site's floor.
///   The shares ([`LaborAllocation::keep_shares`]) are what staffing reads, and the total the sites
///   give is [`LaborAllocation::keep_sustainable`].
/// - a kind the player owns, and a leaderless band, are not touched.
///
/// A pinned row ([`row_is_pinned`]) is never moved or removed; off the set it is staffed to nobody
/// until the pass that finds it unpinned.
pub fn manage_dig_rows(
    allocation: &mut LaborAllocation,
    inputs: &DigSiteInputs<'_>,
    has_leader: bool,
    candidate_tiles: &dyn Fn(&str) -> Vec<UVec2>,
    on_hand: &dyn Fn(&str) -> f32,
) {
    allocation.last_keep_site.clear();
    allocation.keep_shares.clear();
    allocation.keep_sustainable.clear();
    if !has_leader {
        return;
    }
    let band_orders = &inputs.sources.labor.band_orders;
    for kind in &band_orders.ordered_kinds {
        if allocation.orders.contains_key(kind) {
            continue;
        }
        let Some(target) = band_orders.starting_target_for(kind) else {
            continue;
        };
        allocation.orders.insert(
            kind.clone(),
            BandOrder {
                target,
                priority: SourcePriority::default(),
                owner: OrderOwner::Chief,
            },
        );
        allocation.unannounced_takeover.push(kind.clone());
        allocation.adopt_pending.insert(kind.clone());
    }
    let chief_materials: Vec<(String, BandOrder)> = allocation
        .orders
        .iter()
        .filter(|(kind, order)| {
            order.owner == OrderOwner::Chief && inputs.sources.extraction.deposit(kind).is_some()
        })
        .map(|(kind, order)| (kind.clone(), *order))
        .collect();
    for (material, order) in chief_materials {
        // Adoption: a chief's arrival or a whole-kind claim lists every existing dig row of the
        // material under the order.
        if allocation.adopt_pending.remove(&material) {
            let keys: Vec<String> = allocation
                .assignments
                .iter()
                .filter(|row| {
                    matches!(&row.target, LaborTarget::Extract { material: held, .. } if *held == material)
                })
                .filter_map(|row| source_key(&row.target))
                .collect();
            for key in keys {
                allocation.order_rows.insert(key, material.clone());
            }
        }
        // A listed key whose row is gone is dropped from the roster.
        let stale: Vec<String> = allocation
            .order_rows
            .iter()
            .filter(|(key, listed)| {
                listed.as_str() == material
                    && !allocation
                        .assignments
                        .iter()
                        .any(|row| source_key(&row.target).as_deref() == Some(key.as_str()))
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            allocation.order_rows.remove(&key);
        }
        let is_listed = |allocation: &LaborAllocation, target: &LaborTarget| {
            allocation.row_order(target) == Some(material.as_str())
        };
        // The player's stands of this material: a site the player crews is not a candidate.
        let player_sites: Vec<UVec2> = allocation
            .assignments
            .iter()
            .filter_map(|row| match &row.target {
                LaborTarget::Extract {
                    tile,
                    material: held,
                    ..
                } if *held == material && !is_listed(allocation, &row.target) => Some(*tile),
                _ => None,
            })
            .collect();
        let mut ranked: Vec<SiteRank> = candidate_tiles(&material)
            .into_iter()
            .filter(|tile| !player_sites.contains(tile))
            .filter_map(|tile| rank_site(inputs, tile, &material))
            .collect();
        ranked.sort_by(|a, b| {
            if a.beats(b) {
                std::cmp::Ordering::Less
            } else if b.beats(a) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        if let Some(best) = ranked.first() {
            allocation
                .last_keep_site
                .insert(material.clone(), best.tile);
        }
        // What the player's stands of the material brought in LAST TURN (`last_yields`, the one-turn
        // lag `material_draw` accepts), subtracted from the want before the chief's sites are dealt.
        let incoming: f32 = allocation
            .assignments
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                matches!(&row.target, LaborTarget::Extract { material: held, .. } if *held == material)
                    && !is_listed(allocation, &row.target)
            })
            .filter_map(|(index, _)| allocation.last_yields.get(index))
            .flat_map(|yields| yields.materials.iter())
            .filter(|payoff| payoff.material == material)
            .map(|payoff| payoff.amount)
            .sum();
        // The want: what the band uses plus a tenth of the gap, nothing at or above the target.
        let draw = allocation.material_draw(&material);
        let deficit = order.target as f32 - on_hand(&material);
        let gross_want = if deficit > NO_TAKE {
            draw + deficit / inputs.sources.labor.keep_on_hand_refill_turns as f32
        } else {
            NO_TAKE
        };
        let want = (gross_want - incoming).max(NO_TAKE);
        let mut want_left = want;
        let mut draw_left = if deficit > NO_TAKE {
            draw.min(want)
        } else {
            NO_TAKE
        };
        let mut set: Vec<SiteRank> = Vec::new();
        for site in &ranked {
            if !set.is_empty() && want_left <= NO_TAKE {
                break;
            }
            let take = site.sustainable.min(want_left);
            let hold = take.min(draw_left);
            allocation.keep_shares.push((
                material.clone(),
                crate::components::KeepShare {
                    tile: site.tile,
                    herd: String::new(),
                    hold,
                    take,
                },
            ));
            want_left -= take;
            draw_left -= hold;
            set.push(*site);
        }
        let unbounded = ranked.iter().any(|site| !site.sustainable.is_finite());
        let sustainable = if unbounded {
            gross_want.max(draw)
        } else {
            ranked.iter().map(|site| site.sustainable).sum::<f32>() + incoming
        };
        allocation
            .keep_sustainable
            .insert(material.clone(), sustainable);

        // The chief's rows on a site of the set stay; a listed row anywhere else is retired unless it
        // is pinned. A row the player crews is never touched.
        let listed_rows: Vec<(UVec2, LaborTarget)> = allocation
            .assignments
            .iter()
            .filter(|row| is_listed(allocation, &row.target))
            .filter_map(|row| match &row.target {
                LaborTarget::Extract { tile, .. } => Some((*tile, row.target.clone())),
                _ => None,
            })
            .collect();
        let mut held_sites: Vec<UVec2> = Vec::new();
        for (tile, target) in listed_rows {
            if set.iter().any(|site| site.tile == tile) && !held_sites.contains(&tile) {
                held_sites.push(tile);
            } else if !row_is_pinned(allocation, &target) {
                if let Some(key) = source_key(&target) {
                    allocation.order_rows.remove(&key);
                }
                allocation.drop_source_row(&target);
            }
        }
        for site in &set {
            if held_sites.contains(&site.tile) {
                continue;
            }
            if let Some((working, _)) = inputs.sources.working(site.tile, &material) {
                let kit = working_default_kit(inputs.sources.equipment, &working);
                let target = LaborTarget::Extract {
                    tile: site.tile,
                    material: material.clone(),
                    floor: site.floor,
                };
                let key = source_key(&target);
                allocation.set_staffed_row(target, None, Some(kit));
                if let Some(key) = key {
                    allocation.order_rows.insert(key, material.clone());
                }
            }
        }
        let mine: Vec<String> = allocation
            .order_rows
            .iter()
            .filter(|(_, listed)| listed.as_str() == material)
            .map(|(key, _)| key.clone())
            .collect();
        for row in allocation.assignments.iter_mut() {
            let Some(key) = source_key(&row.target) else {
                continue;
            };
            if !mine.contains(&key) {
                continue;
            }
            if let LaborTarget::Extract { tile, floor, .. } = &mut row.target {
                // **The chief never overdraws**: no row of his is worked below the site's floor.
                if let Some(site) = set.iter().find(|site| site.tile == *tile) {
                    *floor = floor.max(site.floor);
                }
            }
            row.priority = order.priority;
            row.max_workers = None;
        }
    }
}
