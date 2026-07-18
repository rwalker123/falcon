use super::*;

/// Capture a live `Herd` into its authoritative snapshot mirror for rollback (the inverse is
/// `fauna::HerdRegistry::update_from_states`). Movement/identity fields are mirrored directly; the
/// depletable-ecology subset goes into the embedded `EcologyState`. Coordinates cross as `(x, y)`.
pub(crate) fn herd_state(herd: &Herd) -> HerdState {
    HerdState {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.clone(),
        size_class: herd.size_class.as_str().to_string(),
        route: herd.route.iter().map(|p| (p.x, p.y)).collect(),
        step_index: herd.step_index as u32,
        current_pos: (herd.current_pos.x, herd.current_pos.y),
        dwell_remaining: herd.dwell_remaining,
        roam: HerdRoamState {
            mode: herd.roam.mode_key().to_string(),
            loiter_turns_left: herd.roam.loiter_turns_left(),
        },
        next_pos: herd.next_pos.map(|p| (p.x, p.y)),
        corralled_at: herd.corralled_at.map(|p| (p.x, p.y)),
        corral_progress: herd.corral_progress,
        pen_radius: herd.pen_radius,
        pen_extend_progress: herd.pen_extend_progress,
        pen_extending: herd.pen_extending,
        fodder_per_biomass: herd.fodder_per_biomass,
        regrowth_rate: herd.regrowth_rate,
        body_mass: herd.body_mass,
        hunt_credit: herd.hunt_credit,
        husbandry_ceiling: herd.husbandry_ceiling.as_str().to_string(),
        ecology: EcologyState {
            biomass: herd.biomass,
            carrying_capacity: herd.carrying_capacity,
            ecology_phase: herd.ecology_phase.as_str().to_string(),
            progress: herd.domestication_progress,
            owner: herd.owner.map(|f| f.0),
        },
    }
}

/// Capture a live `ForagePatch` into its authoritative snapshot mirror for rollback (the inverse is
/// `forage::ForageRegistry::update_from_states`). The depletable-ecology subset — including
/// cultivation (`progress`/`owner`, Phase 1a) — goes into the shared `EcologyState`, mirroring
/// `herd_state`. Coordinates cross as the `(x, y)` tile key.
pub(crate) fn forage_state(patch: &ForagePatch) -> ForageState {
    ForageState {
        x: patch.tile.x,
        y: patch.tile.y,
        field_progress: patch.field_progress,
        ecology: EcologyState {
            biomass: patch.biomass,
            carrying_capacity: patch.carrying_capacity,
            ecology_phase: patch.ecology_phase.as_str().to_string(),
            progress: patch.cultivation_progress,
            owner: patch.owner.map(|f| f.0),
        },
    }
}

/// Capture a live `GrazePatch` into its authoritative snapshot mirror for rollback (the inverse is
/// `graze::GrazeRegistry::update_from_states`). Mirrors `forage_state`; graze is **wild ground**, so
/// the shared `EcologyState`'s cultivation fields (`progress`/`owner`) stay at their defaults.
pub(crate) fn graze_state(patch: &GrazePatch) -> GrazeState {
    GrazeState {
        x: patch.tile.x,
        y: patch.tile.y,
        ecology: EcologyState {
            biomass: patch.biomass,
            carrying_capacity: patch.carrying_capacity,
            ecology_phase: patch.ecology_phase.as_str().to_string(),
            progress: 0.0,
            owner: None,
        },
    }
}

/// The compact per-tile pasture-phase code the client reads off `TileState` (`GRAZE_PHASE_*`).
/// A tile with **no patch** (a biome that carries no pasture: water, ice, bare rock) is
/// [`GRAZE_PHASE_NONE`] — the zero/default, so an absent pasture can never be misread as a healthy one.
pub(crate) fn graze_phase_code(patch: Option<&GrazePatch>) -> u8 {
    match patch.map(|patch| patch.ecology_phase) {
        None => GRAZE_PHASE_NONE,
        Some(EcologyPhase::Thriving) => GRAZE_PHASE_THRIVING,
        Some(EcologyPhase::Stressed) => GRAZE_PHASE_STRESSED,
        Some(EcologyPhase::Collapsing) => GRAZE_PHASE_COLLAPSING,
    }
}

