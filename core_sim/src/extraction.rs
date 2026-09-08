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
//! # The turn: regrow, then take
//!
//! ```text
//! Logistics  (once per working, `advance_deposits`)
//!   stock    += regrowth(stock, capacity, regrowth_rate(terrain) × regrowth_multiplier(position))
//! Population (once per band row on it)
//!   floor     = (1 − recovery_fraction(position)) × capacity
//!   reachable = max(0, stock − floor)
//!   take      = min(workers × yield_per_worker_turn(position), reachable)
//!   stock    -= take
//! ```
//!
//! **The growth term runs once per working and the take once per row**, which is the plant web's
//! split and is load-bearing rather than tidy: renewal inside the take ran `K` times on a working
//! `K` bands shared and never at all on one nobody held — see [`take_from_deposit`].
//!
//! **Over-cutting is POSSIBLE and that is the point** — the take is not clamped to the sustainable
//! rate, because the whole of the renewable half is that you can ruin a wood. The warning is a
//! readout, not a guard.
//!
//! **These sources pay NO food and NO fodder.** Not a zero-valued food term; no food term at all. A
//! woodcutter is a mouth that is not gathering, and *that* is what wood costs.

use std::collections::BTreeMap;

use bevy::prelude::{Res, ResMut, Resource};
use glam::UVec2;
use serde::{Deserialize, Serialize};

