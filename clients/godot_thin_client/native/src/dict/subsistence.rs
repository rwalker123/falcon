//! `subsistence` section -- herds, forage patches, food modules, and the
//! intensification/sedentarization readouts built on them.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

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
        // Per-policy BAND / local-hunt take ceilings (provisions/turn) for this herd's CURRENT state.
        // Surfaced as a `{policy -> provisions_per_turn}` Dictionary. With the cohort's
        // `hunt_per_worker_provisions` + `output_multiplier` this is everything the RESIDENT-BAND hunt
        // preview needs as pure arithmetic (`Hud._local_hunt_preview_bbcode`) — the client must never
        // re-derive the ecology model itself.
        if let Some(ceilings) = herd.huntPolicyCeilings() {
            let mut ceiling_dict = VarDictionary::new();
            for ceiling in ceilings {
                if let Some(policy) = ceiling.policy() {
                    let _ = ceiling_dict.insert(policy, f64::from(ceiling.provisionsPerTurn()));
                }
            }
            let _ = dict.insert("hunt_policy_ceilings", &ceiling_dict);
        }
        // The sim's PRE-LAUNCH TRIP ESTIMATES for a hunting EXPEDITION against this herd — one entry
        // per (policy × party size). An expedition's trip length is NOT a rate division: for
        // Surplus/Market the per-policy ceiling is a *stock*, so the party strips the headroom in a
        // turn or two and then crawls at the herd's regrowth trickle (on a full Rabbit Warren under
        // Surplus only a LONE hunter fills at all — 23 turns; a party of 4 never fills within the
        // horizon, and under Sustain no party size fills at any size). The sim therefore
        // forward-simulates the trip and exports the ANSWER; the client does ZERO arithmetic — a pure
        // table lookup keyed
        // `"<policy>:<party_workers>"` →
        // `{turns_to_fill, delivers_food, animals_taken, delivered_food, wasted_food}`:
        //   turns_to_fill == 0   → the raid ran the whole horizon still delivering (a long raid)
        //   delivers_food false  → eradicate, a denial mission ("no food delivered", never an ETA)
        //   delivered_food == 0  → herd at/below the policy floor, no surplus to raid (too lean); NOT
        //                          animals_taken == 0, which is ≥ 1 whenever there's surplus (a small
        //                          party kills one animal and wastes the meat it can't carry)
        // Empty for a non-huntable herd (the HUD then shows no forecast).
        if let Some(estimates) = herd.huntTripEstimates() {
            let mut estimate_dict = VarDictionary::new();
            for estimate in estimates {
                if let Some(policy) = estimate.policy() {
                    let mut entry = VarDictionary::new();
                    let _ = entry.insert("turns_to_fill", i64::from(estimate.turnsToFill()));
                    let _ = entry.insert("delivers_food", estimate.deliversFood());
                    // The whole animals the raid delivers — the payload the client headlines
                    // ("delivers ≈N animals over M turns") and the plateau it caps the party stepper
                    // at. Dropped from this dict on four prior appended fields; this is the newest.
                    let _ = entry.insert("animals_taken", i64::from(estimate.animalsTaken()));
                    // The PRIMARY payload: food the party actually LANDS over the raid (a partial
                    // for a small party) + the food killed-but-not-carried it wastes. The client
                    // headlines `delivered_food` (NOT animals × food_per_animal, which overstates a
                    // partial) and shows the waste fraction `wasted / (delivered + wasted)` beside
                    // it; `delivered_food == 0` (not `animals_taken == 0`) is now "too lean to raid".
                    let _ = entry.insert("delivered_food", f64::from(estimate.deliveredFood()));
                    let _ = entry.insert("wasted_food", f64::from(estimate.wastedFood()));
                    let key = format!("{}:{}", policy, estimate.partyWorkers());
                    let _ = estimate_dict.insert(key, &entry);
                }
            }
            let _ = dict.insert("hunt_trip_estimates", &estimate_dict);
        }
        let _ = dict.insert("corralled", herd.corralled());
        // Pen-construction meter 0..1 accrued while a keeper band works this herd under the Corral
        // policy — the animal twin of `ForagePatchState.cultivationProgress`. Read by Hud's herd
        // drawer for the "Corral: Building N%" row.
        let _ = dict.insert("corral_progress", herd.corralProgress());
        // Pre-commit yield forecast (food/turn at the herd's CURRENT biomass, exported at
        // output_multiplier 1.0 — the client scales by the acting band's multiplier):
        //   expected(workers, policy) = min(workers * per_worker_yield, hunt_policy_ceilings[policy])
        //   max_useful_workers(policy) = ceil(hunt_policy_ceilings[policy] / per_worker_yield)
        // Read by Hud's %HerdAssignControls to show the expected yield live and to cap the
        // hunter stepper at what the herd can actually absorb. EVERY herd-side ceiling now comes
        // from the `hunt_policy_ceilings` Dictionary above — the old per-policy scalars
        // (ceilingSustain/Surplus/Market/Eradicate/Corral) are deprecated schema slots and are no
        // longer decoded. (ForagePatchState keeps its scalars: a patch has no such list.)
        let _ = dict.insert("per_worker_yield", herd.perWorkerYield());
        // `corral_yield` is the Corral rung's PAYOFF — what the herd pays once penned. Its
        // DURING-BUILDING dip rides the `hunt_policy_ceilings` list (the "corral" row), so together
        // they drive the pre-commit "Preparing: +X → then +Y" forecast on %HerdAssignControls.
        // `corral_yield` is GROSS — the pen's feed below is a separate debit on the keeper's larder.
        let _ = dict.insert("corral_yield", herd.corralYield());
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
        // Staffing of a MANAGED herd (intensification ladder). A domesticated herd needs
        // `herders_needed` herders every turn to HOLD its tameness; `herded_fraction` = min(1,
        // assigned / needed) is how well that demand is met. Understaffed (< 1) means the herd's
        // domestication is DECAYING — it slips back to wild and stops earning Penning — so the herd
        // drawer surfaces the deficit. `herders_needed` is 0 for a wild/unmanaged herd (never show a
        // herder readout then); `herded_fraction` defaults to 1.0 for any unmanaged/vanished herd.
        let _ = dict.insert("herders_needed", i64::from(herd.herdersNeeded()));
        let _ = dict.insert("herded_fraction", herd.herdedFraction());
        // The Tame rung's PAYOFF — the pastoral twin of `corral_yield`: food/turn a Sustain hunt pays
        // ONCE this herd is tamed (the pastoral MSY). While Tame's DURING-BUILDING dip rides the
        // `hunt_policy_ceilings` list, this is the "then +Y" the client shows so Tame reads as
        // `→ +pastoral_yield` (like Cultivate/Sow/Corral) instead of quoting only the dip. Sustain <
        // Tame < Corral. Appended-field audit: this is the newest slot on HerdTelemetryState.
        let _ = dict.insert("pastoral_yield", herd.pastoralYield());
        array.push(&dict.to_variant());
    }
    array
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
        // Pre-commit yield forecast — identical contract to the herd fields above (food/turn at
        // the patch's CURRENT biomass, at output_multiplier 1.0). MapView cross-refs these onto
        // `tile_info` (as `patch_*`) so %ForageAssignControls can forecast + cap the stepper.
        let _ = dict.insert("per_worker_yield", patch.perWorkerYield());
        let _ = dict.insert("ceiling_sustain", patch.ceilingSustain());
        let _ = dict.insert("ceiling_surplus", patch.ceilingSurplus());
        let _ = dict.insert("ceiling_market", patch.ceilingMarket());
        let _ = dict.insert("ceiling_eradicate", patch.ceilingEradicate());
        // The Cultivate INVESTMENT rung (forage-only): `ceiling_cultivate` is the food/turn the patch
        // pays WHILE it is being prepared (the deliberate dip), `tended_yield` what it pays once
        // cultivated. MapView cross-refs both onto `tile_info` (as `patch_*`) for the pre-commit
        // "Preparing: +X → then +Y" forecast on %ForageAssignControls.
        let _ = dict.insert("ceiling_cultivate", patch.ceilingCultivate());
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
        let _ = dict.insert("ceiling_sow", patch.ceilingSow());
        let _ = dict.insert("field_yield", patch.fieldYield());
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
