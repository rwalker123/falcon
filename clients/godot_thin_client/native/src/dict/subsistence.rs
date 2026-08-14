//! `subsistence` section -- herds, forage patches, food modules, and the
//! intensification/sedentarization readouts built on them.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

/// The `regrowthSamples` vector both source tables publish, as the packed float array GDScript
/// interpolates over. **An ABSENT vector stays EMPTY** rather than becoming a run of zeros:
/// "published no curve" and "does not grow" are different claims, and only the first may leave the
/// chart's projection undrawn.
fn regrowth_samples_packed(samples: Option<flatbuffers::Vector<'_, f32>>) -> PackedFloat32Array {
    let mut packed = PackedFloat32Array::new();
    if let Some(samples) = samples {
        for value in samples {
            packed.push(value);
        }
    }
    packed
}

pub(crate) fn sedentarization_to_array(
    states: Vector<'_, ForwardsUOffset<fb::SedentarizationState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        let _ = dict.insert("score", state.score());
        if let Some(stage) = state.stage() {
            let _ = dict.insert("stage", stage);
        }
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn herds_to_array(
    herds: Vector<'_, ForwardsUOffset<fb::HerdTelemetryState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for herd in herds {
        let mut dict = VarDictionary::new();
        if let Some(id) = herd.id() {
            let _ = dict.insert("id", id);
        }
        if let Some(label) = herd.label() {
            let _ = dict.insert("label", label);
        }
        if let Some(species) = herd.species() {
            let _ = dict.insert("species", species);
        }
        let _ = dict.insert("x", herd.x() as i64);
        let _ = dict.insert("y", herd.y() as i64);
        let _ = dict.insert("biomass", herd.biomass());
        let _ = dict.insert("route_length", herd.routeLength() as i64);
        let _ = dict.insert("next_x", herd.nextX() as i64);
        let _ = dict.insert("next_y", herd.nextY() as i64);
        if let Some(size_class) = herd.sizeClass() {
            let _ = dict.insert("size_class", size_class);
        }
        let _ = dict.insert("huntable", herd.huntable());
        if let Some(ecology_phase) = herd.ecologyPhase() {
            let _ = dict.insert("ecology_phase", ecology_phase);
        }
        // **WHERE THE PHASE WORDS CHANGE HANDS** — `classify_ecology_phase`'s own cut points, as
        // fractions of `carrying_capacity`, i.e. **the units the escapement floor is in**. That is
        // what lets the harvest-floor chart draw them as horizontal ZONES behind the floor line: a
        // floor and a phase band are the same kind of object, so the bar's colour and the floor's
        // position share one y-axis. `ecology_phase` above ships the WORD for the stock the herd is
        // at; these ship the ladder (`B/K < collapse -> collapsing`, `< stressed -> stressed`, else
        // thriving).
        //
        // They are PER SOURCE, never a global echo: `fauna::herd_ecology` resolves wild / pastoral /
        // pen and each managed block carries its own cuts, so one pair copied into GDScript would be
        // right for a wild herd and wrong for a penned one. On this web `collapse_fraction` is ALSO
        // the Allee threshold — the point `regrowth_samples` below turns negative — so the zone edge
        // and the curve's sign change describe the same cliff from either side.
        let _ = dict.insert("collapse_fraction", f64::from(herd.collapseFraction()));
        let _ = dict.insert("stressed_fraction", f64::from(herd.stressedFraction()));
        // Predators Phase 0 — the four RAW combat components (strength ≠ danger; danger is DERIVED,
        // never stored). `attack` / `defense` are open-ended strength scalars (human-strength anchor
        // 1.0); `ferocity` / `aggression` are native 0..1 (fights-back-vs-flees / initiates-unprovoked).
        // Two derived dangers read off these: HUNT danger ≈ attack × ferocity (cost to hunt), THREAT ≈
        // attack × aggression (menace unprovoked). This decoder has a history of silently dropping
        // appended fields, so decode all four beside the other scalars.
        let _ = dict.insert("attack", herd.attack());
        let _ = dict.insert("defense", herd.defense());
        let _ = dict.insert("ferocity", herd.ferocity());
        let _ = dict.insert("aggression", herd.aggression());
        // Grazing 2d-δ — how far up the husbandry ladder THIS species can climb ("wild" hunt-only /
        // "pastoral" tame+roam-but-never-penned / "pen" the full ladder). Empty/absent ⇒ the client
        // treats it as "pen" (the full ladder). Same string convention as `species`/`ecologyPhase`;
        // the herd drawer gates its domestication + corral/extend affordances on it.
        if let Some(husbandry_ceiling) = herd.husbandryCeiling() {
            let _ = dict.insert("husbandry_ceiling", husbandry_ceiling);
        }
        let _ = dict.insert("domestication", herd.domestication());
        // **THE PER-BIOMASS YIELD VECTOR** — what ONE UNIT of this herd's biomass is worth, in each
        // account (`docs/plan_harvest_floor.md` §5). It replaces the retired `huntPolicyCeilings`
        // rows, because four rows cannot answer a **continuous** floor. With `biomass` (B),
        // `carrying_capacity` (K) already in this dict:
        //
        //   ceiling(floor)      = max(0, B - floor*K) * <account>_per_biomass
        //   collection(workers) = workers * per_worker_yield * <rung>_build_fraction
        //
        // **THE BUILD DIP MULTIPLIES THE CREW, NOT THE CEILING** (`docs/plan_harvest_floor.md` §3.1,
        // sim-side `fauna::forecast_production_and_take`). It moved there because dipping the ceiling
        // let a deeper floor build for free — a fraction of a bigger standing stock still filled the
        // crew's baskets — and it is what leaves the ceiling linear in the floor, hence composable
        // here at all.
        //
        // An INEDIBLE species (a wolf) reads `provisions_per_biomass == 0`, and what it is really
        // worth is material batches this row does not carry. No animal pays fodder, so that
        // component is `0` on every herd; it is surfaced anyway so both webs read the same pair.
        let _ = dict.insert(
            "provisions_per_biomass",
            f64::from(herd.provisionsPerBiomass()),
        );
        let _ = dict.insert("fodder_per_biomass", f64::from(herd.fodderPerBiomass()));
        // **WHAT A HUNT OF THIS HERD IS MADE OF** (arc #527) — the material twins of the two rates
        // above, and the reason an INEDIBLE quarry stops quoting nothing: a wolf's
        // `provisions_per_biomass` and `per_worker_yield` are honestly `0`, and these carry its whole
        // payload. Each is an ARRAY of `{ material_id, amount }` dicts (see `flora_share`'s cash
        // quote for the shared contract).
        //
        //   material_per_biomass — what ONE UNIT of the herd's biomass is made of. Composes at ANY
        //                          floor by the same rule the scalar rates do:
        //                          `ceiling(floor) = max(0, B - floor*K) x rate`.
        //   per_worker_material  — what ONE HUNTER brings home per turn. Clamp a band preview
        //                          `min(workers x rate, ceiling)` PER MATERIAL, exactly as for food.
        //
        // **AN EMPTY ARRAY IS "NO ROW", NEVER "ZERO"** — most species are made of nothing anyone
        // builds with. The key is always inserted so a reader can tell "no quote sent" from "this
        // herd pays no material". **DO NOT SUM** the rows into one figure: that is the retired trade
        // axis under a new name.
        let _ = dict.insert(
            "material_per_biomass",
            &material_payoffs_to_array(herd.materialPerBiomass()),
        );
        let _ = dict.insert(
            "per_worker_material",
            &material_payoffs_to_array(herd.perWorkerMaterial()),
        );
        // THE TWO INVESTMENT RUNGS' MATERIAL PAYOFFS (arc #527) — the twins of `corral_yield` /
        // `pastoral_yield`, and the replacement for the retired `corral_trade`/`pastoral_trade`.
        // Without them an INEDIBLE quarry's Tame and Corral rungs quote nothing at all: a wolf's food
        // payoff on both is honestly `0`, so the "→ then +Y" face had no number. Priced on the same
        // MSY biomass their food siblings are. EMPTY = "no row" (including a rung this herd never
        // offers), never "zero".
        let _ = dict.insert(
            "corral_material",
            &material_payoffs_to_array(herd.corralMaterial()),
        );
        let _ = dict.insert(
            "pastoral_material",
            &material_payoffs_to_array(herd.pastoralMaterial()),
        );

        // **WHAT ONE HUNTER MOVES, IN BIOMASS** — the crew term the panel's two worker targets
        // divide by (`clear it now` = room / this, `hold it after` = the regrowth at the floor /
        // this). It is NOT derivable from `per_worker_yield / provisions_per_biomass`: on a wolf
        // both of those are honestly `0`, and `0/0` is exactly the source whose crew the panel
        // most needs to price. No seasonal factor on this web (the animal side has none).
        let _ = dict.insert("per_worker_biomass", f64::from(herd.perWorkerBiomass()));
        // **THE SAMPLED REGROWTH CURVE** — this herd's own per-turn biomass delta at evenly spaced
        // fractions of `K` (sample `i` of `n` is the delta at `B = i/(n-1) * K`; the x-axis is
        // implicit). The client INTERPOLATES between samples and never fits a formula to them: the
        // two webs are different functions (a patch is logistic with a reseed floor, a herd has
        // critical depensation below `collapse_fraction`), so a GDScript copy would drift and the
        // drift would be invisible — a wrong curve still looks like a curve.
        //
        // **THE LOW SAMPLES ARE NEGATIVE, AND THAT IS THE POINT.** Below the Allee threshold the
        // herd declines whether or not it is hunted. A reader must render them as DECLINE; clamping
        // to zero draws a herd crashing to extinction as a herd sitting still, which is the whole
        // difference between floor 0 on this web and floor 0 on the plant one.
        let _ = dict.insert(
            "regrowth_samples",
            &regrowth_samples_packed(herd.regrowthSamples()),
        );
        let _ = dict.insert("corralled", herd.corralled());
        // Pen-construction meter 0..1 accrued while a keeper band works this herd under the Corral
        // policy — the animal twin of `ForagePatchState.cultivationProgress`. Read by Hud's herd
        // drawer for the "Corral: Building N%" row.
        let _ = dict.insert("corral_progress", herd.corralProgress());
        // **THE ANIMAL BUILDS, PRICED IN WORK** (docs/plan_unit_costed_work.md §8). An improvement
        // costs a fixed number of WORK UNITS and a crew produces work units per turn, so TURNS ARE
        // THE OUTPUT — which is a statement the two `0..1` meters above structurally cannot make.
        // `*_work_done / *_work_cost` IS `domestication` / `corral_progress`; the absolutes are what
        // let a readout say "18 of 50 work", and nothing here may re-derive one pair from the other.
        //
        // **THE COST IS PUBLISHED WHETHER OR NOT A BUILD IS IN FLIGHT** — it is the resolved price
        // of that job on THIS herd (the tame pair carrying the species' own cost multiplier, the pen
        // pair not: a fence is a fence), which is what lets the compose sheet quote the job BEFORE
        // the player commits.
        let _ = dict.insert("tame_work_done", herd.tameWorkDone());
        let _ = dict.insert("tame_work_cost", herd.tameWorkCost());
        let _ = dict.insert("corral_work_done", herd.corralWorkDone());
        let _ = dict.insert("corral_work_cost", herd.corralWorkCost());
        // HOW MANY MORE TURNS the running build needs, at the crew, floor and kit that worked this
        // herd. **`-1` IS "NO ESTIMATE" AND MUST RENDER AS NOTHING AT ALL** — a stalled build has no
        // finite answer, and a `0` in its place is a promise. The client CANNOT compute it (it holds
        // neither the crew's output, nor the floor multiplier, nor the kit's contribution), so the
        // sim answers, exactly as it does for `pen_upkeep` and the yield forecast.
        // **THREE NEGATIVES, THREE FACTS** — `-1` is *no estimate*, `-2` is *the meter holds
        // exactly where it is* and `-3` is *the meter is going backwards*
        // (`sim_schema::{NO_BUILD_TURNS_ESTIMATE, BUILD_METER_HOLDS, BUILD_METER_ROTS}`). Passed
        // through verbatim so GDScript reads the sim's own answer rather than deriving a second
        // opinion — and every one of them has to be READ on the other side: the client accepted the
        // first two and flattened `-3` back to *no estimate*, which rendered a bleeding build as no
        // line at all.
        let _ = dict.insert("build_turns_remaining", herd.buildTurnsRemaining() as i64);
        // WHAT THE CREW'S TOOLS TOOK OFF THIS BUILD, in work units — the `t` in
        // `effective_cost = work_cost − t`. `0` = no build in
        // flight, or the crew carries nothing that helps. It rides BESIDE the raw job rather than
        // folded into it: the cost above must not move under a tool, or the readout's price would
        // change every time a hurdle wore out.
        let _ = dict.insert("build_work_from_gear", herd.buildWorkFromGear());
        // **THE SOURCE'S HALF OF THE ESTIMATE'S TERMS**, beside the sim's own answer above rather
        // than instead of it (`.claude/rules/core_sim/yield-forecast.md` → "THE BOUNDARY, stated
        // once"). `build_turns_remaining` answers for the crew ALREADY working the herd, which is the
        // right and only thing for a card with no stepper; a sheet with one has to answer for the
        // crew the player is PROPOSING, and this term is what makes that a closed form:
        //
        //   gear(w)  = min(w, <the kit row's `build_work_saturating_crew`>)
        //              × <that row's `build_work_per_worker`>
        //   turns(w) = ceil((cost − done − gear(w)) / (w × build_work_per_worker_turn × floor/peak))
        //
        // It is READ, never assumed to be the `1.0` it is today: the sim writes worker output as a
        // sum of terms so a future buff lands there, and a client hard-coding the constant would
        // quote a number the sim disagrees with. **The GEAR half is not here** — both its terms are
        // facts about the band's ledger, so they ride the kit row (`dict/population.rs`), which is
        // what lets a compose sheet re-price the whole estimate when the player picks another kit.
        let _ = dict.insert("build_work_per_worker_turn", herd.buildWorkPerWorkerTurn());
        // Pre-commit yield forecast (food/turn at the herd's CURRENT biomass, exported at
        // output_multiplier 1.0 — the client scales by the acting band's multiplier):
        //   expected(workers, floor) = min(workers * per_worker_yield * dip, ceiling(floor))
        //   max_useful_workers(floor) = ceil(ceiling(floor) / (per_worker_yield * dip))
        // Read by Hud's %HerdAssignControls to show the expected yield live and to cap the hunter
        // stepper at what the herd can actually absorb. `ceiling(floor)` is composed from the
        // per-biomass vector above — `hunt_policy_ceilings` and the older per-policy scalars
        // (ceilingSustain/Surplus/Deplete/Eradicate/Corral) are ALL retired `(deprecated)` slots the
        // sim no longer writes, and are no longer decoded.
        let _ = dict.insert("per_worker_yield", herd.perWorkerYield());
        // **RETIRED with the trade-goods yield axis** (arc #527): the wire slot is
        // `(deprecated)` and the sim writes nothing to it. What a source pays beyond food is
        // MATERIALS, which ride the cohort's `material_batches`. The GDScript that read the
        // key it used to insert is a separate pass — the key simply stops appearing.
        // `corral_yield` is the Corral rung's PAYOFF — what the herd pays once penned; its
        // during-building dip is `corral_build_fraction` on the CREW, so together they drive the
        // pre-commit "+X → +Y while building → +Z" deal on %HerdAssignControls.
        // **On an ALREADY-PENNED herd this same field is the pen's live managed production** — a
        // corralled herd is never drawn down, so its ceiling is this number at EVERY floor and the
        // escapement composition above does not apply to it (sim `SourceYieldForecast::managed`).
        // `corral_yield` is GROSS — the pen's feed below is a separate debit on the keeper's larder.
        let _ = dict.insert("corral_yield", herd.corralYield());
        // **RETIRED with the trade-goods yield axis** (arc #527): the wire slot is
        // `(deprecated)` and the sim writes nothing to it. What a source pays beyond food is
        // MATERIALS, which ride the cohort's `material_batches`. The GDScript that read the
        // key it used to insert is a separate pass — the key simply stops appearing.
        // The pen as a managed POPULATION (docs/plan_corral_managed_population.md): a confined herd
        // cannot graze, so its keeper hauls it food every turn.
        //   `pen_upkeep`       = the feed/turn the pen DEMANDS, or WOULD demand once built, at the
        //                        herd's CURRENT biomass. Always meaningful — a projection for an
        //                        unpenned herd, the live demand for a penned one — NEVER
        //                        "0-because-unpenned". Computed on the same biomass basis as
        //                        `corral_yield`, so the two are a matched pair the Corral forecast row
        //                        subtracts ("…then +Y − Z feed"). This is the DEMANDED figure, distinct
        //                        from the PAID amount (the per-band PopulationCohortState.penFeedUpkeep
        //                        the food ledger actually debits) — a starving pen demands more than it
        //                        is paid, and `pen_fed_fraction` is that ratio.
        //   `pen_fed_fraction` = the share of that demand the keeper actually paid last turn.
        //                        1.0 = fully fed (also the value for any un-penned herd); < 1.0 = the
        //                        herd is STARVING and shrinking every turn.
        // Read by Hud's herd drawer (the Corral row's starving state + the Pen feed row) and by
        // MapView's herd marker (a starving pen's glyph tints DANGER).
        let _ = dict.insert("pen_upkeep", herd.penUpkeep());
        let _ = dict.insert("pen_fed_fraction", herd.penFedFraction());
        // Ecological carrying capacity + grazing range (Grazing Phase 2b-iii). `carrying_capacity` is
        // the herd's CURRENT derived K (what it caps at on its range); `graze_range_radius` is the hex
        // radius of that range (small game 0, big game 1, migratory = its loiter_radius). The herd
        // drawer reads them for the "Carrying capacity" / "Range" rows + the honest overgrazing test
        // (`biomass > carrying_capacity`), and MapView draws the EXACT ring the sim grazes over.
        let _ = dict.insert("carrying_capacity", herd.carryingCapacity());
        let _ = dict.insert("graze_range_radius", herd.grazeRangeRadius() as i64);
        // Predators Phase 1a — the carnivore's PREY-SENSE radius (hex radius it reaches to find/feed on
        // prey). Appended strictly after `aggression`; `predators.prey_sense_radius` (4) for a carnivore,
        // 0 for a herbivore — so `prey_sense_radius > 0` is BOTH the "this herd is a predator" signal AND
        // the ring radius the map draws in place of the (meaningless) graze ring. Same uint convention as
        // `grazeRangeRadius`; this decoder has a history of silently dropping appended fields, so decode it
        // beside the graze radius it replaces.
        let _ = dict.insert("prey_sense_radius", herd.preySenseRadius() as i64);
        // The pen as a piece of fenced LAND (docs/plan_grazing_2d.md §7). A penned herd grazes its own
        // fenced footprint and the grass it eats offsets the larder bill:
        //   `pen_radius`           = the footprint hex radius (0 = the single corralled tile).
        //   `pen_footprint_tiles`  = the count of IN-BOUNDS fenced tiles the SIM computes over
        //                            (`hex_range_tiles(corralled_at, penRadius)` length). Display as-is —
        //                            the client must NOT reconstruct the closed-form hex-disk count, which
        //                            is wrong at map edges.
        //   `pen_pasture_fraction` = the share of the pen's feed its footprint covered (0..1); with
        //                            `pen_upkeep` (the OFFSET larder bill) this drives the "Fed by pasture
        //                            NN% · larder N.N food/turn" split in the herd drawer.
        //   `pen_extend_progress`  = the in-flight fence ring's build meter (0..1) for a "Fencing N%" badge.
        // Read by Hud's herd drawer (feed-split + footprint rows, Extend affordance) and MapView's pen
        // footprint highlight.
        let _ = dict.insert("pen_radius", herd.penRadius() as i64);
        let _ = dict.insert("pen_footprint_tiles", herd.penFootprintTiles() as i64);
        let _ = dict.insert("pen_pasture_fraction", herd.penPastureFraction());
        let _ = dict.insert("pen_extend_progress", herd.penExtendProgress());
        // `fodder_draw` = the hay this pen drew from its keeper's fodder store last turn (Flora roster
        // F3). NOTE THE UNITS: this is in FODDER units (`fodder_per_biomass × biomass` scale, ~25× the
        // food-unit scale for deer), NOT food-equivalent — so it CANNOT sit in the feed-split row beside
        // the food-unit pasture/larder terms. `pen_hay_food` below is its food-equivalent twin, which
        // does drive the split. Surfaced for the fodder-store readout / completeness.
        let _ = dict.insert("fodder_draw", herd.fodderDraw());
        // The RENDER-READY three-way feed split (Flora roster F3), both in FOOD units so they share the
        // row with the pasture term — the sim partitions the pen's GROSS demand (`pen_upkeep`) into
        // three, ZERO client arithmetic (the `pen_feed_upkeep` precedent):
        //   pasture_food     = pen_upkeep × pen_pasture_fraction  (grazed free by the footprint)
        //   `pen_hay_food`   = hay's contribution, food-equivalent (0 without Foddering / no hay drawn)
        //   `pen_larder_bill`= the NET food/turn the keeper actually hauls from the FOOD larder, AFTER
        //                      pasture + hay (0 when fully fed by them). This is the honest bread bill —
        //                      the herd drawer's "larder Y.Y" term reads THIS, never the gross
        //                      `pen_upkeep` (which stays the pre-commit Corral decision's projection).
        // Sim-pinned invariant: pasture_food + pen_hay_food + pen_larder_bill == pen_upkeep (gross).
        let _ = dict.insert("pen_larder_bill", herd.penLarderBill());
        let _ = dict.insert("pen_hay_food", herd.penHayFood());
        // Body mass = the biomass of ONE animal of this species (intensification ladder slice 8b). A
        // real appended wire field (was being dropped — decoder audit), surfaced for completeness /
        // future "N animals" readouts. NOTE: it is BIOMASS, so it CANNOT drive the kill-rhythm — that
        // divides a FOOD rate (`sustainable_yield`, provisions), and food ÷ biomass is a unit error
        // (~50× too long at provisions_per_biomass 0.02). `food_per_animal` below is the food-unit twin.
        let _ = dict.insert("body_mass", herd.bodyMass());
        // Food per animal = one animal's worth of YIELD in provisions (= body_mass ×
        // provisions_per_biomass, the sim's `SourceYieldForecast::body_mass_yield`). This is what the
        // kill-rhythm divides the per-turn food rate by (`Hud._hunt_kill_rhythm`: food ÷ food →
        // animals/turn), so a mammoth reads "≈1 / 7 turns" not the biomass-÷-food 333. 0 if unknown.
        let _ = dict.insert("food_per_animal", herd.foodPerAnimal());
        // **RETIRED with the trade-goods yield axis** (arc #527): the wire slot is
        // `(deprecated)` and the sim writes nothing to it. What a source pays beyond food is
        // MATERIALS, which ride the cohort's `material_batches`. The GDScript that read the
        // key it used to insert is a separate pass — the key simply stops appearing.
        // Staffing of a MANAGED herd (intensification ladder). A domesticated herd needs
        // `herders_needed` herders every turn to HOLD its tameness; `herded_fraction` = min(1,
        // assigned / needed) is how well that demand is met. Understaffed (< 1) means the herd's
        // domestication is DECAYING — it slips back to wild and stops earning Penning — so the herd
        // drawer surfaces the deficit. `herders_needed` is 0 for a wild/unmanaged herd (never show a
        // herder readout then); `herded_fraction` defaults to 1.0 for any unmanaged/vanished herd.
        let _ = dict.insert("herders_needed", i64::from(herd.herdersNeeded()));
        // The ownership-INDEPENDENT would-be herder crew size (from biomass): equal to
        // `herders_needed` on an already-managed herd, `0` for a species that can never be tamed
        // (wild ceiling). The compose Tame/Corral worker-cap floors on this so a still-WILD herd (whose
        // `herders_needed` is ownership-gated to 0) offers the real crew up front instead of 1.
        let _ = dict.insert(
            "herders_needed_if_managed",
            i64::from(herd.herdersNeededIfManaged()),
        );
        let _ = dict.insert("herded_fraction", herd.herdedFraction());
        // The Tame rung's PAYOFF — the pastoral twin of `corral_yield`: food/turn a Sustain hunt pays
        // ONCE this herd is tamed (the pastoral MSY). While Tame's DURING-BUILDING dip rides the
        // `hunt_policy_ceilings` list, this is the "then +Y" the client shows so Tame reads as
        // `→ +pastoral_yield` (like Cultivate/Sow/Corral) instead of quoting only the dip. Sustain <
        // Tame < Corral. Appended-field audit: this is the newest slot on HerdTelemetryState.
        let _ = dict.insert("pastoral_yield", herd.pastoralYield());
        // **RETIRED with the trade-goods yield axis** (arc #527): the wire slot is
        // `(deprecated)` and the sim writes nothing to it. What a source pays beyond food is
        // MATERIALS, which ride the cohort's `material_batches`. The GDScript that read the
        // key it used to insert is a separate pass — the key simply stops appearing.
        // THE BUILD DIPS ARE RETIRED (docs/plan_standing_upkeep.md section 2.2). A crew's turn is
        // one work budget, so a crew building takes NOTHING and `preparing(stance, rung)` is `0`
        // from the model rather than from a published factor. The `tameBuildFraction` /
        // `corralBuildFraction` wire slots stay `(deprecated)` and flatc no longer emits an
        // accessor, so the `tame_build_fraction` / `corral_build_fraction` dict keys simply stop
        // appearing — the GDScript that reads them is a separate pass.
        //
        // THE STANDING UPKEEP (same doc, section 2) — what it costs to HOLD this herd's rung, per
        // turn, in work units. All three terms ship so the client subtracts nothing, and
        // `upkeep_demand` follows `pen_upkeep`'s rule: ALWAYS MEANINGFUL, so a rung with no upkeep
        // reads an honest `0` (which is every shipped rung today) rather than a sentinel.
        //
        // THERE IS NO `maintain` FLAG: "stop maintaining this" is a crew of ZERO
        // (`maintain <faction> hunt <herd> 0`), so the state rides the number the player typed
        // rather than a boolean that could disagree with it. `upkeep_workers_needed` is the MAINTAIN
        // activity's own workers_needed, in its own unit, beside the TAKE activity's
        // (`SourceYield.workersNeeded` = hands to haul the offer).
        let _ = dict.insert("upkeep_demand", herd.upkeepDemand());
        let _ = dict.insert("upkeep_supplied", herd.upkeepSupplied());
        let _ = dict.insert("upkeep_shortfall", herd.upkeepShortfall());
        let _ = dict.insert(
            "upkeep_workers_needed",
            i64::from(herd.upkeepWorkersNeeded()),
        );
        // **THE PRE-COMMIT RATE, PER RUNG** — what holding THAT rung costs per turn, published
        // unconditionally exactly as the `*_work_cost` beside it is. `upkeep_demand` above answers
        // *"what is this herd billed right now"*, which is `0` on a herd with nothing started, so a
        // compose sheet netting the build crew's output against it quoted a finish date for a build
        // whose rung can never advance at that crew. These are the ladder's own rates, so the
        // stepper's closed form subtracts the rate of the rung it is PRICING and price, meter and
        // rate always name one rung. Both carry this herd's own keeper load (`scaled_by:
        // source_load`) and are ownership-independent: a quote exists before the herd is anyone's.
        let _ = dict.insert("tame_upkeep_demand", herd.tameUpkeepDemand());
        let _ = dict.insert("corral_upkeep_demand", herd.corralUpkeepDemand());
        // THE NEGLECT GRACE (issue #442) — the animal twin of `ForagePatchState`'s pair. A COUNTDOWN,
        // not a counter: `0` = the shed is biting NOW, `N > 0` = it bites in N more un-herded turns,
        // and a herd whose keepers are present reads the rung's full `grace + 1` ("walk away and you
        // have this long"). **`has_neglect_grace = false` means NOTHING IS AT RISK** — a wild herd,
        // which is the common case — and it exists precisely because "nothing at risk" would
        // otherwise collide with the "biting now" zero. Read the bool first; the countdown is
        // meaningless without it, exactly as `owner` is without `has_owner`.
        let _ = dict.insert("has_neglect_grace", herd.hasNeglectGrace());
        let _ = dict.insert(
            "neglect_grace_remaining",
            i64::from(herd.neglectGraceRemaining()),
        );
        // THE ENGAGEMENT THROUGHPUT (`docs/plan_hunt_through_combat.md` §2) — how many animals ONE
        // hunter brings into contact per turn, and the THIRD bound on a take beside the stock above
        // the floor and the party's carry. Without it `SourceForecast`'s pre-commit curve is
        // carry-bound only and overstates a light-bodied species by the ratio of the two (~30× on a
        // Wild Fowl herd with one hunter: 40 biomass of carry is 307 birds against 10 of reach).
        // **`<= 0` MEANS "NO ENGAGEMENT STAGE", not "reaches nothing"** — the wire's finite stand-in
        // for the sim's `f32::INFINITY`, published for a PEN (a penned animal is not stalked) and for
        // a species the roster cannot resolve. The client reads it as unbounded and drops the term,
        // which is also what leaves the plant web (which never publishes this field) untouched. This
        // decoder has a history of silently dropping appended fields; it is the newest slot on
        // `HerdTelemetryState`, decoded beside the neglect pair it follows.
        let _ = dict.insert("engage_rate", herd.engageRate());
        // HOW MUCH DAMAGE ONE ANIMAL SOAKS BEFORE IT GOES DOWN (`docs/plan_hunt_through_combat.md`
        // 4.2 / 6.5) — the last term needed to explain the combat gate BEFORE a hunt is launched.
        // The client already held the other two (`PopulationCohortState.hunterAttack`, `defense`
        // above), so the gate is composable client-side and the sim exports no verdict:
        //     effective_attack = max(0, hunter_attack − defense)   // 0 ⇒ cannot be hunted at all
        //     hunter_turns     = durability / effective_attack     // what ONE hunter needs
        // **DEFENSE AND DURABILITY ARE DIFFERENT AXES**: defense is whether a hit counts at all,
        // durability is how many counting hits it takes. Authored per species, never derived from
        // `body_mass`. `0` for a herd whose species the roster cannot resolve. It is the newest slot
        // on `HerdTelemetryState`, decoded beside the `engage_rate` it follows.
        let _ = dict.insert("durability", herd.durability());
        // **The retreat, as a term** — `1 - wariness`, the fraction of what a party reaches that
        // stays to be fought. A kit's `dispersion` multiplies the FLIGHT half of it. `1` is "nothing
        // breaks off", which is a pen and the whole plant web.
        let _ = dict.insert("stay_fraction", herd.stayFraction());
        // **THE KIT THIS QUARRY'S OWN COMPOSE SHEET OPENS ON** — DERIVED per herd, not the hunt job's
        // default: the sim scores every hunt kit's per-hunter-turn take against this animal at the
        // FRESH tier and publishes the winner where it beats the job default by
        // `quarry_default_kit_margin`. It is also the kit `assign_labor … hunt <herd> <n>` resolves
        // when the command names none, so a sheet that opened on the job default instead would say
        // Stalking while the sim ran Trapping — a spear party losing three rabbits in four to the
        // retreat, which is the whole reason this field exists.
        //
        // `""` means the roster could not resolve the species; the client reads that as "no herd
        // answer" and falls back to `SubsistenceSection.defaultHuntKitId`, exactly as the sim does.
        // Newest live slot on `HerdTelemetryState`, following the two retired `*EstimatesKitId`
        // slots the forecast query replaced — those are `(deprecated)` and are decoded nowhere.
        let _ = dict.insert("default_kit_id", herd.defaultKitId().unwrap_or(""));
        array.push(&dict.to_variant());
    }
    array
}

