//! `deposits` — the live WORKINGS on the ground (arc #583, `docs/plan_extraction.md` §7,
//! `.claude/rules/core_sim/extraction.md`).
//!
//! ONE ROW PER DEPOSIT-BEARING TILE, keyed by **(tile, material)**. A working is a per-TILE improvement
//! like a road, but it belongs to a CAMP like a patch: a road follows nobody and is free to leave,
//! and a quarry you walk away from is a quarry you lost. `DepositState` is modelled field-for-field
//! on `RouteState` and the two share their standing-bill, neglect and build blocks verbatim — read
//! `dict::routes` beside this file rather than inventing a second reading of the same quad.
//!
//! ⛔ **A ROW DESCRIBES THE GROUND; THE WORKING IS ITS STATE (issue #650).** The section publishes a
//! row for every DISCOVERED tile that holds a deposit — `snapshot_forage_patches`' shape — and merges
//! the registry's live working in where a band has opened one; where none has, the sim derives the
//! opening state at capture. So **most rows on the map are untouched ground**, and a consumer that
//! read a row's presence as *somebody is working this* renders every seam on the map as a working
//! whose bill is met. `HudDepositVocab.is_unopened` is the client's reading of that difference, and
//! it is a FINGERPRINT of the opening state rather than a flag: the wire carries no
//! *has-a-band-opened-this* bool. Publishing only the registry is what made the feature unreachable —
//! the tile card's affordance is built off these rows, so a fresh world offered no way to open the
//! first working anywhere.
//!
//! ⛔ **ONE TILE CAN HOLD TWO WORKINGS.** A wooded highland holds timber and rock, and working one
//! is not working the other, so `tile_x`/`tile_y` alone is NOT a key: a consumer joining a crew to
//! its working must carry `material` too (`LaborAssignment.material`, `labor_assignments[*].
//! material` on the cohort dict).
//!
//! ⛔ **WHICH READOUT A WORKING PUBLISHES IS DECIDED BY `regrowth_rate > 0`, NEVER BY `branch`.** A
//! flint scatter and a quarry are both `extraction` and read differently — the same skill, the same
//! ladder, and only one of them runs out. `> 0` → the OVER-CUT pair (`sustainable_take` against
//! `actual_take`); `== 0` → the RUNWAY (`turns_remaining`). Forking on the branch string paints a
//! renewing scatter with a runway that never moves.
//!
//! Already fog-filtered SIM-SIDE, on the ROAD's gate rather than the herd list's: `Discovered` or
//! `Active`, because a working does not wander off and remembering one is remembering something
//! true. **The band's own bill is NOT a sum of these rows** — `PopulationCohortState`'s
//! `quarrywork_demand` / `quarrywork_supplied` / `quarrywork_shortfall` carry it, precisely because
//! these rows are fog-filtered and a working out of sight would drop out of a client-side total the
//! band still owes.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

