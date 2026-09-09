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
//!   floor     = max((1 − recovery_fraction(position)) × capacity, escapement × capacity)
//!   reachable = max(0, stock − floor)
//!   take      = min(workers × yield_per_worker_turn(position), reachable)
//!   stock    -= take
//! ```
//!
//! **The rung's floor and the crew's are ONE floor, taken as a MAXIMUM** ([`deposit_effective_floor`],
//! issue #650). Both are *an amount left standing* — the first is what this rung's reach cannot get
//! at, the second is what the player told the crew to leave — so you stop at whichever is higher.
//! Summing them, or clamping twice, would double-count on every rung.
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
        build_fraction, interpolate, neglect_grace_remaining, rung_span, rung_work_done, BuildGate,
        BuildTurns, LadderConfig, RungBranch, RungDef, RungExtractionPayoff, RungKey, RungStanding,
        NEGLECT_NONE, NO_CREW_ON_THIS_ACTIVITY, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT,
        RUNG_COST_UNSCALED, RUNG_UNSTARTED,
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

/// **NOBODY CUT THIS WORKING THIS TURN** — the reset value of [`DepositSource::last_take`], and the
/// reading that makes the runway *"there is no rate to project"* rather than a division by zero.
pub const NO_TAKE_THIS_TURN: f32 = 0.0;

/// **NO CREW NAMED A FLOOR ON THIS WORKING THIS TURN** — the reset value of
/// [`DepositSource::last_floor`], and its `opening` state. Named rather than written as a bare
/// `None` at three sites so the pair with [`NO_TAKE_THIS_TURN`] is visible: the two clear together,
/// because each is a statement about the turn just resolved.
pub const NOBODY_ASKED_FOR_A_FLOOR: Option<f32> = None;

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
    /// **WHAT EVERY CREW TOOK OUT OF THIS WORKING THIS TURN**, in the material's own units.
    ///
    /// Accumulates (`+=`) across the bands cutting it — [`Self::upkeep_supplied`]'s rule, and for
    /// its reason: a shared working is drawn down **sequentially**, so the row-by-row takes are the
    /// only place the total exists — and is cleared once per turn by [`advance_deposits`].
    ///
    /// ⛔ **IT IS THE RATE THE RUNWAY IS PROJECTED FORWARD ON, NEVER A TRAILING AVERAGE.** The
    /// arrivals rule: `turns_remaining` is *this turn's* take carried forward, so it moves the turn
    /// the crew does. A working nobody cut reads [`NO_TAKE_THIS_TURN`], which is the honest *"there
    /// is no rate to project"* rather than a zero the projection would divide by.
    #[serde(default)]
    pub last_take: f32,
    /// **THE DEEPEST ESCAPEMENT FLOOR ANY CREW WORKED THIS WORKING TO**, as a fraction of capacity —
    /// [`Self::last_take`]'s twin, cleared once per turn by [`advance_deposits`] beside it.
    ///
    /// ⛔ **THE MINIMUM ACROSS THE BANDS CUTTING IT, WHERE THE TAKE IS THE SUM.** A floor is not an
    /// amount to add up: two bands on one wood each stop at their own, so the stock comes to rest at
    /// the **lowest** of them and that is the one every published reading is about. Summing floors
    /// would be meaningless and taking the last writer's would make the row depend on band order.
    ///
    /// `None` is *"nobody cut this working this turn"* — [`NO_TAKE_THIS_TURN`]'s reading one field
    /// over — and [`Self::escapement_floor`] answers it as [`crate::components::STRIP_IT_BARE`],
    /// which is the **identity** of the `max` in [`deposit_effective_floor`]: an unworked deposit
    /// therefore publishes exactly the rung's own reach, as it did before this field existed.
    #[serde(default)]
    pub last_floor: Option<f32>,
    /// **WHY THE POOL IS STUCK ON THIS WORKING** — [`BuildGate::Open`] when it is not stuck, which
    /// is also what a working nobody has queued reads. `routes::Road::build_blocked_reason`'s twin,
    /// stamped by the labour pass's `Extract` arm where the quote is struck and **cleared at the top
    /// of every turn** by [`advance_deposits`], so a cause is a statement about *this* turn.
    #[serde(default)]
    pub build_blocked_reason: BuildGate,
    /// **HOW MANY TURNS UNTIL THIS WORKING REACHES WHERE ITS ENTRY IS SENDING IT** — the chained
    /// countdown, and the exact twin of `routes::Road::build_turns_remaining`.
    ///
    /// ⛔ **IT CAN ONLY BE STAMPED BY THE CHAIN PASS**, because it is a fact about the **queue**: an
    /// entry is dated as everything above it plus its own span, which no per-source seam can see. A
    /// working nobody has queued keeps `None` — the honest *no estimate*, never a `0` that would
    /// render as a finished build.
    #[serde(default)]
    pub build_turns_remaining: Option<BuildTurns>,
    /// **THIS WORKING'S 0-BASED PLACE IN ITS BAND'S BUILD QUEUE**, or
    /// [`crate::intensification::NOT_IN_ANY_BUILD_QUEUE`] when no pass has placed it.
    ///
    /// **Scratch, not published**: its one reader is the countdown's *"has an estimate pass ever run
    /// for this entry"* test, which is what separates a build queued a second ago from one that is
    /// genuinely stalled — both sit at `0%`. Cleared every turn with the pair above, which is what
    /// makes *"live-queued and still cleared"* mean *"queued since the last pass"*.
    #[serde(default = "not_in_any_build_queue")]
    pub build_queue_position: i32,
}

