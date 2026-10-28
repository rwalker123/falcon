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
//! works the kind is the order's owner. A chief-owned material owns **every** row of that material
//! on the band; hand commands on it are refused ([`order_refusal`]). *Crew it yourself*
//! ([`release_order`]) hands the chief's rows to the player where they stand; *Hand it to the chief*
//! ([`claim_order`]) takes everything back over. An order the player released stays the player's -
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
    let kinds = taken
        .iter()
        .map(|(kind, _)| format!("the {kind} digging"))
        .collect::<Vec<_>>()
        .join(" and ");
    let keeps = match taken {
        [(_, target)] => format!("{target} on hand"),
        _ => {
            taken
                .iter()
                .map(|(kind, target)| format!("{target} {kind}"))
                .collect::<Vec<_>>()
                .join(" and ")
                + " on hand"
        }
    };
    let take = keeper.verb("takes", "take");
    let keep = keeper.verb("keeps", "keep");
    if keeper.plural {
        format!("{} {take} over {kinds} and {keep} {keeps}.", keeper.subject)
    } else {
        format!(
            "{} is chief of {band_name} \u{2014} he {take} over {kinds} and {keep} {keeps}.",
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
/// chief's; an existing order keeps its owner.
pub fn set_order(
    allocation: &mut LaborAllocation,
    kind: &str,
    target: u32,
    priority: SourcePriority,
) {
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
    for row in allocation.assignments.iter_mut() {
        if matches!(&row.target, LaborTarget::Extract { material, .. } if material == kind) {
            row.max_workers = Some(row.workers.max(MIN_RELEASED_CREW));
        }
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
    Some(SiteRank {
        tile,
        per_hand: payoff.yield_per_worker_turn + best_kit,
        distance: inputs.camp.map_or(0, |camp| camp.distance_to(tile)),
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
/// - **per chief-owned material**: publish the best site; create the one row on it, or move it to a
///   strictly better site (or off a worked-out one); retire the band's other rows of that material
///   (a takeover). The rows' priority is the order's and their Max is cleared.
/// - a kind the player owns, and a leaderless band, are not touched.
///
/// A pinned row ([`row_is_pinned`]) is never moved or removed.
pub fn manage_dig_rows(
    allocation: &mut LaborAllocation,
    inputs: &DigSiteInputs<'_>,
    has_leader: bool,
    candidate_tiles: &dyn Fn(&str) -> Vec<UVec2>,
) {
    allocation.last_keep_site.clear();
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
        let best = candidate_tiles(&material)
            .into_iter()
            .filter_map(|tile| rank_site(inputs, tile, &material))
            .reduce(|best, next| if next.beats(&best) { next } else { best });
        if let Some(best) = best {
            allocation
                .last_keep_site
                .insert(material.clone(), best.tile);
        }
        let rows_of_material = |allocation: &LaborAllocation| -> Vec<(UVec2, LaborTarget)> {
            allocation
                .assignments
                .iter()
                .filter_map(|row| match &row.target {
                    LaborTarget::Extract {
                        tile,
                        material: held,
                        ..
                    } if *held == material => Some((*tile, row.target.clone())),
                    _ => None,
                })
                .collect()
        };
        // The row that stays is the first one still at a site the order wants.
        let mut stayed: Option<UVec2> = None;
        for (tile, target) in rows_of_material(allocation) {
            let stays = stayed.is_none()
                && best.is_some_and(|best| {
                    rank_site(inputs, tile, &material).is_some_and(|here| !best.beats(&here))
                });
            if stays {
                stayed = Some(tile);
            } else if !row_is_pinned(allocation, &target) {
                allocation.drop_source_row(&target);
            }
        }
        if let (None, Some(best)) = (stayed, best) {
            let any_left = !rows_of_material(allocation).is_empty();
            if !any_left {
                if let Some((working, ground)) = inputs.sources.working(best.tile, &material) {
                    let floor = dig_floor(tile_deposit_regrowth(
                        inputs.sources.extraction,
                        &material,
                        &ground,
                    ));
                    let kit = working_default_kit(inputs.sources.equipment, &working);
                    allocation.set_staffed_row(
                        LaborTarget::Extract {
                            tile: best.tile,
                            material: material.clone(),
                            floor,
                        },
                        None,
                        Some(kit),
                    );
                }
            }
        }
        for row in allocation.assignments.iter_mut() {
            if matches!(&row.target, LaborTarget::Extract { material: held, .. } if *held == material)
            {
                row.priority = order.priority;
                row.max_workers = None;
            }
        }
    }
}
