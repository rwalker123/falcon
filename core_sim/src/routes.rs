//! **Roads** — the intensification ladder's third branch, and the first reader
//! `TerrainDefinition::infrastructure_cost` has ever had (`docs/plan_standing_upkeep.md` §4.13,
//! issue #532).
//!
//! Routes climb **game trail → trail → dirt road → paved road**. Each rung is *cheaper to travel and
//! dearer to keep*, which is deliberately **not** a straight upgrade path: you pave where the traffic
//! pays for the upkeep, and everywhere else a trail is the right answer for ever.
//!
//! # ⛔ A ROAD IS IN THE GROUND. IT DOES NOT FOLLOW THE CAMP.
//!
//! The rejected model defined a route as *"the road between band A and band B"*, re-derived each turn
//! from wherever those bands stand. It is simpler and it is wrong, and the fault is **not** the
//! unphysicality — Ray: *"How would a road follow a camp? That makes no sense and is in fact a factor
//! in moving vs staying. Roads can't follow camps."*
//!
//! **A road that follows its camp deletes a decision.** A road you paid to build and pay to hold is
//! one of the strongest reasons the game can give you to *stay*; one that packs up and comes with you
//! costs nothing to leave, so it can never weigh on move/stay/fork — the pillar the whole project is
//! built around.
//!
//! **So #532's and §Q4's own framing — *"a route is an edge, and belongs to a connection"* — is
//! wrong, and both were corrected.** A [`Route`] is a world object with a fixed tile path and its own
//! [`RouteId`]. The band pair is who *uses* it, never what it *is*. There is deliberately **no**
//! `RouteKey { low, high }` and no band-pair keying anywhere in this module.
//!
//! # The four rules, and each one replaces a rule the band-pair shape needed
//!
//! | | Rule | What it dissolves |
//! |---|---|---|
//! | **1** | A road's tiles are **stamped once**, from the path the first traffic walked, and never re-derived ([`Route::path`]). | The re-stamp rule. |
//! | **2** | A band is served by a road while **standing on one of its tiles** ([`RouteLedger::routes_on_tile`]). The road's own path is the catchment, so there is **no radius and no "close enough" constant**. | The tolerance constant. |
//! | **3** | A road nobody stands on earns no traffic, is claimed by no band, and **reverts** on the meter decay and grace every rung already declares. | The orphan case, and *"who pays for a road to nowhere"* — nobody does, and nobody needs to. |
//! | **4** | New traffic **prefers an existing road** whose path already joins where it is going. | The near-duplicate-road swarm. It is also why real networks consolidate: roads attract the traffic that widens them. |
//!
//! **A road is therefore a SHARED PUBLIC GOOD**, which the band-pair shape could not express: camp A
//! leaves, camp C settles on the same ground, and **C inherits the road** — because the road never
//! belonged to A. Every band standing on one claims its keeping from that band's own `Roadwork` pool,
//! and §2.5's existing *"several bands can pay one source in one turn"* accumulation is already the
//! mechanism for that, so there is no new funding rule.
//!
//! **The accepted cost, named rather than softened: a band that steps one tile off its own road loses
//! it.** No radius is added to cushion that — a radius is precisely the constant rule 2 exists to
//! avoid, and *stay on your road* is the legible half of the same pillar.
//!
//! # What a rung buys — and why it had to buy something
//!
//! `infrastructure_cost` was authored for all 37 terrains and had **zero readers**; nothing anywhere
//! read a route rung to reduce a cost. **Shipping the dearer half alone builds a tax, not a ladder.**
//! The consumer was already chosen and never wired — `SupplyNetworkConfig::reach_tiles`' own shipped
//! doc comment says *"beyond it a link needs a route to hold it open"*.
//!
//! So a rung buys three things, all read on the very edge the road sits on:
//!
//! - **[`RouteRungPayoff::holds_link_to_tiles`]** — a pooling link forms within `reach_tiles` **or**
//!   where a road of this rung spans it. A *capability*, not a discount.
//! - **[`RouteRungPayoff::friction_multiplier`]** — a routed link loses less of what it sends. Needed
//!   because reach alone pays nothing to a road between two neighbours already inside `reach_tiles`,
//!   which is the commonest road in the game.
//! - **`Seen` along a kept road** ([`Route::grants_sight`]) — see the callout on that method.
//!
//! All three are **purely additive**, so §Q4's *"no early-game regression, by construction"* holds:
//! an unrouted pair inside `reach_tiles` pools exactly as it does today, at exactly today's friction,
//! and sees exactly what it sees today.
//!
//! # The build is paid by TRAFFIC, so a route takes no builder and no queue entry
//!
//! *"A crew clears ground | **traffic wears the route in**."* Traffic **is** the crew, so a route rung
//! declares no `verb`, appends no `BuildQueueEntry`, and draws nothing from the builders' pool
//! ([`RungBranch::is_crew_built`] is the one predicate that says so). That is what lets a road be
//! owned by nobody: the queue is the most band-shaped thing in the engine.
//!
//! **Traffic converts to WORK UNITS**, the same currency `RungBuild::work_cost` is quoted in, so
//! *"what does it cost to raise this"* has one answer in one unit whichever branch is asked.

use std::collections::BTreeMap;

use bevy::prelude::*;
use sim_runtime::TerrainType;

use crate::{
    components::Tile,
    grid_utils::{hex_distance_wrapped, hex_neighbor, HEX_DIRECTION_COUNT},
    intensification::{
        build_fraction, interpolate, neglect_grace_remaining, rung_work_done, upkeep_shortfall,
        upkeep_shortfall_fraction, LadderConfig, RungBranch, RungKey, RungRoutePayoff,
        RungStanding, FRICTION_UNCHANGED, FULLY_SUPPLIED, NEGLECT_NONE, NO_CREW_ON_THIS_ACTIVITY,
        NO_RUNG_WORK_BANKED, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT,
        RUNG_COST_UNSCALED, RUNG_UNSTARTED,
    },
    resources::TileRegistry,
    terrain::terrain_definition,
};

/// **Trailcraft** — taught by traffic on a game trail, gates `route:trail`. *You learn to wear a path
/// in by walking one.*
pub const TRAILCRAFT_DISCOVERY_ID: u32 = 2011;
/// **Roadbuilding** — taught by keeping a trail, gates `route:dirt_road`.
pub const ROADBUILDING_DISCOVERY_ID: u32 = 2012;
/// **Paving** — taught by keeping a dirt road, gates `route:paved_road`.
pub const PAVING_DISCOVERY_ID: u32 = 2013;

