//! Supply network — throughput-limited goods sharing between **connected** bands standing near
//! each other.
//!
//! Every band is a small logistics node holding a local goods store (`PopulationCohort.stores`).
//! Each turn `balance_supply_networks` joins bands **of one people** that are within `reach_tiles`
//! **hex steps** of each other **and** hold a live tie in [`crate::connections::ConnectionLedger`]
//! into **supply
//! networks** (connected components) and moves each commodity toward a **per-capita balance** across
//! the network — capped at `throughput_per_turn` per node and losing `friction` in transit. So a
//! gatherer band automatically feeds a scouting band it's near, while a band that is detached, one
//! nobody has met, or one belonging to another people lives off its own larder. Runs in
//! `TurnStage::Logistics` (before `TurnStage::Population` consumes), so balanced larders are eaten
//! the same turn.
//!
//! # A logistics link is a rider on a CONNECTION
//!
//! The edge used to be derived implicitly from proximity alone, which made this the second
//! independent implementation of *"goods move between two bands"* beside a trade shipment's tie
//! gate. It is now the same object: proximity produces a connection, and over a short distance a
//! logistics link is cheap enough to **hold itself for free** — which is what `reach_tiles` means
//! now (`docs/plan_contact_and_logistics.md` §Q4). Beyond it a link needs a route to hold it open,
//! and that state belongs to the route ladder, not here: there is deliberately no `LogisticsLink`
//! component or resource.
//!
//! # The LINK is faction-blind; what the balancer POOLS OVER is policy
//!
//! Two questions, deliberately separated. *Does a logistics link exist between these two bands?* is
//! the arc's edge and asks nothing about faction — see [`tie_is_live`]. *Do they equalize their
//! larders for free across it?* is this rider's own policy, and the answer is **only within one
//! people** — see [`pools_freely`]. The connection primitive's discipline ("no faction field on the
//! edge, no faction branch inside `connections.rs`") governs the edge, not what every rider decides
//! to do with one.
//! `docs/plan_settlement_population.md`.

use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::math::UVec2;
use bevy::prelude::*;

