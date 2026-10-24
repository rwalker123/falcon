use sim_runtime::{MaterialPayoff, RungQuoteState, SpeciesMaterialRates};

use std::collections::BTreeMap;

use super::*;
use crate::fauna::{
    classify_ecology_phase, herd_capacity, herd_ecology, net_biomass_delta,
    reseeding_logistic_regrowth, NO_RETREAT_STAGE_STAY, ONE_UNIT_OF_BIOMASS,
};
use crate::forage::{
    forage_per_worker_biomass, patch_ecology, patch_fodder_per_biomass,
    patch_neglect_grace_remaining, patch_provisions_per_biomass,
};
use crate::intensification::{
    build_fraction, build_work_per_worker_turn, knowledge_title_from_id, SourceKind,
    NOT_IN_ANY_BUILD_QUEUE, NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY, NO_DEPOSIT_FLOOR,
    NO_RUNG_WIDTH, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED,
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
/// **`queued_live` is read off the bands' own queues** ([`QueuedBuildSources`]), not off the turn-written
/// row, for the reason the kit beside it is: the row's scratch lags a command by a whole turn, and
/// this state exists precisely in the frame before that turn.
pub(crate) fn published_build_countdown(
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
/// **The keeping line's forecast as the wire states it** — whole workers at completion, and the
/// game turn the next whole worker is needed ([`sim_runtime::NO_NEXT_KEEPING_WORKER`] where never
/// within the build). `current_turn` is the tick the frame is captured at, so a forecast
/// `turns_to_next_worker` turns out lands on `current_turn + turns`.
pub(crate) fn keeping_forecast_wire(
    forecast: crate::intensification::KeepingForecast,
    current_turn: u64,
) -> (u32, i32) {
    let next = forecast
        .turns_to_next_worker
        .and_then(|turns| i32::try_from(current_turn.saturating_add(u64::from(turns))).ok())
        .unwrap_or(sim_runtime::NO_NEXT_KEEPING_WORKER);
    (forecast.workers_at_completion, next)
}

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
fn published_destination_rung(destination: Option<crate::intensification::RungId>) -> String {
    destination.map_or_else(String::new, |rung| rung.wire_key())
}

/// **THE LEGS THE ENTRY STILL HAS TO LAY**, each with the chained date the publish pass struck.
///
/// **The client must not re-derive these.** The rung spans are the sim's config, the source's
/// position is the sim's state, and the dates are stamped against builds the client cannot
/// see — so a client reconstructing them would be a second producer of a verdict that has one.
pub(crate) fn published_build_legs(
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

/// **WHICH SOURCES SOME BAND HAS A BUILD DECLARED ON**, keyed the four ways a source is named —
/// the `queued_live` term of [`published_build_countdown`] and the wire's `isQueued` on a working.
///
/// # IT IS READ LIVE, NOT STAMPED BY THE TURN
///
/// `build_queue_position` beside it on the same row is scratch the labor pass writes, so it lags a
/// command by a whole turn; the server re-captures and broadcasts after **every** dispatched command
/// (`recapture_and_broadcast`), so an entry queued since the last turn has to be visible in that
/// frame. This walks the bands' live queues instead.
///
/// # ⛔ IT CARRIES NO KIT ANY MORE (`docs/plan_pool_toe.md` §4)
///
/// It was `BuildKitIds`, and each entry's value was the kit that build would be raised with — the
/// entry's own named choice, else the roster's answer for its web. **A builder's tools follow from
/// the rung the leg in flight stands on now**, settled band-wide by the player's own row priority
/// and published per pool as `PopulationCohortState.poolToe`, so there is no per-site kit left to
/// resolve and no source row carries one.
///
/// **The membership survives because it was never about the kit.** This is the one place the bands'
/// live queues are read, and two published states depend on it.
///
/// ⛔ **The claims arbitration went with the kit.** It picked the band whose entry sat where the
/// row's stamped position said, so that one source worked by several bands published one band's kit
/// beside that band's position. A set has no such question: *is this source in somebody's queue* has
/// one answer however many bands queued it.
#[derive(Default)]
pub(crate) struct QueuedBuildSources {
    patches: std::collections::HashSet<UVec2>,
    herds: std::collections::HashSet<String>,
    /// **The rung each queued herd entry climbs to**, keyed `(herd id, rung)` - what lets a rung
    /// quote say its step is already spoken for (`SiteRefusal::PenTaken`).
    herd_climbs: std::collections::HashSet<(String, crate::intensification::RungId)>,
    roads: std::collections::HashSet<UVec2>,
    /// **The workings some band has queued**, keyed `(tile, material)` because one tile can hold two
    /// and a `fell` queued on the timber is not a `quarry` queued on the rock beneath it.
    deposits: std::collections::HashSet<(UVec2, String)>,
    /// **The tile improvements some band has queued**, keyed `(tile, branch)` - the registry's own
    /// key, since one tile can carry an improvement of several branches.
    tiles: std::collections::HashSet<(UVec2, crate::intensification::BranchId)>,
    /// **WHAT THE VIEWER'S OWN BANDS PUT ON EACH DECLARED BUILD** - the builders staffing assigned and
    /// the tools they were issued, summed over the viewer's bands that declared the source.
    crews: std::collections::HashMap<crate::components::BuildSource, PublishedBuildCrew>,
}

/// **One declared build's crew as the wire states it** (`buildersStaffed` / `buildersKitted` /
/// `buildersCap` / `buildToe`), aggregated over the viewer's own bands.
///
/// Staffed and kitted add across bands (each band's builders are different people); the cap is the
/// largest any band was offered, because it is the rung's own limit cut by the tile's room and is
/// not additive. The tool lines merge per item, summing what was asked and what was issued.
#[derive(Default, Clone)]
pub(crate) struct PublishedBuildCrew {
    pub(crate) staffed: u32,
    pub(crate) kitted: u32,
    pub(crate) cap: u32,
    pub(crate) toe: Vec<sim_schema::state::PoolToeLineState>,
}

/// The pool token a build's tool lines are published under: the pool table's line shape is shared,
/// and a build is not one of the standing pools.
const BUILD_TOE_POOL: &str = "build";

impl PublishedBuildCrew {
    /// **One band's crew on one declared build**, as that band's allocation settled it.
    pub(crate) fn of_allocation(
        allocation: &crate::components::LaborAllocation,
        source: &crate::components::BuildSource,
    ) -> Self {
        let mut published = Self::default();
        published.add_band(allocation, source);
        published
    }

    /// Fold one band's crew and tool lines on `source` into this reading: staffed and kitted add,
    /// the cap is the largest any band was offered, tool lines merge per item.
    fn add_band(
        &mut self,
        allocation: &crate::components::LaborAllocation,
        source: &crate::components::BuildSource,
    ) {
        if let Some(crew) = allocation.build_crew(source) {
            self.staffed += crew.total();
            self.kitted += crew.kitted;
            self.cap = self.cap.max(crew.cap);
        }
        for line in allocation
            .last_build_toe
            .iter()
            .filter(|line| &line.source == source)
        {
            match self.toe.iter_mut().find(|held| held.item_id == line.item) {
                Some(held) => {
                    held.required += line.required;
                    held.filled += line.filled;
                }
                None => self.toe.push(sim_schema::state::PoolToeLineState {
                    pool: BUILD_TOE_POOL.to_string(),
                    item_id: line.item.clone(),
                    required: line.required,
                    filled: line.filled,
                }),
            }
        }
    }
}

impl QueuedBuildSources {
    /// **The viewer's crew on this declared build**, or the empty reading when none of the viewer's
    /// bands declared it.
    pub(crate) fn crew_of(&self, source: &crate::components::BuildSource) -> PublishedBuildCrew {
        self.crews.get(source).cloned().unwrap_or_default()
    }

    /// **IS THIS TILE IMPROVEMENT IN SOME BAND'S LIVE QUEUE?** - the tile kind's twin of
    /// [`Self::road_is_queued`], and the gate on the builder's scratch a rival may not read.
    pub(crate) fn tile_is_queued(
        &self,
        tile: UVec2,
        branch: crate::intensification::BranchId,
    ) -> bool {
        self.tiles.contains(&(tile, branch))
    }

    /// **Does some band have a build declared on this road tile?** — the route twin of
    /// [`Self::patch_is_queued`], and the `queued_live` term of [`published_build_countdown`].
    pub(crate) fn road_is_queued(&self, tile: UVec2) -> bool {
        self.roads.contains(&tile)
    }

    /// **IS THIS PATCH IN SOME BAND'S LIVE QUEUE?** — membership of an index built by walking the
    /// bands' `builds`, rather than by reading the turn-written row.
    fn patch_is_queued(&self, tile: UVec2) -> bool {
        self.patches.contains(&tile)
    }

    /// The animal twin — see [`Self::patch_is_queued`].
    fn herd_is_queued(&self, id: &str) -> bool {
        self.herds.contains(id)
    }

    /// **IS THIS HERD'S CLIMB TO `rung` IN SOME BAND'S LIVE QUEUE?**
    fn herd_climb_is_queued(&self, id: &str, rung: crate::intensification::RungId) -> bool {
        self.herd_climbs.contains(&(id.to_string(), rung))
    }

    /// **IS THIS WORKING IN SOME BAND'S LIVE QUEUE?** — the deposit twin of
    /// [`Self::patch_is_queued`], and the source of the working row's own `isQueued`.
    pub(crate) fn deposit_is_queued(&self, tile: UVec2, material: &str) -> bool {
        self.deposits.contains(&(tile, material.to_string()))
    }
}

/// **The one place a band's live queue becomes a published membership** — see
/// [`QueuedBuildSources`].
///
/// ⛔ **`allocations` MUST BE THE VIEWER'S OWN BANDS.** The index keys purely by tile, herd id and
/// working, so it cannot tell whose queue an entry came from; the caller filters
/// (`snapshot/capture.rs`), and the resulting membership is what both source tables gate a rival's
/// stamped build scratch on. See `factions.md` → "The improvement follows the ground; the BUILDER'S
/// state follows the builder".
pub(crate) fn resolve_queued_build_sources<'a>(
    allocations: impl Iterator<Item = &'a crate::components::LaborAllocation>,
) -> QueuedBuildSources {
    let mut resolved = QueuedBuildSources::default();
    for allocation in allocations {
        for entry in &allocation.builds {
            let source = &entry.source;
            match source {
                crate::components::BuildSource::Patch(tile) => {
                    resolved.patches.insert(*tile);
                }
                crate::components::BuildSource::Herd(id) => {
                    resolved.herds.insert(id.clone());
                    if let Some(rung) = entry.declared.climb() {
                        resolved.herd_climbs.insert((id.clone(), rung));
                    }
                }
                crate::components::BuildSource::Road(tile) => {
                    resolved.roads.insert(*tile);
                }
                crate::components::BuildSource::Deposit { tile, material } => {
                    resolved.deposits.insert((*tile, material.clone()));
                }
                crate::components::BuildSource::Tile { tile, branch } => {
                    resolved.tiles.insert((*tile, *branch));
                }
                // A band improvement publishes on its own band's row, which only its owner ever
                // receives, so there is no shared source table to gate a rival's scratch on.
                crate::components::BuildSource::Band { .. } => {}
            }
            resolved
                .crews
                .entry(source.clone())
                .or_default()
                .add_band(allocation, source);
        }
    }
    resolved
}

/// **WHICH SOURCES THE VIEWER'S OWN BANDS WORK**, keyed the two ways a worked source is named — the
/// second of the two memberships a source row's build scratch is gated on.
///
/// # IT IS READ LIVE, NOT STAMPED BY THE TURN
///
/// [`QueuedBuildSources`]'s rule, for its reason: the server re-captures and broadcasts after
/// **every** dispatched command, so a row staffed this frame has to be visible in it where a
/// turn-written field would lag a whole turn. This walks the bands' live assignment rows.
///
/// # ⛔ IT CARRIES NO KIT ANY MORE (`docs/plan_pool_toe.md` §4)
///
/// It was `UpkeepKitIds`, and each entry's value was the kit that site's keepers carried —
/// `EquipmentConfig::keeping_kit_for` over the player's per-site `upkeep_kit` pick. **A site's
/// keeping tools follow from its own rung now**, so there is nothing per site left to name and no
/// source row carries a keeping kit. The *"a stated override
/// beats a derivation, and the first stated one wins"* arbitration went with them: it existed only
/// to decide which of several bands' picks a shared site should publish.
#[derive(Default)]
pub(crate) struct WorkedSources {
    patches: std::collections::HashSet<UVec2>,
    herds: std::collections::HashSet<String>,
}

impl WorkedSources {
    /// **DOES ONE OF THE VIEWER'S OWN BANDS WORK THIS PATCH?** — the twin of
    /// [`QueuedBuildSources::patch_is_queued`], and the second of the two ways the labor arm reaches
    /// a source: `publish_build_chain` stamps its estimate onto the band's **queued** entries and
    /// onto everything the band **works**, so those two memberships together are exactly *"this
    /// scratch could be ours"*.
    fn patch_is_worked(&self, tile: UVec2) -> bool {
        self.patches.contains(&tile)
    }

    /// The animal twin — see [`Self::patch_is_worked`].
    fn herd_is_worked(&self, id: &str) -> bool {
        self.herds.contains(id)
    }
}

/// **The one place a band's live rows become a published membership** — see [`WorkedSources`].
///
/// ⛔ **`allocations` MUST BE THE VIEWER'S OWN BANDS**, on [`resolve_queued_build_sources`]' rule and
/// for its reason.
///
/// **A working is deliberately not indexed.** The two memberships gate a *build scratch*, and the
/// deposit row gates its own on the queue alone (`snapshot::deposits`) — so a worked-deposit set
/// would have no reader, and a set nothing asks about is a set free to be wrong.
pub(crate) fn resolve_worked_sources<'a>(
    allocations: impl Iterator<Item = &'a crate::components::LaborAllocation>,
) -> WorkedSources {
    let mut resolved = WorkedSources::default();
    for allocation in allocations {
        for assignment in &allocation.assignments {
            match &assignment.target {
                crate::components::LaborTarget::Forage { tile, .. } => {
                    resolved.patches.insert(*tile);
                }
                crate::components::LaborTarget::Hunt { fauna_id, .. } => {
                    resolved.herds.insert(fauna_id.clone());
                }
                // A band-wide role stands on no ground, and a working's scratch is gated on the
                // queue alone.
                _ => continue,
            }
        }
    }
    resolved
}

/// **The viewer's own settling score, and nobody else's** — a bare number with nothing on the map to
/// look at, so it has no visible tier (`factions.md` → "Which frame sections are viewer-scoped").
pub(crate) fn snapshot_sedentarization(
    score: &SedentarizationScore,
    viewer: FactionId,
) -> Vec<SchemaSedentarizationState> {
    score
        .iter_sorted()
        .into_iter()
        .filter(|(faction, _)| *faction == viewer)
        .map(|(faction, entry)| SchemaSedentarizationState {
            faction: faction.0,
            score: entry.score,
            stage: entry.stage.as_str().to_string(),
            belief_points: entry.belief_points,
            domestication_points: entry.domestication_points,
            surplus_points: entry.surplus_points,
            land_points: entry.land_points,
            population_points: entry.population_points,
        })
        .collect()
}

/// **The viewer's own mobility score, and nobody else's** - `snapshot_sedentarization`'s rule: a bare
/// number with nothing on the map to look at, so it has no visible tier (`factions.md` -> "Which
/// frame sections are viewer-scoped").
pub(crate) fn snapshot_mobility(
    score: &crate::mobility::MobilityScore,
    viewer: FactionId,
) -> Vec<SchemaMobilityState> {
    score
        .iter_sorted()
        .into_iter()
        .filter(|(faction, _)| *faction == viewer)
        .map(|(faction, entry)| SchemaMobilityState {
            faction: faction.0,
            score: entry.score,
            skills_points: entry.skills_points,
            distance_points: entry.distance_points,
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
///
/// Shared with [`crate::snapshot::deposits`], which samples a **third** growth model on the same
/// x-axis: one spacing for every curve the client interpolates, or the chart would need to know
/// which source it is drawing before it could place a sample.
pub(crate) fn regrowth_sample_fraction(index: usize) -> f32 {
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
/// `ceiling*` fields are retired from the wire).
///
/// **The list is FOG-FILTERED for the viewer faction** — see [`HerdSnapshotInputs::herd_is_visible`].
pub(crate) struct HerdSnapshotInputs<'a> {
    /// **The tick this frame is captured at** — the base the keeping forecast's next-worker turn is
    /// dated from ([`keeping_forecast_wire`]).
    pub(crate) current_turn: u64,
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
    /// **Every faction's camps** — the tiles its resident bands stand on, which a pastoral herd's `K`
    /// is struck over (it drifts to its owner's nearest camp). The destination quote of a herd being
    /// tamed reads the camps of the faction building it: the herd's owner, else the viewer (the
    /// builder state on a herd row is the viewer's own).
    pub(crate) owner_camps: &'a std::collections::BTreeMap<FactionId, Vec<UVec2>>,
    /// The same ledger `visibility_raster_from_ledger` renders the client's fog from, read for the
    /// same faction — so a herd can never be drawn on a tile the raster paints black.
    pub(crate) visibility: &'a crate::visibility::VisibilityLedger,
    pub(crate) viewer: FactionId,
    /// **Does the viewer's faction know `herd_lore`?** (#799) — the roaming skill that shows a
    /// migratory herd's next seasonal ground. Resolved once per capture off the discovery ledger.
    pub(crate) herd_lore_known: bool,
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
    /// read, resolved off the bands' queues at capture. See [`QueuedBuildSources`].
    pub(crate) build_kits: &'a QueuedBuildSources,
    /// **The live keeping kit per worked source** — the animal half of the same map the patch rows
    /// read, resolved off the bands' rows at capture. See [`WorkedSources`].
    pub(crate) upkeep_kits: &'a WorkedSources,
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
        current_turn,
        telemetry,
        registry,
        fauna,
        ladder,
        equipped_haul_rate,
        grid_size,
        wrap_horizontal,
        graze,
        owner_camps,
        parties,
        penned_parties,
        fallback_party,
        build_kits,
        upkeep_kits,
        viewer,
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
            // ⛔ **WHOSE BUILD THIS IS FOLLOWS THE BUILDER, NOT THE GROUND** — the plant twin's rule,
            // one web over; see `snapshot_forage_patches`. A rival's herd rides this list wherever the
            // viewer can *see* it, and seeing an animal says nothing about the kit its keepers carry,
            // how many turns their Tame has left or where it sits in their queue. Both indices are
            // built from the viewer's own allocations, so their membership is exactly *"one of our
            // bands stamped this"*; the clone is skipped on a herd carrying no build, which is nearly
            // all of them.
            let build_state_is_the_viewers =
                build_kits.herd_is_queued(&entry.id) || upkeep_kits.herd_is_worked(&entry.id);
            let withheld_build;
            let herd = match herd {
                Some(live) if !build_state_is_the_viewers && live.has_build_estimate() => {
                    withheld_build = live.without_build_estimate();
                    Some(&withheld_build)
                }
                other => other,
            };
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
            // **THE NEXT SEASONAL GROUND (#799, herd lore)** — a migratory herd's next route anchor
            // ([`crate::roaming_skills::next_ground`]; the same anchor while it loiters and while it
            // walks the leg). Gated on the herd and on the viewer's knowledge: the row only exists
            // when the viewer can see the herd (the fog gate above), and the ground is published
            // only to a people who know herd lore. **The ground tile is deliberately NOT fog-checked**
            // — seeing past the fog to where a herd is going is what the lesson is. `-1` covers
            // "you do not know the lore" and "it does not migrate" alike.
            let next_ground = herd
                .filter(|_| inputs.herd_lore_known)
                .and_then(crate::roaming_skills::next_ground);
            // The neglect countdown, off the live registry herd (the display `entry` carries no
            // counter). A herd the registry cannot resolve has nothing at risk to report.
            let neglect_grace =
                herd.and_then(|herd| crate::fauna::herd_neglect_grace_remaining(herd, ladder));
            // **The keeping line's forecast** — off the same (possibly withheld) herd, so a rival's
            // build pace never reaches the row.
            let herd_keeping = herd.map_or(
                (
                    NO_CREW_ON_THIS_ACTIVITY,
                    sim_runtime::NO_NEXT_KEEPING_WORKER,
                ),
                |herd| {
                    keeping_forecast_wire(
                        crate::intensification::keeping_forecast(
                            crate::fauna::herd_keeping_basis(herd, fauna, ladder),
                            herd.ladder_position(),
                            crate::intensification::BuildPace::in_flight(&herd.build_legs),
                            |position| {
                                let mut at = herd.clone();
                                at.upkeep_demanded = None;
                                at.set_ladder_position(position, ladder);
                                crate::fauna::herd_keeping_basis(&at, fauna, ladder)
                            },
                        ),
                        current_turn,
                    )
                },
            );
            // **The herd's own ecology — the rung's, not the wild block's.** `herd_ecology` picks
            // wild / pastoral / pen, and it is the seam `refresh_ecology_phase` classifies the
            // `ecology_phase` word with, so the bands below cannot describe a different source than
            // the word does. Resolved once and used for the cuts, the sampled curve **and** the word,
            // which is what makes that agreement structural rather than a convention.
            let ecology = herd.map(|herd| herd_ecology(herd, fauna));
            let herd_crew =
                build_kits.crew_of(&crate::components::BuildSource::Herd(entry.id.clone()));
            HerdTelemetryState {
                id: entry.id.clone(),
                label: entry.label.clone(),
                species: entry.species.clone(),
                // **`x`/`y` stay on the display entry, as a PAIR with the fog gate above - except for
                // the viewer's OWN herd.** The gate decided this row's visibility against
                // `entry.position`, so publishing a different tile beside it would describe a herd
                // whose presence was judged somewhere else. The one Population-stage writer of
                // `current_pos` is `Herd::corral_at`, which moves a herd onto the pen site it was
                // handed (the best-grazing tile in range, not the hex it stood on), so the turn a pen
                // goes up the entry is a turn behind. An owned herd passes the gate whatever tile it
                // stands on, so its row reads the live position and the marker follows the pen the
                // same turn; a rival's herd keeps the entry's tile, which is the one the gate judged.
                x: herd
                    .filter(|herd| herd.owner == Some(viewer))
                    .map_or(entry.position.x, |herd| herd.current_pos.x),
                y: herd
                    .filter(|herd| herd.owner == Some(viewer))
                    .map_or(entry.position.y, |herd| herd.current_pos.y),
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
                next_ground_x: next_ground.map(|pos| pos.x as i32).unwrap_or(-1),
                next_ground_y: next_ground.map(|pos| pos.y as i32).unwrap_or(-1),
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
                per_worker_yield: forecast.per_worker_yield.provisions,
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
                // **The standing split's player-facing half** (`docs/plan_pen_standing_yield.md`):
                // what this herd gives as milk, eggs and wool, and the order that is being paid
                // for. The target rides a **negative sentinel** when nothing is in flight, because
                // `0.0` is a real order — see `HerdTelemetryState::standing_output_target`.
                standing_output_fraction: herd
                    .map(|herd| herd.standing_output_fraction)
                    .unwrap_or(0.0),
                standing_output_target: herd
                    .and_then(|herd| herd.standing_output_target)
                    .unwrap_or(sim_schema::NO_OUTPUT_COMMITMENT_IN_FLIGHT),
                output_recommit_progress: herd
                    .map(|herd| herd.output_recommit_progress)
                    .unwrap_or(0.0),
                output_recommit_cost: herd.map(|herd| herd.output_recommit_cost).unwrap_or(0.0),
                // Husbandry ceiling (Grazing 2d-δ) — the client hides the corral/extend affordance on a
                // non-`pen` herd and the domestication track on a `wild` one.
                husbandry_ceiling: herd
                    .map(|herd| {
                        inputs
                            .ladder
                            .ceiling_wire_id(herd.husbandry_ceiling, SourceKind::Herd)
                            .to_string()
                    })
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
                // **built** too, where it means exactly the same thing: the site's own crew owes
                // the rate from the first work banked, so these are the hands that hold a
                // half-tamed herd as much as a finished one (`docs/plan_standing_upkeep.md` §4.6a).
                // It is **not** a minimum viable build crew — a build crew supplies nothing toward
                // the rate — and it read `0` mid-build on the older premise that an unfinished
                // meter owed no keeping. The take activity's answer rides
                // `SourceYield::workers_needed`.
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
                // **THE KEEPING LINE'S TWO FORECASTS** (`docs/plan_site_crews.md`) — the same bill,
                // projected along the in-flight leg's own pace to the rung's top.
                upkeep_workers_at_completion: herd_keeping.0,
                upkeep_next_worker_turn: herd_keeping.1,
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
                    .map_or(
                        ladder.kind_root(SourceKind::Herd).rung_id(),
                        crate::fauna::herd_rung_key,
                    )
                    .wire_key(),
                // **THE SIZE TIER THIS HERD'S FENCE HAS REACHED** (`ranch`), empty in none - derived
                // from the footprint through the one `fauna::herd_size_tier`.
                size_tier: herd
                    .and_then(|herd| crate::fauna::herd_size_tier(herd, ladder))
                    .map(|tier| tier.id.clone())
                    .unwrap_or_default(),
                // **WHERE THE QUEUED CORRAL WILL PEN THIS HERD** - the stamped `Herd::pen_site`, `-1`
                // on both axes when none is queued.
                // **Builder state, so the viewer's own queue only**: a rival's queued pen site is withheld
                // like every other builder field on the row (the authoritative field is untouched).
                pen_site_x: herd
                    .and_then(|herd| herd.pen_site)
                    .filter(|_| build_kits.herd_is_queued(&entry.id))
                    .map_or(NO_PEN_SITE, |site| site.x as i32),
                pen_site_y: herd
                    .and_then(|herd| herd.pen_site)
                    .filter(|_| build_kits.herd_is_queued(&entry.id))
                    .map_or(NO_PEN_SITE, |site| site.y as i32),
                // **Whose herd it is** — no more secret than the rung the row already publishes, and
                // fog-filtered with the row itself.
                owner: herd.and_then(|herd| herd.owner).map(|faction| faction.0),
                builder_band: herd
                    .and_then(|herd| herd.builder_band)
                    .map_or(0, |band| band.0),
                // **EVERY RUNG'S QUOTE** - one row per rung of the herd branch above the root, built
                // by iterating the ladder; see [`herd_rung_quotes`].
                rung_quotes: herd_rung_quotes(&HerdQuoteInputs {
                    herd,
                    species: &entry.species,
                    fauna,
                    ladder,
                    forecast: &forecast,
                    queued: inputs.build_kits,
                }),
                // **THE MATERIAL HALF OF THE LADDER'S PRICE** (`docs/plan_standing_upkeep.md` §2.7).
                // The pile is the rung the `⌃` track would offer next — `LadderConfig::above_id` the one the
                // herd stands on — resolved off the **ladder** at capture and published whether or
                // not a build is in flight, exactly as `*WorkCost` beside it is. Empty at the top of
                // the branch, which is the honest reading rather than a repeat of the pen's own —
                // **and the pen's own pile is published beside it as `corral_build_material_cost`**,
                // which is the field a ring card reads. Without that name here the emptiness reads
                // as "a ring is free", and the gap gets re-derived.
                build_material_cost: herd
                    .map(|herd| {
                        inputs
                            .ladder
                            .above_id(crate::fauna::herd_rung_key(herd))
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
                    let camps = owner_camps
                        .get(&herd.owner.unwrap_or(viewer))
                        .map_or(&[][..], Vec::as_slice);
                    crate::fauna::herd_destination_capacity(
                        herd,
                        species_def,
                        graze,
                        &prey_index,
                        fauna,
                        ladder,
                        width,
                        height,
                        wrap_horizontal,
                        camps,
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
                // **The builders the viewer's own bands put on this herd's declared build**, the
                // kitted share, the most the build could take and the tools they were issued.
                builders_staffed: herd_crew.staffed,
                builders_kitted: herd_crew.kitted,
                builders_cap: herd_crew.cap,
                build_toe: herd_crew.toe,
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
                // **WHAT THE HERD'S OWN CREW SPENT KEEPING IT THIS TURN** (`docs/plan_site_crews.md`
                // §2.2) — stamped by the labour pass beside `upkeep_supplied`, reported not re-derived.
                upkeep_hands: herd.map_or(crate::fauna::NO_HANDS, |herd| herd.upkeep_hands),
                // **Which keeping tools the VIEWER's crews were short of, by name**, and the flag
                // read off those same lines.
                upkeep_tools_short: herd
                    .is_some_and(|herd| viewer_keeping_tools_short(&herd.upkeep_toe, viewer)),
                upkeep_toe: herd
                    .map_or_else(Vec::new, |herd| upkeep_toe_lines(&herd.upkeep_toe, viewer)),
                // **Who ate the growth** — the same stamp `BuildGate::PredatorsAteGrowth` is judged
                // on. This row is already fog-gated above, so it adds no disclosure of its own.
                predator_eaten_by: herd
                    .and_then(|herd| herd.predator_eaten_by.clone())
                    .unwrap_or_default(),
                predator_eaten: herd.map_or(0.0, |herd| herd.predator_eaten_this_turn),
                // **Where this herd's meat keeps** (#798) — the class `huntMeals` is keyed by. Empty
                // only where the table cannot resolve the species.
                keeping: species_def
                    .map(|def| def.keeping.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// A rung's site rule as the catalog publishes it.
fn site_rule_state(
    site: &crate::intensification::RungSiteRequirement,
) -> sim_runtime::SiteRuleState {
    sim_runtime::SiteRuleState {
        requires_gathering_site: site.requires_gathering_site,
        min_forage_capacity: site.min_forage_capacity,
        requires_fresh_water: site.requires_fresh_water,
    }
}

/// **WHAT A PLANT ROW'S RUNG QUOTES ARE STRUCK FROM** - one patch (already redacted to wild ground
/// where the viewer may not read its improvement), the tile's basket and the three configs, plus the
/// two measures a rung's price and bill are quoted at.
struct PatchQuoteInputs<'a> {
    patch: &'a crate::forage::ForagePatch,
    tile_composition: &'a [crate::flora_config::FloraShare],
    forage: &'a ForageLaborConfig,
    flora: &'a FloraConfig,
    ladder: &'a LadderConfig,
    /// The tile's tender-loads: the measure every `source_load` rung quotes its rate per.
    tender_loads: f32,
    /// What a `replaced_share` rung is priced at on this patch.
    field_cost_multiplier: f32,
    /// Why each site-gated rung refuses this tile; absent = it takes the rung.
    site_refusals: &'a HashMap<(UVec2, RungId), SiteRefusal>,
}

/// **EVERY RUNG OF THE PLANT BRANCH, QUOTED ON THIS PATCH** - the one producer of what the retired
/// `cultivation*` / `tended*` / `field*` / `sowSiteRefusal` fields carried. It **iterates the
/// ladder's rungs above the root** and prices each by its own record and its own traits
/// ([`crate::forage::PlantPayoff::of_rung`]); nothing here names a rung, so a fourth plant step is a
/// JSON edit that appears on the wire with no code change.
///
/// Every figure is the same seam the sim pays or gates with, so a quote and its outcome cannot
/// disagree: the work cost through `forage::plant_rung_cost`, the meter through
/// `forage::patch_rung_work_done`, the yield and fodder through `forage::rung_payoff` /
/// `rung_fodder_payoff`, the refusal through the capture's `RungSiteRequirement::refusal` sweep.
fn patch_rung_quotes(inputs: &PatchQuoteInputs<'_>) -> Vec<RungQuoteState> {
    let PatchQuoteInputs {
        patch,
        tile_composition,
        forage,
        flora,
        ladder,
        tender_loads,
        field_cost_multiplier,
        site_refusals,
    } = *inputs;
    let held = patch.standing().held;
    ladder
        .rungs_of(ladder.branch_of_kind(SourceKind::Patch))
        .into_iter()
        .skip(BRANCH_ROOT_RUNGS)
        .map(|def| {
            let rung = def.rung_id();
            let work_done = crate::forage::patch_rung_work_done(patch, rung, ladder);
            let work_cost =
                crate::forage::plant_rung_cost(def, field_cost_multiplier).unwrap_or(NO_RUNG_WIDTH);
            let payoff = crate::forage::PlantPayoff::of_rung(ladder, rung);
            RungQuoteState {
                rung_key: rung.wire_key(),
                reached: ladder.at_or_above(held, rung),
                progress: build_fraction(work_done, work_cost),
                work_done,
                work_cost,
                build_material_cost: rung_material_pile(ladder, rung),
                upkeep_demand: def.upkeep_demand(tender_loads),
                upkeep_material_demand: rung_material_rate(ladder, rung, tender_loads),
                food_yield: crate::forage::rung_payoff(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                    payoff,
                ),
                fodder: crate::forage::rung_fodder_payoff(
                    patch,
                    tile_composition,
                    forage,
                    flora,
                    FORECAST_OUTPUT_MULTIPLIER,
                    payoff,
                ),
                // A plant's materials are priced per species (`FloraShareInfo`), not per rung.
                material_payoff: Vec::new(),
                site_refusal: site_refusals
                    .get(&(patch.tile, rung))
                    .map_or(SITE_ACCEPTED, |refusal| refusal.as_str())
                    .to_string(),
            }
        })
        .collect()
}

/// **WHAT A HERD ROW'S RUNG QUOTES ARE STRUCK FROM** - the live herd (`None` only for the
/// unreachable "in telemetry, gone from the registry" row, which quotes prices and no standing), the
/// species, the configs and the row's own yield forecast.
struct HerdQuoteInputs<'a> {
    herd: Option<&'a crate::fauna::Herd>,
    species: &'a str,
    fauna: &'a FaunaConfig,
    ladder: &'a LadderConfig,
    forecast: &'a crate::fauna::SourceYieldForecast,
    /// The viewer's own declared builds - what says a rung is already spoken for.
    queued: &'a QueuedBuildSources,
}

/// The wire's `-1` for "no Corral queued" on `HerdTelemetryState.penSiteX` / `penSiteY`.
const NO_PEN_SITE: i32 = -1;

/// **EVERY RUNG OF THE HERD BRANCH, QUOTED ON THIS HERD** - the animal twin of
/// [`patch_rung_quotes`], and the one producer of what the retired `domestication*` / `tame*` /
/// `corral*` / `pastoral*` fields carried. It iterates the ladder's rungs above the root and prices
/// each by its record and its traits ([`crate::fauna::HerdPayoff::of`]); nothing here names a rung.
///
/// The payoff block is the **forecast's own**: an `enclosed` rung pays `managed_yield` and a
/// `domesticated` one `pastoral_yield` (which the forecast already zeroes past the pastoral rung -
/// an offer that is no longer on the table), both converted into the species' material rows at the
/// same MSY biomass the food quote is, so the two accounts describe one harvest.
fn herd_rung_quotes(inputs: &HerdQuoteInputs<'_>) -> Vec<RungQuoteState> {
    let HerdQuoteInputs {
        herd,
        species,
        fauna,
        ladder,
        forecast,
        queued,
    } = *inputs;
    ladder
        .rungs_of(ladder.branch_of_kind(SourceKind::Herd))
        .into_iter()
        .skip(BRANCH_ROOT_RUNGS)
        .map(|def| {
            let rung = def.rung_id();
            // **Two loads, because a tier loosens the hands and not the fence**: the WORK term is
            // struck at the load a size tier divides (a Ranch minds more head per keeper); the
            // MATERIAL term at the raw load (a fence mends in proportion to the herd it holds).
            let work_load =
                herd.map(|herd| crate::fauna::herd_work_keeper_load(herd, fauna, ladder));
            let material_load = herd.map(|herd| crate::fauna::herd_keeper_load(herd, fauna));
            // `(food, the biomass the material rows are struck at)` for the block this rung pays.
            let (food_yield, payoff_biomass) = match crate::fauna::HerdPayoff::of(def.trait_set()) {
                crate::fauna::HerdPayoff::Pen => (
                    forecast.managed_yield.provisions,
                    forecast.managed_yield_biomass,
                ),
                crate::fauna::HerdPayoff::Pastoral => (
                    forecast.pastoral_yield.provisions,
                    forecast.pastoral_yield_biomass,
                ),
                crate::fauna::HerdPayoff::Wild => (NO_RUNG_PAYOFF, NO_RUNG_PAYOFF),
            };
            let work_done = herd.map_or(NO_RUNG_WIDTH, |herd| herd.rung_work_done(rung, ladder));
            RungQuoteState {
                rung_key: rung.wire_key(),
                reached: herd.is_some_and(|herd| ladder.at_or_above(herd.standing().held, rung)),
                // **The raw meter, not the standing's credit**: an `on_completion` rung (the pen)
                // credits nothing until its fence closes, which governs what a half-built pen is
                // *worth*, never what its progress bar reads.
                progress: herd.map_or(NO_RUNG_WIDTH, |herd| {
                    build_fraction(work_done, herd.rung_cost(rung, ladder))
                }),
                work_done,
                // The ladder's price on THIS species: a `species_taming` rung times the species'
                // own multiplier, every other rung the ladder's own.
                work_cost: def
                    .build_cost(match def.cost_scale {
                        crate::intensification::CostScale::SpeciesTaming => {
                            fauna.taming_cost_multiplier_for(species)
                        }
                        _ => RUNG_COST_UNSCALED,
                    })
                    .unwrap_or(NO_RUNG_WIDTH),
                build_material_cost: rung_material_pile(ladder, rung),
                upkeep_demand: work_load.map_or(NO_UPKEEP_DEMAND, |load| def.upkeep_demand(load)),
                upkeep_material_demand: material_load
                    .map_or_else(Vec::new, |load| rung_material_rate(ladder, rung, load)),
                food_yield,
                fodder: NO_ANIMAL_PAYS_FODDER,
                material_payoff: herd
                    .map(|herd| {
                        material_rates(
                            fauna.hunt_materials_for(&herd.species),
                            payoff_biomass,
                            FORECAST_OUTPUT_MULTIPLIER,
                        )
                    })
                    .unwrap_or_default(),
                // **A herd is penned in one place only**: while one of the viewer's bands has this
                // rung's climb queued on an `enclosed` rung, the quote says so.
                site_refusal: match herd {
                    Some(herd)
                        if def
                            .trait_set()
                            .has(crate::intensification::RungTrait::Enclosed)
                            && queued.herd_climb_is_queued(&herd.id, rung) =>
                    {
                        crate::intensification::SiteRefusal::PenTaken
                            .as_str()
                            .to_string()
                    }
                    _ => SITE_ACCEPTED.to_string(),
                },
            }
        })
        .collect()
}

/// How many leading rungs of a branch carry no quote: the root, which is what a source stands on
/// before anything is built and so has no price, bill or payoff of its own to state.
const BRANCH_ROOT_RUNGS: usize = 1;

/// The payoff a rung that pays no block (a rung carrying neither `domesticated` nor `enclosed`)
/// quotes: nothing.
const NO_RUNG_PAYOFF: f32 = 0.0;

/// Per-tile depletable-forage cultivation/ecology display state (Intensification Phase 1a) for the
/// client tile card, plus each patch's **pre-commit yield forecast** (`forage::forage_forecast` —
/// the same ceiling/conversion helpers `forage_take` pays with, so forecast == actual). One entry per
/// live `ForagePatch`, emitted in a stable `(y, x)` order so the snapshot is deterministic (the
/// `ForageRegistry` map iteration order is not). `owner` crosses as the tending faction's `u32`
/// (`None` for a wild/untended patch).
///
/// `rung_site_refusals` maps `(tile coord, rung)` → **why that rung refuses that ground**, resolved
/// by the caller (which has the tiles and the hydrology) through the one shared
/// `RungSiteRequirement::refusal` seam, for every plant rung that declares a `site_requirement`.
/// **Absent = the land takes the rung** — the same absent-means-nothing convention
/// `seasonal_weights` uses.
///
/// `seasonal_weights` maps tile coord → that tile's `FoodModuleTag::seasonal_weight`, folded into the
/// forecast's per-worker throughput exactly as the Forage labor arm folds it into `forage_take`. A
/// patch whose tile carries no food module forecasts at [`NO_FORAGE_SEASON`] — no per-worker gather at
/// all, which is exactly what such a tile offers. **That is a reachable state since slice 5**: `Sow`
/// places a Field on any ground the `plant:farm` rung's `site_requirement` accepts — module or not —
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
/// (`forage::patch_tender_loads`). A coord absent from the map presents
/// [`crate::forage::NO_TENDER_LOAD`] worth of ground, the same absent-means-nothing convention —
/// never a substituted capacity.
/// **The patch rows that are the same for every viewer, kept for the seats after the first.**
///
/// A patch row is *derived* — the ladder's two rung quotes, the material rates, the upkeep bill, the
/// regrowth samples — and at 80×52 there are ~2100 of them, which makes this section over half of a
/// capture. But **the viewer enters the row in exactly two places**: it decides which patch struct
/// the row is derived from (the real one, the wild-ground stand-in, or the one with its build
/// estimate withheld), and it decides whether either of the two membership indices names the tile.
/// Where neither can
/// apply for *any* viewer, the row is a pure function of ground and config, so the first seat's
/// derivation is every seat's answer.
///
/// See [`patch_row_is_viewer_invariant`] for the three conditions, and
/// `.claude/rules/core_sim/turn-profiling.md` → "One frame per seat" for what it buys.
///
/// **It is per capture, never per world.** A patch's biomass, meters and rung move every turn, so a
/// memo that outlived the pass would publish last turn's ground; `capture_snapshot` owns one on its
/// stack for the length of one sweep over the audiences.
pub struct WildRowMemo {
    rows: HashMap<UVec2, ForagePatchState>,
    /// **The two membership indices over EVERY faction's bands**, which is the third condition
    /// below. They live here rather than beside the per-viewer indices because they are *only* the
    /// sharing rule's input: nothing published reads them, and a single-audience capture — which has
    /// no sharing rule — must not pay to build them (0.29 ms at 80×52, measured; see
    /// `.claude/rules/core_sim/turn-profiling.md` → "One frame per seat").
    queued_by_anyone: QueuedBuildSources,
    worked_by_anyone: WorkedSources,
}

impl WildRowMemo {
    /// The memo for a capture with **more than one audience**. `queued_by_anyone` / `worked_by_anyone`
    /// are the unfiltered twins of the per-viewer indices — every faction's queues and every
    /// faction's worked rows.
    pub fn new(queued_by_anyone: QueuedBuildSources, worked_by_anyone: WorkedSources) -> Self {
        Self {
            rows: HashMap::new(),
            queued_by_anyone,
            worked_by_anyone,
        }
    }

    /// **This patch's row: the shared one if there is one, else `derive` — and kept if it is
    /// shareable.**
    ///
    /// `derive` is called at most once. A row that is not viewer-invariant never enters the map, so a
    /// later seat cannot be handed a row that was built for somebody else's entitlement.
    fn row_for(
        &mut self,
        patch: &ForagePatch,
        derive: impl FnOnce() -> ForagePatchState,
    ) -> ForagePatchState {
        if !patch_row_is_viewer_invariant(patch, &self.queued_by_anyone, &self.worked_by_anyone) {
            return derive();
        }
        if let Some(row) = self.rows.get(&patch.tile) {
            return row.clone();
        }
        let row = derive();
        self.rows.insert(patch.tile, row.clone());
        row
    }
}

/// **Is this patch's row the same whoever is looking?**
///
/// Three conditions, each ruling out one way the viewer reaches the row:
///
/// | condition | what it rules out |
/// |---|---|
/// | the patch has **no owner** | the wild-ground substitution — an unowned patch has no improvement to withhold, so `improvement_is_legible` is true for every viewer |
/// | it carries **no build estimate** | the withheld-build substitution, which only ever replaces a patch that has one |
/// | **nobody** has it queued or worked | the two membership indices, which are built from *the viewer's own* bands and reach the build countdown and the withheld build scratch |
/// | it carries **no keeping-tool lines** | `upkeepToe` / `upkeepToolsShort`, which publish each viewer its own people's lines only |
///
/// The third is asked of **every faction's** allocations rather than of the viewer's, and that is
/// the whole point of it: a tile a rival queued this turn is a row that differs between viewers
/// while nothing about the patch itself has moved yet — a declared build carries no estimate until a
/// turn resolves, so the second condition does not catch it.
fn patch_row_is_viewer_invariant(
    patch: &ForagePatch,
    queued_by_anyone: &QueuedBuildSources,
    worked_by_anyone: &WorkedSources,
) -> bool {
    patch.owner.is_none()
        && !patch.has_build_estimate()
        // A people's keeping-tool lines are published to that people alone (`upkeep_toe_lines`).
        && patch.upkeep_toe.is_empty()
        && !queued_by_anyone.patch_is_queued(patch.tile)
        && !worked_by_anyone.patch_is_worked(patch.tile)
}

// The registry, three configs, three lookup maps, a rate, the viewer and its fog — and the memo.
#[allow(clippy::too_many_arguments)]
pub(crate) fn snapshot_forage_patches(
    registry: &ForageRegistry,
    forage: &ForageLaborConfig,
    // The EQUIPPED reference gather rate — a patch row has no band to resolve a basket tier
    // against, exactly as a herd row has none for the sled's. See `HerdSnapshotInputs`.
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    ladder: &LadderConfig,
    seasonal_weights: &HashMap<UVec2, f32>,
    rung_site_refusals: &HashMap<(UVec2, RungId), SiteRefusal>,
    tile_capacities: &HashMap<UVec2, f32>,
    tile_quotes: &FloraQuoteCache,
    // **The live builders kit per queued source** — read off the bands' queues at capture rather
    // than off the patch, so a kit picked this turn shows in the recapture. See [`QueuedBuildSources`].
    build_kits: &QueuedBuildSources,
    // **The live keeping kit per worked source**, on the same rule one account over. See
    // [`WorkedSources`].
    upkeep_kits: &WorkedSources,
    // **The tick this frame is captured at** — the keeping forecast's next-worker turn is dated
    // from it ([`keeping_forecast_wire`]).
    current_turn: u64,
    // ⛔ **WHO IS LOOKING, AND WHAT THEY HAVE SEEN.** A patch row is a fact about a TILE, and tiles
    // are published whole — the client fogs the map from `visibility_raster`. What is *not* a fact
    // about the tile is the improvement standing on it: who tends it and how far along their
    // Cultivate and Sow meters are. That is a fact about a people, and before this every client
    // received it for every patch on the map — a live readout of exactly which ground a rival was
    // farming. See [`ForagePatchState`]'s ownership fields and `factions.md` → "Which frame
    // sections are viewer-scoped".
    viewer: FactionId,
    visibility: &crate::visibility::VisibilityLedger,
    fog_enabled: bool,
    // **`None` for a single-audience capture, which is every single-player turn and every test.**
    // With one seat there is nobody to share a row with, so the memo — and the two unfiltered
    // indices it carries — would be bought for nothing, and *unchanged single-player pacing* is the
    // property this arc is measured against.
    mut shared_rows: Option<&mut WildRowMemo>,
) -> Vec<ForagePatchState> {
    // **One patch's row, derived.** A closure rather than the `.map` body it used to be so the memo
    // below can wrap it *without* the single-audience path paying for the wrapper: with no memo the
    // call is the whole of the map body, exactly as before.
    let derive_row = |patch: &ForagePatch| -> ForagePatchState {
        let patch_keeping = |patch: &ForagePatch, tile_capacity: f32| {
            keeping_forecast_wire(
                crate::intensification::keeping_forecast(
                    crate::forage::patch_keeping_basis(patch, ladder, tile_capacity, forage),
                    patch.ladder_position(),
                    crate::intensification::BuildPace::in_flight(&patch.build_legs),
                    |position| {
                        let mut at = patch.clone();
                        at.upkeep_demanded = None;
                        at.set_ladder_position(position, ladder);
                        crate::forage::patch_keeping_basis(&at, ladder, tile_capacity, forage)
                    },
                ),
                current_turn,
            )
        };
        // ⛔ **IS THE IMPROVEMENT ON THIS TILE THE VIEWER'S TO READ?**
        //
        // Yours always is, and an untended patch has no improvement to hide. A *rival's* is
        // legible exactly where the viewer has **explored** the ground — `is_discovered`, not
        // `is_visible`, on `route_states`' precedent: a field is built into the ground and does
        // not wander off, so having seen it once remains true, the same argument a road makes.
        // (A herd is the opposite case and uses `Active`.) The staleness that buys — a field
        // that has since gone feral still reading as a field — is the same staleness a
        // remembered road carries, and is accepted for the same reason.
        let improvement_is_legible = patch.owner.is_none_or(|owner| owner == viewer)
            || !fog_enabled
            || visibility.is_discovered(viewer, patch.tile.x, patch.tile.y);
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
        // bleeds against and `site_keeping_claims` claims against, which is what keeps the row's
        // `demand − supplied == shortfall` a statement about one number.
        let tile_capacity =
            crate::forage::patch_land_capacity(patch, tile_capacities.get(&patch.tile).copied());
        // ⛔ **THE REDACTION IS OF THE SOURCE, AND EVERY FIELD BELOW READS THE REDACTED ONE.**
        //
        // Five fields used to be gated by name — `owner`, the two flags and the two meters —
        // while `carrying_capacity`, the two rung yields, `provisions_per_biomass` and the
        // basket were all derived from the same improvement and published bare. So
        // `carrying_capacity != tile_capacity` was an exact test for *"this rival tile carries a
        // standing improvement"*, sitting on the very row that denied one, and a client
        // declining to draw it is presentation rather than a boundary.
        //
        // **A row the viewer may not read the improvement on is built from WILD GROUND**
        // ([`crate::forage::ForagePatch::as_wild_ground`]) — not from zeros, which would be a lie
        // about terrain the whole map already publishes. Every improvement-derived term takes its
        // unimproved value because the *source* it is derived from has none, so the row is
        // internally consistent by construction and a field added below is redacted without
        // anybody remembering to add it to a list.
        //
        // `tile_capacity` above is resolved from the real patch deliberately: it is the land's
        // own `K` and carries no rung gain, and its off-map fallback
        // (`patch_land_capacity`) is a capacity `advance_forage_regrowth` provably never
        // multiplied. The wild twin then takes it as its `carrying_capacity`.
        let wild_ground;
        let patch = if improvement_is_legible {
            patch
        } else {
            wild_ground = patch.as_wild_ground(
                tile_capacity,
                forage,
                crate::intensification::RungStanding::unstarted_on(
                    ladder,
                    ladder.branch_of_kind(SourceKind::Patch),
                ),
            );
            &wild_ground
        };
        // ⛔ **AND WHOSE BUILD THIS IS FOLLOWS THE BUILDER, NOT THE GROUND.** The improvement
        // standing on a tile is legible where the viewer has explored, because a field does not
        // wander off. *Who* is raising it, with what kit, how many turns from done and where it
        // sits in their queue is the builder's internal state — the same category as the larder
        // and the bench a foreign band's row already withholds — so it is the viewer's own bands
        // or nothing. Both indices are built from the viewer's allocations alone
        // (`snapshot/capture.rs`), and their two memberships are the two ways the labor arm
        // reaches a source at all, so this is exactly *"one of our bands stamped this"*.
        //
        // The clone is skipped on a source carrying no build, which is nearly all of them.
        let withheld_build;
        let patch = if build_kits.patch_is_queued(patch.tile)
            || upkeep_kits.patch_is_worked(patch.tile)
            || !patch.has_build_estimate()
        {
            patch
        } else {
            withheld_build = patch.without_build_estimate();
            &withheld_build
        };
        // ⛔ **THE FIVE RATE BLOCKS THAT BELONG TO THE GROUND, OFF THE MEMO** — resolved once
        // here, `None` on a patch carrying a commitment or banked work, which then derives every
        // one of them live. See `WildGroundQuotes` for what each is and
        // `forage::patch_is_wild_ground` for why bare ground makes them a property of the *tile*:
        // with no favored crop the basket seams answer the tile's own mix at every rung, so
        // nothing about where the patch stands can move them.
        //
        // **Resolved AFTER the two redactions above, deliberately.** A row whose improvement the
        // viewer may not read is built from wild ground, so it takes the memo's route — which is
        // the same reading, and is what keeps the redaction from being detectable by which route
        // a number came down. See `as_wild_ground`.
        let wild = crate::forage::patch_is_wild_ground(patch)
            .then(|| tile_quotes.wild_ground(patch.tile))
            .flatten();
        // **The measure both rung quotes below are struck per** — one reading, so the price a
        // compose sheet shows and the bill the patch is handed cannot come from two places.
        let tender_loads = crate::forage::patch_tender_loads(tile_capacity, forage);
        let neglect_grace = patch_neglect_grace_remaining(patch, ladder);
        // The patch's own ecology — the seam `refresh_ecology_phase` classified the published
        // `ecology_phase` word with, so the bands and the word describe the same source.
        let ecology = patch_ecology(patch, forage);
        // **THE PRICE OF A REPLACED-SHARE RUNG ON THIS PATCH, resolved once** and handed to every
        // rung quote below, so a rung's progress fraction and its work cost divide by the same
        // number. A rung priced `replaced_share` (the shipped Sow) is priced by how much of the
        // tile the crop still has to replace (`forage::patch_field_cost_multiplier`,
        // `docs/plan_standing_upkeep.md` §4.15); every other rung is the ladder's own price. The
        // **published** figure has to be the scaled one, because this is the price the compose
        // sheet and the `⌃` mark quote at, and a quote that disagreed with the charge is exactly
        // the defect class §4.3's rule exists to catch.
        let field_cost_multiplier = wild.map_or_else(
            || {
                crate::forage::patch_field_cost_multiplier(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                    ladder,
                )
            },
            |wild| wild.field_cost_multiplier,
        );
        // **THE CONVERSION RATE, RESOLVED ONCE FOR THE ROW AND THE FORECAST BOTH.** It is
        // published as `provisionsPerBiomass` *and* is the rate every ceiling the forecast
        // composes is struck at, so deriving it in both places evaluated one basket average
        // twice per patch per turn — see `turn-profiling.md` and
        // `forage::forage_forecast_at_rate`.
        let provisions_per_biomass = wild.map_or_else(
            || patch_provisions_per_biomass(patch, tile_composition, flora, forage),
            |wild| wild.provisions_per_biomass,
        );
        let forecast = forage_forecast_at_rate(
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
            provisions_per_biomass,
        );
        // **The published basket and every vector aligned with it, resolved together** — see
        // the fields below.
        let basket =
            patch_composition_info(patch, tile_composition, forage, flora, tile_quotes, wild);
        // **ONE decomposition, two rates struck off it.** `material_per_biomass` and
        // `per_worker_material` differ only in the biomass scalar they are quoted per, so the
        // per-species decomposition beneath them is the same value twice. It was computed twice
        // — and it is not cheap: it allocates a row per named plant per material, each carrying a
        // deep copy of that plant's characteristic vector. See `turn-profiling.md`.
        //
        // **Borrowed off the memo on bare ground**, which is nearly every patch: the rows are a
        // property of the tile there, so the row's two rates are struck off the memo's own slice
        // and no decomposition happens on the turn at all.
        let material_yields: Cow<[crate::materials_config::MaterialYieldDef]> = wild.map_or_else(
            || {
                Cow::Owned(crate::forage::patch_material_yields(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                ))
            },
            |wild| Cow::Borrowed(&wild.material_yields[..]),
        );
        let patch_crew = build_kits.crew_of(&crate::components::BuildSource::Patch(patch.tile));
        ForagePatchState {
            x: patch.tile.x,
            y: patch.tile.y,
            owner: patch.owner.map(|faction| faction.0),
            builder_band: patch.builder_band.map_or(0, |band| band.0),
            biomass: patch.biomass,
            // **WHAT THE PATCH HOLDS NOW — the rung is IN this number.** It is the tile's `K`
            // times the interpolated `field_capacity_gain` (`patch_carrying_capacity`, written
            // once per turn by `advance_forage_regrowth`), so a standing Field reads ~2.53× the
            // same ground wild.
            //
            // ⛔ **WHICH IS WHY IT IS THE SOURCE THAT IS REDACTED, NOT THE CLIENT'S RENDERING.**
            // This used to say *"the client must redact it under fog and render `tile_capacity`
            // instead"* — but a client declining to draw a number is presentation, and the pair
            // `carrying_capacity != tile_capacity` was itself the disclosure. A row whose
            // improvement the viewer may not read is built from a wild source, so this **is**
            // `tile_capacity` there, by construction.
            carrying_capacity: patch.carrying_capacity,
            ecology_phase: patch.ecology_phase.as_str().to_string(),
            // The plant web's forecast is food-only for now — its fodder component is
            // `forage::PLANT_FODDER_FORECAST_NOT_YET_PROJECTED` (a known gap, #426), so these
            // project the provisions component rather than shipping a false `0` fodder line.
            per_worker_yield: forecast.per_worker_yield.provisions,
            // **THE PER-BIOMASS YIELD VECTOR** — what one unit of this patch's standing crop
            // is worth, in every account (`docs/plan_harvest_floor.md` §5), at the patch's own
            // basket-averaged rates: the same `patch_*_per_biomass` seams `forage_take` pays
            // with, so a tended patch reads its committed conversion and not the wild one.
            //
            // It replaces the four stance ceiling rows because a player drags a **continuous**
            // floor: with `biomass` and `carrying_capacity` the client evaluates
            // `max(0, B − floor·K) × rate` anywhere on the dial. **No dip term** — since §3.1
            // the build fraction multiplies the crew's throughput, never the ceiling.
            provisions_per_biomass,
            fodder_per_biomass: wild.map_or_else(
                || patch_fodder_per_biomass(patch, tile_composition, flora, forage),
                |wild| wild.fodder_per_biomass,
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
                &material_yields,
                ONE_UNIT_OF_BIOMASS,
                FORECAST_OUTPUT_MULTIPLIER,
            ),
            // The gatherer's own throughput, with the tile's **seasonal weight** folded in
            // exactly as `per_worker_yield` folds it — so this is honestly EMPTY in a dead
            // season, and a client must not divide by it.
            per_worker_material: material_rates(
                &material_yields,
                forage_per_worker_biomass(equipped_gather_rate, seasonal),
                FORECAST_OUTPUT_MULTIPLIER,
            ),
            // **EVERY RUNG'S QUOTE** - one row per rung of the plant branch above the root, built by
            // iterating the ladder; see [`patch_rung_quotes`].
            rung_quotes: patch_rung_quotes(&PatchQuoteInputs {
                patch,
                tile_composition,
                forage,
                flora,
                ladder,
                tender_loads,
                field_cost_multiplier,
                site_refusals: rung_site_refusals,
            }),
            // **WHAT THE GROUND WILL LOSE UNDER THE BUILDERS** — exactly what the next
            // decay pass will bleed off the at-risk meter, and the term a build's closed form
            // nets (`docs/plan_standing_upkeep.md` §4.6a). See `RungDef::meter_rot` for why the
            // forecast is exact rather than an estimate. It is emphatically not
            // the two demands above: the site's own crew owes those whatever a build crew does, so
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
            build_material_cost: ladder
                .above_id(crate::forage::patch_rung_key(patch))
                .map_or_else(Vec::new, |next| rung_material_pile(ladder, next)),
            upkeep_material_demand: material_payoffs(&crate::forage::patch_material_keeping_basis(
                patch,
                ladder,
                tile_capacity,
                forage,
            )),
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
                ladder,
            ),
            // **WHAT THE PATCH'S OWN CREW SPENT KEEPING IT THIS TURN** (`docs/plan_site_crews.md`
            // §2.1) — stamped by the labour pass beside `upkeep_supplied`, reported not re-derived.
            upkeep_hands: patch.upkeep_hands,
            // **Which keeping tools the VIEWER's crews were short of, by name**, and the flag read
            // off those same lines.
            upkeep_tools_short: viewer_keeping_tools_short(&patch.upkeep_toe, viewer),
            upkeep_toe: upkeep_toe_lines(&patch.upkeep_toe, viewer),
            // **WHAT THE GROUND HOLDS** — the tile's own `K` with no rung gain in it, the
            // ungained twin of `carrying_capacity` above and the denominator every upkeep figure
            // on this row is quoted per. **The reading already resolved once above**, never a
            // second lookup: two producers of one number are two numbers. On a row whose
            // improvement is redacted the two are deliberately **equal**, because the source
            // itself reads as wild ground — see `carrying_capacity`.
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
            // The plant twin - see the herd row.
            builders_staffed: patch_crew.staffed,
            builders_kitted: patch_crew.kitted,
            builders_cap: patch_crew.cap,
            build_toe: patch_crew.toe,
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
            upkeep_demand: crate::forage::patch_keeping_basis(patch, ladder, tile_capacity, forage),
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
            // **THE KEEPING LINE'S TWO FORECASTS** (`docs/plan_site_crews.md`) — the same bill,
            // projected along the in-flight leg's own pace to the rung's top.
            upkeep_workers_at_completion: patch_keeping(patch, tile_capacity).0,
            upkeep_next_worker_turn: patch_keeping(patch, tile_capacity).1,
            // **The neglect countdown**, resolved through the *same* `patch_unwinding_rung` seam
            // `advance_cultivation` bleeds through — so the wire counts down against the rung
            // that will actually revert, not one the patch merely stands on. `None` = a wild
            // patch, which is most of them.
            has_neglect_grace: neglect_grace.is_some(),
            neglect_grace_remaining: neglect_grace.unwrap_or(NO_NEGLECT_REMAINING),
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
    };
    let mut patches: Vec<ForagePatchState> = registry
        .patches
        .values()
        // **The memo is asked, and it is the only thing that knows the sharing rule.** `None` is a
        // single-audience capture and reduces to `derive_row` alone.
        .map(|patch| match shared_rows.as_deref_mut() {
            None => derive_row(patch),
            Some(memo) => memo.row_for(patch, || derive_row(patch)),
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
///
/// `wild` is the caller's already-resolved bare-ground memo entry (`None` on a patch carrying a
/// commitment or banked work) — threaded in rather than re-resolved here so the whole row answers
/// *"is this bare ground"* exactly once. On bare ground the per-species rate rows come off it, which
/// is what keeps a wild patch's basket free of any per-turn decomposition at all.
fn patch_composition_info(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    tile_quotes: &FloraQuoteCache,
    wild: Option<&WildGroundQuotes>,
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
        let rates: Cow<[crate::forage::SpeciesRate]> = wild.map_or_else(
            || {
                Cow::Owned(crate::forage::patch_species_rates(
                    patch,
                    tile_composition,
                    flora,
                    forage,
                ))
            },
            |wild| Cow::Borrowed(&wild.species_rates[..]),
        );
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

/// **WHAT THERE IS TO LEARN** — the ladder's knowledge roster, once per world and carrying no
/// faction. Every field is derived from `intensification_ladder.json`
/// ([`LadderConfig::knowledge_roster`]); nothing here is separately authored, which is the whole
/// point: a knowledge added to the ladder reaches a client's knowledge screen with no client edit.
///
/// ⛔ **IT IS FACTION-INDEPENDENT ON PURPOSE.** A faction that has learned nothing has no ledger row
/// at all, so a roster carried on the per-faction row below would leave a new player's screen empty
/// — with nothing on it to say there was anything to learn, which is the exact regression the
/// knowledge screen exists to have fixed.
pub(crate) fn snapshot_ladder_knowledge(ladder: &LadderConfig) -> Vec<LadderKnowledgeState> {
    ladder
        .knowledge_roster()
        .into_iter()
        .map(|entry| LadderKnowledgeState {
            knowledge_id: entry.knowledge.to_string(),
            display_name: knowledge_title_from_id(entry.knowledge),
            branch: entry.branch.to_string(),
            area: entry.area.to_string(),
            order: entry.order,
            is_step: entry.is_step,
        })
        .collect()
}

/// **THE SUBJECT AREAS, IN DISPLAY ORDER** — `intensification_ladder.json`'s `areas`, once per
/// world and beside the roster above.
///
/// ⛔ **PUBLISHED RATHER THAN INFERRED FROM THE ROSTER.** Areas are peers, so first-seen order read
/// off the rows would reshuffle the whole knowledge screen whenever a rung was added — the same
/// defect that made column order unstable before the roster carried it. An area with no domains is
/// never drawn, which is why the list may name areas no branch sits under yet: the order has to be
/// right already on the day a branch lands in one.
pub(crate) fn snapshot_ladder_areas(ladder: &LadderConfig) -> Vec<String> {
    ladder.areas.clone()
}

/// **A RUNG NOBODY BUILDS COSTS NOTHING TO REACH** — the `work_cost` a rung with no `build` block
/// publishes. On the shipped ladder that is `route:path` and nothing else: the branch's floor is
/// where a road already stands, so there is no pile to raise.
const NO_BUILD_WORK: f32 = 0.0;
/// **The builder cap a rung with nothing to build publishes** - `0`, the wire's "no build" reading.
const NO_BUILDERS_ALLOWED: u32 = 0;

/// **A RUNG THAT EATS NOTHING SWALLOWS NO PILE** — the `build_material_cost` a rung declaring no
/// `build.materials` publishes, and it rides with an empty `build_material_id`: the pair is one
/// reading, so *"no amount"* and *"no noun"* are the same answer said twice. Every route rung but
/// `route:paved_road` sits here.
const NO_BUILD_MATERIAL: f32 = 0.0;

/// **A RUNG WITH NO ROAD PAYOFF HELPS NO HAUL** - the `friction_multiplier` a rung without a
/// `route_payoff` publishes: the wire's neutral 1.0 (no help), so a defaulted row never claims a
/// non-road rung makes a road cheaper.
const NO_ROUTE_FRICTION_HELP: f32 = 1.0;

/// **A RUNG WITH NO ROAD PAYOFF HOLDS NO LINK OPEN** - its `holds_link_to_tiles`.
const NO_LINK_HELD_OPEN: u32 = 0;

/// **A RUNG WITH NO EXTRACTION PAYOFF TAKES NOTHING** - the neutral `yield_per_worker_turn` and
/// `recovery_fraction` a non-deposit rung publishes.
const NO_EXTRACTION_TAKE: f32 = 0.0;

/// **WHAT ANY SOURCE MAY BECOME** - the ladder's rung catalog, once per world and carrying no tile.
/// The source rows publish the rung a source **stands on**; this publishes every branch's whole
/// climb, so a client can price a rung nothing has built yet, on a ground where nothing has been.
///
/// ⛔ **EVERY FIELD IS DERIVED FROM `intensification_ladder.json`, EXACTLY AS THE KNOWLEDGE ROSTER
/// ABOVE IS.** Nothing here is separately authored and no value is restated: the cost and the bill
/// are the rung's own `build` / `upkeep` blocks, a road's payoff is its `route_payoff`, a deposit's
/// is its `extraction_payoff`, the site rule is its `site_requirement`, and the chain is its
/// `requires_rung`. A rung or a whole branch added to that config appears with no further edit,
/// which is the whole reason this rides the wire instead of a client-side table.
///
/// **Branches in the config's own order, rungs in climb order within each.** The payoff fields of a block
/// the rung does not declare read the wire's neutral value, so a plant rung never claims to be a
/// road that helps a haul.
///
/// **The two rates are the rung's own, before the source's multipliers.** A road's real price also
/// carries its keeper's remoteness quote and its ground's `infrastructure_cost`, a working's bill
/// its keeper-loads - per-source facts published on those rows. A catalog row is the branch's
/// figure, the only one that is the same for every source on it.
///
/// **`build_work_per_worker_turn` is the one field that is the SIM'S and not the config rung's**:
/// identical for every rung and every source, so it rides the catalog, and a client left to
/// transcribe [`crate::intensification::PER_WORKER_OUTPUT`] would not learn of a second term landing
/// in it.
pub(crate) fn snapshot_ladder_rungs(ladder: &LadderConfig) -> Vec<LadderRungState> {
    ladder
        .branches_in_config_order()
        .into_iter()
        .filter_map(|branch| ladder.branch_def(branch).map(|def| (branch, def)))
        .flat_map(|(branch, def)| {
            ladder.rungs_of(branch).into_iter().map(move |rung| {
                // **THE PILE AND ITS NOUN, FROM ONE LOOKUP**, so the amount and the material it is
                // counted in can never disagree. `None` for a rung that eats nothing. The first
                // entry and not a sum: the wire carries a single float, so a second declared
                // material would make the *amount* meaningless before the id had to choose.
                let pile = rung.build_materials().next();
                let route = rung.route_payoff.as_ref();
                let extraction = rung.extraction_payoff.as_ref();
                LadderRungState {
                    rung_key: rung.wire_key(),
                    // One vector carries every ladder, so each row says which it is on.
                    branch: rung.branch.as_str().to_string(),
                    order: rung.order,
                    // The same spelling the knowledge roster resolves its titles with - a second
                    // capitalization rule would be a second answer to one question.
                    display_name: knowledge_title_from_id(&rung.id),
                    label: rung.label.clone(),
                    label_fallback: rung.label_fallback.clone(),
                    verb: rung.verb.clone().unwrap_or_default(),
                    source_kind: def.source.as_str().to_string(),
                    traits: rung
                        .trait_set()
                        .members()
                        .map(|trait_| trait_.as_str().to_string())
                        .collect(),
                    unlock_knowledge: rung.unlock_knowledge.clone().unwrap_or_default(),
                    // **The remedy a gate asks for**: the rung that TEACHES a knowledge and the
                    // rung directly beneath the one it gates are two different facts.
                    earns_knowledge: rung.earns_knowledge.clone().unwrap_or_default(),
                    requires_rung: rung.requires_rung_wire_key().unwrap_or_default(),
                    // Unscaled: any per-source quote (a road's remoteness) is the source's.
                    work_cost: rung.build_cost(RUNG_COST_UNSCALED).unwrap_or(NO_BUILD_WORK),
                    // How many hands can build this rung at once; `0` on a rung with nothing to build.
                    max_builders: rung
                        .build
                        .as_ref()
                        .map_or(NO_BUILDERS_ALLOWED, |build| build.max_builders),
                    upkeep_work_per_turn: rung
                        .upkeep
                        .as_ref()
                        .map_or(NO_UPKEEP_DEMAND, |upkeep| upkeep.work_per_turn),
                    // ⛔ **THE PILE IS FLAT WHERE `work_cost` IS NOT**, and the noun rides with it
                    // because the amount alone cannot be a sentence - and a client must not supply
                    // the noun, which is a fact about the config.
                    build_material_cost: pile.map_or(NO_BUILD_MATERIAL, |(_, amount)| amount),
                    build_material_id: pile.map_or_else(String::new, |(id, _)| id.to_string()),
                    // **The bare rate, read and not restated** - through the same
                    // `build_work_per_worker_turn` seam every source row publishes by.
                    build_work_per_worker_turn: build_work_per_worker_turn(NO_BUILD_GEAR),
                    friction_multiplier: route
                        .map_or(NO_ROUTE_FRICTION_HELP, |payoff| payoff.friction_multiplier),
                    holds_link_to_tiles: route
                        .map_or(NO_LINK_HELD_OPEN, |payoff| payoff.holds_link_to_tiles),
                    grants_sight: route.is_some() && crate::routes::rung_grants_sight(rung),
                    yield_per_worker_turn: extraction
                        .map_or(NO_EXTRACTION_TAKE, |payoff| payoff.yield_per_worker_turn),
                    // ⛔ **PUBLISHED, NEVER DERIVED FROM `reachable / capacity`**, which falls as
                    // the rock is worked on a rung whose reach never moved.
                    recovery_fraction: extraction
                        .map_or(NO_EXTRACTION_TAKE, |payoff| payoff.recovery_fraction),
                    regrowth_multiplier: extraction
                        .map_or(sim_schema::DEPOSIT_RUNG_REGROWTH_UNCHANGED, |payoff| {
                            payoff.regrowth_multiplier
                        }),
                    // **Why a rung is refused, and not merely that it is** - the rung's own floor,
                    // struck against the capacity a working already publishes. A rung with no
                    // `site_requirement` reads [`NO_DEPOSIT_FLOOR`].
                    requires_held_rung_below: rung.requires_held_rung_below,
                    requires_crop: rung.requires_crop,
                    requires_work_room: rung.requires_work_room,
                    requires_ownership: rung.requires_ownership,
                    places_source: rung.places_source,
                    site_rule: rung
                        .site_requirement
                        .as_ref()
                        .map(site_rule_state)
                        .unwrap_or_default(),
                    size_tiers: rung
                        .size_tiers
                        .iter()
                        .map(|tier| sim_runtime::SizeTierState {
                            id: tier.id.clone(),
                            label: tier.label.clone(),
                            label_fallback: tier.label_fallback.clone(),
                            min_rings: tier.min_rings,
                            keeper_ratio_gain: tier.keeper_ratio_gain,
                            earns_knowledge: tier.earns_knowledge.clone().unwrap_or_default(),
                        })
                        .collect(),
                    knowledge_gains: rung
                        .knowledge_gains
                        .iter()
                        .map(|gain| sim_runtime::KnowledgeGainState {
                            knowledge: gain.knowledge.clone(),
                            regrowth_gain: gain.regrowth_gain,
                        })
                        .collect(),
                    site_relaxations: rung
                        .site_relaxations
                        .iter()
                        .map(|relaxation| sim_runtime::SiteRelaxationState {
                            knowledge: relaxation.knowledge.clone(),
                            rule: site_rule_state(&relaxation.site_requirement),
                        })
                        .collect(),
                    min_deposit_capacity: rung
                        .site_requirement
                        .as_ref()
                        .map_or(NO_DEPOSIT_FLOOR, |site| site.min_deposit_capacity),
                }
            })
        })
        .collect()
}

/// **THE LADDER'S BRANCHES** - the grouping [`snapshot_ladder_rungs`] is drawn under, once per
/// world, in the config's branch order. Derived from the config's branch descriptors alone.
pub(crate) fn snapshot_ladder_branches(ladder: &LadderConfig) -> Vec<LadderBranchState> {
    ladder
        .branches_in_config_order()
        .into_iter()
        .filter_map(|branch| ladder.branch_def(branch).map(|def| (branch, def)))
        .map(|(branch, def)| LadderBranchState {
            branch: branch.as_str().to_string(),
            area: def.area.clone(),
            source_kind: def.source.as_str().to_string(),
            label: def.label.clone(),
        })
        .collect()
}

/// Per-faction intensification-ladder knowledge for the client's learning/known meters — **one row
/// per faction carrying a `0..1` for every knowledge the ladder teaches**, in the roster's own
/// order. Iterates the ledger's factions in sorted order; a faction is emitted only when it has
/// begun learning **something** (all zero → skipped), mirroring how `discovery_progress_entries`
/// skips empty progress.
///
/// ⛔ **THE LIST IS SPARSE IN VALUE AND NEVER IN MEMBERSHIP.** A knowledge the faction has not begun
/// rides at `0` rather than being absent, so no reader has to tell *not begun* from *not published*
/// — and no reader needs the roster to know a row is missing.
///
/// **The five named float fields it replaced are retired.** Adding a knowledge used to mean adding a
/// schema field, which is why the route branch's two lessons had nowhere to appear at all.
/// **Only the viewer's own progress.** What another people has learned is not legible from outside —
/// there is no thing on the map that is "their knowledge" to be seen — so this has no visible tier
/// (`factions.md` → "Which frame sections are viewer-scoped").
pub(crate) fn snapshot_intensification_knowledge(
    ledger: &DiscoveryProgressLedger,
    ladder: &LadderConfig,
    viewer: FactionId,
) -> Vec<IntensificationKnowledgeState> {
    let roster = ladder.knowledge_roster();
    let mut factions: Vec<u32> = ledger
        .progress
        .keys()
        .filter(|faction| **faction == viewer)
        .map(|faction| faction.0)
        .collect();
    factions.sort_unstable();
    factions.dedup();
    factions
        .into_iter()
        .filter_map(|faction_id| {
            let faction = FactionId(faction_id);
            let knowledges: Vec<LadderKnowledgeProgress> = roster
                .iter()
                .map(|entry| LadderKnowledgeProgress {
                    knowledge_id: entry.knowledge.to_string(),
                    progress: ledger.get_progress(faction, entry.discovery_id).to_f32(),
                })
                .collect();
            // A faction with nothing on ANY meter is not published at all — the row would say only
            // "this faction exists", and the roster above already says what there is to learn.
            if knowledges.iter().all(|row| row.progress <= 0.0) {
                return None;
            }
            Some(IntensificationKnowledgeState {
                faction: faction_id,
                knowledges,
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
    rung: RungId,
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
fn rung_material_pile(ladder: &LadderConfig, rung: RungId) -> Vec<MaterialPayoff> {
    ladder
        .rung(rung)
        .build_materials()
        .map(|(material_id, amount)| MaterialPayoff {
            material_id: material_id.to_string(),
            amount,
        })
        .collect()
}

/// **`upkeepToolsShort`, read off the viewer's own [`upkeep_toe_lines`]** — exactly *"some line is
/// short"*, so the flag and the lines cannot disagree and neither speaks for a rival's crew.
pub(crate) fn viewer_keeping_tools_short(
    lines: &[crate::components::KeepingToolLine],
    viewer: FactionId,
) -> bool {
    lines
        .iter()
        .any(|line| line.faction == viewer && line.is_short())
}

/// ⛔ **A SOURCE'S KEEPING-TOOL LINES, ON THE WIRE — THE VIEWER'S OWN ONLY** — `upkeepToe`, item
/// ids only: the client resolves each to its roster display name. A rival's lines would publish its
/// tool stock, and the mark is a statement about the viewer's own crew, so they are not sent.
pub(crate) fn upkeep_toe_lines(
    lines: &[crate::components::KeepingToolLine],
    viewer: FactionId,
) -> Vec<sim_schema::KitToeLineState> {
    lines
        .iter()
        .filter(|line| line.faction == viewer)
        .map(|line| sim_schema::KitToeLineState {
            item_id: line.item.clone(),
            required: line.required,
            filled: line.filled,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fauna_config::{FaunaConfig, SizeClass};
    use crate::labor_config::LaborConfig;
    use crate::ladder_ids::shipped;

    /// The index of the sample taken **at the food peak** — `K/2`, the middle of an odd-length
    /// evenly-spaced sweep. Named because "the peak of this curve IS the food peak" is the reason the
    /// panel needs no separate field for it, and a silent re-spacing of the samples would move it.
    const FOOD_PEAK_SAMPLE: usize = REGROWTH_CURVE_SAMPLES / 2;

    /// A carrying capacity large enough that every sampled stock is well clear of float noise, and
    /// arbitrary otherwise — the properties asserted here are scale-free.
    const PROBE_CAPACITY: f32 = 1_000.0;

    fn probe_patch() -> ForagePatch {
        let mut patch = ForagePatch::new(
            UVec2::new(0, 0),
            PROBE_CAPACITY,
            shipped::root_standing(shipped::plant()),
        );
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
            shipped::root_standing(shipped::animal()),
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
