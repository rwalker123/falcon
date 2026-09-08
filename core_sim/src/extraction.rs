//! **The deposit, the source, and the take** — the producer `wood` and `stone` never had
//! (`docs/plan_extraction.md`, issue #583).
//!
//! Every work site in the game before this one is a **food** site: a species declares a material on
//! its yield edge and you get `B × per_biomass` of it out of the biomass you took *to eat*. Neither
//! of these two materials can be produced that way — timber in proportion to how many acorns you ate
//! is not how a wood works, and a cliff pays no food at all, so the fraction has no `B` to multiply.
//! So they are one problem, not two: **go to a place, spend work, get a material, get no food.**
//!
//! # The one idea
//!
//! > **A deposit is a stock with a capacity and a regrowth rate, and rock's rate is zero.**
//!
//! There is no `is_finite` flag, no finite-deposit branch and no special case: with
//! `regrowth_rate: 0.0` [`deposit_regrowth`] returns nothing, so *a quarry only ever goes down* is
//! arithmetic. And the forestry rungs' [`RungExtractionPayoff::regrowth_multiplier`] multiplies
//! **the deposit's own** rate, so `0 × anything` is still `0` — one payoff block serves both
//! branches and nothing in this module asks which one a rung is on.
//!
//! # What is state and what is not
//!
//! | | where it lives |
//! |---|---|
//! | **capacity** | a pure function of the tile ([`tile_deposit_capacity`]) — `extraction.json`, never saved |
//! | **regrowth rate** | the same, per terrain ([`tile_deposit_regrowth`]) |
//! | **characteristics** | the same, per terrain ([`tile_deposit_characteristics`]) |
//! | **the stock** | [`DepositSource::stock`] — the **only** saved thing, and it starts at capacity |
//! | **the ladder position** | [`DepositSource`] too, like a patch's |
//!
//! A source is therefore **created lazily, the first time a band puts a crew on it**
//! ([`DepositRegistry::open`]): a deposit nobody has ever worked stands at exactly its capacity, and
//! recording that for every tile on the map twice over would be storing a derivation.
//!
//! # The take
//!
//! ```text
//! floor      = (1 − recovery_fraction(position)) × capacity
//! reachable  = max(0, stock − floor)
//! take       = min(workers × yield_per_worker_turn(position), reachable)
//! stock     -= take
//! stock     += regrowth(stock, capacity, regrowth_rate(terrain) × regrowth_multiplier(position))
//! ```
//!
//! **Over-cutting is POSSIBLE and that is the point** — the take is not clamped to the sustainable
//! rate, because the whole of the renewable half is that you can ruin a wood. The warning is a
//! readout, not a guard.
//!
//! **These sources pay NO food and NO fodder.** Not a zero-valued food term; no food term at all. A
//! woodcutter is a mouth that is not gathering, and *that* is what wood costs.

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::Resource;
use glam::UVec2;
use serde::{Deserialize, Serialize};
use sim_schema::TerrainType;

use crate::{
    components::Tile,
    extraction_config::{DepositDef, ExtractionConfig, NEVER_RENEWS, NO_DEPOSIT},
    intensification::{
        interpolate, rung_span, LadderConfig, RungBranch, RungExtractionPayoff, RungKey,
        RungStanding, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
    },
};

/// **WOODCRAFT** — what gathering deadfall teaches, and the gate on `fell`. Discovery ids 2011 (the
/// retired `trailcraft`) and everything below are spoken for; the two extraction branches take the
/// next three (`.claude/rules/core_sim/intensification.md`).
pub const WOODCRAFT_DISCOVERY_ID: u32 = 2014;
/// **CONSERVATIONISM** — what *felling* teaches, and the gate on `coppice`. It is the forestry
/// branch's own knowledge (`docs/plan_extraction.md` §3): **the skill of taking from a wood without
/// ruining it**, learned by being in a position to ruin one.
pub const CONSERVATIONISM_DISCOVERY_ID: u32 = 2015;
/// **QUARRYING** — what picking loose stone teaches, and the gate on `quarry`. **The minerals arc's
/// `mine` will sit above it on this same branch**, which is the whole reason stone and metal share a
/// ladder: quarrying and mining are one skill and only the material in the deposit differs.
pub const QUARRYING_DISCOVERY_ID: u32 = 2016;

