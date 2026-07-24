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
    }
}

fn population_to_dict(cohort: fb::PopulationCohortState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("entity", cohort.entity() as i64);
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
    // Fauna-pursuit sub-mode: "single" (one-shot) or "sustain"/"surplus"/"market"/
    // "eradicate" (follow policies); empty when the band isn't hunting. Mirrors `activity`.
    if let Some(hunt_mode) = cohort.huntMode() {
        let _ = dict.insert("hunt_mode", hunt_mode);
    }
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
            // The per-source STEADY average: the honest long-run average of this source's lumpy
            // `actual_yield`. Headlines the Band panel row + map label so they don't swing turn-to-turn.
            let _ = entry.insert("realized_yield", assignment.realizedYield() as f64);
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
            if let Some(policy) = assignment.policy() {
                let _ = entry.insert("policy", policy);
            }
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
    // markers are unaffected. (The persistence-only pending-reveal fields stay undecoded.)
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
    // Hunt expedition (PR 2, docs/plan_exploration_and_sites.md §2b): the herd a hunt party
    // follows (fauna_id string like "game_deer_57", mirrors LaborAssignment.faunaId); "" for a
    // scout expedition / normal band. `expedition_mission` also takes "hunt", `expedition_phase`
    // also takes "hunting"/"delivering" — same string fields already decoded above, new values.
    let _ = dict.insert(
        "expedition_target_herd",
        cohort.expeditionTargetHerd().unwrap_or(""),
    );
    // Hunt-party take policy (sustain|surplus|market|eradicate; "" for scouts/bands) + the carry
    // ceiling (party × per_worker_carry; 0 for scouts/bands). The hunt panel shows "Carried X / cap"
    // + a FULL state, and the launched party's policy.
    let _ = dict.insert(
        "expedition_hunt_policy",
        cohort.expeditionHuntPolicy().unwrap_or(""),
    );
    let _ = dict.insert(
        "expedition_carry_cap",
        f64::from(cohort.expeditionCarryCap()),
    );
    // In-flight hunt-party next-delivery forecast (the drawer's "Next delivery: ~X food in ~N turns"
    // line) — the in-flight twin of the pre-launch huntTripEstimates. 0 / 0.0 / false when n/a
    // (scout, normal band, or a raid with no finite ETA). See core_sim expedition_delivery.
    let _ = dict.insert(
        "expedition_eta_turns",
        i64::from(cohort.expeditionEtaTurns()),
    );
    let _ = dict.insert(
        "expedition_projected_delivery",
        f64::from(cohort.expeditionProjectedDelivery()),
    );
    let _ = dict.insert("expedition_recurring", cohort.expeditionRecurring());
    // Hard cap on party size the server enforces (from the expedition config, default 8). The
    // outfit stepper clamps its max to min(idle_workers, this) so the player can't dial an
    // over-cap party.
    let _ = dict.insert(
        "max_expedition_party_size",
        cohort.maxExpeditionPartySize() as i64,
    );
    // Global expedition/labor config echoed onto EVERY cohort (same idiom as
    // `max_expedition_party_size`). These are DISPLAY levers only — none of them is an input to an
    // expedition trip length. An expedition's turns-to-fill comes from the herd's
    // `hunt_trip_estimates` (decoded in `herds_to_array` above) and NOTHING ELSE: the sim
    // forward-simulates the trip (`hunt_trip_forecast`) and exports the ANSWER per (policy, party
    // size), so the client performs a PURE TABLE LOOKUP and does ZERO arithmetic for an expedition.
    // It must NEVER divide a carry cap by a take rate: the herd's state moves under the party and
    // its stock exhausts mid-trip, so any closed form drifts from the take the sim actually
    // performs. Pinned by core_sim/tests/expedition_hunt.rs.
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
        ] {
            assert!(
                value.abs() < 1_000.0,
                "{name} decoded as {value} — that is wire scale, not real units (missing /1e6)"
            );
        }
    }
}
