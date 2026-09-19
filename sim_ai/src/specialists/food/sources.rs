//! **What a crew can be put to work on** — the source vocabulary `Food` ranks and `Land` shares:
//! a forage patch or a huntable herd, its expected take for a crew, and the two eligibility
//! accessors both specialists must ask the same way.

use sim_runtime::{
    ForagePatchState, HerdTelemetryState, KitOptionState, LaborAssignmentState,
    PopulationCohortState, TerrainTags,
};

use super::ledger::{regrowth_at, BEST_FLOOR};
use super::{Food, ROLE_FORAGE, ROLE_HUNT};
use crate::geometry::Tile;
use crate::oracle::CrewTakeCurve;
use crate::view::{band_tile, row_key, SeatMemory, SeatView};

/// **What one worker would take off `patch` this turn** — the rate the band has realized on that
/// ground, else the web's prior, else the frame's forecast ([`Food::rate`]).
///
/// ⛔ **This, not `carrying_capacity`, is what ground is worth to a crew.** The capacity is the
/// stand's standing biomass `K`; the two disagree by more than 2× on the shipped bench seeds, and in
/// the wrong direction — the richest stand in a neighbourhood can be the worst ground to work. Both
/// `Food` (ranking sources) and `Land` (ranking ground to move onto) read this one quantity, so the
/// two specialists cannot disagree about what a tile pays.
pub(crate) fn patch_per_worker_yield(
    memory: &SeatMemory,
    band: &PopulationCohortState,
    patch: &ForagePatchState,
) -> f32 {
    Food::rate(
        memory,
        band,
        &SourceKey::Patch(Tile::new(patch.x, patch.y)),
        patch.per_worker_yield,
    )
}

/// **A gathering site**: the plant rung's `site_requirement` (`plant_rung_site_refusal`,
/// `core_sim/src/bin/server.rs`) is the tile carrying a food module, and a patch row is published
/// for ground that carries none — a crew sent there is refused *"nobody gathers here"*.
///
/// ⛔ **`Food` AND `Land` MUST ASK THIS THE SAME WAY.** `Food` filters its sources on it, so ground
/// off a food module is ground the band cannot work; `Land` ranking a move on a patch that fails it
/// walks the band onto ground it will then be refused every assignment on, and counts the same patch
/// as *"something better in view"* so the `land_short` alarm never fires. One accessor, both
/// callers — the divergence [`patch_per_worker_yield`] already closed for the rate.
pub(crate) fn is_food_site(view: &SeatView, tile: Tile) -> bool {
    view.snapshot
        .food_modules
        .iter()
        .any(|site| site.x == tile.x && site.y == tile.y)
}

/// **The patch on `tile` a crew could actually be put to work on**: the published forage row, but
/// only where [`is_food_site`] holds. Bare ground and ground off a food module read the same way —
/// as nothing — because a crew takes nothing off either.
pub(crate) fn workable_patch_at(view: &SeatView, tile: Tile) -> Option<&ForagePatchState> {
    view.patch_at(tile).filter(|_| is_food_site(view, tile))
}

