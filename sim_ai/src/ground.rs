//! **The land reading** — what the discovered ground would feed, and the shape of the band that
//! would feed on it, read *before* any per-turn rule runs.
//!
//! Two layers. The **site layer** ([`Site`]): every discovered site the faction could work — a
//! gathering patch or a huntable herd — with what it gives per turn at the Best floor's regrowth
//! (`sustained_food`), the crew that take needs (`sustained_hands`), and the same pair for the
//! tended and field rungs at the best committable crop's payoff. The **hex layer** ([`Hex`]): every
//! discovered walkable hex a band could stand on, with the sites within its `work_range` (patches)
//! and `hunt_reach` (herds) — the site layer convolved with the band's two ranges, which is the
//! heat map a standing choice is read off.
//!
//! **The shape** ([`Reading::plan`]): standing hexes for up to `k_max` bands — the sim's two split
//! floors decide `k_max` — chosen to feed the most people, each site's value counted once. The
//! covering is greedy maximum coverage (Nemhauser, Wolsey & Fisher 1978) followed by a Teitz–Bart
//! (1968) interchange pass, with two preferences on top: the band's own hex is kept when it is
//! within `stay_tolerance` of the best, and a candidate within the supply-pooling reach of a hex
//! already chosen is weighed up by `pooling_weight`.
//!
//! **The classification** ([`Reading::classify`], [`StartKind`]) asks the same covering at four
//! bounds — the hex the band stands on, the near ring, the far ring, everything discovered — and
//! names the first that feeds the whole population. It is a pure function of the reading, so the
//! bench reads it off the first observation to say what kind of start each seed is.
//!
//! **It is read once per own band per turn** ([`read_all`], from the composite's `observe`) and the
//! same [`BandGround`] is handed to `Food` and `Land` and published by the observation, so what a
//! rule reads and what the record shows are one reading. What consumes it today: `Food`'s hoe
//! estimate (the shape's climbable patch and its worked patch count) and `Land`'s move-everyone
//! target on a `MoveAll` kind; the kit walk and the split rule do not.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_runtime::{ForagePatchState, HerdTelemetryState, PopulationCohortState};

use crate::geometry::{Grid, Tile};
use crate::profile::AiProfile;
use crate::specialists::food::ledger::{regrowth_at, BEST_FLOOR};
use crate::specialists::food::{
    foreign_band_at, herd_kit_id, is_walkable, kit_units_held, patch_per_worker_yield,
    sustained_hands, workable_patch_at, Climb, Food,
};
use crate::view::{band_tile, SeatMemory, SeatView};

/// **What a hoe adds to one keeper's work per turn** — `equipment.json` → `hoes`, tier `flint`,
/// the `build_work` effect's `equipped` value on the `plant` branch (`0.5`); the tillage kit is
/// what an `agriculture` pool holds. Restated: this crate cannot link the server's config.
pub const HOE_BUILD_WORK_PER_WORKER: f32 = 0.5;

/// **The levers the reading is shaped by** — the profile's, carried through the brain's lens so
/// the observation reads the same shape the brain would.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundLevers {
    /// `land.stay_tolerance`: the band's own hex is kept when its value is within this fraction
    /// of the best hex's.
    pub stay_tolerance: f32,
    /// `land.pooling_weight`: a candidate within `split_search_tiles` of a hex already chosen has
    /// its value multiplied by `1 + this`.
    pub pooling_weight: f32,
    /// `food.split_search_tiles`: the near ring, the supply-pooling reach.
    pub split_search_tiles: u32,
    /// `food.split_reach_tiles`: the far ring.
    pub split_reach_tiles: u32,
}

impl GroundLevers {
    pub fn of(profile: &AiProfile) -> Self {
        Self {
            stay_tolerance: profile.land.stay_tolerance,
            pooling_weight: profile.land.pooling_weight,
            split_search_tiles: profile.food.split_search_tiles,
            split_reach_tiles: profile.food.split_reach_tiles,
        }
    }
}

/// **One site the faction could work.** A patch that is a gathering site, unowned or the
/// faction's own, walkable, under no foreign band; or a huntable herd forecast above zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub x: u32,
    pub y: u32,
    /// The herd's id; `None` for a patch.
    pub herd_id: Option<String>,
    /// The reach a band works this site from: `work_range` for a patch, `hunt_reach` for a herd.
    pub reach: u32,
    /// What the site gives per turn at the Best floor's regrowth, in provisions.
    pub sustained_food: f32,
    /// The crew that take needs.
    pub sustained_hands: u32,
    /// What the tended rung pays here with its best committable crop —
    /// `FloraShareInfo::cultivate_payoff` of the committed plant, else of the largest share that
    /// can climb (`Food::climb_payoff`, the upgrade rule's own selection); `0` on a herd and where
    /// no plant can.
    pub tended_food: f32,
    /// The take crew for that plus the keeping crew for the tended rung's bill, bare-handed.
    pub tended_hands: u32,
    /// The same with the keepers holding hoes ([`HOE_BUILD_WORK_PER_WORKER`]).
    pub tended_hands_hoed: u32,
    /// **The keeping crew alone, hoed** — the hands that hold the tended rung's bill with a hoe
    /// each, which is what the outfit's hoe estimate counts (the take crew gathers and needs
    /// none). `0` on a herd and where no plant can climb.
    pub tended_keepers_hoed: u32,
    /// The Field rung's twin, off `FloraShareInfo::sow_payoff`; `0` where `sow_site_refusal`
    /// names a reason, on a herd, and where no plant can climb.
    pub field_food: f32,
    pub field_hands: u32,
    pub field_hands_hoed: u32,
    /// The kit a herd is hunted under; `None` on a patch.
    pub kit_needed: Option<String>,
    /// The units of that kit the band holds (`None` on a patch, and on a kit carrying nothing).
    pub kit_units_held: Option<u32>,
    /// **A herd the sim has not answered for yet** (`SeatMemory::crew_take` holds no curve for
    /// it and this band): it reads `0` food and `0` hands until it has. Always `false` on a patch.
    pub unforecast: bool,
}

impl Site {
    pub fn tile(&self) -> Tile {
        Tile::new(self.x, self.y)
    }
}

/// **A hex a band could stand on**, with the sites in reach of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Hex {
    pub tile: Tile,
    /// Indices into [`Reading::sites`].
    pub sites: Vec<usize>,
}

/// The sums over a set of sites.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Sums {
    food: f32,
    hands: u32,
    tended: f32,
    field: f32,
}