pub(crate) fn deposits_to_array(
    list: Vector<'_, ForwardsUOffset<fb::DepositState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for deposit in list {
        let mut dict = VarDictionary::new();
        // **THE ROW'S IDENTITY IS THE TILE *AND* THE MATERIAL** — see the module header for why the
        // pair is indivisible. A consumer joins and diffs rows on all three.
        let _ = dict.insert("tile_x", deposit.tileX() as i64);
        let _ = dict.insert("tile_y", deposit.tileY() as i64);
        let _ = dict.insert("material", deposit.material().unwrap_or_default());
        // WHICH LADDER works this deposit — `"forestry"` | `"extraction"`, `RungBranch`'s wire form.
        //
        // ⛔ **IT DOES NOT DECIDE THE READOUT.** `regrowth_rate` below does, and the module header
        // says why: a flint scatter and a quarry are both `extraction` and read differently.
        // `branch` answers *which knowledge track* and nothing else.
        let _ = dict.insert("branch", deposit.branch().unwrap_or_default());
        // WHAT IS STANDING HERE RIGHT NOW, in the material's own units — the only SAVED thing about
        // a working. It opens at capacity, is drawn down by the take and put back by the growth
        // term.
        let _ = dict.insert("stock", f64::from(deposit.stock()));
        // What this GROUND holds when full, read live off the tile at capture rather than stored on
        // the source. **NO RUNG MAY RAISE IT** — it is the terrain's, which is what keeps the floor
        // below from climbing out from under a build.
        let _ = dict.insert("capacity", f64::from(deposit.capacity()));
        // What the CURRENT rung can actually get at — capacity minus the rung's own floor, clamped
        // to the stock, and the numerator of `turns_remaining`.
        //
        // ⛔ **IT IS NEVER SIMPLY `stock`.** A rung that cannot reach the whole seam leaves stock it
        // cannot take, and climbing the ladder is precisely how you reach deeper. A readout that
        // drew `stock` as *what you can have* promises the player rock the crew cannot cut.
        let _ = dict.insert("reachable", f64::from(deposit.reachable()));
        // ⛔ **THE FIELD THAT DECIDES WHICH READOUT IS DRAWN** (`docs/plan_extraction.md` §7):
        // `> 0` → the OVER-CUT warning (`sustainable_take` against `actual_take`); `== 0` → the
        // RUNWAY (`turns_remaining`). **NOT `branch`** — the module header carries the whole rule.
        // It is the GROUND'S own renewal rate, already scaled by what this working's rung bought.
        let _ = dict.insert("regrowth_rate", f64::from(deposit.regrowthRate()));
        // **THE RUNG STRING IS THE ANSWER — never threshold `build_fraction` to infer one.** That
        // meter belongs to the rung being RAISED, which is a different rung; `routes`' `rung`
        // carries the identical rule. `"<branch>:<id>"`: `"forestry:deadfall"`,
        // `"forestry:felling"`, `"forestry:coppice"`, `"extraction:gathering"`,
        // `"extraction:quarry"`.
        let _ = dict.insert("rung", deposit.rung().unwrap_or_default());
        // The meter on the rung being raised, 0..1, off the shared `intensification::build_fraction`
        // seam both food webs and the road publish theirs from. **NEVER DERIVED BY SUBTRACTION**
        // sim-side, so a working that has just completed a rung reads exactly `1.0` — and so does
        // one at the top of its ladder, with nothing left to raise. Draw a full bar, not an empty
        // one.
        let _ = dict.insert("build_fraction", f64::from(deposit.buildFraction()));
        // How far up its branch this working has been raised, in CUMULATIVE work units — the one
        // meter both deposit branches carry, and the absolute position `rung` and `build_fraction`
        // are both read out of. Published so a ladder card can place the working on the WHOLE
        // branch rather than only within its current rung.
        let _ = dict.insert("ladder_position", f64::from(deposit.ladderPosition()));
        // **THE OVER-CUT PAIR** — what a crew could take EVERY TURN AT THIS STOCK and leave the
        // working where it stands, against what it actually paid out last turn summed over every
        // band cutting it. This is the EXISTING intensification income breakdown pointed at a new
        // source, not a new readout: actual above sustainable is over-cutting, which is possible on
        // purpose and warned about rather than refused.
        //
        // **`sustainable_take` IS `0` ON A FINITE DEPOSIT, and that is the honest answer rather than
        // a gap.** Rock's rate is zero, so there is no take a quarry can sustain — what a finite
        // working publishes instead is the runway below, which is why `regrowth_rate` and not this
        // field is the fork.
        let _ = dict.insert("sustainable_take", f64::from(deposit.sustainableTake()));
        let _ = dict.insert("actual_take", f64::from(deposit.actualTake()));
        // **THE RUNWAY** — how many turns this working lasts at the current take,
        // `floor(reachable / actual_take)`. **A FORWARD PROJECTION, never a trailing average and
        // never an EMA**: it is this turn's rate carried forward, so it moves the turn the crew
        // does.
        //
        // TWO NEGATIVES, TWO DIFFERENT SENTENCES, and they are passed through VERBATIM so GDScript
        // reads the sim's own answer rather than deriving a second opinion
        // (`sim_schema::{DEPOSIT_RUNWAY_NOT_APPLICABLE, DEPOSIT_RUNWAY_NO_TAKE}`):
        //   `>= 0` a real count of turns;
        //   `-1`   NOT APPLICABLE — this deposit RENEWS, so it does not run out and the over-cut
        //          pair above is its warning instead. It is NOT "unknown": render the other readout
        //          here, never an empty runway;
        //   `-2`   NO TAKE THIS TURN — nobody is cutting it, so there is no rate to carry forward.
        //          It is NOT `-1`: this working WILL run out, just not while it is idle.
        // Flattening the two into one "no runway" is the defect the split exists to prevent.
        let _ = dict.insert("turns_remaining", deposit.turnsRemaining() as i64);
        // **THE STANDING BILL — the patch / herd / road quad, verbatim**, drawn from the band's
        // `quarrywork` pool. **`demand - supplied == shortfall` HOLDS ON THE WIRE**, all three
        // reading the sim's STAMPED basis at the post-decay position, so nothing here is re-derived
        // by subtraction — the build arm moves the ladder position inside the same turn, and a bill
        // struck on one side of the accrual against a payment on the other are two readings of two
        // different workings.
        //
        // **`0` ON BOTH FREE FLOORS** (`forestry:deadfall`, `extraction:gathering`), which declare
        // no upkeep at all: nobody built them, so there is nothing to hold, and that is the whole of
        // what makes a floor free. `upkeep_workers_needed` is the whole `quarrywork` keepers the
        // bill wants — the readout that makes a standing cost legible ("wants 2, you have 0").
        let _ = dict.insert("upkeep_demand", f64::from(deposit.upkeepDemand()));
        let _ = dict.insert("upkeep_supplied", f64::from(deposit.upkeepSupplied()));
        let _ = dict.insert("upkeep_shortfall", f64::from(deposit.upkeepShortfall()));
        let _ = dict.insert(
            "upkeep_workers_needed",
            deposit.upkeepWorkersNeeded() as i64,
        );
        // THE NEGLECT COUNTDOWN, NOT THE COUNTER — `RouteState`'s rule verbatim. `0` means IT IS
        // SLIDING NOW, and a working whose bill is met reads its rung's full grace + 1 ("walk away
        // and you have this long"). `has_neglect_grace == false` means there is NOTHING AT RISK
        // here — a working on either free floor, which declares no upkeep and so has no meter to
        // lose. **Read the bool first**; the number reuses the "biting now" `0` rather than
        // inventing a sentinel a client could mistake for a real countdown.
        let _ = dict.insert("has_neglect_grace", deposit.hasNeglectGrace());
        let _ = dict.insert(
            "neglect_grace_remaining",
            deposit.neglectGraceRemaining() as i64,
        );
        // **THE BUILD IN FRONT OF THIS WORKING — the CHAINED countdown**, everything above this
        // entry in its band's queue plus this entry's own span, and **THE SAME QUANTITY WITH THE
        // SAME SENTINELS a patch, a herd and a road publish**: there is deliberately no deposit
        // dialect, so `DetailFormat.build_sentinel_value` renders a working through the identical
        // fork with no branch of its own. `-1` no estimate, `-2` the meter holds, `-3` it rots,
        // `-4` the queue is blocked at this entry, `-5` queued since the last turn resolved.
        //
        // ⛔ **ONLY A QUEUED WORKING HAS A REAL NUMBER, AND AN UNORDERED RUNG READS `-1` RATHER THAN
        // `0`** — a `0` would render as a finished build.
        let _ = dict.insert(
            "build_turns_remaining",
            deposit.buildTurnsRemaining() as i64,
        );
        // WHY THE POOL IS STUCK ON THIS WORKING — the same free-form `BuildGate` vocabulary a patch
        // and a road publish, never an enum, so one reader answers for every branch.
        //
        // ⛔ **`""` IS NOT *FINE*.** It is *nothing is being built here*, which is a different
        // sentence from *nothing is wrong* — and it is every working on the map until a player types
        // `fell` or `quarry`.
        let _ = dict.insert(
            "build_blocked_reason",
            deposit.buildBlockedReason().unwrap_or_default(),
        );
        // IS THIS WORKING IN SOME BAND'S BUILD QUEUE RIGHT NOW? — the MEMBERSHIP flag, and the term
        // `build_turns_remaining`'s `-5` is separated from `-1` by. Resolved by the same live-queue
        // walk that resolves `build_kit_id`, because the row's own scratch lags a command by a whole
        // turn and this state exists precisely in that frame.
        //
        // ⛔ **IT CANNOT BE REPLACED BY A `build_kit_id != ""` TEST**: a resolved builders kit is
        // NEVER empty, the bare-handed kit being a roster entry like any other.
        let _ = dict.insert("is_queued", deposit.isQueued());
        // The kit this working's build is being raised with — the patch's `build_kit_id` off the
        // same one resolution seam, so the row cannot state a tool the pool is not using. `""` only
        // where no band has this working queued.
        let _ = dict.insert("build_kit_id", deposit.buildKitId().unwrap_or_default());
        // **THE KEEPING KIT** — the patch / herd pair, from the upkeep-kit resolution pass.
        // `upkeep_kit_named` is not recoverable from the id (a player may name the very kit the
        // derivation would have picked), which is why it rides beside it rather than being
        // re-derived on the client.
        let _ = dict.insert("upkeep_kit_id", deposit.upkeepKitId().unwrap_or_default());
        let _ = dict.insert("upkeep_kit_named", deposit.upkeepKitNamed());
        // --- THE ESCAPEMENT FLOOR AND THE CURVE IT IS DRAGGED ON (issue #650) -------------------
        // WHERE THIS TURN'S CREWS STOPPED, as a fraction of `capacity` — the working's own reading
        // of the dial, kept at the DEEPEST floor any band cutting it named. `0` where nobody cut it,
        // which is the identity of the max below rather than a strip order.
        //
        // ⛔ **NOTHING ON THE SHEET SEEDS FROM IT.** A compose sheet states what ONE band is asking
        // for, and that is `LaborAssignment.floor` on that band's own `extract` row
        // (`HudBandLaborState.floor_for_extract`); this is the SOURCE-level minimum across every band
        // on the working, so seeding a dial from it would silently adopt another band's deeper floor.
        // Its consequence is already `reachable` above, which the sim composes at exactly this value.
        let _ = dict.insert("floor", f64::from(deposit.floor()));
        // THE RUNG'S OWN FLOOR, IN THE SAME UNITS — `1 - recovery_fraction`, what this rung's reach
        // cannot get at. `extraction:gathering` recovers 0.15, so it strands 85% of a rock body and
        // this reads 0.85; every FORESTRY rung recovers 1.0, so this reads 0.
        //
        // ⛔ **COMPOSE IT WITH THE PLAYER'S FLOOR AS A MAXIMUM, NEVER AS A SUM.** Both are the same
        // kind of quantity — an amount left standing — so a crew stops at whichever is greater
        // (`HudDepositVocab.composed_floor`, the client's ONE composition of the pair). Added, they
        // double-count on every rung: a gathering crew would be drawn stopping 85% of a seam short of
        // where it really stops.
        let _ = dict.insert(
            "rung_floor_fraction",
            f64::from(deposit.rungFloorFraction()),
        );
        // **WHAT ONE CUTTER MOVES PER TURN AT THE RUNG THIS WORKING HOLDS**, in the material's own
        // units — the deposit twin of `ForagePatchState.perWorkerBiomass` and named after it, because
        // the crew arithmetic it feeds (*clear it now* / *hold it after*) is the same division on
        // every web. Never `0` on a live rung: there is no seasonal weight and no TAKE kit on either
        // deposit branch, so it is the rung's rate flat.
        let _ = dict.insert("per_worker_biomass", f64::from(deposit.perWorkerBiomass()));
        // **THE SAMPLED REGROWTH CURVE** — the third on the wire beside the patch's and the herd's,
        // on the same implicit x-axis: sample `i` of `n` is the one-turn delta at
        // `stock = i/(n-1) × capacity`. No sample is ever negative (a deposit has no Allee term), so
        // it is the plant curve's shape rather than the herd's.
        //
        // ⛔ **A QUARRY'S ARE ALL ZERO AND THAT IS AN ANSWER RATHER THAN AN ABSENCE.** Rock's rate is
        // zero, so the delta is exactly `0` everywhere — *this does not grow*. An EMPTY vector is the
        // different claim *no curve was sent*, and `regrowth_samples_packed` keeps the two apart:
        // `SourceForecast.has_growth_curve` is false for both, which is why the client's dial forks
        // on `regrowth_rate` and never on the curve.
        let _ = dict.insert(
            "regrowth_samples",
            &crate::dict::subsistence::regrowth_samples_packed(deposit.regrowthSamples()),
        );
        array.push(&dict.to_variant());
    }
    array
}