/// **A DEPOSIT THAT HAS BEEN TAKEN TO NOTHING** — the stock's floor, and what a wood cut clean sits
/// at until [`deposit_regrowth`] seeds it back.
pub const DEPOSIT_EMPTY: f32 = 0.0;

/// **NOBODY IS WORKING THIS DEPOSIT** — a crew of no hands, which takes nothing and teaches nothing.
pub const NO_CREW_ON_THE_DEPOSIT: u32 = 0;

/// **WHAT A DEPOSIT'S RUNGS COST THIS SOURCE** — the ladder's own price, unscaled.
///
/// It is stated rather than left implicit because [`RungStanding::at`] takes a per-source price list
/// and both food webs supply a real one (a species' `taming_cost_multiplier`, a patch's
/// `field_cost_multiplier`). **A deposit has no such multiplier and deliberately gets none**: the
/// thing that varies between two woods is the *capacity*, which is already the terrain's, and a
/// second per-source price would be a second way for the same ground to be dearer.
const DEPOSIT_RUNG_PRICE: f32 = RUNG_COST_UNSCALED;

/// **A LIVE WORKING ON A DEPOSIT** — the source a band's `extract` row names, and the twin of
/// `ForagePatch` / `Herd` on the two deposit branches.
///
/// It carries **the stock and the ladder position, and nothing else that could be derived**: the
/// capacity, the rate and the material's characteristics are all read fresh off the tile every turn
/// through this module's three seams, so retuning `extraction.json` reaches every working already on
/// the map and a saved source cannot carry stale terrain figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositSource {
    /// The tile this working stands on — half of its registry key.
    pub tile: UVec2,
    /// The `extraction.json` deposit being worked — the other half. **One tile can hold two**, and
    /// working the timber is not working the rock.
    pub material: String,
    /// **The standing stock**, drawn down by [`take_from_deposit`] and put back by
    /// [`deposit_regrowth`]. The only saved thing about a deposit, and it opens at the tile's
    /// capacity.
    pub stock: f32,
    /// **HOW FAR UP ITS BRANCH THIS WORKING HAS BEEN RAISED, in cumulative work units** — the
    /// deposit branches' one meter (`docs/plan_standing_upkeep.md` §2.8), exactly as a patch's.
    ///
    /// ⛔ **PRIVATE, for [`crate::forage::ForagePatch`]'s reason**: [`Self::standing`] is derived
    /// from it and is what every rate seam reads, so a public field would let a caller move one
    /// without the other. [`Self::set_ladder_position`] is the only way to move it and it writes
    /// both.
    ladder_position: f32,
    /// **WHERE THIS WORKING STANDS — DERIVED, AND RE-STAMPED ON EVERY WRITE TO
    /// [`Self::ladder_position`]**. Stored rather than resolved on demand because the readers hold
    /// no ladder.
    standing: RungStanding,
}

impl DepositSource {
    /// **A FRESH WORKING ON UNTOUCHED GROUND** — full stock, standing on its branch's free floor.
    ///
    /// `branch` is the deposit's ([`DepositDef::branch`]), because the free floor differs by ladder
    /// and only the config knows which one this material is worked by.
    pub fn opening(tile: UVec2, material: &str, capacity: f32, branch: RungBranch) -> Self {
        Self {
            tile,
            material: material.to_string(),
            stock: capacity.max(DEPOSIT_EMPTY),
            ladder_position: RUNG_UNSTARTED,
            standing: RungStanding::unstarted(branch),
        }
    }

    /// The work banked into this working, in cumulative units.
    pub fn ladder_position(&self) -> f32 {
        self.ladder_position
    }

    /// **WHERE IT STANDS** — the one verdict every per-rung quantity is read off.
    pub fn standing(&self) -> &RungStanding {
        &self.standing
    }

    /// **THE RUNG THIS WORKING HOLDS IN FULL** — what a lesson is read off and what a readout names.
    pub fn rung(&self) -> RungKey {
        self.standing.held
    }

    /// **THE ONE WRITER OF THE POSITION**, which re-stamps the standing with it.
    pub fn set_ladder_position(
        &mut self,
        position: f32,
        ladder: &LadderConfig,
        branch: RungBranch,
    ) {
        self.ladder_position = position.max(RUNG_UNSTARTED);
        self.standing = deposit_standing(self.ladder_position, ladder, branch);
    }
}