use crate::{
    components::Tile,
    extraction_config::{ExtractionConfig, NEVER_RENEWS, NO_DEPOSIT},
    intensification::{
        interpolate, rung_span, LadderConfig, RungBranch, RungExtractionPayoff, RungKey,
        RungStanding, NEGLECT_NONE, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
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
    /// **THIS TURN'S BILL, STAMPED ONCE** — what holding this working was judged to cost, struck by
    /// [`advance_deposits`] at the post-decay position and read by everything downstream.
    ///
    /// ⛔ **STAMPED, NOT RE-DERIVED, for `ForagePatch::upkeep_demanded`'s reason**: the demand
    /// interpolates on the position and the build arm moves the position inside the same turn, so a
    /// bill struck on one side of the accrual and a payment on the other are two readings of two
    /// different workings — and `demand − supplied == shortfall` goes false. `None` is *"nobody has
    /// judged it this turn"*, which is a working opened part-way through a Population stage and is
    /// forgiven exactly as an unbilled road is.
    #[serde(default)]
    pub upkeep_demanded: Option<f32>,
    /// **WHAT THIS TURN'S KEEPERS PUT ON IT**, in work units. Accumulates (`+=`) across the bands
    /// working the source — §2.5's rule, kept even though a working has one row per band — and is
    /// cleared once per turn by [`advance_deposits`].
    #[serde(default)]
    pub upkeep_supplied: f32,
    /// **CONSECUTIVE TURNS THE KEEPING WENT UNMET.** Reset outright by any turn it was met, so it is
    /// a run rather than a lifetime budget. The bleed applies only while it **exceeds** the at-risk
    /// rung's `upkeep.grace_turns` — a crew re-tasked for a season does not cost the working.
    #[serde(default)]
    pub neglect_turns: u16,
}

impl DepositSource {
    /// **A FRESH WORKING ON UNTOUCHED GROUND** — full stock, standing on its branch's free floor.
    ///
    /// `branch` is the deposit's ([`crate::extraction_config::DepositDef::branch`]), because the
    /// free floor differs by ladder
    /// and only the config knows which one this material is worked by.
    pub fn opening(tile: UVec2, material: &str, capacity: f32, branch: RungBranch) -> Self {
        Self {
            tile,
            material: material.to_string(),
            stock: capacity.max(DEPOSIT_EMPTY),
            ladder_position: RUNG_UNSTARTED,
            standing: RungStanding::unstarted(branch),
            upkeep_demanded: None,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            neglect_turns: NEGLECT_NONE,
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

// **RETIRED BEFORE IT HAD A CALLER: `terrain_deposits`** — *"every material this terrain holds"*,
// in the config's own id order.
//
// It answers a **tile-card** question — *what does this ground offer a band that walks onto it* —
// and nothing about a deposit reaches the client yet (`docs/plan_extraction.md` §7). A `pub fn` with
// no caller is either a seam with a stated future reader or it is dead, and this one had no reader
// named. `ExtractionConfig::deposits()` plus `DepositDef::terrain` is the whole of it, so the
// readout slice re-adds it in four lines rather than inheriting a guess at its signature.

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

/// **HOW BIG A WORKING THIS IS, IN KEEPER-LOADS** — the [`crate::intensification::UpkeepScale::SourceLoad`] measure the
/// two deposit rungs quote their `work_per_turn` per, and the third branch reading of one primitive.
///
/// It is **the deposit's own capacity over the material's `capacity_per_keeper`** — a great wood
/// takes more holding than a few stands along a draw, and a mountain quarry more than a hillside
/// one. That is the plant web's shape exactly (`forage::patch_tender_loads`, the tile's own `K` over
/// `cultivation.capacity_per_tender`) and the route branch's (`road_upkeep_measure`, the tile's own
/// `infrastructure_cost`): **all three shipped branches measure the PLACE, never the activity**, and
/// a fourth reading here would be a departure with nothing behind it.
///
/// ⛔ **IT IS POSITION-FREE, AND THAT IS NOT A SIMPLIFICATION — IT IS `capacity_per_tender`'s TRAP
/// AVOIDED.** The obvious alternative — *how much of the deposit this rung can reach*,
/// `recovery_fraction × capacity` — interpolates on the position, and `upkeep.work_per_turn`
/// interpolates on the position too, so the two would **compound**: a quarry would owe the rung's
/// climb twice over, which is exactly the 10× a Field landed at when the plant measure briefly read
/// the boosted `carrying_capacity` instead of the tile's own `K`. Capacity is the terrain's and no
/// rung may raise it (`no_rung_on_either_branch_may_raise_capacity`), so this measure provably
/// cannot compound with the rate that rides it.
///
/// **The ratio belongs to the MATERIAL and the rate to the RUNG**, which is `animals_per_herder`'s
/// division: 600 units of wood and 600 units of stone are not the same size of job, so one global
/// divisor would make the two branches' bills incomparable for a reason that is about units rather
/// than about workings.
pub fn deposit_keeper_loads(capacity: f32, per_keeper: f32) -> f32 {
    if per_keeper <= NO_KEEPER_RATIO {
        return NO_UPKEEP_DEMAND;
    }
    (capacity / per_keeper).max(NO_UPKEEP_DEMAND)
}

/// **A DIVISOR THAT WOULD DIVIDE BY ZERO** — [`deposit_keeper_loads`]'s guard. `validate` rejects a
/// `capacity_per_keeper` at or below it, so this is the arithmetic's own backstop rather than a
/// live path.
const NO_KEEPER_RATIO: f32 = 0.0;

/// **THIS WORKING'S KEEPER-LOAD**, resolved off the tile it stands on. `NO_UPKEEP_DEMAND` for ground
/// that holds none of the material, which no live working can be standing on.
pub fn deposit_measure(source: &DepositSource, ground: &Tile, config: &ExtractionConfig) -> f32 {
    let Some(deposit) = config.deposit(&source.material) else {
        return NO_UPKEEP_DEMAND;
    };
    deposit_keeper_loads(
        tile_deposit_capacity(config, &source.material, ground),
        deposit.capacity_per_keeper,
    )
}

/// **WHAT HOLDING THIS WORKING COSTS PER TURN**, in work units — the rung's `work_per_turn`
/// [`interpolate`]d over the standing and scaled by [`deposit_measure`], the shape every branch's
/// demand takes.
///
/// **Both free floors declare no `upkeep` at all**, so a working that has never been raised owes
/// exactly nothing — `plant:wild` and `route:path`'s own reading, and what makes the floor free.
pub fn deposit_upkeep_demand(source: &DepositSource, measure: f32, ladder: &LadderConfig) -> f32 {
    interpolate(&source.standing, |rung| {
        ladder.rung(rung).upkeep_demand(measure)
    })
}

/// **THE BILL THIS TURN'S SHARE WAS STRUCK AGAINST** — the stamp where [`advance_deposits`] has made
/// one, the live demand where it has not. `routes::road_keeping_basis`'s rule, and the reason both
/// halves of `demand − supplied == shortfall` describe one position.
pub fn deposit_keeping_basis(source: &DepositSource, measure: f32, ladder: &LadderConfig) -> f32 {
    source
        .upkeep_demanded
        .unwrap_or_else(|| deposit_upkeep_demand(source, measure, ladder))
}

/// **THE RUNG AT RISK ON THIS WORKING** — the newest rung carrying work, which is the rung a decay
/// eats and whose grace and rot rate govern. `routes::road_at_risk_rung`'s twin, and one helper for
/// the same reason: the bill, the grace and the bleed must not read different rungs.
pub fn deposit_at_risk_rung(standing: &RungStanding) -> RungKey {
    standing
        .raising
        .filter(|_| standing.banked > crate::intensification::NO_RUNG_WORK_BANKED)
        .unwrap_or(standing.held)
}

/// **WHAT THIS WORKING'S METER WILL LOSE ON THE NEXT DECAY PASS**, in work units — the term a build
/// countdown nets its supply against, resolved through the same seams [`advance_deposits`] bleeds
/// through so a quote cannot promise a rung will finish while the pass takes more off it.
///
/// **One currency, because a working owes no material RATE.** `validate` refuses an
/// `upkeep.materials` on either deposit branch, so the work pair is the whole of *how short* here —
/// see that check for why the alternative was a dial that reads live and bills nothing.
pub fn deposit_meter_rot(source: &DepositSource, measure: f32, ladder: &LadderConfig) -> f32 {
    let rung = ladder.rung(deposit_at_risk_rung(&source.standing));
    if rung.upkeep.is_none() {
        return crate::intensification::NO_UPKEEP_DECAY;
    }
    rung.meter_rot_at_fraction(
        crate::intensification::upkeep_shortfall_fraction(
            deposit_keeping_basis(source, measure, ladder),
            source.upkeep_supplied,
        ),
        source.neglect_turns,
    )
}

// **RETIRED BEFORE IT HAD A CALLER: `deposit_neglect_grace_remaining`** — the countdown a working
// publishes beside its shortfall, `routes::road_neglect_grace_remaining`'s twin.
//
// Its one consumer on every other branch is the **wire** (`hasNeglectGrace` /
// `neglectGraceRemaining`), and a working has no wire row (`docs/plan_extraction.md` §7). Nothing in
// the sim branches on a grace *remaining* — the bleed asks `RungDef::upkeep_decay`, which owns the
// `>` against `neglect_turns` — so this was a readout with no reader. It comes back with the
// deposit row, as `neglect_grace_remaining(source.neglect_turns, rung.upkeep_grace_turns())` on the
// at-risk rung, which is the same three lines.

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
    /// The stock as the take left it — what the source now carries. **Renewal has already happened
    /// this turn**, a whole stage earlier ([`renew_deposit`]), so this is the low-water mark the
    /// next turn's growth term will start from.
    pub stock: f32,
}

/// **THE TAKE, THE DRAW-DOWN AND THE RENEWAL, IN THAT ORDER** — one turn of one working, resolved as
/// a pure function of the source, the ground and the ladder so the turn's system and any projection
/// read one model.
///
/// # ⛔ IT DOES NOT RENEW, AND THAT IS THE ONE THING THIS SEAM MUST NOT DO
///
/// **Renewal is [`renew_deposit`]'s, called once per working per turn by [`advance_deposits`]** —
/// the plant web's arrangement exactly (`advance_forage_regrowth` in Logistics, the gather in
/// Population), so the turn's order is **regrow → take**.
///
/// It ran here for one slice, and *"a deposit has no forecast riding a pre-regrowth reading, so
/// there is nothing for the split to serve"* was the wrong reading of what the split is for. **The
/// split is about how often the growth term runs.** This function is called **once per band-row**,
/// so renewal inside it failed in both directions at once:
///
/// - **two bands holding a row on one wood ran the growth term twice in a turn** — `K` bands, `K`×
///   renewal — which undercut *"over-cutting is possible and must stay so"* in exact proportion to
///   how many bands shared a deposit;
/// - **a working no band held a row on never renewed at all**, so an abandoned over-cut wood was
///   frozen at its low-water mark for ever — against the arc's own headline that a wood recovers and
///   rock does not.
///
/// What survives of the old reading is [`DepositTake::reachable_before`], which is still *"the room
/// this turn's crew actually found"* — it is simply found after the pass has grown the stand rather
/// than before.
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
    source.stock = (source.stock - taken).max(DEPOSIT_EMPTY);
    DepositTake {
        taken,
        reachable_before,
        stock: source.stock,
    }
}

/// **ONE TURN OF RENEWAL ON ONE WORKING** — the deposit branches' `forage::regrow_patch`, and the
/// one seam [`advance_deposits`] and any projection share.
///
/// **The ground's own rate, scaled by what the rung bought.** Rock's rate is `0`, so a forestry
/// rung's `regrowth_multiplier` cannot make a quarry renew however it is tuned — the arc's whole
/// arithmetic, applied at the one place the stock grows.
///
/// **It reads the POST-DECAY position**, because `advance_deposits` bleeds before it grows: a
/// working that has slumped off its coppice rung renews at the rate it now stands on, which is what
/// makes an unkept managed wood fall back to an ordinary one rather than keeping its management for
/// free.
pub fn renew_deposit(
    source: &mut DepositSource,
    ground: &Tile,
    config: &ExtractionConfig,
    ladder: &LadderConfig,
) {
    let capacity = tile_deposit_capacity(config, &source.material, ground);
    let payoff = deposit_payoff(&source.standing, ladder);
    let rate = tile_deposit_regrowth(config, &source.material, ground) * payoff.regrowth_multiplier;
    source.stock = deposit_regrowth(source.stock, capacity, rate, config.seed_fraction);
}

// **RETIRED: `share_of_take` and the `DepositCrews` alias** — a per-band split of one summed take,
// and the sweep that would have summed the crews.
//
// **The sweep was never built and must not be.** Each band's row takes from the shared stock
// **sequentially**, drawing it down as it goes, which is exactly how `forage_take` divides a patch
// two bands gather: the total can never exceed what the rung could reach, because every take is
// capped by the stock it actually finds. A summed-crew sweep would be a second way to divide one
// number, and the one thing that genuinely had to be per-working — the **growth term** — is
// `advance_deposits`' now (see [`take_from_deposit`]).

/// **THE WORKINGS' DECAY AND THIS TURN'S BILL** — the deposit branches' `routes::advance_roads`,
/// and the half of the standing upkeep that makes neglect **self-limiting**.
///
/// # ⛔ WHY AN UNKEPT WORKING HAS TO SLIDE
///
/// Without it a working's position never falls, so **a quarry is free to hold for ever** — and an
/// improvement that costs nothing to hold cannot weigh on move-or-stay, which is the whole of
/// `docs/plan_standing_upkeep.md`. What the slide buys is that the penalty **shrinks itself**: the
/// position falls, the interpolated demand falls with it, and an abandoned working decays toward
/// costing nothing rather than bleeding a band's roster for ever (§2.7).
///
/// # The three phases, in the order `advance_roads` runs them
///
/// 1. **How short**, off the **stamped** bill — a working nobody billed reads
///    [`crate::intensification::FULLY_SUPPLIED`] and is forgiven — and the neglect run steps or
///    resets on that same reading, so there is no second dial free to disagree with the first.
/// 2. **The bleed**, at the at-risk rung's own rate and past that rung's own grace.
///    `upkeep_decay` owns the `>` that decides whether the penalty is biting.
/// 3. **Clear the payment and re-stamp the bill**, at the **post-decay** position, so the turn that
///    is about to run judges the working as this pass left it.
/// 4. **Renew the stock** ([`renew_deposit`]), at that same post-decay position.
///
/// # ⛔ PHASE 4 IS WHY THE GROWTH TERM IS HERE AND NOT IN THE TAKE
///
/// This pass sweeps **every working exactly once**; the take runs **once per band-row**. Renewal
/// inside the take therefore ran `K` times on a working `K` bands shared, and **never** on one no
/// band held a row on — an abandoned over-cut wood frozen at its low-water mark for ever, against
/// the arc's own headline. It is the plant web's arrangement now, for the plant web's reason.
///
/// # ⛔ IT RUNS ON EVERY WORKING, KEPT OR NOT
///
/// This is the load-bearing half, and it is `bill_and_stock_roads`' lesson: a pass that billed only
/// the workings some band still has a row on would leave an **abandoned** working reading as kept
/// for ever — never arming its counter, never decaying. A working whose band walked out of range is
/// exactly the case the branch's move-or-stay pressure is made of, so it is exactly the case that
/// must decay.
///
/// **It stamps rather than paying**, and the payment is a whole stage later
/// (`systems::settle_bands_extraction`, inside the labour pass, because the head count it divides is
/// the one the shedding order left). Logistics runs before Population, so the stamp this pass writes
/// is the bill that turn's keepers pay against, and the next Logistics pass judges that pair.
pub fn advance_deposits(
    mut registry: ResMut<DepositRegistry>,
    ladder: Res<crate::intensification::LadderConfigHandle>,
    extraction: Res<crate::extraction_config::ExtractionConfigHandle>,
    tile_registry: Res<crate::resources::TileRegistry>,
    tiles: bevy::prelude::Query<&Tile>,
) {
    let ladder = ladder.get();
    let config = extraction.get();
    for source in registry.sources.values_mut() {
        // A working whose tile has gone from the map keeps whatever it was — the same forgiveness
        // `advance_forage_regrowth` gives a synthetic off-map patch, and the only way a harness can
        // build one.
        let Some(ground) = tile_registry
            .index(source.tile.x, source.tile.y)
            .and_then(|entity| tiles.get(entity).ok())
        else {
            continue;
        };
        let measure = deposit_measure(source, ground, &config);
        let basis = deposit_keeping_basis(source, measure, &ladder);
        // ## 1 — how short, and the run of turns it has been short for.
        let shortfall_fraction =
            crate::intensification::upkeep_shortfall_fraction(basis, source.upkeep_supplied);
        if crate::intensification::upkeep_shortfall(basis, source.upkeep_supplied)
            > NO_UPKEEP_DEMAND
        {
            source.neglect_turns = source.neglect_turns.saturating_add(1);
        } else {
            source.neglect_turns = NEGLECT_NONE;
        }
        // ## 2 — the bleed.
        let branch = source.standing.held.branch();
        let at_risk = deposit_at_risk_rung(&source.standing);
        let decay = ladder
            .rung(at_risk)
            .upkeep_decay(shortfall_fraction, source.neglect_turns);
        if decay > crate::intensification::NO_UPKEEP_DECAY {
            let bled = source.ladder_position() - decay;
            source.set_ladder_position(bled, &ladder, branch);
        }
        // ## 3 — clear the payment and re-stamp, at the position the bleed left.
        source.upkeep_supplied = NO_UPKEEP_DEMAND;
        let measure = deposit_measure(source, ground, &config);
        source.upkeep_demanded = Some(deposit_upkeep_demand(source, measure, &ladder));
        // ## 4 — the renewal, once per working, at that same post-decay position.
        renew_deposit(source, ground, &config, &ladder);
    }
}

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

    /// **A SHARED WORKING IS DRAWN DOWN SEQUENTIALLY, AND THE TOTAL CANNOT EXCEED THE REACH.**
    ///
    /// This replaced `a_shared_working_splits_its_take_by_hands`, which asserted a per-band split of
    /// one summed take. That model was never built and must not be: each band's row takes from the
    /// stock it actually finds, drawing it down as it goes, which is how `forage_take` divides a
    /// patch two bands gather. The invariant that matters is not *"the shares sum to the take"* but
    /// *"the takes cannot sum past what the rung could reach"*, and that falls out of the
    /// arithmetic rather than out of a divider.
    #[test]
    fn the_takes_of_several_bands_cannot_sum_past_what_the_rung_can_reach() {
        let felling = payoff(2.0, WHOLE_DEPOSIT_REACHED, REGROWTH_UNCHANGED);
        let capacity = 30.0;
        let mut stock = capacity;
        let reach = deposit_reachable(stock, capacity, &felling);
        // Four bands of five, one after another: `4 x 5 x 2.0 = 40` against a reach of 30.
        let mut total = 0.0;
        for _ in 0..4 {
            let taken = deposit_take(5, stock, capacity, &felling);
            total += taken;
            stock = (stock - taken).max(DEPOSIT_EMPTY);
        }
        assert!(
            total <= reach + 1e-4,
            "four bands drawing sequentially took {total} of a reach of {reach}"
        );
        assert!(
            total > 0.0 && stock < capacity,
            "**LIVENESS**: they must actually have taken something"
        );
    }
}