/// The KIT ROSTER (`SubsistenceSection.kits`, `equipment.json` `kits`) — every kit a party may be
/// sent out with, in roster order, each with the tiers it grants a party whose components are all
/// FRESH. The client renders the picker off this rather than carrying a second copy of the TOE
/// table.
///
/// **The tiers here are the FRESH-KIT ones and are not this band's numbers.** What a given band's
/// WEAR does to them is the band's own row (`hunter_attack` / `hunt_carry_per_worker_biomass` /
/// `forage_carry_per_worker_biomass` on the cohort), and a readout quoting these against a band with
/// dry spears is a lie of the exact class this arc keeps correcting.
///
/// `"none"` is an ORDINARY roster entry — a kit that grants nothing, so its tiers are the unequipped
/// ones throughout — and is deliberately NOT special-cased here.
pub(crate) fn kits_to_array(kits: Vector<'_, ForwardsUOffset<fb::KitOption<'_>>>) -> VarArray {
    let mut array = VarArray::new();
    for kit in kits {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("id", kit.id().unwrap_or(""));
        let _ = dict.insert("display_name", kit.displayName().unwrap_or(""));
        // Which verbs this kit may be sent on ("hunt", "forage", "scout" and/or "warrior" — the
        // two band-wide roles gained a kit axis with the wayfinding and warrior kits). A kit named
        // for a job outside
        // this list is a COMMAND FAILURE server-side, never a silent fall back to the default, so the
        // picker filters by the job it is composing.
        let jobs = kit
            .jobs()
            .map(crate::dict::strings_to_variant_array)
            .unwrap_or_default();
        let _ = dict.insert("jobs", &jobs);
        let _ = dict.insert("attack", kit.attack() as f64);
        let _ = dict.insert(
            "hunt_carry_per_worker_biomass",
            kit.huntCarryPerWorkerBiomass() as f64,
        );
        let _ = dict.insert(
            "forage_carry_per_worker_biomass",
            kit.forageCarryPerWorkerBiomass() as f64,
        );
        // The PEN's and the SCOUT VANTAGE's tiers. `pen_carry_per_worker_biomass` is deliberately
        // NOT `hunt_carry_per_worker_biomass`: a sled drags a carcass in off the range and a pen
        // stands at the camp, so a kit carrying only a sled collects a pen at the bare rate.
        let _ = dict.insert(
            "pen_carry_per_worker_biomass",
            kit.penCarryPerWorkerBiomass() as f64,
        );
        let _ = dict.insert("scout_vantage_range", kit.scoutVantageRange() as f64);
        // **THE BUILD AXIS, IN WORK UNITS** — what ONE equipped worker takes off an improvement's
        // cost, summed over the equipped crew. Neutral `0`; the shipped handling gear declares 8.5.
        //
        // **IT REPLACES `buildRate`, WHICH IS RETIRED AND NOW FROZEN AT ITS NEUTRAL `1`** — so the
        // old key is not decoded at all rather than left decoded and always neutral, which would
        // silently strip the husbandry kit's build clause and withhold the kit from the very herd
        // the player is taming (`KitRoster.kit_offer` asks this axis FIRST). A multiplier on the
        // crew cancels the job's cost and so saves the same PERCENTAGE of turns on a garden and on a
        // farm; subtracted from the job, the job's own size decides what the tool is worth.
        let _ = dict.insert("build_work_per_worker", kit.buildWorkPerWorker() as f64);
        // What the kit does BESIDES the tiers. `dispersion` multiplies the quarry's own retreat and
        // `exposure` the hunt's injury hazard, both neutral at 1. The two mass bounds say which
        // quarry `attack` above actually applies to — 0 on an end is unbounded — so a picker can
        // resolve the gate against the animal in front of it rather than the kit's best case.
        let _ = dict.insert("dispersion", kit.dispersion() as f64);
        let _ = dict.insert("exposure", kit.exposure() as f64);
        let _ = dict.insert("attack_min_body_mass", kit.attackMinBodyMass() as f64);
        let _ = dict.insert("attack_max_body_mass", kit.attackMaxBodyMass() as f64);
        // **WHICH ITEMS THIS KIT ACTUALLY CARRIES** — the `equipment.json` `uses` list verbatim, in
        // config order (weapon first, haul aid after). The tiers above are bare numbers and name
        // nothing, so a condition readout had to GUESS which item produced them: the client mapped
        // `attack → "spears"` and told a Trapping party it carried spears, quoting the SPEARS' wear
        // against a band whose traps were fresh. `KitRoster` iterates this list instead, so the hint
        // names the kit's own gear. An EMPTY array is the real answer for `"none"` (carries nothing,
        // wears nothing), never "unknown".
        let item_ids = kit
            .itemIds()
            .map(crate::dict::strings_to_variant_array)
            .unwrap_or_default();
        let _ = dict.insert("item_ids", &item_ids);
        array.push(&dict.to_variant());
    }
    array
}

