use sim_runtime::{MaterialPayoff, SpeciesMaterialRates};

use std::collections::BTreeMap;

use super::*;
use crate::fauna::{
    classify_ecology_phase, herd_capacity, herd_ecology, net_biomass_delta,
    reseeding_logistic_regrowth, NO_RETREAT_STAGE_STAY, ONE_UNIT_OF_BIOMASS,
};
use crate::forage::{
    forage_per_worker_biomass, patch_ecology, patch_fodder_per_biomass,
    patch_neglect_grace_remaining, patch_provisions_per_biomass, tended_fodder,
};
use crate::intensification::{
    build_fraction, build_work_per_worker_turn, NOT_IN_ANY_BUILD_QUEUE, NO_BUILD_GEAR,
    NO_CREW_ON_THIS_ACTIVITY, NO_RUNG_WIDTH, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED,
};
use sim_schema::{
    BUILD_METER_HOLDS, BUILD_METER_ROTS, BUILD_NOT_YET_ESTIMATED, BUILD_QUEUE_BLOCKED,
    NO_BUILD_TURNS_ESTIMATE,
};

/// **THE COUNTDOWN, ON THE WIRE** — the one place `BuildTurns` becomes an `i32`, so the plant and the
/// animal web cannot come to publish the same state as two different numbers.
///
/// The `None` case is the caller's (`map_or`'s default), because it is the *absence* of an estimate
/// rather than a variant of one.
fn published_build_turns(turns: crate::intensification::BuildTurns) -> i32 {
    match turns {
        crate::intensification::BuildTurns::Turns(count) => count as i32,
        crate::intensification::BuildTurns::Holding => BUILD_METER_HOLDS,
        crate::intensification::BuildTurns::Rotting => BUILD_METER_ROTS,
        crate::intensification::BuildTurns::Blocked => BUILD_QUEUE_BLOCKED,
    }
}

/// **THE COUNTDOWN A SOURCE ROW PUBLISHES, INCLUDING THE TWO WAYS OF HAVING NO ANSWER** — the one
/// seam both webs' rows go through, so a patch and a herd can never resolve the same state to two
/// different numbers.
///
/// `Some(turns)` is an answer and maps straight through [`published_build_turns`]. `None` is where
/// two different facts used to be folded into one sentinel:
///
/// - **the estimate pass ran and had no number** — nobody works the source, its gate refuses, or a
///   running build banked nothing and is genuinely **stalled** → [`NO_BUILD_TURNS_ESTIMATE`];
/// - **no estimate pass has ever run for this entry** — the player queued it since the last turn
///   resolved → [`sim_schema::BUILD_NOT_YET_ESTIMATED`].
///
/// # THE TEST IS THE ESTIMATE PASS, NEVER THE METER
///
/// A genuinely stalled build sits at `0%` too, so progress cannot separate them. What can is that
/// `publish_build_chain` stamps **every entry it walks** with that entry's 0-based place in the line
/// ([`crate::intensification::NOT_IN_ANY_BUILD_QUEUE`] otherwise), and the Logistics decay passes
/// clear the place back to that sentinel every turn — so *"in a band's **live** queue **and** still
/// carrying the cleared place"* is exactly *"queued since the last pass"*.
///
/// **Both terms are load-bearing.** The live-queue term is what stops every unworked patch on the map
/// — which also carries the cleared place — reading as a build waiting to start; the position term is
/// what stops a genuinely stalled entry, which *is* live-queued, reading the same way.
///
/// **`queued_live` is read off the bands' own queues** ([`BuildKitIds`]), not off the turn-written
/// row, for the reason the kit beside it is: the row's scratch lags a command by a whole turn, and
/// this state exists precisely in the frame before that turn.
fn published_build_countdown(
    turns: Option<crate::intensification::BuildTurns>,
    stamped_position: i32,
    queued_live: bool,
) -> i32 {
    match turns {
        Some(turns) => published_build_turns(turns),
        None if queued_live && stamped_position == NOT_IN_ANY_BUILD_QUEUE => {
            BUILD_NOT_YET_ESTIMATED
        }
        None => NO_BUILD_TURNS_ESTIMATE,
    }
}

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
///
/// **On a pen, *unbounded* is the wrong reading and the field is simply unpublished** — the tend
/// branch is bounded by [`crate::fauna::herd_engage_rate`] like every other rung. See the
/// `engage_rate` site below and issue #572.
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

/// **WHERE THE QUEUED ENTRY IS TAKING THIS SOURCE, in the wire's spelling** — the destination rung as
/// `<branch>:<id>`, or `""` for a source no band has queued.
fn published_destination_rung(destination: Option<crate::intensification::RungKey>) -> String {
    destination.map_or_else(String::new, |rung| rung.wire_key())
}

/// **THE LEGS THE ENTRY STILL HAS TO LAY**, each with the chained date the publish pass struck.
///
/// **The client must not re-derive these.** The rung spans are the sim's config, the source's
/// position is the sim's state, and the dates are chained against a build queue the client cannot
/// see — so a client reconstructing them would be a second producer of a verdict that has one.
fn published_build_legs(
    legs: &[crate::intensification::PublishedBuildLeg],
) -> Vec<sim_runtime::BuildLegState> {
    legs.iter()
        .map(|published| sim_runtime::BuildLegState {
            rung: published.leg.rung.wire_key(),
            work_remaining: published.leg.work_remaining,
            turns_remaining: published
                .turns
                .map_or(NO_BUILD_TURNS_ESTIMATE, published_build_turns),
        })
        .collect()
}

/// **THE KIT EACH QUEUED SOURCE'S BUILD IS BEING RAISED WITH**, keyed the two ways a source is named
/// (`docs/plan_standing_upkeep.md` §4.7a ②) — the wire's `buildKitId` on both source tables.
///
/// # IT IS READ LIVE, NOT STAMPED BY THE TURN
///
/// `build_queue_position` beside it on the same row is scratch the labor pass writes, so it lags a
/// command by a whole turn; the server re-captures and broadcasts after **every** dispatched command
/// (`recapture_and_broadcast`), so a kit picked on a queue row has to be visible in that frame. This
/// walks the bands' live queues instead.
///
/// # AND IT RIDES THE SAME WINNING BAND AS THE POSITION
///
/// Several bands may work one source, and the position on the row is the one that band published.
/// A kit taken from a *different* band's queue beside that position would be two answers pretending
/// to be one — so the winner here is **the band whose live entry sits where the row says**, and a
/// band that does not match only fills a source no matching band claimed.
#[derive(Default)]
pub(crate) struct BuildKitIds {
    patches: HashMap<UVec2, String>,
    herds: HashMap<String, String>,
}

impl BuildKitIds {
    /// The kit id this patch's entry resolves to, `""` when no band has it queued.
    fn patch(&self, tile: UVec2) -> String {
        self.patches.get(&tile).cloned().unwrap_or_default()
    }

    /// The animal twin, keyed by herd id.
    fn herd(&self, id: &str) -> String {
        self.herds.get(id).cloned().unwrap_or_default()
    }

    /// **IS THIS PATCH IN SOME BAND'S LIVE QUEUE?** — membership of the same index the kit comes
    /// out of, which is built by walking the bands' `build_queue`s rather than by reading the
    /// turn-written row.
    ///
    /// It is asked, and cannot be replaced by a `!patch(tile).is_empty()` test, because a resolved
    /// builders kit is **never** the empty string: `builders_kit_for` always names a roster entry,
    /// the bare-handed one included.
    fn patch_is_queued(&self, tile: UVec2) -> bool {
        self.patches.contains_key(&tile)
    }

    /// The animal twin — see [`Self::patch_is_queued`].
    fn herd_is_queued(&self, id: &str) -> bool {
        self.herds.contains_key(id)
    }
}