/// **WHERE A POSITION PUTS A WORKING ON ITS BRANCH** — [`RungStanding::at`] at the deposit branches'
/// flat price ([`DEPOSIT_RUNG_PRICE`]).
pub fn deposit_standing(position: f32, ladder: &LadderConfig, branch: RungBranch) -> RungStanding {
    RungStanding::at(ladder, branch, position, |rung| {
        ladder
            .rung(rung)
            .build
            .as_ref()
            .map(|build| build.work_cost * DEPOSIT_RUNG_PRICE)
    })
}

/// **THE BASE AND WIDTH OF ONE DEPOSIT RUNG**, in cumulative work units — [`deposit_standing`]'s
/// twin, for a caller that needs to know where a rung's leg starts.
pub fn deposit_rung_span(rung: RungKey, ladder: &LadderConfig) -> (f32, f32) {
    rung_span(rung, &|key: RungKey| {
        ladder
            .rung(key)
            .build
            .as_ref()
            .map(|build| build.work_cost * DEPOSIT_RUNG_PRICE)
    })
}

/// **WHAT THIS WORKING'S POSITION BUYS** — the rung payoff [`interpolate`]d over the standing, the
/// general rule (`docs/plan_standing_upkeep.md` §2.8). All three terms are **rates and shares**, so
/// all three blend; nothing here is a classifier whose cut points would have to step.
pub fn deposit_payoff(standing: &RungStanding, ladder: &LadderConfig) -> RungExtractionPayoff {
    let term = |pick: fn(&RungExtractionPayoff) -> f32| {
        interpolate(standing, |rung| {
            pick(
                ladder
                    .rung(rung)
                    .extraction_payoff
                    .as_ref()
                    .expect("validate requires an extraction_payoff on every deposit rung"),
            )
        })
    };
    RungExtractionPayoff {
        yield_per_worker_turn: term(|payoff| payoff.yield_per_worker_turn),
        recovery_fraction: term(|payoff| payoff.recovery_fraction),
        regrowth_multiplier: term(|payoff| payoff.regrowth_multiplier),
    }
}

/// **THE DEPOSIT-CAPACITY OF A TILE — the single source the opening path, the site rule and any
/// future wire path all read**, so what seeds a working and what a refusal is struck against can
/// never drift. `forage::tile_forage_capacity`'s exact discipline, one material over.
///
/// [`NO_DEPOSIT`] where the terrain carries no row, which is what makes *absence is the answer* a
/// reading: there is nothing here, so nothing can be opened here and the verb is refused here.
///
/// ⛔ **IT READS `tile.terrain`, NEVER `resource_terrain()`**, and so do its two siblings below. A
/// deposit is a thing on the ground of the hex, so a navigable river does **not** inherit the timber
/// of the valley it cut — which is exactly where `forage::tile_forage_capacity`'s underlay reading
/// *does* belong, because a fishery is a property of the water standing over that valley.
pub fn tile_deposit_capacity(config: &ExtractionConfig, material: &str, tile: &Tile) -> f32 {
    config
        .deposit(material)
        .and_then(|deposit| deposit.terrain(tile.terrain))
        .map(|ground| ground.capacity)
        .unwrap_or(NO_DEPOSIT)
}

/// **THE GROUND'S OWN RENEWAL RATE** — [`NEVER_RENEWS`] on every rock body and on any terrain with
/// no deposit at all. The rung's `regrowth_multiplier` multiplies *this*, which is what makes
/// *stone's rate is zero* survive as arithmetic.
pub fn tile_deposit_regrowth(config: &ExtractionConfig, material: &str, tile: &Tile) -> f32 {
    config
        .deposit(material)
        .and_then(|deposit| deposit.terrain(tile.terrain))
        .map(|ground| ground.regrowth_rate)
        .unwrap_or(NEVER_RENEWS)
}

/// **WHAT THIS GROUND'S MATERIAL IS LIKE** — the vector that rides every unit into the band's store,
/// so a streambed pays knappable flint and a quarry pays building block out of one generic material.
/// Empty for a terrain with no deposit, which no arrival can reach.
pub fn tile_deposit_characteristics(
    config: &ExtractionConfig,
    material: &str,
    tile: &Tile,
) -> BTreeMap<String, f32> {
    config
        .deposit(material)
        .and_then(|deposit| deposit.terrain(tile.terrain))
        .map(|ground| ground.characteristics.clone())
        .unwrap_or_default()
}