/// **The reading**, taken for one band: the two layers, and the numbers the shape is sized by.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub grid: Grid,
    /// The hex the band stands on.
    pub here: Tile,
    /// The band's whole people, the ones the shape must feed.
    pub population: u32,
    pub working_age: u32,
    /// The sim's two split floors, off the cohort row.
    pub founding_min_workers: u32,
    pub founding_parent_min_workers: u32,
    /// `food_consumption / size`, what one person eats a turn.
    pub per_person_consumption: f32,
    pub sites: Vec<Site>,
    /// Every candidate standing hex that covers at least one site, sorted `(y, x)`.
    pub hexes: Vec<Hex>,
}

/// One planned band of a [`Shape`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedBand {
    pub x: u32,
    pub y: u32,
    /// The sites this band claims — indices into the reading's site list.
    pub sites: Vec<usize>,
    /// Σ `sustained_hands` over its sites, clamped to the floors.
    pub hands: u32,
    /// `hands × population / working_age` — the dependents follow proportionally.
    pub people: u32,
    pub food: f32,
    pub tended_food: f32,
    pub field_food: f32,
    /// Hex steps to the nearest other planned band; `None` when it is the only one.
    pub nearest_planned_distance: Option<u32>,
    /// **The kits this band would want**: baskets for every sustained patch hand (Σ
    /// `sustained_hands` over its patch sites) …
    pub baskets: u32,
    /// … and, per hunt kit id (`Site::kit_needed`), the hunter hands over the herds it claims.
    /// Read off the roster's kit for the herd, never the units held: at tick 1 every band holds
    /// `0` of every hunt kit — the outfit lands after the tick-1 command.
    pub hunt_kits: BTreeMap<String, u32>,
}

impl PlannedBand {
    pub fn tile(&self) -> Tile {
        Tile::new(self.x, self.y)
    }
}

/// **The shape**: the planned bands and what they feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub bands: Vec<PlannedBand>,
    /// People fed on the wild take of every claimed site.
    pub people_fed: f32,
    /// The same ground farmed: patches at their tended quote, herds as they are.
    pub people_fed_tended: f32,
    /// …and at their field quote.
    pub people_fed_field: f32,
    /// `population − Σ people`, at least 0.
    pub people_uncovered: u32,
    /// The "move everyone" case: the best single hex discovered beats the first planned band by
    /// more than `stay_tolerance` and lies beyond the far ring of the band's current hex.
    pub move_target: Option<(u32, u32)>,
}

impl Shape {
    fn empty(population: u32) -> Self {
        Self {
            bands: Vec::new(),
            people_fed: 0.0,
            people_fed_tended: 0.0,
            people_fed_field: 0.0,
            people_uncovered: population,
            move_target: None,
        }
    }

    pub fn move_target_tile(&self) -> Option<Tile> {
        self.move_target.map(|(x, y)| Tile::new(x, y))
    }
}

/// **What kind of start the reading says this is** — the first of the four bounds whose covering
/// feeds the whole population, on wild food.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartKind {
    /// The hex the band stands on feeds everyone.
    Stay,
    /// A covering within `split_search_tiles` of it does.
    SplitLocal,
    /// A covering within `split_reach_tiles` does.
    SplitFar,
    /// A `move_target` exists and the covering around it feeds everyone.
    MoveAll,
    /// Nothing discovered feeds everyone.
    Short,
}

impl StartKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StartKind::Stay => "stay",
            StartKind::SplitLocal => "split_local",
            StartKind::SplitFar => "split_far",
            StartKind::MoveAll => "move_all",
            StartKind::Short => "short",
        }
    }
}

/// [`Reading::classify`]'s answer: the kind, and the four coverings it was read off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classified {
    pub kind: StartKind,
    pub stay: Shape,
    pub local: Shape,
    pub far: Shape,
    pub visible: Shape,
    /// The far covering's move target, and the covering around it when there is one.
    pub move_target: Option<(u32, u32)>,
    pub around_target: Option<Shape>,
}

impl Classified {
    pub fn move_target_tile(&self) -> Option<Tile> {
        self.move_target.map(|(x, y)| Tile::new(x, y))
    }

    /// **The shape the kind was read off**: the band's own hex for `Stay`, the near ring for
    /// `SplitLocal`, the far ring for `SplitFar`, the near ring around the move target for
    /// `MoveAll`, and the far ring for `Short` (the best the band can reach from where it stands
    /// when nothing feeds everyone) — what the hoe estimate reads its patches off.
    pub fn shape(&self) -> &Shape {
        match self.kind {
            StartKind::Stay => &self.stay,
            StartKind::SplitLocal => &self.local,
            StartKind::SplitFar | StartKind::Short => &self.far,
            StartKind::MoveAll => self.around_target.as_ref().unwrap_or(&self.far),
        }
    }
}

/// **One band's reading for this turn**: the reading and its classification, taken once in the
/// composite's `observe` and handed to every specialist and to the observation record alike.
#[derive(Debug, Clone, PartialEq)]
pub struct BandGround {
    pub reading: Reading,
    pub classified: Classified,
}

impl BandGround {
    /// The shape the start kind was read off ([`Classified::shape`]).
    pub fn shape(&self) -> &Shape {
        self.classified.shape()
    }
}

/// The readings of every own band this turn, by band id.
pub type GroundReadings = BTreeMap<u64, BandGround>;

/// **Read the ground for every own band of `faction`** — the composite's per-turn reading.
pub fn read_all(
    view: &SeatView,
    memory: &SeatMemory,
    faction: u32,
    levers: &GroundLevers,
) -> GroundReadings {
    view.own_bands(faction)
        .map(|band| {
            let reading = Reading::read(view, memory, band);
            let classified = reading.classify(levers);
            (
                band.band_id,
                BandGround {
                    reading,
                    classified,
                },
            )
        })
        .collect()
}

/// `ceil(amount / per)` as a crew; `0` when either is nothing.
fn crew_for(amount: f32, per: f32) -> u32 {
    if amount <= 0.0 || per <= 0.0 {
        0
    } else {
        (amount / per).ceil() as u32
    }
}