/// **The one place a band's live queue becomes the wire's `buildKitId`** — see [`BuildKitIds`].
pub(crate) fn resolve_build_kit_ids<'a>(
    allocations: impl Iterator<Item = &'a crate::components::LaborAllocation>,
    forage: &ForageRegistry,
    herds: &HerdRegistry,
    equipment: &crate::equipment_config::EquipmentConfig,
) -> BuildKitIds {
    let mut resolved = BuildKitIds::default();
    // A band matching the row's own published position is the winner; anything else is only a
    // fallback for a source no matching band claimed, so it must never displace one.
    let mut claimed_patches: std::collections::HashSet<UVec2> = std::collections::HashSet::new();
    let mut claimed_herds: std::collections::HashSet<String> = std::collections::HashSet::new();
    for allocation in allocations {
        for (position, entry) in allocation.build_queue.iter().enumerate() {
            let branch = match entry.source {
                crate::components::BuildSource::Patch(_) => {
                    crate::intensification::RungBranch::Plant
                }
                crate::components::BuildSource::Herd(_) => {
                    crate::intensification::RungBranch::Animal
                }
                crate::components::BuildSource::Road(_) => {
                    crate::intensification::RungBranch::Route
                }
            };
            // **The one resolution seam**, so the row cannot state a kit the pool is not using.
            let kit = equipment
                .builders_kit_for(entry.kit.as_ref(), Some(branch))
                .id()
                .to_string();
            let position = position as i32;
            match &entry.source {
                crate::components::BuildSource::Patch(tile) => {
                    let wins = forage
                        .patch(*tile)
                        .is_some_and(|patch| patch.build_queue_position == position);
                    if wins || !claimed_patches.contains(tile) {
                        resolved.patches.insert(*tile, kit);
                    }
                    if wins {
                        claimed_patches.insert(*tile);
                    }
                }
                crate::components::BuildSource::Herd(id) => {
                    let wins = herds
                        .herds
                        .iter()
                        .find(|herd| &herd.id == id)
                        .is_some_and(|herd| herd.build_queue_position == position);
                    if wins || !claimed_herds.contains(id) {
                        resolved.herds.insert(id.clone(), kit);
                    }
                    if wins {
                        claimed_herds.insert(id.clone());
                    }
                }
                // **A road publishes no `buildKitId`**, because it has no source row to publish one
                // on: the two food webs key this map by patch tile and herd id, which are the rows
                // the wire carries. A road's kit is `default_kits.builders` bare-handed and states
                // itself; when the route branch gains a per-tile build readout it takes a key here.
                crate::components::BuildSource::Road(_) => {}
            }
        }
    }
    resolved
}

/// **WHAT EACH WORKED SOURCE IS KEPT WITH**, keyed the two ways a source is named
/// (`docs/plan_standing_upkeep.md` §2.7) — the wire's `upkeepKitId` / `upkeepKitNamed` on both
/// source tables.
///
/// # IT IS READ LIVE, NOT STAMPED BY THE TURN
///
/// [`BuildKitIds`]'s rule, for its reason: the server re-captures and broadcasts after **every**
/// dispatched command, so a kit picked on a row has to be visible in that frame where a turn-written
/// field would lag a whole turn. This walks the bands' live assignment rows.
///
/// # A STATED OVERRIDE BEATS A DERIVATION, AND THE FIRST STATED ONE WINS
///
/// Several bands may work one source. Every band that named nothing answers the **same** derived
/// kit — it is a pure function of the site's web — so the only way two bands can disagree is that one
/// of them stated something, and publishing the derivation over a real pick would hide the pick
/// entirely. Among two bands that both stated one the first in iteration order wins, which is the
/// same arbitrary-but-deterministic fallback `BuildKitIds` uses for a source no band claims.
#[derive(Default)]
pub(crate) struct UpkeepKitIds {
    patches: HashMap<UVec2, ResolvedUpkeepKit>,
    herds: HashMap<String, ResolvedUpkeepKit>,
}

/// One site's answer: the kit its keepers carry, and whether a band stated it.
#[derive(Clone, Default)]
struct ResolvedUpkeepKit {
    id: String,
    /// **Not recoverable from the id** — a player may name the very kit the derivation would have
    /// picked — which is why it rides the wire beside it rather than being re-derived on the client.
    named: bool,
}

impl UpkeepKitIds {
    /// The kit this patch's keepers carry and whether it was stated, `("", false)` when no band of
    /// the faction works it.
    fn patch(&self, tile: UVec2) -> (String, bool) {
        self.patches
            .get(&tile)
            .map_or_else(Default::default, |kit| (kit.id.clone(), kit.named))
    }

    /// The animal twin, keyed by herd id.
    fn herd(&self, id: &str) -> (String, bool) {
        self.herds
            .get(id)
            .map_or_else(Default::default, |kit| (kit.id.clone(), kit.named))
    }
}

/// **The one place a band's live rows become the wire's `upkeepKitId`** — see [`UpkeepKitIds`].
pub(crate) fn resolve_upkeep_kits<'a>(
    allocations: impl Iterator<Item = &'a crate::components::LaborAllocation>,
    equipment: &crate::equipment_config::EquipmentConfig,
) -> UpkeepKitIds {
    let mut resolved = UpkeepKitIds::default();
    for allocation in allocations {
        for assignment in &allocation.assignments {
            let (branch, key) = match &assignment.target {
                crate::components::LaborTarget::Forage { tile, .. } => (
                    crate::intensification::RungBranch::Plant,
                    SourceKey::Patch(*tile),
                ),
                crate::components::LaborTarget::Hunt { fauna_id, .. } => (
                    crate::intensification::RungBranch::Animal,
                    SourceKey::Herd(fauna_id.clone()),
                ),
                // A band-wide role stands on no ground, so it keeps nothing.
                _ => continue,
            };
            // **The one resolution seam**, so the row cannot state a kit the keepers are not using.
            let entry = ResolvedUpkeepKit {
                id: equipment
                    .keeping_kit_for(assignment.upkeep_kit.as_ref(), branch)
                    .id()
                    .to_string(),
                named: assignment.upkeep_kit.is_some(),
            };
            let slot = match key {
                SourceKey::Patch(tile) => resolved.patches.entry(tile).or_default(),
                SourceKey::Herd(id) => resolved.herds.entry(id).or_default(),
            };
            // A stated override beats a derivation; among two stated ones the first wins, so a slot
            // already carrying a named pick is never displaced. An empty slot is the fresh entry in
            // the map, which every band fills.
            if (entry.named && !slot.named) || slot.id.is_empty() {
                *slot = entry;
            }
        }
    }
    resolved
}