pub(crate) fn snapshot_sedentarization(
    score: &SedentarizationScore,
) -> Vec<SchemaSedentarizationState> {
    score
        .iter_sorted()
        .into_iter()
        .map(|(faction, entry)| SchemaSedentarizationState {
            faction: faction.0,
            score: entry.score,
            stage: entry.stage.as_str().to_string(),
        })
        .collect()
}

/// Every **Hunt** policy's per-turn **BAND / local-hunt** ceiling for one herd's current state, in
/// provisions — the worker-independent half of the client's local-hunt yield preview. It is a pure
/// projection of the herd's `SourceYieldForecast` (`fauna::hunt_forecast` — the same ceiling +
/// biomass→provisions helpers `hunt_take` pays with, so forecast == actual), NOT a second derivation:
/// the list rows and the scalar `ceiling*` fields below are literally the same numbers, so they cannot
/// drift.
///
/// Walks [`FollowPolicy::HUNT_POLICIES`] — the four extractive rungs **plus `Corral`** (a legitimate
/// Hunt policy whose dipped yield is exactly what a player must see *before* committing to the pen).
/// `Cultivate` is Forage-only, so a herd has no cultivate row. Because the rows come from the
/// forecast, `Corral` is automatically **phase-correct**: the `animal:pen` rung's
/// `yield_fraction_while_building × MSY` dip
/// while the pen is being built, and the full corral yield once the herd `is_corralled()` (the
/// forecast reports a penned herd as `SourceYieldForecast::tended`, every ceiling = the managed yield).
///
/// The **expedition** has no ceiling field: a hunting party's trip is not `cap / rate` (see
/// `hunt_trip_forecast`), so the sim exports the *answer* instead — `hunt_trip_estimate_entries`.
pub(crate) fn hunt_policy_ceiling_entries(
    forecast: &SourceYieldForecast,
) -> Vec<HuntPolicyCeilingState> {
    FollowPolicy::HUNT_POLICIES
        .iter()
        .map(|&policy| HuntPolicyCeilingState {
            policy: policy.as_str().to_string(),
            provisions_per_turn: forecast.ceiling_for(policy),
        })
        .collect()
}