use crate::{
    components::{
        BandId, LaborAllocation, MaterialBatch, PopulationCohort, ResidentBand, Tile, TransferLink,
        FODDER, FOOD,
    },
    connections::{ConnectionKey, ConnectionLedger, NO_TIE},
    grid_utils::hex_distance_wrapped,
    materials_config::BandKey,
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

/// **What a material batch is balanced AS** — one material at one per-axis band.
///
/// This is the whole of "materials never pool as a scalar": the balancer is run once per *rating*,
/// not once per material, so a mammoth hide (`toughness excellent`) and a hare pelt
/// (`toughness poor`) are two different commodities to it and can never be averaged into each
/// other. Inside one rating the exact readings **are** blended, and that is the store's own merge
/// rule — the same thing that happens when two of the band's own hunts land in the same band.
type MaterialKey = (String, BandKey);

/// One band participating in the supply network this turn (a snapshot taken before any transfers,
/// so all flows resolve against the turn's opening stores).
struct Node {
    entity: Entity,
    /// **The endpoint's identity.** A cohort with no [`BandId`] has nothing to tie, so it is never
    /// collected as a node at all and simply never joins a network.
    band: BandId,
    /// **Whose people this band is** — read by [`pools_freely`], never by the link rule.
    faction: FactionId,
    pos: UVec2,
    /// Per-capita balancing weight = population.
    weight: Scalar,
    /// Opening goods store (commodity → quantity), sorted for determinism.
    stores: Vec<(String, Scalar)>,
    /// Opening **material** batches, keyed by rating. Sorted for determinism, exactly as `stores` is.
    materials: BTreeMap<MaterialKey, MaterialBatch>,
}

impl Node {
    fn store_of(&self, commodity: &str) -> Scalar {
        self.stores
            .iter()
            .find(|(k, _)| k == commodity)
            .map(|(_, v)| *v)
            .unwrap_or_else(scalar_zero)
    }

    fn material_amount(&self, key: &MaterialKey) -> Scalar {
        self.materials
            .get(key)
            .map(|batch| batch.amount)
            .unwrap_or_else(scalar_zero)
    }
}

/// **The link rule.** An undirected logistics link exists between two resident bands iff they are
/// within `reach_tiles` **hex steps** of each other ([`crate::grid_utils::hex_distance_wrapped`])
/// *and* the ledger holds a live tie (`strength > NO_TIE`) in at least one direction.
///
/// **Reach is measured in hex distance, like every other radius in the sim** — `band_work_range`,
/// the hunt leash, a predator's prey-sensing disk. It was squared Euclidean on offset coordinates
/// until the route branch, which is *stricter than it reads* at the diagonals: two camps 3 hex
/// steps apart at
/// `(53,15)`/`(56,14)` measure `3² + 1² = 10` against a threshold of `9` and were excluded by one
/// unit, so `reach_tiles: 3` did not mean three hexes. Hex distance widens pooling at the diagonals
/// and is a **gameplay change**, deliberately taken so the shipped lever means what it says.
///
/// **Either direction, not both.** A connection is directed — *who found whom* — and whether a
/// rider requires mutuality is the rider's business (`connections.rs`). This rider does not:
/// pooling is one undirected mechanism, and requiring both edges would make the commonest traffic
/// in the game depend on two independent sight sweeps agreeing on the same turn.
///
/// **A parked tie does not pool.** `strength == NO_TIE` is the keystone's *"at zero nothing
/// flows"*: the edge still exists — we know such a people exist — and it carries nothing.
fn tie_is_live(ledger: &ConnectionLedger, a: BandId, b: BandId) -> bool {
    [ConnectionKey::new(a, b), ConnectionKey::new(b, a)]
        .iter()
        .any(|key| {
            ledger
                .get(key)
                .is_some_and(|connection| connection.strength > NO_TIE)
        })
}

/// **The pooling policy — free per-capita equalization is a same-faction affordance.**
///
/// The link is faction-blind ([`tie_is_live`]): any two connected neighbours have one. What rides
/// over it is this rider's decision, and free equalization only makes sense within one people — a
/// parent band feeding the splinter it just calved has one interest at both ends. Between two
/// peoples the same move is not trade at all; it is your larder draining into a stranger's because
/// they camped nearby. Consent and price are a *priced exchange*'s to model (#546) and a shipment's
/// to carry (#517), so the balancer must not pre-empt that design with an accidental default that
/// would activate silently the day a second faction lands.
///
/// **It gates the UNION, not the balancing of an already-built component**, and that is the
/// non-obvious part. Partitioning each component by faction afterwards would let bands A and C of
/// one people — each within reach of a foreign band B, and neither within reach of the other — land
/// in one component and pool *through* B, relaying goods across a stranger's camp. Gating the union
/// reproduces exactly the pairing the proximity-only network had, with no relay.
fn pools_freely(a: &Node, b: &Node) -> bool {
    a.faction == b.faction
}

/// ⛔ **CAN THESE TWO CAMPS POOL AT THIS DISTANCE — THE FREE REACH, **OR** WHAT THE ROAD BETWEEN
/// THEM HOLDS OPEN?**
///
/// ```text
/// hex_distance(a, b) <= max(reach_tiles, path_reach_tiles(roads, trace_path(a, b, ..)))
/// ```
///
/// **Purely additive**: a pair inside `reach_tiles` pools exactly as it does today, and the other two
/// gates ([`pools_freely`], [`tie_is_live`]) are untouched. This is the first consumer
/// `RungRoutePayoff::holds_link_to_tiles` has ever had — the reach the client was rendering in the
/// future tense — and it is what makes the top rungs a **capability** rather than a discount: without
/// a road two bands six tiles apart cannot pool at all.
///
/// **Reach takes the run's weakest tile** ([`crate::routes::path_reach_tiles`]), so one bare tile in
/// an otherwise paved run holds nothing open — a link goods must get *through* is not
/// most-of-the-way-there.
///
/// **Cost**: the trace only runs once the free test has already failed **and** the pair is within
/// `widest_route_reach`, so a game with no roads — the shipped turn-1 state — traces nothing.
#[allow(clippy::too_many_arguments)] // The geometry a hex distance needs, plus the two reaches.
fn link_holds(
    roads: &crate::routes::RoadRegistry,
    a: UVec2,
    b: UVec2,
    free_reach: u32,
    widest_route_reach: u32,
    width: u32,
    height: u32,
    wrap: bool,
) -> bool {
    let distance = hex_distance_wrapped(a, b, width, wrap);
    if distance <= free_reach {
        return true;
    }
    if distance > widest_route_reach {
        return false;
    }
    let path = crate::routes::trace_path(a, b, width, height, wrap, roads);
    distance <= crate::routes::path_reach_tiles(roads, &path)
}

/// ⛔ **WHAT A COMPONENT'S POOLING LOSES IN TRANSIT, as a multiple of the base friction — DERIVED
/// FROM THE TILES, never stored** (`docs/plan_standing_upkeep.md` §4.13b).
///
/// For each pooling link in this component, walk the tiles between its two camps
/// ([`crate::routes::trace_path`]) and **average** what the roads on them are worth
/// ([`crate::routes::path_friction_multiplier`]): you genuinely lose less over the roaded stretch of
/// a haul, so **a partly-built road pays partly**. That is the per-tile model's own answer, and it is
/// what the retired *"best road binding the network"* rule could not express — reading a path of
/// tiles, *best* would call a thirty-tile dirt road with one paved tile a paved road.
///
/// ⛔ **THE COMPONENT TAKES ITS BEST LINK, AND THAT IS FORCED RATHER THAN GENEROUS.**
/// [`balance_commodity`] pools a whole component against **one** friction scalar and has no path
/// model, so any per-component reading is an approximation of the same thing. Under a *worst*-link
/// reading, a component that gained a new unroaded neighbour would see its friction **rise** — a
/// band punished for having walked somewhere, and *"a rung can only widen the set of links and lower
/// a loss, never the reverse"* broken outright. Each link's own reading is monotone-improving in the
/// roads beneath it, and a minimum of monotone-improving readings is monotone-improving too.
///
/// A component with no link of its own — which a singleton cannot have — pools at exactly today's
/// friction, so there is no early-game regression, by construction.
fn component_friction(
    roads: &crate::routes::RoadRegistry,
    nodes: &[Node],
    links: &[(usize, usize)],
    members: &[usize],
    width: u32,
    height: u32,
    wrap: bool,
) -> f32 {
    links
        .iter()
        .filter(|(i, j)| members.contains(i) && members.contains(j))
        .map(|(i, j)| {
            let path =
                crate::routes::trace_path(nodes[*i].pos, nodes[*j].pos, width, height, wrap, roads);
            crate::routes::path_friction_multiplier(roads, &path)
        })
        .fold(crate::intensification::FRICTION_UNCHANGED, f32::min)
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

/// The bands [`balance_supply_networks`] pools between — named because the tuple grew a fourth
/// member ([`BandId`], the endpoint identity the link rule reads) and an inline four-tuple query is
/// what `clippy::type_complexity` exists to stop, exactly as `VisionCohorts` is named next door.
type SupplyBands<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut PopulationCohort,
        Option<&'static BandId>,
        Option<&'static mut LaborAllocation>,
    ),
    With<ResidentBand>,
>;

// Every parameter is a distinct world resource or query this pass genuinely reads — the ECS's own
// signature, not a call site anyone types. It crossed the threshold when the route branch added the
// road ledger and the traffic log; the repo's convention for that is this allow, as on
// `terrain::def` and `handle_send_trade_expedition`.
#[allow(clippy::too_many_arguments)]
pub fn balance_supply_networks(
    config: Res<SupplyNetworkConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // **The edge, read one stage early.** `advance_connections` runs later the same turn, in
    // `TurnStage::Visibility`, so this pass sees the ledger as of the *previous* turn's contacts —
    // and on the world's very first turn the ledger is empty, so nothing pools on turn 1. Both are
    // accepted and neither is worth reordering a stage for: bands open with
    // `startup.food_reserve_days` of their own food, and two bands standing within `reach_tiles`
    // see each other every turn, which pins their tie at `FULL_TIE` from turn 2 onward. The
    // alternative — supply seeding the ledger itself — would make a second producer of contact
    // that no sight sweep agrees with.
    ledger: Res<ConnectionLedger>,
    // **The roads, read one stage early — the same lag, for the same reason.** `advance_roads` runs
    // later in this stage, so the payoff below is read at each road tile's standing as of the
    // *previous* turn, exactly as the connection ledger above is. Both are accepted rather than
    // reordered: a supply pass that raised a road itself would be a second producer of a position.
    roads: Res<crate::routes::RoadRegistry>,
    // **The ladder, for the ROUTE PAYOFF's own numbers** — the widest reach any rung holds open
    // (`routes::max_route_reach_tiles`, which the binning below sizes its cells by) and the rate a
    // recorded link banks. Both are read through their seams rather than off a rung record.
    ladder: Res<crate::intensification::LadderConfigHandle>,
    mut route_traffic: ResMut<crate::routes::RouteTrafficLog>,
    // `With<ResidentBand>`: an expedition manages its own larder — its drop-off is the explicit
    // fold-back on arrival, not a passive supply-network leak — so it is excluded here.
    // **The food ledger's transfer terms ride here**, because this system is one of their writers:
    // a balancing move crosses two larders through neither income nor consumption
    // ([`LaborAllocation::last_food_transfers`]), and hay does the same into the fodder account
    // beside it. `Option`, matching how the sibling ledger terms are read at capture — a band without
    // an allocation reports zero for every one of them.
    mut cohorts: SupplyBands,
    mut membership: ResMut<SupplyNetworkMembership>,
) {
    // Recomputed from scratch every turn; a 0/1-band map (early return below) leaves it empty.
    membership.0.clear();
    let cfg = config.get();
    let ladder = ladder.get();
    let width = tile_registry.width;
    let height = tile_registry.height;
    let wrap = sim_config.map_topology.wrap_horizontal;
    let throughput = scalar_from_f32(cfg.throughput_per_turn);
    let friction = scalar_from_f32(cfg.friction).clamp(scalar_zero(), scalar_one());
    let min_transfer = scalar_from_f32(cfg.min_transfer);

    // Pass 1: snapshot each band's position, population weight, and opening stores.
    let mut nodes: Vec<Node> = Vec::new();
    for (entity, cohort, band, _) in cohorts.iter() {
        let Ok(tile) = tiles.get(cohort.current_tile) else {
            continue;
        };
        // No id, no identity to tie — and therefore no edge this band could ever be an endpoint of.
        let Some(&band) = band else {
            continue;
        };
        nodes.push(Node {
            entity,
            band,
            faction: cohort.faction,
            pos: tile.position,
            weight: cohort.total(),
            stores: cohort
                .stores
                .iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            materials: cohort
                .stores
                .materials()
                .flat_map(|(material, batches)| {
                    batches.iter().map(move |(band, batch)| {
                        ((material.to_string(), band.clone()), batch.clone())
                    })
                })
                .collect(),
        });
    }
    if nodes.len() < 2 {
        return;
    }
    // Deterministic node order for the union-find and all downstream iteration.
    nodes.sort_by_key(|node| node.entity.to_bits());

    // Union nodes that are linked ([`tie_is_live`]) *and* pool with each other ([`pools_freely`])
    // into supply networks. Rather than an O(n²) all-pairs scan, bin nodes into a spatial hash of
    // `cell_size`-tile cells (cell_size = reach, so any two nodes within reach fall in the same or
    // an adjacent cell), then compare each node only against candidates in its neighbouring cells.
    // **The bin key is position alone**: the bins are geometry, and both the tie and the pooling
    // policy are pair predicates, so a foreign neighbour only ever widens the candidate net — which
    // is negligible at band counts.
    //
    // ⛔ **THE NEIGHBOURHOOD MUST BE A SUPERSET OF WHAT THE DISTANCE TEST ACCEPTS**, or pairs are
    // dropped silently. It is, and for the same reason `hex_range_tiles` may scan a bounding box:
    // every hex step changes the offset column and row by at most 1 (`HEX_NEIGHBOR_OFFSETS`), so a
    // node `reach` hex steps away is within `reach` columns and `reach` rows — the *same* offset
    // box the retired squared-Euclidean test implied (`dx² + dy² <= reach²` also gives
    // `|dx|, |dy| <= reach`). A delta of at most `cell_size` moves a floor-division cell index by at
    // most one, so ±1 cells cover it, and the ±2 in x below still covers the runt seam cell.
    // Changing the metric therefore did not change the required neighbourhood at all.
    //
    // ⛔ **AND THAT IS WHY THE CELL IS SIZED BY THE ROUTE PAYOFF TOO.** The distance test below
    // accepts a pair out to `max(reach_tiles, path_reach_tiles(..))`, so a road holding a link open
    // at 16 tiles against a cell size of 3 would drop long routed pairs **silently** — it fails as
    // *some long roads just don't work*, with nothing erroring anywhere.
    // `routes::max_route_reach_tiles` is the seam that answers it, so retuning a rung's reach moves
    // the neighbourhood with it and no call site moves.
    let count = nodes.len();
    let mut parent: Vec<usize> = (0..count).collect();

    let widest_route_reach = crate::routes::max_route_reach_tiles(&ladder);
    let cell_size = cfg.reach_tiles.max(widest_route_reach).max(1);
    let num_cells_x = width.div_ceil(cell_size).max(1) as i32;
    let cell_of =
        |pos: UVec2| -> (i32, i32) { ((pos.x / cell_size) as i32, (pos.y / cell_size) as i32) };
    let mut bins: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        let (cx, cy) = cell_of(node.pos);
        bins.entry((cx, cy)).or_default().push(idx);
    }
    // With horizontal wrap, a runt seam cell (when width isn't a multiple of cell_size) can leave
    // two within-reach nodes two cells apart across the seam, so search ±2 in x (folded into range)
    // when wrapping, ±1 otherwise. y never wraps.
    let x_offsets: &[i32] = if wrap {
        &[-2, -1, 0, 1, 2]
    } else {
        &[-1, 0, 1]
    };
    let mut links: Vec<(usize, usize)> = Vec::new();
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
                let Some(candidates) = bins.get(&(ncx, ncy)) else {
                    continue;
                };
                for &j in candidates {
                    if j <= i {
                        continue; // each unordered pair once; also skips self
                    }
                    if pools_freely(&nodes[i], &nodes[j])
                        && link_holds(
                            &roads,
                            nodes[i].pos,
                            nodes[j].pos,
                            cfg.reach_tiles,
                            widest_route_reach,
                            width,
                            height,
                            wrap,
                        )
                        && tie_is_live(&ledger, nodes[i].band, nodes[j].band)
                    {
                        // **THE COMMONEST TRAFFIC IN THE GAME, recorded where it is known.** Two
                        // camps pooling a larder are people walking between them, turn after turn —
                        // #532's *"it must not be the one case that produces no trail because nobody
                        // typed a command"*. The road is worn by `routes::advance_routes` later in
                        // this stage rather than here, so this turn's pooling cannot read a road
                        // this turn's pooling created.
                        route_traffic.walked(nodes[i].pos, nodes[j].pos, &ladder);
                        // **THE LINKS THEMSELVES, kept for the friction reading below.** A road's
                        // payoff is derived from *the tiles a journey crosses*, so a component's
                        // friction is read off its own pooling links rather than off whatever roads
                        // its members happen to be camped on.
                        links.push((i, j));
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
    /// `(band, rating, signed amount, the reading a RECEIVED amount arrives at)`.
    type MaterialTransfer = (Entity, MaterialKey, Scalar, BTreeMap<String, f32>);
    let mut applied_materials: Vec<MaterialTransfer> = Vec::new();
    for members in components.values() {
        if members.len() < MIN_NETWORK_MEMBERS {
            continue;
        }
        let weights: Vec<Scalar> = members.iter().map(|&m| nodes[m].weight).collect();

        // ⛔ **THE FIRST THING A ROUTE RUNG HAS EVER BOUGHT, and it is DERIVED FROM THE TILES.**
        // See [`component_friction`]: each pooling link averages the roads on the tiles between its
        // two camps, so a **partly** roaded run pays partly.
        let friction = friction
            * scalar_from_f32(component_friction(
                &roads, &nodes, &links, members, width, height, wrap,
            ));

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

        // **Materials pool per RATING, never as a scalar.** One `balance_commodity` run per
        // `(material, band key)` — see [`MaterialKey`] — so the balancer never sees two different
        // ratings of one material as the same thing. What moves keeps its exact characteristics: a
        // sender's remaining half is untouched (an average does not move when a uniform part of it
        // is removed) and the shipped half arrives carrying the senders' own amount-weighted
        // reading, which the receiver then merges by the store's ordinary rule.
        let mut ratings: BTreeSet<&MaterialKey> = BTreeSet::new();
        for &m in members {
            ratings.extend(nodes[m].materials.keys());
        }
        for rating in ratings {
            let stores: Vec<Scalar> = members
                .iter()
                .map(|&m| nodes[m].material_amount(rating))
                .collect();
            let deltas = balance_commodity(&weights, &stores, throughput, friction, min_transfer);
            // The reading everything shipped this turn carries — the amount-weighted average of the
            // **senders'**, which is one rating's worth of readings and therefore cannot smear a
            // mammoth hide into a hare pelt. Resolved before any delta is applied, off the same
            // opening snapshot every other flow reads.
            let mut shipped_total = scalar_zero();
            let mut shipped_reading: BTreeMap<String, f32> = BTreeMap::new();
            for (k, &m) in members.iter().enumerate() {
                if deltas[k] >= scalar_zero() {
                    continue;
                }
                let sent = -deltas[k];
                let Some(batch) = nodes[m].materials.get(rating) else {
                    continue;
                };
                shipped_total += sent;
                for (axis, reading) in &batch.characteristics {
                    *shipped_reading.entry(axis.clone()).or_insert(0.0) += reading * sent.to_f32();
                }
            }
            if shipped_total > scalar_zero() {
                for value in shipped_reading.values_mut() {
                    *value /= shipped_total.to_f32();
                }
            }
            for (k, &m) in members.iter().enumerate() {
                if deltas[k] == scalar_zero() {
                    continue;
                }
                applied_materials.push((
                    nodes[m].entity,
                    rating.clone(),
                    deltas[k],
                    shipped_reading.clone(),
                ));
            }
        }
    }

    for (entity, commodity, delta) in applied {
        if let Ok((_, mut cohort, _, allocation)) = cohorts.get_mut(entity) {
            cohort.stores.add(&commodity, delta);
            // **TWO KEYS ARE COUNTED, AND THEY EACH HAVE THEIR OWN ACCOUNT.** `FOOD` closes the
            // larder identity; `FODDER` closes nothing but is what the hay rows and the fodder
            // runway read (`snapshot::population`), and a pooled store that nothing counted is
            // exactly why a receiving band's runway used to say it was draining while its hay rose.
            // Materials still have no account here: theirs is the batch store itself, and a scalar
            // total of hide and bone is the retired trade axis under a new name.
            let ledger = if commodity == FOOD {
                allocation
                    .map(|allocation| allocation.map_unchanged(|a| &mut a.last_food_transfers))
            } else if commodity == FODDER {
                allocation
                    .map(|allocation| allocation.map_unchanged(|a| &mut a.last_fodder_transfers))
            } else {
                None
            };
            if let Some(mut ledger) = ledger {
                // **Added, never assigned** — a band can balance against several neighbours in one
                // pass, and the ledger also carries what a command drew earlier in the same snapshot
                // window.
                //
                // **The link is [`TransferLink::Local`]** — pooling is what bands standing near
                // each other do without anybody carrying anything. This pass is the bulk of that
                // arm; a fission's dowry is the other writer on it, for the same reason.
                if delta > scalar_zero() {
                    ledger.credit(TransferLink::Local, delta.to_f32());
                } else {
                    ledger.debit(TransferLink::Local, (-delta).to_f32());
                }
            }
        }
    }

    for (entity, (material, band), delta, reading) in applied_materials {
        let Ok((_, mut cohort, _, _)) = cohorts.get_mut(entity) else {
            continue;
        };
        if delta < scalar_zero() {
            // A send comes out of exactly the batch it was priced against — never re-sorted, since
            // the rating is already named.
            cohort.stores.take_material_batch(&material, &band, -delta);
        } else {
            cohort
                .stores
                .deposit_material(&material, band, delta, &reading);
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
