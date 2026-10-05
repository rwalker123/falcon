//! **THE ANCESTORS' REACH RELAYS THROUGH A PEOPLE'S OWN BANDS** (issue #699,
//! `.claude/rules/core_sim/belief.md` → "Kin relay the reach").
//!
//! A band within walking reach of its anchor is near at full strength. A band of the SAME people
//! within walking reach of a near band is near at `relay_per_hop`, and each further hop multiplies by
//! it again — no hop cap, the strength simply dies out.
//!
//! [`resolve_belief_relay`] is the ONE search. `simulate_population` reads a band's hop count off it
//! to blend the culture term, and the snapshot capture reads the same result to publish the hop count
//! and the relayed region — so the morale term and the drawn region cannot disagree.
//!
//! **Same people only, resident bands only.** A detached party is not a camp a band can be tied in
//! through, and another people's band is not your kin. Crossing between peoples belongs to the
//! single neighbour-mixing system #765 calls for (culture, belief and later quantities spreading
//! between neighbouring bands), and this module is its first special case.
//! **Relaying is never adoption**: a band only ever reaches toward an anchor it already holds; which
//! place it holds is decided directly, by `systems::population::refresh_belief_anchor`.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::{Entity, UVec2};

use crate::orders::FactionId;
use crate::routes::RoadRegistry;
use crate::supply::WalkReach;

/// The hop count of a band standing within walking reach of its anchor itself.
pub const DIRECT_HOPS: u32 = 0;

/// Where a band with no durable id sorts — after every band that has one.
const NO_BAND_ID_ORDER: u64 = u64::MAX;

/// **The one order the relay's input is sorted by** — `BandId`, then entity — so the sim and the
/// capture hand the search the same slice and walk it the same way.
pub fn relay_order_key(band_id: Option<crate::components::BandId>, entity: Entity) -> (u64, u64) {
    (
        band_id.map_or(NO_BAND_ID_ORDER, |band| band.0),
        entity.to_bits(),
    )
}

/// One resident band, as the relay search sees it. The caller orders the slice with
/// [`relay_order_key`] — the search's answer does not depend on the order, but its walk does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelayBand {
    pub faction: FactionId,
    /// Where the band STANDS (`current_tile`'s position).
    pub standing: UVec2,
    pub anchor: Option<UVec2>,
}

/// The search key: a people and one anchor, `(y, x)` so the walk is row-major.
type SearchKey = (FactionId, (u32, u32));

fn key_of(band: &RelayBand) -> Option<SearchKey> {
    band.anchor
        .map(|anchor| (band.faction, (anchor.y, anchor.x)))
}

/// The relay search's answer for every band handed in, index-aligned with the input slice.
#[derive(Debug, Clone, Default)]
pub struct BeliefRelay {
    bands: Vec<RelayBand>,
    /// For each `(people, anchor)` searched: every band of that people the anchor's reach got to,
    /// and the hops it took.
    reached: BTreeMap<SearchKey, BTreeMap<usize, u32>>,
}

impl BeliefRelay {
    /// How many hops of kin tie band `band` to its OWN anchor — [`DIRECT_HOPS`] when it stands within
    /// reach itself, `None` with no anchor or when no chain of its people reaches it.
    pub fn hops(&self, band: usize) -> Option<u32> {
        let key = key_of(self.bands.get(band)?)?;
        self.reached.get(&key)?.get(&band).copied()
    }

    /// The relay strength `relay_per_hop ^ hops` toward the band's own anchor; `0` when unreached
    /// or anchorless. At `relay_per_hop = 0` only a direct band reads `1`.
    pub fn strength(&self, band: usize, relay_per_hop: f32) -> f32 {
        const UNREACHED_STRENGTH: f32 = 0.0;
        self.hops(band).map_or(UNREACHED_STRENGTH, |hops| {
            relay_per_hop.powi(i32::try_from(hops).unwrap_or(i32::MAX))
        })
    }

    /// **Where the bands that would tie band `band` in stand** — every band of its people the
    /// anchor's reach gets to WITHOUT `band` itself (any hop), row-major. A band cannot relay to
    /// itself, so the search is re-run with it left out: what a band would be tied in through,
    /// wherever it walked. Their walking reach is the band's relayed region. Empty with no anchor.
    pub fn relayers_without(
        &self,
        band: usize,
        walk: &WalkReach,
        roads: &RoadRegistry,
    ) -> Vec<UVec2> {
        let Some(key) = self.bands.get(band).and_then(key_of) else {
            return Vec::new();
        };
        let kin: Vec<usize> = kin_of(&self.bands, key.0)
            .filter(|&index| index != band)
            .collect();
        let mut positions: Vec<UVec2> = search(&self.bands, &kin, key, walk, roads)
            .keys()
            .map(|&index| self.bands[index].standing)
            .collect();
        positions.sort_by_key(|tile| (tile.y, tile.x));
        positions.dedup();
        positions
    }
}

fn kin_of(bands: &[RelayBand], faction: FactionId) -> impl Iterator<Item = usize> + '_ {
    (0..bands.len()).filter(move |&index| bands[index].faction == faction)
}