/// **THE LADDER THIS MATERIAL IS WORKED BY** — `forestry` for wood, `extraction` for stone and every
/// metal after it. `None` for a material no deposit yields.
pub fn deposit_branch(config: &ExtractionConfig, material: &str) -> Option<RungBranch> {
    config.deposit(material).map(|deposit| deposit.branch)
}

/// **EVERY MATERIAL THIS TERRAIN HOLDS**, in the config's own id order — what a tile offers a band
/// that walks onto it. A wooded highland answers both.
pub fn terrain_deposits(
    config: &ExtractionConfig,
    terrain: TerrainType,
) -> impl Iterator<Item = (&str, &DepositDef)> {
    config
        .deposits()
        .filter(move |(_, deposit)| deposit.terrain(terrain).is_some())
}

/// **WHAT THIS RUNG MAY NOT DRAW BELOW** — `(1 − recovery_fraction) × capacity`, the fauna escapement
/// floor upside down (`docs/plan_extraction.md` §4b).
///
/// ⛔ **IT CAN ONLY EVER MOVE DOWN, and that is the §6 floor trap's guard.** `capacity` is the
/// terrain's and no rung may raise it, so climbing the ladder strictly *lowers* this — where the trap
/// was a rung raising a herd's `K`, dragging `floor_fraction × K` up under a tame already standing on
/// it and making that build uncompletable at any crew size.
pub fn deposit_floor(capacity: f32, payoff: &RungExtractionPayoff) -> f32 {
    ((1.0 - payoff.recovery_fraction) * capacity).max(DEPOSIT_EMPTY)
}

/// **WHAT IS LEFT ABOVE THE FLOOR FOR THIS RUNG TO TAKE** — the deposit's twin of
/// `forage::patch_take_room`, and what the take is capped by.
pub fn deposit_reachable(stock: f32, capacity: f32, payoff: &RungExtractionPayoff) -> f32 {
    (stock - deposit_floor(capacity, payoff)).max(DEPOSIT_EMPTY)
}

/// **WHAT A CREW OF `workers` TAKES THIS TURN** — `min(what the hands can lift, what the rung can
/// reach)`.
///
/// ⛔ **IT IS NOT CLAMPED TO THE SUSTAINABLE RATE, ON PURPOSE.** Over-cutting has to be *possible* or
/// the renewable half of this model buys nothing: the whole point of a wood is that you can ruin it.
/// What warns the player is a readout (`docs/plan_extraction.md` §7 — the existing
/// sustainable-versus-actual breakdown pointed at a new source), never a guard here.
///
/// **There is no kit term.** The shipped roster declares no *take* gear on either branch — forestry
/// deliberately (its natural tool is an axe and a bone-hafted axe is a roster question §9 leaves
/// open) and extraction because its two tools are `build_work`, which lands on the pool that
/// *raises* a working. `yield_per_worker_turn` is therefore a bare-handed rate throughout, which is
/// what makes the free floor of both branches workable with an empty kit roster.
pub fn deposit_take(workers: u32, stock: f32, capacity: f32, payoff: &RungExtractionPayoff) -> f32 {
    let labor = workers as f32 * payoff.yield_per_worker_turn;
    labor
        .max(DEPOSIT_EMPTY)
        .min(deposit_reachable(stock, capacity, payoff))
}