/// **THE TWO DEPOSIT BRANCHES' RUNG CATALOG** -- one row per rung of `intensification_ladder.json`'s
/// `forestry` and `extraction` branches, published ONCE PER WORLD beside `routeRungs` and carrying no
/// faction and no tile.
///
/// ⛔ **IT IS `route_rungs_to_array`'s TWIN, FIELD FOR FIELD WHERE THE TWO BRANCHES AGREE** -- read
/// `dict::routes` beside this function. Every rule there holds here: the row order is the CLIMB
/// order, the `""` fields are STATES rather than absences, and the base figures are quoted as
/// published with no client-side scaling.
///
/// ⛔ **ONE VECTOR CARRIES BOTH LADDERS, WHICH IS WHY `branch` IS A FIELD.** A working climbs the
/// branch its MATERIAL belongs to; a reader that ignored `branch` would offer a coppice above a
/// quarry. `order` is per branch, so it is only a climb order WITHIN one.
pub(crate) fn deposit_rungs_to_array(
    rungs: Vector<'_, ForwardsUOffset<fb::DepositRungState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for rung in rungs {
        let mut dict = VarDictionary::new();
        // The join key with a working's own `rung`, spelled `"<branch>:<id>"` exactly as
        // `DepositState.rung` spells it.
        let _ = dict.insert("rung_key", rung.rungKey().unwrap_or_default());
        // ⛔ **WHICH LADDER THIS ROW IS ON** -- `"forestry"` | `"extraction"`, the same vocabulary
        // `DepositState.branch` publishes. One vector, two ladders: without this the rows would be
        // one undifferentiated climb and a wood would be offered a quarry.
        let _ = dict.insert("branch", rung.branch().unwrap_or_default());
        let _ = dict.insert("order", rung.order() as i64);
        // "Coppice" -- resolved SIM-SIDE, so no client authors a second spelling of it.
        let _ = dict.insert("display_name", rung.displayName().unwrap_or_default());
        // The TILE COMMAND that raises this rung, and `""` on both FREE FLOORS, which nobody
        // declares: deadfall and a stone scatter are what the ground already offers.
        let _ = dict.insert("verb", rung.verb().unwrap_or_default());
        // The ladder knowledge that gates it, joining to `LadderKnowledgeState.knowledgeId`.
        let _ = dict.insert(
            "unlock_knowledge",
            rung.unlockKnowledge().unwrap_or_default(),
        );
        // The rung directly beneath, `""` at a branch's floor -- the chain a client renders the climb
        // from without holding a second copy of the order.
        let _ = dict.insert("requires_rung", rung.requiresRung().unwrap_or_default());
        // ⛔ **WHAT STANDING HERE TEACHES, AND IT IS THE CRAFT GATE'S REMEDY.** `unlock_knowledge`
        // above says what a rung WAITS ON; this says what a rung EARNS, and the two are different
        // rungs. Inferring the pairing from `requires_rung` produces byte-identical sentences on the
        // shipped five and sends the player to stand on the wrong ground the day a config breaks it.
        let _ = dict.insert("earns_knowledge", rung.earnsKnowledge().unwrap_or_default());
        let _ = dict.insert("work_cost", f64::from(rung.workCost()));
        // ...and the standing bill, BEFORE the working's own keeper-loads scale it
        // (`DepositState.upkeep_demand` is the resolved per-working reading).
        let _ = dict.insert("upkeep_work_per_turn", f64::from(rung.upkeepWorkPerTurn()));
        // ⛔ **THE PILE AND ITS NOUN ARE ONE READING** -- 8 wood on `extraction:quarry`, nothing
        // anywhere else. An amount with no word cannot be rendered into a sentence and a word with
        // no amount says nothing, so a reader takes both or neither; and the client must not supply
        // the noun itself, *a quarry eats wood* being a fact about the CONFIG.
        let _ = dict.insert("build_material_cost", f64::from(rung.buildMaterialCost()));
        let _ = dict.insert(
            "build_material_id",
            rung.buildMaterialId().unwrap_or_default(),
        );
        // ⛔ **WHAT ONE BARE-HANDED WORKER BANKS IN A TURN -- the SIM'S rate, not the rung's**, and
        // it is DECODED rather than transcribed for `RouteRungState.buildWorkPerWorkerTurn`'s
        // reason: the sim writes worker output as a sum of terms, so a copy in the client goes stale
        // in silence the day a second term lands. A reader finding this missing or `0` states NO
        // ESTIMATE rather than substituting a rate of its own.
        let _ = dict.insert(
            "build_work_per_worker_turn",
            f64::from(rung.buildWorkPerWorkerTurn()),
        );
        // --- WHAT THIS RUNG BUYS (`extraction_payoff`) ------------------------------------------
        // What ONE worker takes in a turn at this rung, in the deposit's material's own units,
        // before the reachable stock caps it. **Both free floors carry a real bare-handed rate**:
        // the whole material economy bootstraps through them.
        let _ = dict.insert(
            "yield_per_worker_turn",
            f64::from(rung.yieldPerWorkerTurn()),
        );
        // ⛔ **HOW MUCH OF THE SEAM THIS RUNG CAN EVER REACH, 0..1 -- PUBLISHED, NEVER DERIVED FROM
        // `reachable / capacity`.** That ratio clamps to the STOCK, so it falls as the rock is
        // worked while the rung's reach never moves; a payoff row computing it would quietly begin
        // quoting a different number.
        let _ = dict.insert("recovery_fraction", f64::from(rung.recoveryFraction()));
        // What this rung multiplies the deposit's OWN `regrowth_rate` by -- 2.0 at
        // `forestry:coppice`, 1.0 everywhere else. **No rung raises capacity and there is no field
        // for one**, so "stone's rate is zero" survives as arithmetic rather than as a rule.
        let _ = dict.insert("regrowth_multiplier", f64::from(rung.regrowthMultiplier()));
        // ⛔ **WHAT THE GROUND MUST HOLD FOR THIS RUNG TO STAND THERE** -- 100 at
        // `extraction:quarry`, the one placement rule on either branch. It is published so a client
        // can say WHY a rung is refused rather than only that it is; the working's own `capacity` is
        // the other half of that sentence, and a threshold transcribed client-side would be a second
        // authority over a rule the config owns.
        let _ = dict.insert("min_deposit_capacity", f64::from(rung.minDepositCapacity()));
        array.push(&dict.to_variant());
    }
    array
}