/// One breadth-first search for one `(people, anchor)` over the `kin` indices: hop 0 is every band
/// standing within walking reach of the anchor (`walk.within(roads, standing, anchor)` — the term's
/// own test); a band joins at hop `h + 1` when it stands within reach of some band at hop `h`
/// (`walk.within(roads, its standing, that band's standing)`, standing tile first).
fn search(
    bands: &[RelayBand],
    kin: &[usize],
    (_, (y, x)): SearchKey,
    walk: &WalkReach,
    roads: &RoadRegistry,
) -> BTreeMap<usize, u32> {
    let anchor = UVec2::new(x, y);
    let mut hops: BTreeMap<usize, u32> = BTreeMap::new();
    let mut frontier: Vec<usize> = kin
        .iter()
        .copied()
        .filter(|&index| walk.within(roads, bands[index].standing, anchor))
        .collect();
    for &index in &frontier {
        hops.insert(index, DIRECT_HOPS);
    }
    let mut hop = DIRECT_HOPS;
    while !frontier.is_empty() {
        hop += 1;
        let next: Vec<usize> = kin
            .iter()
            .copied()
            .filter(|index| !hops.contains_key(index))
            .filter(|&index| {
                frontier.iter().any(|&relayer| {
                    walk.within(roads, bands[index].standing, bands[relayer].standing)
                })
            })
            .collect();
        for &index in &next {
            hops.insert(index, hop);
        }
        frontier = next;
    }
    hops
}

/// **The one relay search.** For each people and each distinct anchor any of its bands holds, a
/// breadth-first search over that people's bands. A relayer need not hold the anchor itself.
pub fn resolve_belief_relay(
    bands: &[RelayBand],
    walk: &WalkReach,
    roads: &RoadRegistry,
) -> BeliefRelay {
    let searches: BTreeSet<SearchKey> = bands.iter().filter_map(key_of).collect();
    let reached = searches
        .into_iter()
        .map(|key| {
            let kin: Vec<usize> = kin_of(bands, key.0).collect();
            (key, search(bands, &kin, key, walk, roads))
        })
        .collect();
    BeliefRelay {
        bands: bands.to_vec(),
        reached,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A no-road walk at a base reach of 4 on a wide, unwrapped grid.
    const BASE_REACH: f32 = 4.0;
    const FREE_REACH: u32 = 3;
    const GRID: u32 = 64;
    const HALF: f32 = 0.5;
    const NO_RELAY: f32 = 0.0;
    const HOME: FactionId = FactionId(0);
    const OTHER: FactionId = FactionId(1);

    fn walk() -> WalkReach {
        WalkReach {
            base_reach: BASE_REACH,
            free_reach: FREE_REACH,
            widest_route_reach: FREE_REACH,
            width: GRID,
            height: GRID,
            wrap: false,
        }
    }

    fn band(faction: FactionId, x: u32, anchor: Option<u32>) -> RelayBand {
        RelayBand {
            faction,
            standing: UVec2::new(x, 0),
            anchor: anchor.map(|ax| UVec2::new(ax, 0)),
        }
    }

    #[test]
    fn a_chain_of_kin_halves_per_hop_and_another_people_relays_nothing() {
        let bands = [
            band(HOME, 2, Some(0)),
            band(HOME, 6, Some(0)),
            band(HOME, 10, Some(0)),
            band(OTHER, 6, Some(0)),
            band(OTHER, 10, Some(0)),
        ];
        let relay = resolve_belief_relay(&bands, &walk(), &RoadRegistry::default());
        assert_eq!(relay.hops(0), Some(0));
        assert_eq!(relay.hops(1), Some(1));
        assert_eq!(relay.hops(2), Some(2));
        assert_eq!(relay.strength(2, HALF), HALF * HALF);
        // The other people's band at 6 has no kin within reach of the anchor, so neither it nor the
        // one beyond it is reached — the home chain does not carry them.
        assert_eq!(relay.hops(3), None);
        assert_eq!(relay.hops(4), None);
        assert_eq!(relay.strength(4, HALF), 0.0);
    }

    #[test]
    fn an_anchorless_band_reaches_nothing_and_relay_zero_keeps_only_direct() {
        let bands = [
            band(HOME, 2, Some(0)),
            band(HOME, 6, None),
            band(HOME, 6, Some(0)),
        ];
        let relay = resolve_belief_relay(&bands, &walk(), &RoadRegistry::default());
        assert_eq!(relay.hops(1), None, "no anchor, nothing to reach toward");
        assert_eq!(relay.strength(0, NO_RELAY), 1.0);
        assert_eq!(relay.strength(2, NO_RELAY), 0.0);
    }

    /// A band's relayers never include itself, nor bands reached only through it.
    #[test]
    fn a_band_is_not_its_own_relayer() {
        let bands = [
            band(HOME, 2, Some(0)),
            band(HOME, 6, Some(0)),
            band(HOME, 10, Some(0)),
        ];
        let relay = resolve_belief_relay(&bands, &walk(), &RoadRegistry::default());
        let roads = RoadRegistry::default();
        assert_eq!(
            relay.relayers_without(2, &walk(), &roads),
            vec![UVec2::new(2, 0), UVec2::new(6, 0)]
        );
        assert_eq!(
            relay.relayers_without(1, &walk(), &roads),
            vec![UVec2::new(2, 0)],
            "the band at 10 is reached only through the band at 6"
        );
    }
}