/// **ONE TURN OF RENEWAL** — the logistic curve every stock in this game grows on, evaluated at a
/// **seeded** reading so a deposit taken to nothing can come back.
///
/// ⛔ **THE SEED IS THE POINT THE CURVE IS READ AT, NOT A LIFT ON THE STOCK.**
/// `forage::regrow_patch` lifts the biomass itself to `reseed_floor_fraction × K` before growing it,
/// and doing that here would raise a **quarry** off zero — the one thing this whole model exists to
/// make impossible. Reading the curve at `max(stock, seed)` and adding the *delta* to the real stock
/// means the seed is multiplied by the deposit's own rate, so a rate of [`NEVER_RENEWS`] seeds
/// exactly nothing and rock stays monotonically non-increasing by arithmetic rather than by a branch.
///
/// **`max`, not `+`.** An additive seed would hold the curve's zero-crossing `seed × capacity`
/// *below* capacity, so a wood would stall a couple of percent short of full for ever — a stated
/// ceiling nothing ever reaches. Taking the larger of the two leaves a healthy stock untouched,
/// which is `reseeding_logistic_regrowth`'s own reasoning about its `max()`.
///
/// The result is clamped to `capacity`: renewal fills a deposit, it never overfills one.
pub fn deposit_regrowth(stock: f32, capacity: f32, rate: f32, seed_fraction: f32) -> f32 {
    if capacity <= NO_DEPOSIT {
        return stock.max(DEPOSIT_EMPTY);
    }
    let stock = stock.max(DEPOSIT_EMPTY);
    let seeded = stock.max(seed_fraction * capacity);
    let delta = (rate * seeded * (1.0 - seeded / capacity)).max(0.0);
    (stock + delta).min(capacity)
}

/// **EVERY LIVE WORKING**, keyed by the pair that names one — the deposit branches' twin of
/// `ForageRegistry` / `HerdRegistry`.
///
/// A `BTreeMap` rather than a `HashMap`: the turn's take walks it and a checkpoint records it, so the
/// order has to be an order and not an accident.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DepositRegistry {
    /// Live workings, keyed `(tile, material)`.
    pub sources: BTreeMap<(u32, u32, String), DepositSource>,
}

impl DepositRegistry {
    fn key(tile: UVec2, material: &str) -> (u32, u32, String) {
        (tile.x, tile.y, material.to_string())
    }

    pub fn source(&self, tile: UVec2, material: &str) -> Option<&DepositSource> {
        self.sources.get(&Self::key(tile, material))
    }

    pub fn source_mut(&mut self, tile: UVec2, material: &str) -> Option<&mut DepositSource> {
        self.sources.get_mut(&Self::key(tile, material))
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// **OPEN A WORKING ON THIS GROUND, or hand back the one already standing there** — the lazy
    /// seeding this module uses instead of a worldgen sweep.
    ///
    /// A deposit nobody has ever worked stands at exactly the tile's capacity, which is a pure
    /// function of the tile — so recording one per land tile per material would be storing a
    /// derivation, twice over, for the whole map.
    ///
    /// `None` where **the ground holds none of this material**, which is the one refusal: absence is
    /// the answer, so there is nothing to open and nothing to work.
    pub fn open(
        &mut self,
        tile: UVec2,
        material: &str,
        ground: &Tile,
        config: &ExtractionConfig,
    ) -> Option<&mut DepositSource> {
        let capacity = tile_deposit_capacity(config, material, ground);
        if capacity <= NO_DEPOSIT {
            return None;
        }
        let branch = deposit_branch(config, material)?;
        Some(
            self.sources
                .entry(Self::key(tile, material))
                .or_insert_with(|| DepositSource::opening(tile, material, capacity, branch)),
        )
    }

    /// Drop a working outright — the seam a restore and a test fixture use.
    pub fn remove(&mut self, tile: UVec2, material: &str) -> Option<DepositSource> {
        self.sources.remove(&Self::key(tile, material))
    }

    /// Insert a working wholesale — the checkpoint restore's seam.
    pub fn insert(&mut self, source: DepositSource) {
        let key = Self::key(source.tile, &source.material);
        self.sources.insert(key, source);
    }
}

/// **WHAT ONE TURN'S WORK ON ONE DEPOSIT PRODUCED** — what [`take_from_deposit`] hands back, so the
/// caller can pay it out and say so.
#[derive(Debug, Clone, PartialEq)]
pub struct DepositTake {
    /// The units taken, in the material's own units. [`DEPOSIT_EMPTY`] when the crew hit the rung's
    /// floor with nothing above it.
    pub taken: f32,
    /// **WHAT THE RUNG COULD STILL HAVE REACHED before this take** — the room the crew was working
    /// in, which is what says whether the source is workable at all.
    pub reachable_before: f32,
    /// The stock as the take left it, **before** renewal.
    pub stock_after_take: f32,
    /// The stock as renewal left it — what the source now carries.
    pub stock: f32,
}

/// **THE TAKE, THE DRAW-DOWN AND THE RENEWAL, IN THAT ORDER** — one turn of one working, resolved as
/// a pure function of the source, the ground and the ladder so the turn's system and any projection
/// read one model.
///
/// **Renewal happens after the take**, unlike the food webs (which regrow in Logistics and gather in
/// Population). A deposit has no ecology phase and no forecast riding a `before_regrowth` reading, so
/// there is nothing for the split to serve — and taking first is what makes *"this turn's crew could
/// reach `reachable_before`"* a fact about the stock the crew actually found.
pub fn take_from_deposit(
    source: &mut DepositSource,
    workers: u32,
    ground: &Tile,
    config: &ExtractionConfig,
    ladder: &LadderConfig,
) -> DepositTake {
    let capacity = tile_deposit_capacity(config, &source.material, ground);
    let payoff = deposit_payoff(&source.standing, ladder);
    let reachable_before = deposit_reachable(source.stock, capacity, &payoff);
    let taken = deposit_take(workers, source.stock, capacity, &payoff);
    let stock_after_take = (source.stock - taken).max(DEPOSIT_EMPTY);
    // **The ground's own rate, scaled by what the rung bought.** Rock's is zero, so a forestry
    // rung's multiplier cannot make one renew however it is tuned.
    let rate = tile_deposit_regrowth(config, &source.material, ground) * payoff.regrowth_multiplier;
    source.stock = deposit_regrowth(stock_after_take, capacity, rate, config.seed_fraction);
    DepositTake {
        taken,
        reachable_before,
        stock_after_take,
        stock: source.stock,
    }
}

/// **HOW ONE TURN'S TAKE IS SPLIT BETWEEN THE BANDS THAT MADE IT** — by head count, which is the only
/// division that has a meaning: the take is `Σ workers × rate`, so each band's share is its own
/// hands' contribution to that sum.
///
/// Returns nothing for a crew of [`NO_CREW_ON_THE_DEPOSIT`], which cannot have taken anything.
pub fn share_of_take(taken: f32, crew: u32, total_crew: u32) -> f32 {
    if total_crew == NO_CREW_ON_THE_DEPOSIT {
        return DEPOSIT_EMPTY;
    }
    taken * (crew as f32 / total_crew as f32)
}

/// **EVERY BAND'S CREW ON EVERY WORKING, this turn** — the one sweep the take pass builds, so a
/// source two bands work is taken from **once** at the summed crew rather than twice at each.
///
/// Keyed exactly as [`DepositRegistry`] is, and the value is `(total crew, per-band crews)` in band
/// order.
pub type DepositCrews = HashMap<(u32, u32, String), Vec<(usize, u32)>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intensification::{NO_DEPOSIT_REACHED, REGROWTH_UNCHANGED, WHOLE_DEPOSIT_REACHED};

