//! Supply network — per-faction, throughput-limited goods sharing between nearby bands.
//!
//! Every band is a small logistics node holding a local goods store (`PopulationCohort.stores`).
//! Each turn `balance_supply_networks` connects same-faction bands within `reach_tiles` into
//! **supply networks** (connected components) and moves each commodity toward a **per-capita
//! balance** across the network — capped at `throughput_per_turn` per node and losing `friction`
//! in transit. So a gatherer band automatically feeds a scouting band it's near, while a detached
//! band lives off its own larder. Runs in `TurnStage::Logistics` (before `TurnStage::Population`
//! consumes), so balanced larders are eaten the same turn.
//!
//! This is the general mechanism the design scales later: raise reach/throughput for settlements
//! and cities, and add a *trade policy* (consent + priced return flow) on cross-faction edges.
//! `docs/plan_settlement_population.md`.

use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::math::UVec2;
use bevy::prelude::*;

use crate::{
    components::{PopulationCohort, ResidentBand, Tile},
    grid_utils::wrapped_distance_sq,
    orders::FactionId,
    resources::{SimulationConfig, TileRegistry},
    scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar},
    supply_network_config::SupplyNetworkConfigHandle,
};

/// Per-turn supply-network membership: `entity → network id`. Recomputed every turn by
/// `balance_supply_networks`. `id >= 1` is a stable-per-snapshot id shared by every band in the
/// same multi-band connected component; a band absent from the map (singleton/isolated) reads `0`.
/// Not snapshot-persisted — it is a derived readout the capture reads to tag each cohort so the
/// client can draw supply links between members of the same network.
#[derive(Resource, Default)]
pub struct SupplyNetworkMembership(pub HashMap<Entity, u32>);

impl SupplyNetworkMembership {
    /// The network id for a band this turn: `0` when it is not in a multi-band network.
    pub fn network_of(&self, entity: Entity) -> u32 {
        self.0.get(&entity).copied().unwrap_or(0)
    }
}

/// A band in a single-member component is not part of a shared network.
const MIN_NETWORK_MEMBERS: usize = 2;
/// First multi-band network's id (singletons read `0`).
const FIRST_NETWORK_ID: u32 = 1;

/// One band participating in the supply network this turn (a snapshot taken before any transfers,
/// so all flows resolve against the turn's opening stores).
struct Node {
    entity: Entity,
    faction: FactionId,
    pos: UVec2,
    /// Per-capita balancing weight = population.
    weight: Scalar,
    /// Opening goods store (commodity → quantity), sorted for determinism.
    stores: Vec<(String, Scalar)>,
}

impl Node {
    fn store_of(&self, commodity: &str) -> Scalar {
        self.stores
            .iter()
            .find(|(k, _)| k == commodity)
            .map(|(_, v)| *v)
            .unwrap_or_else(scalar_zero)
    }
}

