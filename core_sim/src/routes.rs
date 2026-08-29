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
        LadderConfig, RungBranch, RungKey, RungStanding, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
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
    pub neglect_turns: u32,
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
            upkeep_demanded: None,
            upkeep_supplied: 0.0,
            neglect_turns: 0,
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

    /// What this turn's bill left unpaid, off the **stamped** basis (§2.5), never the live demand.
    pub fn upkeep_shortfall(&self) -> f32 {
        match self.upkeep_demanded {
            Some(demand) => (demand - self.upkeep_supplied).max(0.0),
            None => 0.0,
        }
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