/// One rung's per-material crop quote, as an array of `{ material_id, amount }` dicts.
///
/// **Empty in, empty out** — an absent wire vector and an empty one are the same answer here ("this
/// plant pays no material at this rung"), which is why this collapses them rather than distinguishing
/// them. See the insertion site for why the KEY is nonetheless always written.
pub(crate) fn material_payoffs_to_array(
    payoffs: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::MaterialPayoff<'_>>>>,
) -> VarArray {
    let mut rows = VarArray::new();
    let Some(payoffs) = payoffs else {
        return rows;
    };
    for payoff in payoffs.iter() {
        let mut row = VarDictionary::new();
        let _ = row.insert("material_id", payoff.materialId().unwrap_or_default());
        let _ = row.insert("amount", payoff.amount());
        rows.push(&row.to_variant());
    }
    rows
}

pub(crate) fn forage_patches_to_array(
    patches: Vector<'_, ForwardsUOffset<fb::ForagePatchState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for patch in patches {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("x", patch.x() as i64);
        let _ = dict.insert("y", patch.y() as i64);
        let _ = dict.insert("cultivation_progress", patch.cultivationProgress());
        let _ = dict.insert("is_cultivated", patch.isCultivated());
        let _ = dict.insert("has_owner", patch.hasOwner());
        let _ = dict.insert("owner", patch.owner() as i64);
        let _ = dict.insert("biomass", patch.biomass());
        let _ = dict.insert("carrying_capacity", patch.carryingCapacity());
        if let Some(ecology_phase) = patch.ecologyPhase() {
            let _ = dict.insert("ecology_phase", ecology_phase);
        }
        // The plant twin of the herd's phase BANDS above — same contract, same units (fractions of
        // `carrying_capacity`, which is the floor's own axis), read through `forage::patch_ecology`
        // so the published word and the published cuts cannot disagree. The one ASYMMETRY is that a
        // patch has no Allee term: `collapse_fraction` here is a phase boundary only, and every
        // sample of `regrowth_samples` below stays non-negative through it.
        let _ = dict.insert("collapse_fraction", f64::from(patch.collapseFraction()));
        let _ = dict.insert("stressed_fraction", f64::from(patch.stressedFraction()));
        // Pre-commit yield forecast — identical contract to the herd fields above (food/turn at
        // the patch's CURRENT biomass, at output_multiplier 1.0). MapView cross-refs these onto
        // `tile_info` (as `patch_*`) so %ForageAssignControls can forecast + cap the stepper.
        let _ = dict.insert("per_worker_yield", patch.perWorkerYield());
        // The Cultivate INVESTMENT rung (forage-only): `ceiling_cultivate` is the food/turn the patch
        // pays WHILE it is being prepared (the deliberate dip), `tended_yield` what it pays once
        // cultivated. MapView cross-refs both onto `tile_info` (as `patch_*`) for the pre-commit
        // "Preparing: +X → then +Y" forecast on %ForageAssignControls.
        let _ = dict.insert("tended_yield", patch.tendedYield());
        // The Sow INVESTMENT rung + the FIELD — plant RUNG 3, the twin of the herd's Corral block
        // (docs/plan_intensification_ladder.md §2). The plant branch carries TWO build meters on ONE
        // source and both ship: `cultivation_progress`/`is_cultivated` (rung 2, above) and these.
        // They are independent — `Sow` needs no prior patch, so a Field may stand on ground that was
        // never tended. Read `is_field` (the BOOL) for the completed rung; never infer a rung from
        // the float. MapView cross-refs all five onto `tile_info` (as `patch_*`) exactly as the
        // Cultivate pair above.
        let _ = dict.insert("field_progress", patch.fieldProgress());
        let _ = dict.insert("is_field", patch.isField());
        // Sow's "preparing X → then Y" pre-commit pair, mirroring `ceiling_cultivate`/`tended_yield`.
        // `ceiling_sow` is the dip WHILE the ground is being sown (honestly ~0 on bare ground — there
        // is no standing crop to take a fraction of, so a bare-ground sow is pure investment);
        // `field_yield` is what the Field pays once sown (2× `tended_yield` on the shipped dials).
        let _ = dict.insert("field_yield", patch.fieldYield());
        // **THE PLANT BUILDS, PRICED IN WORK** (docs/plan_unit_costed_work.md §8) — the twin of the
        // herd block above, and the same contract: `*_work_done / *_work_cost` IS the `*_progress`
        // fraction beside it, the cost is the resolved price of that job on THIS patch and is
        // published whether or not a build runs, and `build_turns_remaining` of `-1` means NO
        // ESTIMATE rather than zero (with `-2` / `-3` the two never-finishing answers). TWO pairs for two rungs, the `cultivate_build_fraction` /
        // `sow_build_fraction` rule: independently tunable jobs must not share a number. ONE
        // turns/gear pair for both, because at most one improvement is ever in flight on one source.
        // MapView cross-refs all six onto `tile_info` (as `patch_*`), like the rest of the payload.
        let _ = dict.insert("cultivation_work_done", patch.cultivationWorkDone());
        let _ = dict.insert("cultivation_work_cost", patch.cultivationWorkCost());
        let _ = dict.insert("field_work_done", patch.fieldWorkDone());
        let _ = dict.insert("field_work_cost", patch.fieldWorkCost());
        // The plant twin of the herd row's — `-1` no estimate, `-2` the meter holds, `-3` it rots.
        let _ = dict.insert("build_turns_remaining", patch.buildTurnsRemaining() as i64);
        let _ = dict.insert("build_work_from_gear", patch.buildWorkFromGear());
        // The plant twin of the herd block's estimate TERM — see there for why it rides beside
        // `build_turns_remaining` rather than replacing it, why the figure is read rather than
        // assumed, and why the gear half is on the kit row instead. Every forage kit's saturating
        // crew is `0` today (no plant item declares the build stat yet, issue #539), which the closed
        // form handles as the ungeared case rather than as a missing term.
        let _ = dict.insert("build_work_per_worker_turn", patch.buildWorkPerWorkerTurn());
        // WHY this ground will not take seed — "" when it will. "too_poor" / "too_dry" /
        // "too_poor_and_too_dry", resolved server-side through the SAME `RungSiteRequirement::refusal`
        // seam the `sow` command gates on. Shipped as an ANSWER rather than a bool because only ~1% of
        // tiles are sowable (46 of 4160 on the standard map): the client has neither the per-biome
        // capacity table nor the hydrology, so it CANNOT re-derive this. Same free-form-string
        // convention as `species` / `husbandry_ceiling`; absent ⇒ treated as sowable by the client.
        if let Some(sow_site_refusal) = patch.sowSiteRefusal() {
            let _ = dict.insert("sow_site_refusal", sow_site_refusal);
        }
        // WHAT GROWS HERE (flora roster F1) — the named plants this tile's forage capacity is made
        // of, as normalized shares that sum to 1. Derived from the BIOME, not from patch state, so
        // every tile of a biome reads the same list. Already sorted (share DESC, then species key
        // ASC) server-side: preserve the wire order, never re-sort client-side.
        if let Some(composition) = patch.composition() {
            let mut shares = VarArray::new();
            for share in composition {
                let mut share_dict = VarDictionary::new();
                if let Some(species) = share.species() {
                    let _ = share_dict.insert("species", species);
                }
                if let Some(display_name) = share.displayName() {
                    let _ = share_dict.insert("display_name", display_name);
                }
                let _ = share_dict.insert("share", share.share());
                // CAN THIS PLANT EVER CLIMB THIS RUNG (flora roster S1) — species-GLOBAL legality,
                // not "is this a good idea here". An oak's mast is a wild harvest forever, so it is
                // shown in the crop picker and greyed; `share` is what says whether a LEGAL crop is
                // a wise one, and a marginal share must never disable anything.
                let _ = share_dict.insert("can_cultivate", share.canCultivate());
                let _ = share_dict.insert("can_sow", share.canSow());
                // WHAT COMMITTING PAYS — this rung's yield RELATIVE to gathering the plant wild.
                // Already folds in the tile's share AND the species' conversion rate, computed
                // sim-side through the same seams the real payout uses, so the client only ever
                // FORMATS it: >1 committing beats gathering, <1 it is a loss, and 0 is the
                // "cannot climb this rung" sentinel (a real ratio is never 0), never a number to
                // print. The raw per-species rate is deliberately NOT published — it is meaningless
                // alone and would put the payoff formula in two places.
                let _ = share_dict.insert("cultivate_yield_ratio", share.cultivateYieldRatio());
                let _ = share_dict.insert("sow_yield_ratio", share.sowYieldRatio());
                // WHAT THIS RUNG PAYS ONCE COMPLETE, committed to THIS species — same units and
                // output-multiplier convention as the forecast `payoff` the compose sheet already
                // renders, so the client SUBSTITUTES it into the "→ then" term rather than computing
                // anything. 0 on a rung the species cannot climb. (The ratio above is exactly this
                // divided by the wild rate; both come from the sim so the two can never disagree.)
                let _ = share_dict.insert("cultivate_payoff", share.cultivatePayoff());
                let _ = share_dict.insert("sow_payoff", share.sowPayoff());
                // The FODDER twin of `sow_payoff` (Flora roster F3): provisions-equivalent hay a Sown
                // Field of THIS species would pay per turn, routed to the fodder account. >0 marks a
                // fodder crop (e.g. hay_grass), whose provisions payoff/ratio read 0 — worthless as
                // food but valuable as feed. The crop picker shows this hay value in place of the 0×
                // provisions ratio so a fodder crop does not read as a loss. 0 for a normal crop.
                let _ = share_dict.insert("sow_fodder_payoff", share.sowFodderPayoff());
                // The same account AT THE TENDED RUNG (#419). The two `sow_*` payoffs above are
                // Field figures, so a Cultivate row that read them quoted rung 3's managed rate for a
                // rung that pays an MSY skim off a merely-weeded basket. Which one a row states is the
                // POLICY's question, so both ride the entry and the picker picks by rung.
                let _ = share_dict.insert("cultivate_fodder_payoff", share.cultivateFodderPayoff());
                // WHAT THIS PLANT IS FOR — "staple" | "fodder" | "cash", the species' own display
                // tag. The tile card leads each basket row with one icon per role, so a player sees
                // at a glance how much of a stand is food, feed or cash.
                //
                // **ABSENT MEANS UNSTATED, NOT "staple"** — the key is only inserted when the wire
                // carries one, exactly as `species`/`display_name` are, so GDScript reads `""` and
                // renders NO icon rather than defaulting a missing tag into a real category.
                //
                // NEVER RE-DERIVE IT FROM THE PAYOFFS ABOVE: those are rung-2/rung-3 numbers that
                // fold in the weeding and conversion gains, and they are all zero for a species
                // that cannot climb on this ground — which is exactly the case where the role is
                // still a true fact about the plant.
                if let Some(role) = share.role() {
                    let _ = share_dict.insert("role", role);
                }
                // WHAT A CASH CROP PAYS, PER MATERIAL (arc #527) — the replacement for the retired
                // `sow_trade_payoff`/`cultivate_trade_payoff`, which answered "how much trade": a
                // number a market could total and a player could not act on. Each key holds an
                // ARRAY of `{ material_id, amount }` dicts — one row per material this plant would
                // yield per turn at that rung on this tile.
                //
                // **AN EMPTY ARRAY IS "NO ROW", NEVER "ZERO".** A food crop yields no material and
                // must render nothing at all; a `0` would read as a cash crop that pays badly. The
                // key is always inserted (empty array included) so a reader can tell "this server
                // sent no quote" from "this plant pays no material" — the opposite convention to
                // `role` above, and deliberately so, because here the empty case is a real answer
                // rather than an unstated one.
                //
                // **DO NOT SUM THEM INTO ONE materials/turn FIGURE.** That is the retired trade axis
                // under a new name, and it collapses the distinction the materials model exists to
                // keep. `material_id` resolves for display against the material catalogue the
                // snapshot already ships.
                let _ = share_dict.insert(
                    "sow_material_payoff",
                    &material_payoffs_to_array(share.sowMaterialPayoff()),
                );
                let _ = share_dict.insert(
                    "cultivate_material_payoff",
                    &material_payoffs_to_array(share.cultivateMaterialPayoff()),
                );
                shares.push(&share_dict.to_variant());
            }
            let _ = dict.insert("composition", &shares);
        }
        // THE COMMITTED CROP (flora roster S1) — "" when the patch is still the wild MIXED basket
        // above, else the one species `Cultivate`/`Sow` committed this patch to (the rest of the
        // basket is displaced — docs/plan_flora_roster.md §4.3). Empty means WILD, never "unknown",
        // so the tile card switches rows on it rather than treating it as missing data. The display
        // name is resolved server-side (same convention as `species` / `sow_site_refusal`).
        if let Some(committed_species) = patch.committedSpecies() {
            let _ = dict.insert("committed_species", committed_species);
        }
        if let Some(committed_display_name) = patch.committedDisplayName() {
            let _ = dict.insert("committed_display_name", committed_display_name);
        }
        // **THE PER-BIOMASS YIELD VECTOR** — what ONE UNIT of this patch's standing crop is worth,
        // in each account, at the patch's own basket-averaged rates
        // (`docs/plan_harvest_floor.md` §5). It replaces the retired `foragePolicyCeilings` rows,
        // because four rows cannot answer a **continuous** floor. With `biomass` (B),
        // `carrying_capacity` (K) already in this dict:
        //
        //   ceiling(floor)       = max(0, B - floor*K) * <account>_per_biomass
        //   collection(workers)  = workers * per_worker_biomass * <rung>_build_fraction
        //                          * <account>_per_biomass
        //   take                 = min(ceiling, collection)      [per account]
        //
        // **THE BUILD DIP MULTIPLIES THE CREW, NOT THE CEILING** — see the herd twin above.
        //
        // **`per_worker_biomass` IS ON THE WIRE NOW**, and it closed a real gap: the patch publishes
        // `per_worker_yield` (the FOOD throughput) but no per-worker term for the other two accounts,
        // and the client used to recover the shared biomass throughput as
        // `per_worker_yield / provisions_per_biomass` — exact, and `0/0` on exactly the patches that
        // pay no food (a sown Field of flax, cotton or hay). See the field below.
        //
        // **No account carries a factor of any kind** since the 4x `market.trade_goods_multiplier`
        // was retired (plan §4): a deeper floor earns more only because it takes more biomass, which
        // is what removed the per-policy per-worker terms this used to need.
        let _ = dict.insert(
            "provisions_per_biomass",
            f64::from(patch.provisionsPerBiomass()),
        );
        let _ = dict.insert("fodder_per_biomass", f64::from(patch.fodderPerBiomass()));
        // WHAT A GATHER OF THIS PATCH IS MADE OF (arc #527) — the material twins of the two rates
        // above, and the RUNG-1 half of the material story: the crop picker's
        // `sow_material_payoff`/`cultivate_material_payoff` quote rungs 3 and 2, and a WILD gather
        // had nothing at all. A tile whose basket is 32% cotton and 26% tobacco read
        // "0.24 FOOD, — FODDER" while the turn banked fibre and leaf.
        //
        //   material_per_biomass — composes at ANY floor: `max(0, B − floor*K) × rate`.
        //   per_worker_material  — clamp `min(workers × rate, ceiling)` PER MATERIAL. Folds in the
        //                          tile's SEASONAL WEIGHT, so it is honestly EMPTY in a dead season.
        //
        // A PATCH IS A MIXED BASKET: two species that both give fibre are merged into one fibre
        // RATE (which is what a rate means), but their characteristic READINGS are never averaged —
        // those ride the batches in `material_batches`. EMPTY = "no row", never "zero". DO NOT SUM.
        let _ = dict.insert(
            "material_per_biomass",
            &material_payoffs_to_array(patch.materialPerBiomass()),
        );
        let _ = dict.insert(
            "per_worker_material",
            &material_payoffs_to_array(patch.perWorkerMaterial()),
        );

        // **WHAT ONE GATHERER MOVES, IN BIOMASS** — the plant twin of the herd's field above, with
        // the tile's SEASONAL WEIGHT folded in exactly as `per_worker_yield` folds it. So it is
        // honestly **`0` in a dead season**: do not divide by it, and do not read the zero as "no
        // forecast was sent" (`biomass`/`carrying_capacity`/the rate vector still describe the patch).
        let _ = dict.insert("per_worker_biomass", f64::from(patch.perWorkerBiomass()));
        // **THE SAMPLED REGROWTH CURVE** — the plant twin of the herd's, and the ASYMMETRY between
        // them is the model: a patch is pure logistic with a reseed floor and no Allee term, so every
        // sample here is **non-negative** and the `0.0` entry is the reseed floor's lift. That is why
        // floor 0 sets a patch back and ends a herd. Interpolated, never fitted; the peak of the
        // curve IS the food peak the chart marks, which is why no separate peak field ships.
        let _ = dict.insert(
            "regrowth_samples",
            &regrowth_samples_packed(patch.regrowthSamples()),
        );
        // The two investment rungs' FODDER payoff twins — the non-food half of
        // `tended_yield`/`field_yield`, quoted at **its own** rung (#433), never at the rung the
        // patch happens to stand on. (Their `*_trade` siblings went with arc #527's axis.)
        let _ = dict.insert("tended_fodder", patch.tendedFodder());
        let _ = dict.insert("field_fodder", patch.fieldFodder());
        // THE BUILD DIPS ARE RETIRED — the plant twins of `HerdTelemetryState`'s pair; see there
        // for why, and for what replaced them. THE STANDING UPKEEP rides here in their place.
        let _ = dict.insert("upkeep_demand", patch.upkeepDemand());
        let _ = dict.insert("upkeep_supplied", patch.upkeepSupplied());
        let _ = dict.insert("upkeep_shortfall", patch.upkeepShortfall());
        let _ = dict.insert(
            "upkeep_workers_needed",
            i64::from(patch.upkeepWorkersNeeded()),
        );
        // **THE PRE-COMMIT RATE, PER RUNG** — the plant twin of the herd block's pair; see there for
        // why the stepper cannot net against `upkeep_demand`. Both plant rungs declare
        // `scaled_by: flat`, so these are the ladder's numbers verbatim (2 and 4 work a turn today)
        // and are the same on every patch in the game.
        let _ = dict.insert("cultivation_upkeep_demand", patch.cultivationUpkeepDemand());
        let _ = dict.insert("field_upkeep_demand", patch.fieldUpkeepDemand());
        // THE NEGLECT GRACE (issue #442) — how many more un-worked turns this patch can absorb before
        // its improvement starts reverting. A COUNTDOWN, not a counter, so no client does the
        // subtraction: `0` = the ground is reverting RIGHT NOW, `N > 0` = it starts in N more
        // un-worked turns, and a patch worked this turn reads the rung's full `grace + 1`.
        // **`has_neglect_grace = false` means NOTHING IS AT RISK HERE** (a wild patch, both meters at
        // zero) and is the common case — read the bool first, or the honest "biting now" zero and the
        // "nothing to lose" case become indistinguishable. It describes whichever rung would bleed
        // NEXT, the plant web unwinding newest-first (a Field's meter goes before the tended ground
        // under it).
        let _ = dict.insert("has_neglect_grace", patch.hasNeglectGrace());
        let _ = dict.insert(
            "neglect_grace_remaining",
            i64::from(patch.neglectGraceRemaining()),
        );
        // THE BUILD CREWS ARE RETIRED with `crew_needed` (docs/plan_standing_upkeep.md section
        // 2.2). They floored the compose sheet's worker cap because that cap was inverted out of the
        // TAKE and a building crew was paid a dipped take, so a 25-turn improvement asked for FEWER
        // hands than gathering the same ground. THE PLAYER STATES THE BUILD'S CREW NOW —
        // `cultivate|sow <faction> <x> <y> <workers>` — so there is no blended count for a
        // rung-level floor to raise. The `cultivateCrewNeeded` / `sowCrewNeeded` wire slots stay
        // `(deprecated)` and flatc emits no accessor, so those dict keys simply stop appearing.
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn intensification_knowledge_to_array(
    states: Vector<'_, ForwardsUOffset<fb::IntensificationKnowledgeState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        // The FACTION-WIDE half of the two-meter split (docs/plan_intensification_ladder.md §4.1):
        // "can my PEOPLE do this verb at all?", earned once by cumulative practice and permanent —
        // as opposed to the per-source build meters (`domestication`/`corral_progress` on a herd,
        // `cultivation_progress`/`field_progress` on a patch), which are local to ONE food source and
        // decay if abandoned. One field per rung-transition, so these read as the ladder itself:
        //   plant:  wild --cultivation--> tended --seed_selection--> field
        //   animal: wild --herding------> pastoral --penning-------> pen
        let _ = dict.insert("cultivation", state.cultivation());
        let _ = dict.insert("herding", state.herding());
        // Appended by slice 4 (discovery ids 2005/2006). The §4.3 gate reshuffle: `herding` now gates
        // `tame` ALONE, and `penning` — not `herding` — gates `corral` + `extend_pen`.
        let _ = dict.insert("seed_selection", state.seedSelection());
        let _ = dict.insert("penning", state.penning());
        // **NOT A RUNG-TRANSITION GATE like the four above** — no rung waits on it. It is the lesson
        // the PEN rung itself teaches (`intensification_ladder.json`, corral's
        // `earns_knowledge: "foddering"`), and it gates every fodder seam a faction has: the pen's
        // hay DRAW, the pen's `K` fodder term, and the WILD forage patch's fodder credit. So a wild
        // hay meadow can publish a positive `ForagePatchState.fodderPerBiomass` — what the LAND pays
        // — that this faction cannot bank. Appended after `penning`, and this decoder has repeatedly
        // dropped appended fields silently: if it arrives as zero, check HERE first.
        let _ = dict.insert("foddering", state.foddering());
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn food_modules_to_array(
    modules: Vector<'_, ForwardsUOffset<fb::FoodModuleState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for module in modules {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("x", module.x() as i64);
        let _ = dict.insert("y", module.y() as i64);
        if let Some(label) = module.module() {
            let _ = dict.insert("module", label);
        }
        let _ = dict.insert("seasonal_weight", module.seasonalWeight());
        if let Some(kind) = module.kind() {
            let _ = dict.insert("kind", kind);
        }
        array.push(&dict.to_variant());
    }
    array
}

// ==============================================================================================
// THE CRAFTING CATALOGUES (`docs/plan_crafting_and_materials.md` §7)
//
// Four baselines beside the kit roster: the materials the world declares, the shared rating
// vocabulary, the recipe book, and each faction's craft knowledge. All four are decoded on BOTH the
// full and the delta path, which is the rule the `food_modules` / `faction_inventory` staleness
// recorded — a whole-section field read only on the full path republishes the BASELINE's value for
// the life of the world.
// ==============================================================================================

/// The MATERIAL CATALOGUE (`SubsistenceSection.materials`). A material is the generic thing — `hide`,
/// never `deer_hide` — and it owns its craft, its characteristic AXES, and whether it can be worked
/// with no tool at all.
///
/// **`axes` is ORDERED and the order is the contract**: a batch's readings are published in it, so a
/// readout that re-sorted them alphabetically would put `suppleness` before `toughness` on a hide and
/// call it the first axis.
pub(crate) fn materials_to_array(
    materials: Vector<'_, ForwardsUOffset<fb::MaterialDefState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for material in materials {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("id", material.id().unwrap_or(""));
        let _ = dict.insert("craft", material.craft().unwrap_or(""));
        let axes = material
            .axes()
            .map(crate::dict::strings_to_variant_array)
            .unwrap_or_default();
        let _ = dict.insert("axes", &axes);
        // `false` is the WHOLE refusal mechanism for a material with no bench tool present: the rate
        // is zero and nothing branches. The panel still has to say so out loud, which is why the flag
        // rides rather than being inferred from a zero somewhere else.
        let _ = dict.insert("hand_workable", material.handWorkable());
        let _ = dict.insert("hand_working_rate", material.handWorkingRate() as f64);
        let _ = dict.insert(
            "hand_working_quality_ceiling",
            material.handWorkingQualityCeiling() as f64,
        );
        // The equipment item that BOUNDS this material at the bench; `""` when the roster has none.
        let _ = dict.insert("tool_item_id", material.toolItemId().unwrap_or(""));
        array.push(&dict.to_variant());
    }
    array
}