/// **The hunting kits `band` could actually send a crew out under** — the roster's hunt-job kits
/// (`WorldSnapshot::kits`, `jobs` includes `hunt`) whose resolved `attack` on *this* band
/// (`PopulationCohortState::kit_tiers`, joined on `kit_id`) is above the bare hand's. The bare
/// hand is read off the same list: the hunt-job kit that carries no items (`equipment.json`'s
/// `none`), whose row resolves to the `creatures.json` `person` attack because there is nothing to
/// add to it. A roster without one, or a cohort with no tiers published, reads `0`.
///
/// **Why the tiers and not the batches.** `BandKitTiersState` is *"the RESOLVED answer. A client
/// must not re-derive it … 'all items dry' keeps it at full tier with only the sled left"*: a kit's
/// `item_ids` says what it carries, not which item is its weapon, so a band holding a sled and no
/// spear counted as armed by the batches and reads bare here — the sim resolved `big_game` to the
/// intrinsic attack because nothing it holds declares one.
///
/// ⛔ **NO KIT, NO HUNT.** `equipment.json`: *"A SPAWNING BAND OWNS NO EQUIPMENT AT ALL … HUNTING
/// YIELDS NOTHING AT ANY CREW SIZE until a spear is crafted"*, and a seat whose loadout grants no
/// hunting kit plays bare-handed (the demand board asks for one only when a herd in reach can be
/// brought down with it — `Food::outfit_demands`; a window closed with nothing applied leaves the
/// band bare, `starting_loadout::close_opening_window`). So a herd is not a source for a band that
/// reads `0` here, whatever the herd is forecast to pay: before this, twelve hands sent to it were
/// rejected next turn as *no useful crew*, every turn, for the first ten-odd turns of every bench
/// seed.
pub(crate) fn hunting_kits_held(view: &SeatView, band: &PopulationCohortState) -> u32 {
    let hunt_kits: Vec<&KitOptionState> = view
        .snapshot
        .kits
        .iter()
        .filter(|kit| kit.jobs.iter().any(|job| job == ROLE_HUNT))
        .collect();
    let attack_under = |kit: &KitOptionState| {
        band.kit_tiers
            .iter()
            .find(|tier| tier.kit_id == kit.id)
            .map(|tier| tier.attack)
    };
    let Some(bare) = hunt_kits
        .iter()
        .filter(|kit| kit.item_ids.is_empty())
        .find_map(|kit| attack_under(kit))
    else {
        return 0;
    };
    hunt_kits
        .iter()
        .filter(|kit| !kit.item_ids.is_empty())
        .filter(|kit| attack_under(kit).is_some_and(|attack| attack > bare))
        .count() as u32
}

/// **The kit a herd is hunted under** — the row's own `HerdTelemetryState::default_kit_id` (*"the
/// one `assign_labor … hunt <herd> <n>` resolves when the player names none"*), else the hunt
/// job's default, `WorldSnapshot::default_hunt_kit_id` — the same fall-back the wire doc names
/// for a herd whose species the roster cannot resolve.
pub(crate) fn herd_kit_id<'v>(view: &'v SeatView, herd: &'v HerdTelemetryState) -> &'v str {
    if herd.default_kit_id.is_empty() {
        &view.snapshot.default_hunt_kit_id
    } else {
        &herd.default_kit_id
    }
}

/// **How many crews `band` can send out under `kit`** — the units of the kit it holds: over the
/// kit's `item_ids` (`KitOptionState`, *"WHICH ITEMS THIS KIT ACTUALLY CARRIES"*), the least of
/// the summed `count` of the band's `equipment_batches` rows for each item, since a party needs
/// every item of the kit. `None` for a kit that carries nothing (`none`): a bare crew is not
/// bounded by gear. `Some(0)` for a kit the roster does not list — nothing to send anyone with.
///
/// ⛔ **A herd's crew is bounded by the kit units held, not by the herd.** The band holding one
/// `big_game` kit read a deer row at `0.8` a hunter and rule 1 multiplied by twelve hands; the
/// sim's own reading on that row was three useful hunters, then one. Hands past the units held
/// hunt with nothing and take nothing.
pub(crate) fn kit_units_held(
    view: &SeatView,
    band: &PopulationCohortState,
    kit_id: &str,
) -> Option<u32> {
    let kit = view.snapshot.kits.iter().find(|kit| kit.id == kit_id);
    let Some(kit) = kit else {
        return Some(0);
    };
    if kit.item_ids.is_empty() {
        return None;
    }
    kit.item_ids
        .iter()
        .map(|item| {
            band.equipment_batches
                .iter()
                .filter(|batch| &batch.item_id == item)
                .map(|batch| batch.count)
                .sum::<u32>()
        })
        .min()
}

/// **The hands on `row` its take did not need** — `workers − workers_needed`, the frame's own
/// overstaffing signal: `LaborAssignmentState::workers_needed` is *"Minimum workers that would
/// have produced this turn's take — the **overstaffing** signal. `workers > workers_needed` ⇒ the
/// binding constraint was not labor, so the extra workers were idle."* They cost nothing to move,
/// because the row's take is what the needed hands bring home.
///
/// `0` when `workers_needed` is `0`: that is the sim's *"the source produced nothing"* and a fresh
/// row the turn has not resolved yet alike, and neither is an overstaffing signal — a row nobody
/// was useful on is *negative income*'s to empty, not surplus to skim.
pub(crate) fn surplus_hands(row: &LaborAssignmentState) -> u32 {
    if row.workers_needed == 0 {
        0
    } else {
        row.workers.saturating_sub(row.workers_needed)
    }
}