/// The serde default of [`DepositSource::build_queue_position`] — *"no pass has placed this
/// working"*, which is a different fact from *"it is at the head"* that a derived `0` would give.
fn not_in_any_build_queue() -> i32 {
    crate::intensification::NOT_IN_ANY_BUILD_QUEUE
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
            last_take: NO_TAKE_THIS_TURN,
            last_floor: NOBODY_ASKED_FOR_A_FLOOR,
            build_blocked_reason: BuildGate::Open,
            build_turns_remaining: None,
            build_queue_position: crate::intensification::NOT_IN_ANY_BUILD_QUEUE,
        }
    }

    /// The work banked into this working, in cumulative units.
    pub fn ladder_position(&self) -> f32 {
        self.ladder_position
    }

    /// **THE ESCAPEMENT FLOOR EVERY PUBLISHED READING OF THIS WORKING IS TAKEN AT** — what this
    /// turn's crews left standing ([`DepositSource::last_floor`]), or
    /// [`crate::components::STRIP_IT_BARE`] where nobody cut it.
    ///
    /// The fallback is the **identity** of [`deposit_effective_floor`]'s `max`, not a policy: a
    /// working no band is on is described exactly by its rung's own reach, which is what its row
    /// said before a player floor existed.
    pub fn escapement_floor(&self) -> f32 {
        self.last_floor.unwrap_or(crate::components::STRIP_IT_BARE)
    }

    /// **RECORD THE FLOOR THIS TURN'S CREW WORKED TO**, keeping the deepest — see
    /// [`DepositSource::last_floor`] for why the aggregate is a minimum where the take's is a sum.
    fn note_escapement_floor(&mut self, escapement: f32) {
        self.last_floor = Some(match self.last_floor {
            Some(deepest) => deepest.min(escapement),
            None => escapement,
        });
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

/// **THE TWO LADDERS A DEPOSIT IS WORKED BY, IN THE ORDER A READER MEETS THEM** — wood first, then
/// stone and, later, every metal.
///
/// It is named rather than written as a `matches!` at each call site because the *order* is part of
/// what [`deposit_rungs_in_climb_order`] publishes: a catalog whose branches came out of a `HashSet`
/// walk would reorder itself between runs, and a reader grouping the rows would find them
/// interleaved differently every world. The day the minerals arc adds a third deposit ladder, this
/// is the one list that has to grow — and every sweep over it follows without moving.
pub const DEPOSIT_BRANCHES: [RungBranch; 2] = [RungBranch::Forestry, RungBranch::Extraction];

/// **EVERY RUNG THE TWO DEPOSIT BRANCHES DECLARE, GROUPED BY BRANCH AND CLIMBING WITHIN IT** — the
/// branches' catalog, read straight off the config records. `routes::route_rungs_in_climb_order` is
/// the precedent, one branch wider.
///
/// ⛔ **IT WALKS `ladder.rungs` AND NOT [`RungKey::ALL`], DELIBERATELY.** The key enum names the
/// rungs a *system* reasons about; this answers *what does the config hold*, so a rung added to
/// `intensification_ladder.json` is in the catalog — and therefore on the wire and in the client's
/// ladder — with no code edit. That is not hypothetical here: the minerals arc's `mine` is already
/// reserved above `extraction:quarry` on this same branch.
///
/// Sorted by the record's own `order` within each branch, which the ladder validates as a dense
/// climb from `1`; the branches themselves come in [`DEPOSIT_BRANCHES`] order.
pub fn deposit_rungs_in_climb_order(ladder: &LadderConfig) -> Vec<&RungDef> {
    let mut rungs: Vec<&RungDef> = Vec::new();
    for branch in DEPOSIT_BRANCHES {
        let mut on_this_branch: Vec<&RungDef> = ladder
            .rungs
            .iter()
            .filter(|rung| rung.branch == branch)
            .collect();
        on_this_branch.sort_by_key(|rung| rung.order);
        rungs.append(&mut on_this_branch);
    }
    rungs
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

/// **DOES THIS GROUND HOLD ANY DEPOSIT AT ALL** — the pre-filter the snapshot's tile sweep picks
/// the deposit-bearing ground out with, so the wire pass walks the tiles that can carry a row
/// rather than the whole map twice over.
///
/// It reads through [`tile_deposit_capacity`], so the set it selects and the rows
/// `snapshot::deposits::deposit_states` builds off that set cannot disagree about which ground
/// holds something: `false` here is exactly *"every material answered [`NO_DEPOSIT`]"*.
pub fn tile_holds_a_deposit(config: &ExtractionConfig, tile: &Tile) -> bool {
    config
        .deposits()
        .any(|(material, _)| tile_deposit_capacity(config, material, tile) > NO_DEPOSIT)
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

/// **THE RUNG'S OWN FLOOR AS A FRACTION OF CAPACITY** — [`deposit_floor`] divided by the `capacity`
/// it is a fraction of, which is `1 − recovery_fraction` and needs no `capacity` to say so.
///
/// It exists because the **player's** floor ([`crate::components::LaborTarget::Extract::floor`]) is
/// stated in exactly these units, so the two are comparable numbers rather than two readings that
/// have to be converted before they can be composed. It is what the wire publishes for a client
/// drawing the escapement chart, whose whole x-axis is fractions of `K`.
pub fn deposit_floor_fraction(payoff: &RungExtractionPayoff) -> f32 {
    (1.0 - payoff.recovery_fraction).clamp(DEPOSIT_EMPTY, WHOLE_DEPOSIT_STANDING)
}

/// **A FLOOR OF THE WHOLE DEPOSIT** — [`deposit_floor_fraction`]'s ceiling, and what a rung that
/// recovers nothing leaves standing. Named because a bare `1.0` in a clamp reads as a normalisation
/// constant rather than as *"the crew may take none of it"*.
const WHOLE_DEPOSIT_STANDING: f32 = 1.0;

/// **THE ONE FLOOR A CREW ACTUALLY STOPS AT** — the greater of what the **rung** cannot reach and
/// what the **player** told the crew to leave standing (`docs/plan_harvest_floor.md` §1, issue #650).
///
/// ⛔ **THE TWO FLOORS COMPOSE AS A MAXIMUM — NEVER AS TWO CLAMPS, AND NEVER AS A SUM.** They are
/// the same kind of quantity: *an amount left standing*. [`deposit_floor`] is the remainder this
/// rung's reach cannot get at (gathering recovers `0.15`, so it strands 85% of a seam);
/// `escapement × capacity` is the remainder the player asked for. You leave whichever is greater, so
/// a player floor **below** the rung's changes nothing and one **above** it binds. Adding them would
/// double-count on every rung — a gathering crew told to leave half a seam would be refused 135% of
/// it — and clamping twice is the same arithmetic written out longer.
///
/// `escapement` is a fraction of `capacity` (`components::floor_is_valid`'s `0.0..=1.0`, enforced at
/// the command boundary), which is why it is multiplied by the capacity here rather than compared
/// against a stock.
pub fn deposit_effective_floor(
    capacity: f32,
    payoff: &RungExtractionPayoff,
    escapement: f32,
) -> f32 {
    deposit_floor(capacity, payoff).max((escapement * capacity).max(DEPOSIT_EMPTY))
}

/// **WHAT IS LEFT ABOVE THE FLOOR FOR THIS CREW TO TAKE** — the deposit's twin of
/// `forage::patch_take_room`, and what the take is capped by.
///
/// ⛔ **IT IS THE SINGLE PLACE THE TWO FLOORS ARE COMPOSED** ([`deposit_effective_floor`]), so the
/// take, the runway, the row's `reachable` and the lesson's work predicate all move together. A
/// consumer that subtracted a floor of its own would be a second answer to one question.
pub fn deposit_reachable(
    stock: f32,
    capacity: f32,
    payoff: &RungExtractionPayoff,
    escapement: f32,
) -> f32 {
    (stock - deposit_effective_floor(capacity, payoff, escapement)).max(DEPOSIT_EMPTY)
}

/// **WHAT A CREW OF `workers` TAKES THIS TURN** — `min(what the hands can lift, what the crew is
/// allowed to reach)`, the second term being [`deposit_reachable`] at the composed floor.
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
pub fn deposit_take(
    workers: u32,
    stock: f32,
    capacity: f32,
    payoff: &RungExtractionPayoff,
    escapement: f32,
) -> f32 {
    let labor = workers as f32 * payoff.yield_per_worker_turn;
    labor
        .max(DEPOSIT_EMPTY)
        .min(deposit_reachable(stock, capacity, payoff, escapement))
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

/// **HOW MANY MORE TURNS OF SHORTFALL THIS WORKING CAN ABSORB BEFORE IT SLIDES** — the countdown,
/// not the counter, through [`neglect_grace_remaining`] so all four branches and the wire mean one
/// thing by a grace.
///
/// **`None` = THERE IS NOTHING AT RISK HERE**, which is a working on either free floor: neither
/// `forestry:deadfall` nor `extraction:gathering` declares an `upkeep`, so there is no meter to
/// lose. `routes::road_neglect_grace_remaining`'s twin, and the field the wire's `hasNeglectGrace`
/// is read off — a client must check the bool first, because the number reuses the *"biting now"*
/// `0` rather than inventing a sentinel.
pub fn deposit_neglect_grace_remaining(
    source: &DepositSource,
    ladder: &LadderConfig,
) -> Option<u32> {
    let rung = ladder.rung(deposit_at_risk_rung(&source.standing));
    rung.upkeep.as_ref()?;
    Some(neglect_grace_remaining(
        source.neglect_turns,
        rung.upkeep_grace_turns(),
    ))
}

/// **HOW MANY WHOLE `quarrywork` KEEPERS THIS WORKING'S BILL WANTS** — `ceil(basis /
/// PER_WORKER_OUTPUT)`, the deposit twin of `routes::road_upkeep_workers_needed`.
///
/// It reads the **stamped** basis, exactly as the published demand beside it does, so *"wants 2, you
/// have 0"* and the shortfall on the same row describe one bill.
pub fn deposit_upkeep_workers_needed(
    source: &DepositSource,
    measure: f32,
    ladder: &LadderConfig,
) -> u32 {
    let demand = deposit_keeping_basis(source, measure, ladder);
    if demand <= NO_UPKEEP_DEMAND {
        return NO_CREW_ON_THIS_ACTIVITY;
    }
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

/// **THE METER ON THE RUNG THIS WORKING IS ACTUALLY RAISING**, `0..=1` — the deposit branches' twin
/// of `road_build_fraction` / `cultivationProgress`, and what the wire's `buildFraction` publishes.
///
/// ⛔ **IT GOES THROUGH [`rung_work_done`], NEVER THROUGH A SUBTRACTION.** That seam answers a rung
/// the standing already holds with the rung's full `width` by construction rather than with
/// `fl(base + width) − base`, which is the rounding that published a completed Field at *"99%"*.
pub fn deposit_build_fraction(source: &DepositSource, ladder: &LadderConfig) -> f32 {
    let standing = source.standing();
    let at_risk = deposit_at_risk_rung(standing);
    let span = deposit_rung_span(at_risk, ladder);
    build_fraction(
        rung_work_done(*standing, at_risk, source.ladder_position(), span),
        span.1,
    )
}

/// **WHAT A CREW COULD TAKE EVERY TURN FOR EVER AND STILL HAVE A WOOD** — the deposit reading of
/// `sustainable_yield` (`docs/plan_intensification.md`), and half of the over-cut pair on the wire.
///
/// ⛔ **IT IS THE MSY READING OF THE GROWTH TERM, NOT THE GROWTH AT TODAY'S STOCK** — the curve is
/// evaluated at `min(stock, MSY_BIOMASS_FRACTION × capacity)`, which is
/// [`crate::fauna::sustainable_yield`]'s own expression with the deposit's curve substituted for the
/// food web's. **The instantaneous reading is not a sustainable rate, it is the rate at one point**,
/// and taking it literally made the ⚠ fire on correct play and never clear (issue #650): a full
/// stand has `(1 − S/K) = 0`, so *any* take out-cut it, and the stock then converges on the
/// stock where growth equals the take from above — an asymptote, so `actual > sustainable` stayed
/// true for ever. A wood at 600 wood and `r = 0.03` sustains `r·K/4 = 4.5` a turn; one cutter takes
/// `0.3`. The honest answer is that this is fifteen times inside the wood's means.
///
/// ⛔ **A QUARRY STILL READS ZERO, AND STILL BY ARITHMETIC.** Rock's rate is [`NEVER_RENEWS`], so
/// [`deposit_regrowth`] returns its argument unchanged at *any* reading point and the difference is
/// exactly `0` — *stone sustains no take* needs no finite branch here either. What a finite working
/// publishes instead is [`deposit_runway`].
///
/// It reads through [`renew_deposit`]'s own terms — the ground's rate scaled by what the rung
/// bought, at the seeded reading — so the only thing separating it from the growth the next
/// Logistics pass applies is *where on the curve it is taken*, which is the whole of the MSY idea.
pub fn deposit_sustainable_take(
    source: &DepositSource,
    ground: &Tile,
    config: &ExtractionConfig,
    ladder: &LadderConfig,
) -> f32 {
    let capacity = tile_deposit_capacity(config, &source.material, ground);
    let payoff = deposit_payoff(&source.standing, ladder);
    let rate = tile_deposit_regrowth(config, &source.material, ground) * payoff.regrowth_multiplier;
    let at_the_peak = source
        .stock
        .min(crate::fauna::MSY_BIOMASS_FRACTION * capacity);
    (deposit_regrowth(at_the_peak, capacity, rate, config.seed_fraction) - at_the_peak)
        .max(DEPOSIT_EMPTY)
}

/// **HOW MANY TURNS THIS WORKING LASTS AT THE CURRENT TAKE** — `floor(reachable / take)`, and the
/// other half of the §7 fork.
///
/// ⛔ **A FORWARD PROJECTION, NEVER A TRAILING AVERAGE AND NEVER AN EMA** (the food-arrivals rule):
/// the numerator is what this rung can reach *now* and the denominator is what the crews took *this*
/// turn, so the answer moves the turn the crew does rather than lagging it.
///
/// ⛔ **WHICH READOUT A WORKING GETS IS DECIDED BY THE RATE, NEVER BY THE BRANCH**
/// (`docs/plan_extraction.md` §7). A renewing deposit does not run out, so it answers
/// [`sim_schema::DEPOSIT_RUNWAY_NOT_APPLICABLE`] and its warning is the over-cut pair instead — and
/// a flint scatter and a quarry are both `extraction` and land on opposite sides of this test. A
/// finite working nobody is cutting answers [`sim_schema::DEPOSIT_RUNWAY_NO_TAKE`]: it *will* run
/// out, just not while it stands idle.
pub fn deposit_runway(
    source: &DepositSource,
    ground: &Tile,
    config: &ExtractionConfig,
    ladder: &LadderConfig,
) -> i32 {
    if tile_deposit_regrowth(config, &source.material, ground) > NEVER_RENEWS {
        return sim_schema::DEPOSIT_RUNWAY_NOT_APPLICABLE;
    }
    if source.last_take <= NO_TAKE_THIS_TURN {
        return sim_schema::DEPOSIT_RUNWAY_NO_TAKE;
    }
    let capacity = tile_deposit_capacity(config, &source.material, ground);
    let payoff = deposit_payoff(&source.standing, ladder);
    // **The runway picks the crew's floor up through `deposit_reachable`, not beside it** — the
    // numerator is what the crews cutting this working can still get at, so raising the floor
    // shortens the runway by exactly the stock the player asked to leave standing.
    let reachable = deposit_reachable(source.stock, capacity, &payoff, source.escapement_floor());
    (reachable / source.last_take).floor() as i32
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
    escapement: f32,
    ground: &Tile,
    config: &ExtractionConfig,
    ladder: &LadderConfig,
) -> DepositTake {
    let capacity = tile_deposit_capacity(config, &source.material, ground);
    let payoff = deposit_payoff(&source.standing, ladder);
    let reachable_before = deposit_reachable(source.stock, capacity, &payoff, escapement);
    let taken = deposit_take(workers, source.stock, capacity, &payoff, escapement);
    source.stock = (source.stock - taken).max(DEPOSIT_EMPTY);
    // **The floor this row worked to, kept at the deepest across the bands cutting this working** —
    // stamped here rather than by the caller so the take and the reading every readout is composed
    // at can never come from different floors.
    source.note_escapement_floor(escapement);
    // **The turn's take, accumulated across the bands cutting this working** — the wire's
    // `actualTake` and the denominator of its runway. `+=` for `upkeep_supplied`'s reason: a shared
    // working is drawn down sequentially, so this is the only place the total exists.
    source.last_take += taken;
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
        // ## 3 — clear the payment and re-stamp, at the position the bleed left. **This turn's
        // take, the blocked cause and the countdown clear with it**, and for the same reason: each
        // is a statement about the turn just resolved, and a stale one would leave an idle working
        // still quoting a runway off a crew that has gone. The countdown and its place in the line
        // clear **together**, which is what makes *"live-queued and still cleared"* mean *"queued
        // since the last pass"*.
        source.upkeep_supplied = NO_UPKEEP_DEMAND;
        source.last_take = NO_TAKE_THIS_TURN;
        source.last_floor = NOBODY_ASKED_FOR_A_FLOOR;
        source.build_blocked_reason = BuildGate::Open;
        source.build_turns_remaining = None;
        source.build_queue_position = crate::intensification::NOT_IN_ANY_BUILD_QUEUE;
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

    /// **NOBODY TOLD THE CREW TO LEAVE ANYTHING** — the identity of the composed floor's `max`, so a
    /// test about the RUNG's floor alone reads the same arithmetic it read before a player floor
    /// existed. Named rather than written `0.0`, because a bare zero in that argument reads as
    /// *"strip it"* rather than as *"this test is not about the dial"*.
    const NO_CREW_FLOOR: f32 = crate::components::STRIP_IT_BARE;

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
            deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &deep, NO_CREW_FLOOR)
                > deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow, NO_CREW_FLOOR)
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
        let reach = deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow, NO_CREW_FLOOR);
        assert!(
            1000.0 * shallow.yield_per_worker_turn > reach,
            "fixture: the crew must outrun the reach, or this asserts nothing"
        );
        assert_eq!(
            deposit_take(1000, A_ROCK_BODY, A_ROCK_BODY, &shallow, NO_CREW_FLOOR),
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
            let taken = deposit_take(10, stock, capacity, &felling, NO_CREW_FLOOR);
            let after = (stock - taken).max(DEPOSIT_EMPTY);
            stock = deposit_regrowth(after, capacity, 0.03 * felling.regrowth_multiplier, A_SEED);
            assert!(stock < previous, "a wood being over-cut must fall");
            previous = stock;
        }
    }

    /// ⛔ **THE RUNG'S FLOOR AND THE CREW'S COMPOSE AS A MAXIMUM — NEVER AS A SUM, NEVER AS TWO
    /// CLAMPS** (issue #650). The one line a future reader is most likely to re-derive wrongly, so
    /// it is asserted from both sides and against the sum.
    ///
    /// `shallow` recovers 0.15, so its own floor strands 85% of the body: a crew told to leave
    /// *half* changes nothing there, and a crew told to leave nearly all of it binds. `deep`
    /// recovers 0.9 and strands a tenth, so the same half-the-body order binds immediately. One
    /// payoff would have shown only one of those.
    #[test]
    fn the_rungs_floor_and_the_crews_compose_as_a_maximum() {
        let shallow = payoff(0.4, 0.15, REGROWTH_UNCHANGED);
        let deep = payoff(2.2, 0.9, REGROWTH_UNCHANGED);

        // **Below the rung's own floor the dial does nothing** — `max(0.85, 0.5) == 0.85`.
        assert_eq!(
            deposit_effective_floor(A_ROCK_BODY, &shallow, HALF_THE_BODY),
            deposit_floor(A_ROCK_BODY, &shallow),
            "a crew asked to leave less than the rung already cannot reach leaves the rung's floor"
        );
        assert_eq!(
            deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow, HALF_THE_BODY),
            deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &shallow, NO_CREW_FLOOR),
            "…so the reach is the same number it was before anyone touched the dial"
        );

        // **Above it the crew's binds** — on a rung that reaches nearly the whole body.
        let unbid = deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &deep, NO_CREW_FLOOR);
        let bid = deposit_reachable(A_ROCK_BODY, A_ROCK_BODY, &deep, HALF_THE_BODY);
        assert!(
            bid < unbid,
            "a crew asked to leave half a body it could nearly all reach must reach less: \
             {bid} against {unbid}"
        );
        assert!(
            bid > DEPOSIT_EMPTY,
            "**LIVENESS**: and it must still reach SOMETHING, or the ordering above would hold \
             for a floor that simply broke the take"
        );

        // ⛔ **AND IT IS NOT A SUM.** `0.85 + 0.5` of a body is 135% of it, which would strand the
        // whole seam on the shallow rung and read as a working nobody can cut.
        assert!(
            deposit_effective_floor(A_ROCK_BODY, &shallow, HALF_THE_BODY)
                < deposit_floor(A_ROCK_BODY, &shallow) + HALF_THE_BODY * A_ROCK_BODY,
            "the two floors must not add"
        );
    }

    /// **Half the body left standing** — a floor below `shallow`'s own (0.85) and above `deep`'s
    /// (0.10), which is what lets one number assert both directions of the maximum.
    const HALF_THE_BODY: f32 = 0.5;

    /// **A CREW AT A FLOOR DRAWS DOWN TO IT AND THEN TAKES ONLY WHAT GROWS BACK** — the behaviour
    /// the whole feature exists for, walked far enough that the settling is a fact rather than a
    /// first-turn coincidence.
    ///
    /// The crew is stated well past the crossing (`10 × 2.0 = 20` a turn against a room that runs
    /// out in fifteen), so the assertion is about the **floor** and not about a rate a retune would
    /// move.
    #[test]
    fn a_crew_at_a_floor_settles_on_it_and_then_takes_the_regrowth() {
        const CAPACITY: f32 = 600.0;
        const RATE: f32 = 0.03;
        const CREW: u32 = 10;
        let felling = payoff(2.0, WHOLE_DEPOSIT_REACHED, REGROWTH_UNCHANGED);
        let floor_stock = HALF_THE_BODY * CAPACITY;

        let mut stock = CAPACITY;
        let mut last_take = 0.0;
        for turn in 0..60 {
            stock = deposit_regrowth(stock, CAPACITY, RATE, A_SEED);
            let taken = deposit_take(CREW, stock, CAPACITY, &felling, HALF_THE_BODY);
            stock = (stock - taken).max(DEPOSIT_EMPTY);
            assert!(
                stock >= floor_stock - A_ROUNDING,
                "turn {turn}: the crew must never draw below the floor it was given: {stock}"
            );
            last_take = taken;
        }
        assert!(
            (stock - floor_stock).abs() < A_ROUNDING,
            "the stand settles ON the floor rather than above it: {stock} against {floor_stock}"
        );
        // At rest the take IS the regrowth at the floor — `r·fK·(1 − f)`, the sustained yield §2 of
        // `docs/plan_harvest_floor.md` derives, written out rather than borrowed from the code.
        let sustained = RATE * floor_stock * (1.0 - HALF_THE_BODY);
        assert!(
            (last_take - sustained).abs() < A_ROUNDING,
            "…and thereafter it takes exactly what grew back: {last_take} against {sustained}"
        );
        assert!(
            last_take > DEPOSIT_EMPTY,
            "**LIVENESS**: which is a positive number, not a crew that stopped cutting"
        );
    }

    /// These are single-precision products of three config numbers walked sixty times over, so an
    /// exact `==` would be a statement about float layout rather than about the curve.
    const A_ROUNDING: f32 = 1e-2;

    /// **A rung that reaches nothing takes nothing** — the floor's degenerate end, stated so the
    /// arithmetic is pinned at both ends of the interval the config is bounded to.
    #[test]
    fn a_rung_that_reaches_none_of_the_deposit_takes_none_of_it() {
        let none = payoff(5.0, NO_DEPOSIT_REACHED, REGROWTH_UNCHANGED);
        assert_eq!(deposit_floor(A_ROCK_BODY, &none), A_ROCK_BODY);
        assert_eq!(
            deposit_take(50, A_ROCK_BODY, A_ROCK_BODY, &none, NO_CREW_FLOOR),
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
        let reach = deposit_reachable(stock, capacity, &felling, NO_CREW_FLOOR);
        // Four bands of five, one after another: `4 x 5 x 2.0 = 40` against a reach of 30.
        let mut total = 0.0;
        for _ in 0..4 {
            let taken = deposit_take(5, stock, capacity, &felling, NO_CREW_FLOOR);
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