/// The shared RATING VOCABULARY (`SubsistenceSection.characteristicBands`) — `poor` / `fair` /
/// `good` / `excellent`, ascending — published ONCE for the whole world rather than per material.
/// Every reading already carries its own band NAME, so this is only the legend.
pub(crate) fn characteristic_bands_to_array(
    bands: Vector<'_, ForwardsUOffset<fb::CharacteristicBandState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for band in bands {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("name", band.name().unwrap_or(""));
        let _ = dict.insert("from", band.from() as f64);
        array.push(&dict.to_variant());
    }
    array
}

/// A recipe's INPUT rows. **Inputs and outputs are both lists of THINGS**, and a thing is a material
/// or a piece of equipment, which is what makes alloying, smelting and equipment one structure with
/// no branch.
fn recipe_inputs_to_array(
    inputs: Vector<'_, ForwardsUOffset<fb::RecipeInputState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for input in inputs {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("material_id", input.materialId().unwrap_or(""));
        let _ = dict.insert("amount", input.amount() as f64);
        // The ONE characteristic this recipe judges, `""` on every other input row — a sled reads
        // toughness and cordage reads suppleness, which is why there is no "best" hide.
        let _ = dict.insert("reads_axis", input.readsAxis().unwrap_or(""));
        array.push(&dict.to_variant());
    }
    array
}