/// Whether a band of another faction than `faction` stands on `tile`. **Ground under a rival is
/// not ground to work or walk to**: walking a band into a foreign camp is a contact, and the
/// defection gate (`core_sim/tests/defection_contact_gate.rs`) can hand the whole band over.
pub(crate) fn foreign_band_at(view: &SeatView, faction: u32, tile: Tile) -> bool {
    view.snapshot
        .populations
        .iter()
        .any(|cohort| cohort.faction != faction && band_tile(cohort) == tile)
}

/// **Whether a band may stand on `tile`** — the sim's own rule for a `move_band`, restated:
/// `ensure_land_tile` (`core_sim/src/bin/server.rs`) refuses a tile whose
/// `terrain_tags.contains(TerrainTags::WATER)` as `water_tile`. A tile the frame carries no row
/// for cannot be judged and is not offered.
pub(crate) fn is_walkable(view: &SeatView, tile: Tile) -> bool {
    view.snapshot
        .tiles
        .iter()
        .find(|row| row.x == tile.x && row.y == tile.y)
        .is_some_and(|row| !row.terrain_tags.contains(TerrainTags::WATER))
}

/// Whether a source is dead in this seat's memory — `Food::is_dead`, handed in as a closure so
/// `Land`, which holds none of `Food`'s levers, reads the same cluster with no dead-row judgement.
pub(crate) type IsDead<'a> = dyn Fn(&SourceKey) -> bool + 'a;

/// What a band would take, per turn, from **every** workable site within its `work_range` of a
/// standing tile ([`cluster_take`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClusterTake {
    /// The takes summed over `sites`.
    pub total: f32,
    /// Each site dealt hands: its tile, the hands dealt, and what those hands take there.
    pub sites: Vec<(Tile, u32, f32)>,
}

/// **The hands a site needs at the Best floor's sustained regrowth**:
/// the floor's regrowth in provisions over what one hand takes, rounded up; `0` for a patch whose
/// curve was not sent.
pub(crate) fn sustained_hands(patch: &ForagePatchState, rate: f32) -> u32 {
    let sustained = regrowth_at(&patch.regrowth_samples, BEST_FLOOR) * patch.provisions_per_biomass;
    if rate <= 0.0 || sustained <= 0.0 {
        return 0;
    }
    (sustained / rate).ceil() as u32
}

/// **The food a band could take from every workable site within `work_range` of `standing`** —
/// the reading `Land` ranks a standing tile on and `Food` deals free hands by
/// (`docs/plan_ai_driver.md` §4, *"`Land` positions by the cluster, not the patch"*). A site is a
/// discovered forage patch that is a gathering site ([`workable_patch_at`]), forecast above zero,
/// unowned or the band's own, under no foreign band, and not dead; it is rated by [`Food::rate`]
/// (the realized rate first) with the ceiling [`Source`] carries. `hands` are dealt greedily, the
/// best rate first, each site up to its **plateau** — the smallest crew `n` with `crew_take(n) ==
/// crew_take(n + 1)`, which for `min(n × rate, ceiling)` is `ceil(ceiling / rate)` — and then the
/// next site. `existing(tile)` is the crew already on a site, which counts against its plateau and
/// whose take is not counted again; `None` strikes the site out (a row the hands are leaving).
pub(crate) fn cluster_take_over(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    hands: u32,
    is_dead: &IsDead<'_>,
    existing: &dyn Fn(Tile) -> Option<u32>,
) -> ClusterTake {
    cluster_take_by(
        view,
        memory,
        band,
        standing,
        hands,
        is_dead,
        existing,
        Ceiling::Standing,
    )
}

/// **What a site's take is capped at** when a cluster is dealt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ceiling {
    /// The standing biomass — everything on the ground this turn (`biomass ×
    /// provisions_per_biomass`); the reading `Land` moves on.
    Standing,
    /// The Best floor's regrowth — what the site gives every turn without being drawn down
    /// ([`sustained_hands`]); the reading that says whether ground can *feed* a band, and the
    /// first pass of `Food`'s free-hand deal ([`deal_free_hands`]).
    Sustained,
    /// **The room above the Best floor** — `max(0, biomass − BEST_FLOOR × carrying_capacity) ×
    /// provisions_per_biomass`, what the stand above the floor is worth this turn; the second
    /// pass of the free-hand deal. Its plateau is `ceil(room / rate)` **on top of the sustained
    /// crew**: only the hands above [`sustained_hands`] count against it, so a patch at or below
    /// its floor has no room and takes no hand here. Dealt by the standing stock instead, a
    /// patch stripped to its floor still looked able to use twelve hands (bench seed 19, t6–t9:
    /// seventeen on a patch needing two, then eight "surplus" dealt onto a patch at its floor).
    Surplus,
}

