use sim_runtime::MaterialPayoff;

use super::*;
use crate::fauna::{
    classify_ecology_phase, herd_capacity, herd_ecology, net_biomass_delta,
    reseeding_logistic_regrowth, NO_RETREAT_STAGE_STAY, ONE_UNIT_OF_BIOMASS,
};
use crate::forage::{
    field_fodder, forage_per_worker_biomass, patch_ecology, patch_fodder_per_biomass,
    patch_neglect_grace_remaining, patch_provisions_per_biomass, tended_fodder,
};
use crate::intensification::{
    build_fraction, build_work_per_worker_turn, NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY,
    NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED, UNSCALED_UPKEEP,
};
use sim_schema::NO_BUILD_TURNS_ESTIMATE;

/// **No animal pays fodder** — the herd half of the per-biomass yield triple is structurally zero,
/// and stated rather than defaulted so a reader sees it is a fact about animals and not an
/// unprojected gap. Both webs publish the same triple so a client needs one code path.
const NO_ANIMAL_PAYS_FODDER: f32 = 0.0;

/// **The countdown a source with nothing at risk publishes.** Paired with `has_neglect_grace: false`,
/// which is the field a reader must check — this number is only here because the wire has no optional
/// scalars, and it deliberately reuses the "biting now" value rather than inventing a sentinel the
/// client could mistake for a real countdown.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **The wire's finite reading of "this source has no engagement stage"** — a **pen** (a penned animal
/// is not stalked) and a species the roster cannot resolve, both of which
/// [`crate::fauna_config::FaunaConfig::engage_rate_for`] / [`crate::SourceYieldForecast`] answer with
/// [`f32::INFINITY`] sim-side. FlatBuffers floats carry an infinity fine; a *client* dividing by one
/// does not, so the seam converts once, here, and the schema documents `<= 0` as *unbounded* — the
/// same reading `fauna::hunt_engage_workers` gives it.
const NO_ENGAGEMENT_STAGE: f32 = 0.0;

/// **The dispersion the wire's `stayFraction` is published at** — the neutral `1.0`, which leaves the
/// species' own `wariness` untouched. The field is the *species* half of the retreat; the *party*
/// half is the chosen kit's `KitOption.dispersion`, which the client multiplies in (schema:
/// `effective = clamp(1 − (1 − stayFraction) × dispersion, 0, 1)`). Publishing a party-resolved value
/// here would bake one band's kit into a per-herd row every band reads.
const WIRE_NEUTRAL_DISPERSION: f32 = 1.0;

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

