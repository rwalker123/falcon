//! ⛔ **ONE BAND OF A PEOPLE PER TAKE SOURCE.**
//!
//! Within one people a forage patch, a herd or a deposit working is worked by ONE band. The labor
//! pass settles bands in `BandId` order and every take draws the source down at that band's own
//! floor, so two bands of one people on one source meant the lower-numbered band took its share and
//! the other brought home nothing. Across peoples nothing changes: bands of different peoples on one
//! source are still settled in `BandId` order.
//!
//! This module is the one seam that answers *which bands of a faction hold a row on this source, and
//! is each row the chief's (listed under an order) or the player's*:
//!
//! - [`SourceHolders`] is every band's holdings, keyed by [`crate::keep_on_hand::source_key`]
//!   (`f:x,y`, `h:herd`, `d:material:x,y`). The labor pass builds it from every band's allocation
//!   before the per-band loop and refreshes a band's entries after that band's managing passes; the
//!   command path builds it from the world ([`SourceHolders::from_world`]).
//! - [`OtherBands`] is that map as one band sees it: the OTHER bands of its people only.
//!
//! Who reads it: `food_orders::manage_food_orders` and `keep_on_hand::manage_dig_rows` (a chief never
//! lists a source another band of his people holds, and retires a duplicate row he loses),
//! `assign_labor` and the staffing preview ([`other_band_working`]), and the AI seat's source picks.
//!
//! Scope: [`LaborTarget::Forage`], [`LaborTarget::Hunt`] and [`LaborTarget::Extract`]. A tile
//! improvement, a band improvement, Roadwork, Scout and Warrior are not take sources: several bands
//! may build one tile improvement together.

use std::collections::BTreeMap;

use bevy::prelude::World;

use crate::components::{BandId, LaborAllocation, LaborTarget, PopulationCohort};
use crate::keep_on_hand::source_key;
use crate::orders::FactionId;

/// **THE KEY OF A TAKE SOURCE ROW**, `None` for any other target.
pub fn take_source_key(target: &LaborTarget) -> Option<String> {
    match target {
        LaborTarget::Forage { .. } | LaborTarget::Hunt { .. } | LaborTarget::Extract { .. } => {
            source_key(target)
        }
        _ => None,
    }
}

/// One band's row on a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holder {
    pub band: BandId,
    pub faction: FactionId,
    /// Whether the row is listed under a chief's order (`order_rows`); `false` is the player's.
    pub listed: bool,
}

/// **EVERY BAND'S HOLDINGS ON THE TAKE SOURCES**, keyed by source key.
#[derive(Debug, Clone, Default)]
pub struct SourceHolders {
    by_key: BTreeMap<String, Vec<Holder>>,
}

impl SourceHolders {
    /// The holdings of `bands` (a band with no durable id holds nothing here).
    pub fn from_bands<'a>(
        bands: impl IntoIterator<Item = (Option<BandId>, FactionId, &'a LaborAllocation)>,
    ) -> Self {
        let mut holders = Self::default();
        for (band, faction, allocation) in bands {
            holders.set_band(band, faction, allocation);
        }
        holders
    }

    /// The holdings of every band in the world.
    pub fn from_world(world: &World) -> Self {
        Self::from_bands(world.iter_entities().filter_map(|entity| {
            let cohort = entity.get::<PopulationCohort>()?;
            let allocation = entity.get::<LaborAllocation>()?;
            Some((entity.get::<BandId>().copied(), cohort.faction, allocation))
        }))
    }

    /// Replace `band`'s entries with what `allocation` holds now.
    pub fn set_band(
        &mut self,
        band: Option<BandId>,
        faction: FactionId,
        allocation: &LaborAllocation,
    ) {
        let Some(band) = band else {
            return;
        };
        for rows in self.by_key.values_mut() {
            rows.retain(|holder| holder.band != band);
        }
        self.by_key.retain(|_, rows| !rows.is_empty());
        for row in &allocation.assignments {
            let Some(key) = take_source_key(&row.target) else {
                continue;
            };
            let listed = allocation.order_rows.contains_key(&key);
            self.by_key.entry(key).or_default().push(Holder {
                band,
                faction,
                listed,
            });
        }
    }

    /// The other bands of `faction` as `band` sees them.
    pub fn for_band(&self, band: Option<BandId>, faction: FactionId) -> OtherBands {
        let held = self
            .by_key
            .iter()
            .filter_map(|(key, rows)| {
                let others: Vec<(BandId, bool)> = rows
                    .iter()
                    .filter(|holder| holder.faction == faction && Some(holder.band) != band)
                    .map(|holder| (holder.band, holder.listed))
                    .collect();
                (!others.is_empty()).then(|| (key.clone(), others))
            })
            .collect();
        OtherBands { me: band, held }
    }
}

/// **THE OTHER BANDS OF ONE BAND'S PEOPLE, and the rows they hold on each take source.**
#[derive(Debug, Clone, Default)]
pub struct OtherBands {
    me: Option<BandId>,
    held: BTreeMap<String, Vec<(BandId, bool)>>,
}

impl OtherBands {
    /// No other band holds anything (a fixture, or a band alone of its people).
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether any other band of the people holds a row on `key`.
    pub fn holds(&self, key: &str) -> bool {
        self.held.contains_key(key)
    }

    /// The lowest-numbered other band holding a row on `key`.
    pub fn first_holder(&self, key: &str) -> Option<BandId> {
        self.held
            .get(key)?
            .iter()
            .map(|(band, _)| *band)
            .min_by_key(|band| band.0)
    }

    /// **DOES ANOTHER BAND'S ROW TAKE THE SOURCE FROM THIS BAND'S CHIEF-LISTED ROW?** True when that
    /// row is the player's, or is chief-listed and its band is the lower-numbered one. Between two
    /// chief rows the higher `BandId` retires; two player rows are left alone.
    pub fn outranks_my_listed_row(&self, key: &str) -> bool {
        let me = self.me.map_or(u64::MAX, |band| band.0);
        self.held
            .get(key)
            .is_some_and(|rows| rows.iter().any(|(band, listed)| !listed || band.0 < me))
    }

    /// **IS `key` OFF LIMITS TO THIS BAND'S CHIEF AS A CANDIDATE?** A source the band holds no row on
    /// is off limits when any other band of its people holds one; a chief-listed row the band already
    /// holds is off limits only when it loses the duplicate ([`Self::outranks_my_listed_row`]).
    pub fn blocks_candidate(&self, key: &str, mine_listed: bool) -> bool {
        if mine_listed {
            self.outranks_my_listed_row(key)
        } else {
            self.holds(key)
        }
    }
}

/// **THE OTHER BAND OF `band`'S PEOPLE THAT WORKS `target`, WHEN `band` HAS NO ROW THERE**: the
/// refusal `assign_labor` and the staffing preview give a new row on a source another band of the
/// same people already holds. `None` for a source `band` already holds a row on (editing it is never
/// refused), for a target that is no take source, and where no other band of the people holds one.
pub fn other_band_working(
    world: &World,
    band: bevy::prelude::Entity,
    target: &LaborTarget,
) -> Option<BandId> {
    let key = take_source_key(target)?;
    let allocation = world.get::<LaborAllocation>(band)?;
    if allocation
        .assignments
        .iter()
        .any(|row| row.target.same_source(target))
    {
        return None;
    }
    let faction = world.get::<PopulationCohort>(band)?.faction;
    SourceHolders::from_world(world)
        .for_band(world.get::<BandId>(band).copied(), faction)
        .first_holder(&key)
}