/// **A route's identity.** Deliberately not a band pair: a road outlives the bands that wore it in,
/// and is inherited by whoever camps on it next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteId(pub u64);

/// **No traffic this turn** — the neutral [`Route::traffic_work`] accumulates from.
pub const NO_TRAFFIC: f32 = 0.0;

/// **A road: a fixed path of tiles, and where it stands on the route branch.**
///
/// One position in cumulative work units, exactly as a patch and a herd carry one
/// (`docs/plan_standing_upkeep.md` §2.8) — the ladder position **is** the accumulator, so a
/// fractional turn's traffic banks as a fraction of a work unit and crosses a rung boundary when the
/// sum crosses it. **No separate traffic accumulator may be added**; that would be a second producer
/// of the same number.
#[derive(Debug, Clone)]
pub struct Route {
    /// **THE TILES THIS ROAD RUNS OVER, stamped once and never re-derived** (rule 1). Ordered from
    /// the end it was first walked from, and never empty for a route in the ledger.
    pub path: Vec<UVec2>,
    /// How far up the route branch this road has been worn, in cumulative work units.
    position: f32,
    /// The derived standing, re-stamped on every write to [`Self::position`] so the two cannot drift
    /// — the `ForagePatch::standing` / `Herd::standing` convention.
    standing: RungStanding,
    /// **WHAT THIS ROAD BUYS, stamped beside the standing it is derived from.**
    ///
    /// Derived and re-stamped on every write to [`Self::position`], exactly as [`Self::standing`] is
    /// — so a reader asking *what is this road worth* needs the road and nothing else.
    ///
    /// **That is what keeps the ladder out of `balance_supply_networks`.** The payoff is a pure
    /// function of the held rung, so a supply pass that resolved it would be taking a config handle
    /// to re-derive a number the road already knows — and every harness that stands the pooling up
    /// would have to hand it one. A stamped reading has one producer, which is the rule the standing
    /// beside it is stamped under.
    payoff: RungRoutePayoff,
    /// **What this road's keeping was billed at, stamped once per turn, first-write-wins.** The wire
    /// states `demand − supplied == shortfall` verbatim, and an interpolated demand moves *within* a
    /// turn, so every reader must take the stamp rather than re-reading the live cost (§2.5).
    ///
    /// **A road's demand moves more than a source's**, because bands walk on and off it, so this is
    /// the branch most exposed to that defect.
    pub upkeep_demanded: Option<f32>,
    /// What the bands standing on this road actually supplied this turn. **Accumulates** across them
    /// (`+=`), cleared once per turn by the decay pass — the §2.5 rule, unchanged.
    pub upkeep_supplied: f32,
    /// Consecutive turns of shortfall. Any turn the bill is met wipes it; it is not a lifetime budget.
    ///
    /// **`u16`, and the exact twin of `ForagePatch::neglect_turns` / `Herd::neglect_turns`** — it is
    /// handed straight to [`crate::intensification::RungDef::upkeep_decay`], which owns both the rate
    /// and the *strictly greater than the grace* comparison, so all three webs count in one unit.
    pub neglect_turns: u16,
    /// **Work units earned from traffic this turn**, banked into [`Self::position`] by the accrual
    /// pass and then cleared. Not persisted state in its own right — a within-turn accumulator across
    /// the several links that may cross one road.
    pub traffic_work: f32,
}

impl Route {
    /// A brand-new road at the branch's floor: a game trail, with work banked on nothing yet.
    pub fn worn_in(path: Vec<UVec2>, ladder: &LadderConfig) -> Self {
        debug_assert!(!path.is_empty(), "a route with no tiles is not a road");
        Self {
            path,
            position: RUNG_UNSTARTED,
            standing: route_standing_at(ladder, RUNG_UNSTARTED),
            payoff: route_payoff_at(ladder, RUNG_UNSTARTED),
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            neglect_turns: NEGLECT_NONE,
            traffic_work: NO_TRAFFIC,
        }
    }

    /// Where this road stands on its branch. Read-only — [`Self::set_position`] is the one mutator.
    pub fn standing(&self) -> RungStanding {
        self.standing
    }

    /// The raw position, in cumulative work units.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// **The one mutator**, writing the position and its derived standing together so a caller cannot
    /// leave the pair disagreeing — `ForagePatch::set_ladder_position`'s rule.
    pub fn set_position(&mut self, position: f32, ladder: &LadderConfig) {
        self.position = position.max(RUNG_UNSTARTED);
        self.standing = route_standing_at(ladder, self.position);
        self.payoff = route_payoff_at(ladder, self.position);
    }

    /// **What this road buys**, at the rung it holds — the stamped reading, never a re-derivation.
    pub fn payoff(&self) -> RungRoutePayoff {
        self.payoff
    }

    /// The rung this road **holds** — what it is entitled to in full.
    pub fn held_rung(&self) -> RungKey {
        self.standing.held
    }

    /// **Is this road one somebody keeps?** `false` at the game-trail floor, which is the whole of
    /// what makes that rung free: nobody maintains a game trail.
    pub fn is_built(&self) -> bool {
        self.held_rung() != RungKey::RouteGameTrail
    }

    /// ⛔ **DOES THIS ROAD LIGHT ITS OWN TILES?**
    ///
    /// **Yes, while it stands at a built rung and its keeping is met.** Ray: *"If a road exists and is
    /// maintained, the assumption is that there is traffic on it and it is seen."*
    ///
    /// The design pass first answered *no*, objecting that the commonest routed link is a pooling link
    /// where nobody physically walks. **That inference ran backwards.** Maintenance is not free — a
    /// kept road bills a band every turn out of its `Roadwork` pool, and what those hands are doing is
    /// being on the road. **Paying the upkeep IS the presence.** A road nobody walks is a road nobody
    /// pays for, and rule 3 has it reverting.
    ///
    /// ⛔ **AND THIS IS WHY THE CONNECTION KEYSTONE DOES NOT BEND.** `connections.rs` states it as
    /// inviolable — *"Only presence makes a tile `Seen`. A connection can only ever grant
    /// `Discovered`."* — and names **logistics** as the first rider that will be tempted to break it.
    /// **This is not that temptation.** The sight is granted by the *road*, which is maintained
    /// presence on specific ground, and **never by the connection**, which still grants `Discovered`
    /// and nothing else. A band with a live tie to a people it has never travelled to sees exactly
    /// what it sees today, and `core_sim/tests/connections.rs` passes unchanged.
    ///
    /// **The grant must therefore be written as its own visibility source beside a band's own
    /// presence — never routed through the connection grant.** Plumbing it through the connection
    /// would satisfy the keystone test by accident rather than by the rule.
    ///
    /// **The condition is the PAID BILL, not the held rung**, so a road in shortfall **goes dark
    /// before it decays** — the honest early warning that the road is being lost.
    pub fn grants_sight(&self) -> bool {
        self.is_built() && self.keeping_is_met()
    }