/// The **pre-launch hunt-trip estimate table** for one herd: `hunt_trip_forecast` run for every
/// policy × every legal party size (`1..=expedition.max_party_size`), so the client's outfit UI is a
/// pure **table lookup** — zero arithmetic, zero ecology model. The forecast is a bounded forward
/// simulation of the greedy raid (grab the standing surplus, come home), which has no single per-turn
/// rate to divide by, and each row now carries both `turns_to_fill` (turns until the raid completes)
/// and `animals_taken` (the payload the client headlines).
///
/// Cost is bounded by construction: `policies × max_party_size × hunt.forecast_horizon_turns`
/// turn-steps per herd, and only **huntable** herds are estimated. In practice a raid is **short** —
/// it grabs the surplus and terminates — so a snapshot's worth of raids simulates cheaply (the old
/// O(1) "cannot fill" short-circuit was retired with the raid: its premise, "won't fill the pack ⇒
/// doomed", is inverted by a raid, where "won't fill the pack" is the normal successful short trip).
pub(crate) fn hunt_trip_estimate_entries(
    herd: &Herd,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> Vec<HuntTripEstimateState> {
    let mut entries =
        Vec::with_capacity(FollowPolicy::EXTRACTIVE.len() * expedition.max_party_size as usize);
    // The four **extractive** rungs only. The investment policies (Cultivate/Corral) are place-bound
    // work a resident band does — `send_hunt_expedition` rejects them — so a trip estimate for one
    // would be a number for a trip that cannot be launched (and would inflate this table for nothing).
    for &policy in FollowPolicy::EXTRACTIVE.iter() {
        for party_workers in 1..=expedition.max_party_size {
            let forecast =
                hunt_trip_forecast(party_workers, herd, policy, fauna, labor, expedition);
            entries.push(HuntTripEstimateState {
                policy: policy.as_str().to_string(),
                party_workers,
                // `0` = the raid never completes within `hunt.forecast_horizon_turns`.
                turns_to_fill: forecast.turns_to_fill.unwrap_or(0),
                delivers_food: forecast.delivers_food,
                animals_taken: forecast.animals_taken,
            });
        }
    }
    entries
}

/// Display herd telemetry for the client, plus each herd's **pre-commit yield forecast**
/// (`fauna::hunt_forecast` — the same ceiling/conversion helpers `hunt_take` pays with, so
/// forecast == actual) and its **pre-launch expedition trip estimates**. All three need the herd's
/// *carrying capacity*, which the display telemetry doesn't carry, so the live `Herd` is resolved
/// from the authoritative `HerdRegistry` by id (a herd that vanished between the two — not possible
/// in the capture, both are read in the same frame — reports a zeroed forecast and no rows).
/// Captured at `output_multiplier = 1.0`: the client scales by the acting band's `outputMultiplier`.
///
/// The scalar `ceiling*` fields and the `hunt_policy_ceilings` list are two views of the **same**
/// `SourceYieldForecast` — one forecast per herd, projected twice — so they can never disagree.
#[allow(clippy::too_many_arguments)] // every config the exported forecast reads is a lever
pub(crate) fn herd_snapshot_entries(
    telemetry: &HerdTelemetry,
    registry: &HerdRegistry,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
    grid_size: UVec2,
    wrap_horizontal: bool,
) -> Vec<HerdTelemetryState> {
    let width = grid_size.x.max(1);
    let height = grid_size.y.max(1);
    telemetry
        .entries
        .iter()
        .map(|entry| {
            let herd = registry.find(&entry.id);
            let forecast = herd
                .map(|herd| {
                    hunt_forecast(
                        herd,
                        fauna,
                        ladder,
                        labor.hunt.per_worker_biomass_capacity,
                        FORECAST_OUTPUT_MULTIPLIER,
                    )
                })
                .unwrap_or_default();
            HerdTelemetryState {
                id: entry.id.clone(),
                label: entry.label.clone(),
                species: entry.species.clone(),
                x: entry.position.x,
                y: entry.position.y,
                biomass: entry.biomass,
                route_length: entry.route_length,
                next_x: entry.next_position.map(|pos| pos.x as i32).unwrap_or(-1),
                next_y: entry.next_position.map(|pos| pos.y as i32).unwrap_or(-1),
                size_class: entry.size_class.clone(),
                huntable: entry.huntable,
                ecology_phase: entry.ecology_phase.clone(),
                domestication: entry.domestication,
                corralled: entry.corralled,
                corral_progress: entry.corral_progress,
                per_worker_yield: forecast.per_worker_yield,
                ceiling_sustain: forecast.ceiling_sustain,
                ceiling_surplus: forecast.ceiling_surplus,
                ceiling_market: forecast.ceiling_market,
                ceiling_eradicate: forecast.ceiling_eradicate,
                // The Corral investment rung: the preparing dip + the (gross) payoff once penned.
                ceiling_corral: forecast.ceiling_prepare,
                corral_yield: forecast.managed_yield,
                // The pen as a managed population: what it EATS, and whether its keeper is paying.
                // `pen_upkeep` is answered for EVERY herd — a projection ("what would this pen cost to
                // feed?") for an unpenned one, the live demand for a penned one — on the same biomass
                // basis as `corral_yield`, so the pre-commit Corral row can show the running cost next
                // to the payoff. `pen_fed_fraction` is the value the keeper's tend branch wrote this
                // turn (Population runs before the capture), so the client reads the CURRENT turn's
                // feeding, and `1.0` for anything unpenned.
                pen_upkeep: herd.map(|herd| pen_upkeep(herd, fauna)).unwrap_or(0.0),
                pen_fed_fraction: herd
                    .map(|herd| herd.pen_fed_fraction)
                    .unwrap_or(PEN_FULLY_FED),
                // The same forecast, projected as the per-policy BAND ceiling table (incl. Corral).
                hunt_policy_ceilings: herd
                    .map(|_| hunt_policy_ceiling_entries(&forecast))
                    .unwrap_or_default(),
                // Only a huntable herd can be the target of a trip — don't pay for the rest.
                hunt_trip_estimates: herd
                    .filter(|_| entry.huntable)
                    .map(|herd| hunt_trip_estimate_entries(herd, fauna, labor, expedition))
                    .unwrap_or_default(),
                // Grazing 2b-iii: the herd's live derived K, and the exact hex radius the sim
                // grazes/derives K over (migratory `loiter_radius` resolved via `species_by_display`,
                // exactly as `advance_herds` does; an unresolved species falls back to the loiter
                // default). A vanished herd (unreachable here) reports the neutral 0 / 0.
                carrying_capacity: herd.map(|herd| herd.carrying_capacity).unwrap_or(0.0),
                graze_range_radius: herd
                    .map(|herd| herd.graze_range_radius(fauna.species_by_display(&herd.species)))
                    .unwrap_or(0),
                // The pen economy (Grazing 2d). `penFootprintTiles` is the SERVER's in-bounds count of
                // the fenced footprint (not the closed-form disk, which is wrong at map edges); `0` for
                // an unpenned herd. `pen_pasture_fraction` is transient per-turn scratch (Population ran
                // before this capture, so it reflects the current turn); `pen_extend_progress` is
                // authoritative, snapshot-persisted `Herd` state (the in-flight ExtendPen ring meter,
                // rollback-safe) — here it just crosses to the client wire alongside it.
                pen_radius: herd.map(|herd| herd.pen_radius).unwrap_or(0),
                pen_footprint_tiles: herd
                    .and_then(|herd| {
                        herd.corralled_at.map(|anchor| {
                            crate::grid_utils::hex_range_tiles(
                                anchor,
                                herd.pen_radius,
                                width,
                                height,
                                wrap_horizontal,
                            )
                            .len() as u32
                        })
                    })
                    .unwrap_or(0),
                pen_pasture_fraction: herd.map(|herd| herd.pen_pasture_fraction).unwrap_or(0.0),
                pen_extend_progress: herd.map(|herd| herd.pen_extend_progress).unwrap_or(0.0),
                // Husbandry ceiling (Grazing 2d-δ) — the client hides the corral/extend affordance on a
                // non-`pen` herd and the domestication track on a `wild` one.
                husbandry_ceiling: herd
                    .map(|herd| herd.husbandry_ceiling.as_str().to_string())
                    .unwrap_or_default(),
                // Body mass (slice 8b) — the client turns a per-turn rate into a kill-rhythm with it.
                body_mass: herd.map(|herd| herd.body_mass).unwrap_or(0.0),
                // One animal's worth of yield in provisions (slice 8b) — the rhythm's numerator
                // (`food_per_animal / sustainable_yield`), already converted the same way every other
                // yield field is.
                food_per_animal: forecast.body_mass_yield,
            }
        })
        .collect()
}

/// Per-tile depletable-forage cultivation/ecology display state (Intensification Phase 1a) for the
/// client tile card, plus each patch's **pre-commit yield forecast** (`forage::forage_forecast` —
/// the same ceiling/conversion helpers `forage_take` pays with, so forecast == actual). One entry per
/// live `ForagePatch`, emitted in a stable `(y, x)` order so the snapshot is deterministic (the
/// `ForageRegistry` map iteration order is not). `owner` crosses as the tending faction's `u32`
/// (`None` for a wild/untended patch).
///
/// `sow_site_refusals` maps tile coord → **why the `plant:field` rung refuses that ground**, resolved
/// by the caller (which has the tiles and the hydrology) through the one shared
/// `RungSiteRequirement::refusal` seam. **Absent = the land takes seed** — the same
/// absent-means-nothing convention `seasonal_weights` uses.
///
/// `seasonal_weights` maps tile coord → that tile's `FoodModuleTag::seasonal_weight`, folded into the
/// forecast's per-worker throughput exactly as the Forage labor arm folds it into `forage_take`. A
/// patch whose tile carries no food module forecasts at [`NO_FORAGE_SEASON`] — no per-worker gather at
/// all, which is exactly what such a tile offers. **That is a reachable state since slice 5**: `Sow`
/// places a Field on any ground the `plant:field` rung's `site_requirement` accepts — module or not —
/// and a Field's managed harvest is biomass-based and seasonless, so it forecasts correctly regardless. Captured at
/// `output_multiplier = 1.0`: the client scales by the acting band's `outputMultiplier`.
pub(crate) fn snapshot_forage_patches(
    registry: &ForageRegistry,
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    seasonal_weights: &HashMap<UVec2, f32>,
    sow_site_refusals: &HashMap<UVec2, SiteRefusal>,
) -> Vec<ForagePatchState> {
    let mut patches: Vec<ForagePatchState> = registry
        .patches
        .values()
        .map(|patch| {
            let seasonal = seasonal_weights
                .get(&patch.tile)
                .copied()
                .unwrap_or(NO_FORAGE_SEASON);
            let forecast =
                forage_forecast(patch, forage, ladder, seasonal, FORECAST_OUTPUT_MULTIPLIER);
            ForagePatchState {
                x: patch.tile.x,
                y: patch.tile.y,
                cultivation_progress: patch.cultivation_progress,
                is_cultivated: patch.is_cultivated(),
                owner: patch.owner.map(|faction| faction.0),
                biomass: patch.biomass,
                carrying_capacity: patch.carrying_capacity,
                ecology_phase: patch.ecology_phase.as_str().to_string(),
                per_worker_yield: forecast.per_worker_yield,
                ceiling_sustain: forecast.ceiling_sustain,
                ceiling_surplus: forecast.ceiling_surplus,
                ceiling_market: forecast.ceiling_market,
                ceiling_eradicate: forecast.ceiling_eradicate,
                // The Cultivate investment rung: the preparing dip + the payoff once cultivated.
                ceiling_cultivate: forecast.ceiling_prepare,
                tended_yield: forecast.managed_yield,
                // The Sow rung (plant 3): its own two meters — independent of cultivation's, since a
                // Field may stand on ground that was never tended — and its own preparing/payoff
                // pair. `field_provisions` is the same helper the labor arm pays a Field with, so the
                // client's "then Y" is the number the sim will hand over.
                field_progress: patch.field_progress,
                is_field: patch.is_field(),
                ceiling_sow: forecast.ceiling_sow,
                field_yield: field_provisions(patch.biomass, forage, FORECAST_OUTPUT_MULTIPLIER),
                // **Why this ground will not take seed** — resolved by the caller through the *same*
                // `RungSiteRequirement::refusal` seam the `sow` command and the labor arm gate on, so
                // the wire cannot disagree with the gate. Absent from the map = the land takes seed
                // (`SITE_ACCEPTED`), mirroring `seasonal_weights`' absent-means-none convention.
                sow_site_refusal: sow_site_refusals
                    .get(&patch.tile)
                    .map_or(SITE_ACCEPTED, |refusal| refusal.as_str())
                    .to_string(),
            }
        })
        .collect();
    patches.sort_unstable_by_key(|patch| (patch.y, patch.x));
    patches
}

/// Per-faction intensification-ladder knowledge for the client's learning/known meters — one field
/// per rung-transition: Cultivation (2003) → Seed Selection (2005) up the plant ladder, Herding
/// (2004) → Penning (2006) up the animal one. Iterates the ledger's factions in sorted order; a
/// faction is emitted only when it has begun learning **something** (all zero → skipped), mirroring
/// how `discovery_progress_entries` skips empty progress.
pub(crate) fn snapshot_intensification_knowledge(
    ledger: &DiscoveryProgressLedger,
) -> Vec<IntensificationKnowledgeState> {
    let mut factions: Vec<u32> = ledger.progress.keys().map(|faction| faction.0).collect();
    factions.sort_unstable();
    factions.dedup();
    factions
        .into_iter()
        .filter_map(|faction_id| {
            let faction = FactionId(faction_id);
            let cultivation = ledger
                .get_progress(faction, CULTIVATION_DISCOVERY_ID)
                .to_f32();
            let herding = ledger.get_progress(faction, HERDING_DISCOVERY_ID).to_f32();
            let seed_selection = ledger
                .get_progress(faction, SEED_SELECTION_DISCOVERY_ID)
                .to_f32();
            let penning = ledger.get_progress(faction, PENNING_DISCOVERY_ID).to_f32();
            // A rung-3 knowledge cannot be positive while its rung-2 gate is zero (you cannot work a
            // tended patch you never cultivated), so this stays equivalent to the old
            // cultivation/herding-only check — but stating every meter keeps it true if a later slice
            // grants one another way.
            if cultivation <= 0.0 && herding <= 0.0 && seed_selection <= 0.0 && penning <= 0.0 {
                return None;
            }
            Some(IntensificationKnowledgeState {
                faction: faction_id,
                cultivation,
                herding,
                seed_selection,
                penning,
            })
        })
        .collect()
}
