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
/// one raw renders a 30-person band as "9292500 children" or a morale of 820000%.
///
/// It exists to be TESTABLE: `population_to_dict` returns a Godot `Dictionary`, which cannot be
/// constructed outside a running engine, so the dict itself is unreachable from `cargo test`. This
/// struct is plain Rust over a real FlatBuffer, so `cohort_scalars_decode_fixed_point` can pin the
/// scale of each field. **A new Scalar cohort field belongs here, not inlined at its insert site** —
/// inlined is exactly how the age cohorts shipped un-divided.
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
    age_children: f64,
    age_working: f64,
    age_elders: f64,
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
        age_children: fixed64_to_f64(cohort.children()),
        age_working: fixed64_to_f64(cohort.working()),
        age_elders: fixed64_to_f64(cohort.elders()),
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
            kit_item_conditions.push(&row.to_variant());
        }
    }
    let _ = dict.insert("kit_item_conditions", &kit_item_conditions);
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
            kit_tiers.push(&entry.to_variant());
        }
    }
    let _ = dict.insert("kit_tiers", &kit_tiers);
    // The RESOLVED tiers, so the client renders this band's real numbers instead of re-deriving them
    // from the durabilities plus a config it does not have. `hunter_attack` is the term the combat
    // gate `max(0, attack − defense)` compares against `HerdTelemetryState.defense`.
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
            // BOTH CURRENCIES, read as one vector beside their scalars: a wolf's food band is
            // honestly all-zero, so a food-only range could not state its take at all. Undecoded
            // until now — see the kit block in `population_to_dict` for the class of bug.
            let _ = entry.insert("actual_yield_low", assignment.actualYieldLow() as f64);
            let _ = entry.insert("actual_yield_high", assignment.actualYieldHigh() as f64);
            let _ = entry.insert("trade_yield_low", assignment.tradeYieldLow() as f64);
            let _ = entry.insert("trade_yield_high", assignment.tradeYieldHigh() as f64);
            // The per-source STEADY average: the honest long-run average of this source's lumpy
            // `actual_yield`. Headlines the Band panel row + map label so they don't swing turn-to-turn.
            let _ = entry.insert("realized_yield", assignment.realizedYield() as f64);
            // The TRADE twins (issue #337): `trade_yield` is this turn's actual trade from the source,
            // `realized_trade_yield` its steady forward-projected rate — the same actual/steady split
            // as the food pair above, in the currency a wolf hunt pays exclusively. They are NOT food
            // income (the cohort's `food_income` stays Σ actual_yield, which is what keeps the larder
            // identity closed for an inedible quarry); the client renders a trade component ONLY when
            // it is > 0. `realized_trade_yield` is 0 on every FORAGE source — the plant web's trade
            // PROJECTION is a documented sim-side gap, while the trade a gather actually earned does
            // ship in `trade_yield`.
            let _ = entry.insert("trade_yield", assignment.tradeYield() as f64);
            let _ = entry.insert(
                "realized_trade_yield",
                assignment.realizedTradeYield() as f64,
            );
            // The FEED currency (issue #449) — the third account beside the food and trade pairs
            // above, exactly the fodder the band's `FODDER` store was credited with. PLANT-ONLY:
            // no animal pays fodder, so a hunt row's `0` here is a structural zero rather than a
            // gap, and a sown hay Field (no provisions, no trade) states its whole product through
            // this key alone instead of reading `+0.00`.
            // **There is no `realized_fodder_yield` twin, deliberately.** `realized_trade_yield`
            // exists because the ANIMAL web projects a steady rate; fodder is paid by the PLANT web
            // alone, whose projection is the documented gap that already leaves `realized_trade_yield`
            // at 0 on every forage source — so a projected-fodder field would be a constant zero on
            // the only web that can pay it. This actual IS the honest rate, and the client reads it
            // with no fallback (`SourceForecast.fodder_rate_of`).
            let _ = entry.insert("fodder_yield", assignment.fodderYield() as f64);
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
    let _ = dict.insert("working_age", cohort.workingAge() as i64);
    // Age cohorts (children / working / elders head-counts). Deliberately prefixed `age_*` and NOT
    // named `working`/`working_age`: `workingAge` above is the count of ASSIGNABLE workers, a
    // different quantity, and a key collision between the two would be silent and awful.
    //
    // **These are SCALAR fixed-point (`PopulationCohort.children: Scalar`), not raw counts** — the
    // population is fractional in the sim — so they take `fixed64_to_f64` like `morale` above.
    // Reading the `long` raw renders a 30-person band as "9292500 children"; the ×1e6 is big enough
    // that it can only ever be a wire-scale mistake, never a plausible head-count.
    // NOTE the neighbouring `PopulationDemographicsState` children/working/elders are `uint` PLAIN
    // COUNTS (the faction-wide top-bar strip) — same three words, two different wire encodings.
    let _ = dict.insert("age_children", scalars.age_children);
    let _ = dict.insert("age_working", scalars.age_working);
    let _ = dict.insert("age_elders", scalars.age_elders);
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

    dict
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
                size: 30,
                children: 9_292_500,
                working: 16_537_500,
                elders: 4_642_500,
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

        // The age brackets are Scalar, NOT head-counts: 9_292_500 is 9.2925 people.
        assert!((scalars.age_children - 9.2925).abs() < 1e-9);
        assert!((scalars.age_working - 16.5375).abs() < 1e-9);
        assert!((scalars.age_elders - 4.6425).abs() < 1e-9);
        // ... and they describe the SAME band the cohort's own `size` reports.
        let people = scalars.age_children + scalars.age_working + scalars.age_elders;
        assert!(
            (people - f64::from(cohort.size())).abs() < 1.0,
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
            ("age_children", scalars.age_children),
            ("age_working", scalars.age_working),
            ("age_elders", scalars.age_elders),
            ("morale", scalars.morale),
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