    /// Was this turn's keeping bill met? A road with no stamped bill has not been judged this turn and
    /// is treated as met — the same reading `Some(0.0)` gets, which is an honest *"owes nothing"*.
    pub fn keeping_is_met(&self) -> bool {
        match self.upkeep_demanded {
            Some(demand) => self.upkeep_supplied + KEEPING_EPSILON >= demand,
            None => true,
        }
    }

    /// **THE BILL THIS ROAD WAS HANDED THIS TURN** — the stamped demand, and [`NO_UPKEEP_DEMAND`]
    /// where nobody stamped one (§2.5). The plant web's `patch_keeping_basis` one branch over: every
    /// reader of the keeping takes the stamp, because an interpolated demand moves *within* a turn
    /// and the wire states `demand − supplied == shortfall` verbatim.
    ///
    /// **A road with no stamp owes nothing rather than owing its live cost.** That is honest here
    /// and nowhere else on the ladder: [`crate::systems::settle_route_keeping`] stamps **every**
    /// road in the ledger, not only the ones a band stands on, so an absent stamp means the keeping
    /// pass has not run at all — a harness driving [`advance_routes`] alone, which must not decay
    /// roads against a bill nobody was ever handed.
    pub fn upkeep_basis(&self) -> f32 {
        self.upkeep_demanded.unwrap_or(NO_UPKEEP_DEMAND)
    }

    /// What this turn's bill left unpaid, off the **stamped** basis (§2.5), never the live demand.
    pub fn upkeep_shortfall(&self) -> f32 {
        upkeep_shortfall(self.upkeep_basis(), self.upkeep_supplied)
    }
}

/// Slack on the keeping comparison, so a road funded to the last representable fraction of its bill is
/// not judged short by a rounding. The `f32` reason `RungStanding` asks `held` rather than comparing a
/// subtraction to a width.
const KEEPING_EPSILON: f32 = 1.0e-4;

/// **Every road in the world, by id, plus the tile index rule 2 is answered from.**
///
/// **`BTreeMap`, not `HashMap`** — the iteration order is observed by the snapshot and by the
/// checkpoint, so it has to be an order and not an accident. `ConnectionLedger`'s rule, and for the
/// same reason.
#[derive(Resource, Default, Debug, Clone)]
pub struct RouteLedger {
    routes: BTreeMap<RouteId, Route>,
    /// **Which roads cross this tile** — the index behind [`Self::routes_on_tile`], rebuilt with every
    /// insert and removal so it cannot drift from the paths it indexes. Keyed `(y, x)` so iteration is
    /// row-major like every other tile sweep.
    by_tile: BTreeMap<(u32, u32), Vec<RouteId>>,
    /// The id the next road takes. Monotonic, never reused, so a checkpointed id cannot collide with a
    /// road laid after the restore.
    next_id: u64,
}

impl RouteLedger {
    /// Lay a new road along `path` and return its id.
    pub fn insert(&mut self, path: Vec<UVec2>, ladder: &LadderConfig) -> RouteId {
        let id = RouteId(self.next_id);
        self.next_id += 1;
        for tile in &path {
            self.by_tile.entry((tile.y, tile.x)).or_default().push(id);
        }
        self.routes.insert(id, Route::worn_in(path, ladder));
        id
    }

    /// **Forget a road entirely** — what rule 3 does once a reverted road has nothing left on it.
    pub fn remove(&mut self, id: RouteId) -> Option<Route> {
        let route = self.routes.remove(&id)?;
        for tile in &route.path {
            if let Some(ids) = self.by_tile.get_mut(&(tile.y, tile.x)) {
                ids.retain(|held| *held != id);
                if ids.is_empty() {
                    self.by_tile.remove(&(tile.y, tile.x));
                }
            }
        }
        Some(route)
    }

    pub fn get(&self, id: RouteId) -> Option<&Route> {
        self.routes.get(&id)
    }

