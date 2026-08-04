use super::*;
use crate::fauna::{herd_capacity, herd_ecology, net_biomass_delta, reseeding_logistic_regrowth};
use crate::forage::{
    field_fodder, field_trade_goods, forage_per_worker_biomass, patch_ecology,
    patch_fodder_per_biomass, patch_neglect_grace_remaining, patch_provisions_per_biomass,
    patch_trade_per_biomass, tended_fodder, tended_trade_goods,
};
use crate::intensification::NO_BUILD_REMAINING_FRACTION;

/// **No animal pays fodder** — the herd half of the per-biomass yield triple is structurally zero,
/// and stated rather than defaulted so a reader sees it is a fact about animals and not an
/// unprojected gap. Both webs publish the same triple so a client needs one code path.
const NO_ANIMAL_PAYS_FODDER: f32 = 0.0;

/// **The countdown a source with nothing at risk publishes.** Paired with `has_neglect_grace: false`,
/// which is the field a reader must check — this number is only here because the wire has no optional
/// scalars, and it deliberately reuses the "biting now" value rather than inventing a sentinel the
/// client could mistake for a real countdown.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **The crew a rung that declares none publishes.** Unreachable on the plant branch today (both
/// plant rungs state a `crew_needed`), but the wire must say something, and `0` is the schema's own
/// "this rung declares no crew" reading — never a fabricated `1`, which would floor the worker cap at
/// a number nobody chose.
const NO_RUNG_CREW: u32 = 0;

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

/// **The floors the pre-launch raid table is SAMPLED at** — marks on a dial, **not** a set of
/// options.
///
/// A raid's trip length has no closed form: it is a bounded forward simulation
/// ([`hunt_trip_forecast`]) of "grab the standing surplus, come home", so unlike a resident band's
/// ceiling the sim cannot hand the client a formula to evaluate at an arbitrary floor. It must
/// export answers at chosen points, and the client interpolates between them for the outfit preview.
///
/// **This is the shape a four-stance axis comes back in, so it is named to make that visible.** The
/// launch command accepts **any** floor in `0.0..=1.0` (`send_hunt_expedition`), and nothing here
/// constrains it — these are five readings of a continuum picked to span it (bare, the Allee brink,
/// a heavy draw, the food peak, deliberate under-harvest). Adding a sample costs a row per party
/// size and changes no behaviour; treating a sample as an offered stance would undo the arc.
pub(crate) const RAID_FORECAST_FLOOR_SAMPLES: [f32; 5] = [0.0, 0.15, 0.30, 0.50, 0.80];

/// **How many points each source's regrowth curve is SAMPLED at** — readings of a continuum, **not**
/// a set of states, and a **display-resolution choice rather than a model fact**. The growth curve is
/// continuous; this is how finely the client is handed it.
///
/// **Why the curve is sampled at all, when the ceiling is not.** The arc holds one rule with two
/// halves: *where a closed form exists the sim ships the terms and the client evaluates it; where one
/// does not, the sim ships answers and the client interpolates between them.* An escapement ceiling
/// is the first case — `max(0, B − floor·K) × rate` is linear and exact, which is what let the four
/// stance rows die. A growth curve is the second, and not because it is hard to write down: it is
/// **two different functions**. A patch is pure logistic with a reseed floor and no Allee term; a
/// herd has critical depensation below `collapse_fraction`. Publishing `r` and the thresholds would
/// put a second copy of both models in a language with no tests over them, and the drift would be
/// invisible because either one still draws a plausible curve.
///
/// The samples are **evenly spaced over `0.0..=1.0` of `K`**, so sample `i` sits at
/// `i / (SAMPLES − 1)` and the x-axis needs no wire field. Changing this number changes the chart's
/// resolution and nothing else; a client must interpolate between samples rather than treat them as
/// the only stocks a source can hold.
pub(crate) const REGROWTH_CURVE_SAMPLES: usize = 11;

/// The fraction of `K` sample `index` is taken at — see [`REGROWTH_CURVE_SAMPLES`] for why the
/// spacing is uniform and therefore implicit on the wire.
fn regrowth_sample_fraction(index: usize) -> f32 {
    index as f32 / (REGROWTH_CURVE_SAMPLES - 1) as f32
}