/// The two ways a worked source is named, so [`resolve_upkeep_kits`]'s two arms share one body.
enum SourceKey {
    Patch(UVec2),
    Herd(String),
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
    /// **THE LAND THE DESTINATION CAPACITY IS STRUCK OVER** — the same registry
    /// `fauna::advance_herds` sums a herd's `K` from, because
    /// [`crate::fauna::herd_destination_capacity`] *is* that seam at a second standing. Without it
    /// the row would have to re-derive the flow, and a second producer of a capacity is exactly the
    /// drift this arc keeps paying for.
    pub(crate) graze: &'a crate::graze::GrazeRegistry,
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
    /// A pen has no fight stage, so its default is the kit that can still carry the meat home and
    /// not the range scorer's winner — see [`crate::fauna::herd_default_hunt_kit`]. It is a
    /// suggestion, not a rate: the *carry* is band-wide either way (issue #543). With no carrying
    /// kit on the roster this map holds the *same* choice the range map does, and a penned herd
    /// reads exactly as it did before.
    pub(crate) penned_parties: &'a HashMap<String, QuotedParty>,
    /// **The party for a herd whose species the roster cannot resolve** — the hunt job's default,
    /// resolved unbounded, which is the same fallback every other unresolved field on the row gives.
    /// Answers for a penned herd too: with no species there is no row to quote either axis from.
    pub(crate) fallback_party: &'a QuotedParty,
    /// **The live builders kit per queued source** — the animal half of the same map the patch rows
    /// read, resolved off the bands' queues at capture. See [`BuildKitIds`].
    pub(crate) build_kits: &'a BuildKitIds,
    /// **The live keeping kit per worked source** — the animal half of the same map the patch rows
    /// read, resolved off the bands' rows at capture. See [`UpkeepKitIds`].
    pub(crate) upkeep_kits: &'a UpkeepKitIds,
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
        graze,
        parties,
        penned_parties,
        fallback_party,
        build_kits,
        upkeep_kits,
        ..
    } = inputs;
    let width = grid_size.x.max(1);
    let height = grid_size.y.max(1);
    // **The prey layer the CARNIVORE arm of the `K` seam reads**, for the destination quote below.
    // Built **only when something on the map is actually climbing**, because it is a pass over every
    // herd and no shipped carnivore can carry a husbandry destination (`husbandry_ceiling: wild`) —
    // an empty slice would nevertheless quote a pack's destination `K` as zero, so the index is real
    // whenever any quote is taken rather than assumed unreachable.
    let prey_index = if registry
        .herds
        .iter()
        .any(|herd| herd.build_destination.is_some())
    {
        crate::fauna::build_prey_index(&registry.herds, fauna)
    } else {
        Vec::new()
    };
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
            // pen has no fight stage, so the range score is meaningless there, which no score
            // against the *species* can say (`fauna::herd_default_hunt_kit`). A
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
                        build_fraction(
                            herd.rung_work_done(RungKey::AnimalPastoral, ladder),
                            herd.rung_cost(RungKey::AnimalPastoral, ladder),
                        )
                    })
                    .unwrap_or(entry.domestication),
                corralled: herd
                    .map(|herd| herd.is_corralled())
                    .unwrap_or(entry.corralled),
                // **THE RAW METER, not the standing's credit.** `animal:pen` is `on_completion`, so
                // its credit is `0` until the fence closes — that rule governs what a half-built pen
                // is *worth*, never what its progress bar reads. A player fencing a range watches it
                // fill, exactly as the plant web publishes `cultivationProgress`.
                corral_progress: herd
                    .map(|herd| {
                        build_fraction(
                            herd.rung_work_done(RungKey::AnimalPen, ladder),
                            herd.rung_cost(RungKey::AnimalPen, ladder),
                        )
                    })
                    .unwrap_or(entry.corral_progress),
                per_worker_yield: forecast.per_worker_yield.provisions,
                // The Corral investment rung's (gross) payoff once penned; the preparing dip is
                // `hunt_policy_ceilings[stance] × corral_build_fraction` (issue #442).
                corral_yield: forecast.managed_yield.provisions,
                // The pen as a managed population: whether its grass and hay are feeding it.
                // `pen_fed_fraction` is the value the keeper's tend branch wrote this turn (Population
                // runs before the capture), so the client reads the CURRENT turn's feeding, and `1.0`
                // for anything unpenned.
                //
                // **`pen_upkeep` is RETIRED** — `upkeep_per_biomass × biomass`, the running FOOD cost
                // quoted here beside `corral_yield` so a pre-commit Corral row could subtract one from
                // the other. A pen eats grass and hay, so there is no food-unit running cost to
                // subtract and the payoff stands alone.
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
                // an unresolvable species, and the wire's finite reading of it spares a reader an
                // infinity to divide by.
                //
                // ⛔ **For a pen that `0` is no longer the truth.** Since §4.9 item 12b a penned
                // herd is engaged, retreats and fights through the very same `systems::hunt_take` the
                // range runs — its reach is `fauna::herd_engage_rate` (the species' rate × the pen's
                // handling gain) — so it has a real engagement bound and always did have one under
                // the take path that preceded this. A reader following the schema's `<= 0 ⇒
                // unbounded` rule therefore quotes a penned collection above what the sim pays. The
                // published value stays `0` here: a real rate on this field flips the gate clients
                // use to route pens away from the hunt paths. Issue #572 tracks closing it.
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
                //
                // `pen_extend_cost` is that meter's DENOMINATOR, in the same work units, and is read
                // from the SAME `herd` in the same expression so the pair can never come from two
                // reads. Both are `0.0` with no ring in flight (and on the turn one is begun, before
                // `accrue_pen_extension` stamps the cost), which is why the client's percentage has
                // to guard the zero denominator rather than assume one.
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
                pen_extend_cost: herd.map(|herd| herd.pen_extend_cost).unwrap_or(0.0),
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
                // Herd staffing — the keepers a flock of this species and this size WANTS (0 for a
                // wild/unmanaged one, per `herd_herders_needed`) and how well it is kept. Both
                // resolve through the ladder's `upkeep` now (`docs/plan_standing_upkeep.md` §2.4):
                // the count is the rung's `upkeep_crew_needed` at this herd's keeper load, and the
                // ratio is derived from the one stored supply, so the published pair and the shed the
                // sim applies can never describe different staffings.
                //
                // **It is the HEAD-COUNT requirement, not the bill's crew** — position-independent,
                // so it does not slide while a `Tame` fills. `upkeepWorkersNeeded` below is the hands
                // the *bill* takes; the two agree at the top of a rung and diverge below it. See
                // `fauna::herd_herders_needed` for why this one must not interpolate.
                herders_needed: herd
                    .map(|herd| herd_herders_needed(herd, fauna, ladder))
                    .unwrap_or(0),
                herded_fraction: herd
                    .map(|herd| crate::fauna::herd_herded_fraction(herd, fauna, ladder))
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
                // the corral-tend branch wrote. **It is the whole of the feed split beside
                // `pen_pasture_fraction`**: grass and hay in one unit against one demand, so the
                // client draws "fed by pasture NN% · hay X.X" and the remainder, if any, is what the
                // herd starves for. `0.0` for an unpenned/absent herd or one no hay reached.
                //
                // **`pen_larder_bill` and `pen_hay_food` are RETIRED with the larder feed** — the
                // FOOD-unit third term and the hay-in-food-units conversion that only existed to sit
                // in the same row as it.
                fodder_draw: herd.map(|herd| herd.fodder_draw).unwrap_or(0.0),
                // **How much more fodder this pen needs per turn** — `max(0, hay need −
                // fodder_draw)`, in fodder units, where the hay need is the gap the pen's own
                // footprint leaves (`max(0, demand_grass − footprint_intake)`). The row reads "40%
                // pasture · 7% fodder · needs 11.3 more/turn" off it and `pen_pasture_fraction`.
                //
                // **The gap itself is not published** — it rode this row as `pen_hay_need` and
                // nothing read it, because what a pen row states is how much MORE it needs. The band
                // -level roll-up of the gross gap is `PopulationCohortState::fodder_need`.
                //
                // Stamped by the corral arm on the same pass as both its terms, so it cannot
                // describe a different turn from them; **ungated by Foddering** unlike the draw, so a
                // band that cannot hay at all publishes its whole need as its shortfall. `0.0` for an
                // unpenned/absent herd and for a pen its own land feeds.
                pen_fodder_shortfall: herd.map(|herd| herd.pen_fodder_shortfall).unwrap_or(0.0),
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
                // dispersion**: the herd's half of the retreat, which the client composes with its
                // chosen `KitOption.dispersion`.
                //
                // **AT THE HERD'S OWN RUNG when there is a herd** ([`crate::fauna::herd_wariness`]):
                // a fence calms the animals (`husbandry.pen_wariness`) rather than deleting the
                // stage, so a pen row that published the wild `wariness` would size a crew against a
                // retreat its keepers never run. The species-table fall-back is for a row whose herd
                // is out of the registry, and `1.0` for a species the roster cannot resolve —
                // nothing breaks off — which is the reading the plant web gives and keeps an
                // unresolved row from silently zeroing a take.
                stay_fraction: match herd {
                    Some(herd) => crate::fauna::stay_fraction(
                        crate::fauna::herd_wariness(herd, fauna),
                        WIRE_NEUTRAL_DISPERSION,
                    ),
                    None => species_def.map_or(NO_RETREAT_STAGE_STAY, |def| {
                        crate::fauna::stay_fraction(def.combat.wariness, WIRE_NEUTRAL_DISPERSION)
                    }),
                },
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
                    .map(|herd| would_be_herders_needed(herd, fauna, ladder))
                    .unwrap_or(0),
                // **THE STANDING UPKEEP** (`docs/plan_standing_upkeep.md` §2) — what holding this
                // herd's rung demands, what its keepers supplied and what went unmet, all three
                // published so the client subtracts nothing. The demand is the ladder's own price,
                // always meaningful (the `penUpkeep` rule); the supplied/unmet pair is this turn's
                // scratch, stamped by the labor arm that resolved the keeping crew.
                // Every one of them resolves through the **keeping** rung
                // (`fauna::herd_keeping_rung` — the newest meter with progress on it), the same seam
                // `advance_husbandry` sheds through and the grace below counts down against, so a row
                // cannot bill one rung's demand while the sim judges another's.
                upkeep_demand: herd.map_or(NO_UPKEEP_DEMAND, |herd| {
                    crate::fauna::herd_keeping_basis(herd, fauna, ladder)
                }),
                upkeep_supplied: herd.map_or(NO_UPKEEP_DEMAND, |herd| herd.upkeep_supplied),
                // **Derived, so the three always describe one turn and one rung** — a stored
                // shortfall would be stamped only on herds some band is assigned to, and would read
                // `0` on exactly the abandoned herds that are shedding.
                upkeep_shortfall: herd.map_or(NO_UPKEEP_DEMAND, |herd| {
                    crate::fauna::herd_upkeep_shortfall(herd, fauna, ladder)
                }),
                // **HANDS TO MEET THE DEMAND** — and published while the rung is still being
                // **built** too, where it means exactly the same thing: the keeping pool owes the
                // rate from the first work banked, so these are the hands that hold a half-tamed herd
                // as much as a finished one (`docs/plan_standing_upkeep.md` §4.6a). It is **not** a
                // minimum viable build crew — a build crew supplies nothing toward the rate — and it
                // read `0` mid-build on the older premise that an unfinished meter owed no keeping.
                // The take activity's answer rides `SourceYield::workers_needed`.
                //
                // **⛔ IT IS THE `ceil` OF THE BILL DIRECTLY ABOVE, NOT OF `herders_needed`.** The wire
                // states the identity `upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT)`
                // and tells the client to do no arithmetic of its own, so the two terms must come off
                // one number. This line used to read `herd_herders_needed`, which answers a different
                // question — the *head-count* requirement at the rung's bare rate — and stopped
                // agreeing the moment the animal keeping demand began **interpolating on the herd's
                // position**: a herd a tenth of the way up a Tame was billed `0.185` work and told to
                // staff **two** keepers. The plant row was already `ceil` of its own basis; this is
                // the same seam (`fauna::herd_upkeep_workers_needed`).
                upkeep_workers_needed: herd.map_or(NO_CREW_ON_THIS_ACTIVITY, |herd| {
                    crate::fauna::herd_upkeep_workers_needed(herd, fauna, ladder)
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
                tame_work_done: herd
                    .map(|herd| herd.rung_work_done(RungKey::AnimalPastoral, ladder))
                    .unwrap_or(0.0),
                tame_work_cost: ladder
                    .rung(RungKey::AnimalPastoral)
                    .build_cost(fauna.taming_cost_multiplier_for(&entry.species))
                    .unwrap_or(0.0),
                corral_work_done: herd
                    .map(|herd| herd.rung_work_done(RungKey::AnimalPen, ladder))
                    .unwrap_or(0.0),
                corral_work_cost: ladder
                    .rung(RungKey::AnimalPen)
                    .build_cost(RUNG_COST_UNSCALED)
                    .unwrap_or(0.0),
                // **AND WHAT HOLDING IT WILL COST** — the *second* term of the same quote, resolved
                // here for the same reason and by the same rule as the `*_work_cost` above: off the
                // **ladder**, per rung, **whether or not a build is in flight**. A price with no
                // standing bell beside it is half a quote: the player is committing to a rate their
                // keeping pool will owe forever, from the first work banked.
                //
                // **It is NOT netted off the build** (`docs/plan_standing_upkeep.md` §4.6a) — the
                // keeping owes it whatever the builders do, so `work_cost / crew` really is the
                // pace. What a build's closed form nets is `meter_rot_per_turn`, published below.
                //
                // **`upkeep_demand` above cannot answer this**, and deliberately: it is what this
                // herd is *billed* today — `0` on a herd nobody has started, which is exactly the
                // herd a compose sheet is looking at.
                //
                // **⛔ THE TWO NO LONGER COINCIDE ONCE A BUILD IS IN FLIGHT.** This line used to say
                // they did, and that stopped being true when the animal keeping demand began
                // **interpolating on the herd's position**: a herd part-way up the pastoral rung is
                // billed a *fraction* of that rung's rate, so the bill sits strictly below this quote
                // for the whole build and meets it only at the rung's top. That is the plant web's
                // shape exactly, and `build_turns_closed_form` pins it as the ordering
                // `0 < billed <= quoted` rather than as an equality.
                //
                // **At this herd's own keeper load**, because both animal rungs quote their rate per
                // keeper-load (`scaled_by: source_load`), and the load is ownership-independent —
                // `herders_needed_if_managed`'s rule, for its reason: a quote has to exist before
                // the herd is anyone's.
                tame_upkeep_demand: herd.map_or(NO_UPKEEP_DEMAND, |herd| {
                    ladder
                        .rung(RungKey::AnimalPastoral)
                        .upkeep_demand(crate::fauna::herd_keeper_load(herd, fauna))
                }),
                corral_upkeep_demand: herd.map_or(NO_UPKEEP_DEMAND, |herd| {
                    ladder
                        .rung(RungKey::AnimalPen)
                        .upkeep_demand(crate::fauna::herd_keeper_load(herd, fauna))
                }),
                // **THE MATERIAL TWIN OF THAT PAIR** — the rung's own rate at this herd's keeper
                // load, resolved live because it prices a rung the herd may not be on. It is what
                // makes the `⌃` track's aside reachable at all: a **pastoral** herd is the only
                // source that track ever offers the Pen rung from, and its own rung declares no
                // material, so the stamped `upkeep_material_demand` beside it is empty on exactly
                // the row the player is deciding on.
                tame_upkeep_material_demand: herd.map_or_else(Vec::new, |herd| {
                    rung_material_rate(
                        ladder,
                        RungKey::AnimalPastoral,
                        crate::fauna::herd_keeper_load(herd, fauna),
                    )
                }),
                corral_upkeep_material_demand: herd.map_or_else(Vec::new, |herd| {
                    rung_material_rate(
                        ladder,
                        RungKey::AnimalPen,
                        crate::fauna::herd_keeper_load(herd, fauna),
                    )
                }),
                // **THE BUILD TWIN OF THAT PAIR, AND THE ONLY PLACE A RING'S PILE IS PRICED** —
                // the whole `animal:pen` build pile, at full coverage. It exists for word-for-word
                // the reason `corral_upkeep_material_demand` above does: a **corralled** herd is the
                // only source a ring is ever offered from, `RungKey::AnimalPen.above()` is `None`,
                // and `build_material_cost` below is therefore empty on exactly the row the player
                // is deciding on. `systems::labor::head_ring_leg` prices a ring's width at this same
                // rung's `build_cost(RUNG_COST_UNSCALED)` and `build_material_wants` draws this same
                // rung's pile against it, so a whole ring swallows exactly this list.
                //
                // **It takes NO herd**, unlike every neighbour here: those scale by
                // `herd_keeper_load`, and this one must not. Penning carries no per-species
                // multiplier and the ring's width is unscaled, so a scaled quote would show one
                // price and charge another. The pile is a property of the ladder rung alone.
                //
                // On a **pastoral** herd this equals `build_material_cost` below **by
                // construction** — the same `rung_material_pile(ladder, RungKey::AnimalPen)` reached
                // through two selectors, never a second reading of the ladder.
                corral_build_material_cost: rung_material_pile(ladder, RungKey::AnimalPen),
                // **The plant twin's field**, so a client's build estimate is one expression across
                // both webs — and **always `0` here**, because neither animal rung declares a
                // `meter_decay`: an under-kept flock sheds animals instead. Nothing eats an animal
                // build. See `fauna::herd_meter_rot`; do not read the zero as a gap.
                meter_rot_per_turn: herd.map_or(NO_UPKEEP_DECAY, |herd| {
                    crate::fauna::herd_meter_rot(herd, fauna, ladder)
                }),
                // **The turns estimate the labor arm stamped this turn** — the running build's, or,
                // when nothing is being built, the **projection** for the rung this herd would climb
                // next, so the pair reads "50 work, ≈13 turns" before the player commits. Which
                // `*WorkCost` it belongs beside is the assignment's own `improvement`, or the next
                // rung up when that is empty. `-1` only where there is genuinely no answer (penned,
                // a gate refuses, or a stalled build). The client can derive none of it.
                // **FOUR NEGATIVES, FOUR FACTS** (`intensification::BuildTurns`): `-1` where
                // there is genuinely no answer (nothing queued here, a gate refuses a waiting
                // entry, the top of the ladder); `-2` where the net supply is exactly zero, so the
                // meter holds where it is; `-3` where it is negative, so the meter is going
                // backwards; and `-4` where the band's **builders are staffed and standing on this
                // entry** and its own gate refuses it, so the whole queue is stuck behind it. The
                // last three are the ones the player can act on, and they are three answers because
                // holding wastes a turn, rotting destroys bought work, and a block is fixed by
                // staffing the KEEPING rather than by adding builders.
                build_destination_rung: published_destination_rung(
                    herd.and_then(|herd| herd.build_destination),
                ),
                // **THE RUNG THIS HERD STANDS ON**, beside the one it is headed for. Through
                // `fauna::herd_rung_key`, the single home of that test — never a second reading of
                // `domestication`/`corralled` here. `snapshot.fbs`'s `currentRung` carries the why.
                // The unreachable "in telemetry, gone from the registry" row falls back to the
                // branch's bottom rung, like every other field here falls back rather than lying.
                current_rung: herd
                    .map_or(RungKey::AnimalWild, crate::fauna::herd_rung_key)
                    .wire_key(),
                // **THE MATERIAL HALF OF THE LADDER'S PRICE** (`docs/plan_standing_upkeep.md` §2.7).
                // The pile is the rung the `⌃` track would offer next — `RungKey::above` the one the
                // herd stands on — resolved off the **ladder** at capture and published whether or
                // not a build is in flight, exactly as `*WorkCost` beside it is. Empty at the top of
                // the branch, which is the honest reading rather than a repeat of the pen's own —
                // **and the pen's own pile is published beside it as `corral_build_material_cost`**,
                // which is the field a ring card reads. Without that name here the emptiness reads
                // as "a ring is free", and the gap gets re-derived.
                build_material_cost: herd
                    .map(|herd| {
                        crate::fauna::herd_rung_key(herd)
                            .above()
                            .map_or_else(Vec::new, |next| rung_material_pile(inputs.ladder, next))
                    })
                    .unwrap_or_default(),
                // **WHAT HOLDING THIS RUNG SWALLOWS PER TURN** — the STAMPED bill, through
                // `fauna::herd_material_keeping_basis`, so the published pair satisfies the same
                // `demand − supplied == shortfall` identity the work trio does.
                upkeep_material_demand: herd
                    .map(|herd| {
                        material_payoffs(&crate::fauna::herd_material_keeping_basis(
                            herd,
                            inputs.fauna,
                            inputs.ladder,
                        ))
                    })
                    .unwrap_or_default(),
                upkeep_material_supplied: herd
                    .map(|herd| material_payoffs(&herd.upkeep_materials_supplied))
                    .unwrap_or_default(),
                build_legs: herd
                    .map_or_else(Vec::new, |herd| published_build_legs(&herd.build_legs)),
                // **WHERE THAT DESTINATION LEAVES THIS HERD'S `K`** — `None` (the wire's sentinel)
                // when no band has queued it, which is a different statement from a capacity of
                // zero. Read through `fauna::herd_destination_capacity`, i.e. through the **one**
                // seam that writes the live `carrying_capacity` above, evaluated at the destination
                // standing — never a second expression that happens to agree today.
                build_destination_capacity: herd.and_then(|herd| {
                    crate::fauna::herd_destination_capacity(
                        herd,
                        species_def,
                        graze,
                        &prey_index,
                        fauna,
                        width,
                        height,
                        wrap_horizontal,
                    )
                }),
                // **The countdown, with "queued but never estimated" told apart from "no answer"** —
                // see [`published_build_countdown`]. A herd the viewer cannot see at all publishes
                // the plain no-answer sentinel: an unseen source is not a build waiting to start.
                build_turns_remaining: herd.map_or(NO_BUILD_TURNS_ESTIMATE, |herd| {
                    published_build_countdown(
                        herd.build_turns_remaining,
                        herd.build_queue_position,
                        build_kits.herd_is_queued(&entry.id),
                    )
                }),
                // **What the keepers' tools ADD to the running build each turn** — quoted beside
                // the `*WorkCost` above, never folded into it, so a readout can say "your hurdles:
                // +9 work a turn" against a price no tool can move (§4.8).
                build_work_from_gear: herd
                    .map(|herd| herd.build_work_from_gear)
                    .unwrap_or(NO_BUILD_GEAR),
                // **Where this herd sits in that same band's queue** — the third of the set, and
                // what makes a chained date legible rather than an unexplained number.
                build_queue_position: herd
                    .map(|herd| herd.build_queue_position)
                    .unwrap_or(NOT_IN_ANY_BUILD_QUEUE),
                // **And WHY the pool is stuck, when `buildTurnsRemaining` reads `-4`** — the
                // conjunct of the rung's own gate that refused, `""` when this herd is not a blocked
                // build. Read live off the `Herd` like every other build field, and off the same
                // winner: the chain pass stamps the cause with the countdown it belongs to.
                build_blocked_reason: herd
                    .map(|herd| herd.build_blocked_reason.key().to_string())
                    .unwrap_or_default(),
                // **The BARE crew-output term of the compose sheet's closed form** (the boundary
                // rule in `.claude/rules/core_sim/yield-forecast.md`): what one worker banks per
                // turn carrying nothing. With `*WorkCost` / `*WorkDone` here and the gear pair on
                // the band's own `kitTiers` row,
                //
                // ```text
                // gear(w)  = min(w, buildWorkSaturatingCrew) × buildWorkPerWorker
                // turns(w) = ceil((workCost − workDone)
                //                 / (w × buildWorkPerWorkerTurn + gear(w) − meterRotPerTurn))
                // ```
                //
                // is a closed form the client can evaluate against a *proposed* crew — which
                // `buildTurnsRemaining` beside it cannot, because it is the sim's answer for the
                // crew already there.
                //
                // **⛔ THE GEAR TERM MOVED FROM THE NUMERATOR TO THE DENOMINATOR** (§4.8). It used
                // to be subtracted from the job (`workCost − workDone − gear(w)`); a kit raises what
                // a worker delivers now, so it is an addend on the supply. **Both terms stay on
                // `kitTiers`, and the saturation with them** — coverage arms a *prefix* of a pool,
                // so an eleventh keeper with ten sets of hurdles between them adds only their own
                // hands. Publishing a pre-averaged pool rate here instead would have lost exactly
                // that, and lost it silently on the one crew a compose sheet is *for*: a proposed
                // one, of a size the sim never resolved.
                build_work_per_worker_turn: build_work_per_worker_turn(NO_BUILD_GEAR),
                // **What this herd's build is being raised with** — the animal twin of the patch
                // row's, resolved live off the winning band's queue entry.
                build_kit_id: build_kits.herd(&entry.id),
                upkeep_kit_id: upkeep_kits.herd(&entry.id).0,
                upkeep_kit_named: upkeep_kits.herd(&entry.id).1,
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
/// and a Field is gathered through the same seasonless `forage_take` path as every rung beneath it, so it forecasts correctly regardless. Captured at
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
///
/// `tile_capacities` maps tile coord → that tile's own forage `K` (`forage::tile_forage_capacity`),
/// filled by the caller (which has the tiles) for the same reason the two maps above are: it is the
/// **size of the land**, and every plant upkeep figure on the row is quoted per tender-load of it
/// (`forage::patch_tender_loads`). A coord absent from the map presents [`NO_TENDER_LOAD`] worth of
/// ground, the same absent-means-nothing convention — never a substituted capacity.
#[allow(clippy::too_many_arguments)] // the registry, three configs, three lookup maps and a rate
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
    tile_capacities: &HashMap<UVec2, f32>,
    tile_quotes: &FloraQuoteCache,
    // **The live builders kit per queued source** — read off the bands' queues at capture rather
    // than off the patch, so a kit picked this turn shows in the recapture. See [`BuildKitIds`].
    build_kits: &BuildKitIds,
    // **The live keeping kit per worked source**, on the same rule one account over. See
    // [`UpkeepKitIds`].
    upkeep_kits: &UpkeepKitIds,
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
            // **THE SIZE OF THE LAND UNDER THIS PATCH** — the tile's own `K`, which every upkeep
            // figure on this row is quoted per tender-load of. Through
            // `forage::patch_land_capacity`, so a patch whose coord is **not on the map** publishes
            // the bill struck against its seeded capacity — the same reading `advance_cultivation`
            // bleeds against and `maintenance_shares` claims against, which is what keeps the row's
            // `demand − supplied == shortfall` a statement about one number.
            let tile_capacity = crate::forage::patch_land_capacity(
                patch,
                tile_capacities.get(&patch.tile).copied(),
            );
            // **The measure both rung quotes below are struck per** — one reading, so the price a
            // compose sheet shows and the bill the patch is handed cannot come from two places.
            let tender_loads = crate::forage::patch_tender_loads(tile_capacity, forage);
            let neglect_grace = patch_neglect_grace_remaining(patch, ladder);
            // The patch's own ecology — the seam `refresh_ecology_phase` classified the published
            // `ecology_phase` word with, so the bands and the word describe the same source.
            let ecology = patch_ecology(patch, forage);
            // **THE LADDER'S price for each plant rung, resolved once and used by both halves of the
            // pair.** The fraction and the work pair must divide by the same number or the wire
            // states one meter twice, from two denominators.
            //
            // **`RUNG_COST_UNSCALED` on the tended rung, THIS PATCH'S OWN PRICE on the Field.**
            // Clearing wild ground is clearing wild ground, so Cultivate is flat; a Sow is priced by
            // how much of the tile the crop still has to replace
            // (`forage::patch_field_cost_multiplier`, `docs/plan_standing_upkeep.md` §4.15). The
            // **published** figure has to be the scaled one, because this is the price the compose
            // sheet and the `⌃` mark quote a Sow at — and a quote that disagreed with the charge is
            // exactly the defect class §4.3's rule exists to catch. Before the leg starts it is the
            // live measure, which is the same number that leg will be stamped with (see
            // `forage::field_replaced_share`).
            let cultivation_work_cost = ladder
                .rung(RungKey::PlantTended)
                .build_cost(RUNG_COST_UNSCALED)
                .unwrap_or(NO_RUNG_WIDTH);
            let field_work_cost = ladder
                .rung(RungKey::PlantField)
                .build_cost(crate::forage::patch_field_cost_multiplier(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                    ladder,
                ))
                .unwrap_or(NO_RUNG_WIDTH);
            let forecast = forage_forecast(
                patch,
                tile_composition,
                forage,
                flora,
                // **The EQUIPPED reference rate, not any band's basket tier** — a patch row is a fact
                // about the *patch*, and a patch has no band to resolve a kit against. Exactly the
                // rule `HerdTelemetryState` already follows for the hunt's haul; a band's real,
                // kit-resolved gather rate rides its own `PopulationCohortState`
                // (`forageCarryPerWorkerBiomass`) and its `SourceYield` row.
                forage_per_worker_biomass(equipped_gather_rate, seasonal),
                FORECAST_OUTPUT_MULTIPLIER,
                // **The WHOLE basket** — a patch row is a fact about the *patch*, and a patch has no
                // crew to have named anything, exactly as it has no band to resolve a kit against. A
                // narrowed crew's own numbers ride its `SourceYield` row; what the client composes
                // per species off this one is `share × biomass`, which is why every entry's standing
                // biomass ships beside the composition.
                &crate::components::TakeSelection::EVERYTHING,
            );
            // **The published basket and every vector aligned with it, resolved together** — see
            // the fields below.
            let basket =
                patch_composition_info(patch, tile_composition, forage, flora, tile_quotes);
            ForagePatchState {
                x: patch.tile.x,
                y: patch.tile.y,
                // **The wire keeps the 0..1 fraction; the source keeps ONE position** — the
                // per-rung meter is that position read into the rung's own span **through the
                // patch's standing** (`forage::patch_rung_work_done`), divided by the rung's live
                // cost. So a patch that holds the tended rung reads exactly `1.0` beside an
                // `is_cultivated` that is already true, and a Field at 40% still reads its Cultivate
                // as complete — which is the rung-ordering bug made unrepresentable rather than
                // merely forbidden.
                //
                // ⛔ **"CLAMPED INTO THE RUNG'S OWN SPAN" IS WHAT THIS USED TO SAY, AND IT IS WHY A
                // FINISHED FIELD READ 99%.** The clamp is `position − base` against a completion
                // test of `position >= base + width`, and `fl(base + width) − base` is not `width`
                // whenever that addition rounds — so `isField` and `fieldProgress` were two readings
                // of one question and could contradict each other. The meter asks the standing now
                // (`intensification::rung_work_done`); the equality above is a construction rather
                // than a coincidence of the arithmetic.
                cultivation_progress: build_fraction(
                    crate::forage::patch_rung_work_done(patch, RungKey::PlantTended, ladder),
                    cultivation_work_cost,
                ),
                is_cultivated: patch.is_cultivated(),
                owner: patch.owner.map(|faction| faction.0),
                biomass: patch.biomass,
                // **WHAT THE PATCH HOLDS NOW — the rung is IN this number.** It is the tile's `K`
                // times the interpolated `field_capacity_gain` (`patch_carrying_capacity`, written
                // once per turn by `advance_forage_regrowth`), so a standing Field reads ~2.53× the
                // same ground wild. **The client must redact it under fog** and render
                // `tile_capacity` below instead — see that field.
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
                // pair. `field_yield` below comes off the same `rung_payoff` seam the labor arm pays a
                // Field with, so the client's "then Y" is the number the sim will hand over.
                field_progress: build_fraction(
                    crate::forage::patch_rung_work_done(patch, RungKey::PlantField, ladder),
                    field_work_cost,
                ),
                is_field: patch.is_field(),
                // **Through `rung_payoff` at rung 3** — the same seam the sim pays every plant rung
                // with, asked about the Field by name. It used to call a rung-3-only managed rate;
                // that model is retired, so the quote and the payout are one expression again.
                field_yield: crate::forage::rung_payoff(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                    RungKey::PlantField,
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
                cultivation_work_done: crate::forage::patch_rung_work_done(
                    patch,
                    RungKey::PlantTended,
                    ladder,
                ),
                cultivation_work_cost,
                field_work_done: crate::forage::patch_rung_work_done(
                    patch,
                    RungKey::PlantField,
                    ladder,
                ),
                field_work_cost,
                // **AND THE RATE THAT EATS IT** — the plant twin; the herd row has the reasoning.
                // `upkeep_demand` below resolves through the **at-risk** rung
                // (`forage::patch_unwinding_rung`) and is therefore `0` on a wild patch, which is
                // precisely the patch a compose sheet is quoting.
                //
                // # ⛔ THE QUOTE MOVES WITH THE BILL, or there are two producers of one verdict
                //
                // Both plant rungs declare `scaled_by: source_load` and quote their rate **per
                // tender-load**, so these are struck through **this patch's own tile capacity** —
                // the same measure `patch_upkeep_demand` bills against. Quoting the bare ladder rate
                // would price every patch in the game identically and promise `4.0` for a Field that
                // will actually be billed `4.31`.
                cultivation_upkeep_demand: ladder
                    .rung(RungKey::PlantTended)
                    .upkeep_demand(tender_loads),
                field_upkeep_demand: ladder.rung(RungKey::PlantField).upkeep_demand(tender_loads),
                // **The material twin of that pair** — the herd row's own rule, one web over, at this
                // patch's own tender-loads. Empty on both plant rungs today; the seam exists because
                // the route branch's stone is what lands in it next, and a per-web asymmetry here
                // would be a second model.
                cultivation_upkeep_material_demand: rung_material_rate(
                    ladder,
                    RungKey::PlantTended,
                    tender_loads,
                ),
                field_upkeep_material_demand: rung_material_rate(
                    ladder,
                    RungKey::PlantField,
                    tender_loads,
                ),
                // **WHAT THE GROUND WILL LOSE UNDER THE BUILDERS** — exactly what the next
                // decay pass will bleed off the at-risk meter, and the term a build's closed form
                // nets (`docs/plan_standing_upkeep.md` §4.6a). See `RungDef::meter_rot` for why the
                // forecast is exact rather than an estimate. It is emphatically not
                // the two demands above: the keeping pool owes those whatever a build crew does, so
                // netting a rate off a build would re-price the wrong thing.
                //
                // **DERIVED here rather than stamped by the labor arm**, unlike
                // `build_turns_remaining` beside it, and that is what keeps an *unworked* patch
                // honest: the labor arm visits only sources some band is assigned to, so a stamped
                // rot would read a tidy `0` on exactly the abandoned patches that are bleeding. Both
                // its inputs — `upkeep_supplied` and `neglect_turns` — are stored, so the number is
                // the same one the labor arm struck its countdown from.
                meter_rot_per_turn: crate::forage::patch_meter_rot(
                    patch,
                    ladder,
                    tile_capacity,
                    forage,
                ),
                // **The turns estimate the labor arm stamped this turn** — the running build's, or,
                // when nothing is being built, the **projection** for the rung this patch would climb
                // next, so the compose sheet can quote the job before the player commits. Read it
                // beside the `*WorkCost` for the assignment's own `improvement`, or for the next rung
                // up when that is empty. `-1` only where there is genuinely no answer (a Field, a
                // gate that refuses, or a stalled build).
                // **FOUR NEGATIVES, FOUR FACTS** (`intensification::BuildTurns`): `-1` where
                // there is genuinely no answer (nothing queued here, a gate refuses a waiting
                // entry, the top of the ladder); `-2` where the net supply is exactly zero, so the
                // meter holds where it is; `-3` where it is negative, so the meter is going
                // backwards; and `-4` where the band's **builders are staffed and standing on this
                // entry** and its own gate refuses it, so the whole queue is stuck behind it. The
                // last three are the ones the player can act on, and they are three answers because
                // holding wastes a turn, rotting destroys bought work, and a block is fixed by
                // staffing the KEEPING rather than by adding builders.
                build_destination_rung: published_destination_rung(patch.build_destination),
                // **THE RUNG THIS PATCH STANDS ON**, beside the one it is headed for. Through
                // `forage::patch_rung_key`, the single home of that test — never a second reading of
                // `is_cultivated()`/`is_field()` here. `snapshot.fbs`'s `currentRung` carries the why.
                current_rung: crate::forage::patch_rung_key(patch).wire_key(),
                // **THE MATERIAL HALF OF THE LADDER'S PRICE** — the herd twin's rule, one web over.
                // No plant rung on the shipped ladder declares a material, so all three are empty
                // today; the seam exists because the route branch's stone is the next thing to land
                // in it and a per-web asymmetry here would be a second model.
                build_material_cost: crate::forage::patch_rung_key(patch)
                    .above()
                    .map_or_else(Vec::new, |next| rung_material_pile(ladder, next)),
                upkeep_material_demand: material_payoffs(
                    &crate::forage::patch_material_keeping_basis(
                        patch,
                        ladder,
                        tile_capacity,
                        forage,
                    ),
                ),
                upkeep_material_supplied: material_payoffs(&patch.upkeep_materials_supplied),
                build_legs: published_build_legs(&patch.build_legs),
                // **WHERE THAT DESTINATION LEAVES THIS PATCH'S `K`** — `None` (the wire's sentinel)
                // when no band has queued it, which is a different statement from a capacity of
                // zero. Read through `forage::patch_destination_capacity`, i.e. through the **one**
                // expression `advance_forage_regrowth` writes the live `carrying_capacity` with,
                // evaluated at the destination standing. A `Cultivate` destination therefore quotes
                // the capacity the patch already has — only rung 3 raises `K` on this web.
                build_destination_capacity: crate::forage::patch_destination_capacity(
                    tile_capacity,
                    patch,
                    forage,
                ),
                // **What this patch's build is being raised with** — the RESOLVED kit of the winning
                // band's queue entry, read live so a pick shows in this frame rather than next turn.
                build_kit_id: build_kits.patch(patch.tile),
                upkeep_kit_id: upkeep_kits.patch(patch.tile).0,
                upkeep_kit_named: upkeep_kits.patch(patch.tile).1,
                // **WHAT THE GROUND HOLDS** — the tile's own `K` with no rung gain in it, the
                // fog-safe twin of `carrying_capacity` above and the denominator every upkeep figure
                // on this row is quoted per. **The reading already resolved once above**, never a
                // second lookup: two producers of one number are two numbers.
                tile_capacity,
                // The plant twin — see [`published_build_countdown`] and the herd row above.
                build_turns_remaining: published_build_countdown(
                    patch.build_turns_remaining,
                    patch.build_queue_position,
                    build_kits.patch_is_queued(patch.tile),
                ),
                // The plant twin — the hoes' delivery, or `NO_BUILD_GEAR` for a pool sent out bare
                // or carrying the animal web's hurdles.
                build_work_from_gear: patch.build_work_from_gear,
                // The plant twin — see the herd row.
                build_queue_position: patch.build_queue_position,
                // The plant twin — see the herd row.
                build_blocked_reason: patch.build_blocked_reason.key().to_string(),
                // The plant twin — see the herd row for why the estimate's terms ship beside the
                // sim's own answer, and for where the gear term sits in it.
                build_work_per_worker_turn: build_work_per_worker_turn(NO_BUILD_GEAR),
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
                //
                // **ALL FOUR TERMS ARE THE BILL, so the row is internally consistent**:
                // `demand − supplied == shortfall`, and `workersNeeded == ceil(demand /
                // PER_WORKER_OUTPUT)`, are what the client's under-kept readout is built on and it
                // is told to do no arithmetic of its own. The supply answers the demand the keepers
                // were *handed* (`forage::patch_keeping_basis`), not the one the turn's own build
                // work has since raised — the stamp is taken before the accrual, so a fourth term
                // reading the live demand published *"wants 3, you have 2"* beside a shortfall of
                // zero. The *live* cost of holding the rung a player is composing against is the
                // `<rung>UpkeepDemand` quote pair above, which is what that pair exists for.
                //
                // The bill is itself struck through the **at-risk** rung
                // (`forage::patch_unwinding_rung`), the same seam `advance_cultivation` bleeds and
                // the grace below counts down against, so a row cannot bill one rung's demand while
                // the sim bleeds another's.
                upkeep_demand: crate::forage::patch_keeping_basis(
                    patch,
                    ladder,
                    tile_capacity,
                    forage,
                ),
                upkeep_supplied: patch.upkeep_supplied,
                // **Derived, so the three always describe one turn and one rung.** A stored
                // shortfall would be stamped only on patches some band is assigned to, and would
                // therefore read `0` on exactly the abandoned patches that are reverting.
                upkeep_shortfall: crate::forage::patch_upkeep_shortfall(
                    patch,
                    ladder,
                    tile_capacity,
                    forage,
                ),
                // **The MAINTAIN activity's own `workers_needed`** — the plant twin, and what makes
                // a standing cost legible: *"this wants 1, you have 0"*. `ceil` of the **same
                // bill** the three terms above ship, never of the live demand beside it.
                upkeep_workers_needed: crate::forage::patch_upkeep_workers_needed(
                    patch,
                    ladder,
                    tile_capacity,
                    forage,
                ),
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
                field_fodder: crate::forage::rung_fodder_payoff(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                    RungKey::PlantField,
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
                composition: basket.composition,
                // **HOW MUCH OF EACH PLANT IS STANDING** — `share × biomass`, index-aligned with the
                // basket above **by construction**: both come out of one call, so no later edit can
                // leave the two describing different baskets. It is what a selective gather's crop
                // chip reads ("70% (63)"), and it rides the patch row rather than the memoized
                // composition entries because it moves every turn while they do not.
                composition_standing_biomass: basket.standing_biomass,
                // **AND WHAT EACH OF THEM CONVERTS AT** — the per-species twins of
                // `provisions_per_biomass` / `fodder_per_biomass` beside them, so a compose sheet
                // can price a **narrowing** before the player commits to it. The basket average
                // alone cannot: it does not move when a crop chip does.
                composition_provisions_per_biomass: basket.provisions_per_biomass,
                composition_fodder_per_biomass: basket.fodder_per_biomass,
                // …and the third account, which is the one the selective gather was argued on:
                // baskets are made of fibre, so *"tick cotton, see how much fibre"* is the first
                // thing a player tries and the basket-averaged rate beside it cannot answer it.
                composition_material_per_biomass: basket.material_per_biomass,
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
) -> PublishedBasket {
    // **Every per-entry vector is derived from the SAME list that is published**, so all of them are
    // index-aligned by construction rather than by call sites agreeing about which entries survive
    // the zero-share filter. Adding a fifth means adding it here, and nowhere else.
    // ⛔ **IT RETURNS THE FOUR VECTORS, NEVER A WHOLE `PublishedBasket`.** It used to answer the
    // basket, and the wild arm — the >99% case — took the four rows off it with a functional update
    // while overriding `composition` with the memo's `Arc`. Functional-update syntax evaluates the
    // base expression *in full*, so that arm built the deep copy of the whole basket (three `String`s
    // and two `Vec<MaterialPayoff>` per named plant) that the memo exists to avoid, and then dropped
    // it. Handing back the rows alone makes the copy unspellable rather than merely avoided.
    let aligned = |shares: &[FloraShareInfo]| -> AlignedRows {
        let rates = crate::forage::patch_species_rates(patch, tile_composition, flora, forage);
        // The rate rows come off `patch_composition` too, so they are the same basket in the same
        // order — but only the *published* entries survive the zero-share filter above, so each row
        // is matched **by key** rather than by position. A plant with no row reads `0`, which is what
        // an unnamed plant contributes to the basket average as well.
        let rate_of = |species: &str| {
            rates
                .iter()
                .find(|rate| rate.species == species)
                .map_or((NO_SPECIES_RATE, NO_SPECIES_RATE), |rate| {
                    (rate.provisions_per_biomass, rate.fodder_per_biomass)
                })
        };
        AlignedRows {
            standing_biomass: shares
                .iter()
                .map(|info| info.share * patch.biomass)
                .collect(),
            provisions_per_biomass: shares.iter().map(|info| rate_of(&info.species).0).collect(),
            fodder_per_biomass: shares.iter().map(|info| rate_of(&info.species).1).collect(),
            // **What each of them is MADE OF** — the same rows, in the wire's per-entry wrapper.
            // Empty `rows` is a plant that pays no material ("no row"), and a plant the roster no
            // longer names lands there too: it contributes to no account at all.
            material_per_biomass: shares
                .iter()
                .map(|info| SpeciesMaterialRates {
                    rows: rates
                        .iter()
                        .find(|rate| rate.species == info.species)
                        .and_then(|rate| rate.materials.as_ref())
                        .map(|rows| {
                            rows.iter()
                                .map(|payoff| MaterialPayoff {
                                    material_id: payoff.material.clone(),
                                    amount: payoff.amount,
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect(),
        }
    };
    let effective = patch_composition(patch, tile_composition, flora, forage);
    let quoted = tile_quotes.composition(patch.tile);
    let Cow::Owned(effective) = effective else {
        // wild: the tile's basket verbatim, shared rather than rebuilt.
        return aligned(&quoted).published(Arc::clone(&quoted));
    };
    let shares: Arc<[FloraShareInfo]> = effective
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
        .collect();
    aligned(&shares).published(shares)
}

/// **The patch's basket as the wire carries it** — the published entries and every per-entry vector
/// that must line up with them, resolved in one call so the alignment is structural. A named record
/// rather than a tuple because four vectors in a row is a shape a caller can mis-order.
struct PublishedBasket {
    composition: Arc<[FloraShareInfo]>,
    standing_biomass: Vec<f32>,
    provisions_per_biomass: Vec<f32>,
    fodder_per_biomass: Vec<f32>,
    material_per_biomass: Vec<SpeciesMaterialRates>,
}

/// **The per-entry vectors alone, before a basket is named to carry them** — what
/// [`patch_composition_info`]'s aligner answers, so a caller that already holds the published
/// entries (the wild patch, which shares the tile memo's `Arc`) can pair them up without a second
/// copy of the basket being built and thrown away.
struct AlignedRows {
    standing_biomass: Vec<f32>,
    provisions_per_biomass: Vec<f32>,
    fodder_per_biomass: Vec<f32>,
    material_per_biomass: Vec<SpeciesMaterialRates>,
}

impl AlignedRows {
    /// Pair the rows with the entries they were aligned against.
    fn published(self, composition: Arc<[FloraShareInfo]>) -> PublishedBasket {
        PublishedBasket {
            composition,
            standing_biomass: self.standing_biomass,
            provisions_per_biomass: self.provisions_per_biomass,
            fodder_per_biomass: self.fodder_per_biomass,
            material_per_biomass: self.material_per_biomass,
        }
    }
}

/// **What a plant the roster no longer names converts at** — `0`, in either scalar account, which is
/// exactly what it contributes to the basket average it must stay consistent with. Named because a
/// bare `0.0` at the call site reads as a measurement rather than as "there is no such plant".
const NO_SPECIES_RATE: f32 = 0.0;

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

/// **A PER-GOOD LEDGER AS THE WIRE CARRIES IT** — a `BTreeMap` in id order, dropped where it holds
/// nothing, because **empty means "no row" and never "zero of something"** (the `MaterialPayoff`
/// contract every such list on this wire follows).
fn material_payoffs(ledger: &BTreeMap<String, f32>) -> Vec<MaterialPayoff> {
    ledger
        .iter()
        .filter(|(_, amount)| **amount > 0.0)
        .map(|(material_id, amount)| MaterialPayoff {
            material_id: material_id.clone(),
            amount: *amount,
        })
        .collect()
}

/// **WHAT ONE RUNG COSTS TO HOLD IN GOODS, PER TURN, AT A SOURCE'S OWN SCALE** — the material twin
/// of the `*UpkeepDemand` pre-commit quote, and the number the `⌃` track's third aside prices the
/// rung it is **offering** with (`docs/plan_standing_upkeep.md` §2.7).
///
/// ⛔ **IT IS THE RUNG'S RATE, RESOLVED LIVE — NEVER THE STAMPED BILL.** The stamp
/// (`*::upkeep_materials_demanded`) answers *"what was this source billed"* through the rung it
/// stands **on**; this answers *"what would this rung cost"* for a rung it may not have reached, and
/// reading the stamp would publish an empty list for every rung a track ever offers. The two
/// disagree on a source mid-climb, which is correct and is exactly the relationship the work pair
/// already has. **A quote is carried nowhere**, so it has nothing the stamp exists to protect it
/// from.
///
/// `source_measure` is the source's own scale reading, so the quote and the bill read one
/// `scaled_by`: quoting the bare ladder rate would show one price and charge another.
fn rung_material_rate(
    ladder: &LadderConfig,
    rung: RungKey,
    source_measure: f32,
) -> Vec<MaterialPayoff> {
    let def = ladder.rung(rung);
    def.upkeep_materials()
        .filter_map(|(id, _)| {
            let amount = def.upkeep_material_demand(id, source_measure);
            (amount > crate::intensification::NO_UPKEEP_DEMAND).then(|| MaterialPayoff {
                material_id: id.to_string(),
                amount,
            })
        })
        .collect()
}

/// **THE WHOLE PILE ONE RUNG SWALLOWS TO RAISE**, per material — the ladder's own declaration, at no
/// per-source multiplier. A species' `taming_cost_multiplier` prices the *job*; there is no reading
/// under which it is five times the fence panels.
fn rung_material_pile(ladder: &LadderConfig, rung: RungKey) -> Vec<MaterialPayoff> {
    ladder
        .rung(rung)
        .build_materials()
        .map(|(material_id, amount)| MaterialPayoff {
            material_id: material_id.to_string(),
            amount,
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