/// The room above the Best floor on `patch`, in provisions — [`Ceiling::Surplus`]'s ceiling.
pub(crate) fn surplus_room(patch: &ForagePatchState) -> f32 {
    (patch.biomass - BEST_FLOOR * patch.carrying_capacity).max(0.0) * patch.provisions_per_biomass
}

/// **What `patch` can give a crew this turn at the Best floor**, in provisions: the room above
/// the floor plus the floor's regrowth — [`Ceiling::Surplus`] and [`Ceiling::Sustained`]
/// summed, the two passes of the free-hand deal. A patch [`Source`]'s `ceiling`, and the cap on
/// what a worked patch row is forecast (`SeatMemory`): a patch standing at its floor expects
/// only its regrowth, which is what it pays. Not the standing stock (`biomass ×
/// provisions_per_biomass`, the take at a zero floor): ranked on that, a patch at its floor read
/// as having room for one more hand while the frame read the hand as surplus, and rule 1
/// shuffled it between 47,5 and 49,5 every turn of seed 23's t45–t52.
pub(crate) fn honest_ceiling(patch: &ForagePatchState) -> f32 {
    surplus_room(patch)
        + regrowth_at(&patch.regrowth_samples, BEST_FLOOR) * patch.provisions_per_biomass
}

#[allow(clippy::too_many_arguments)]
fn cluster_take_by(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    hands: u32,
    is_dead: &IsDead<'_>,
    existing: &dyn Fn(Tile) -> Option<u32>,
    ceiling: Ceiling,
) -> ClusterTake {
    let grid = view.grid();
    // Per site: tile, rate, ceiling, the hands already counted against the plateau, the plateau.
    let mut sites: Vec<(Tile, f32, f32, u32, u32)> = view
        .snapshot
        .forage_patches
        .iter()
        .filter(|patch| patch.per_worker_yield > 0.0)
        .filter(|patch| patch.owner.is_none_or(|owner| owner == band.faction))
        .filter_map(|patch| {
            let tile = Tile::new(patch.x, patch.y);
            let key = SourceKey::Patch(tile);
            (view.is_discovered(tile)
                && grid.distance(standing, tile) <= band.work_range
                && !foreign_band_at(view, band.faction, tile)
                && !is_dead(&key))
            .then(|| workable_patch_at(view, tile))
            .flatten()
            .and_then(|patch| {
                existing(tile).map(|already| {
                    let rate = patch_per_worker_yield(memory, band, patch);
                    let hands_for = |ceiling: f32| {
                        if rate > 0.0 {
                            (ceiling / rate).ceil() as u32
                        } else {
                            0
                        }
                    };
                    let (ceiling, already, plateau) = match ceiling {
                        Ceiling::Standing => {
                            let ceiling = patch.biomass * patch.provisions_per_biomass;
                            // The plateau: `ceil(ceiling / rate)`, the standing biomass included.
                            (ceiling, already, hands_for(ceiling))
                        }
                        Ceiling::Sustained => (
                            regrowth_at(&patch.regrowth_samples, BEST_FLOOR)
                                * patch.provisions_per_biomass,
                            already,
                            sustained_hands(patch, rate),
                        ),
                        Ceiling::Surplus => {
                            let room = surplus_room(patch);
                            // The sustained crew stands first; only the hands above it draw on
                            // the room.
                            let above = already.saturating_sub(sustained_hands(patch, rate));
                            (room, above, hands_for(room))
                        }
                    };
                    (tile, rate, ceiling, already, plateau)
                })
            })
        })
        .filter(|(_, rate, _, _, _)| *rate > 0.0)
        .collect();
    sites.sort_by(|(a_tile, a_rate, _, _, _), (b_tile, b_rate, _, _, _)| {
        b_rate
            .total_cmp(a_rate)
            .then_with(|| (a_tile.y, a_tile.x).cmp(&(b_tile.y, b_tile.x)))
    });
    let mut left = hands;
    let mut dealt = Vec::new();
    for (tile, rate, ceiling, already, plateau) in sites {
        if left == 0 {
            break;
        }
        let room = plateau.saturating_sub(already).min(left);
        if room == 0 {
            continue;
        }
        let take = crew_take(already + room, rate, ceiling) - crew_take(already, rate, ceiling);
        dealt.push((tile, room, take));
        left -= room;
    }
    ClusterTake {
        // Folded from `0.0`: `f32::sum` of nothing is `-0.0`, which a reason would print.
        total: dealt.iter().fold(0.0, |total, (_, _, take)| total + take),
        sites: dealt,
    }
}