    /// A payoff stated inline, so each test says exactly what rung shape it is asserting about.
    fn payoff(
        yield_per_worker_turn: f32,
        recovery_fraction: f32,
        regrowth_multiplier: f32,
    ) -> RungExtractionPayoff {
        RungExtractionPayoff {
            yield_per_worker_turn,
            recovery_fraction,
            regrowth_multiplier,
        }
    }

    /// A capacity big enough that a single turn's take is a small share of it — the shape a real
    /// rock body has, so the arithmetic under test is not dominated by the floor.
    const A_ROCK_BODY: f32 = 1000.0;
    /// The seed fraction the shipped table uses; stated here so the tests are not reading config.
    const A_SEED: f32 = 0.02;

    /// ⛔ **THE ONE IDEA.** A deposit whose rate is zero never gains a unit, whatever multiplier a
    /// rung applies to it — `0 × anything` is still `0`, and there is no branch anywhere that
    /// checks.
    #[test]
    fn a_rate_of_zero_renews_nothing_at_any_multiplier() {
        for multiplier in [REGROWTH_UNCHANGED, 2.0, 100.0] {
            let grown = deposit_regrowth(
                A_ROCK_BODY / 2.0,
                A_ROCK_BODY,
                NEVER_RENEWS * multiplier,
                A_SEED,
            );
            assert_eq!(
                grown,
                A_ROCK_BODY / 2.0,
                "a quarry must not renew at multiplier {multiplier}"
            );
        }
    }