/// **The patch's own per-turn regrowth, sampled across its `K`** — through
/// `fauna::reseeding_logistic_regrowth`, the *same* seam `forage::regrow_patch` advances the stock
/// with, and on the patch's **own** ecology (`forage::patch_ecology`), so a tended patch's curve is
/// the one its rung actually bought.
///
/// Each entry is a **delta in biomass**: what one Logistics turn adds at that standing stock.
///
/// **The `0.0` sample is the reseed floor's lift, not zero.** A stripped patch is raised to
/// `reseed_floor_fraction × K` before the logistic step, which is exactly why a plant stand driven to
/// floor `0` comes back while a herd driven there dies — see [`herd_regrowth_samples`], whose low
/// samples are negative. **No sample is ever negative here**: plants have no Allee crash.
///
/// **The peak of this curve IS the food peak** the panel marks at `K/2`. It is not published
/// separately, deliberately: one number derived two ways is how the two start disagreeing.
fn patch_regrowth_samples(patch: &ForagePatch, forage: &ForageLaborConfig) -> Vec<f32> {
    let ecology = patch_ecology(patch, forage);
    let cap = patch.carrying_capacity;
    (0..REGROWTH_CURVE_SAMPLES)
        .map(|index| {
            let standing = regrowth_sample_fraction(index) * cap;
            reseeding_logistic_regrowth(
                standing,
                cap,
                ecology.regrowth_rate,
                forage.reseed_floor_fraction,
            ) - standing
        })
        .collect()
}

/// **The herd's own per-turn regrowth, sampled across its `K`** — the animal twin of
/// [`patch_regrowth_samples`], through `fauna::net_biomass_delta` on `herd_ecology` /
/// `herd_capacity`: the same seam `fauna::regrow_biomass` advances the herd with, so a pastoral or
/// penned herd's curve is the one its rung bought.
///
/// **The low samples are NEGATIVE, and that is the point.** Below `collapse_fraction × K` a herd is
/// past its Allee threshold and declines by `collapse_rate` of its biomass every turn, hunted or not.
/// A client must render those as **decline** and must not clamp them to zero: the crash is the whole
/// reason floor `0` ends a herd while it only sets a patch back.
fn herd_regrowth_samples(herd: &Herd, fauna: &FaunaConfig) -> Vec<f32> {
    let ecology = herd_ecology(herd, fauna);
    let cap = herd_capacity(herd, fauna);
    (0..REGROWTH_CURVE_SAMPLES)
        .map(|index| net_biomass_delta(regrowth_sample_fraction(index) * cap, cap, &ecology))
        .collect()
}