impl Reading {
    /// **Read the ground for `band`** off the view: the sites the faction could work, the hexes
    /// it could stand on, and the band's own numbers. The rate a patch's crews are read at is
    /// [`patch_per_worker_yield`] — what this seat has realized there, else the web's prior,
    /// else the forecast — the same rate every rule ranks on; a herd is read at the sim's crew
    /// take ([`herd_site`]): its best crew and what that crew likely brings home, capped at its
    /// sustainable line.
    pub fn read(view: &SeatView, memory: &SeatMemory, band: &PopulationCohortState) -> Self {
        let grid = view.grid();
        let faction = band.faction;
        let per_person_consumption = if band.size > 0 {
            band.food_consumption / band.size as f32
        } else {
            0.0
        };
        let mut sites: Vec<Site> = view
            .snapshot
            .forage_patches
            .iter()
            .filter(|patch| patch.per_worker_yield > 0.0)
            .filter(|patch| patch.owner.is_none_or(|owner| owner == faction))
            .filter_map(|patch| {
                let tile = Tile::new(patch.x, patch.y);
                (view.is_discovered(tile)
                    && is_walkable(view, tile)
                    && !foreign_band_at(view, faction, tile))
                .then(|| workable_patch_at(view, tile))
                .flatten()
            })
            .map(|patch| patch_site(memory, band, patch))
            .filter(|site| site.sustained_food > 0.0)
            .collect();
        sites.extend(
            view.snapshot
                .herds
                .iter()
                .filter(|herd| herd.huntable && herd.per_worker_yield > 0.0)
                .filter(|herd| {
                    let tile = Tile::new(herd.x, herd.y);
                    view.is_discovered(tile) && !foreign_band_at(view, faction, tile)
                })
                .map(|herd| herd_site(view, memory, band, herd))
                // A herd the sim has not answered for stays in the reading at nothing, flagged,
                // so the record says which herds were not yet forecast.
                .filter(|site| site.sustained_food > 0.0 || site.unforecast),
        );
        // The candidate hexes: every discovered, walkable, unoccupied tile within some site's
        // reach.
        let mut candidates: Vec<Tile> = Vec::new();
        for site in &sites {
            for tile in grid.disk(site.tile(), site.reach) {
                if !candidates.contains(&tile)
                    && view.is_discovered(tile)
                    && is_walkable(view, tile)
                    && !foreign_band_at(view, faction, tile)
                {
                    candidates.push(tile);
                }
            }
        }
        Self::from_sites(
            grid,
            band_tile(band),
            band.size,
            band.working_age,
            (band.founding_min_workers, band.founding_parent_min_workers),
            per_person_consumption,
            sites,
            candidates,
        )
    }

    /// The reading from a site list and the hexes a band could stand on — [`Self::read`]'s
    /// second half, and what a fixture builds directly. `floors` is `(founding_min_workers,
    /// founding_parent_min_workers)`. A candidate that covers no site is dropped.
    #[allow(clippy::too_many_arguments)]
    pub fn from_sites(
        grid: Grid,
        here: Tile,
        population: u32,
        working_age: u32,
        floors: (u32, u32),
        per_person_consumption: f32,
        sites: Vec<Site>,
        candidates: impl IntoIterator<Item = Tile>,
    ) -> Self {
        let mut hexes: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for tile in candidates {
            let covered: Vec<usize> = sites
                .iter()
                .enumerate()
                .filter(|(_, site)| grid.distance(tile, site.tile()) <= site.reach)
                .map(|(index, _)| index)
                .collect();
            if !covered.is_empty() {
                hexes.insert((tile.y, tile.x), covered);
            }
        }
        Self {
            grid,
            here,
            population,
            working_age,
            founding_min_workers: floors.0,
            founding_parent_min_workers: floors.1,
            per_person_consumption,
            sites,
            hexes: hexes
                .into_iter()
                .map(|((y, x), sites)| Hex {
                    tile: Tile::new(x, y),
                    sites,
                })
                .collect(),
        }
    }

    /// **The same reading with the herds struck out** — the patches alone, every hex re-read
    /// over them (a hex that reached only herds is no candidate now). What the ground feeds
    /// with no hunt kit at all, which is how every band starts.
    pub fn patches_only(&self) -> Self {
        let kept: Vec<usize> = (0..self.sites.len())
            .filter(|&index| self.sites[index].herd_id.is_none())
            .collect();
        let new_index = |old: usize| kept.iter().position(|&index| index == old);
        Self {
            grid: self.grid,
            here: self.here,
            population: self.population,
            working_age: self.working_age,
            founding_min_workers: self.founding_min_workers,
            founding_parent_min_workers: self.founding_parent_min_workers,
            per_person_consumption: self.per_person_consumption,
            sites: kept
                .iter()
                .map(|&index| self.sites[index].clone())
                .collect(),
            hexes: self
                .hexes
                .iter()
                .filter_map(|hex| {
                    let sites: Vec<usize> =
                        hex.sites.iter().filter_map(|&old| new_index(old)).collect();
                    (!sites.is_empty()).then_some(Hex {
                        tile: hex.tile,
                        sites,
                    })
                })
                .collect(),
        }
    }

    /// **How many bands the population could be** — `1 + floor((working_age −
    /// founding_parent_min_workers) / founding_min_workers)`, the sim's two floors; `1` when the
    /// band cannot spare a founding crew.
    pub fn k_max(&self) -> u32 {
        if self.founding_min_workers == 0 || self.working_age <= self.founding_parent_min_workers {
            return 1;
        }
        1 + (self.working_age - self.founding_parent_min_workers) / self.founding_min_workers
    }

    /// People fed by `food` a turn.
    pub fn people_fed(&self, food: f32) -> f32 {
        if self.per_person_consumption > 0.0 {
            food / self.per_person_consumption
        } else {
            0.0
        }
    }

    /// The hex at `tile`, if it is a candidate.
    #[cfg(test)]
    fn hex_at(&self, tile: Tile) -> Option<&Hex> {
        self.hexes.iter().find(|hex| hex.tile == tile)
    }

    /// The sums over `hex`'s sites not yet covered.
    fn uncovered_sums(&self, hex: &Hex, covered: &[bool]) -> Sums {
        hex.sites
            .iter()
            .filter(|&&index| !covered[index])
            .map(|&index| &self.sites[index])
            .fold(Sums::default(), |sums, site| Sums {
                food: sums.food + site.sustained_food,
                hands: sums.hands + site.sustained_hands,
                tended: sums.tended + site.tended_food.max(site.sustained_food),
                field: sums.field + site.field_food.max(site.sustained_food),
            })
    }

    /// The wild food over the union of the sites of the hexes at `chosen`.
    fn union_food(&self, chosen: &[usize]) -> f32 {
        let mut covered = vec![false; self.sites.len()];
        let mut total = 0.0;
        for &index in chosen {
            total += self.uncovered_sums(&self.hexes[index], &covered).food;
            for &site in &self.hexes[index].sites {
                covered[site] = true;
            }
        }
        total
    }

    /// The best single hex over everything discovered — its index and its wild food.
    fn best_single(&self) -> Option<(usize, f32)> {
        let none = vec![false; self.sites.len()];
        self.hexes
            .iter()
            .enumerate()
            .map(|(index, hex)| (index, self.uncovered_sums(hex, &none).food))
            .filter(|(_, food)| *food > 0.0)
            .max_by(|a, b| {
                a.1.total_cmp(&b.1).then_with(|| {
                    self.nearer_first(self.here, self.hexes[a.0].tile, self.hexes[b.0].tile)
                })
            })
    }