    pub fn get_mut(&mut self, id: RouteId) -> Option<&mut Route> {
        self.routes.get_mut(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (RouteId, &Route)> {
        self.routes.iter().map(|(id, route)| (*id, route))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (RouteId, &mut Route)> {
        self.routes.iter_mut().map(|(id, route)| (*id, route))
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// **RULE 2 — which roads is a band standing on?** The road's own path is the catchment, so this
    /// is an index lookup and not a radius test.
    pub fn routes_on_tile(&self, tile: UVec2) -> &[RouteId] {
        self.by_tile
            .get(&(tile.y, tile.x))
            .map_or(&[], |ids| ids.as_slice())
    }

    /// **RULE 4 — the road new traffic should wear, if one already joins these two tiles.** Traffic
    /// prefers an existing road, which is what keeps a valley carrying one trail rather than nine
    /// near-duplicates, and is why real road networks consolidate.
    ///
    /// **Both ends must be ON the road** (rule 2), so this is the same catchment the payoff is read
    /// through — a road that merely passes nearby is not a road you are walking.
    pub fn road_joining(&self, from: UVec2, to: UVec2) -> Option<RouteId> {
        self.routes_on_tile(from)
            .iter()
            .copied()
            .find(|id| self.routes_on_tile(to).contains(id))
    }
}

/// **THIS TURN'S TRAFFIC, recorded where it happens and spent where roads are worn.**
///
/// `balance_supply_networks` knows which pairs pooled; it must not also be the thing that lays roads,
/// because it runs **before** the accrual and laying a road mid-pass would let this turn's pooling
/// read a road this turn's pooling created. So it writes the pairs here and
/// [`advance_routes`] spends them — the same producer/consumer split `upkeep_supplied` uses across
/// the Population→Logistics carry.
///
/// **Cleared by the accrual, every turn**, so a turn with no pooling wears nothing rather than
/// re-wearing last turn's links.
#[derive(Resource, Default, Debug, Clone)]
pub struct RouteTrafficLog {
    /// The tile pairs that carried traffic this turn. Unordered within a pair — a road has no
    /// direction — and duplicates are meaningful: two links over one road are twice the traffic.
    pub links: Vec<(UVec2, UVec2)>,
}

impl RouteTrafficLog {
    /// Record one turn of traffic between two camps.
    pub fn walked(&mut self, from: UVec2, to: UVec2) {
        if from != to {
            self.links.push((from, to));
        }
    }
}

/// **WHAT A COMPONENT'S POOLING LOSES IN TRANSIT, as a multiple of the base friction** — the payoff
/// `balance_supply_networks` reads, and the first thing a route rung has ever bought.
///
/// **It is the BEST road binding the network, not the worst, and that is forced rather than
/// generous.** The whole branch's safety argument is that a rung is **purely additive** — *"a rung
/// can only widen the set of links and lower a loss, never the reverse"*, which is what preserves
/// §Q4's *"no early-game regression, by construction"*. Under a worst-road reading, wearing a **new**
/// poor trail into an existing network would **raise** that network's friction: a road would make
/// things worse, and a band would be punished for having walked somewhere. Best-road cannot do that.
///
/// The balancer pools a whole component against **one** friction scalar — it has no path model — so
/// either reading is an approximation of the same thing. This is the approximation that cannot
/// regress.
///
/// **A road counts only if at least TWO members of the component stand on it** (rule 2): a road one
/// band happens to be camped on carries none of that component's pooling, and crediting it would pay
/// a network for a road nobody is using to reach anyone.
///
/// **And only if it is BUILT and KEPT** — the same condition [`Route::grants_sight`] reads, for the
/// same reason: an unmaintained road is not carrying anything.
pub fn component_friction_multiplier<'a>(
    ledger: &RouteLedger,
    member_tiles: impl IntoIterator<Item = &'a UVec2>,
) -> f32 {
    let mut standing_on: BTreeMap<RouteId, u32> = BTreeMap::new();
    for tile in member_tiles {
        for id in ledger.routes_on_tile(*tile) {
            *standing_on.entry(*id).or_default() += 1;
        }
    }
    standing_on
        .into_iter()
        .filter(|(_, members)| *members >= MEMBERS_TO_CARRY_A_LINK)
        .filter_map(|(id, _)| ledger.get(id))
        .filter(|route| route.grants_sight())
        .map(|route| route.payoff().friction_multiplier)
        .fold(FRICTION_UNCHANGED, f32::min)
}

/// **A road carries a component's pooling only once two of its bands stand on it** — one camp on a
/// road is a camp beside a road, not a link over one.
pub const MEMBERS_TO_CARRY_A_LINK: u32 = 2;

/// **THE SCALE TERM — `length × terrain`, as one sum** (`UpkeepScale::RouteSpan`).
///
/// ```text
/// span = Σ over the road's tiles of infrastructure_cost(that tile's terrain)
/// ```
///
/// **Summed per tile crossed, never averaged**: three tiles of marsh cost three tiles of marsh, and
/// averaging would price a long road and a short one through the same country identically.
///
/// **This is the one place `infrastructure_cost` is read**, so the bill, the quote, the decay and the
/// wire cannot answer three different geometries — the `forage::patch_land_capacity` rule.
///
/// A tile absent from the registry contributes nothing rather than defaulting to some terrain nobody
/// chose; an off-map path is a harness artifact, and inventing a cost for it would make a synthetic
/// road dearer than a real one.
pub fn route_span(route: &Route, registry: &TileRegistry, tiles: &Query<&Tile>) -> f32 {
    span_of_terrains(
        route
            .path
            .iter()
            .filter_map(|pos| registry.index(pos.x, pos.y))
            .filter_map(|entity| tiles.get(entity).ok())
            .map(|tile| tile.terrain),
    )
}

/// **The sum itself**, for a caller that already holds the terrains. [`route_span`] is the ECS
/// wrapper over it and adds nothing but the lookup, so there is exactly one definition of the
/// arithmetic and a test can reach it without standing up a world.
pub fn span_of_terrains(terrains: impl IntoIterator<Item = TerrainType>) -> f32 {
    terrains
        .into_iter()
        .map(|terrain| terrain_definition(terrain).infrastructure_cost)
        .sum()
}

/// **WHAT THIS ROAD OWES EVERY TURN TO STAY WHERE IT IS**, in work units — the road's own
/// `upkeep_demand`, *interpolated* on its standing and scaled by its [`route_span`].
///
/// It is `forage::patch_upkeep_demand`'s exact shape, with the span where the tender-loads are: the
/// rung owns the rate, the branch owns the scale measure. **Interpolated rather than read off the
/// held rung**, so a road part-way into a dirt road owes part of a dirt road — the same continuous
/// bill the two food webs charge, and the reason a bill can never step within a turn.
///
/// ⛔ **THE GAME TRAIL FALLS OUT OF THE ARITHMETIC RATHER THAN BEING BRANCHED AROUND.** The floor
/// declares no `upkeep`, so [`crate::intensification::RungDef::upkeep_demand`] answers
/// [`NO_UPKEEP_DEMAND`] for it and a road holding only the trail interpolates to nothing owed. An
/// `is_built()` guard here would be a second statement of *"nobody maintains a game trail"*, free to
/// disagree with the ladder that already says it.
pub fn route_upkeep_demand(route: &Route, span: f32, ladder: &LadderConfig) -> f32 {
    interpolate(&route.standing(), |rung| {
        ladder.rung(rung).upkeep_demand(span)
    })
}

/// **THE BILL A CLAIM IS PRICED AT** — the **stamped** demand where this turn's keeping pass has
/// already struck one, and the live [`route_upkeep_demand`] where it has not.
///
/// `forage::patch_keeping_basis`' rule exactly, and for its reason: a claim and the bill it is
/// judged against must be one number, and an interpolated demand moves *within* a turn.
///
/// **The fallback is the only reading available to the SHED**, which counts a band's spare road
/// keepers against this bill inside `advance_labor_allocation` — a whole system before
/// [`crate::systems::settle_route_keeping`] stamps anything. Nothing moves a road's position between
/// the two, so the two readings are the same number; what the fallback buys is that the count is not
/// struck against a bill of zero and every road keeper shed as spare.
pub fn route_keeping_basis(route: &Route, span: f32, ladder: &LadderConfig) -> f32 {
    route
        .upkeep_demanded
        .unwrap_or_else(|| route_upkeep_demand(route, span, ladder))
}

/// **THE RUNG AT RISK ON THIS ROAD** — the newest rung carrying work, which is the rung a decay eats
/// and the rung whose grace and rot rate govern.
///
/// **One helper because three readers must agree**: the bill above interpolates *through* it, the
/// grace lookup asks it how long neglect is forgiven, and [`advance_routes`] bleeds it. A road that
/// billed one rung and decayed another is exactly the drift `forage::patch_unwinding_key` exists to
/// prevent one branch over.
///
/// It answers a rung rather than an `Option`, unlike the plant web's: a route position always holds
/// **something** ([`RungKey::RouteGameTrail`] is the floor), and that rung declares no upkeep — so
/// *"a road with nothing built on it is at risk of nothing"* is already the arithmetic's own answer.
pub fn route_at_risk_rung(standing: &RungStanding) -> RungKey {
    standing
        .raising
        .filter(|_| standing.banked > NO_RUNG_WORK_BANKED)
        .unwrap_or(standing.held)
}

/// **THE METER ON THE RUNG THIS ROAD IS ACTUALLY RAISING**, `0..=1` — the route branch's twin of
/// `cultivationProgress` / `corralProgress`, and what `RouteState::buildFraction` publishes.
///
/// It is read at [`route_at_risk_rung`], the **same** seam the bill interpolates through, the grace
/// counts down against and [`advance_routes`] bleeds — so a row cannot show one rung's meter beside
/// another rung's countdown. A road that has just *completed* a rung has nothing banked in the next
/// one, so the at-risk rung is the one it holds and this reads **exactly** [`METER_FULL`]; the turn
/// its first traffic lands on the rung above, this becomes that rung's meter from zero.
///
/// ⛔ **IT GOES THROUGH [`rung_work_done`], NEVER THROUGH A SUBTRACTION.** That seam answers a rung
/// the standing already holds with the rung's full `width` by construction rather than with
/// `fl(base + width) − base`, which is the rounding that published a completed Field at *"99%"*
/// (`intensification::rung_work_done`'s own callout).
///
/// A road at the top of the branch is raising nothing and reads [`METER_FULL`] — an honest *"there
/// is no meter in flight here"* rather than a zero a client would draw as an empty bar.
pub fn route_build_fraction(route: &Route, ladder: &LadderConfig) -> f32 {
    let standing = route.standing();
    let at_risk = route_at_risk_rung(&standing);
    let span = route_rung_span(at_risk, ladder);
    build_fraction(
        rung_work_done(standing, at_risk, route.position(), span),
        span.1,
    )
}

/// **A meter with nothing left to raise** — what [`route_build_fraction`] answers at the top of the
/// branch, and the value [`build_fraction`] returns for a rung the standing already holds.
pub const METER_FULL: f32 = 1.0;

/// **HOW MANY WHOLE ROAD KEEPERS THIS ROAD'S BILL WANTS** — `ceil(basis / PER_WORKER_OUTPUT)`, the
/// route twin of `forage::patch_upkeep_workers_needed`, and the readout that makes a standing cost
/// legible: *"this wants 1, you have 0"*.
///
/// Struck against the **same** [`route_keeping_basis`] the published demand, supplied and shortfall
/// are, so the four numbers on a row describe one bill. [`NO_CREW_ON_THIS_ACTIVITY`] for a road that
/// owes nothing — a game trail, which nobody maintains.
pub fn route_upkeep_workers_needed(route: &Route, span: f32, ladder: &LadderConfig) -> u32 {
    let demand = route_keeping_basis(route, span, ladder);
    if demand <= NO_UPKEEP_DEMAND {
        return NO_CREW_ON_THIS_ACTIVITY;
    }
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

/// **HOW MANY MORE TURNS OF SHORTFALL THIS ROAD CAN ABSORB BEFORE IT BLEEDS** — the countdown, not
/// the counter, through [`crate::intensification::neglect_grace_remaining`] so all three webs and
/// the wire mean one thing by a grace.
///
/// Resolved at [`route_at_risk_rung`], the same seam [`advance_routes`] bleeds through, so the wire
/// cannot count down against a rung the sim is not touching.
///
/// **`None` = THERE IS NOTHING AT RISK HERE**, which is a road holding only the game trail with no
/// traffic banked above it: that rung declares no `upkeep`, so there is no grace to count and no
/// meter to lose. It falls out of the ladder rather than out of an `is_built()` guard, exactly as
/// the bill does.
pub fn route_neglect_grace_remaining(route: &Route, ladder: &LadderConfig) -> Option<u32> {
    let rung = ladder.rung(route_at_risk_rung(&route.standing()));
    rung.upkeep.as_ref()?;
    Some(neglect_grace_remaining(
        route.neglect_turns,
        rung.upkeep_grace_turns(),
    ))
}

/// **What a route rung costs to raise.** `None` for the game-trail floor, which is nothing to build.
///
/// **Routes take no per-source price multiplier** — a mile of road is a mile of road — so every rung
/// is quoted at [`RUNG_COST_UNSCALED`]. That is the plant web's own shape before the Sow share price
/// landed on it, and it is why there is no `Route`-side twin of `patch_field_cost_multiplier`: the
/// thing a road's price varies with is its **span**, and the span is already a term of the *upkeep*
/// rather than of the build.
fn route_rung_cost(rung: RungKey, ladder: &LadderConfig) -> Option<f32> {
    ladder.rung(rung).build_cost(RUNG_COST_UNSCALED)
}

/// **What a route standing at `position` buys.** Read off the rung it *holds*, so a half-worn dirt
/// road buys exactly what the trail beneath it buys until the rung fills — the payoff is a property
/// of the road you have, not of the one you are wearing in.
pub fn route_payoff_at(ladder: &LadderConfig, position: f32) -> RungRoutePayoff {
    let held = route_standing_at(ladder, position).held;
    *ladder
        .rung(held)
        .route_payoff
        .as_ref()
        .expect("validate requires a route_payoff on every route rung")
}

/// Resolve a route position through the ladder — the one seam that answers *where does this road
/// stand*, so no call site re-derives a standing from a meter.
pub fn route_standing_at(ladder: &LadderConfig, position: f32) -> RungStanding {
    RungStanding::at(ladder, RungBranch::Route, position, |rung| {
        route_rung_cost(rung, ladder)
    })
}

/// **Where a route rung starts and how wide it is**, in cumulative work units — `route:trail` is
/// `(0, 40)`, `route:dirt_road` `(40, 110)` and `route:paved_road` `(150, 260)` on the shipped ladder.
pub fn route_rung_span(rung: RungKey, ladder: &LadderConfig) -> (f32, f32) {
    crate::intensification::rung_span(rung, &|key| route_rung_cost(key, ladder))
}

/// **The path traffic between two tiles wears** — a hex walk that greedily closes the distance.
///
/// Deterministic (it takes the lowest-numbered direction among equally good steps), wrap-aware
/// through [`hex_neighbor`], and inclusive of both ends: a road runs from the camp to the camp, and
/// the endpoints are on it by construction, which is what makes rule 2 true the moment a road is laid.
pub fn trace_path(from: UVec2, to: UVec2, width: u32, height: u32, wrap: bool) -> Vec<UVec2> {
    let mut path = vec![from];
    let mut cursor = from;
    // The walk closes the distance by at least one step each iteration, so the hex distance bounds it
    // — no unbounded loop is reachable, and the guard is a belt for an unreachable tie.
    let mut budget = hex_distance_wrapped(from, to, width, wrap);
    while cursor != to && budget > 0 {
        let mut best: Option<(u32, UVec2)> = None;
        for dir in 0..HEX_DIRECTION_COUNT {
            let Some((nx, ny)) = hex_neighbor(cursor.x, cursor.y, dir, width, height, wrap) else {
                continue;
            };
            let candidate = UVec2::new(nx, ny);
            let distance = hex_distance_wrapped(candidate, to, width, wrap);
            if best.is_none_or(|(held, _)| distance < held) {
                best = Some((distance, candidate));
            }
        }
        let Some((_, step)) = best else { break };
        cursor = step;
        path.push(cursor);
        budget -= 1;
    }
    path
}

/// **TRAFFIC WEARS THE ROADS IN, AND NEGLECT WEARS THEM OUT** — the route branch's build accrual and
/// its decay pass in one system, the counterpart of `forage::advance_cultivation` and
/// `fauna::advance_husbandry` on the two food webs.
///
/// # THE FOUR PHASES, IN THIS ORDER
///
/// 1. **Judge last turn's keeping.** The shortfall against the **stamped** bill
///    ([`Route::upkeep_basis`]) arms or wipes [`Route::neglect_turns`] — *consecutive* turns short,
///    never a lifetime budget, which is the rule both food webs already follow.
/// 2. **Bleed the rung at risk** ([`route_at_risk_rung`]) at
///    `shortfall_fraction × meter_decay.per_turn`, once the neglect has outlasted that rung's own
///    `grace_turns`. [`crate::intensification::RungDef::upkeep_decay`] owns both the rate and the
///    strictly-greater comparison, so this system never restates either.
/// 3. **Clear the bill** for the coming turn's stamp — the `advance_cultivation` cycle exactly:
///    `upkeep_demanded` back to `None` and `upkeep_supplied` back to [`NO_UPKEEP_DEMAND`], so next
///    turn's shortfall is the whole demand again unless somebody restates it.
/// 4. **Bank this turn's traffic** onto the position.
///
/// Then the ledger is **pruned** of every road back at [`RUNG_UNSTARTED`] — see the callout at the
/// foot of the body for why that must happen *after* the banking.
///
/// # ⛔ THE ONE-TURN CARRY IS THE ARRANGEMENT, NOT A DEFECT TO FIX
///
/// Logistics runs **before** Population, so the [`Route::upkeep_supplied`] phase 1 judges was
/// stamped by *last* turn's [`crate::systems::settle_route_keeping`]. That is the same lag
/// `forage::advance_cultivation` and `fauna::advance_husbandry` already run on, and it is what makes
/// the keeping a carry-across-turns signal rather than a within-turn one. **Do not reorder a stage
/// for it.**
///
/// It runs in `TurnStage::Logistics` **after `balance_supply_networks`**, which is what lets it see
/// this turn's links. The consequence is that the *payoff* is read at the standing as of the
/// **previous** turn — precisely the one-turn lag `balance_supply_networks` already accepts against
/// `ConnectionLedger` (*"on the world's very first turn the ledger is empty, so nothing pools on turn
/// 1"*). **Do not reorder a stage for it**, and do not let the supply pass raise a road itself: that
/// would make a second producer of a rung's position, which is the failure this arc has had three of.
///
/// # Rules 1 and 4, in that order
///
/// For each link, the road that already joins both ends is the one that gets the work
/// ([`RouteLedger::road_joining`] — **rule 4**, and why real networks consolidate). Only when there
/// is none is a fresh path traced and **stamped once** (**rule 1**), which is also what puts both
/// camps on it and makes rule 2 true of a road from the moment it exists.
pub fn advance_routes(
    mut ledger: ResMut<RouteLedger>,
    mut traffic: ResMut<RouteTrafficLog>,
    ladder: Res<crate::intensification::LadderConfigHandle>,
    sim_config: Res<crate::resources::SimulationConfig>,
    tile_registry: Res<TileRegistry>,
) {
    let ladder = ladder.get();
    let rate = ladder.route_traffic.work_per_link_tile_per_turn;
    let (width, height) = (tile_registry.width, tile_registry.height);
    let wrap = sim_config.map_topology.wrap_horizontal;

    // ## Phases 1-3, over every road that already existed when this turn began.
    //
    // A road the drain below is about to lay is deliberately not here: it carries no stamped bill
    // and no neglect, so judging it would compare two zeroes and clearing it would clear nothing.
    for (_, route) in ledger.iter_mut() {
        // **1 — HOW SHORT, as a fraction of what was asked**, off the stamped basis and through the
        // ladder's own seam, so the three branches share one reading of *"how short"* and supply
        // only their own rate. A road nobody billed reads [`FULLY_SUPPLIED`] and is forgiven.
        let shortfall_fraction =
            upkeep_shortfall_fraction(route.upkeep_basis(), route.upkeep_supplied);
        if shortfall_fraction > FULLY_SUPPLIED {
            route.neglect_turns = route.neglect_turns.saturating_add(1);
        } else {
            // Any turn the bill is met wipes the counter outright: the grace is about *consecutive*
            // shortfall rather than a lifetime budget, which is the rule both food webs follow.
            route.neglect_turns = NEGLECT_NONE;
        }
        // **2 — THE BLEED, at the at-risk rung's own rate, past that rung's own grace.** Three
        // dials answering three questions, and `upkeep_decay` owns the `>` that decides whether the
        // penalty is biting — so nothing here restates the grace comparison.
        let at_risk = route_at_risk_rung(&route.standing());
        let decay = ladder
            .rung(at_risk)
            .upkeep_decay(shortfall_fraction, route.neglect_turns);
        if decay > NO_UPKEEP_DECAY {
            // `set_position` floors at `RUNG_UNSTARTED`, so a road cannot bleed past the game trail
            // — and it re-stamps the standing and the payoff, which is what keeps a decaying road's
            // friction reading honest without a second producer.
            let bled = route.position() - decay;
            route.set_position(bled, &ladder);
        }
        // **3 — the bill and this turn's payment, cleared on the one-turn cycle.** Both describe
        // the keepers that held this road, so a road whose keepers have gone must stop reporting
        // what they paid; clearing is also what re-arms phase 1 next turn.
        route.upkeep_demanded = None;
        route.upkeep_supplied = NO_UPKEEP_DEMAND;
    }

    // ## Phase 4 — bank this turn's traffic.
    //
    // Drained rather than read: this turn's traffic is spent once, and a turn with no pooling must
    // wear nothing rather than re-wearing last turn's links.
    for (from, to) in std::mem::take(&mut traffic.links) {
        let id = match ledger.road_joining(from, to) {
            Some(id) => id,
            None => {
                let path = trace_path(from, to, width, height, wrap);
                // A traced path always carries both ends, so this cannot be empty for `from != to` —
                // and `walked` refuses a self-link, so the pair is never degenerate.
                if path.len() < MEMBERS_TO_CARRY_A_LINK as usize {
                    continue;
                }
                ledger.insert(path, &ladder)
            }
        };
        let Some(route) = ledger.get_mut(id) else {
            continue;
        };
        // **Per tile of road**, so a longer link banks proportionally more into the longer road it
        // needs — which keeps a road's pace roughly independent of its length, exactly as the span
        // keeps its cost proportional to it.
        route.traffic_work += rate * route.path.len() as f32;
    }

    // **The position IS the accumulator** (§2.8), so the turn's traffic is banked straight onto it
    // and no second meter exists to disagree with it.
    for (_, route) in ledger.iter_mut() {
        if route.traffic_work <= NO_TRAFFIC {
            continue;
        }
        let banked = route.position() + route.traffic_work;
        route.traffic_work = NO_TRAFFIC;
        route.set_position(banked, &ladder);
    }

    // ## ⛔ THE PRUNE, AND IT MUST COME AFTER THE BANKING
    //
    // **A game trail with no work in it is indistinguishable from no road at all** — it holds the
    // free floor, buys nothing, lights nothing and owes nothing — so a ledger that kept every road
    // it ever laid would grow without bound on reverted trails, and every one of them would be a
    // live answer to `routes_on_tile` that carried no traffic and no payoff.
    //
    // **After the banking, because a road laid THIS turn is at `RUNG_UNSTARTED` until its first
    // traffic lands.** Pruning before phase 4 would delete every road on the turn it formed, which
    // is the whole feature.
    //
    // Remembering that animals once walked there is **issue #215's concern, not this ledger's**: the
    // game trail is a rung, and #215 is about seeding the world with them.
    let reverted: Vec<RouteId> = ledger
        .iter()
        .filter(|(_, route)| route.position() <= RUNG_UNSTARTED)
        .map(|(id, _)| id)
        .collect();
    for id in reverted {
        ledger.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intensification::UpkeepScale;

    /// The four route rungs, bottom to top.
    const ROUTE_RUNGS: [RungKey; 4] = [
        RungKey::RouteGameTrail,
        RungKey::RouteTrail,
        RungKey::RouteDirtRoad,
        RungKey::RoutePavedRoad,
    ];

    /// ⛔ **THE LIVENESS CLAIM EVERY OTHER TEST IN THIS FILE RESTS ON.** Without it, a branch that
    /// silently failed to load would leave every assertion below passing over an empty ladder.
    ///
    /// It pins the coded climb against the shipped records' own `order`, which is the pairing
    /// `the_coded_climb_matches_the_shipped_ladders_own_order` already makes for the two food webs.
    #[test]
    fn the_shipped_ladder_carries_four_route_rungs_and_a_position_climbs_them() {
        let ladder = LadderConfig::builtin();

        for (index, key) in ROUTE_RUNGS.iter().enumerate() {
            let rung = ladder.rung(*key);
            assert_eq!(rung.branch, RungBranch::Route, "{key:?} is a route rung");
            assert_eq!(
                rung.order as usize,
                index + 1,
                "{key:?}'s record sits where the coded climb puts it"
            );
        }

        // The floor is held at zero work, costs nothing, and is nobody's to keep.
        let floor = route_standing_at(&ladder, RUNG_UNSTARTED);
        assert_eq!(floor.held, RungKey::RouteGameTrail);
        assert_eq!(floor.raising, Some(RungKey::RouteTrail));
        assert!(
            ladder.rung(RungKey::RouteGameTrail).build.is_none()
                && ladder.rung(RungKey::RouteGameTrail).upkeep.is_none(),
            "NOBODY MAINTAINS A GAME TRAIL — that is the whole of what makes the floor free"
        );

        // And a position really climbs: banking every rung's work reaches the top of the branch.
        let all_the_work: f32 = ROUTE_RUNGS
            .iter()
            .filter_map(|key| ladder.rung(*key).build.as_ref())
            .map(|build| build.work_cost)
            .sum();
        let top = route_standing_at(&ladder, all_the_work);
        assert_eq!(
            top.held,
            RungKey::RoutePavedRoad,
            "banking every rung's work reaches the top of the branch"
        );
        assert_eq!(top.raising, None, "there is nothing above a paved road");
    }

    /// ⛔ **THE SPAN IS A SUM OVER THE TILES CROSSED — NOT AN AVERAGE, AND NOT A TILE COUNT.**
    ///
    /// Both failure modes are asserted against explicitly, because each one passes the *other's*
    /// test: an average is right about terrain and blind to length, a bare tile count is right about
    /// length and blind to terrain. Only a sum is right about both.
    #[test]
    fn the_span_is_a_sum_over_the_tiles_crossed_and_never_an_average_or_a_tile_count() {
        let valley_span = span_of_terrains([TerrainType::AlluvialPlain; 4]);
        let range_span = span_of_terrains([TerrainType::AlpineMountain; 4]);
        let short_range_span = span_of_terrains([TerrainType::AlpineMountain; 2]);

        // The arithmetic itself, stated rather than merely compared.
        assert!(
            (valley_span - 4.0 * 0.9).abs() < 1.0e-5,
            "four tiles of alluvial plain sum to 4 × 0.9, got {valley_span}"
        );

        // **Kills the tile count**: same length, different country, different price.
        assert!(
            range_span > valley_span,
            "a road over a range costs more to hold than one down a river valley ({range_span} vs \
             {valley_span}) — the per-terrain answer terrain.rs has carried unread for 37 biomes"
        );
        // **Kills the average**: same country, different length, different price.
        assert!(
            range_span > short_range_span,
            "twice the road through the same country costs twice as much ({range_span} vs \
             {short_range_span}); averaging prices these identically and deletes the length half of \
             `length × terrain`"
        );
        assert!(
            (range_span - 2.0 * short_range_span).abs() < 1.0e-5,
            "and it is exactly linear in length"
        );
    }

    /// Every route rung reads its own geometry, and no food-web rung does.
    #[test]
    fn the_route_branch_is_the_only_branch_that_scales_on_its_own_geometry() {
        let ladder = LadderConfig::builtin();
        for rung in &ladder.rungs {
            let Some(upkeep) = rung.upkeep.as_ref() else {
                continue;
            };
            let expected = if rung.branch == RungBranch::Route {
                UpkeepScale::RouteSpan
            } else {
                UpkeepScale::SourceLoad
            };
            assert_eq!(
                upkeep.scaled_by,
                expected,
                "{}:{} scales on the wrong measure",
                rung.branch.as_str(),
                rung.id
            );
        }
    }

    /// The bill is the rung's rate times the road's own span — one rule, and the whole reason a
    /// valley road is cheap to keep and a mountain road is dear.
    #[test]
    fn the_keeping_bill_is_the_rungs_rate_times_the_roads_span() {
        let ladder = LadderConfig::builtin();
        let trail = ladder.rung(RungKey::RouteTrail);
        let rate = trail
            .upkeep
            .as_ref()
            .expect("the trail rung is kept")
            .work_per_turn;

        let valley = span_of_terrains([TerrainType::AlluvialPlain; 5]);
        let range = span_of_terrains([TerrainType::AlpineMountain; 5]);

        assert!(
            (trail.upkeep_demand(valley) - rate * valley).abs() < 1.0e-5,
            "the demand is rate × span"
        );
        assert!(
            trail.upkeep_demand(range) > trail.upkeep_demand(valley),
            "the same trail costs more to hold over a range than down a valley"
        );
        assert_eq!(
            ladder.rung(RungKey::RouteGameTrail).upkeep_demand(range),
            0.0,
            "and a game trail costs nothing to hold over ANY country — nobody maintains one"
        );
    }

    /// **RULE 2 — the road's own path is the catchment.** A band standing on the road is served; one
    /// standing a tile off it is not, and there is no radius to soften that.
    #[test]
    fn a_band_is_served_only_while_it_stands_on_the_road() {
        let ladder = LadderConfig::builtin();
        let mut ledger = RouteLedger::default();
        let path = vec![UVec2::new(3, 3), UVec2::new(4, 3), UVec2::new(5, 3)];
        let id = ledger.insert(path.clone(), &ladder);

        for tile in &path {
            assert_eq!(
                ledger.routes_on_tile(*tile),
                &[id],
                "a band camped on {tile:?} is on this road"
            );
        }
        assert!(
            ledger.routes_on_tile(UVec2::new(4, 4)).is_empty(),
            "ONE TILE OFF THE ROAD IS OFF THE ROAD — accepted, and deliberately not softened with a \
             radius, which is the 'close enough' constant this rule exists to avoid"
        );
    }

    /// **RULE 4 — new traffic prefers a road that already joins both ends**, which is why real road
    /// networks consolidate instead of sprawling into near-duplicates.
    #[test]
    fn new_traffic_prefers_a_road_that_already_joins_both_ends() {
        let ladder = LadderConfig::builtin();
        let mut ledger = RouteLedger::default();
        let id = ledger.insert(
            vec![UVec2::new(1, 1), UVec2::new(2, 1), UVec2::new(3, 1)],
            &ladder,
        );

        assert_eq!(
            ledger.road_joining(UVec2::new(1, 1), UVec2::new(3, 1)),
            Some(id),
            "both ends stand on it, so this traffic widens the existing road"
        );
        assert_eq!(
            ledger.road_joining(UVec2::new(1, 1), UVec2::new(9, 9)),
            None,
            "a road reaching only one of the two ends is not a road you are walking, so new traffic \
             wears its own"
        );
    }

    /// ⛔ **A ROAD LIGHTS ITS TILES ONLY WHILE IT IS BUILT *AND* KEPT** — and all three states are
    /// asserted, because any two of them pass with the third condition dropped.
    #[test]
    fn a_road_lights_its_tiles_only_while_it_is_built_and_kept() {
        let ladder = LadderConfig::builtin();
        let mut route = Route::worn_in(vec![UVec2::new(2, 2)], &ladder);

        // ① The floor. Nobody maintains a game trail, so it lights nothing.
        assert!(!route.is_built());
        assert!(
            !route.grants_sight(),
            "a GAME TRAIL grants no sight — it is free precisely because nobody keeps it"
        );

        // ② A built rung with its bill met.
        let trail_cost = ladder
            .rung(RungKey::RouteTrail)
            .build
            .as_ref()
            .expect("the trail rung is built")
            .work_cost;
        route.set_position(trail_cost, &ladder);
        assert_eq!(route.held_rung(), RungKey::RouteTrail);
        route.upkeep_demanded = Some(4.0);
        route.upkeep_supplied = 4.0;
        assert!(
            route.grants_sight(),
            "a KEPT trail is presence on that ground — paying the upkeep IS the traffic"
        );

        // ③ The same road, short of hands. It goes DARK BEFORE IT DECAYS, which is the honest early
        //    warning that the road is being lost.
        route.upkeep_supplied = 1.0;
        assert!(
            route.upkeep_shortfall() > 0.0,
            "precondition: this road really is short"
        );
        assert!(
            !route.grants_sight(),
            "a road in SHORTFALL goes dark before it decays — the condition is the PAID BILL, not \
             the held rung"
        );
    }

    /// `trace_path` carries both ends, so rule 2 is true of a road the moment it is laid: the camps
    /// that wore it in are standing on it by construction.
    #[test]
    fn a_traced_path_reaches_its_target_and_carries_both_ends() {
        let (from, to) = (UVec2::new(2, 2), UVec2::new(7, 5));
        let path = trace_path(from, to, 40, 30, false);

        assert_eq!(path.first(), Some(&from), "the road starts at the camp");
        assert_eq!(path.last(), Some(&to), "and reaches the other one");
        assert!(
            path.len() >= 2,
            "a road between two different tiles has at least two of them"
        );
        for pair in path.windows(2) {
            assert_eq!(
                hex_distance_wrapped(pair[0], pair[1], 40, false),
                1,
                "every step of a road is one hex — {:?} to {:?} is not",
                pair[0],
                pair[1]
            );
        }
    }

    /// A road forgotten by rule 3 leaves no trace in the tile index either — otherwise
    /// `routes_on_tile` would go on serving bands off a road that no longer exists.
    #[test]
    fn forgetting_a_road_clears_the_tiles_it_ran_over() {
        let ladder = LadderConfig::builtin();
        let mut ledger = RouteLedger::default();
        let tile = UVec2::new(6, 6);
        let id = ledger.insert(vec![tile, UVec2::new(7, 6)], &ladder);
        assert!(!ledger.routes_on_tile(tile).is_empty(), "precondition");

        ledger.remove(id);
        assert!(
            ledger.routes_on_tile(tile).is_empty(),
            "a forgotten road serves nobody"
        );
        assert!(ledger.is_empty());
    }
}