/// Iterative path-halving union-find root lookup.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Pure per-commodity balancer for one supply network (no ECS). Given each member's population
/// `weights` and current `stores` of a single commodity, return the net change to apply to each
/// member (index-aligned): surplus nodes above their per-capita fair share ship (capped at
/// `throughput`), `friction` is lost in transit, and the remaining pool is split among deficit
/// nodes in proportion to how much each is short. Transfers below `min_transfer` are dropped so a
/// balanced network doesn't churn. Net change over the network is `-friction × amount shipped`.
fn balance_commodity(
    weights: &[Scalar],
    stores: &[Scalar],
    throughput: Scalar,
    friction: Scalar,
    min_transfer: Scalar,
) -> Vec<Scalar> {
    let n = weights.len();
    let mut deltas = vec![scalar_zero(); n];
    let total_weight = weights.iter().copied().fold(scalar_zero(), |a, b| a + b);
    if total_weight <= scalar_zero() {
        return deltas;
    }
    let total = stores.iter().copied().fold(scalar_zero(), |a, b| a + b);
    let mut sends = vec![scalar_zero(); n];
    let mut wants = vec![scalar_zero(); n];
    for i in 0..n {
        let fair = total * (weights[i] / total_weight);
        if stores[i] > fair {
            let send = min(stores[i] - fair, throughput);
            if send >= min_transfer {
                sends[i] = send;
            }
        } else {
            let want = min(fair - stores[i], throughput);
            if want >= min_transfer {
                wants[i] = want;
            }
        }
    }
    let total_sends = sends.iter().copied().fold(scalar_zero(), |a, b| a + b);
    let total_wants = wants.iter().copied().fold(scalar_zero(), |a, b| a + b);
    if total_sends <= scalar_zero() || total_wants <= scalar_zero() {
        return deltas;
    }
    let pool = total_sends * (scalar_one() - friction);
    // Receivers can absorb at most `total_wants` in aggregate (each capped at its own want),
    // so only `deliverable` actually arrives; senders must ship `deliverable / (1 - friction)`
    // to deliver it — never more, or the surplus is destroyed beyond friction.
    let deliverable = min(pool, total_wants);
    let one_minus_friction = scalar_one() - friction;
    let shipped = if one_minus_friction > scalar_zero() {
        min(deliverable / one_minus_friction, total_sends)
    } else {
        scalar_zero()
    };
    let ship_ratio = shipped / total_sends; // total_sends > 0 here (guarded above); ≤ 1
    for i in 0..n {
        if sends[i] > scalar_zero() {
            deltas[i] = -(sends[i] * ship_ratio);
        } else if wants[i] > scalar_zero() {
            deltas[i] = min(deliverable * (wants[i] / total_wants), wants[i]);
        }
    }
    deltas
}

