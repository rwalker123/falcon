//! `population` section -- cohorts, demographics, and generations.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::economy::fragment_to_dict;
use crate::dict::fixed64_to_f64;

pub(crate) fn demographics_to_array(
    states: Vector<'_, ForwardsUOffset<fb::PopulationDemographicsState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in states {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        let _ = dict.insert("children", state.children() as i64);
        let _ = dict.insert("working", state.working() as i64);
        let _ = dict.insert("elders", state.elders() as i64);
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn audience_generations_to_array(
    generations: Option<flatbuffers::Vector<'_, u16>>,
) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(list) = generations {
        array.resize(list.len());
        let slice = array.as_mut_slice();
        for (index, value) in list.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

/// EVERY fixed-point (`Scalar`, 1e6) field on a `PopulationCohortState`, converted to real units in
/// ONE place — the sim stores these as `Scalar`, the wire carries them as a raw `long`, and reading
/// one raw renders a morale of 820000% or a fertility bonus of 250000x.
///
/// It exists to be TESTABLE: `population_to_dict` returns a Godot `Dictionary`, which cannot be
/// constructed outside a running engine, so the dict itself is unreachable from `cargo test`. This
/// struct is plain Rust over a real FlatBuffer, so `cohort_scalars_decode_fixed_point` can pin the
/// scale of each field. **A new Scalar cohort field belongs here, not inlined at its insert site** —
/// inlined is how the retired fixed-point age cohorts once shipped un-divided.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CohortScalars {
    morale: f64,
    morale_delta: f64,
    output_multiplier: f64,
    discontent_fraction: f64,
    grievance: f64,
    morale_settling: f64,
    morale_terrain: f64,
    morale_climate: f64,
    morale_unrest: f64,
    fertility_hunger: f64,
    fertility_reserve: f64,
    fertility_trend: f64,
}

fn cohort_scalars(cohort: fb::PopulationCohortState<'_>) -> CohortScalars {
    CohortScalars {
        morale: fixed64_to_f64(cohort.morale()),
        morale_delta: fixed64_to_f64(cohort.moraleDelta()),
        output_multiplier: fixed64_to_f64(cohort.outputMultiplier()),
        discontent_fraction: fixed64_to_f64(cohort.discontentFraction()),
        grievance: fixed64_to_f64(cohort.grievance()),
        morale_settling: fixed64_to_f64(cohort.moraleSettling()),
        morale_terrain: fixed64_to_f64(cohort.moraleTerrain()),
        morale_climate: fixed64_to_f64(cohort.moraleClimate()),
        morale_unrest: fixed64_to_f64(cohort.moraleUnrest()),
        fertility_hunger: fixed64_to_f64(cohort.fertilityHunger()),
        fertility_reserve: fixed64_to_f64(cohort.fertilityReserve()),
        fertility_trend: fixed64_to_f64(cohort.fertilityTrend()),
    }
}

fn population_to_dict(cohort: fb::PopulationCohortState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("entity", cohort.entity() as i64);
    // The band's DURABLE identity, and the only handle a command may name. `entity` is ECS
    // allocation state — a rollback rebuilds the world and renumbers every entity, so a command
    // addressed by entity bits resolved to nothing when replayed. `band_id` survives that, which is
    // why the server's `resolve_starting_unit_entity` takes it and nothing else. `entity` stays on
    // the wire for CLIENT-LOCAL identity only (selection, marker keys, roster lookup) — never for
    // anything sent back.
    //
    // **The range is bounded by the counter, not by this cast.** Godot ints are `i64`, and the trip
    // home is a *decimal text* command line (`Main.format_*` → `"assign_labor %d %d …"`), not these
    // bits — so an id above `i64::MAX` would be transmitted as a negative decimal and fail the
    // server's `u64` parse. It cannot arise: `BandIdAllocator` starts at 1 and increments by 1, so
    // ids stay astronomically inside `i64`. `0` means "no id" and does not occur for a real band.
    let _ = dict.insert("band_id", cohort.bandId() as i64);
    let _ = dict.insert("home", cohort.home() as i64);
    let _ = dict.insert("current_x", cohort.currentX() as i64);
    let _ = dict.insert("current_y", cohort.currentY() as i64);
    let _ = dict.insert("is_traveling", cohort.isTraveling());
    // Destination tile while traveling (`isTraveling` gates it; `0,0` otherwise). The map
    // draws a wrap-aware reticle + line to it for the selected traveling unit.
    let _ = dict.insert("travel_target_x", i64::from(cohort.travelTargetX()));
    let _ = dict.insert("travel_target_y", i64::from(cohort.travelTargetY()));
    let _ = dict.insert("size", cohort.size() as i64);
    // Every Scalar field below comes from `cohort_scalars` — see its doc comment for why.
    let scalars = cohort_scalars(cohort);
    let _ = dict.insert("morale", scalars.morale);
    // Signed per-turn morale trend + the dominant negative driver when falling
    // (0=None, 1=Terrain, 2=Cold, 3=Unrest). A rehydrated save reports 0/None for
    // one turn (the sim doesn't persist them) — the HUD handles that gracefully.
    let _ = dict.insert("morale_delta", scalars.morale_delta);
    let _ = dict.insert("morale_cause", i64::from(cohort.moraleCause()));
    // Civilization Wellbeing (docs/plan_civ_wellbeing.md). Productivity + discontent +
    // migration counters + the four signed Layer-1 morale contributions (their sum IS
    // morale_delta) that drive the itemized morale breakdown in the band drawer.
    let _ = dict.insert("output_multiplier", scalars.output_multiplier);
    let _ = dict.insert("discontent_fraction", scalars.discontent_fraction);
    let _ = dict.insert("last_emigrated", cohort.lastEmigrated() as i64);
    let _ = dict.insert("last_immigrated", cohort.lastImmigrated() as i64);
    // grievance: telemetry only (reserved for a future revolution consequence) — not displayed in P1.
    let _ = dict.insert("grievance", scalars.grievance);
    let _ = dict.insert("morale_settling", scalars.morale_settling);
    let _ = dict.insert("morale_terrain", scalars.morale_terrain);
    let _ = dict.insert("morale_climate", scalars.morale_climate);
    let _ = dict.insert("morale_unrest", scalars.morale_unrest);
    // The birth path's parallel of the morale contributions: the three named fertility factors whose
    // PRODUCT (not sum) is the birth_rate multiplier — hunger (did we eat) x reserve (is there a
    // cushion) x trend (is the cushion growing or shrinking). NEUTRAL AT 1.0, not at 0, so the
    // breakdown renders each as its deviation from 1.0. A rehydrated cohort reports all-zero, and
    // ZERO RESERVE IS THE NOT-PROJECTED SENTINEL (a computed reserve is >= 1 by construction, while
    // hunger and trend both legitimately reach 0) — the HUD must read that as "no reading", never as
    // a famine.
    let _ = dict.insert("fertility_hunger", scalars.fertility_hunger);
    let _ = dict.insert("fertility_reserve", scalars.fertility_reserve);
    let _ = dict.insert("fertility_trend", scalars.fertility_trend);
    let _ = dict.insert("generation", cohort.generation() as i64);
    let _ = dict.insert("faction", cohort.faction() as i64);
    let _ = dict.insert("turns_of_food", cohort.turnsOfFood() as f64);
    // Band food ledger (food/turn): total realized income across all worked sources and total
    // consumption across the cohort's population, summarized in the allocation panel's ledger footer.
    let _ = dict.insert("food_income", cohort.foodIncome() as f64);
    // NOTE: there is deliberately no band-level "steady income" key here. The Food line's income half
    // is summed CLIENT-side from the per-source `realized_yield` (see `Hud._band_food_income`), so the
    // headline provably equals the Gathered + Hunted rows beneath it rather than being a second,
    // independently-computed number that could drift from them. The cohort-level `foodIncomeAverage`
    // that briefly existed for this was redundant and is retired.
    let _ = dict.insert("food_consumption", cohort.foodConsumption() as f64);
    // The THIRD term of the band's food ledger: the food this band actually PAID this turn to feed
    // the pens it keeps, summed across every corralled herd it works. It is taken straight off the
    // larder and is in NEITHER of the two rows above, so the true net is
    //     larder_delta == food_income − food_consumption − pen_feed_upkeep
    // (pinned sim-side by `integration_tests/tests/pen_food_ledger.rs`). The sim answers this — the
    // client must never re-derive it by summing the herds' `pen_upkeep`.
    let _ = dict.insert("pen_feed_upkeep", cohort.penFeedUpkeep() as f64);
    // The band's FODDER store (Flora roster F3): hay this band has stockpiled to feed its pens, a second
    // larder distinct from the food larder above. Copied verbatim from `cohort.fodderStore()` — the
    // FODDER `LocalStore` value in fodder/grass units (`fodder_per_biomass × biomass` scale, ~25× the
    // food scale, no conversion), consistent with `fodder_draw` (grass units) and distinct from
    // `pen_hay_food` (the food-equivalent term). A pen that knows Foddering draws from this each turn
    // (`HerdTelemetryState.fodderDraw`) to shrink the bread bill it would otherwise pay from the food
    // larder. 0 for a forager band with no fodder economy.
    let _ = dict.insert("fodder_store", cohort.fodderStore() as f64);
    // Predators Phase 3 (raid legibility pair, appended after fodderStore in the schema):
    //   raid_radius  — echo of `fauna.predators.raid_radius`: how close (odd-r hex distance) an
    //                  aggressive carnivore herd must be to raid this band's larder. The band panel
    //                  uses it to decide whether a *visible* threatening predator is in exact raid
    //                  range and to raise the live "Predator nearby" Warrior-card alert. The
    //                  `work_range` idiom above (a plain `uint` reach) — decoded the same way.
    let _ = dict.insert("raid_radius", cohort.raidRadius() as i64);
    //   raid_forfeit — food this band lost to predator raids THIS turn (the raid twin of
    //                  `pen_feed_upkeep`): a negative food-ledger line the sim answers, never
    //                  re-derived client-side. 0 when no raid landed → the ledger omits the row.
    //                  Full net is larder_delta == food_income − food_consumption − pen_feed_upkeep
    //                  − raid_forfeit.
    let _ = dict.insert("raid_forfeit", cohort.raidForfeit() as f64);
    //   transfer_received / transfer_sent — FOOD THAT CROSSED BETWEEN BANDS (arc #527), the last two
    //                  terms of the ledger identity
    //                    larder_delta == food_income − food_consumption − pen_feed_upkeep
    //                                    − raid_forfeit + transfer_received − transfer_sent
    //                  Food moving from one larder to another passes through NEITHER income (what
    //                  THIS band's workers produced) nor consumption (what its people ate) — the
    //                  same hole pen_feed_upkeep and raid_forfeit were each minted for. TWO NAMED
    //                  MAGNITUDES, never one signed net: a band that both sends and receives in a
    //                  window is doing something, and a net would render that as nothing happening.
    //                  **NOT trade-only** — `balance_supply_networks` has pooled food between
    //                  neighbouring larders every turn since turn one, so any two co-networked bands
    //                  move these. FOOD ONLY: materials cross too and there is deliberately no
    //                  materials identity (the batch store IS a material's account).
    let _ = dict.insert("transfer_received", cohort.transferReceived() as f64);
    let _ = dict.insert("transfer_sent", cohort.transferSent() as f64);
    // --- THE MINIMAL TOE (`docs/plan_hunt_through_combat.md` 4.8) ---------------------------------
    // The band's THREE consumable kits and the tiers they resolve to. **All six shipped on the wire
    // with NO consumer here**, which is this crate's most-repeated bug and the third time this arc
    // has reproduced it — so they are decoded beside `raid_forfeit`, the previous newest slot, and
    // the golden now carries all six.
    //
    // ONE KIT, ONE JOB. Spears raise `hunter_attack`, a SLED raises the HUNT's carry, BASKETS raise
    // the FORAGE web's. The two carry tiers are NOT two readings of one number — a band can be out
    // of baskets with its sled untouched — and a readout that renders one on the other's row is the
    // exact defect slice 5 corrected.
    //
    // Remaining condition on the equipment.json 0-100 scale; `0` = DRY, and a dry kit steps its role
    // down to the unequipped tier and STAYS there (no replenishment path exists yet). **Performance
    // is FLAT until expiry**, so no client readout may scale anything by what is left here.
    //
    // **ONE ROW PER ITEM, driven by the server's config** — the three fixed
    // `hunting`/`sled`/`basket` floats this replaced are deprecated on the wire. Render whatever rows
    // arrive rather than looking for known ids: the item table is config, so the trapping kit's
    // `traps` (and the next item after it) appears here with no client change.
    let mut kit_item_conditions = VarArray::new();
    if let Some(conditions) = cohort.kitItemConditions() {
        for condition in conditions.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("item_id", condition.itemId().unwrap_or_default());
            let _ = row.insert("remaining", condition.remaining() as f64);
            // **`remaining == 0` IS NOT "DRY". IT IS "OWNS NONE".** The count slice inverted the
            // field above: a batch that runs out of units is REMOVED, so a worn-out item and one the
            // band never had both read `remaining 0`, and only `EquipmentBatchState.life` ("Worn
            // out" vs "Never made") tells them apart. This is the explicit ownership statement, so
            // no readout ever has to infer it — `count > 0` means the band holds units, and
            // `remaining` is then the life left on the unit in hand.
            let _ = row.insert("count", condition.count() as i64);
            // **A UNIT ARMS A PERSON, SO OWNING ONE IS NOT ARMING THE BAND.** `count` is UNITS,
            // this is PEOPLE reached — and the two differ whenever the band is short of an item or
            // holds the spawn's reserve above its head count. The sim resolves it through the same
            // `coverage` seam the take runs through; a client CANNOT compute it, because
            // `workers_per_unit` and which job is staffed are both sim-side.
            //
            // Quoted at the job whose kit carries the item (spears/sled at the hunt row, baskets at
            // the forage default, clubs at the warrior one), so a `0` here is THREE sentences —
            // nobody staffed, the band owns none, or no quoted kit carries it — and `count` beside
            // it is what separates them. A float, because a forecast counts workers in fractions.
            let _ = row.insert("workers_holding", condition.workersHolding() as f64);
            // **ITS DENOMINATOR, AND THE TWO ARE ONE SENTENCE** — *"workers_holding of
            // workers_on_quoted_job"*. The head count of the job this row is quoted at, resolved off
            // the SAME coverage the numerator came from, so the pair can never describe two
            // different jobs. Without it only the hunt was renderable (`Σ hunt_crews.workers` being
            // the only job head count on the wire), so a spears shortfall could be stated and a
            // basket's, club's or wayfinding's could not.
            //
            // **TWO ZEROS A READER MUST NOT CONFUSE**, and nothing may divide by this without
            // guarding it: `0` here means NOBODY IS STAFFED on that job — `0 of 0`, not a warning,
            // because a band with no gatherers needed no basket — while a POSITIVE denominator with
            // `workers_holding == 0` is the real shortfall, every worker on a staffed job at the
            // unequipped tier.
            let _ = row.insert(
                "workers_on_quoted_job",
                condition.workersOnQuotedJob() as f64,
            );
            kit_item_conditions.push(&row.to_variant());
        }
    }
    let _ = dict.insert("kit_item_conditions", &kit_item_conditions);
    // **HOW THIS BAND'S GEAR DIVIDES ITS HUNT WORKERS** (issue #520). `hunter_attack` below is ONE
    // number per band and, for a partly-equipped party, it is the BEST-equipped answer for
    // everybody — wrong in the reassuring direction, because `max(0, attack − defense)` decides
    // whether a species can be taken AT ALL. Ten spears among seventeen hunters take a Red Deer
    // with ten of them and with none of the other seven, and one tier cannot say that.
    //
    // One row per run of workers holding identical gear, best-equipped FIRST, `Σ workers` = the
    // band's hunt head count (an in-flight party's own workers). **Never empty**: a uniformly
    // equipped band publishes exactly ONE row, so no reader has to tell "no crews" from "one crew
    // holding nothing", and a band with nobody on the hunt job publishes one row at `workers 0`.
    //
    // **THE SIM'S ANSWER, never an input to a client-side derivation.** Each row's `hunter_attack`
    // is that run's own FLAT tier — the same rule `kit_tiers` states, one level down.
    let mut hunt_crews = VarArray::new();
    if let Some(crews) = cohort.huntCrews() {
        for crew in crews.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("workers", crew.workers() as f64);
            let _ = row.insert("hunter_attack", crew.hunterAttack() as f64);
            let item_ids = crew
                .itemIds()
                .map(crate::dict::strings_to_variant_array)
                .unwrap_or_default();
            let _ = row.insert("item_ids", &item_ids);
            hunt_crews.push(&row.to_variant());
        }
    }
    let _ = dict.insert("hunt_crews", &hunt_crews);
    // **WHAT EVERY OFFERED KIT WOULD GRANT *THIS* BAND, RIGHT NOW** — one row per roster kit,
    // resolved against this band's LIVE wear. It is the sim's ANSWER, not an input to a client-side
    // derivation: stepping a fresh tier down needs to know which ITEM supplies which AXIS, that
    // mapping is per kit (`big_game` gets attack from `spears`, `trapping` from `traps`), and no rule
    // over `KitOption.itemIds` recovers it. The client guessed `attack → spears` and repriced a band
    // with fresh traps and dry spears to the bare hand under `trapping`.
    //
    // **BOTH MASS BOUNDS RIDE PER BAND TOO**, and that is not symmetry for its own sake: a spent item
    // contributes no bound either, so a kit whose mass-bounded weapon has run dry has NO size window
    // rather than its fresh one. `KitOption`'s bounds are the fresh-kit reference only.
    let mut kit_tiers = VarArray::new();
    if let Some(tiers) = cohort.kitTiers() {
        for row in tiers.iter() {
            let mut entry = VarDictionary::new();
            let _ = entry.insert("kit_id", row.kitId().unwrap_or_default());
            let _ = entry.insert("attack", row.attack() as f64);
            let _ = entry.insert(
                "hunt_carry_per_worker_biomass",
                row.huntCarryPerWorkerBiomass() as f64,
            );
            let _ = entry.insert(
                "forage_carry_per_worker_biomass",
                row.forageCarryPerWorkerBiomass() as f64,
            );
            let _ = entry.insert("attack_min_body_mass", row.attackMinBodyMass() as f64);
            let _ = entry.insert("attack_max_body_mass", row.attackMaxBodyMass() as f64);
            let _ = entry.insert("dispersion", row.dispersion() as f64);
            let _ = entry.insert("exposure", row.exposure() as f64);
            // **THE PEN AND THE VANTAGE RIDE PER KIT TOO.** They were absent from this row until the
            // wire carried them, so a reader fell back to the ROSTER's FRESH tier for exactly these
            // two — a pen compose sheet quoting 40/keeper against a sim collecting 12, and a Scout
            // card quoting 2 tiles of sight against a reveal at 1. The band's flat
            // `pen_carry_per_worker_biomass` / `scout_vantage_range` below answer a DIFFERENT
            // question (this band at its JOB DEFAULT); these answer for the kit under the cursor.
            let _ = entry.insert(
                "pen_carry_per_worker_biomass",
                row.penCarryPerWorkerBiomass() as f64,
            );
            let _ = entry.insert("scout_vantage_range", row.scoutVantageRange() as f64);
            let _ = entry.insert("build_rate", row.buildRate() as f64);
            kit_tiers.push(&entry.to_variant());
        }
    }
    let _ = dict.insert("kit_tiers", &kit_tiers);
    // The RESOLVED tiers, so the client renders this band's real numbers instead of re-deriving them
    // from the durabilities plus a config it does not have. `hunter_attack` is the term the combat
    // gate `max(0, attack − defense)` compares against `HerdTelemetryState.defense`.
    //
    // **IT IS THE BEST-EQUIPPED CREW'S TIER, NOT THE WHOLE BAND'S** — the sim reads it off
    // `hunt_crews[0]`, so anything rendering it as if it spoke for everybody states the reassuring
    // half of a split party. `hunt_crews` above is the rest of the answer.
    let _ = dict.insert("hunter_attack", cohort.hunterAttack() as f64);
    let _ = dict.insert(
        "hunt_carry_per_worker_biomass",
        cohort.huntCarryPerWorkerBiomass() as f64,
    );
    let _ = dict.insert(
        "forage_carry_per_worker_biomass",
        cohort.forageCarryPerWorkerBiomass() as f64,
    );
    // The three tiers the EXPANDED ROSTER added — husbandry gear, wayfinding gear and clubs — each
    // published per band for the first time here. Same shape and the same cliff as the three above:
    // `pen_carry_per_worker_biomass` is a pen keeper's collection rate and is NOT the sled's
    // `hunt_carry_per_worker_biomass`; `scout_vantage_range` is how far each posted vantage SEES
    // (how far they are posted is not a kit axis); `warrior_attack` is the defending contingent's
    // own `attack` and is NOT `hunter_attack` — a band fights raids with clubs and hunts with
    // spears, so the two are different numbers on the same band.
    let _ = dict.insert(
        "pen_carry_per_worker_biomass",
        cohort.penCarryPerWorkerBiomass() as f64,
    );
    let _ = dict.insert("scout_vantage_range", cohort.scoutVantageRange() as f64);
    let _ = dict.insert("warrior_attack", cohort.warriorAttack() as f64);
    // **THE KIT THE HUNT TIERS ABOVE ARE RESOLVED THROUGH** (`docs/plan_denial_raid.md`). For an
    // IN-FLIGHT PARTY it is the kit it was SENT OUT WITH, decided at launch and carried for the
    // party's whole life — the drawer's answer to "what did I send them with?", and the tier the
    // party really fights and hauls at, every tier on the row included. For a RESIDENT BAND it is
    // the HUNT JOB'S DEFAULT, because a band has one kit per assignment and this row is per cohort;
    // the per-crew truth is the labor assignment's own `kit_id` beside that row's yields.
    //
    // **On a resident band it answers for the HUNT tiers only** — `hunter_attack`,
    // `hunt_carry_per_worker_biomass` and `pen_carry_per_worker_biomass` (a pen is worked from a
    // Hunt row). `forage_carry_per_worker_biomass`, `scout_vantage_range` and `warrior_attack` each
    // resolve through their OWN job's default, which rides the wire as
    // `default_forage_kit_id` / `default_scout_kit_id` / `default_warrior_kit_id`. Rendering any of
    // those three against this id quotes the wrong kit's tier.
    //
    // Never empty on the wire.
    let _ = dict.insert("kit_id", cohort.kitId().unwrap_or(""));
    // Data-driven settlement stage (id/label/icon are opaque pass-through strings resolved
    // by the sim from `settlement_stage_config.json`). Missing/pre-stage snapshots yield
    // `None` → empty strings, which the client renders as a neutral non-circular fallback
    // marker (ownership is on the banner, no disc).
    let settlement_stage = cohort.settlementStage();
    let _ = dict.insert(
        "settlement_stage_id",
        settlement_stage.and_then(|s| s.id()).unwrap_or(""),
    );
    let _ = dict.insert(
        "settlement_stage_label",
        settlement_stage.and_then(|s| s.label()).unwrap_or(""),
    );
    let _ = dict.insert(
        "settlement_stage_icon",
        settlement_stage.and_then(|s| s.icon()).unwrap_or(""),
    );
    if let Some(activity) = cohort.activity() {
        let _ = dict.insert("activity", activity);
    }
    // `huntMode` is RETIRED (docs/plan_harvest_floor.md): it named the largest Hunt assignment's
    // stance, and pressure is a per-source FLOOR now — read `labor_assignments[..]["floor"]`. One
    // band-wide string cannot summarise a continuous per-source dial.
    let _ = dict.insert("supply_network_id", cohort.supplyNetworkId() as i64);
    if let Some(stores) = cohort.stores() {
        let mut stores_dict = VarDictionary::new();
        for store in stores {
            if let Some(item) = store.item() {
                let _ = stores_dict.insert(item, fixed64_to_f64(store.quantity()));
            }
        }
        let _ = dict.insert("stores", &stores_dict);
    }

    if let Some(fragments) = cohort.knowledgeFragments() {
        let mut array = VarArray::new();
        for fragment in fragments {
            let dict = fragment_to_dict(fragment);
            array.push(&dict.to_variant());
        }
        let _ = dict.insert("knowledge_fragments", &array);
    }

    if let Some(migration) = cohort.migration() {
        let mut migration_dict = VarDictionary::new();
        let _ = migration_dict.insert("destination", migration.destination() as i64);
        let _ = migration_dict.insert("eta", migration.eta() as i64);
        if let Some(fragments) = migration.fragments() {
            let mut fragment_array = VarArray::new();
            for fragment in fragments {
                let dict = fragment_to_dict(fragment);
                fragment_array.push(&dict.to_variant());
            }
            let _ = migration_dict.insert("fragments", &fragment_array);
        } else {
            let _ = migration_dict.insert("fragments", &VarArray::new());
        }
        let _ = dict.insert("migration", &migration_dict);
    }

    // Early-Game Labor (slice 3b): the band's source-centric labor allocation. Each entry is a
    // staffed Forage tile / Hunt herd / Scout / Warrior demand. `harvestTask`/`scoutTask` are now
    // always null server-side and no longer decoded.
    // Always insert `labor_assignments` (empty array when the vector is absent) so the client
    // sees a stable band-dict shape regardless of whether the server serialized an empty vector.
    let mut array = VarArray::new();
    if let Some(assignments) = cohort.laborAssignments() {
        for assignment in assignments {
            let mut entry = VarDictionary::new();
            if let Some(kind) = assignment.kind() {
                let _ = entry.insert("kind", kind);
            }
            let _ = entry.insert("workers", assignment.workers() as i64);
            let _ = entry.insert("target_x", assignment.targetX() as i64);
            let _ = entry.insert("target_y", assignment.targetY() as i64);
            // Per-source food yield (food/turn): `actual_yield` is this turn's realized take, headlined
            // on the allocation row; `sustainable_yield` is the renewable-without-depletion ceiling,
            // surfaced in the row tooltip and used to flag overhunting (actual > sustainable). Forage
            // is renewable, so its two values match; only depletable herds diverge.
            let _ = entry.insert("actual_yield", assignment.actualYield() as f64);
            let _ = entry.insert("sustainable_yield", assignment.sustainableYield() as f64);
            // **THE BAND THE TWO SCALARS ABOVE SIT IN THE MIDDLE OF** (§6.4). `actual_yield` is the
            // take's EXPECTATION over the retreat seed; the take the sim pays lies inside
            // [low, high], and where nothing is stochastic the distribution is degenerate and
            // low == actual == high BIT-FOR-BIT. That is the shipped case today (wariness 0,
            // hit_chance 1.0 across the roster), so a reader must render ONE number when the bounds
            // agree and a range only when they differ — slice 7 authors wariness and the same
            // readout turns on with no further change here.
            //
            // Undecoded until now — see the kit block in `population_to_dict` for the class of bug.
            let _ = entry.insert("actual_yield_low", assignment.actualYieldLow() as f64);
            let _ = entry.insert("actual_yield_high", assignment.actualYieldHigh() as f64);
            // The per-source STEADY average: the honest long-run average of this source's lumpy
            // `actual_yield`. Headlines the Band panel row + map label so they don't swing turn-to-turn.
            let _ = entry.insert("realized_yield", assignment.realizedYield() as f64);
            // **RETIRED: `trade_yield` / `realized_trade_yield` / `trade_yield_low` /
            // `trade_yield_high`** (arc #527), with the trade-goods yield axis they decoded. The
            // wire slots are `(deprecated)` and the sim writes nothing to them. What a take pays
            // beyond food is MATERIALS, which ride `material_batches` on the cohort dict.
            //
            // **The GDScript that reads these keys is a separate pass** — they simply stop appearing
            // in the dict, so a reader falling back to `0` degrades to "no trade line", which is what
            // it already rendered for a source with no trade.
            // The FEED currency (issue #449) — the second account beside the food one above,
            // exactly the fodder the band's `FODDER` store was credited with. PLANT-ONLY: no animal
            // pays fodder, so a hunt row's `0` here is a structural zero rather than a gap, and a
            // sown hay Field states its whole product through this key alone instead of `+0.00`.
            //
            // **There is no `realized_fodder_yield` twin, deliberately** — fodder is paid by the
            // PLANT web alone, whose forward projection is food-only, so a projected-fodder field
            // would be a constant zero on the only web that can pay it. This actual IS the honest
            // rate, and the client reads it with no fallback (`SourceForecast.fodder_rate_of`).
            let _ = entry.insert("fodder_yield", assignment.fodderYield() as f64);
            // THE MATERIAL ACCOUNT (arc #527) — the third, and the ONLY one a cash Field or an
            // inedible quarry pays into at all: without it a wolf hunt's row and a cotton Field's
            // row both publish their whole product as `+0.00`. An ARRAY of `{ material_id, amount }`
            // dicts, holding what `credit_material_yield` actually deposited.
            //
            // **AN EMPTY ARRAY IS "NO ROW", NEVER "ZERO"** — most sources pay no material. A
            // PRE-COMMIT (seeded) row is empty even where the turn will pay, because the forecast
            // does not project materials; the compose-sheet number is the herd row's
            // `material_per_biomass` / `per_worker_material`. **DO NOT SUM**, and never fold into
            // `food_income`.
            let _ = entry.insert(
                "material_yield",
                &crate::dict::subsistence::material_payoffs_to_array(assignment.materialYield()),
            );
            // WHEN that steady average actually lands: index i = the food delivered i+1 turns from
            // now, length = arrivals_horizon_turns (20), 0.0 on a turn nothing arrives. A big-game
            // hunt reads lumpy (gaps between hauls); a forage patch is positive in every slot. EMPTY
            // means "not projected" (Scout/Warrior, rehydrated save) — the client must read that as
            // no data, never as famine. Always inserted so the entry shape is stable.
            let mut arrival_schedule = PackedFloat32Array::new();
            if let Some(schedule) = assignment.arrivalSchedule() {
                for amount in schedule {
                    arrival_schedule.push(amount);
                }
            }
            let _ = entry.insert("arrival_schedule", &arrival_schedule);
            // Minimum workers that would have produced this turn's take. `workers > workers_needed`
            // (with needed > 0) means labor was NOT the binding constraint — the source's yield is
            // capped by its policy ceiling / resource biomass, so the surplus workers idled here.
            // The allocation row surfaces that as the "only N of M working" overstaffing note.
            // 0 on a rehydrated save ⇒ the note degrades to hidden, never wrong.
            let _ = entry.insert("workers_needed", assignment.workersNeeded() as i64);
            // Provisions this source OFFERED that the crew could not collect (production − actual):
            // the UNDERSTAFFING signal, the exact mirror of workers_needed. > 0 ⇒ the party is
            // under-crewed for the kill (an animal too big to fully carry, or an over-abundant pulse)
            // and food is being left standing — the allocation row surfaces it as a muted "· N.N
            // wasted". 0 on a rehydrated save ⇒ hidden, never wrong.
            let _ = entry.insert("wasted_yield", assignment.wastedYield() as f64);
            // THE overhunting ⚠, answered by the sim (`!managed && policy.overdraws()`): does this
            // take draw the stock below what it sustains? False for Sustain (and investment/managed
            // sources). Confirmed rows/map labels flag on this wire bool rather than the client-derived
            // `actual > sustainable`, which false-positives on a hunt's kill turn (banked animal spikes
            // actual above the steady sustainable even under Sustain).
            let _ = entry.insert("overdraws", assignment.overdraws());
            if let Some(fauna_id) = assignment.faunaId() {
                let _ = entry.insert("fauna_id", fauna_id);
            }
            // **WHERE THIS CREW STOPS, as a fraction of the source's carrying capacity** — THE
            // authority on harvest pressure (`docs/plan_harvest_floor.md`), and since 2b the ONLY
            // statement of it: the four-value `policy` label that used to ride beside it is a
            // retired wire slot the sim can no longer write. Always inserted, so the entry shape is
            // stable.
            let _ = entry.insert("floor", assignment.floor());
            // THE SECOND AXIS (issue #442). This is what the crew is BUILDING on the source,
            // independent of how hard it pulls: "" | "cultivate" | "sow" | "tame" | "corral". Always
            // inserted (as "" when the string is absent) so the entry shape is stable and no consumer
            // has to distinguish "not building" from "older snapshot" — the two mean the same thing.
            let _ = entry.insert("improvement", assignment.improvement().unwrap_or_default());
            // **THE KIT THIS CREW IS WORKING UNDER** (`docs/plan_denial_raid.md`) — the roster id the
            // row's yields are priced at: what the player named on `assign_labor`, or the job's
            // default when they named none, already RESOLVED (the sim never publishes
            // "unspecified"). `""` on a band-wide role (scout / warrior), which consumes no kit
            // component and so has no kit axis — read that as "no selection to make", never as "no
            // kit". Always inserted so the entry shape is stable.
            let _ = entry.insert("kit_id", assignment.kitId().unwrap_or_default());
            array.push(&entry.to_variant());
        }
    }
    let _ = dict.insert("labor_assignments", &array);
    let _ = dict.insert("idle_workers", cohort.idleWorkers() as i64);
    // **THE AGE BRACKETS ARE WHOLE PEOPLE, AND THERE ARE ONLY THREE NUMBERS HERE.**
    // `working_age` IS the working bracket — the count of assignable workers — so `children` +
    // `working_age` + `elders` == `size`, guaranteed by the sim (it writes `size` as that sum).
    // There is deliberately no fourth `age_working`-style twin: two names for one number is how a
    // band came to render "17" in the PEOPLE bar beside "0 idle of 16" in the WORKFORCE header.
    //
    // The fraction the sim keeps internally is a GROWTH ACCUMULATOR, not a fact about people, and
    // the deprecated `children`/`working`/`elders` Scalar slots that once published it are gone
    // from the bindings. Nothing here rounds; the rounding happened once, in the sim.
    let _ = dict.insert("children", cohort.childrenCount() as i64);
    let _ = dict.insert("working_age", cohort.workingAge() as i64);
    let _ = dict.insert("elders", cohort.eldersCount() as i64);
    // Forage work radius (Chebyshev tiles) drives the MapView band-selection work-range ring.
    // scout_reveal_radius is now the band's effective sight-range bonus (extra tiles beyond
    // base, 0 when no scouts) — its effect shows directly in the fog, NOT as a drawn disc.
    let _ = dict.insert("work_range", cohort.workRange() as i64);
    let _ = dict.insert("scout_reveal_radius", cohort.scoutRevealRadius() as i64);
    // Hunt reach = work_range + hunt_leash_tiles (default 5): the max hex distance at which the band
    // can run a LOCAL hunt. Beyond it, the herd-hunt affordance offers a hunting EXPEDITION instead.
    let _ = dict.insert("hunt_reach", cohort.huntReach() as i64);

    // Scouting expedition (docs/plan_exploration_and_sites.md §2): a detached party is a
    // PopulationCohort tagged Expedition that flows through this same populations[] array as a
    // resident band, carrying discriminator fields. Default to false/"" so resident-band
    // markers are unaffected.
    let _ = dict.insert("is_expedition", cohort.isExpedition());
    let _ = dict.insert(
        "expedition_mission",
        cohort.expeditionMission().unwrap_or(""),
    );
    let _ = dict.insert("expedition_phase", cohort.expeditionPhase().unwrap_or(""));
    // The real band that outfitted this party (entity bits; 0 for a normal band). The Band/City
    // panel groups a band's active expeditions by `home_band_entity == band.entity`, and the band
    // cycler excludes expeditions. Bit-reinterpreted as i64 like `entity` above so the comparison
    // matches. Empty/0 for resident bands.
    let _ = dict.insert("home_band_entity", cohort.homeBandEntity() as i64);
    // **THE LENGTH OF `pendingReveal{X,Y}`, NOT THE ARRAYS — this decoder PROJECTS here, and the
    // reason is the payload.** The only question the client ever asks of those coordinates is "does
    // this party still owe its home band a map report", the fourth term of the sim's cancel-in-camp
    // test (`core_sim` `cancel_party_standing_in_camp` → `party_owes_a_report`, which is itself just
    // `!pending_reveal.is_empty()`). The coordinates themselves are a scout's ACCUMULATED reveals —
    // hundreds of tiles per cohort per frame, every frame until it reports — so marshalling them into
    // GDScript would be that whole payload carried to answer a boolean. `0` for a resident band and
    // for a party with nothing left to deliver.
    let _ = dict.insert(
        "pending_reveal_count",
        cohort.pendingRevealX().map_or(0, |coords| coords.len()) as i64,
    );
    // Hunt expedition (PR 2, docs/plan_exploration_and_sites.md §2b): the herd a hunt party
    // follows (fauna_id string like "game_deer_57", mirrors LaborAssignment.faunaId); "" for a
    // scout expedition / normal band. `expedition_mission` also takes "hunt", `expedition_phase`
    // also takes "hunting"/"delivering" — same string fields already decoded above, new values.
    let _ = dict.insert(
        "expedition_target_herd",
        cohort.expeditionTargetHerd().unwrap_or(""),
    );
    // THE NAME OF THAT HERD ("Red Deer"), resolved by the sim at launch and carried for the party's
    // life — what the HUD renders for the quarry, while the id above stays the key it addresses
    // commands by. It exists because the herd list the client used to join the id against is
    // fog-filtered and extinction-pruned, and a detached party is not a vision source, so a party's
    // own target routinely leaves that list and left the raw id on screen (issue #378). "" for a
    // scout or a resident band.
    let _ = dict.insert(
        "expedition_target_species",
        cohort.expeditionTargetSpecies().unwrap_or(""),
    );
    // WHERE THE RAID STOPS, as a fraction of the herd's carrying capacity — the launched party's
    // orders (`docs/plan_harvest_floor.md`), replacing the retired `expeditionHuntPolicy` string.
    // `1.0` on a scout or a resident band: they harvest no herd, and an absent floor must not read as
    // "take everything". Beside it, the carry ceiling (party × per_worker_carry; 0 for scouts/bands),
    // which the hunt panel shows as "Carried X / cap" plus a FULL state.
    let _ = dict.insert("expedition_floor", f64::from(cohort.expeditionFloor()));
    let _ = dict.insert(
        "expedition_carry_cap",
        f64::from(cohort.expeditionCarryCap()),
    );
    // --- THE SHIPMENT A TRADE PARTY IS CARRYING (arc #527, issue #517) ----------------------------
    // **THE KEY AND ITS DISPLAY TWIN**, the `expedition_target_herd` / `expedition_target_species`
    // rule: `expedition_destination_band` is the `BandId` `send_trade_expedition` addresses and must
    // NEVER be rendered, `expedition_destination_name` is what a readout shows — resolved at LAUNCH
    // and carried, because the destination is exactly the thing a party outlives (a band walks away,
    // leaves the viewer's sight, or is gone while the shipment is still bound for it).
    //
    // **THERE IS NO FACTION FIELD, AND THAT IS THE ARC'S DISCIPLINE.** A shipment to your own
    // splinter and one to strangers are the same row.
    let _ = dict.insert(
        "expedition_destination_band",
        cohort.expeditionDestinationBand() as i64,
    );
    let _ = dict.insert(
        "expedition_destination_name",
        cohort.expeditionDestinationName().unwrap_or(""),
    );
    // What the party carries, in the two accounts a band store holds. The materials reuse
    // `MaterialPayoff`, so this is the `material_yield` / `delivered_material` shape a third time —
    // **NEVER SUMMED** (a total of hide and bone is the retired trade axis under a new name), EMPTY
    // MEANS "no row" rather than zero, and the key is always present. The per-material amount is the
    // total across the batches the party holds; the batches themselves, with their exact readings,
    // ride `material_batches` above.
    let _ = dict.insert(
        "expedition_cargo_food",
        f64::from(cohort.expeditionCargoFood()),
    );
    let mut cargo_materials = VarArray::new();
    if let Some(rows) = cohort.expeditionCargoMaterials() {
        for row in rows.iter() {
            let mut entry = VarDictionary::new();
            let _ = entry.insert("material_id", row.materialId().unwrap_or(""));
            let _ = entry.insert("amount", row.amount() as f64);
            cargo_materials.push(&entry.to_variant());
        }
    }
    let _ = dict.insert("expedition_cargo_materials", &cargo_materials);
    // **THE TWO SHIPMENT-MASS LEVERS, ECHOED ONTO EVERY COHORT** — the same global-lever idiom as
    // `expedition_per_worker_carry` / `hunt_per_worker_provisions` above, and the pair the OUTFIT UI
    // needs: it prices a manifest for a party that does not exist yet, so no per-party field can
    // serve that screen. The sim's own expression, held verbatim client-side:
    //
    //   mass = Σ food rows + expedition_trade_material_carry_weight × Σ material row amounts
    //   cap  = party_workers × expedition_trade_per_worker_carry
    //
    // **THE PACK LEVER IS NOT `expedition_per_worker_carry`.** That one is the HUNT pack — a raid's
    // provisions ceiling — and a client composing a trade cap out of it is one config edit away from
    // quoting a cap `send_trade_expedition` refuses. Once a shipment is on the map its own pack is
    // `expedition_carry_cap` above, which resolves per MISSION.
    let _ = dict.insert(
        "expedition_trade_per_worker_carry",
        f64::from(cohort.expeditionTradePerWorkerCarry()),
    );
    let _ = dict.insert(
        "expedition_trade_material_carry_weight",
        f64::from(cohort.expeditionTradeMaterialCarryWeight()),
    );
    // WHICH STOP WILL END THIS PARTY'S RAID — the `core_sim::HuntTripBound` key
    // ("pack_full" | "floor" | "herd_lost" | "horizon"), off the same in-flight
    // forward simulation `expedition_eta_turns` comes from, so it answers for the party's REAL
    // orders rather than for the band-agnostic pre-launch table.
    //
    // `""` = NOT RAIDING (a resident band, a scout, or a party already walking a load home), and it
    // is deliberately a different statement from `"horizon"`, which means the projection ran and
    // found no stop. The client renders no bound clause at all for `""`.
    let _ = dict.insert(
        "expedition_trip_bound",
        cohort.expeditionTripBound().unwrap_or(""),
    );
    // In-flight hunt-party next-delivery forecast (the drawer's "Next delivery: ~X food in ~N turns"
    // line) — the in-flight twin of the pre-launch forecast the client now ASKS for over the query
    // channel. 0 / 0.0 / false when n/a (scout, normal band, or a raid with no finite ETA). See
    // core_sim expedition_delivery.
    let _ = dict.insert(
        "expedition_eta_turns",
        i64::from(cohort.expeditionEtaTurns()),
    );
    let _ = dict.insert(
        "expedition_projected_delivery",
        f64::from(cohort.expeditionProjectedDelivery()),
    );
    let _ = dict.insert("expedition_recurring", cohort.expeditionRecurring());
    // Global expedition/labor config echoed onto EVERY cohort. These are DISPLAY levers only — none
    // of them is an input to an expedition trip length. An expedition's turns-to-fill comes from the
    // sim's FORECAST QUERY answer and NOTHING ELSE: the sim forward-simulates the trip for the exact
    // (band, kit, party, floor) that was asked about and returns the ANSWER, so the client performs a
    // PURE READ and does ZERO arithmetic for an expedition. It must NEVER divide a carry cap by a
    // take rate: the herd's state moves under the party and its stock exhausts mid-trip, so any
    // closed form drifts from the take the sim actually performs. Pinned by
    // core_sim/tests/expedition_hunt.rs.
    // What each lever is actually FOR:
    //   expedition_viability_warn_turns — the viable/not-viable threshold applied to `turns_to_fill`
    //   hunt_per_worker_provisions      — one hunter's throughput, used ONLY by the RESIDENT-BAND
    //     local-hunt preview, which genuinely IS arithmetic:
    //         min(workers × hunt_per_worker_provisions, band_ceiling) × output_multiplier
    //     over the herd's `hunt_policy_ceilings` (a renewable FLOW), pinned by
    //     `exported_snapshot_fields_reproduce_band_hunt_take`.
    // Band = flow arithmetic; expedition = lookup.
    let _ = dict.insert(
        "hunt_per_worker_provisions",
        f64::from(cohort.huntPerWorkerProvisions()),
    );
    let _ = dict.insert(
        "expedition_viability_warn_turns",
        cohort.expeditionViabilityWarnTurns() as i64,
    );
    // **HOW LONG THE SIM'S RAID PROJECTION RUNS** — the SCALE every "never completed" sentinel this
    // subsystem publishes is relative to (`turns_to_fill == 0`, `turns_to_collapse{,_low,_high} == 0`,
    // `expedition_trip_bound == "horizon"`). ONE lever serves both raid tables (the sim's
    // `denial_projection_at` and `hunt_trip_forecast_seeded` read the same
    // `expedition_config.hunt.forecast_horizon_turns`), so there is nothing here for a client to pick
    // wrongly between.
    //
    // **IT IS NOT A TRIP LENGTH.** It bounds the HUNTING only — `turns_to_fill` excludes travel — so a
    // client quoting it as a trip figure understates the trip by the entire walk. The floor on a hunt's
    // whole span is `this + round-trip travel`; see `SourceForecast.RAID_TURNS_UNBOUNDED`.
    let _ = dict.insert(
        "expedition_forecast_horizon_turns",
        cohort.expeditionForecastHorizonTurns() as i64,
    );
    // Per-worker carry the pack fills to: an expedition delivers `party_workers ×
    // expeditionPerWorkerCarry` food when it fills. This IS a display number the client may multiply
    // by the party size (the same blessed party×lever arithmetic as the band ceiling — NOT the
    // ecology/turns-to-fill lookup the expedition discipline protects), used to show the pre-launch
    // HAUL beside the turns-to-fill forecast. 0 when absent.
    let _ = dict.insert(
        "expedition_per_worker_carry",
        f64::from(cohort.expeditionPerWorkerCarry()),
    );
    // Band move speed (tiles/turn, LaborConfig scalar echoed per-cohort). The hunt-expedition
    // forecast's round-trip TRAVEL turns are `ceil(2 × hex_distance(band, herd) / this)` — without
    // it the travel breakdown reads 0 and degrades to hunting-turns-only. 0/absent = no travel line.
    let _ = dict.insert(
        "band_move_tiles_per_turn",
        f64::from(cohort.bandMoveTilesPerTurn()),
    );
    // **The two split floors** (`docs/plan_band_fission.md` §Config levers), echoed per-cohort the
    // way `bandMoveTilesPerTurn` directly above is, so the compose sheet can state each number
    // without a second copy of the config.
    //
    // **THE FLOORS CROSS, THE VERDICT DOES NOT.** The sheet moves a worker stepper, so a published
    // verdict would need one field per possible composition; what the client renders instead is a
    // pair of thresholds the sim owns, exactly as the per-source forecast publishes rates rather
    // than an answer per party size.
    let _ = dict.insert("founding_min_workers", cohort.foundingMinWorkers() as i64);
    let _ = dict.insert(
        "founding_parent_min_workers",
        cohort.foundingParentMinWorkers() as i64,
    );

    if let Some(access) = cohort.accessibleStockpile() {
        let mut stock_dict = VarDictionary::new();
        let _ = stock_dict.insert("radius", access.radius() as i64);
        if let Some(entries) = access.entries() {
            let mut entry_array = VarArray::new();
            for entry in entries {
                let mut entry_dict = VarDictionary::new();
                if let Some(item) = entry.item() {
                    let _ = entry_dict.insert("item", item);
                }
                let _ = entry_dict.insert("quantity", entry.quantity());
                entry_array.push(&entry_dict.to_variant());
            }
            let _ = stock_dict.insert("entries", &entry_array);
        }
        let _ = dict.insert("accessible_stockpile", &stock_dict);
    }

    // --- CRAFTING & MATERIALS (`docs/plan_crafting_and_materials.md` §7) ---------------------------
    // **THE SIM RESOLVES THE REFUSAL, THIS DECODER ONLY CARRIES IT.** Every string below —
    // `reason`, `severity`, `life`, `life_severity`, `blocked_reason`, `quantum_noun` — is resolved
    // sim-side and must reach the panel VERBATIM. Re-deriving one here (or above it, in GDScript) is
    // impossible rather than merely redundant: the derivation needs the band's batch readings, the
    // tool that bounds a material, the recipe's grade seams and the item's wear quantum, and those
    // either do not ride this wire or cannot be joined correctly from it. That is the rule `kitTiers`
    // exists to enforce, one subsystem over.
    // Always inserted (empty array / all-default dict when the vector is absent) so the band dict has
    // a stable shape and the panel never has to distinguish "no crafting" from "older snapshot".
    let mut material_batches = VarArray::new();
    if let Some(batches) = cohort.materialBatches() {
        for batch in batches.iter() {
            let mut row = VarDictionary::new();
            // The GENERIC material id — "hide", never "deer_hide".
            let _ = row.insert("material_id", batch.materialId().unwrap_or(""));
            let _ = row.insert("amount", batch.amount() as f64);
            // **THE EXACT READING AND ITS BAND, BOTH.** The band (`poor`/`fair`/`good`/`excellent`)
            // is the merge key and the word the rail shows; the exact value is what crafting reads,
            // so two `good` hides are not interchangeable. Published in the material's DECLARED axis
            // order, which this decoder preserves.
            let mut readings = VarArray::new();
            if let Some(list) = batch.readings() {
                for reading in list.iter() {
                    let mut entry = VarDictionary::new();
                    let _ = entry.insert("axis", reading.axis().unwrap_or(""));
                    let _ = entry.insert("value", reading.value() as f64);
                    let _ = entry.insert("band_name", reading.bandName().unwrap_or(""));
                    readings.push(&entry.to_variant());
                }
            }
            let _ = row.insert("readings", &readings);
            // The nearest declared VARIETY ("copper", "bronze"), `""` when the material declares
            // none — which is every shipped material. Varieties are NAMING, not materials.
            let _ = row.insert("variety_name", batch.varietyName().unwrap_or(""));
            material_batches.push(&row.to_variant());
        }
    }
    let _ = dict.insert("material_batches", &material_batches);

    // WHAT IS ON THIS BAND'S BENCH — one job at a time, so the panel never has to explain a queue.
    // An empty `recipe_id` is an IDLE bench, which is a different statement from a BLOCKED one: a
    // blocked bench has a recipe AND a `blocked_reason`.
    let mut bench_dict = VarDictionary::new();
    let _ = bench_dict.insert(
        "recipe_id",
        cohort.bench().and_then(|b| b.recipeId()).unwrap_or(""),
    );
    let _ = bench_dict.insert(
        "display_name",
        cohort.bench().and_then(|b| b.displayName()).unwrap_or(""),
    );
    let _ = bench_dict.insert("workers", cohort.bench().map_or(0, |b| b.workers()) as i64);
    let _ = bench_dict.insert(
        "progress",
        cohort.bench().map_or(0.0, |b| b.progress()) as f64,
    );
    let _ = bench_dict.insert("work", cohort.bench().map_or(0.0, |b| b.work()) as f64);
    // The craft one completed item credits — crafting is the fourth teacher.
    let _ = bench_dict.insert(
        "teaches",
        cohort.bench().and_then(|b| b.teaches()).unwrap_or(""),
    );
    // WHY THE BENCH IS NOT MOVING, resolved sim-side. `""` = it is working. It reads in exactly the
    // offer vocabulary plus the crew's own refusal, because a bench with a full pile and nobody on
    // it is also stopped.
    let _ = bench_dict.insert(
        "blocked_reason",
        cohort.bench().and_then(|b| b.blockedReason()).unwrap_or(""),
    );
    // WHETHER THAT REASON IS A FAULT OR A PROMPT, in the same `danger`/`neutral`/`good` vocabulary as
    // `CraftOffer.severity`, `""` when nothing is blocking. **A bench waiting for its crew is the
    // NORMAL state one click after Make** — the player staffs the bench — so it resolves `neutral`
    // while a shortage, an unknown craft or a zero craft rate resolve `danger`. Resolved sim-side
    // beside the reason for the reason the reason itself is: a client tinting every refusal one
    // colour renders the expected state as an alarm, and one re-deriving the severity from the
    // wording is parsing a string it may only render.
    let _ = bench_dict.insert(
        "blocked_severity",
        cohort
            .bench()
            .and_then(|b| b.blockedSeverity())
            .unwrap_or(""),
    );
    let _ = bench_dict.insert(
        "shortfalls",
        &shortfalls_to_array(cohort.bench().and_then(|b| b.shortfalls())),
    );
    let _ = bench_dict.insert(
        "items_completed",
        cohort.bench().map_or(0, |b| b.itemsCompleted()) as i64,
    );
    let _ = bench_dict.insert("drawn", cohort.bench().is_some_and(|b| b.drawn()));
    // The grade the pile in flight FIXED — `""` before the draw, or on an ungraded recipe.
    let _ = bench_dict.insert(
        "output_grade",
        cohort.bench().and_then(|b| b.outputGrade()).unwrap_or(""),
    );
    // **WHAT ONE TURN ADDS, ALREADY THROUGH THE TOOL JOIN** — `workers × progress_per_worker_turn ×
    // craft_speed`, where `craft_speed` is the equipped bench tool's rate or the material's
    // bare-handed one. A client multiplying `workers` by anything of its own would miss that factor
    // and promise a finish in half the turns it takes, which is why this rides resolved, exactly as
    // `kit_tiers` does. `0` is a STATE — no crew, no recipe, or a craft speed of zero — and the
    // `blocked_reason` beside it says which.
    let _ = bench_dict.insert(
        "rate_per_turn",
        cohort.bench().map_or(0.0, |b| b.ratePerTurn()) as f64,
    );
    // **THE PILE ALREADY CUT, SO A CLEAR CAN NAME WHAT IT DESTROYS.** The WITHDRAWN amounts, not the
    // recipe's stated inputs and not a shortfall's `required`: a bench tool's material efficiency
    // sits between the book and the withdrawal. Empty on an undrawn bench.
    let _ = bench_dict.insert(
        "drawn_inputs",
        &drawn_inputs_to_array(cohort.bench().and_then(|b| b.drawnInputs())),
    );
    let _ = dict.insert("bench", &bench_dict);

    // **ONE ROW PER RECIPE, ALWAYS**, and `reason` + `severity` are the contract rather than
    // `available`: "Not needed yet" is a SHRUG and "Short 4.9 bone" is a PROBLEM, and a client
    // deriving both from a boolean cannot tell them apart.
    let mut craft_offers = VarArray::new();
    if let Some(offers) = cohort.craftOffers() {
        for offer in offers.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("recipe_id", offer.recipeId().unwrap_or(""));
            let _ = row.insert("display_name", offer.displayName().unwrap_or(""));
            // "kit" | "tool" | "stock" — the three groups the ledger is one table in.
            let _ = row.insert("group", offer.group().unwrap_or(""));
            // The equipment id this recipe makes, `""` for a material recipe — the JOIN key onto
            // `equipment_batches`.
            let _ = row.insert("output_item_id", offer.outputItemId().unwrap_or(""));
            let _ = row.insert("available", offer.available());
            let _ = row.insert("reason", offer.reason().unwrap_or(""));
            let _ = row.insert("severity", offer.severity().unwrap_or(""));
            let _ = row.insert("shortfalls", &shortfalls_to_array(offer.shortfalls()));
            let _ = row.insert("output_grade", offer.outputGrade().unwrap_or(""));
            let _ = row.insert("on_bench", offer.onBench());
            // **THE LEDGER'S GROUP HEAD** — the tier a craft would produce right now, and its rank
            // in the item's own list. The heads run rank-DESCENDING (newest first), which is the
            // client's only honest ordering: alphabetical would put Iron above Bronze.
            let _ = row.insert("output_tier_name", offer.outputTierName().unwrap_or(""));
            let _ = row.insert("output_tier_rank", offer.outputTierRank() as i64);
            // **RENDER IT VERBATIM, and only this carries a tier word into the Owned cell.** `""`
            // when there is no news — what the band carries is said only when it disagrees with
            // what the band could now make.
            let _ = row.insert("owned_note", offer.ownedNote().unwrap_or(""));
            craft_offers.push(&row.to_variant());
        }
    }
    let _ = dict.insert("craft_offers", &craft_offers);

    // **THE LIFE METER IS A FUEL GAUGE, NOT A PERFORMANCE METER.** A spear at 34% is exactly as
    // deadly as one at 100%, so `life` reads in the item's OWN USE QUANTA and never in percent, and
    // the noun those quanta are counted in is resolved sim-side off the item's `wear.per`. A client
    // must not map quanta to English, and must not draw a percentage of its own beside them.
    let mut equipment_batches = VarArray::new();
    if let Some(batches) = cohort.equipmentBatches() {
        for batch in batches.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("item_id", batch.itemId().unwrap_or(""));
            let _ = row.insert("tier_id", batch.tierId().unwrap_or(""));
            let _ = row.insert("grade", batch.grade().unwrap_or(""));
            // **`count == 0` MEANS THE BAND OWNS NONE**, and it is the only honest ownership test:
            // a batch that runs out of units is removed, so worn-out and never-made both read 0
            // here and are told apart by `life` alone.
            let _ = row.insert("count", batch.count() as i64);
            let _ = row.insert("remaining", batch.remaining() as f64);
            let _ = row.insert("quanta_left", batch.quantaLeft() as f64);
            let _ = row.insert("quantum_noun", batch.quantumNoun().unwrap_or(""));
            let _ = row.insert("life", batch.life().unwrap_or(""));
            let _ = row.insert("life_severity", batch.lifeSeverity().unwrap_or(""));
            equipment_batches.push(&row.to_variant());
        }
    }
    let _ = dict.insert("equipment_batches", &equipment_batches);

    dict
}