    /// Ties: nearer to `from` first, then the lower `(y, x)` — as an ordering where `a` is the
    /// preferred of two equal values.
    fn nearer_first(&self, from: Tile, a: Tile, b: Tile) -> std::cmp::Ordering {
        self.grid
            .distance(from, b)
            .cmp(&self.grid.distance(from, a))
            .then_with(|| (b.y, b.x).cmp(&(a.y, a.x)))
    }

    /// **Choose standing hexes for up to `k_max` bands around `anchor`**, from the candidate
    /// hexes within `bound` hex steps of it (`None`: everything discovered), maximising the
    /// people fed with each site counted once:
    ///
    /// 1. greedy — the hex with the greatest uncovered value, weighed up by `pooling_weight`
    ///    when it is within `split_search_tiles` of a hex already chosen; `anchor` itself is
    ///    tried first and kept when within `stay_tolerance` of the best; after the first band a
    ///    hex must add a founding crew's worth of people (`founding_min_workers × population /
    ///    working_age`) to be a band;
    /// 2. interchange — each chosen hex against each unchosen candidate, keeping a swap that
    ///    raises the wild food covered, until none does; a kept anchor is not swapped out,
    ///    since the tolerance is the point of keeping it;
    /// 3. sizing — each band claims the sites it is the first to cover, its hands are their
    ///    `sustained_hands` clamped to the floors (the first band keeps at least the parent's
    ///    floor, the rest at least the founding floor, none above `working_age`), and its people
    ///    follow in proportion.
    pub fn plan(&self, levers: &GroundLevers, anchor: Tile, bound: Option<u32>) -> Shape {
        let candidates: Vec<usize> = self
            .hexes
            .iter()
            .enumerate()
            .filter(|(_, hex)| {
                bound.is_none_or(|bound| self.grid.distance(anchor, hex.tile) <= bound)
            })
            .map(|(index, _)| index)
            .collect();
        let mut covered = vec![false; self.sites.len()];
        let mut chosen: Vec<usize> = Vec::new();
        let mut pinned: Option<usize> = None;
        let min_band_people = if self.working_age > 0 {
            self.founding_min_workers as f32 * self.population as f32 / self.working_age as f32
        } else {
            0.0
        };
        let k_max = self.k_max();
        while (chosen.len() as u32) < k_max {
            let pooled = |tile: Tile| {
                chosen.iter().any(|&index| {
                    self.grid.distance(self.hexes[index].tile, tile) <= levers.split_search_tiles
                })
            };
            let best = candidates
                .iter()
                .copied()
                .filter(|index| !chosen.contains(index))
                .map(|index| {
                    let hex = &self.hexes[index];
                    let raw = self.uncovered_sums(hex, &covered).food;
                    let weighted = if pooled(hex.tile) {
                        raw * (1.0 + levers.pooling_weight)
                    } else {
                        raw
                    };
                    (index, raw, weighted)
                })
                .filter(|(_, raw, _)| *raw > 0.0)
                .max_by(|a, b| {
                    a.2.total_cmp(&b.2).then_with(|| {
                        self.nearer_first(anchor, self.hexes[a.0].tile, self.hexes[b.0].tile)
                    })
                });
            let Some((best_index, best_raw, _)) = best else {
                break;
            };
            let pick = if chosen.is_empty() {
                // The band's own hex first: kept when it is close enough to the best.
                let own = candidates
                    .iter()
                    .copied()
                    .find(|&index| self.hexes[index].tile == anchor)
                    .map(|index| {
                        (
                            index,
                            self.uncovered_sums(&self.hexes[index], &covered).food,
                        )
                    })
                    .filter(|(_, raw)| {
                        *raw > 0.0 && *raw >= best_raw * (1.0 - levers.stay_tolerance)
                    });
                match own {
                    Some((index, _)) => {
                        pinned = Some(index);
                        index
                    }
                    None => best_index,
                }
            } else {
                if self.people_fed(best_raw) < min_band_people {
                    break;
                }
                best_index
            };
            chosen.push(pick);
            for &site in &self.hexes[pick].sites {
                covered[site] = true;
            }
        }
        if chosen.is_empty() {
            return Shape::empty(self.population);
        }
        // Teitz–Bart: one-for-one swaps while any raises the food covered.
        let mut total = self.union_food(&chosen);
        loop {
            let mut improved = false;
            for position in 0..chosen.len() {
                if pinned == Some(chosen[position]) {
                    continue;
                }
                for &candidate in &candidates {
                    if chosen.contains(&candidate) {
                        continue;
                    }
                    let mut trial = chosen.clone();
                    trial[position] = candidate;
                    let with = self.union_food(&trial);
                    if with > total {
                        chosen = trial;
                        total = with;
                        improved = true;
                    }
                }
            }
            if !improved {
                break;
            }
        }
        self.size(levers, &chosen)
    }