/// A recipe's OUTPUT rows. Exactly one of `equipment_id` / `material_id` is set per row, and which
/// one it is decides whether the output lands as a stock batch or as a kit entry.
fn recipe_outputs_to_array(
    outputs: Vector<'_, ForwardsUOffset<fb::RecipeOutputState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for output in outputs {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("equipment_id", output.equipmentId().unwrap_or(""));
        let _ = dict.insert("material_id", output.materialId().unwrap_or(""));
        let _ = dict.insert("amount", output.amount() as f64);
        array.push(&dict.to_variant());
    }
    array
}

/// The RECIPE BOOK (`SubsistenceSection.recipes`). The Materials & Crafting ledger reads `inputs` for
/// the rebuild cost and `outputs` for what a stock recipe makes; the per-band `craft_offers` row
/// beside it carries the RESOLVED refusal, which is never re-derived from this.
pub(crate) fn recipes_to_array(
    recipes: Vector<'_, ForwardsUOffset<fb::RecipeDefState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for recipe in recipes {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("id", recipe.id().unwrap_or(""));
        let _ = dict.insert("display_name", recipe.displayName().unwrap_or(""));
        let _ = dict.insert("craft", recipe.craft().unwrap_or(""));
        // "kit" | "tool" | "stock" — the three groups the ledger is one table in.
        let _ = dict.insert("group", recipe.group().unwrap_or(""));
        let _ = dict.insert("work", recipe.work() as f64);
        // Empty on every ordinary kit recipe — TOOLS ARE EARNED, NEVER A PREREQUISITE.
        let requires = recipe
            .requiresKnowledge()
            .map(crate::dict::strings_to_variant_array)
            .unwrap_or_default();
        let _ = dict.insert("requires_knowledge", &requires);
        let inputs = recipe
            .inputs()
            .map(recipe_inputs_to_array)
            .unwrap_or_default();
        let _ = dict.insert("inputs", &inputs);
        let outputs = recipe
            .outputs()
            .map(recipe_outputs_to_array)
            .unwrap_or_default();
        let _ = dict.insert("outputs", &outputs);
        array.push(&dict.to_variant());
    }
    array
}

/// CRAFT KNOWLEDGE, per faction per craft (`SubsistenceSection.craftKnowledge`). Not a per-world
/// constant like the three above — a craft is LEARNED — so the sim diffs it as a whole vector each
/// frame.
///
/// `completion_threshold` rides beside `progress` so the client draws no scale of its own: the
/// material rail's craft meter is `progress / completion_threshold`, and a client that guessed the
/// denominator would draw a track disagreeing with the sim's.
pub(crate) fn craft_knowledge_to_array(
    tracks: Vector<'_, ForwardsUOffset<fb::CraftKnowledgeState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for track in tracks {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", track.faction() as i64);
        let _ = dict.insert("craft_id", track.craftId().unwrap_or(""));
        // "Bone-working" — the id, hyphenated and capitalized, resolved SIM-SIDE. The client must
        // never map a craft id to English itself.
        let _ = dict.insert("display_name", track.displayName().unwrap_or(""));
        let _ = dict.insert("known", track.known());
        let _ = dict.insert("progress", track.progress() as f64);
        let _ = dict.insert("completion_threshold", track.completionThreshold() as f64);
        array.push(&dict.to_variant());
    }
    array
}