/// **WHAT THE STORE ACTUALLY LOST FOR THE JOB IN FLIGHT** — one row per input material, in the
/// recipe's own input order, empty when nothing has been cut yet. It is the WITHDRAWAL rather than
/// the recipe's price, which is the whole point: a clear or a swap spends this pile, and the tool's
/// material efficiency means the book's number would name the wrong loss.
fn drawn_inputs_to_array(
    inputs: Option<Vector<'_, ForwardsUOffset<fb::DrawnInput<'_>>>>,
) -> VarArray {
    let mut array = VarArray::new();
    if let Some(list) = inputs {
        for input in list.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("material_id", input.materialId().unwrap_or(""));
            let _ = row.insert("amount", input.amount() as f64);
            array.push(&row.to_variant());
        }
    }
    array
}

/// **WHAT A DRAW IS SHORT, AS A NUMBER** — the shared shape `BenchState` and `CraftOffer` both carry.
/// The panel says *"Short 4.9 bone"*, never *"cannot craft"*, so the arithmetic is done sim-side and
/// this only carries it; `required` is already net of the bench tool's material efficiency.
fn shortfalls_to_array(
    shortfalls: Option<Vector<'_, ForwardsUOffset<fb::MaterialShortfall<'_>>>>,
) -> VarArray {
    let mut array = VarArray::new();
    if let Some(list) = shortfalls {
        for shortfall in list.iter() {
            let mut row = VarDictionary::new();
            let _ = row.insert("material_id", shortfall.materialId().unwrap_or(""));
            let _ = row.insert("required", shortfall.required() as f64);
            let _ = row.insert("held", shortfall.held() as f64);
            let _ = row.insert("short", shortfall.short() as f64);
            array.push(&row.to_variant());
        }
    }
    array
}