    /// Step 3 of [`Self::plan`]: the bands off the chosen hexes, and the shape's totals.
    fn size(&self, levers: &GroundLevers, chosen: &[usize]) -> Shape {
        let mut covered = vec![false; self.sites.len()];
        let mut bands: Vec<PlannedBand> = Vec::new();
        for (position, &index) in chosen.iter().enumerate() {
            let hex = &self.hexes[index];
            let sums = self.uncovered_sums(hex, &covered);
            let claimed: Vec<usize> = hex
                .sites
                .iter()
                .copied()
                .filter(|&site| !covered[site])
                .collect();
            for &site in &claimed {
                covered[site] = true;
            }
            let floor = if position == 0 {
                self.founding_parent_min_workers
            } else {
                self.founding_min_workers
            };
            let hands = sums.hands.max(floor).min(self.working_age);
            let people = if self.working_age > 0 {
                ((hands as f32) * (self.population as f32) / (self.working_age as f32)).round()
                    as u32
            } else {
                0
            };
            let mut baskets = 0;
            let mut hunt_kits: BTreeMap<String, u32> = BTreeMap::new();
            for &site in &claimed {
                let site = &self.sites[site];
                match &site.kit_needed {
                    Some(kit) => *hunt_kits.entry(kit.clone()).or_insert(0) += site.sustained_hands,
                    None => baskets += site.sustained_hands,
                }
            }
            bands.push(PlannedBand {
                x: hex.tile.x,
                y: hex.tile.y,
                sites: claimed,
                hands,
                people,
                food: sums.food,
                tended_food: sums.tended,
                field_food: sums.field,
                nearest_planned_distance: None,
                baskets,
                hunt_kits,
            });
        }
        let tiles: Vec<Tile> = bands.iter().map(PlannedBand::tile).collect();
        for (position, band) in bands.iter_mut().enumerate() {
            band.nearest_planned_distance = tiles
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != position)
                .map(|(_, tile)| self.grid.distance(band.tile(), *tile))
                .min();
        }
        let food: f32 = bands.iter().map(|band| band.food).sum();
        let tended: f32 = bands.iter().map(|band| band.tended_food).sum();
        let field: f32 = bands.iter().map(|band| band.field_food).sum();
        let people: u32 = bands.iter().map(|band| band.people).sum();
        let first_food = bands.first().map_or(0.0, |band| band.food);
        let move_target = self
            .best_single()
            .filter(|(index, best)| {
                *best > first_food * (1.0 + levers.stay_tolerance)
                    && self.grid.distance(self.here, self.hexes[*index].tile)
                        > levers.split_reach_tiles
            })
            .map(|(index, _)| (self.hexes[index].tile.x, self.hexes[index].tile.y));
        Shape {
            bands,
            people_fed: self.people_fed(food),
            people_fed_tended: self.people_fed(tended),
            people_fed_field: self.people_fed(field),
            people_uncovered: self.population.saturating_sub(people),
            move_target,
        }
    }

    /// Whether `shape` feeds the whole population on wild food.
    pub fn feeds_everyone(&self, shape: &Shape) -> bool {
        shape.people_fed >= self.population as f32
    }

    /// **The four coverings and the kind they say the start is**: the band's own hex alone
    /// (`stay`), within the near ring (`local`), within the far ring (`far`), everything
    /// discovered (`visible`); the far covering's `move_target` and, when there is one, the near
    /// covering around it.
    pub fn classify(&self, levers: &GroundLevers) -> Classified {
        let stay = self.plan(levers, self.here, Some(0));
        let local = self.plan(levers, self.here, Some(levers.split_search_tiles));
        let far = self.plan(levers, self.here, Some(levers.split_reach_tiles));
        let visible = self.plan(levers, self.here, None);
        let move_target = far.move_target;
        let around_target = far
            .move_target_tile()
            .map(|target| self.plan(levers, target, Some(levers.split_search_tiles)));
        let kind = if self.feeds_everyone(&stay) {
            StartKind::Stay
        } else if self.feeds_everyone(&local) {
            StartKind::SplitLocal
        } else if self.feeds_everyone(&far) {
            StartKind::SplitFar
        } else if around_target
            .as_ref()
            .is_some_and(|around| self.feeds_everyone(around))
        {
            StartKind::MoveAll
        } else {
            StartKind::Short
        };
        Classified {
            kind,
            stay,
            local,
            far,
            visible,
            move_target,
            around_target,
        }
    }
}

/// The site a patch row is, read at `Food::rate`'s rate for `band` (`patch_per_worker_yield`).
fn patch_site(memory: &SeatMemory, band: &PopulationCohortState, patch: &ForagePatchState) -> Site {
    let rate = patch_per_worker_yield(memory, band, patch);
    let sustained_food =
        regrowth_at(&patch.regrowth_samples, BEST_FLOOR) * patch.provisions_per_biomass;
    // The farmed quotes are the best committable crop's, through the selection *upgrade the
    // ground* declares with — the committed plant, else the largest share that may climb —
    // not the patch row's own `tended_yield` / `field_yield`, which are species-blind and read
    // the crop already committed (nothing, at the start).
    let tended_food = Food::climb_payoff(patch, Climb::Tended).map_or(0.0, |(_, payoff)| payoff);
    let field_food = if patch.sow_site_refusal.is_empty() {
        Food::climb_payoff(patch, Climb::Field).map_or(0.0, |(_, payoff)| payoff)
    } else {
        0.0
    };
    let per_turn = patch.build_work_per_worker_turn;
    let hoed = per_turn + HOE_BUILD_WORK_PER_WORKER;
    let tended_keepers_hoed = if tended_food > 0.0 {
        crew_for(patch.cultivation_upkeep_demand, hoed)
    } else {
        0
    };
    Site {
        x: patch.x,
        y: patch.y,
        herd_id: None,
        reach: band.work_range,
        sustained_food,
        sustained_hands: sustained_hands(patch, rate),
        tended_food,
        tended_hands: crew_for(tended_food, rate)
            + crew_for(patch.cultivation_upkeep_demand, per_turn),
        tended_hands_hoed: crew_for(tended_food, rate)
            + crew_for(patch.cultivation_upkeep_demand, hoed),
        tended_keepers_hoed,
        field_food,
        field_hands: crew_for(field_food, rate) + crew_for(patch.field_upkeep_demand, per_turn),
        field_hands_hoed: crew_for(field_food, rate) + crew_for(patch.field_upkeep_demand, hoed),
        kit_needed: None,
        kit_units_held: None,
        unforecast: false,
    }
}

