//! ⛔ **A MINED MATERIAL IS KEPT ON HAND, NOT CREWED** (#874, `docs/plan_labor_at_scale.md` → "Two
//! interfaces: what grows back, and what is mined").
//!
//! A *mined* material is one whose `extraction.json` deposit row names the config's `mined_branch`
//! (`extraction`: stone today; an ore is a config row). For it the band holds a **keep-on-hand target**
//! ([`crate::components::KeepOnHand`]) instead of crews: `0` is no order, anything above is the stock
//! the band wants in its store. The sim owns the dig row — one per band per mined material, on a site
//! the game picks — and staffs it from the deficit ([`crate::staffing`]'s `extract_input`).
//!
//! This module owns the three things that are not staffing: the classification
//! ([`is_mined`]), the site the game picks ([`manage_dig_rows`]) and the refusal the player-facing
//! crew commands share ([`crewed_dig_refusal`]).
//!
//! # The cargo rule: a row never moves or goes while anything of it is out
//!
//! A row is **pinned** ([`row_is_pinned`]) while its party is out, a walk carries its cargo home or a
//! build is declared on its site. A pinned row stays where it is: it is staffed to nobody when the
//! order is gone, its party walks home through the ordinary cut path, and the next pass that finds it
//! unpinned moves or removes it. No stand-down code runs here, so there is no path that strands a load.

use bevy::math::UVec2;

use crate::components::{
    BandEquipment, BuildSource, LaborAllocation, LaborTarget, DEFAULT_ESCAPEMENT_FLOOR,
    STRIP_IT_BARE,
};
use crate::extraction::{
    deposit_payoff, deposit_reachable, tile_deposit_capacity, tile_deposit_regrowth,
    working_default_kit, DEPOSIT_EMPTY, NO_DEPOSIT_GEAR,
};
use crate::extraction_config::{ExtractionConfig, NEVER_RENEWS, NO_DEPOSIT};
use crate::staffing::CampSite;
use crate::take_claims::ClaimSources;

/// How many leading characters of a material id are upper-cased for a sentence ("stone" -> "Stone").
const CAPITALISED_CHARACTERS: usize = 1;

/// **IS THIS MATERIAL MINED** - its deposit row names the config's `mined_branch`. Wood (`forestry`)
/// regrows and keeps its per-site rows, Max and floor.
pub fn is_mined(config: &ExtractionConfig, material: &str) -> bool {
    config
        .deposit(material)
        .is_some_and(|def| def.branch == config.mined_branch)
}

/// Every mined material, in id order.
pub fn mined_materials(config: &ExtractionConfig) -> Vec<String> {
    config
        .materials_on(config.mined_branch)
        .map(|(id, _)| id.to_string())
        .collect()
}

/// A material id as a sentence's noun phrase ("stone" -> "Stone").
fn display_name(material: &str) -> String {
    let (head, tail) = material.split_at(material.len().min(CAPITALISED_CHARACTERS));
    format!("{}{}", head.to_uppercase(), tail)
}

/// **THE REFUSAL FOR A CREW COMMAND ON A MINED MATERIAL** (`assign_labor extract`, `stop_labor`,
/// `work_priority`): the dig is the sim's, set through `keep_on_hand`.
pub fn crewed_dig_refusal(config: &ExtractionConfig, material: &str) -> Option<String> {
    is_mined(config, material).then(|| {
        let name = display_name(material);
        format!("{name} is kept on hand, not crewed - set how much to keep on the {name} sheet.")
    })
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

/// **KEEP THE BAND'S DIG ROWS WHERE THE ORDERS SAY** - run before staffing, by the turn and by every
/// command restaff. For each mined material: publish the best site; with a target, create the row on
/// it or move the row to a strictly better site (or off a worked-out one); without, remove it. A
/// pinned row stays put ([`row_is_pinned`]). The row's priority is the order's.
pub fn manage_dig_rows(
    allocation: &mut LaborAllocation,
    inputs: &DigSiteInputs<'_>,
    candidate_tiles: &dyn Fn(&str) -> Vec<UVec2>,
) {
    allocation.last_keep_site.clear();
    for material in mined_materials(inputs.sources.extraction) {
        let order = allocation.keep_on_hand_for(&material);
        let best = candidate_tiles(&material)
            .into_iter()
            .filter_map(|tile| rank_site(inputs, tile, &material))
            .reduce(|best, next| if next.beats(&best) { next } else { best });
        if let Some(best) = best {
            allocation
                .last_keep_site
                .insert(material.clone(), best.tile);
        }
        let current = allocation
            .assignments
            .iter()
            .find_map(|row| match &row.target {
                LaborTarget::Extract {
                    tile,
                    material: held,
                    ..
                } if *held == material => Some((*tile, row.target.clone())),
                _ => None,
            });
        let wanted = if order.target > 0 { best } else { None };
        let mut keep_current = current.is_some();
        if let Some((tile, target)) = &current {
            let stays = wanted.is_some_and(|best| {
                rank_site(inputs, *tile, &material).is_some_and(|here| !best.beats(&here))
            });
            if !stays && !row_is_pinned(allocation, target) {
                allocation.drop_source_row(target);
                keep_current = false;
            }
        }
        if let (false, Some(best)) = (keep_current, wanted) {
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
        for row in allocation.assignments.iter_mut() {
            if matches!(&row.target, LaborTarget::Extract { material: held, .. } if *held == material)
            {
                row.priority = order.priority;
            }
        }
    }
}