/// One site dealt free hands by [`deal_free_hands`]: the hands up to its sustained crew, the
/// hands onto its surplus room, and what both take there this turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DealtSite {
    pub tile: Tile,
    /// Hands dealt in the first pass, up to the site's [`sustained_hands`].
    pub sustained: u32,
    /// Hands dealt in the second pass, onto the room above the floor ([`Ceiling::Surplus`]).
    pub surplus: u32,
    /// What the dealt hands take there this turn, both passes summed.
    pub take: f32,
}

impl DealtSite {
    pub fn hands(&self) -> u32 {
        self.sustained + self.surplus
    }
}

/// **`Food`'s free-hand deal — two passes over the sites in reach.** Pass one deals `hands`
/// across the sites up to each site's **sustained** crew ([`Ceiling::Sustained`], best rate
/// first, as [`cluster_take_over`] deals); pass two deals the hands left across the same sites
/// up to each site's **surplus** crew ([`Ceiling::Surplus`]) with pass one's hands already
/// standing. Each pass keeps a site only where `improves(tile, hands, take)` holds (the caller's
/// guard); hands neither pass can place are not placed — they stay idle, which is the honest
/// reading of ground that cannot use them and what *split to feed* reads next. `existing(tile)`
/// is as for [`cluster_take_over`]: the crew already on a site, `None` striking it out.
///
/// Dealt in one pass by the standing stock, a patch at its floor still looked able to use
/// twelve hands (bench seed 19: t6 put seventeen on a patch needing two, t9 dealt eight
/// "surplus" onto a patch at its floor, which took nothing extra).
#[allow(clippy::too_many_arguments)]
pub(crate) fn deal_free_hands(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    hands: u32,
    is_dead: &IsDead<'_>,
    existing: &dyn Fn(Tile) -> Option<u32>,
    improves: &dyn Fn(Tile, u32, f32) -> bool,
) -> Vec<DealtSite> {
    let sustained = cluster_take_by(
        view,
        memory,
        band,
        standing,
        hands,
        is_dead,
        existing,
        Ceiling::Sustained,
    );
    let mut dealt: Vec<DealtSite> = sustained
        .sites
        .into_iter()
        .filter(|(tile, hands, take)| improves(*tile, *hands, *take))
        .map(|(tile, hands, take)| DealtSite {
            tile,
            sustained: hands,
            surplus: 0,
            take,
        })
        .collect();
    let placed: u32 = dealt.iter().map(DealtSite::hands).sum();
    let left = hands.saturating_sub(placed);
    if left == 0 {
        return dealt;
    }
    let standing_now = |tile: Tile| {
        existing(tile).map(|already| {
            already
                + dealt
                    .iter()
                    .find(|site| site.tile == tile)
                    .map_or(0, DealtSite::hands)
        })
    };
    let surplus = cluster_take_by(
        view,
        memory,
        band,
        standing,
        left,
        is_dead,
        &standing_now,
        Ceiling::Surplus,
    );
    for (tile, hands, take) in surplus.sites {
        if !improves(tile, hands, take) {
            continue;
        }
        match dealt.iter_mut().find(|site| site.tile == tile) {
            Some(site) => {
                site.surplus += hands;
                site.take += take;
            }
            None => dealt.push(DealtSite {
                tile,
                sustained: 0,
                surplus: hands,
                take,
            }),
        }
    }
    dealt
}