/// The **pre-launch hunt-trip estimate table** for one herd: [`hunt_trip_forecast`] run for every
/// sampled floor ([`RAID_FORECAST_FLOOR_SAMPLES`]) × every legal party size
/// (`1..=expedition.max_party_size`), so the client's outfit UI is a **table lookup** — zero
/// arithmetic, zero ecology model. Each row carries both `turns_to_fill` (turns until the raid
/// completes) and `animals_taken` (the payload the client headlines).
///
/// Cost is bounded by construction: `samples × max_party_size × hunt.forecast_horizon_turns`
/// turn-steps per herd, and only **huntable** herds are estimated. In practice a raid is **short** —
/// it grabs the surplus and terminates — so a snapshot's worth of raids simulates cheaply.
///
/// A species that **pays nothing** ([`crate::fauna::species_requires_denial`]) is estimated at the
/// one floor it can legally be worked at: there is no point quoting a sustainable raid on a quarry
/// with no product.
pub(crate) fn hunt_trip_estimate_entries(
    herd: &Herd,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    expedition: &ExpeditionConfig,
) -> Vec<HuntTripEstimateState> {
    let denial_only = crate::fauna::species_requires_denial(fauna.hunt_yield_for(&herd.species));
    let sampled: &[f32] = if denial_only {
        &[crate::components::STRIP_IT_BARE]
    } else {
        &RAID_FORECAST_FLOOR_SAMPLES
    };
    let mut entries = Vec::with_capacity(sampled.len() * expedition.max_party_size as usize);
    for &floor in sampled {
        for party_workers in 1..=expedition.max_party_size {
            let forecast = hunt_trip_forecast(party_workers, herd, floor, fauna, labor, expedition);
            entries.push(HuntTripEstimateState {
                floor,
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
            // The neglect countdown, off the live registry herd (the display `entry` carries no
            // counter). A herd the registry cannot resolve has nothing at risk to report.
            let neglect_grace =
                herd.and_then(|herd| crate::fauna::herd_neglect_grace_remaining(herd, ladder));
            // **The herd's own ecology — the rung's, not the wild block's.** `herd_ecology` picks
            // wild / pastoral / pen, and it is the seam `refresh_ecology_phase` classified the
            // published `ecology_phase` word with, so the bands below cannot describe a different
            // source than the word does.
            let ecology = herd.map(|herd| herd_ecology(herd, fauna));
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
                // The Corral investment rung's (gross) payoff once penned; the preparing dip is
                // `hunt_policy_ceilings[stance] × corral_build_fraction` (issue #442).
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
                // **THE PER-BIOMASS YIELD VECTOR** — what one unit of this herd's biomass is worth,
                // in every account (`docs/plan_harvest_floor.md` §5). It replaces the four stance
                // ceiling rows: with `biomass` and `carrying_capacity` the client evaluates
                // `max(0, B − floor·K) × rate` at ANY floor, which no fixed set of rows can express.
                // The species' own vector, through the one `FaunaConfig::hunt_yield_for` seam the
                // take path reads.
                //
                // **No dip term.** Since §3.1 `yield_fraction_while_building` multiplies the CREW's
                // throughput, not the ceiling, so the build fractions belong to the `expected(..)`
                // half of the composition and `ceiling_at` takes no `improvement` at all.
                provisions_per_biomass: herd
                    .map(|herd| fauna.hunt_yield_for(&herd.species).provisions_per_biomass)
                    .unwrap_or(0.0),
                // No animal pays fodder; the field is present so both webs publish the same triple.
                fodder_per_biomass: NO_ANIMAL_PAYS_FODDER,
                trade_per_biomass: herd
                    .map(|herd| fauna.hunt_yield_for(&herd.species).trade_goods_per_biomass)
                    .unwrap_or(0.0),
                // **One hunter's BIOMASS throughput** — the term `systems::hunt_take`'s collection
                // multiplies by the head-count, with no seasonal factor (the animal web has none).
                // It is the crew half of the composition: the vector above turns a floor into a
                // ceiling, and this turns that ceiling into a number of people. Shipped rather than
                // left to `per_worker_yield / provisions_per_biomass`, which is `0 / 0` on a wolf.
                per_worker_biomass: labor.hunt.per_worker_biomass_capacity,
                // **The growth curve, sampled** — the third term the panel needs and the one with no
                // closed form the client may safely re-derive (see [`REGROWTH_CURVE_SAMPLES`]). A
                // vanished herd publishes an empty curve rather than a row of zeros, which is a
                // different claim.
                regrowth_samples: herd
                    .map(|herd| herd_regrowth_samples(herd, fauna))
                    .unwrap_or_default(),
                // **The phase bands, resolved through the same seam as the phase WORD above** —
                // `herd_ecology`, which picks wild / pastoral / pen, so a tamed or penned herd
                // publishes its own rung's bands rather than the wild block's. Fractions of `K`, in
                // the units the floor is in; that is what lets the chart draw them as the zones the
                // floor line is dragged against.
                collapse_fraction: ecology
                    .as_ref()
                    .map(|ecology| ecology.collapse_fraction)
                    .unwrap_or(0.0),
                stressed_fraction: ecology
                    .as_ref()
                    .map(|ecology| ecology.stressed_fraction)
                    .unwrap_or(0.0),
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
                // returns 0; the same helper the labor arm reads while a build runs — one seam.
                herders_needed_if_managed: herd
                    .map(|herd| would_be_herders_needed(herd, fauna))
                    .unwrap_or(0),
                // **The two animal build dips, as the fractions they are** (issue #442). They used to
                // be extra `hunt_policy_ceilings` rows, each stating the dip against Sustain alone;
                // the dip now multiplies whichever stance the crew holds, so the wire carries the
                // factor and the client applies it to the selected row. Read off the *same*
                // `SourceYieldForecast::build_dips` the take path prices a build with.
                //
                // A **penned** herd has nothing left to build, and says so with the out-of-range
                // `NO_BUILD_REMAINING_FRACTION` rather than the identity `1.0`, which claimed the
                // build was free *and* still on offer.
                tame_build_fraction: forecast
                    .build_dips
                    .rung_two
                    .unwrap_or(NO_BUILD_REMAINING_FRACTION),
                corral_build_fraction: forecast
                    .build_dips
                    .rung_three
                    .unwrap_or(NO_BUILD_REMAINING_FRACTION),
                // **The neglect countdown**, resolved through the *same* `herd_keeping_rung` seam
                // `advance_husbandry` gates the shed on, so the wire can never count down a grace
                // against a rung the sim is not applying. `None` = a wild herd: nobody's to keep, so
                // the pair reads "nothing at risk" rather than a zero that means "shedding now".
                has_neglect_grace: neglect_grace.is_some(),
                neglect_grace_remaining: neglect_grace.unwrap_or(NO_NEGLECT_REMAINING),
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
            let neglect_grace = patch_neglect_grace_remaining(patch, ladder);
            // The patch's own ecology — the seam `refresh_ecology_phase` classified the published
            // `ecology_phase` word with, so the bands and the word describe the same source.
            let ecology = patch_ecology(patch, forage);
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
                // The Cultivate investment rung: the preparing dip + the payoff once cultivated.
                tended_yield: forecast.managed_yield.provisions,
                // The Sow rung (plant 3): its own two meters — independent of cultivation's, since a
                // Field may stand on ground that was never tended — and its own preparing/payoff
                // pair. `field_provisions` is the same helper the labor arm pays a Field with, so the
                // client's "then Y" is the number the sim will hand over.
                field_progress: patch.field_progress,
                is_field: patch.is_field(),
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
                // **THE PER-BIOMASS YIELD VECTOR** — what one unit of this patch's standing crop
                // is worth, in every account (`docs/plan_harvest_floor.md` §5), at the patch's own
                // basket-averaged rates: the same `patch_*_per_biomass` seams `forage_take` pays
                // with, so a tended patch reads its committed conversion and not the wild one.
                //
                // It replaces the four stance ceiling rows because a player drags a **continuous**
                // floor: with `biomass` and `carrying_capacity` the client evaluates
                // `max(0, B − floor·K) × rate` anywhere on the dial. **No dip term** — since §3.1
                // the build fraction multiplies the crew's throughput, never the ceiling.
                provisions_per_biomass: patch_provisions_per_biomass(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                ),
                fodder_per_biomass: patch_fodder_per_biomass(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                ),
                trade_per_biomass: patch_trade_per_biomass(patch, tile_composition, flora, forage),
                // **One gatherer's BIOMASS throughput** — `per_worker_biomass_capacity × seasonal`,
                // the exact term `forage_take`'s worker cap multiplies by the head-count, through the
                // shared helper so the wire and the take cannot disagree. `0` in a dead season, like
                // `per_worker_yield` beside it. Shipped rather than left to
                // `per_worker_yield / provisions_per_biomass`, which is `0 / 0` on a Field of cotton,
                // flax or hay.
                per_worker_biomass: forage_per_worker_biomass(forage, seasonal),
                // **The growth curve, sampled** — the plant twin; non-negative at every sample, and
                // its `0.0` entry is the reseed floor's lift.
                regrowth_samples: patch_regrowth_samples(patch, forage),
                // The phase bands, off the patch's OWN ecology — the same seam
                // `refresh_ecology_phase` classified the word above with.
                collapse_fraction: ecology.collapse_fraction,
                stressed_fraction: ecology.stressed_fraction,
                // **The two plant build dips, as fractions** (issue #442) — the twins of the herd's
                // `tame_build_fraction`/`corral_build_fraction`, off the same `build_dips` the take
                // path prices a build with. `preparing(stance) = ceiling[stance] × fraction`.
                // A **Field** has nothing left to build; see the herd twin above for why that is
                // `NO_BUILD_REMAINING_FRACTION` and not the identity.
                cultivate_build_fraction: forecast
                    .build_dips
                    .rung_two
                    .unwrap_or(NO_BUILD_REMAINING_FRACTION),
                sow_build_fraction: forecast
                    .build_dips
                    .rung_three
                    .unwrap_or(NO_BUILD_REMAINING_FRACTION),
                // **The neglect countdown**, resolved through the *same* `patch_unwinding_rung` seam
                // `advance_cultivation` bleeds through — so the wire counts down against the rung
                // that will actually revert, not one the patch merely stands on. `None` = a wild
                // patch, which is most of them.
                has_neglect_grace: neglect_grace.is_some(),
                neglect_grace_remaining: neglect_grace.unwrap_or(NO_NEGLECT_REMAINING),
                // **The two build crews** — the floor under the client's worker cap, and the
                // denominator its build actually accrues against. `0` means the rung declares no
                // crew (unreachable on the plant branch today; the field is the honest shape rather
                // than a fabricated `1`).
                cultivate_crew_needed: ladder
                    .rung(RungKey::PlantTended)
                    .build_crew_needed()
                    .unwrap_or(NO_RUNG_CREW),
                sow_crew_needed: ladder
                    .rung(RungKey::PlantField)
                    .build_crew_needed()
                    .unwrap_or(NO_RUNG_CREW),
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
                    || {
                        // The roster is the one place a species' name and its display `role` are
                        // decided, so both come off the same lookup; a key the roster no longer
                        // knows ships both fields empty ("unstated") rather than fabricated.
                        let def = flora.species.get(&entry.species);
                        FloraShareInfo {
                            species: entry.species.clone(),
                            display_name: def
                                .map(|def| def.display_name.clone())
                                .unwrap_or_default(),
                            role: def
                                .map(|def| def.role.as_str().to_string())
                                .unwrap_or_default(),
                            share: entry.share,
                            ..FloraShareInfo::default()
                        }
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
/// (2004) → Penning (2006) up the animal one, plus Foddering (2007), the *capability* the top animal
/// rung teaches rather than a gate on reaching a rung. Iterates the ledger's factions in sorted
/// order; a faction is emitted only when it has begun learning **something** (all zero → skipped),
/// mirroring how `discovery_progress_entries` skips empty progress.
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
            let foddering = ledger
                .get_progress(faction, FODDERING_DISCOVERY_ID)
                .to_f32();
            // A rung-3 knowledge cannot be positive while its rung-2 gate is zero (you cannot work a
            // tended patch you never cultivated), so this stays equivalent to the old
            // cultivation/herding-only check — but stating every meter keeps it true if a later slice
            // grants one another way.
            if cultivation <= 0.0
                && herding <= 0.0
                && seed_selection <= 0.0
                && penning <= 0.0
                && foddering <= 0.0
            {
                return None;
            }
            Some(IntensificationKnowledgeState {
                faction: faction_id,
                cultivation,
                herding,
                seed_selection,
                penning,
                foddering,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fauna_config::{FaunaConfig, SizeClass};
    use crate::labor_config::LaborConfig;

    /// The index of the sample taken **at the food peak** — `K/2`, the middle of an odd-length
    /// evenly-spaced sweep. Named because "the peak of this curve IS the food peak" is the reason the
    /// panel needs no separate field for it, and a silent re-spacing of the samples would move it.
    const FOOD_PEAK_SAMPLE: usize = REGROWTH_CURVE_SAMPLES / 2;

    /// A carrying capacity large enough that every sampled stock is well clear of float noise, and
    /// arbitrary otherwise — the properties asserted here are scale-free.
    const PROBE_CAPACITY: f32 = 1_000.0;

    fn probe_patch() -> ForagePatch {
        let mut patch = ForagePatch::new(UVec2::new(0, 0), PROBE_CAPACITY);
        patch.biomass = PROBE_CAPACITY;
        patch
    }

    /// A wild herd whose `r` is the shipped global — the species is named only so `herd_ecology`
    /// resolves a real roster entry; every property below is scale- and rate-free.
    fn probe_herd(fauna: &FaunaConfig) -> Herd {
        Herd::new(
            "probe".to_string(),
            "Wild Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(0, 0)],
            PROBE_CAPACITY,
            PROBE_CAPACITY,
            0.0,
            fauna.ecology.regrowth_rate,
            PROBE_BODY_MASS,
        )
    }

    /// One animal's mass. It cannot reach any assertion here — the regrowth curve is continuous and
    /// quantisation lives in the take path — but `Herd::new` requires one.
    const PROBE_BODY_MASS: f32 = 50.0;

    /// **The two webs are sampled on the same axis but are NOT the same function**, and that
    /// asymmetry is the whole reason the curve is shipped as answers rather than as `r` + thresholds
    /// for the client to evaluate (`.claude/rules/core_sim/yield-forecast.md`).
    ///
    /// A patch is pure logistic with a reseed floor and **no Allee term**, so it never declines. A
    /// herd has critical depensation below `collapse_fraction`, so it declines *by itself* down
    /// there — which is why floor `0` ends a herd and only sets a patch back. A client that clamped
    /// the herd curve at zero would erase exactly that.
    #[test]
    fn the_plant_curve_never_declines_and_the_animal_curve_does_below_the_allee_point() {
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();

        let patch_curve = patch_regrowth_samples(&probe_patch(), &labor.forage);
        assert_eq!(patch_curve.len(), REGROWTH_CURVE_SAMPLES);
        assert!(
            patch_curve.iter().all(|sample| *sample >= 0.0),
            "plants have no Allee crash — no sample may decline: {patch_curve:?}"
        );
        assert!(
            patch_curve[0] > 0.0,
            "…and the `0.0` sample is the RESEED FLOOR's lift, not zero: {}",
            patch_curve[0]
        );

        let herd = probe_herd(&fauna);
        let herd_curve = herd_regrowth_samples(&herd, &fauna);
        assert_eq!(herd_curve.len(), REGROWTH_CURVE_SAMPLES);
        // The Allee threshold, read off the herd's own ecology rather than restated, so a retune of
        // `collapse_fraction` moves the fixture with it.
        let allee = herd_ecology(&herd, &fauna).collapse_fraction;
        let mut saw_decline = false;
        for (index, sample) in herd_curve.iter().enumerate() {
            let fraction = regrowth_sample_fraction(index);
            // The `0.0` sample is the one place both webs agree on nothing: an extinct herd has no
            // biomass to lose, so `net_biomass_delta` returns `0` rather than a negative.
            if fraction > 0.0 && fraction < allee {
                assert!(
                    *sample < 0.0,
                    "a herd at {fraction} of K is past its Allee threshold ({allee}) and must be \
                     DECLINING, not growing: {sample}"
                );
                saw_decline = true;
            }
        }
        assert!(
            saw_decline,
            "the sweep must actually reach the Allee band, or it asserts nothing (threshold {allee} \
             at a spacing of 1/{})",
            REGROWTH_CURVE_SAMPLES - 1
        );
    }

    /// **The peak of the sampled curve IS the food peak** the panel marks at `K/2` — which is why it
    /// is not published as its own field. One number derived two ways is how the two start
    /// disagreeing.
    #[test]
    fn the_sampled_curves_peak_at_the_food_peak_on_both_webs() {
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        assert!(
            (regrowth_sample_fraction(FOOD_PEAK_SAMPLE) - crate::fauna::MSY_BIOMASS_FRACTION).abs()
                < f32::EPSILON,
            "the sweep must actually land ON the food peak, or the panel's mark has no sample"
        );

        for (web, curve) in [
            (
                "plant",
                patch_regrowth_samples(&probe_patch(), &labor.forage),
            ),
            ("animal", herd_regrowth_samples(&probe_herd(&fauna), &fauna)),
        ] {
            let peak = curve[FOOD_PEAK_SAMPLE];
            assert!(peak > 0.0, "{web}: the peak must be a real rate: {curve:?}");
            for (index, sample) in curve.iter().enumerate() {
                if index != FOOD_PEAK_SAMPLE {
                    assert!(
                        *sample < peak,
                        "{web}: sample {index} must sit strictly below the food peak's {peak}: \
                         {sample}"
                    );
                }
            }
        }
    }
}