    /// **A wood recovers, and never past capacity.**
    #[test]
    fn a_wood_climbs_back_and_stops_at_capacity() {
        let capacity = 600.0;
        let mut stock = capacity * 0.25;
        for _ in 0..500 {
            let grown = deposit_regrowth(stock, capacity, 0.03, A_SEED);
            assert!(grown >= stock, "renewal must never take a wood backwards");
            assert!(grown <= capacity, "renewal must never overfill a deposit");
            stock = grown;
        }
        assert!(
            stock > capacity * 0.99,
            "five hundred turns of renewal must refill a wood, reached {stock}"
        );
    }

    /// **A wood cut clean comes back** — the seed term's whole job, and the reason it is inside the
    /// growth term rather than a lift on the stock.
    #[test]
    fn a_wood_taken_to_nothing_still_seeds_itself() {
        let grown = deposit_regrowth(DEPOSIT_EMPTY, 600.0, 0.03, A_SEED);
        assert!(
            grown > DEPOSIT_EMPTY,
            "a cleared wood must regrow from its seed, got {grown}"
        );
    }

    /// **The rung lowers the floor, and that is what raises the reach.**
    #[test]
    fn a_higher_rung_reaches_deeper_into_the_same_deposit() {
        let shallow = payoff(0.4, 0.15, REGROWTH_UNCHANGED);
        let deep = payoff(2.2, 0.85, REGROWTH_UNCHANGED);
        assert!(deposit_floor(A_ROCK_BODY, &deep) < deposit_floor(A_ROCK_BODY, &shallow));
        assert!(
            deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &deep)
                > deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow)
        );
    }

    /// **The take is capped by the reach, not by the hands** — pile on enough workers to outrun the
    /// rung's whole reach and the take stops at the floor.
    ///
    /// The crew is stated well past the crossing (`1000 × 0.4 = 400` against a reach of `150`)
    /// rather than at it, so the assertion is about the **cap** and not about a boundary a retune of
    /// the shallow rung's rate would move.
    #[test]
    fn the_take_stops_at_the_rungs_floor_however_many_hands_are_on_it() {
        let shallow = payoff(0.4, 0.15, REGROWTH_UNCHANGED);
        let reach = deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow);
        assert!(
            1000.0 * shallow.yield_per_worker_turn > reach,
            "fixture: the crew must outrun the reach, or this asserts nothing"
        );
        assert_eq!(
            deposit_take(1000, A_ROCK_BODY, A_ROCK_BODY, &shallow),
            reach,
            "the rung's floor is the cap, not the crew"
        );
    }

    /// **Over-cutting is reachable** — a wood at its ceiling with the whole deposit in reach loses
    /// stock turn on turn when the crew outpaces the renewal. If this ever fails the renewable half
    /// of the model has quietly become a guarantee.
    #[test]
    fn enough_hands_drive_a_wood_down_turn_on_turn() {
        let capacity = 600.0;
        let felling = payoff(2.0, WHOLE_DEPOSIT_REACHED, REGROWTH_UNCHANGED);
        let mut stock = capacity;
        let mut previous = stock;
        for _ in 0..20 {
            let taken = deposit_take(10, stock, capacity, &felling);
            let after = (stock - taken).max(DEPOSIT_EMPTY);
            stock = deposit_regrowth(after, capacity, 0.03 * felling.regrowth_multiplier, A_SEED);
            assert!(stock < previous, "a wood being over-cut must fall");
            previous = stock;
        }
    }

    /// **A rung that reaches nothing takes nothing** — the floor's degenerate end, stated so the
    /// arithmetic is pinned at both ends of the interval the config is bounded to.
    #[test]
    fn a_rung_that_reaches_none_of_the_deposit_takes_none_of_it() {
        let none = payoff(5.0, NO_DEPOSIT_REACHED, REGROWTH_UNCHANGED);
        assert_eq!(deposit_floor(A_ROCK_BODY, &none), A_ROCK_BODY);
        assert_eq!(
            deposit_take(50, A_ROCK_BODY, A_ROCK_BODY, &none),
            DEPOSIT_EMPTY
        );
    }

    /// **A take is split by head count**, so a source two bands work pays each what its own hands
    /// earned and the parts sum to the whole.
    #[test]
    fn a_shared_working_splits_its_take_by_hands() {
        let taken = 9.0;
        let first = share_of_take(taken, 2, 3);
        let second = share_of_take(taken, 1, 3);
        assert!((first + second - taken).abs() < 1e-5);
        assert!(first > second);
    }
}