pub(crate) fn populations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::PopulationCohortState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for cohort in list {
        let dict = population_to_dict(cohort);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn generation_to_dict(state: fb::GenerationState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("bias_knowledge", fixed64_to_f64(state.biasKnowledge()));
    let _ = dict.insert("bias_trust", fixed64_to_f64(state.biasTrust()));
    let _ = dict.insert("bias_equity", fixed64_to_f64(state.biasEquity()));
    let _ = dict.insert("bias_agency", fixed64_to_f64(state.biasAgency()));
    dict
}

pub(crate) fn generations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GenerationState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in list {
        let dict = generation_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

#[cfg(test)]
mod cohort_decode_tests {
    use super::*;

    /// THE GAP THIS CLOSES: every preview/UI harness feeds `Hud` a hand-written fixture dict and so
    /// bypasses this decoder entirely. A cohort field can be decoded at the wrong SCALE — or never
    /// decoded at all — and every rendered frame still looks perfect; both have now reached the
    /// running client. `population_to_dict` itself is untestable here (its Godot `Dictionary` cannot
    /// be constructed outside a live engine), which is exactly why the fixed-point conversions live
    /// in `cohort_scalars`: plain Rust over a real FlatBuffer, so the wire scale can be pinned.
    ///
    /// The values are deliberately chosen so a MISSING divide is unmistakable rather than plausible:
    /// they are the numbers from the playtest that caught it, where the panel rendered "9292500
    /// children" for a band of thirty people.
    fn build_cohort(builder: &mut flatbuffers::FlatBufferBuilder<'_>) -> Vec<u8> {
        let cohort = fb::PopulationCohortState::create(
            builder,
            &fb::PopulationCohortStateArgs {
                // The age brackets are WHOLE PEOPLE and sum to `size` by the sim's construction.
                size: 30,
                childrenCount: 9,
                workingAge: 17,
                eldersCount: 4,
                morale: 820_000,
                // == the four Layer-1 contributions below, which the test asserts.
                moraleDelta: -11_000,
                outputMultiplier: 1_000_000,
                discontentFraction: 250_000,
                grievance: 40_000,
                moraleSettling: 10_000,
                moraleTerrain: -26_000,
                moraleClimate: -6_000,
                moraleUnrest: 11_000,
                // The three fertility factors: a band eating short (0.6) off a fat larder (1.5)
                // with its income collapsed (0.25) — the case the model exists for, and the one
                // where all three sit off their neutral 1.0.
                fertilityHunger: 600_000,
                fertilityReserve: 1_500_000,
                fertilityTrend: 250_000,
                ..Default::default()
            },
        );
        builder.finish(cohort, None);
        builder.finished_data().to_vec()
    }

    #[test]
    fn cohort_scalars_decode_fixed_point() {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let bytes = build_cohort(&mut builder);
        let cohort = flatbuffers::root::<fb::PopulationCohortState>(&bytes).expect("valid cohort");
        let scalars = cohort_scalars(cohort);

        // The age brackets are NOT here: they are whole `uint` people on the wire, read straight
        // off the cohort with no divide, and they partition `size` exactly.
        let people = cohort.childrenCount() + cohort.workingAge() + cohort.eldersCount();
        assert_eq!(
            people,
            cohort.size(),
            "age brackets sum to {people}, cohort size is {}",
            cohort.size()
        );

        assert!((scalars.morale - 0.82).abs() < 1e-9);
        assert!((scalars.morale_delta - -0.011).abs() < 1e-9);
        assert!((scalars.output_multiplier - 1.0).abs() < 1e-9);
        assert!((scalars.discontent_fraction - 0.25).abs() < 1e-9);
        assert!((scalars.grievance - 0.04).abs() < 1e-9);
        // The four signed Layer-1 contributions must sum to the reported morale trend.
        let contributions = scalars.morale_settling
            + scalars.morale_terrain
            + scalars.morale_climate
            + scalars.morale_unrest;
        assert!(
            (contributions - scalars.morale_delta).abs() < 1e-9,
            "contributions {contributions} != morale_delta {}",
            scalars.morale_delta
        );

        // The fertility factors are NEUTRAL AT 1.0 and combine by PRODUCT, not by sum — a band
        // eating short off a fat larder with collapsed income breeds at 0.6 x 1.5 x 0.25 = 22.5% of
        // the base rate. Decoding one raw would read a 250000x fertility bonus.
        assert!((scalars.fertility_hunger - 0.6).abs() < 1e-9);
        assert!((scalars.fertility_reserve - 1.5).abs() < 1e-9);
        assert!((scalars.fertility_trend - 0.25).abs() < 1e-9);
        let multiplier =
            scalars.fertility_hunger * scalars.fertility_reserve * scalars.fertility_trend;
        assert!(
            (multiplier - 0.225).abs() < 1e-9,
            "fertility multiplier {multiplier} != 0.225"
        );
    }

    /// A raw-`long` read would leave every one of these at 1e6 scale. This is the assertion that
    /// actually fails when someone adds a Scalar field and forgets the divide.
    #[test]
    fn cohort_scalars_are_never_wire_scale() {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let bytes = build_cohort(&mut builder);
        let cohort = flatbuffers::root::<fb::PopulationCohortState>(&bytes).expect("valid cohort");
        let scalars = cohort_scalars(cohort);
        for (name, value) in [
            ("morale", scalars.morale),
            ("morale_delta", scalars.morale_delta),
            ("output_multiplier", scalars.output_multiplier),
            ("discontent_fraction", scalars.discontent_fraction),
            ("grievance", scalars.grievance),
            ("fertility_hunger", scalars.fertility_hunger),
            ("fertility_reserve", scalars.fertility_reserve),
            ("fertility_trend", scalars.fertility_trend),
        ] {
            assert!(
                value.abs() < 1_000.0,
                "{name} decoded as {value} — that is wire scale, not real units (missing /1e6)"
            );
        }
    }
}