/// **One quarry's quoted party** — the fight tier the herd row is priced with, and the roster id
/// that says which kit it came from.
///
/// They travel together because they must agree: the published id names the kit the fight tier was
/// resolved through, and a row quoting an attack from one kit beside an id naming another is the
/// mis-pairing `equipment.md`'s per-job tier table exists to prevent.
///
/// **The sled tier that used to ride here went with the two pre-launch estimate tables** — they were
/// its only readers, and `crate::forecast_query` answers a raid per band, per kit, per exact party
/// and floor, on demand.
#[derive(Debug, Clone)]
pub(crate) struct QuotedParty {
    /// The hunter profile and resolver tuning the herd's `hunt_forecast` resolves its fight with.
    pub(crate) party: crate::fauna::HuntingParty,
    /// The roster id of that kit, published verbatim on `default_kit_id`.
    pub(crate) kit_id: String,
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
    /// **The EQUIPPED reference haul rate**, resolved through the item table's default tier
    /// ([`crate::equipment_config::EquipmentConfig::equipped_reference`]) — a herd row is a fact
    /// about the *herd*, and a herd has no band to resolve a kit against, so it quotes what a
    /// kitted party hauls. It must NOT be `labor.hunt.per_worker_biomass_capacity`, which is the
    /// sledless baseline since the carries moved onto their tiers.
    pub(crate) equipped_haul_rate: f32,
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
    /// **The party this herd's per-worker YIELD row is priced for, resolved per SPECIES** — the
    /// hunter profile and the resolver tuning (`docs/plan_hunt_through_combat.md` §4), at **that
    /// quarry's own default kit** ([`crate::fauna::herd_default_hunt_kit`]).
    ///
    /// **It survived the estimate tables; it is not a leftover of them.** The two pre-launch tables
    /// that used to be priced here are gone (`crate::forecast_query` answers them per band, per kit,
    /// on demand), but `hunt_forecast` still needs a party to resolve the fight that decides
    /// `per_worker_yield` / `corral_yield` — and those are
    /// facts about the **herd**, published once for every viewer, with no band to ask.
    ///
    /// The herd row therefore has no band to ask, but it *can* ask the **quarry**: it is still
    /// exactly one party per herd, carrying the kit the compose sheet opens on rather than the hunt
    /// job's blanket default, and it **publishes which** ([`HerdTelemetryState::default_kit_id`]).
    ///
    /// **Keyed by species DISPLAY name**, the same key `FaunaConfig::species_by_display` and the
    /// herd's own `species` string use, and resolved once per species per capture — the default is
    /// a pure function of quarry × roster, so a per-herd resolution would re-score the same roster
    /// for every herd of the same animal.
    pub(crate) parties: &'a HashMap<String, QuotedParty>,
    /// **The PEN axis of that same table** — the identical per-species quote resolved for a herd
    /// that is *corralled*, which is a fact about the herd rather than the species and so cannot
    /// live in one map beside it.
    ///
    /// A pen is collected on [`crate::equipment_config::EquipmentStat::PenCarry`], so its default is
    /// the kit that supplies it and not the range scorer's winner — see
    /// [`crate::fauna::herd_default_hunt_kit`]. With no such kit on the roster this map holds the
    /// *same* choice the range map does, and a penned herd reads exactly as it did before.
    pub(crate) penned_parties: &'a HashMap<String, QuotedParty>,
    /// **The party for a herd whose species the roster cannot resolve** — the hunt job's default,
    /// resolved unbounded, which is the same fallback every other unresolved field on the row gives.
    /// Answers for a penned herd too: with no species there is no row to quote either axis from.
    pub(crate) fallback_party: &'a QuotedParty,
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
        equipped_haul_rate,
        grid_size,
        wrap_horizontal,
        parties,
        penned_parties,
        fallback_party,
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
            // **THIS HERD'S quoted party** — the kit the compose sheet opens on, memoized once per
            // species per source axis by the caller. **A corralled herd reads the PEN table**: a
            // pen is collected on `EquipmentStat::PenCarry` and only the handling gear supplies it,
            // which no score against the *species* can say (`fauna::herd_default_hunt_kit`). A
            // species the roster cannot resolve falls back to the hunt job's default, unbounded,
            // like every other unresolved field here.
            let axis = if herd.is_some_and(|herd| herd.is_corralled()) {
                penned_parties
            } else {
                parties
            };
            let quoted = axis.get(&entry.species).unwrap_or(fallback_party);
            let party = &quoted.party;
            let forecast = herd
                .map(|herd| {
                    hunt_forecast(
                        herd,
                        fauna,
                        equipped_haul_rate,
                        party,
                        FORECAST_OUTPUT_MULTIPLIER,
                    )
                })
                .unwrap_or_default();
            // The heading arrow points at the herd's NEXT hex, which is a second, independent tile —
            // so it is fog-filtered on its own terms through the same rule, or a visible herd on the
            // edge of your sight would hand you a free look at where it is going. `-1` (the existing
            // "no heading" sentinel the client already renders as no arrow) covers both "loitering"
            // and "you cannot see that far", which the client has no reason to distinguish.
            //
            // **OFF THE LIVE HERD.** `Herd::corral_at` clears `next_pos` in **Population**, after the
            // display entry was written, so a herd penned this turn published the heading of the roam
            // its pen had just ended — a migration arrow on an animal that cannot move.
            let next_position = herd
                .map(|herd| herd.next_position())
                .unwrap_or(entry.next_position)
                .filter(|pos| inputs.herd_is_visible(herd, *pos));
            // The neglect countdown, off the live registry herd (the display `entry` carries no
            // counter). A herd the registry cannot resolve has nothing at risk to report.
            let neglect_grace =
                herd.and_then(|herd| crate::fauna::herd_neglect_grace_remaining(herd, ladder));
            // **The herd's own ecology — the rung's, not the wild block's.** `herd_ecology` picks
            // wild / pastoral / pen, and it is the seam `refresh_ecology_phase` classifies the
            // `ecology_phase` word with, so the bands below cannot describe a different source than
            // the word does. Resolved once and used for the cuts, the sampled curve **and** the word,
            // which is what makes that agreement structural rather than a convention.
            let ecology = herd.map(|herd| herd_ecology(herd, fauna));
            HerdTelemetryState {
                id: entry.id.clone(),
                label: entry.label.clone(),
                species: entry.species.clone(),
                // **`x`/`y` stay on the display entry, as a PAIR with the fog gate above.** The gate
                // decided this row's visibility against `entry.position`, so publishing a different
                // tile beside it would describe a herd whose presence was judged somewhere else. They
                // cannot disagree in any case: the only Population-stage writer of `current_pos` is
                // `Herd::corral_at`, and the pen tile it is handed is `herd.position()` — the hex the
                // herd already stands on. Ordinary movement happens in `advance_herds`, before the
                // entry is written.
                x: entry.position.x,
                y: entry.position.y,
                // **THE STOCK COMES OFF THE LIVE HERD.** Two writers land after the last telemetry
                // write: `advance_husbandry`'s shed/starve shrink (later in Logistics) and the hunt
                // take in `advance_labor_allocation` (Population). This is not cosmetic — the client
                // composes the escapement ceiling as `max(0, B − floor·K) × rate` with `B` from here
                // and `K` from the live `carrying_capacity` below, so a stale `B` quoted a yield
                // preview assembled from two different turns, every turn.
                biomass: herd.map(|herd| herd.biomass).unwrap_or(entry.biomass),
                // Live for uniformity with the heading it sits beside; a `Herd`'s route is built at
                // spawn and never rewritten, so this cannot currently differ from the entry's copy.
                route_length: herd
                    .map(|herd| herd.route_length() as u32)
                    .unwrap_or(entry.route_length),
                next_x: next_position.map(|pos| pos.x as i32).unwrap_or(-1),
                next_y: next_position.map(|pos| pos.y as i32).unwrap_or(-1),
                size_class: entry.size_class.clone(),
                huntable: entry.huntable,
                // **THE WORD IS RE-DERIVED AT CAPTURE, from the same stock, capacity and ecology the
                // row publishes beside it.** The entry's copy was classified in Logistics; the cuts
                // and `regrowthSamples` next to it come from the live `herd_ecology`, which switches
                // rung the instant a Tame or Corral completes in Population — so on a completing turn
                // the published word and the published cuts described *different rungs*. This is the
                // same classification `Herd::refresh_ecology_phase` makes, through the same two seams,
                // so it is a restatement of the sim's own call and not a second model.
                //
                // **Nothing in the sim gates on `Herd::ecology_phase`** (the rung health gates were
                // deleted): its only readers are the analytics log line, the display mirror, and the
                // Telling's `fauna.collapsing_group_count` / `most_collapsed_species`, which sample
                // the stored word and are untouched here. So re-deriving cannot make the wire
                // disagree with behaviour — there is no behaviour to disagree with.
                ecology_phase: match (herd, ecology.as_ref()) {
                    (Some(herd), Some(ecology)) => {
                        classify_ecology_phase(herd.biomass, herd_capacity(herd, fauna), ecology)
                            .as_str()
                            .to_string()
                    }
                    _ => entry.ecology_phase.clone(),
                },
                // **THE BUILD METERS COME OFF THE LIVE HERD, NOT THE DISPLAY ENTRY.**
                // `HerdTelemetry` is written in Startup and Logistics, and the build accrual runs
                // in `advance_labor_allocation` at **Population** — after both — so `entry`'s copy
                // of these three is always the meter as of the *previous* turn. That was invisible
                // while the row said only "Domesticating 96%", and became a self-contradiction once
                // the live `tameWorkDone`/`tameWorkCost` pair joined it in the same sentence: a
                // finished Tame published as "50 / 50 work (99%)". Read live, as
                // `penExtendProgress` below already is. `entry` remains the fallback for the
                // unreachable "in telemetry, gone from the registry" case, exactly like every other
                // field here.
                domestication: herd
                    .map(|herd| {
                        build_fraction(herd.domestication_progress, herd.domestication_cost)
                    })
                    .unwrap_or(entry.domestication),
                corralled: herd
                    .map(|herd| herd.is_corralled())
                    .unwrap_or(entry.corralled),
                corral_progress: herd
                    .map(|herd| build_fraction(herd.corral_progress, herd.corral_cost))
                    .unwrap_or(entry.corral_progress),
                per_worker_yield: forecast.per_worker_yield.provisions,
                // The Corral investment rung's (gross) payoff once penned; the preparing dip is
                // `hunt_policy_ceilings[stance] × corral_build_fraction` (issue #442).
                corral_yield: forecast.managed_yield.provisions,
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
                // No animal pays fodder; the field is present so both webs publish the same pair.
                fodder_per_biomass: NO_ANIMAL_PAYS_FODDER,
                // **WHAT A HUNT OF THIS HERD IS MADE OF** (arc #527) — the material twins of the two
                // rates around them, and the reason an inedible quarry stops quoting nothing: a
                // wolf's `provisions_per_biomass` and `per_worker_yield` are honestly `0`, and these
                // carry its whole payload.
                //
                // **The species' OWN `hunt_yield.materials` rows** — the very rows
                // `credit_material_yield` is handed at the take site — through the same two biomass
                // terms every other field here uses. Nothing is re-derived, so a retune of a rate
                // moves the quote and the payout together.
                material_per_biomass: herd
                    .map(|herd| {
                        material_rates(
                            fauna.hunt_materials_for(&herd.species),
                            ONE_UNIT_OF_BIOMASS,
                            FORECAST_OUTPUT_MULTIPLIER,
                        )
                    })
                    .unwrap_or_default(),
                per_worker_material: herd
                    .map(|herd| {
                        material_rates(
                            fauna.hunt_materials_for(&herd.species),
                            equipped_haul_rate,
                            FORECAST_OUTPUT_MULTIPLIER,
                        )
                    })
                    .unwrap_or_default(),
                // **One hunter's BIOMASS throughput** — the term `systems::hunt_take`'s collection
                // multiplies by the head-count, with no seasonal factor (the animal web has none).
                // It is the crew half of the composition: the vector above turns a floor into a
                // ceiling, and this turns that ceiling into a number of people. Shipped rather than
                // left to `per_worker_yield / provisions_per_biomass`, which is `0 / 0` on a wolf.
                per_worker_biomass: equipped_haul_rate,
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
                // **The THIRD bound on a take** (`docs/plan_hunt_through_combat.md` §2): how many
                // animals one hunter can bring into contact per turn. The escapement ceiling and the
                // per-worker carry already ship as terms, and a client composing `min()` from those
                // two alone quotes a carry-bound take the sim will never pay — ~30× over on a Wild
                // Fowl herd with one hunter, whose 40 biomass of carry is 307 birds against 10 of
                // reach. It is exact and linear in the crew, so it ships as a term like its two
                // siblings; the whole-animal `floor()` stays the sim's answer in `SourceYield.actual`.
                //
                // **A PEN publishes `NO_ENGAGEMENT_STAGE`** — `engage_rate_for` answers `INFINITY` for
                // an unresolvable species and a penned animal is not stalked either, so the wire's
                // finite "unbounded" reading covers both and no reader has to carry an infinity.
                engage_rate: herd
                    .filter(|herd| !herd.is_corralled())
                    .map(|herd| fauna.engage_rate_for(&herd.species))
                    .filter(|rate| rate.is_finite())
                    .unwrap_or(NO_ENGAGEMENT_STAGE),
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
                // **THE TWO INVESTMENT RUNGS' MATERIAL PAYOFFS** (arc #527) — the twins of
                // `corral_yield`/`pastoral_yield` above, and the replacement for the retired
                // `corral_trade`/`pastoral_trade`. Without them an inedible quarry's Tame and Corral
                // rungs quote nothing at all: a wolf's food payoff on both is honestly `0`.
                //
                // Priced on the **same** MSY biomass the food quotes are — the forecast hands both
                // over (`managed_yield_biomass` / `pastoral_yield_biomass`) precisely so a rung's two
                // readouts cannot describe different harvests.
                corral_material: herd
                    .map(|herd| {
                        material_rates(
                            fauna.hunt_materials_for(&herd.species),
                            forecast.managed_yield_biomass,
                            FORECAST_OUTPUT_MULTIPLIER,
                        )
                    })
                    .unwrap_or_default(),
                pastoral_material: herd
                    .map(|herd| {
                        material_rates(
                            fauna.hunt_materials_for(&herd.species),
                            forecast.pastoral_yield_biomass,
                            FORECAST_OUTPUT_MULTIPLIER,
                        )
                    })
                    .unwrap_or_default(),
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
                // **The gate's other half** (§6.5): `defense` says whether a hit counts, this says
                // how many counting hits a body takes. The client composes both — it already holds
                // the band's own `hunterAttack` — so no "can this band win" answer is exported.
                durability: species_def.map(|def| def.combat.durability).unwrap_or(0.0),
                // **`1 − wariness`, the retreat as a term** — through the sim's own
                // [`crate::fauna::stay_fraction`] rather than re-spelled, so the wire and the crew
                // /take sizing that divides by it cannot drift. Published at the **neutral
                // dispersion**: the species half of the retreat, which the client composes with its
                // chosen `KitOption.dispersion`. `1.0` for a species the roster cannot resolve —
                // nothing breaks off — which is the same reading a pen and the plant web give and
                // keeps an unresolved row from silently zeroing a take.
                stay_fraction: species_def.map_or(NO_RETREAT_STAGE_STAY, |def| {
                    crate::fauna::stay_fraction(def.combat.wariness, WIRE_NEUTRAL_DISPERSION)
                }),
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
                // **THE STANDING UPKEEP** (`docs/plan_standing_upkeep.md` §2) — what holding this
                // herd's rung demands, what its keepers supplied and what went unmet, all three
                // published so the client subtracts nothing. The demand is the ladder's own price,
                // always meaningful (the `penUpkeep` rule); the supplied/unmet pair is this turn's
                // scratch, stamped by the labor arm that resolved the keeping crew.
                upkeep_demand: herd.map_or(NO_UPKEEP_DEMAND, |herd| {
                    ladder
                        .rung(crate::fauna::herd_rung_key(herd))
                        .upkeep_demand(crate::fauna::herd_head_count(herd))
                }),
                upkeep_supplied: herd.map_or(NO_UPKEEP_DEMAND, |herd| herd.upkeep_supplied),
                upkeep_shortfall: herd.map_or(NO_UPKEEP_DEMAND, |herd| herd.upkeep_shortfall),
                // **The MAINTAIN activity's own `workers_needed`** — hands to meet the demand, in
                // its own unit. The take activity's answer rides `SourceYield::workers_needed`.
                upkeep_workers_needed: herd.map_or(NO_CREW_ON_THIS_ACTIVITY, |herd| {
                    ladder
                        .rung(crate::fauna::herd_rung_key(herd))
                        .upkeep_crew_needed(crate::fauna::herd_head_count(herd))
                }),
                // **The neglect countdown**, resolved through the *same* `herd_keeping_rung` seam
                // `advance_husbandry` gates the shed on, so the wire can never count down a grace
                // against a rung the sim is not applying. `None` = a wild herd: nobody's to keep, so
                // the pair reads "nothing at risk" rather than a zero that means "shedding now".
                has_neglect_grace: neglect_grace.is_some(),
                neglect_grace_remaining: neglect_grace.unwrap_or(NO_NEGLECT_REMAINING),
                // **The kit this quarry wants** — what the compose sheet opens on and what
                // `assign_labor … hunt <herd> <n>` resolves with no `kit` token. Off the single
                // `quoted` resolution above, so it names the kit the row's own tiers came from.
                default_kit_id: quoted.kit_id.clone(),
                // **THE BUILD, PRICED IN WORK** (`docs/plan_unit_costed_work.md` §8). `work_done` is
                // the herd's own meter; `work_cost` is what that job costs **on this herd**, resolved
                // LIVE off the ladder (times the species' `taming_cost_multiplier` for the Tame) and
                // published **whether or not a build is in flight** — the compose sheet has to quote
                // the price before the player commits, and the herd's *stamped* cost is `0` until
                // someone starts. Penning takes no species multiplier: a fence is a fence.
                tame_work_done: herd.map(|herd| herd.domestication_progress).unwrap_or(0.0),
                tame_work_cost: ladder
                    .rung(RungKey::AnimalPastoral)
                    .build_cost(fauna.taming_cost_multiplier_for(&entry.species))
                    .unwrap_or(0.0),
                corral_work_done: herd.map(|herd| herd.corral_progress).unwrap_or(0.0),
                corral_work_cost: ladder
                    .rung(RungKey::AnimalPen)
                    .build_cost(RUNG_COST_UNSCALED)
                    .unwrap_or(0.0),
                // **The turns estimate the labor arm stamped this turn** — the running build's, or,
                // when nothing is being built, the **projection** for the rung this herd would climb
                // next, so the pair reads "50 work, ≈13 turns" before the player commits. Which
                // `*WorkCost` it belongs beside is the assignment's own `improvement`, or the next
                // rung up when that is empty. `-1` only where there is genuinely no answer (penned,
                // a gate refuses, or a stalled build). The client can derive none of it.
                build_turns_remaining: herd
                    .and_then(|herd| herd.build_turns_remaining)
                    .map_or(NO_BUILD_TURNS_ESTIMATE, |turns| turns as i32),
                // **What the keepers' tools took off the running build** — quoted beside the RAW
                // `*WorkCost` above, never folded into it, so a readout can say "your hurdles: −17
                // work" against a price that does not move under the crew's kit.
                build_work_from_gear: herd
                    .map(|herd| herd.build_work_from_gear)
                    .unwrap_or(NO_BUILD_GEAR),
                // **The crew-output TERM the compose sheet evaluates its estimate from** (the
                // boundary rule in `.claude/rules/core_sim/yield-forecast.md`): what one worker banks
                // per turn. With `*WorkCost` / `*WorkDone` here and the gear pair on the band's own
                // `kitTiers` row, `turns(workers)` is a closed form the client can evaluate against a
                // *proposed* crew — which `buildTurnsRemaining` beside it cannot, because it is the
                // sim's answer for the crew already there.
                //
                // **It is the LADDER's term, not a literal.** Published so a second term landing in
                // `crew_work_output` reaches the client for free.
                build_work_per_worker_turn: build_work_per_worker_turn(),
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
#[allow(clippy::too_many_arguments)] // the registry, three configs, two lookup maps and a rate
pub(crate) fn snapshot_forage_patches(
    registry: &ForageRegistry,
    forage: &ForageLaborConfig,
    // The EQUIPPED reference gather rate — a patch row has no band to resolve a basket tier
    // against, exactly as a herd row has none for the sled's. See `HerdSnapshotInputs`.
    equipped_gather_rate: f32,
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
                equipped_gather_rate,
                // **The EQUIPPED reference rate, not any band's basket tier** — a patch row is a fact
                // about the *patch*, and a patch has no band to resolve a kit against. Exactly the
                // rule `HerdTelemetryState` already follows for the hunt's haul; a band's real,
                // kit-resolved gather rate rides its own `PopulationCohortState`
                // (`forageCarryPerWorkerBiomass`) and its `SourceYield` row.
                forage_per_worker_biomass(equipped_gather_rate, seasonal),
                FORECAST_OUTPUT_MULTIPLIER,
            );
            ForagePatchState {
                x: patch.tile.x,
                y: patch.tile.y,
                // **The wire keeps the 0..1 fraction; the meter is in work units** — divided here
                // against the patch's OWN stamped cost, so a tended patch reads exactly `1.0`
                // beside an `is_cultivated` that is already true.
                cultivation_progress: build_fraction(
                    patch.cultivation_progress,
                    patch.cultivation_cost,
                ),
                is_cultivated: patch.is_cultivated(),
                owner: patch.owner.map(|faction| faction.0),
                biomass: patch.biomass,
                carrying_capacity: patch.carrying_capacity,
                ecology_phase: patch.ecology_phase.as_str().to_string(),
                // The plant web's forecast is food-only for now — its fodder component is
                // `forage::PLANT_FODDER_FORECAST_NOT_YET_PROJECTED` (a known gap, #426), so these
                // project the provisions component rather than shipping a false `0` fodder line.
                per_worker_yield: forecast.per_worker_yield.provisions,
                // The Cultivate investment rung: the preparing dip + the payoff once cultivated.
                tended_yield: forecast.managed_yield.provisions,
                // The Sow rung (plant 3): its own two meters — independent of cultivation's, since a
                // Field may stand on ground that was never tended — and its own preparing/payoff
                // pair. `field_provisions` is the same helper the labor arm pays a Field with, so the
                // client's "then Y" is the number the sim will hand over.
                field_progress: build_fraction(patch.field_progress, patch.field_cost),
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
                // **WHAT A GATHER OF THIS PATCH IS MADE OF** (arc #527) — the material twins of the
                // two rates above, and the **rung-1** half of the material story: `FloraShareInfo`'s
                // two payoffs quote a commitment at rungs 2 and 3, and a *wild* gather had nothing
                // at all. A tile whose basket carries a cash crop read food-and-fodder-only while
                // the turn banked its fibre and leaf.
                //
                // **Through `patch_material_yields`, the very rows `credit_material_yield` is handed
                // at the take site** — which is also what makes the mixed-basket rule fall out
                // rather than being restated: it decomposes per species, each carrying its own share
                // *and its own exact reading*, and `material_yield_totals` then merges by material
                // id for the RATE. Two species that both give fibre sum into one fibre rate, which
                // is what a rate means; their readings are never averaged, because that would invent
                // a plant that is not growing there. The readings ride the batches the take creates.
                material_per_biomass: material_rates(
                    &crate::forage::patch_material_yields(patch, tile_composition, flora, forage),
                    ONE_UNIT_OF_BIOMASS,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                // The gatherer's own throughput, with the tile's **seasonal weight** folded in
                // exactly as `per_worker_yield` folds it — so this is honestly EMPTY in a dead
                // season, and a client must not divide by it.
                per_worker_material: material_rates(
                    &crate::forage::patch_material_yields(patch, tile_composition, flora, forage),
                    forage_per_worker_biomass(equipped_gather_rate, seasonal),
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                // **THE BUILD, PRICED IN WORK** (`docs/plan_unit_costed_work.md` §8). `work_done` is
                // the patch's own meter; `work_cost` is what that job costs, resolved LIVE off the
                // ladder and published **whether or not a build is in flight** — the compose sheet
                // has to quote the price before the player commits, and the patch's *stamped* cost is
                // `0` until someone starts. `RUNG_COST_UNSCALED` on both: the only per-source cost
                // multiplier on the ladder is a species' taming cost, and a plant has no species.
                cultivation_work_done: patch.cultivation_progress,
                cultivation_work_cost: ladder
                    .rung(RungKey::PlantTended)
                    .build_cost(RUNG_COST_UNSCALED)
                    .unwrap_or(0.0),
                field_work_done: patch.field_progress,
                field_work_cost: ladder
                    .rung(RungKey::PlantField)
                    .build_cost(RUNG_COST_UNSCALED)
                    .unwrap_or(0.0),
                // **The turns estimate the labor arm stamped this turn** — the running build's, or,
                // when nothing is being built, the **projection** for the rung this patch would climb
                // next, so the compose sheet can quote the job before the player commits. Read it
                // beside the `*WorkCost` for the assignment's own `improvement`, or for the next rung
                // up when that is empty. `-1` only where there is genuinely no answer (a Field, a
                // gate that refuses, or a stalled build).
                build_turns_remaining: patch
                    .build_turns_remaining
                    .map_or(NO_BUILD_TURNS_ESTIMATE, |turns| turns as i32),
                // The plant twin — `NO_BUILD_GEAR` on every plant build today, since no plant item
                // declares `EquipmentStat::BuildWork` yet (issue #539).
                build_work_from_gear: patch.build_work_from_gear,
                // The plant twin — see the herd row for why the estimate's terms ship beside the
                // sim's own answer.
                build_work_per_worker_turn: build_work_per_worker_turn(),
                // **One gatherer's BIOMASS throughput** — `per_worker_biomass_capacity × seasonal`,
                // the exact term `forage_take`'s worker cap multiplies by the head-count, through the
                // shared helper so the wire and the take cannot disagree. `0` in a dead season, like
                // `per_worker_yield` beside it. Shipped rather than left to
                // `per_worker_yield / provisions_per_biomass`, which is `0 / 0` on a Field of cotton,
                // flax or hay.
                per_worker_biomass: forage_per_worker_biomass(equipped_gather_rate, seasonal),
                // **The growth curve, sampled** — the plant twin; non-negative at every sample, and
                // its `0.0` entry is the reseed floor's lift.
                regrowth_samples: patch_regrowth_samples(patch, forage),
                // The phase bands, off the patch's OWN ecology — the same seam
                // `refresh_ecology_phase` classified the word above with.
                collapse_fraction: ecology.collapse_fraction,
                stressed_fraction: ecology.stressed_fraction,
                // **THE STANDING UPKEEP** — the plant twin; see the herd row for the seam and why
                // all three terms ship.
                upkeep_demand: ladder
                    .rung(crate::forage::patch_rung_key(patch))
                    .upkeep_demand(UNSCALED_UPKEEP),
                upkeep_supplied: patch.upkeep_supplied,
                upkeep_shortfall: patch.upkeep_shortfall,
                // **The MAINTAIN activity's own `workers_needed`** — the plant twin.
                upkeep_workers_needed: ladder
                    .rung(crate::forage::patch_rung_key(patch))
                    .upkeep_crew_needed(UNSCALED_UPKEEP),
                // **The neglect countdown**, resolved through the *same* `patch_unwinding_rung` seam
                // `advance_cultivation` bleeds through — so the wire counts down against the rung
                // that will actually revert, not one the patch merely stands on. `None` = a wild
                // patch, which is most of them.
                has_neglect_grace: neglect_grace.is_some(),
                neglect_grace_remaining: neglect_grace.unwrap_or(NO_NEGLECT_REMAINING),
                // The two investment rungs' PAYOFF twins — each projected at **its own** rung
                // (`tended_*` at rung 2, `field_*` at rung 3), never at the rung the patch happens to
                // stand on. That is the #433 rule, and getting it wrong is the exact defect #433
                // fixed: a Sow quote that inherited the tended basket's conversion gain overstated by
                // 10% on the reference tile and by the full 2× wherever weeding saturates.
                tended_fodder: tended_fodder(
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

/// **A source's material rows scaled onto one basis, in the WIRE's shape** — the sim's own
/// [`crate::materials_config::material_yield_totals`] with its rows renamed for the snapshot.
///
/// `biomass` is what the rate is being stated *per*: [`ONE_UNIT_OF_BIOMASS`] gives the per-biomass
/// rate a client composes a ceiling from, one hunter's haul gives the per-worker throughput. The
/// band's output multiplier folds in for the reason it does on every other rate on the row — a
/// material is another account of one harvest, not a parallel economy.
fn material_rates(
    rows: &[crate::materials_config::MaterialYieldDef],
    biomass: f32,
    output_multiplier: f32,
) -> Vec<MaterialPayoff> {
    crate::materials_config::material_yield_totals(rows, biomass, output_multiplier)
        .into_iter()
        .map(|payoff| MaterialPayoff {
            material_id: payoff.material,
            amount: payoff.amount,
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