/// **The sites a cluster is made of**: every workable patch within `work_range` of `standing`
/// that [`cluster_take_over`] would deal to, with the rate it is dealt at — for a caller that
/// prices the sites itself ([`super::Food::outfit_split`]).
pub(crate) fn cluster_sites<'v>(
    view: &'v SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    is_dead: &IsDead<'_>,
) -> Vec<(&'v ForagePatchState, f32)> {
    let grid = view.grid();
    view.snapshot
        .forage_patches
        .iter()
        .filter(|patch| patch.per_worker_yield > 0.0)
        .filter(|patch| patch.owner.is_none_or(|owner| owner == band.faction))
        .filter_map(|patch| {
            let tile = Tile::new(patch.x, patch.y);
            let key = SourceKey::Patch(tile);
            (view.is_discovered(tile)
                && grid.distance(standing, tile) <= band.work_range
                && !foreign_band_at(view, band.faction, tile)
                && !is_dead(&key))
            .then(|| workable_patch_at(view, tile))
            .flatten()
        })
        .map(|patch| (patch, patch_per_worker_yield(memory, band, patch)))
        .filter(|(_, rate)| *rate > 0.0)
        .collect()
}

/// [`cluster_take_over`] with every site empty: what a band of `hands` would take standing at
/// `standing` — the reading `Land` compares tiles on.
pub(crate) fn cluster_take(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    hands: u32,
    is_dead: &IsDead<'_>,
) -> ClusterTake {
    cluster_take_over(view, memory, band, standing, hands, is_dead, &|_| Some(0))
}

/// **What the ground would feed a band standing on `standing`, every turn**: [`cluster_take`]
/// with each site capped at the Best floor's regrowth and dealt hands only up to
/// [`sustained_hands`] — the standing biomass is left out, so the total is what the sites give
/// without being drawn down. The bench's *ground* measures read this off the first observation
/// (`instruments::observations`): ground whose sustained cluster is at or near the band's
/// consumption can feed it, and a seed there measures the rules and not the start's luck.
pub(crate) fn cluster_take_sustained(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    standing: Tile,
    hands: u32,
) -> ClusterTake {
    cluster_take_by(
        view,
        memory,
        band,
        standing,
        hands,
        &|_| false,
        &|_| Some(0),
        Ceiling::Sustained,
    )
}

/// **The best sustained cluster within `horizon_tiles` of the band** ([`cluster_take_sustained`]
/// over every discovered, walkable tile with no foreign band on it, the band's own tile
/// included) — the best ground the band could walk to, by what it would feed.
pub(crate) fn best_sustained_cluster_within(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    horizon_tiles: u32,
) -> f32 {
    let grid = view.grid();
    let here = band_tile(band);
    grid.disk(here, horizon_tiles)
        .into_iter()
        .filter(|tile| {
            view.is_discovered(*tile)
                && is_walkable(view, *tile)
                && !foreign_band_at(view, band.faction, *tile)
        })
        .map(|tile| cluster_take_sustained(view, memory, band, tile, band.working_age).total)
        .fold(0.0, f32::max)
}

/// **What a crew of `hands` takes off a source in one turn**: `min(hands × rate, ceiling)`, the
/// ceiling being the take at a zero escapement floor (`biomass × provisions_per_biomass`; see the
/// module docs). A per-turn quantity on both sides, so it may be compared with a per-turn demand.
pub(crate) fn crew_take(hands: u32, per_worker_yield: f32, ceiling: f32) -> f32 {
    (hands as f32 * per_worker_yield).min(ceiling)
}

/// A source a band can be assigned to.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKey {
    Patch(Tile),
    Herd(String),
}

impl SourceKey {
    /// The intent subject: `x,y` for a patch, the herd id for a herd.
    pub(super) fn subject(&self) -> String {
        match self {
            SourceKey::Patch(tile) => format!("{},{}", tile.x, tile.y),
            SourceKey::Herd(id) => id.clone(),
        }
    }

    /// The source as the decision log names it: `forage 5,4` / `hunt herd_9`.
    pub(super) fn describe(&self) -> String {
        match self {
            SourceKey::Patch(_) => format!("{ROLE_FORAGE} {}", self.subject()),
            SourceKey::Herd(_) => format!("{ROLE_HUNT} {}", self.subject()),
        }
    }