/// The site a herd row is, **read at the sim's crew take**: its food is the lesser of its
/// sustainable line (the Best-floor regrowth in provisions) and what the best crew likely brings
/// home per turn off the cached curve (`SeatMemory::crew_take`), its hands are that best crew
/// (`CrewTakeCurve::best_crew`, the size taking the most per hunter), and it carries the kit it
/// is hunted under with the units `band` holds. A herd with no curve reads `0` and `unforecast`.
fn herd_site(
    view: &SeatView,
    memory: &SeatMemory,
    band: &PopulationCohortState,
    herd: &HerdTelemetryState,
) -> Site {
    // A herd's low samples are negative (the Allee crash); at the Best floor the regrowth is
    // what the herd gives, and nothing below zero is a take.
    let line =
        regrowth_at(&herd.regrowth_samples, BEST_FLOOR).max(0.0) * herd.provisions_per_biomass;
    let kit = herd_kit_id(view, herd);
    let curve = memory.crew_take(band.band_id, &herd.id);
    let (sustained_food, sustained_hands) = match curve.and_then(|curve| curve.best_crew()) {
        Some(best) => {
            let likely = curve.map_or(0.0, |curve| curve.likely(best));
            (line.min(likely), best)
        }
        None => (0.0, 0),
    };
    Site {
        x: herd.x,
        y: herd.y,
        herd_id: Some(herd.id.clone()),
        reach: band.hunt_reach,
        sustained_food,
        sustained_hands,
        tended_food: 0.0,
        tended_hands: 0,
        tended_hands_hoed: 0,
        tended_keepers_hoed: 0,
        field_food: 0.0,
        field_hands: 0,
        field_hands_hoed: 0,
        kit_needed: Some(kit.to_owned()),
        kit_units_held: kit_units_held(view, band, kit),
        unforecast: curve.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid {
        width: 24,
        height: 16,
        wrap_horizontal: false,
    };
    const HERE: Tile = Tile::new(10, 8);
    const WORK_RANGE: u32 = 2;
    const POPULATION: u32 = 30;
    const WORKING_AGE: u32 = 17;
    const FOUNDING_FLOOR: u32 = 4;
    const PARENT_FLOOR: u32 = 6;
    /// One person eats this a turn.
    const EATS: f32 = 0.2;
    const LEVERS: GroundLevers = GroundLevers {
        stay_tolerance: 0.1,
        pooling_weight: 0.25,
        split_search_tiles: 3,
        split_reach_tiles: 6,
    };

    /// A patch site giving `food` a turn at `rate` a hand.
    fn patch(tile: Tile, food: f32, rate: f32) -> Site {
        Site {
            x: tile.x,
            y: tile.y,
            herd_id: None,
            reach: WORK_RANGE,
            sustained_food: food,
            sustained_hands: crew_for(food, rate),
            tended_food: food * 2.0,
            tended_hands: crew_for(food * 2.0, rate) + 1,
            tended_hands_hoed: crew_for(food * 2.0, rate) + 1,
            tended_keepers_hoed: 1,
            field_food: food * 4.0,
            field_hands: crew_for(food * 4.0, rate) + 2,
            field_hands_hoed: crew_for(food * 4.0, rate) + 1,
            kit_needed: None,
            kit_units_held: None,
            unforecast: false,
        }
    }

    /// A reading over `sites`, every land tile a candidate.
    fn a_reading(sites: Vec<Site>) -> Reading {
        let candidates = (0..GRID.height)
            .flat_map(|y| (0..GRID.width).map(move |x| Tile::new(x, y)))
            .collect::<Vec<_>>();
        Reading::from_sites(
            GRID,
            HERE,
            POPULATION,
            WORKING_AGE,
            (FOUNDING_FLOOR, PARENT_FLOOR),
            EATS,
            sites,
            candidates,
        )
    }

    fn tile_of(band: &PlannedBand) -> Tile {
        band.tile()
    }

    /// Two sites two steps apart: each hex covering one reads its food and hands; the hex
    /// between them reads the sum. `k_max` follows the floors: seventeen hands less the parent's
    /// six is eleven, two founding crews of four — three bands at most.
    #[test]
    fn the_reading_sums_sites_per_hex_and_k_max_follows_the_floors() {
        let a = Tile::new(8, 8);
        let b = Tile::new(12, 8);
        let reading = a_reading(vec![patch(a, 2.0, 0.5), patch(b, 1.0, 0.5)]);
        assert_eq!(reading.sites[0].sustained_hands, 4);
        assert_eq!(reading.sites[1].sustained_hands, 2);
        assert_eq!(reading.people_fed(2.0), 10.0);
        let none = vec![false; 2];
        let at_a = reading.hex_at(a).expect("a covers itself");
        assert_eq!(reading.uncovered_sums(at_a, &none).food, 2.0);
        assert_eq!(reading.uncovered_sums(at_a, &none).hands, 4);
        let between = reading.hex_at(HERE).expect("two steps from each");
        assert_eq!(GRID.distance(HERE, a), 2);
        assert_eq!(GRID.distance(HERE, b), 2);
        assert_eq!(reading.uncovered_sums(between, &none).food, 3.0);
        assert_eq!(reading.uncovered_sums(between, &none).hands, 6);
        assert_eq!(reading.uncovered_sums(between, &none).tended, 6.0);
        assert_eq!(reading.k_max(), 3);
        let small = Reading {
            working_age: 9,
            ..reading.clone()
        };
        assert_eq!(small.k_max(), 1, "nine cannot spare a founding crew");
        let ten = Reading {
            working_age: 10,
            ..reading.clone()
        };
        assert_eq!(ten.k_max(), 2);
        // A hex covering nothing is not a candidate.
        assert!(reading.hex_at(Tile::new(0, 0)).is_none());
    }

    /// Greedy picks the hex covering the most people — here the band's own hex, which reaches
    /// both sites — and a second band only claims what the first left: the far site, alone.
    #[test]
    fn greedy_covers_the_most_first_and_the_second_band_claims_only_what_is_uncovered() {
        let a = Tile::new(8, 8);
        let b = Tile::new(12, 8);
        let far = Tile::new(20, 8);
        let reading = a_reading(vec![
            patch(a, 2.0, 0.5),
            patch(b, 1.0, 0.5),
            patch(far, 1.5, 0.5),
        ]);
        let shape = reading.plan(&LEVERS, HERE, None);
        assert_eq!(shape.bands.len(), 2, "{shape:?}");
        assert_eq!(tile_of(&shape.bands[0]), HERE);
        assert_eq!(shape.bands[0].sites, vec![0, 1]);
        assert_eq!(shape.bands[0].food, 3.0);
        assert_eq!(
            shape.bands[0].hands, 6,
            "six sustained hands, at the parent's floor"
        );
        assert_eq!(shape.bands[1].sites, vec![2]);
        assert_eq!(shape.bands[1].food, 1.5);
        assert_eq!(
            shape.bands[1].hands, 4,
            "three sustained hands, raised to the founding floor"
        );
        assert_eq!(shape.bands[1].people, 7, "4 × 30 / 17, rounded");
        assert_eq!(shape.people_fed, 22.5);
        assert_eq!(shape.people_fed_tended, 45.0);
        assert_eq!(
            shape.people_uncovered,
            POPULATION - shape.bands[0].people - 7
        );
        assert_eq!(
            shape.bands[0].nearest_planned_distance,
            Some(GRID.distance(HERE, tile_of(&shape.bands[1])))
        );
        // The far site within reach of the second band.
        assert!(GRID.distance(tile_of(&shape.bands[1]), far) <= WORK_RANGE);
        // A site that feeds less than a founding crew's people is not a third band.
        assert_eq!(reading.people_fed(1.5), 7.5);
        let min_band_people = FOUNDING_FLOOR as f32 * POPULATION as f32 / WORKING_AGE as f32;
        assert!(reading.people_fed(1.5) >= min_band_people);
        let crumb = a_reading(vec![patch(a, 2.0, 0.5), patch(far, 0.5, 0.5)]);
        let shape = crumb.plan(&LEVERS, HERE, None);
        assert_eq!(shape.bands.len(), 1, "2.5 people is not a band: {shape:?}");
    }

    /// The band stands on its 3.0 site and keeps it. Two more sites: one within the pooling
    /// reach of the first band worth 1.5, one beyond it worth 1.8. Weighed up by a quarter the
    /// pooled one (1.875) wins the second band; with no weight the richer far one does.
    #[test]
    fn pooling_prefers_a_second_hex_within_the_supply_reach_of_the_first() {
        let pooled = Tile::new(15, 8);
        let far = Tile::new(3, 8);
        let reading = a_reading(vec![
            patch(HERE, 3.0, 0.5),
            patch(pooled, 1.5, 0.5),
            patch(far, 1.8, 0.5),
        ]);
        let shape = reading.plan(&LEVERS, HERE, None);
        assert_eq!(shape.bands.len(), 3, "{shape:?}");
        assert_eq!(tile_of(&shape.bands[0]), HERE);
        let second = tile_of(&shape.bands[1]);
        assert!(
            GRID.distance(HERE, second) <= LEVERS.split_search_tiles,
            "pooled at {second:?}: {shape:?}"
        );
        assert_eq!(shape.bands[1].sites, vec![1]);
        assert_eq!(shape.bands[2].sites, vec![2]);
        let unpooled = GroundLevers {
            pooling_weight: 0.0,
            ..LEVERS
        };
        let shape = reading.plan(&unpooled, HERE, None);
        assert_eq!(
            shape.bands[1].sites,
            vec![2],
            "without the weight the richer hex wins: {shape:?}"
        );
    }

    /// Five sites on one row, two bands to place (ten working-age: one founding crew above the
    /// parent's floor). The centre hex (8,4) reaches s0 + s1 + s2 = 7.0 and is greedy's first
    /// pick; the left hex (6,4) reaches s0 + s1 + L = 6.5 and the right hex (10,4) s2 + R =
    /// 5.5. Greedy then adds R alone (4.0) for 11.0; the optimum is left + right = 12.0, one
    /// swap away — the interchange finds it.
    #[test]
    fn one_interchange_swap_repairs_a_greedy_first_pick() {
        let reading = Reading::from_sites(
            GRID,
            Tile::new(0, 0),
            POPULATION,
            10,
            (FOUNDING_FLOOR, PARENT_FLOOR),
            EATS,
            vec![
                patch(Tile::new(6, 4), 1.5, 0.5),  // s0
                patch(Tile::new(8, 4), 4.0, 0.5),  // s1
                patch(Tile::new(10, 4), 1.5, 0.5), // s2
                patch(Tile::new(4, 4), 1.0, 0.5),  // L: reached from x ≤ 6
                patch(Tile::new(13, 4), 4.0, 0.5), // R: reached from x ≥ 11
            ],
            (0..GRID.width).map(|x| Tile::new(x, 4)),
        );
        assert_eq!(reading.k_max(), 2);
        let none = vec![false; 5];
        let sum_at = |tile| {
            reading
                .uncovered_sums(reading.hex_at(tile).expect("a candidate"), &none)
                .food
        };
        assert_eq!(sum_at(Tile::new(8, 4)), 7.0);
        assert_eq!(sum_at(Tile::new(6, 4)), 6.5);
        assert_eq!(sum_at(Tile::new(11, 4)), 5.5);
        let shape = reading.plan(&LEVERS, Tile::new(0, 0), None);
        let total: f32 = shape.bands.iter().map(|band| band.food).sum();
        assert_eq!(total, 12.0, "the swap found the optimum: {shape:?}");
        let hexes: Vec<Tile> = shape.bands.iter().map(tile_of).collect();
        assert!(hexes.contains(&Tile::new(6, 4)), "{hexes:?}");
        assert!(hexes.contains(&Tile::new(11, 4)), "{hexes:?}");
        assert_eq!(shape.people_fed, 60.0);
    }

    /// **A herd site is read at the sim's crew take.** With no curve cached for the band it is
    /// in the reading at nothing, flagged `unforecast`; with one, its food is the lesser of its
    /// sustainable line and what the best crew (the most per hunter) likely brings home, and its
    /// hands are that crew. Before this the herd read its regrowth at `per_worker_biomass` a
    /// hunter — the kit's carry — whatever the crew could bring down.
    #[test]
    fn a_herd_site_reads_the_curve_at_its_best_crew_or_unforecast() {
        use crate::oracle::curve_of;
        use crate::specialists::food::tests::{a_view, memory, BAND, HERD_ID};
        let herd_site_of = |view: &SeatView, memory: &SeatMemory| {
            Reading::read(view, memory, &view.snapshot.populations[0])
                .sites
                .into_iter()
                .find(|site| site.herd_id.as_deref() == Some(HERD_ID))
        };
        let mut view = a_view();
        // The herd's line: one biomass a turn at every fraction of K, one food a biomass.
        let samples = view.snapshot.herds[0].regrowth_samples.len().max(2);
        view.snapshot.herds[0].regrowth_samples = vec![1.0; samples];
        let bare = herd_site_of(&view, &memory()).expect("in the reading, at nothing");
        assert!(bare.unforecast);
        assert_eq!((bare.sustained_food, bare.sustained_hands), (0.0, 0));
        // 0.5 for one, 1.2 for two (0.6 a hunter, the best), 1.5 for three.
        let mut forecast = memory();
        forecast.remember_crew_take(
            BAND,
            HERD_ID,
            curve_of(&[(1, 0.5), (2, 1.2), (3, 1.5)], 1.0),
        );
        let read = herd_site_of(&view, &forecast).expect("forecast");
        assert!(!read.unforecast);
        assert_eq!(
            (read.sustained_food, read.sustained_hands),
            (1.0, 2),
            "the line of 1.0 caps the best crew's 1.2"
        );
        view.snapshot.herds[0].regrowth_samples = vec![2.0; samples];
        let read = herd_site_of(&view, &forecast).expect("forecast");
        assert_eq!(
            (read.sustained_food, read.sustained_hands),
            (1.2, 2),
            "under a line of 2.0 the best crew's 1.2 is the food"
        );
        // A curve that takes nothing reads as nothing, forecast — and is out of the reading.
        let mut nothing = memory();
        nothing.remember_crew_take(BAND, HERD_ID, curve_of(&[(1, 0.0), (2, 0.0)], 1.0));
        assert!(herd_site_of(&view, &nothing).is_none());
    }

    /// A herd site under the `big_game` kit beside two patches: the full reading's first band
    /// wants baskets for the patch hands and two spears' worth of hunters; struck out, the
    /// patches-only reading re-indexes the sites, drops the hex that reached only the herd,
    /// and feeds fewer.
    #[test]
    fn a_planned_band_names_its_kits_and_patches_only_strikes_the_herds_out() {
        let herd_at = Tile::new(14, 8);
        let herd = Site {
            x: herd_at.x,
            y: herd_at.y,
            herd_id: Some("herd_9".to_owned()),
            reach: 5,
            sustained_food: 2.0,
            sustained_hands: 2,
            tended_food: 0.0,
            tended_hands: 0,
            tended_hands_hoed: 0,
            tended_keepers_hoed: 0,
            field_food: 0.0,
            field_hands: 0,
            field_hands_hoed: 0,
            kit_needed: Some("big_game".to_owned()),
            kit_units_held: Some(0),
            unforecast: false,
        };
        let reading = a_reading(vec![
            patch(Tile::new(9, 8), 2.0, 0.5),
            herd,
            patch(Tile::new(11, 8), 1.0, 0.5),
        ]);
        let shape = reading.plan(&LEVERS, HERE, Some(0));
        assert_eq!(shape.bands.len(), 1);
        let band = &shape.bands[0];
        assert_eq!(band.sites, vec![0, 1, 2]);
        assert_eq!(band.baskets, 4 + 2);
        assert_eq!(band.hunt_kits, BTreeMap::from([("big_game".to_owned(), 2)]));
        assert_eq!(shape.people_fed, 25.0);
        let patches = reading.patches_only();
        assert_eq!(patches.sites.len(), 2);
        assert!(patches.sites.iter().all(|site| site.herd_id.is_none()));
        // A hex five steps from the herd and out of reach of both patches is gone.
        assert!(reading.hex_at(Tile::new(19, 8)).is_some());
        assert!(patches.hex_at(Tile::new(19, 8)).is_none());
        let shape = patches.plan(&LEVERS, HERE, Some(0));
        assert_eq!(shape.bands[0].sites, vec![0, 1], "re-indexed");
        assert_eq!(shape.bands[0].hunt_kits, BTreeMap::new());
        assert_eq!(shape.people_fed, 15.0);
        assert_eq!(reading.classify(&LEVERS).kind, StartKind::Short);
        assert_eq!(patches.classify(&LEVERS).kind, StartKind::Short);
    }

    /// The band's own hex is kept when within the tolerance of the best, and not otherwise.
    #[test]
    fn the_own_hex_is_kept_within_the_stay_tolerance() {
        let own = Tile::new(8, 8);
        let better = Tile::new(14, 8);
        // Own hex reaches 2.0; a hex four steps away reaches 2.1 — within a tenth.
        let reading = a_reading(vec![patch(own, 2.0, 0.5), patch(better, 2.1, 0.5)]);
        let shape = reading.plan(&LEVERS, own, Some(0));
        assert_eq!(tile_of(&shape.bands[0]), own);
        let shape = reading.plan(&LEVERS, own, None);
        assert_eq!(tile_of(&shape.bands[0]), own, "kept: {shape:?}");
        let strict = GroundLevers {
            stay_tolerance: 0.0,
            ..LEVERS
        };
        let shape = reading.plan(&strict, own, None);
        assert_ne!(
            tile_of(&shape.bands[0]),
            own,
            "not kept at zero tolerance: {shape:?}"
        );
        assert_eq!(shape.bands[0].sites, vec![1]);
    }

    /// The move target is set only when the best single hex beats the first band by more than
    /// the tolerance and lies beyond the far ring of the band's current hex.
    #[test]
    fn the_move_target_needs_both_a_margin_and_the_far_ring() {
        let near = Tile::new(9, 8);
        let far = Tile::new(20, 8);
        // Far out-values the local first band by more than a tenth and lies ten steps away.
        let reading = a_reading(vec![patch(near, 2.0, 0.5), patch(far, 3.0, 0.5)]);
        let local = reading.plan(&LEVERS, HERE, Some(LEVERS.split_search_tiles));
        assert_eq!(local.bands[0].sites, vec![0]);
        // The target is the nearest hex that reaches the far site, not the site itself.
        let target = local.move_target_tile().expect("a move target: {local:?}");
        assert!(GRID.distance(target, far) <= WORK_RANGE, "{target:?}");
        assert!(GRID.distance(HERE, target) > LEVERS.split_reach_tiles);
        // Not beyond the far ring: no target.
        let within = Tile::new(15, 8);
        assert!(GRID.distance(HERE, within) <= LEVERS.split_reach_tiles);
        let reading = a_reading(vec![patch(near, 2.0, 0.5), patch(within, 3.0, 0.5)]);
        let local = reading.plan(&LEVERS, HERE, Some(LEVERS.split_search_tiles));
        assert_eq!(local.move_target, None);
        // Beyond the ring but not by the margin: no target.
        let reading = a_reading(vec![patch(near, 2.0, 0.5), patch(far, 2.1, 0.5)]);
        let local = reading.plan(&LEVERS, HERE, Some(LEVERS.split_search_tiles));
        assert_eq!(local.move_target, None);
        // The visible covering's first band is the far hex itself, so nothing beats it.
        let reading = a_reading(vec![patch(near, 2.0, 0.5), patch(far, 3.0, 0.5)]);
        let visible = reading.plan(&LEVERS, HERE, None);
        assert_eq!(visible.move_target, None, "{visible:?}");
    }

    /// Each start kind from a fixture: thirty people eating 0.2 need 6.0 a turn.
    #[test]
    fn every_start_kind_is_reachable() {
        let kind = |sites: Vec<Site>| a_reading(sites).classify(&LEVERS).kind;
        // Stay: the own hex reaches 6.0.
        assert_eq!(
            kind(vec![patch(Tile::new(9, 8), 6.0, 0.5)]),
            StartKind::Stay
        );
        // Split local: 3.0 here, 3.0 five steps away (reachable from a hex three steps off).
        assert_eq!(
            kind(vec![
                patch(Tile::new(9, 8), 3.0, 0.5),
                patch(Tile::new(15, 8), 3.0, 0.5)
            ]),
            StartKind::SplitLocal
        );
        // Split far: the second site eight steps away, reachable from six.
        assert_eq!(
            kind(vec![
                patch(Tile::new(9, 8), 3.0, 0.5),
                patch(Tile::new(18, 8), 3.0, 0.5)
            ]),
            StartKind::SplitFar
        );
        // Move all: 1.0 here, 6.0 ten steps away.
        let classified = a_reading(vec![
            patch(Tile::new(9, 8), 1.0, 0.5),
            patch(Tile::new(20, 8), 6.0, 0.5),
        ])
        .classify(&LEVERS);
        assert_eq!(classified.kind, StartKind::MoveAll);
        let target = classified.move_target_tile().expect("a move target");
        assert!(GRID.distance(target, Tile::new(20, 8)) <= WORK_RANGE);
        let around = classified
            .around_target
            .as_ref()
            .expect("the covering around it");
        assert_eq!(around.people_fed, 30.0);
        // The visible covering is the far hex alone: the 1.0 site feeds five, under a founding
        // crew's seven, so it is not a second band.
        assert_eq!(classified.visible.people_fed, 30.0);
        assert_eq!(classified.visible.bands.len(), 1);
        // Short: nothing discovered feeds thirty.
        let classified = a_reading(vec![
            patch(Tile::new(9, 8), 1.0, 0.5),
            patch(Tile::new(20, 8), 2.0, 0.5),
        ])
        .classify(&LEVERS);
        assert_eq!(classified.kind, StartKind::Short);
        assert_eq!(classified.stay.people_fed, 5.0);
        assert_eq!(
            classified.visible.people_fed, 10.0,
            "the 1.0 site is no band"
        );
        assert_eq!(StartKind::SplitLocal.as_str(), "split_local");
        // No sites at all: every covering is empty and the kind is short.
        let classified = a_reading(Vec::new()).classify(&LEVERS);
        assert_eq!(classified.kind, StartKind::Short);
        assert!(classified.visible.bands.is_empty());
        assert_eq!(classified.visible.people_uncovered, POPULATION);
    }
}