pub fn balance_supply_networks(
    config: Res<SupplyNetworkConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // `With<ResidentBand>`: an expedition manages its own larder — its drop-off is the explicit
    // fold-back on arrival, not a passive supply-network leak — so it is excluded here.
    mut cohorts: Query<(Entity, &mut PopulationCohort), With<ResidentBand>>,
    mut membership: ResMut<SupplyNetworkMembership>,
) {
    // Recomputed from scratch every turn; a 0/1-band map (early return below) leaves it empty.
    membership.0.clear();
    let cfg = config.get();
    let reach_sq = (cfg.reach_tiles * cfg.reach_tiles) as i32;
    let width = tile_registry.width;
    let wrap = sim_config.map_topology.wrap_horizontal;
    let throughput = scalar_from_f32(cfg.throughput_per_turn);
    let friction = scalar_from_f32(cfg.friction).clamp(scalar_zero(), scalar_one());
    let min_transfer = scalar_from_f32(cfg.min_transfer);

    // Pass 1: snapshot each band's position, population weight, and opening stores.
    let mut nodes: Vec<Node> = Vec::new();
    for (entity, cohort) in cohorts.iter() {
        let Ok(tile) = tiles.get(cohort.current_tile) else {
            continue;
        };
        nodes.push(Node {
            entity,
            faction: cohort.faction,
            pos: tile.position,
            weight: cohort.total(),
            stores: cohort
                .stores
                .iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        });
    }
    if nodes.len() < 2 {
        return;
    }
    // Deterministic node order for the union-find and all downstream iteration.
    nodes.sort_by_key(|node| node.entity.to_bits());

    // Union same-faction nodes within reach into supply networks. Rather than an O(n²) all-pairs
    // scan, bin nodes into a spatial hash of `cell_size`-tile cells (cell_size = reach, so any two
    // nodes within reach fall in the same or an adjacent cell) keyed by faction, then compare each
    // node only against candidates in its neighbouring cells.
    let count = nodes.len();
    let mut parent: Vec<usize> = (0..count).collect();

    let cell_size = cfg.reach_tiles.max(1);
    let num_cells_x = width.div_ceil(cell_size).max(1) as i32;
    let cell_of =
        |pos: UVec2| -> (i32, i32) { ((pos.x / cell_size) as i32, (pos.y / cell_size) as i32) };
    let mut bins: HashMap<(FactionId, i32, i32), Vec<usize>> = HashMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        let (cx, cy) = cell_of(node.pos);
        bins.entry((node.faction, cx, cy)).or_default().push(idx);
    }
    // With horizontal wrap, a runt seam cell (when width isn't a multiple of cell_size) can leave
    // two within-reach nodes two cells apart across the seam, so search ±2 in x (folded into range)
    // when wrapping, ±1 otherwise. y never wraps.
    let x_offsets: &[i32] = if wrap {
        &[-2, -1, 0, 1, 2]
    } else {
        &[-1, 0, 1]
    };
    for i in 0..count {
        let (cx, cy) = cell_of(nodes[i].pos);
        let mut seen_cells: BTreeSet<(i32, i32)> = BTreeSet::new();
        for &dcy in &[-1, 0, 1] {
            for &dcx in x_offsets {
                let ncx = if wrap {
                    (cx + dcx).rem_euclid(num_cells_x)
                } else {
                    cx + dcx
                };
                let ncy = cy + dcy;
                if !seen_cells.insert((ncx, ncy)) {
                    continue; // wrap folding can repeat a cell on tiny maps
                }
                let Some(candidates) = bins.get(&(nodes[i].faction, ncx, ncy)) else {
                    continue;
                };
                for &j in candidates {
                    if j <= i {
                        continue; // each unordered pair once; also skips self
                    }
                    if wrapped_distance_sq(nodes[i].pos, nodes[j].pos, width, wrap) <= reach_sq {
                        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                        if a != b {
                            parent[a] = b;
                        }
                    }
                }
            }
        }
    }
    let mut components: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..count {
        let root = find(&mut parent, i);
        components.entry(root).or_default().push(i);
    }

    // Assign each multi-band component a stable id (BTreeMap root order → deterministic), then
    // record `entity → id` for its members so the snapshot can group bands by network. Singletons
    // get no entry and read 0.
    let mut next_network_id = FIRST_NETWORK_ID;
    for members in components.values() {
        if members.len() < MIN_NETWORK_MEMBERS {
            continue;
        }
        let network_id = next_network_id;
        next_network_id += 1;
        for &m in members {
            membership.0.insert(nodes[m].entity, network_id);
        }
    }

    // Compute all transfers against the opening snapshot, then apply them once at the end.
    let mut applied: Vec<(Entity, String, Scalar)> = Vec::new();
    for members in components.values() {
        if members.len() < MIN_NETWORK_MEMBERS {
            continue;
        }
        let weights: Vec<Scalar> = members.iter().map(|&m| nodes[m].weight).collect();
        let mut commodities: BTreeSet<&str> = BTreeSet::new();
        for &m in members {
            for (item, _) in &nodes[m].stores {
                commodities.insert(item.as_str());
            }
        }
        for commodity in commodities {
            let stores: Vec<Scalar> = members
                .iter()
                .map(|&m| nodes[m].store_of(commodity))
                .collect();
            let deltas = balance_commodity(&weights, &stores, throughput, friction, min_transfer);
            for (k, &m) in members.iter().enumerate() {
                if deltas[k] != scalar_zero() {
                    applied.push((nodes[m].entity, commodity.to_string(), deltas[k]));
                }
            }
        }
    }

    for (entity, commodity, delta) in applied {
        if let Ok((_, mut cohort)) = cohorts.get_mut(entity) {
            cohort.stores.add(&commodity, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::balance_commodity;
    use crate::scalar::{scalar_from_f32, Scalar};

    fn s(v: f32) -> Scalar {
        scalar_from_f32(v)
    }

    /// Two equal bands, one full and one empty, equalize per-capita when throughput allows.
    #[test]
    fn equal_bands_equalize() {
        let d = balance_commodity(
            &[s(1.0), s(1.0)],
            &[s(100.0), s(0.0)],
            s(1000.0),
            s(0.0),
            s(0.0),
        );
        assert!(
            (d[0].to_f32() + 50.0).abs() < 1e-3,
            "surplus ships 50: {}",
            d[0].to_f32()
        );
        assert!(
            (d[1].to_f32() - 50.0).abs() < 1e-3,
            "deficit receives 50: {}",
            d[1].to_f32()
        );
    }

    /// Throughput caps how fast a gap closes — one turn moves only `throughput`, not the whole gap.
    #[test]
    fn throughput_limits_the_rate() {
        let d = balance_commodity(
            &[s(1.0), s(1.0)],
            &[s(100.0), s(0.0)],
            s(10.0),
            s(0.0),
            s(0.0),
        );
        assert!((d[0].to_f32() + 10.0).abs() < 1e-3);
        assert!((d[1].to_f32() - 10.0).abs() < 1e-3);
    }

    /// Friction is lost in transit — the network's total shrinks by `friction × shipped`.
    #[test]
    fn friction_is_lost_in_transit() {
        let d = balance_commodity(
            &[s(1.0), s(1.0)],
            &[s(100.0), s(0.0)],
            s(1000.0),
            s(0.2),
            s(0.0),
        );
        assert!((d[0].to_f32() + 50.0).abs() < 1e-3, "ships 50");
        assert!(
            (d[1].to_f32() - 40.0).abs() < 1e-3,
            "receives 40 after 20% friction"
        );
        let net = d[0].to_f32() + d[1].to_f32();
        assert!((net + 10.0).abs() < 1e-3, "10 lost to friction: {net}");
    }

    /// Balance is per-capita: a 3×-larger band's fair share is 3× as much food.
    #[test]
    fn balance_is_per_capita() {
        // weights 3 and 1, both holding 40 (total 80) → fair shares 60 and 20.
        let d = balance_commodity(
            &[s(3.0), s(1.0)],
            &[s(40.0), s(40.0)],
            s(1000.0),
            s(0.0),
            s(0.0),
        );
        let after0 = 40.0 + d[0].to_f32();
        let after1 = 40.0 + d[1].to_f32();
        assert!((after0 - 60.0).abs() < 1e-3, "big band → 60: {after0}");
        assert!((after1 - 20.0).abs() < 1e-3, "small band → 20: {after1}");
        // Per-capita holdings are equal.
        assert!(((after0 / 3.0) - (after1 / 1.0)).abs() < 1e-3);
    }

    /// A near-balanced network doesn't churn: sub-`min_transfer` moves are dropped.
    #[test]
    fn min_transfer_dead_band() {
        let d = balance_commodity(
            &[s(1.0), s(1.0)],
            &[s(51.0), s(49.0)],
            s(1000.0),
            s(0.0),
            s(5.0),
        );
        assert!(d[0].to_f32().abs() < 1e-6, "no churn: {}", d[0].to_f32());
        assert!(d[1].to_f32().abs() < 1e-6);
    }

    /// Aggregate send capacity can exceed one throughput-capped receiver's demand; the network must
    /// only lose `friction × shipped`, never destroy the un-absorbed surplus. (Regression: senders
    /// used to ship their full surplus even when receivers couldn't take it.)
    #[test]
    fn excess_supply_is_not_destroyed() {
        let friction = 0.05_f32;
        let d = balance_commodity(
            &[s(1.0), s(1.0), s(1.0)],
            &[s(40.0), s(40.0), s(0.0)],
            s(50.0), // throughput caps the single receiver
            s(friction),
            s(0.0),
        );
        let shipped: f32 = d
            .iter()
            .map(|x| x.to_f32())
            .filter(|&v| v < 0.0)
            .map(|v| -v)
            .sum();
        let net: f32 = d.iter().map(|x| x.to_f32()).sum();
        // Only friction is lost — not the un-absorbable surplus.
        assert!(
            (net + friction * shipped).abs() < 1e-2,
            "network should lose only friction×shipped; net={net}, shipped={shipped}"
        );
        // And the receiver actually gained something.
        assert!(
            d[2].to_f32() > 0.0,
            "receiver got nothing: {}",
            d[2].to_f32()
        );
    }

    /// An already-balanced network is a no-op.
    #[test]
    fn balanced_network_is_noop() {
        let d = balance_commodity(
            &[s(1.0), s(1.0)],
            &[s(50.0), s(50.0)],
            s(1000.0),
            s(0.0),
            s(0.0),
        );
        assert!(d.iter().all(|x| x.to_f32().abs() < 1e-6));
    }
}