    /// The `assign_labor` role — the web — this source is worked under.
    pub(super) fn role(&self) -> &'static str {
        match self {
            SourceKey::Patch(_) => ROLE_FORAGE,
            SourceKey::Herd(_) => ROLE_HUNT,
        }
    }

    /// The memory key of this source's row on `band` ([`crate::view::row_key`]).
    pub(super) fn row_key(&self, band_id: u64) -> String {
        match self {
            SourceKey::Patch(tile) => row_key(band_id, ROLE_FORAGE, tile.x, tile.y, ""),
            SourceKey::Herd(id) => row_key(band_id, ROLE_HUNT, 0, 0, id),
        }
    }

    pub(super) fn of_row(row: &LaborAssignmentState) -> Option<Self> {
        match row.kind.as_str() {
            ROLE_FORAGE => Some(SourceKey::Patch(Tile::new(row.target_x, row.target_y))),
            ROLE_HUNT => Some(SourceKey::Herd(row.fauna_id.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Source {
    pub key: SourceKey,
    /// Where the source stands — the patch's tile, or the herd's this frame.
    pub tile: Tile,
    /// The rate a crew is ranked on. A patch: what this band has **realized** on it, if it has
    /// worked it, else the web's prior, else the frame's forecast ([`Food::rate`]). A herd: the
    /// smallest crew's likely take off the sim's curve (`likely(1)`), so a caller reading a
    /// per-hand rate reads something honest — never the wire's `per_worker_yield`, which is the
    /// kit's carry.
    pub per_worker_yield: f32,
    /// The most a crew takes here this turn. A patch: the room above the Best floor plus the
    /// floor's regrowth ([`honest_ceiling`]). A herd: the standing stock, `biomass ×
    /// provisions_per_biomass` — unread, since a herd's take is the curve's.
    pub ceiling: f32,
    /// **The most hands this source credits** — for a herd, the units of its kit the band holds
    /// ([`kit_units_held`]) or the curve's plateau, whichever is fewer; `None` for a patch.
    /// Hands beyond it hunt with nothing and add nothing.
    pub crew_cap: Option<u32>,
    /// **The sim's crew-take curve**, in food per turn — a herd's forecast
    /// (`SeatMemory::crew_take`); `None` on a patch, whose take is `min(hands × rate, ceiling)`.
    pub curve: Option<CrewTakeCurve>,
}

impl Source {
    /// What a crew of `hands` takes: the curve's likely at that crew for a herd, `min(hands ×
    /// rate, ceiling)` for a patch — the hands past [`Self::crew_cap`] counting for nothing.
    pub fn expected(&self, hands: u32) -> f32 {
        let hands = self.crew_cap.map_or(hands, |cap| hands.min(cap));
        match &self.curve {
            Some(curve) => curve.likely(hands),
            None => crew_take(hands, self.per_worker_yield, self.ceiling),
        }
    }

    /// The source as a reason names it with a crew of `hands` on it: a patch as
    /// [`SourceKey::describe`]; a herd with its curve quoted — `hunt herd_9: 5 hunters, likely
    /// 0.30/turn (sim crew take, low 0.00 high 0.72)` — so the log says what the sim said.
    pub fn describe_for(&self, hands: u32) -> String {
        let hands = self.crew_cap.map_or(hands, |cap| hands.min(cap));
        match self.curve.as_ref().and_then(|curve| curve.row(hands)) {
            Some(row) => format!(
                "{}: {hands} hunters, likely {:.2}/turn (sim crew take, low {:.2} high {:.2})",
                self.key.describe(),
                row.likely,
                row.low,
                row.high
            ),
            None => self.key.describe(),
        }
    }

    /// What `more` hands add on top of `existing` already on this source — the marginal take,
    /// which is what a reassignment onto it gains.
    pub fn marginal(&self, existing: u32, more: u32) -> f32 {
        self.expected(existing + more) - self.expected(existing)
    }

    /// **How many of `more` hands this source can use** on top of `existing`: all of them on a
    /// patch, and on a herd only up to the kit units held — the hands a rule sends, so a herd
    /// is never assigned a crew its gear cannot arm.
    pub fn usable(&self, existing: u32, more: u32) -> u32 {
        self.crew_cap
            .map_or(more, |cap| cap.saturating_sub(existing).min(more))
    }

    /// The reach a band works this source from: `work_range` for a patch, `hunt_reach` for a herd.
    pub fn reach(&self, band: &PopulationCohortState) -> u32 {
        match self.key {
            SourceKey::Patch(_) => band.work_range,
            SourceKey::Herd(_) => band.hunt_reach,
        }
    }
}
