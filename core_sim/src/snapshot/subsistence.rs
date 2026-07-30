use super::*;
use crate::fauna_config::HuntYield;
use crate::forage::{
    field_fodder, field_trade_goods, plant_policy_forecasts, tended_fodder, tended_trade_goods,
};
use sim_schema::ForagePolicyCeilingState;

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
/// Walks [`crate::fauna::hunt_policies_for`] — normally [`FollowPolicy::HUNT_POLICIES`], the four extractive
/// rungs **plus the two investment rungs `Tame` and `Corral`** (legitimate Hunt policies whose dipped
/// yield is exactly what a player must see *before* committing to taming the herd or building the
/// pen). That helper is the **one seam** this export and the `assign_labor` validator share, so the
/// picker the client draws and the picker the sim accepts cannot become two lists; today it prunes
/// only the degenerate `yields_nothing` species (Eradicate alone).
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
    hunt_yield: HuntYield,
) -> Vec<HuntPolicyCeilingState> {
    crate::fauna::hunt_policies_for(hunt_yield)
        .iter()
        .map(|&policy| HuntPolicyCeilingState {
            policy: policy.as_str().to_string(),
            provisions_per_turn: forecast.ceiling_for(policy).provisions,
            // **The row is a PAIR, not a food scalar** (#337): an inedible species reads `0` food
            // here with a strictly positive trade rate, which a food-only row could not express.
            trade_goods_per_turn: forecast.ceiling_for(policy).trade_goods,
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
    // Intersected with the species' offered ladder (`crate::fauna::hunt_policies_for`, the shared picker
    // seam), so a `yields_nothing` quarry estimates its one legal rung and no more.
    let offered = crate::fauna::hunt_policies_for(fauna.hunt_yield_for(&herd.species));
    for &policy in FollowPolicy::EXTRACTIVE
        .iter()
        .filter(|p| offered.contains(p))
    {
        for party_workers in 1..=expedition.max_party_size {
            let forecast =
                hunt_trip_forecast(party_workers, herd, policy, fauna, labor, expedition);
            entries.push(HuntTripEstimateState {
                policy: policy.as_str().to_string(),
                party_workers,
                // `0` = the raid never completes within `hunt.forecast_horizon_turns`.
                turns_to_fill: forecast.turns_to_fill.unwrap_or(0),
                delivers_food: forecast.delivers_food,
                delivers_trade: forecast.delivers_trade,
                animals_taken: forecast.animals_taken,
                delivered_food: forecast.delivered_food,
                delivered_trade: forecast.delivered_trade,
                wasted_food: forecast.wasted_food,
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
/// The `hunt_policy_ceilings` list is the single wire view of a herd's per-policy ceilings — one
/// `SourceYieldForecast` per herd, projected once, keyed by a free-form policy name (the old scalar
/// `ceiling*` fields are retired `(deprecated)` slots).
///
/// **The list is FOG-FILTERED for the viewer faction** — see [`HerdSnapshotInputs::herd_is_visible`].
pub(crate) struct HerdSnapshotInputs<'a> {
    pub(crate) telemetry: &'a HerdTelemetry,
    pub(crate) registry: &'a HerdRegistry,
    pub(crate) fauna: &'a FaunaConfig,
    pub(crate) ladder: &'a LadderConfig,
    pub(crate) labor: &'a LaborConfig,
    pub(crate) expedition: &'a ExpeditionConfig,
    pub(crate) grid_size: UVec2,
    pub(crate) wrap_horizontal: bool,
    /// The same ledger `visibility_raster_from_ledger` renders the client's fog from, read for the
    /// same faction — so a herd can never be drawn on a tile the raster paints black.
    pub(crate) visibility: &'a crate::visibility::VisibilityLedger,
    pub(crate) viewer: FactionId,
    /// `SimulationConfig::fog_enabled` — the server-owned fog-of-war master switch. When it is off
    /// the filter below is a no-op, which is the ONLY way to reveal hidden fauna: unseen herds never
    /// reach the wire, so no client render flag could put them back.
    pub(crate) fog_enabled: bool,
}

impl HerdSnapshotInputs<'_> {
    /// **Wire-level fog for fauna.** A herd reaches the client's display telemetry iff the viewer can
    /// *see where it is standing right now* (`VisibilityState::Active`), **or the viewer owns it**.
    ///
    /// - **`Active`, not `Discovered`.** Ground you saw two hundred turns ago says nothing about
    ///   where a herd wanders today, so `Discovered` would leak live positions across the whole
    ///   explored map — the leak this filter exists to close. Remembering the *last seen* herd is a
    ///   separate, deliberate feature (issue #214) built on top of this, not a weaker filter.
    /// - **Ownership is not a leak.** A tamed or penned herd is your property; you know where your
    ///   animals are. Without this clause a pastoral herd drifting a hex out of sight would take its
    ///   `corralProgress` / `penFedFraction` starving warning with it, and a pen alert that vanishes
    ///   because of fog is a bug, not fog.
    /// - **Fails CLOSED.** An absent faction map (before the first `calculate_visibility`, or the
    ///   turn after a rollback clears the ledger) hides every herd — which is exactly what
    ///   `visibility_raster_from_ledger` does in the same state: it emits an all-unexplored raster,
    ///   so the client is rendering a black map anyway. The two agree by construction.
    ///
    /// A herd the registry cannot resolve has no owner to check, so it is judged on visibility alone.
    ///
    /// With `fog_enabled == false` every herd passes: `visibility_raster_from_ledger` returns an
    /// all-Active raster in the same state, so the list and the raster still agree by construction.
    fn herd_is_visible(&self, herd: Option<&Herd>, pos: UVec2) -> bool {
        if !self.fog_enabled {
            return true;
        }
        self.visibility.is_visible(self.viewer, pos.x, pos.y)
            || herd.is_some_and(|herd| herd.owner == Some(self.viewer))
    }
}

pub(crate) fn herd_snapshot_entries(inputs: HerdSnapshotInputs<'_>) -> Vec<HerdTelemetryState> {
    let HerdSnapshotInputs {
        telemetry,
        registry,
        fauna,
        ladder,
        labor,
        expedition,
        grid_size,
        wrap_horizontal,
        ..
    } = inputs;
    let width = grid_size.x.max(1);
    let height = grid_size.y.max(1);
    telemetry
        .entries
        .iter()
        .filter_map(|entry| {
            // ONE registry resolution per herd, shared by the fog gate and the export below —
            // `HerdRegistry::find` is a linear scan, so resolving it twice doubled the work.
            let herd = registry.find(&entry.id);
            inputs
                .herd_is_visible(herd, entry.position)
                .then_some((entry, herd))
        })
        .map(|(entry, herd)| {
            // The species row backing this herd — resolved once for the raw combat components below.
            let species_def = fauna.species_by_display(&entry.species);
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
            // The heading arrow points at the herd's NEXT hex, which is a second, independent tile —
            // so it is fog-filtered on its own terms through the same rule, or a visible herd on the
            // edge of your sight would hand you a free look at where it is going. `-1` (the existing
            // "no heading" sentinel the client already renders as no arrow) covers both "loitering"
            // and "you cannot see that far", which the client has no reason to distinguish.
            let next_position = entry
                .next_position
                .filter(|pos| inputs.herd_is_visible(herd, *pos));
            HerdTelemetryState {
                id: entry.id.clone(),
                label: entry.label.clone(),
                species: entry.species.clone(),
                x: entry.position.x,
                y: entry.position.y,
                biomass: entry.biomass,
                route_length: entry.route_length,
                next_x: next_position.map(|pos| pos.x as i32).unwrap_or(-1),
                next_y: next_position.map(|pos| pos.y as i32).unwrap_or(-1),
                size_class: entry.size_class.clone(),
                huntable: entry.huntable,
                ecology_phase: entry.ecology_phase.clone(),
                domestication: entry.domestication,
                corralled: entry.corralled,
                corral_progress: entry.corral_progress,
                per_worker_yield: forecast.per_worker_yield.provisions,
                // **The per-herd, SPECIES-AWARE per-worker rate — this is the one a band preview
                // clamps with**, not the cohort's species-blind `hunt_per_worker_provisions`. A wolf
                // reads `0` food here and a positive trade rate, so the two components together are
                // the honest throughput.
                per_worker_trade: forecast.per_worker_yield.trade_goods,
                // The Corral investment rung's (gross) payoff once penned; the preparing dip is the
                // `corral` row of `hunt_policy_ceilings` below.
                corral_yield: forecast.managed_yield.provisions,
                // The trade half of that same `managed_yield` pair — a rung's payoff is a vector.
                corral_trade: forecast.managed_yield.trade_goods,
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
                    .map(|herd| {
                        hunt_policy_ceiling_entries(&forecast, fauna.hunt_yield_for(&herd.species))
                    })
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
                // an unpenned herd. `pen_pasture_fraction` is per-turn scratch (Population ran before
                // this capture, so it reflects the current turn); `pen_extend_progress` is
                // authoritative `Herd` state (the in-flight ExtendPen ring meter) — here it just
                // crosses to the client wire alongside it.
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
                food_per_animal: forecast.body_mass_yield.provisions,
                // The same quantum in trade goods — one animal's pelt. A wolf's `food_per_animal` is
                // honestly `0`, so the client needs this to render its kill rhythm at all.
                trade_per_animal: forecast.body_mass_yield.trade_goods,
                // Herd staffing — the herders a MANAGED herd owes this turn to hold its tameness (0 for
                // a wild/unmanaged herd, per `herd_herders_needed`), and how well it is staffed
                // (`Herd::herded_fraction`, the labor system's per-turn write; `FULLY_HERDED` for a herd
                // that needs nobody or a vanished one). Split out so the client can distinguish an
                // under-HERDING shortfall from the assignment's blended `workers_needed`.
                herders_needed: herd
                    .map(|herd| herd_herders_needed(herd, fauna))
                    .unwrap_or(0),
                herded_fraction: herd
                    .map(|herd| herd.herded_fraction)
                    .unwrap_or(FULLY_HERDED),
                // The Tame rung's payoff — the pastoral twin of `corral_yield`: what a Sustain hunt
                // pays once this herd is tamed (the pastoral MSY), so the client can quote Tame's
                // `→ +Y` beside its during-building dip. Sourced from the same `hunt_forecast` object
                // every ceiling above reads, so it cannot drift; `0` for a source that never offers
                // Tame (penned/forage), which is exactly `SourceYieldForecast::pastoral_yield`.
                pastoral_yield: forecast.pastoral_yield.provisions,
                // The trade half of that same `pastoral_yield` pair — a rung's payoff is a vector.
                pastoral_trade: forecast.pastoral_yield.trade_goods,
                // The hay this pen drew last turn (Flora Roster F3) — the transient `Herd::fodder_draw`
                // the corral-tend branch wrote, so the client can render "fed by hay" beside the
                // `pen_upkeep` bread bill. `0.0` for an unpenned/absent herd or one no hay reached.
                fodder_draw: herd.map(|herd| herd.fodder_draw).unwrap_or(0.0),
                // The render-ready feed split (Flora Roster F3): the NET larder bill after pasture +
                // hay, and hay's food-equivalent, both the transient `Herd` scratch the corral-tend
                // branch stamped, both in FOOD units. `0.0` for an unpenned/absent herd. With
                // `pen_upkeep` (gross) and `pen_pasture_fraction`, the client draws "pasture NN% · hay
                // X.X · larder Y.Y" with no arithmetic of its own.
                pen_larder_bill: herd.map(|herd| herd.pen_larder_bill).unwrap_or(0.0),
                pen_hay_food: herd.map(|herd| herd.pen_hay_food).unwrap_or(0.0),
                // Predators Phase 0 — the RAW combat components of this herd's species
                // (`docs/plan_predators.md`). Danger is DERIVED client-side, never stored, because
                // strength ≠ danger: hunt-danger ≈ attack×ferocity, camp-threat ≈ attack×aggression.
                // Resolved by display name (the herd's `species` string); a herd whose species does
                // not resolve reads all-zeros (harmless).
                attack: species_def.map(|def| def.combat.attack).unwrap_or(0.0),
                defense: species_def.map(|def| def.combat.defense).unwrap_or(0.0),
                ferocity: species_def.map(|def| def.ferocity).unwrap_or(0.0),
                aggression: species_def.map(|def| def.aggression).unwrap_or(0.0),
                // Predators Phase 1a — the herd's prey-sensing radius, but ONLY for a carnivore
                // (`docs/plan_predators.md`): `> 0` is the client's "this is a predator" signal + its
                // view-ring radius (a carnivore's graze ring is meaningless — it hunts other herds). A
                // herbivore reads `0` and the client keeps drawing its graze-range ring.
                prey_sense_radius: species_def
                    .filter(|def| def.diet == crate::fauna_config::Diet::Carnivore)
                    .map(|_| fauna.predators.prey_sense_radius)
                    .unwrap_or(0),
                // **The crew this herd WOULD owe if managed** (taming-startup-lag fix), computed
                // ownership-INDEPENDENTLY from biomass so the client can floor the Tame-compose worker
                // cap at it up front — before ownership is set in the Population stage, which is what
                // leaves the ownership-gated `herders_needed` above reading 0 on the turn taming starts.
                // A `wild`-ceiling species (mammoth/deer) never tames, so `would_be_herders_needed`
                // returns 0; the same helper the labor arm reads for an investment policy — one seam.
                herders_needed_if_managed: herd
                    .map(|herd| would_be_herders_needed(herd, fauna))
                    .unwrap_or(0),
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
///
/// `tile_quotes` answers **what grows there** — the named plants the tile's forage capacity is made
/// of, plus what each would pay once committed to. Filled by the caller (which has the tiles) through
/// the one `forage::tile_flora_composition` seam, so the composition and `tile_forage_capacity` agree
/// about a tile's shape (in particular about a navigable hex's **two** capacity terms). It is a
/// **memo**, not a per-turn derivation — the quotes are a pure function of ground and config, so they
/// are derived once per tile per world (`snapshot/flora_quotes.rs`, #410). A patch whose tile is
/// absent from the map ships an **empty** composition — "no named plants here", never a fabricated
/// one.
pub(crate) fn snapshot_forage_patches(
    registry: &ForageRegistry,
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    ladder: &LadderConfig,
    seasonal_weights: &HashMap<UVec2, f32>,
    sow_site_refusals: &HashMap<UVec2, SiteRefusal>,
    tile_quotes: &FloraQuoteCache,
) -> Vec<ForagePatchState> {
    let mut patches: Vec<ForagePatchState> = registry
        .patches
        .values()
        .map(|patch| {
            let seasonal = seasonal_weights
                .get(&patch.tile)
                .copied()
                .unwrap_or(NO_FORAGE_SEASON);
            // **What is growing on this tile**, off the same memo entry the quotes came from — every
            // rate below is the share-weighted average of the *patch's* basket, which `forage.rs`
            // derives from this one (#433). A patch whose tile is absent from the map names no
            // plants and falls back to the empty-basket defaults.
            let tile_composition = tile_quotes.tile_composition(patch.tile);
            let forecast = forage_forecast(
                patch,
                tile_composition,
                forage,
                flora,
                ladder,
                seasonal,
                FORECAST_OUTPUT_MULTIPLIER,
            );
            ForagePatchState {
                x: patch.tile.x,
                y: patch.tile.y,
                cultivation_progress: patch.cultivation_progress,
                is_cultivated: patch.is_cultivated(),
                owner: patch.owner.map(|faction| faction.0),
                biomass: patch.biomass,
                carrying_capacity: patch.carrying_capacity,
                ecology_phase: patch.ecology_phase.as_str().to_string(),
                // The plant web's forecast is food-only for now — its `trade_goods` component is
                // `forage::PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` (a known gap, #337), so these
                // project the provisions component rather than shipping a false `0` trade line.
                per_worker_yield: forecast.per_worker_yield.provisions,
                ceiling_sustain: forecast.ceiling_sustain.provisions,
                ceiling_surplus: forecast.ceiling_surplus.provisions,
                ceiling_deplete: forecast.ceiling_deplete.provisions,
                ceiling_eradicate: forecast.ceiling_eradicate.provisions,
                // The Cultivate investment rung: the preparing dip + the payoff once cultivated.
                ceiling_cultivate: forecast.ceiling_prepare.provisions,
                tended_yield: forecast.managed_yield.provisions,
                // The Sow rung (plant 3): its own two meters — independent of cultivation's, since a
                // Field may stand on ground that was never tended — and its own preparing/payoff
                // pair. `field_provisions` is the same helper the labor arm pays a Field with, so the
                // client's "then Y" is the number the sim will hand over.
                field_progress: patch.field_progress,
                is_field: patch.is_field(),
                ceiling_sow: forecast.ceiling_sow.provisions,
                field_yield: field_provisions(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                // **Why this ground will not take seed** — resolved by the caller through the *same*
                // `RungSiteRequirement::refusal` seam the `sow` command and the labor arm gate on, so
                // the wire cannot disagree with the gate. Absent from the map = the land takes seed
                // (`SITE_ACCEPTED`), mirroring `seasonal_weights`' absent-means-none convention.
                sow_site_refusal: sow_site_refusals
                    .get(&patch.tile)
                    .map_or(SITE_ACCEPTED, |refusal| refusal.as_str())
                    .to_string(),
                // **THE TILE'S PER-RUNG VECTOR** (#426) — the same six rungs as the flat `ceiling_*`
                // fields above, as rows, so a future rung costs no schema change (the migration
                // `hunt_policy_ceilings` already made). The FOOD axis is live and is the identical
                // number its flat twin carries, read from the one `forecast` above so the two
                // representations cannot disagree while both are on the wire.
                //
                // **The two non-food accounts are still the `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED`
                // gap**, because `forage_forecast` does not project them yet — that is the remaining
                // half of #426 and it needs `YieldAccounts` to carry a third account (see the issue). They
                // are written explicitly rather than defaulted so the sentinel is visible at the site
                // a reader will look, exactly as the food-only comment below has been since #337.
                forage_policy_ceilings: plant_policy_forecasts(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    ladder,
                    seasonal,
                    FORECAST_OUTPUT_MULTIPLIER,
                )
                .into_iter()
                .map(|rung| ForagePolicyCeilingState {
                    policy: rung.policy.as_str().to_string(),
                    provisions_per_turn: rung.ceiling.provisions,
                    trade_goods_per_turn: rung.ceiling.trade_goods,
                    fodder_per_turn: rung.ceiling.fodder,
                    per_worker_provisions: rung.per_worker.provisions,
                    per_worker_trade_goods: rung.per_worker.trade_goods,
                    per_worker_fodder: rung.per_worker.fodder,
                })
                .collect(),
                // The two investment rungs' PAYOFF twins — each projected at **its own** rung
                // (`tended_*` at rung 2, `field_*` at rung 3), never at the rung the patch happens to
                // stand on. That is the #433 rule, and getting it wrong is the exact defect #433
                // fixed: a Sow quote that inherited the tended basket's conversion gain overstated by
                // 10% on the reference tile and by the full 2× wherever weeding saturates.
                tended_trade: tended_trade_goods(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                tended_fodder: tended_fodder(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                field_trade: field_trade_goods(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                field_fodder: field_fodder(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                // **What is growing here — as this PATCH has it** (#433). The tile names the
                // plants (§2, per-tile realization §10) and the patch's rung then says how much of
                // each: a tended patch's basket visibly collapses toward its crop and a Field
                // publishes a single 100% entry, which is the whole of what a rung below 4 does.
                // Resolved through the same `forage::patch_composition` seam every rate reads, so
                // the card cannot show a basket the economy is not using.
                //
                // A **wild** patch is the tile's basket verbatim, and takes the memo's own `Arc`
                // unchanged — shared, never copied, because the basket belongs to the tile and
                // deep-copying it re-allocated two `String`s per named plant on every patch on
                // every turn (half this readout's whole cost). Only a committed patch pays for a
                // rebuilt list, and there are few of those.
                composition: patch_composition_info(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    tile_quotes,
                ),
                // **Which ONE plant this patch is committed to** (Flora Roster S1) — `""` is the
                // wild mixed basket, a positive statement rather than "unknown". The display name is
                // resolved here because the client holds no roster (the `FloraShareInfo::display_name`
                // convention); a key the roster no longer knows ships an empty name rather than a
                // fabricated one.
                committed_species: patch.species.clone().unwrap_or_default(),
                committed_display_name: patch
                    .species
                    .as_ref()
                    .and_then(|key| flora.species.get(key))
                    .map(|def| def.display_name.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();
    patches.sort_unstable_by_key(|patch| (patch.y, patch.x));
    patches
}

/// **The patch's effective basket, in the wire's `FloraShareInfo` shape** — [`patch_composition`]
/// applied to the tile's basket, with every entry's per-species picker payload (display name,
/// ceilings, per-rung payoffs) taken off the tile's memo unchanged.
///
/// The payoffs stay **tile-level on purpose**: they answer *"what would this ground pay committed to
/// this plant"*, which is a property of the land and the roster, not of what somebody already weeded.
/// Only [`FloraShareInfo::share`] moves.
///
/// A **wild** patch hands back the memo's own `Arc` untouched — that is the >99% case and the whole
/// reason the memo exists. A crop the tile's realized basket never named (only reachable through a
/// `Sow` on bare ground, which reads the *affinity* roster) still has to appear, so it is built from
/// the roster with no payoffs rather than dropped: a Field must never publish an empty basket.
fn patch_composition_info(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    tile_quotes: &FloraQuoteCache,
) -> Arc<[FloraShareInfo]> {
    let effective = patch_composition(patch, tile_composition, forage);
    let quoted = tile_quotes.composition(patch.tile);
    let Cow::Owned(effective) = effective else {
        return quoted; // wild: the tile's basket verbatim, shared rather than rebuilt.
    };
    effective
        .iter()
        .filter(|entry| entry.share > NO_PUBLISHED_SHARE)
        .map(|entry| {
            quoted
                .iter()
                .find(|info| info.species == entry.species)
                .map_or_else(
                    || FloraShareInfo {
                        species: entry.species.clone(),
                        display_name: flora
                            .species
                            .get(&entry.species)
                            .map(|def| def.display_name.clone())
                            .unwrap_or_default(),
                        share: entry.share,
                        ..FloraShareInfo::default()
                    },
                    |info| FloraShareInfo {
                        share: entry.share,
                        ..info.clone()
                    },
                )
        })
        .collect()
}

/// **The share below which a plant is not published at all** — a species weeded out of a patch is
/// gone from it, not present at zero. The wire convention `NO_SHARE` states inside `forage.rs`,
/// restated here because this is the readout that enforces it.
const NO_PUBLISHED_SHARE: f32 = 0.0;

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
