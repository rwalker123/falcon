use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use bevy::{
    ecs::system::{RunSystemOnce, SystemParam},
    prelude::*,
};
use sim_runtime::{
    encode_delta_flatbuffer, encode_snapshot_flatbuffer, AxisBiasState, CampaignProfileState,
    CharacteristicBandState, ClimateBandsState, CohortStoreState, CommandEventState,
    ConnectionState, CorruptionLedger, CorruptionSubsystem, CraftKnowledgeState, CrisisGaugeState,
    CrisisMetricKind as SchemaCrisisMetricKind, CrisisOverlayState,
    CrisisSeverityBand as SchemaCrisisSeverityBand, CrisisTelemetryState,
    CrisisTrendSample as SchemaCrisisTrendSample, CultureLayerState, CultureTensionState,
    CultureTraitEntry, DiscoveredSiteState as SchemaDiscoveredSiteState,
    DiscoveredSitesState as SchemaDiscoveredSitesState, DiscoveryProgressEntry,
    ElevationOverlayState, FactionInventoryEntryState as SchemaFactionInventoryEntryState,
    FactionInventoryState as SchemaFactionInventoryState, FloatRasterState, FloraShareInfo,
    FoodModuleState, ForagePatchState, ForkChoiceState, GenerationState, GlossEntryState,
    GreatDiscoveryDefinitionState, GreatDiscoveryProgressState, GreatDiscoveryState,
    GreatDiscoveryTelemetryState, HerdTelemetryState, InfluentialIndividualState,
    IntensificationKnowledgeState, KitOptionState, KnowledgeLedgerEntryState,
    KnowledgeMetricsState, KnowledgeTimelineEventState, LaborAssignmentState, MaterialDefState,
    MountainKind, PendingForkState, PendingForksState, PendingMigrationState,
    PopulationCohortState, PopulationDemographicsState as SchemaPopulationDemographicsState,
    PowerIncidentSeverity, PowerIncidentState, PowerNodeState, PowerTelemetryState, RecipeDefState,
    ScalarRasterState, SedentarizationState as SchemaSedentarizationState, SentimentAxisTelemetry,
    SentimentDriverCategory, SentimentDriverState, SentimentTelemetryState,
    SettlementStageViewState, SnapshotHeader, StanceAxisState, StanceState, StartMarkerState,
    TerrainOverlayState, TerrainSample, TileState, VictoryModeSnapshotState, VictoryResultState,
    VictorySnapshotState, VoiceLineState, VoiceMediumState, WorldDelta, WorldSnapshot,
    GRAZE_PHASE_COLLAPSING, GRAZE_PHASE_NONE, GRAZE_PHASE_STRESSED, GRAZE_PHASE_THRIVING,
};

use crate::{
    components::{
        available_workers, fragments_to_contract, BandEquipment, BandId, BandTravel, Expedition,
        ExpeditionMission, LaborAllocation, LaborAssignment, LaborTarget, PendingMigration,
        PopulationCohort, PowerNode, SourceYield, Tile, FODDER, FOOD, NO_RAID_FLOOR,
    },
    culture::{
        CultureLayer, CultureLayerScope as SimCultureLayerScope, CultureManager, CultureOwner,
        CultureTensionKind as SimCultureTensionKind, CultureTensionRecord,
        CultureTraitAxis as SimCultureTraitAxis,
    },
    demographics_config::{DemographicsConfig, DemographicsConfigHandle},
    fauna::{
        herd_herders_needed, hunt_forecast, pen_upkeep, would_be_herders_needed, EcologyPhase,
        Herd, HerdRegistry, HerdTelemetry, FODDERING_DISCOVERY_ID, FULLY_HERDED,
        HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID, PEN_FULLY_FED,
    },
    fauna_config::FaunaConfig,
    flora_config::{FloraConfig, FloraConfigHandle, FloraShare},
    food::FoodModuleTag,
    forage::{
        forage_forecast, patch_composition, rung_site_refusal, tile_is_fresh_watered, ForagePatch,
        ForageRegistry, CULTIVATION_DISCOVERY_ID, NO_FORAGE_SEASON, SEED_SELECTION_DISCOVERY_ID,
    },
    generations::{GenerationProfile, GenerationRegistry},
    graze::{GrazePatch, GrazeRegistry},
    great_discovery::{
        snapshot_definitions, snapshot_discoveries, snapshot_progress, snapshot_telemetry,
        GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRegistry,
        GreatDiscoveryTelemetry,
    },
    heightfield::ElevationField,
    influencers::InfluentialRoster,
    intensification::{LadderConfig, RungKey, SiteRefusal, SITE_ACCEPTED},
    knowledge_ledger::{encode_ledger_key, KnowledgeLedger, KnowledgeSnapshotPayload},
    labor_config::ForageLaborConfig,
    map_preset::MapPresetsHandle,
    metrics::SimulationMetrics,
    orders::FactionId,
    power::{PowerGridState, PowerIncidentSeverity as GridIncidentSeverity},
    resources::FoodSiteRegistry,
    resources::{
        CapabilityFlags, CommandEventLog, CorruptionLedgers, CorruptionTelemetry,
        DiscoveryProgressLedger, FactionInventory, MoistureRaster, SentimentAxisBias,
        SimulationConfig, SimulationTick, StartLocation, WorldEpoch,
    },
    scalar::Scalar,
    sedentarization::SedentarizationScore,
    sites::DiscoveredSites,
    sites_config::SitesConfigHandle,
    snapshot_overlays_config::{SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle},
    start_profile::{snapshot_profiles, CampaignLabel, StartProfilesHandle},
    supply::SupplyNetworkMembership,
    systems::{
        food_demand, hunt_per_worker_provisions, tile_morale_pressure, MoralePressureConfig,
    },
    telling::BeatLedger,
    terrain::terrain_definition,
    turn_pipeline_config::TurnPipelineConfigHandle,
    victory::VictoryState,
};

use crate::mapgen::MountainType;

use crate::crisis::{
    CrisisMetricKind as InternalCrisisMetricKind,
    CrisisMetricsSnapshot as InternalCrisisMetricsSnapshot, CrisisOverlayCache,
    CrisisSeverityBand as InternalCrisisSeverityBand,
    CrisisTrendSample as InternalCrisisTrendSample,
};

mod campaign;
mod capture;
mod connections;
pub(crate) mod crafting;
mod culture;
mod economy;
mod flora_quotes;
mod governance;
mod knowledge;
mod map;
mod population;
mod publish;
mod subsistence;
mod vision;

pub use campaign::*;
pub use capture::*;
pub(crate) use culture::*;
pub(crate) use economy::*;
pub use flora_quotes::*;
pub(crate) use governance::*;
pub(crate) use knowledge::*;
pub(crate) use map::*;
pub(crate) use population::*;
pub use publish::*;
pub(crate) use subsistence::*;
pub(crate) use vision::*;

// --- shared cross-cutting helpers used by multiple snapshot domains ---

const AXIS_NAMES: [&str; 4] = ["Knowledge", "Trust", "Equity", "Agency"];

const CHANNEL_LABELS: [&str; 4] = ["Popular", "Peer", "Institutional", "Humanitarian"];

/// The per-source **yield forecast** (`ForagePatchState`/`HerdTelemetryState` `per_worker_yield` +
/// policy ceilings) is captured band-agnostically: the productivity multiplier is a per-band value
/// (`PopulationCohortState.output_multiplier`) that scales every forecast field linearly, so the
/// snapshot exports the un-scaled forecast and the client multiplies by the acting band's own.
const FORECAST_OUTPUT_MULTIPLIER: f32 = 1.0;

/// Whether a diff may advance the published baseline it walks.
///
/// The baselines are mutated **in place** (see [`diff_indexed`]), so "don't commit" has to be an
/// argument: there is no longer a freshly-built map a caller could decline to store. A resolved
/// turn advances; a mid-tick **recapture** holds, which is what keeps its deltas cumulative —
/// each one is `baseline(last turn) → now`, so applying them in order is idempotent and losing an
/// intermediate one is harmless (`PublishState::publish`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Baseline {
    Advance,
    Hold,
}

/// Diff one indexed collection against its published baseline, **touching only what changed**.
///
/// The baseline is the last *published* state keyed by `key`; `current` is the freshly captured
/// collection. An entry the baseline already holds and `same` accepts is left **completely
/// alone** — not cloned, not re-inserted, not even rewritten with the identical fresh value — so a
/// steady-state turn costs one hash probe per entry and nothing else. That is the whole point:
/// `tiles`, `power` and `culture_layers` are each one entry per tile, and the overwhelming majority
/// of them are unchanged on any given turn.
///
/// **Leaving the old value in place is the deadband's requirement, not an optimization.** `same` is
/// a rounded, rendering-precision comparison for tiles and culture layers, so the baseline has to
/// keep the value the client actually holds or a slow drift would be re-measured against a moving
/// target and never publish. Storing the *fresh* value for an entry judged unchanged is precisely
/// the bug this shape makes unwritable — there is no store on that path at all. See
/// `TileState::drift_below_the_deadband_still_publishes_as_it_accumulates`.
///
/// Removal is the rare case and is not allowed to cost anything on the common path: the walk counts
/// how many baseline entries the capture still carries, and only when that count falls short of the
/// baseline's size does [`diff_removed`] sweep for the keys that vanished.
fn diff_indexed<K, T, Key, Same>(
    slot: &mut Indexed<K, T>,
    current: &[T],
    key: Key,
    same: Same,
    write: Baseline,
) -> (Vec<T>, Vec<K>)
where
    K: Eq + Hash + Copy,
    T: Clone,
    Key: Fn(&T) -> K,
    Same: Fn(&T, &T) -> bool,
{
    let Indexed { baseline, held } = slot;
    let baseline_len = baseline.len();
    let held_len = held.len();
    let mut sent = Vec::new();
    // How many entries of `current` the baseline already held. Counted before any insert, so a
    // newly-appeared entry does not mask a removal.
    let mut retained = 0usize;
    // The same count for `held`, so a row that only ever existed on a held frame is seen to vanish.
    let mut held_retained = 0usize;
    for state in current {
        let id = key(state);
        let was_held = held_len > 0 && held.contains(&id);
        if was_held {
            held_retained += 1;
        }
        let unchanged = match baseline.get(&id) {
            Some(previous) => {
                retained += 1;
                same(previous, state)
            }
            None => false,
        };
        if unchanged && !was_held {
            continue;
        }
        sent.push(state.clone());
        match write {
            Baseline::Advance => {
                if !unchanged {
                    baseline.insert(id, state.clone());
                }
            }
            // The per-row form of [`diff_whole`]'s Hold arm: a changed row leaves the client off
            // this baseline, an unchanged-but-held one puts it back on.
            Baseline::Hold => {
                if unchanged {
                    held.remove(&id);
                } else {
                    held.insert(id);
                }
            }
        }
    }

    if retained == baseline_len && held_retained == held_len {
        if write == Baseline::Advance {
            held.clear();
        }
        return (sent, Vec::new());
    }
    let removed = diff_removed(baseline, held, current, &key);
    for id in &removed {
        held.remove(id);
    }
    if write == Baseline::Advance {
        for id in &removed {
            baseline.remove(id);
        }
        held.clear();
    }
    (sent, removed)
}

/// The rare second walk: which published keys the captured collection no longer carries.
///
/// Reached only when [`diff_indexed`]'s pass found fewer baseline (or held) hits than there are
/// entries, i.e. when something really was removed. A steady turn never calls it.
///
/// **It sweeps the held set as well as the baseline**, because a row added on a held frame is in no
/// baseline at all — see [`Indexed`].
fn diff_removed<K, T, Key>(
    baseline: &HashMap<K, T>,
    held: &HashSet<K>,
    current: &[T],
    key: Key,
) -> Vec<K>
where
    K: Eq + Hash + Copy,
    Key: Fn(&T) -> K,
{
    let present: HashSet<K> = current.iter().map(&key).collect();
    let mut removed: Vec<K> = baseline
        .keys()
        .filter(|id| !present.contains(id))
        .copied()
        .collect();
    removed.extend(
        held.iter()
            .filter(|id| !present.contains(id) && !baseline.contains_key(id))
            .copied(),
    );
    removed
}

/// A keyed section's published baseline **plus the ids that went out on a held frame**.
///
/// The row-keyed twin of [`Whole`]'s `held` flag, and it closes a hole that flag cannot reach. A
/// [`Baseline::Hold`] frame publishes rows *without storing them*, so an entry that appears on one
/// held frame and is gone by the next lives in **no baseline at all**: the removal sweep has nothing
/// to find, and neither does the next turn's [`Baseline::Advance`] diff, because the baseline never
/// learned the entry existed. The client is left holding a row the sim will never mention again.
///
/// **This class does not self-heal**, which is what separates it from the within-tick reverts a
/// keyed section shrugs off. The worked example is the one that found it: `send_hunt_expedition`
/// spawns a detached party (published on a held frame), `recall_expedition` cancels it in camp and
/// despawns it in the same tick (published on the next held frame) — and every frame from then on,
/// forever, is silent about it. The party row stays on the client's Band panel, its ✕ sends the
/// `BandId` it still holds, and the sim answers `Expedition N does not exist in the simulation` on
/// turn after turn.
///
/// The wrapper exists so a **new** keyed section cannot be added without the set — the baseline
/// field's type carries it rather than a convention someone has to remember, exactly as [`Whole`]
/// does for a whole section.
pub(crate) struct Indexed<K, T> {
    baseline: HashMap<K, T>,
    /// Ids published on a held frame since the last [`Baseline::Advance`]. See the type's doc.
    held: HashSet<K>,
}

impl<K, T> Default for Indexed<K, T> {
    fn default() -> Self {
        Self {
            baseline: HashMap::new(),
            held: HashSet::new(),
        }
    }
}

impl<K: Eq + Hash, T> Indexed<K, T> {
    /// Re-baseline on `baseline` and forget every held id.
    ///
    /// The one caller is a rollback (`PublishState::reset_to_entry`), which hands the client a full
    /// snapshot and commits it in the same breath — so afterwards there is nothing the client could
    /// be holding that this baseline does not know about.
    pub(crate) fn reset(&mut self, baseline: HashMap<K, T>) {
        self.baseline = baseline;
        self.held.clear();
    }

    /// The last committed rows. Nothing in the publisher reads a baseline outside
    /// [`diff_indexed`] — this exists so the diff's tests can assert on what was committed, which
    /// is the only place the deadband rule ("an unchanged entry is not rewritten") is visible.
    #[cfg(test)]
    pub(crate) fn baseline(&self) -> &HashMap<K, T> {
        &self.baseline
    }
}

/// [`diff_indexed`] on exact equality — the rule for every per-entity collection except tiles and
/// culture layers.
fn diff_new<K, T, Key>(
    baseline: &mut Indexed<K, T>,
    current: &[T],
    key: Key,
    write: Baseline,
) -> (Vec<T>, Vec<K>)
where
    K: Eq + Hash + Copy,
    T: Clone + PartialEq,
    Key: Fn(&T) -> K,
{
    diff_indexed(
        baseline,
        current,
        key,
        |previous, state| previous == state,
        write,
    )
}

/// [`diff_indexed`] for tiles, comparing **published** state rather than exact equality.
///
/// Tiles get their own comparison because they are the largest per-entity collection: at 4160
/// entities on an 80x52 map, one field that drifts every turn costs the whole map every turn. See
/// `TileState::same_published_state`.
///
/// (Slow drift reaches the client either way, because the comparison rounds to an absolute grid
/// rather than testing `|a - b| < eps` — a relative band would let sub-band steps accumulate
/// unbounded error, which is why it is not one.)
fn diff_new_tiles(
    baseline: &mut Indexed<u64, TileState>,
    current: &[TileState],
    write: Baseline,
) -> (Vec<TileState>, Vec<u64>) {
    diff_indexed(
        baseline,
        current,
        |state| state.entity,
        TileState::same_published_state,
        write,
    )
}

/// [`diff_new_tiles`] for culture layers — same reasoning, same deadband rule; see
/// `CultureLayerState::same_published_state`.
fn diff_new_culture_layers(
    baseline: &mut Indexed<u32, CultureLayerState>,
    current: &[CultureLayerState],
    write: Baseline,
) -> (Vec<CultureLayerState>, Vec<u32>) {
    diff_indexed(
        baseline,
        current,
        |state| state.id,
        CultureLayerState::same_published_state,
        write,
    )
}

/// A whole-section baseline **plus the flag that says the client may have moved off it**.
///
/// Diffing against the last *turn* rather than the last *publication* leaves a hole that never
/// errors: if one command changes a section and a later command **in the same tick** changes it
/// back, the second [`Baseline::Hold`] diff finds the section equal to the turn baseline and sends
/// nothing — while the client is still holding the intermediate value the first command published.
/// The client is stale against a baseline it agrees with, so nothing anywhere reports it.
///
/// `held` closes it: it means *"this section went out on a held frame since the last
/// [`Baseline::Advance`], so the client may be holding a value this baseline does not know about"*.
/// [`diff_whole`] restates a section whose flag is set even when it compares unchanged.
///
/// The wrapper exists so a **new** whole section cannot be added without the flag — the baseline
/// field's type carries it rather than a convention someone has to remember.
#[derive(Debug, Default, Clone)]
pub(crate) struct Whole<T> {
    baseline: T,
    /// Published on a held frame since the last [`Baseline::Advance`]. See the type's doc comment.
    held: bool,
}

impl<T> Whole<T> {
    /// The last committed value, for the paths that compare against it outside [`diff_whole`]
    /// (the auxiliary feed deltas).
    pub(crate) fn baseline(&self) -> &T {
        &self.baseline
    }

    /// Re-baseline on `value` and clear the held flag.
    ///
    /// The two callers both hand the client a value and commit it in the same breath — a rollback
    /// re-baselines with a full snapshot (`PublishState::reset_to_entry`), and an auxiliary feed
    /// delta publishes its own section whole — so afterwards there is nothing the client could be
    /// holding that this baseline does not know about.
    pub(crate) fn reset(&mut self, value: T) {
        self.baseline = value;
        self.held = false;
    }
}

/// Diff one **whole section** against its published baseline: `None` when nothing changed and the
/// client is known to be on that baseline.
///
/// The clone happens only on the changed branch. It used to be unconditional and taken *before* the
/// comparison (`let state = snapshot.x.clone(); if self.x == state {…}`), which cost a full copy of
/// every raster, roster and telemetry block every turn to discover that most of them had not moved.
///
/// **An unchanged section is not rewritten either**, for the same reason the indexed diff leaves an
/// unchanged entry alone: the baseline already equals the captured value, so the assignment is a
/// copy that cannot change anything.
///
/// **The two rules around [`Whole::held`]**, which are what stop a within-tick revert from leaving
/// the client holding a superseded intermediate value:
/// * [`Baseline::Advance`] — send when changed (and advance), send when *unchanged but held* (a
///   restatement, because the last thing the client was given is not this baseline). Clear the flag
///   either way.
/// * [`Baseline::Hold`] — send when changed and **set** the flag; send when *unchanged but held*
///   and **clear** it, because that frame puts the client back on the baseline. Unchanged and not
///   held sends nothing, exactly as before.
///
/// Every held frame is therefore still exactly `baseline(last turn) → now` plus at most one
/// redundant restatement, so a recapture delta stays cumulative and dropping one stays harmless.
fn diff_whole<T>(slot: &mut Whole<T>, current: &T, write: Baseline) -> Option<T>
where
    T: Clone + PartialEq,
{
    let unchanged = slot.baseline == *current;
    match write {
        Baseline::Advance => {
            let held = std::mem::take(&mut slot.held);
            if unchanged && !held {
                return None;
            }
            if !unchanged {
                slot.baseline = current.clone();
            }
            Some(current.clone())
        }
        Baseline::Hold => {
            if unchanged && !slot.held {
                return None;
            }
            slot.held = !unchanged;
            Some(current.clone())
        }
    }
}

/// A section that is **indexed for comparison but sent whole**: the baseline is a map (so other
/// paths can look an entry up by id) while the wire carries the entire list or nothing.
///
/// `great_discovery_definitions` is the only one — a catalog that changes when a config is loaded
/// and never during play, so the comparison is a probe per entry and the rebuild is unreachable in
/// steady state.
///
/// It sends a whole list, so it carries a [`Whole`] and obeys [`diff_whole`]'s held rules verbatim.
fn diff_whole_indexed<K, T, Key>(
    slot: &mut Whole<HashMap<K, T>>,
    current: &[T],
    key: Key,
    write: Baseline,
) -> Option<Vec<T>>
where
    K: Eq + Hash + Copy,
    T: Clone + PartialEq,
    Key: Fn(&T) -> K,
{
    let unchanged = current.len() == slot.baseline.len()
        && current.iter().all(|state| {
            slot.baseline
                .get(&key(state))
                .is_some_and(|previous| previous == state)
        });
    match write {
        Baseline::Advance => {
            let held = std::mem::take(&mut slot.held);
            if unchanged && !held {
                return None;
            }
            if !unchanged {
                slot.baseline.clear();
                for state in current {
                    slot.baseline.insert(key(state), state.clone());
                }
            }
            Some(current.to_vec())
        }
        Baseline::Hold => {
            if unchanged && !slot.held {
                return None;
            }
            slot.held = !unchanged;
            Some(current.to_vec())
        }
    }
}

/// Diff an **append-only log** against a published cursor: the rows appended since, or `None`.
///
/// The third diff shape, and the only one whose baseline is a single number. `command_events` is a
/// log — rows are appended, never edited, and the oldest fall out of the retention window — so
/// `diff_whole` re-serialised the entire retained ring on every turn any event fired, which at a
/// 20-turn window is ~200 rows to say that three of them are new.
///
/// **A dropped delta permanently loses the events it carried**, where a whole-vector resend was
/// self-healing. That is safe for exactly one reason: the client applies a delta only when it holds
/// the base frame (`WorldCache::accepts`), and a gap raises `resync_needed`, whose answer is a full
/// snapshot carrying the whole retained ring. Break that gate and this diff silently drops history —
/// `core_sim/tests/delta_streaming.rs` pins the pairing.
///
/// **`Baseline::Hold` must not advance the cursor**, and that is load-bearing rather than symmetry
/// with the other diffs: a mid-tick recapture that advanced it would consume the rows, and the next
/// real turn delta — which diffs from the *turn's* baseline — would never send them at all. It is
/// the same cumulativity property recaptures already have for every other section.
fn diff_appended(
    baseline: &mut u64,
    current: &[CommandEventState],
    write: Baseline,
) -> Option<Vec<CommandEventState>> {
    let cursor = *baseline;
    let appended: Vec<CommandEventState> = current
        .iter()
        .filter(|state| state.seq > cursor)
        .cloned()
        .collect();
    if appended.is_empty() {
        return None;
    }
    if write == Baseline::Advance {
        // The highest seq PRESENT, not the highest ever issued: the two differ only if the window
        // evicted the newest row, which cannot happen, and taking the max of what shipped is what
        // keeps the cursor a statement about the client's held state.
        if let Some(highest) = appended.iter().map(|state| state.seq).max() {
            *baseline = highest;
        }
    }
    Some(appended)
}

/// The O(changed) property of the indexed diffs, asserted on the baseline map itself.
///
/// A steady-state turn is almost entirely unchanged entries, so "an unchanged entry is not touched"
/// is the property the whole publication budget now rests on — and it is invisible from the delta,
/// which looks the same either way. These tests read the baseline instead.
#[cfg(test)]
mod indexed_diff_tests {
    use super::*;
    use sim_runtime::{MountainKind, TerrainTags, TerrainType};

    /// Half a hundredth: below `same_published_state`'s rounding grid, so a tile carrying it is
    /// judged UNCHANGED while still being `!=` — which is what makes "was the baseline rewritten?"
    /// observable.
    const SUB_DEADBAND_DRIFT: f32 = 0.004;

    fn tile(entity: u64, relief: f32) -> TileState {
        TileState {
            entity,
            x: 0,
            y: 0,
            element: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
            culture_layer: 0,
            mountain_kind: MountainKind::None,
            mountain_relief: relief,
            habitability: 0,
            graze_biomass: 0.0,
            graze_capacity: 0.0,
            graze_ecology_phase: 0,
            forage_capacity: 0.0,
            underlying_terrain: TerrainType::AlluvialPlain,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        }
    }

    /// A tile baseline already carrying `states`, committed — the starting point every case below
    /// diffs against.
    fn indexed<const N: usize>(states: [TileState; N]) -> Indexed<u64, TileState> {
        let mut slot = Indexed::default();
        slot.reset(states.into_iter().map(|s| (s.entity, s)).collect());
        slot
    }

    /// **An unchanged entry is not rewritten** — not with the old value, and above all not with the
    /// fresh one.
    ///
    /// Rewriting it with the fresh value would be the deadband bug: each turn's sub-hundredth step
    /// would be measured against the previous turn's, so an accumulating drift would never cross the
    /// grid and the client would hold a stale tile forever. The baseline keeping the last *published*
    /// value is what bounds the client's error, and here that is demonstrated by the baseline still
    /// carrying the ORIGINAL relief after a diff that judged the tile unchanged.
    #[test]
    fn an_unchanged_entry_leaves_the_baseline_holding_its_last_published_value() {
        let mut baseline = indexed([tile(1, 1.0)]);

        let captured = vec![tile(1, 1.0 + SUB_DEADBAND_DRIFT)];
        let (sent, removed) = diff_new_tiles(&mut baseline, &captured, Baseline::Advance);

        assert!(sent.is_empty(), "a sub-deadband step publishes nothing");
        assert!(removed.is_empty());
        assert_eq!(
            baseline.baseline()[&1].mountain_relief,
            1.0,
            "the baseline must still hold the last PUBLISHED value — storing the fresh one would \
             restart the deadband every turn and freeze an accumulating drift out of the delta"
        );
    }

    /// The complement: a change past the grid is sent *and* advances the baseline, so the next
    /// turn's comparison is against what the client now holds.
    #[test]
    fn a_changed_entry_is_sent_and_advances_the_baseline() {
        let mut baseline = indexed([tile(1, 1.0)]);

        let captured = vec![tile(1, 2.0)];
        let (sent, _) = diff_new_tiles(&mut baseline, &captured, Baseline::Advance);

        assert_eq!(sent.len(), 1);
        assert_eq!(baseline.baseline()[&1].mountain_relief, 2.0);
    }

    /// `Baseline::Hold` computes the same delta and advances nothing — the mid-tick recapture path,
    /// which must leave the baseline where the last resolved turn left it or its cumulative deltas
    /// stop being cumulative.
    #[test]
    fn holding_the_baseline_still_reports_the_change_but_does_not_commit_it() {
        let mut baseline = indexed([tile(1, 1.0)]);

        let captured = vec![tile(1, 2.0)];
        let (sent, _) = diff_new_tiles(&mut baseline, &captured, Baseline::Hold);

        assert_eq!(sent.len(), 1, "the delta is the same either way");
        assert_eq!(
            baseline.baseline()[&1].mountain_relief,
            1.0,
            "a held baseline is unmoved, so the next diff re-reports the same change"
        );
    }

    /// Removal is the rare path, and it still has to work: a key the capture no longer carries is
    /// reported once and leaves the baseline.
    #[test]
    fn a_vanished_entry_is_reported_and_dropped_from_the_baseline() {
        let mut baseline = indexed([tile(1, 1.0), tile(2, 1.0)]);

        let captured = vec![tile(1, 1.0)];
        let (sent, removed) = diff_new_tiles(&mut baseline, &captured, Baseline::Advance);

        assert!(sent.is_empty());
        assert_eq!(removed, vec![2]);
        assert!(!baseline.baseline().contains_key(&2));
        assert_eq!(baseline.baseline().len(), 1);
    }

    /// **A row that only ever existed on HELD frames is still reported when it vanishes.**
    ///
    /// A held frame publishes without storing, so an entry added on one and gone by the next is in
    /// no baseline at all: the removal sweep had nothing to find, and so did every later diff,
    /// because the baseline never learned the row existed. That is the ghost — it does not
    /// self-heal, unlike the within-tick reverts a keyed section shrugs off. See [`Indexed`].
    #[test]
    fn a_row_added_and_removed_across_held_frames_is_still_reported_as_removed() {
        let mut baseline = indexed([tile(1, 1.0)]);

        let (sent, removed) =
            diff_new_tiles(&mut baseline, &[tile(1, 1.0), tile(2, 1.0)], Baseline::Hold);
        assert_eq!(sent.len(), 1, "the held frame publishes the new row");
        assert!(removed.is_empty());

        let (_, removed) = diff_new_tiles(&mut baseline, &[tile(1, 1.0)], Baseline::Hold);
        assert_eq!(
            removed,
            vec![2],
            "the next frame must say the row is gone, though no baseline ever held it"
        );

        let (_, removed) = diff_new_tiles(&mut baseline, &[tile(1, 1.0)], Baseline::Advance);
        assert!(
            removed.is_empty(),
            "and it is reported once — the client has been told, so the held id is forgotten"
        );
    }

    /// The row-keyed twin of `Whole::held`'s restatement: a value changed on a held frame and back
    /// to the baseline's on the next must be **re-sent**, because the client is holding the
    /// intermediate value and no comparison against the baseline can see that.
    #[test]
    fn a_row_reverted_within_a_tick_is_restated() {
        let mut baseline = indexed([tile(1, 1.0)]);

        let (sent, _) = diff_new_tiles(&mut baseline, &[tile(1, 2.0)], Baseline::Hold);
        assert_eq!(sent.len(), 1, "the held frame publishes the change");

        let (sent, _) = diff_new_tiles(&mut baseline, &[tile(1, 1.0)], Baseline::Hold);
        assert_eq!(
            sent.len(),
            1,
            "reverting to the baseline's value must be restated — the client holds the other one"
        );

        let (sent, _) = diff_new_tiles(&mut baseline, &[tile(1, 1.0)], Baseline::Hold);
        assert!(
            sent.is_empty(),
            "and once restated the client is back on the baseline, so nothing more is owed"
        );
    }

    /// A whole section that did not move is neither cloned nor written back.
    #[test]
    fn an_unchanged_whole_section_is_not_rewritten() {
        let mut slot = Whole::default();
        slot.reset(vec![1u8, 2, 3]);
        assert_eq!(
            diff_whole(&mut slot, &vec![1u8, 2, 3], Baseline::Advance),
            None
        );
        assert_eq!(
            diff_whole(&mut slot, &vec![4u8], Baseline::Advance),
            Some(vec![4u8])
        );
        assert_eq!(slot.baseline(), &vec![4u8]);
    }

    /// A section changed and changed **back** within one tick is restated, rather than silently
    /// leaving the client on the intermediate value the first held frame published.
    #[test]
    fn a_section_reverted_within_a_tick_is_restated_and_then_settles() {
        let mut slot = Whole::default();
        slot.reset(vec![1u8]);

        // The command that moves it: a held frame carries the new value and flags the section.
        assert_eq!(
            diff_whole(&mut slot, &vec![2u8], Baseline::Hold),
            Some(vec![2u8])
        );
        // The command that moves it back: equal to the baseline, but the client is not on it.
        assert_eq!(
            diff_whole(&mut slot, &vec![1u8], Baseline::Hold),
            Some(vec![1u8])
        );
        // The flag is cleared by that restatement, so a third held frame is quiet again.
        assert_eq!(diff_whole(&mut slot, &vec![1u8], Baseline::Hold), None);
        // …and so is the turn that follows, which is the property the steady-turn budget rests on.
        assert_eq!(diff_whole(&mut slot, &vec![1u8], Baseline::Advance), None);
        assert_eq!(slot.baseline(), &vec![1u8]);
    }

    /// A revert that straddles a turn boundary is restated by the turn: the flag outlives the held
    /// frames and is consulted by the [`Baseline::Advance`] arm.
    #[test]
    fn a_turn_restates_a_section_a_held_frame_moved_and_gave_back() {
        let mut slot = Whole::default();
        slot.reset(vec![1u8]);

        assert_eq!(
            diff_whole(&mut slot, &vec![2u8], Baseline::Hold),
            Some(vec![2u8])
        );
        assert_eq!(
            diff_whole(&mut slot, &vec![1u8], Baseline::Advance),
            Some(vec![1u8])
        );
        assert_eq!(diff_whole(&mut slot, &vec![1u8], Baseline::Advance), None);
    }

    fn event(seq: u64) -> CommandEventState {
        CommandEventState {
            tick: seq,
            kind: "born".to_string(),
            faction: 0,
            label: format!("event {seq}"),
            detail: None,
            seq,
        }
    }

    #[test]
    fn an_append_only_diff_ships_only_the_rows_above_the_cursor() {
        let mut cursor = 0u64;
        let ring = vec![event(1), event(2), event(3)];

        let first = diff_appended(&mut cursor, &ring, Baseline::Advance)
            .expect("a fresh cursor is owed every row");
        assert_eq!(
            first.iter().map(|row| row.seq).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(cursor, 3, "the cursor advances to the newest row shipped");

        assert_eq!(
            diff_appended(&mut cursor, &ring, Baseline::Advance),
            None,
            "nothing new is `None`, not an empty vector — an empty section on the wire would mean \
             'the log was cleared'"
        );

        let mut grown = ring.clone();
        grown.push(event(4));
        let second = diff_appended(&mut cursor, &grown, Baseline::Advance).expect("one new row");
        assert_eq!(
            second.iter().map(|row| row.seq).collect::<Vec<_>>(),
            vec![4]
        );
        assert_eq!(cursor, 4);
    }

    /// **The `Hold` case, which is the whole reason `write` is a parameter here.** A mid-tick
    /// recapture must not consume rows: it does not commit the baseline, so the committed turn
    /// delta is still responsible for them, and a cursor advanced by a recapture would make that
    /// turn delta skip them forever.
    #[test]
    fn a_held_diff_reports_the_same_rows_again() {
        let mut cursor = 0u64;
        let ring = vec![event(1), event(2)];

        let recapture = diff_appended(&mut cursor, &ring, Baseline::Hold).expect("both rows");
        assert_eq!(
            recapture.iter().map(|row| row.seq).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            cursor, 0,
            "a recapture leaves the cursor exactly where it was"
        );

        let mut grown = ring.clone();
        grown.push(event(3));
        let turn = diff_appended(&mut cursor, &grown, Baseline::Advance)
            .expect("the committed turn still owes all three");
        assert_eq!(
            turn.iter().map(|row| row.seq).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "cumulative — losing the recapture frame costs nothing"
        );
        assert_eq!(cursor, 3);
    }

    /// The window evicting old rows must not re-send anything: the cursor is a statement about the
    /// client, not an index into the ring.
    #[test]
    fn eviction_does_not_resurrect_delivered_rows() {
        let mut cursor = 0u64;
        diff_appended(&mut cursor, &[event(1), event(2)], Baseline::Advance).expect("both rows");

        // The window dropped 1 and 2; 3 is new.
        let after = diff_appended(&mut cursor, &[event(3)], Baseline::Advance).expect("one row");
        assert_eq!(after.iter().map(|row| row.seq).collect::<Vec<_>>(), vec![3]);
    }
}

#[cfg(test)]
mod tests {

    /// One animal, for the snapshot fixtures (slice 8). These tests assert what crosses the wire, not
    /// what a take pays, so the quantum is deliberately small enough never to bind.
    const SNAPSHOT_BODY_MASS: f32 = 1.0;

    use super::*;
    // Used only by the fixtures below. They lived at file scope while
    // `restore_world_from_snapshot` needed them too; with that gone, the tests are the only caller.
    use crate::components::{ElementKind, LocalStore, MoraleCause, TakeSelection, YieldRange};
    use crate::forage::ForagePatch;
    use crate::power::PowerNodeId;
    use crate::{
        labor_config::LaborConfig,
        orders::FactionId,
        power::PowerIncidentSeverity as GridIncidentSeverity,
        resources::{CorruptionTelemetry, DiscoveryProgressLedger},
        scalar::Scalar,
        PowerIncident,
    };
    use bevy::math::UVec2;
    use sim_runtime::{
        CorruptionEntry, CorruptionSubsystem, GreatDiscoveryProgressState, GreatDiscoveryState,
        GreatDiscoveryTelemetryState, KnowledgeField, TerrainTags, TerrainType,
    };

    /// The viewer every herd-export fixture below captures for.
    const VIEWER: FactionId = FactionId(0);

    /// A ledger in which [`VIEWER`] can see the whole `size × size` map — the neutral fixture for the
    /// tests that are about *what a herd exports*, not about *whether it exports*. The fog filter
    /// itself is exercised by `herd_snapshot_entries` fog tests below, which build partial ledgers.
    fn all_seeing_ledger(size: u32) -> crate::visibility::VisibilityLedger {
        let mut ledger = crate::visibility::VisibilityLedger::default();
        let map = ledger.ensure_faction(VIEWER, size, size);
        for y in 0..size {
            for x in 0..size {
                map.mark_active(x, y, 0);
            }
        }
        ledger
    }

    /// The four wire-shape fixtures below differ only in their herd registry, so the plumbing is
    /// stated once. Grid is 64×64 (the fixtures' own `UVec2::new(64, 64)`), fully visible.
    /// Fog of war ON — the default, and the state every fog fixture below is written against.
    fn export_herds(
        telemetry: &HerdTelemetry,
        registry: &HerdRegistry,
        fauna: &FaunaConfig,
        labor: &LaborConfig,
        visibility: &crate::visibility::VisibilityLedger,
    ) -> Vec<HerdTelemetryState> {
        export_herds_with_fog(telemetry, registry, fauna, labor, visibility, true)
    }

    /// `export_herds` with the server-owned fog switch exposed, for the fog-disabled fixtures.
    fn export_herds_with_fog(
        telemetry: &HerdTelemetry,
        registry: &HerdRegistry,
        fauna: &FaunaConfig,
        labor: &LaborConfig,
        visibility: &crate::visibility::VisibilityLedger,
        fog_enabled: bool,
    ) -> Vec<HerdTelemetryState> {
        // These fixtures test the FOG FILTER, not the pricing, so every species quotes the same
        // ordinary band — `parties` is left empty and every row falls through to the fallback.
        let fallback = QuotedParty {
            party: crate::fauna::HuntingParty::builtin_equipped(),
            kit_id: crate::equipment_config::EquipmentConfig::builtin()
                .default_kit_id(crate::equipment_config::KitJob::Hunt)
                .to_string(),
        };
        // **No graze layer in the fog fixtures** — these test the visibility filter, not a capacity,
        // and an empty registry is exactly the "K is frozen" state `settled_capacity` answers for.
        let graze = crate::graze::GrazeRegistry::default();
        herd_snapshot_entries(HerdSnapshotInputs {
            telemetry,
            registry,
            fauna,
            ladder: &LadderConfig::builtin(),
            equipped_haul_rate: crate::equipment_config::EquipmentConfig::builtin()
                .equipped_reference(
                    crate::equipment_config::EquipmentStat::HuntCarry,
                    labor.hunt.per_worker_biomass_capacity,
                ),
            grid_size: UVec2::new(64, 64),
            wrap_horizontal: false,
            graze: &graze,
            visibility,
            viewer: VIEWER,
            fog_enabled,
            parties: &HashMap::new(),
            penned_parties: &HashMap::new(),
            fallback_party: &fallback,
            // The fog fixtures queue nothing, so no source names a builders kit.
            build_kits: &crate::snapshot::subsistence::BuildKitIds::default(),
        })
    }

    fn tile(entity: u64, x: u32, y: u32) -> TileState {
        TileState {
            entity,
            x,
            y,
            element: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
            culture_layer: 0,
            mountain_kind: MountainKind::None,
            mountain_relief: 1.0,
            habitability: 0,
            graze_biomass: 0.0,
            graze_capacity: 0.0,
            graze_ecology_phase: GRAZE_PHASE_NONE,
            forage_capacity: 0.0,
            underlying_terrain: TerrainType::AlluvialPlain,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        }
    }

    /// `TileState::forage_capacity` is the biome's HUMAN-food potential, read straight from
    /// `forage.capacity_by_biome` for EVERY tile (not from the sparse `ForagePatch`). Confirms the
    /// four contract cases + the no-drift consistency check against a seeded patch.
    #[test]
    fn tile_state_exports_forage_potential_from_biome_table() {
        let labor = LaborConfig::builtin();
        let forage = &labor.forage;
        // The place-based morale terms are irrelevant to the forage readout; zero them out.
        let morale_cfg = MoralePressureConfig {
            ambient_temperature: Scalar::zero(),
            temperature_morale_penalty: Scalar::zero(),
            temperature_morale_tolerance: Scalar::zero(),
            attrition_penalty_scale: Scalar::zero(),
            hardness_penalty_scale: Scalar::zero(),
        };
        let at = |terrain: TerrainType| Tile {
            position: UVec2::new(1, 1),
            element: ElementKind::Arborite,
            temperature: Scalar::zero(),
            terrain,
            terrain_tags: TerrainTags::empty(),
            underlying_terrain: None,
            mountain: None,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        };
        let entity = Entity::from_raw(1);
        let capture = |terrain: TerrainType, graze: Option<&GrazePatch>| {
            tile_state(entity, &at(terrain), &morale_cfg, graze, forage).forage_capacity
        };

        // (a) A food-module tile that DOES hold a `ForagePatch` — the patch was seeded at
        //     `capacity_for(biome)`, so the exported potential must equal its carrying capacity (no
        //     drift between the potential and the realized patch).
        let module_terrain = TerrainType::MixedWoodland;
        let seeded = ForagePatch::new(
            at(module_terrain).position,
            forage.capacity_for(module_terrain),
        );
        assert_eq!(capture(module_terrain, None), seeded.carrying_capacity);

        // (b) A non-food-module LAND tile with a positive-forage biome still exports a NON-ZERO
        //     potential — the whole point: the client sees the biome's potential everywhere, not only
        //     where a patch happens to sit.
        assert!(capture(TerrainType::PrairieSteppe, None) > 0.0);

        // (c) Fishery WATER carries a non-zero fishing value — the deliberate divergence from graze
        //     (where all water is zero): a fishery is a food module on water.
        assert!(capture(TerrainType::ContinentalShelf, None) > 0.0);

        // (d) A genuinely-zero biome reads a STATED zero.
        assert_eq!(capture(TerrainType::Glacier, None), 0.0);
        assert_eq!(capture(TerrainType::DeepOcean, None), 0.0);
    }

    fn snapshot_with_overlay(
        tick: u64,
        tile: TileState,
        overlay: TerrainOverlayState,
    ) -> WorldSnapshot {
        let tiles = vec![tile];
        let header = SnapshotHeader::new(tick, tiles.len(), 0, 0, 0);
        WorldSnapshot {
            header,
            kits: Vec::new(),
            materials: Vec::new(),
            characteristic_bands: Vec::new(),
            recipes: Vec::new(),
            craft_knowledge: Vec::new(),
            default_hunt_kit_id: String::new(),
            default_forage_kit_id: String::new(),
            default_scout_kit_id: String::new(),
            default_warrior_kit_id: String::new(),
            equipment_config_json: String::new(),
            tiles,
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics: PowerTelemetryState::default(),
            great_discovery_definitions: Vec::new(),
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            command_events_retention_turns: 0,
            pending_forks: Vec::new(),
            stance_axes: Vec::new(),
            voice_medium: Vec::new(),
            herds: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            connections: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            terrain: overlay,
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            climate_bands: ClimateBandsState::default(),
            start_marker: None,
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
            fog_enabled: true,
        }
    }

    fn snapshot_with_discoveries(
        tick: u64,
        great_discoveries: Vec<GreatDiscoveryState>,
        great_discovery_progress: Vec<GreatDiscoveryProgressState>,
        great_discovery_telemetry: GreatDiscoveryTelemetryState,
    ) -> WorldSnapshot {
        let header = SnapshotHeader::new(tick, 0, 0, 0, 0);
        WorldSnapshot {
            header,
            kits: Vec::new(),
            materials: Vec::new(),
            characteristic_bands: Vec::new(),
            recipes: Vec::new(),
            craft_knowledge: Vec::new(),
            default_hunt_kit_id: String::new(),
            default_forage_kit_id: String::new(),
            default_scout_kit_id: String::new(),
            default_warrior_kit_id: String::new(),
            equipment_config_json: String::new(),
            tiles: Vec::new(),
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics: PowerTelemetryState::default(),
            great_discovery_definitions: Vec::new(),
            great_discoveries,
            great_discovery_progress,
            great_discovery_telemetry,
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            command_events_retention_turns: 0,
            pending_forks: Vec::new(),
            stance_axes: Vec::new(),
            voice_medium: Vec::new(),
            herds: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            connections: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            climate_bands: ClimateBandsState::default(),
            start_marker: None,
            terrain: TerrainOverlayState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
            fog_enabled: true,
        }
    }

    fn snapshot_with_power_metrics(tick: u64, power_metrics: PowerTelemetryState) -> WorldSnapshot {
        let header = SnapshotHeader::new(tick, 0, 0, 0, 0);
        WorldSnapshot {
            header,
            kits: Vec::new(),
            materials: Vec::new(),
            characteristic_bands: Vec::new(),
            recipes: Vec::new(),
            craft_knowledge: Vec::new(),
            default_hunt_kit_id: String::new(),
            default_forage_kit_id: String::new(),
            default_scout_kit_id: String::new(),
            default_warrior_kit_id: String::new(),
            equipment_config_json: String::new(),
            tiles: Vec::new(),
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics,
            great_discovery_definitions: Vec::new(),
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            command_events_retention_turns: 0,
            pending_forks: Vec::new(),
            stance_axes: Vec::new(),
            voice_medium: Vec::new(),
            herds: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            connections: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            climate_bands: ClimateBandsState::default(),
            start_marker: None,
            terrain: TerrainOverlayState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
            fog_enabled: true,
        }
    }

    /// Build a minimal content band for the food-flow snapshot test, with the given age brackets
    /// (fixed-point) and labor allocation.
    fn food_test_cohort(
        children: Scalar,
        working: Scalar,
        elders: Scalar,
        allocation: LaborAllocation,
    ) -> (PopulationCohort, LaborAllocation) {
        let cohort = PopulationCohort {
            home: Entity::from_raw(2),
            current_tile: Entity::from_raw(2),
            size: 30,
            children,
            working,
            elders,
            stores: LocalStore::new(),
            morale: crate::scalar::scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: crate::scalar::scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: Default::default(),
            last_fertility_factors: Default::default(),
            discontent_fraction: crate::scalar::scalar_zero(),
            grievance: crate::scalar::scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0,
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        };
        (cohort, allocation)
    }

    /// Capture a cohort's `PopulationCohortState` with all-default configs (isolates the food-flow
    /// wiring). Returns the built state.
    fn capture_food_state(
        cohort: &PopulationCohort,
        allocation: &LaborAllocation,
    ) -> PopulationCohortState {
        let demographics = crate::demographics_config::DemographicsConfig::default();
        let wellbeing = crate::wellbeing_config::WellbeingConfig::default();
        let membership = crate::supply::SupplyNetworkMembership::default();
        let stages = crate::settlement_stage_config::SettlementStageConfig::default();
        // Neither the expedition levers nor the TOE tiers are relevant to the food-flow wiring
        // under test.
        let equipment_config = crate::equipment_config::EquipmentConfig::builtin();
        let kit_levers = population::BandKitLevers {
            config: &equipment_config,
            person_intrinsic: crate::creatures_config::CreaturesConfig::builtin().person(),
            baseline_haul_rate: crate::labor_config::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity,
            baseline_gather_rate: crate::labor_config::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
            equipped_vantage_range: crate::labor_config::LaborConfig::builtin()
                .scout
                .vantage_range as f32,
        };
        let levers = ExpeditionLevers {
            hunt_per_worker_carry: 0.0,
            trade_per_worker_carry: 0.0,
            trade_material_carry_weight: 0.0,
            hunt_per_worker_provisions: 0.0,
            hunt_viability_warn_turns: 0,
            hunt_forecast_horizon_turns: 0,
            band_move_tiles_per_turn: 0,
            settle_min_founding_workers: 0,
            settle_parent_min_workers: 0,
        };
        population_state(PopulationStateInputs {
            entity: Entity::from_raw(1),
            // This fixture asserts on the derived readouts, not on band identity.
            band_id: None,
            cohort,
            allocation: Some(allocation),
            expedition: None,
            current_position: None,
            is_traveling: false,
            demographics: &demographics,
            wellbeing: &wellbeing,
            supply_membership: &membership,
            work_range: 0,
            raid_radius: 0,
            scout_vantage_distance: 0,
            expedition_levers: &levers,
            settlement_stage_config: &stages,
            travel_target: None,
            hunt_reach: 0,
            expedition_delivery: None,
            // This fixture asserts on the food ledger, not the TOE.
            equipment: None,
            kit_levers: &kit_levers,
            hunt_crew_levers: crate::snapshot::population::builtin_hunt_crew_levers(),
            bench: None,
            craft_inputs: crate::snapshot::crafting::builtin_craft_inputs(),
            build_sources: crate::snapshot::population::empty_build_sources(),
        })
    }

    /// (d) `food_income` = Σ per-source `actual_yield`, `food_consumption` = the food the people
    /// actually ate this turn (`cohort.last_food_consumption`), and each labor-assignment row carries
    /// its matching actual/sustainable yield (zipped by index).
    #[test]
    fn population_state_reports_food_income_and_consumption() {
        let working = Scalar::from_f32(30.0);
        let allocation = LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        floor: 0.5,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: 10,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: "game_1".to_string(),
                        floor: 0.5,
                    },
                    workers: 5,
                    kit: None,
                },
            ],
            build_queue: Vec::new(),
            last_yields: vec![
                SourceYield {
                    // A staple gather: nothing anyone builds with, and no fodder crop in the basket.
                    materials: Vec::new(),
                    fodder: 0.0,
                    actual: 2.5,
                    sustainable: 2.5,
                    wasted: 0.0,
                    workers_needed: 1,
                    overdraws: false,
                    // Continuous forage: realized == actual.
                    realized: 2.5,
                    // A continuous source lands the same amount every turn.
                    arrivals: vec![2.5; 3],
                    // A resolved fixture row: the take happened, so its band is a point.
                    range: YieldRange::certain(2.5),
                },
                SourceYield {
                    // A hunt: no animal pays fodder, and this fixture's quarry is made of nothing.
                    materials: Vec::new(),
                    fodder: 0.0,
                    actual: 0.5,
                    sustainable: 0.25,
                    wasted: 0.0,
                    workers_needed: 5,
                    overdraws: true,
                    // Lumpy hunt: the steady average is below this kill turn's spike.
                    realized: 0.25,
                    // A lumpy hunt: nothing for two turns, then a whole animal.
                    arrivals: vec![0.0, 0.0, 0.75],
                    range: YieldRange::certain(0.5),
                },
            ],
            last_pen_feed_upkeep: 0.0,
            last_raid_forfeit: 0.0,
            last_transfer_received: 0.0,
            last_transfer_sent: 0.0,
            upkeep_fund_mode: crate::intensification::UpkeepFundMode::default(),
        };
        let (mut cohort, allocation) = food_test_cohort(
            Scalar::from_f32(0.0),
            working,
            Scalar::from_f32(0.0),
            allocation,
        );
        // The food the people actually ate this turn (the real `stores` debit `simulate_population`
        // records), which the ledger's `food_consumption` term echoes verbatim — NOT a `food_demand`
        // re-derived at capture on the post-turn brackets (that would break the larder identity by
        // the same turn's population growth).
        const CONSUMED: f32 = 4.13;
        cohort.last_food_consumption = CONSUMED;
        let state = capture_food_state(&cohort, &allocation);

        // food_income = Σ actual (2.5 + 0.5) — the real (lumpy) arrivals, unchanged.
        assert!(
            (state.food_income - 3.0).abs() < 1e-5,
            "food_income sums per-source actual: {}",
            state.food_income
        );
        // food_consumption == the food actually eaten (`cohort.last_food_consumption`).
        assert!(
            (state.food_consumption - CONSUMED).abs() < 1e-5,
            "food_consumption == last_food_consumption: {} vs {}",
            state.food_consumption,
            CONSUMED
        );
        // Each assignment row carries its zipped actual/sustainable.
        assert_eq!(state.labor_assignments.len(), 2);
        assert!((state.labor_assignments[0].actual_yield - 2.5).abs() < 1e-5);
        assert!((state.labor_assignments[0].sustainable_yield - 2.5).abs() < 1e-5);
        assert!((state.labor_assignments[1].actual_yield - 0.5).abs() < 1e-5);
        assert!((state.labor_assignments[1].sustainable_yield - 0.25).abs() < 1e-5);
        // The overstaffing signal (workers_needed) carries onto the display state, zipped by index.
        assert_eq!(state.labor_assignments[0].workers_needed, 1);
        assert_eq!(state.labor_assignments[1].workers_needed, 5);
        // The steady realized rate carries onto each row too (the client's headline "Food /turn").
        assert!((state.labor_assignments[0].realized_yield - 2.5).abs() < 1e-5);
        assert!((state.labor_assignments[1].realized_yield - 0.25).abs() < 1e-5);
    }

    /// **A fodder-only source publishes its fodder and moves `food_income` by nothing** (issue #449).
    ///
    /// The shape the field exists for, in miniature: a sown hay Field pays no provisions and no trade
    /// and real fodder, so its row must state that fodder rather than the `+0.00` every compact
    /// readout showed. And it must state it *beside* the ledger, never in it — `food_income` is one
    /// side of the pinned larder identity
    /// `larder_delta == food_income − food_consumption − pen_feed_upkeep`, and fodder credits the
    /// band's `FODDER` store without ever touching the larder, so the income here is the hunt's
    /// alone. The hunt row is the control: no animal pays fodder, so its `0.0` is structural.
    #[test]
    fn a_fodder_only_source_publishes_its_fodder_without_moving_food_income() {
        /// The whole product of a sown hay Field — no provisions, no trade, real fodder.
        const HAY_FODDER: f32 = 3.25;
        /// The food the hunt beside it lands: the ONLY contributor to `food_income` in this fixture,
        /// so any fodder that leaked into the larder sum would show up as a mismatch here.
        const HUNT_FOOD: f32 = 0.5;
        /// Float slack on a ledger sum of two published `f32`s.
        const LEDGER_EPSILON: f32 = 1e-5;

        let allocation = LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        floor: 0.5,
                        species: Some("hay_grass".to_string()),
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: 10,
                    kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: "game_1".to_string(),
                        floor: 0.5,
                    },
                    workers: 5,
                    kit: None,
                },
            ],
            build_queue: Vec::new(),
            last_yields: vec![
                SourceYield {
                    fodder: HAY_FODDER,
                    ..SourceYield::ZERO
                },
                SourceYield {
                    actual: HUNT_FOOD,
                    ..SourceYield::ZERO
                },
            ],
            last_pen_feed_upkeep: 0.0,
            last_raid_forfeit: 0.0,
            last_transfer_received: 0.0,
            last_transfer_sent: 0.0,
            upkeep_fund_mode: crate::intensification::UpkeepFundMode::default(),
        };
        let (cohort, allocation) = food_test_cohort(
            Scalar::from_f32(0.0),
            Scalar::from_f32(30.0),
            Scalar::from_f32(0.0),
            allocation,
        );
        let state = capture_food_state(&cohort, &allocation);

        assert!(
            (state.labor_assignments[0].fodder_yield - HAY_FODDER).abs() < LEDGER_EPSILON,
            "the hay row must publish the fodder it was credited: {}",
            state.labor_assignments[0].fodder_yield
        );
        assert_eq!(
            state.labor_assignments[0].actual_yield, 0.0,
            "a hay Field's whole product is fodder — this is the row that read +0.00"
        );
        assert_eq!(
            state.labor_assignments[1].fodder_yield, 0.0,
            "no animal pays fodder, so a hunt row's zero is structural"
        );
        assert!(
            (state.food_income - HUNT_FOOD).abs() < LEDGER_EPSILON,
            "food_income stays Σ actual — fodder never joins the larder sum: {}",
            state.food_income
        );
    }

    /// An allocation with no telemetry yet (empty `last_yields`) reports zero food income and zero
    /// per-row yields
    /// — the default-0.0 branch — while still exporting the assignment rows.
    #[test]
    fn population_state_food_income_defaults_to_zero_without_telemetry() {
        let allocation = LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: 10,
                kit: None,
            }],
            build_queue: Vec::new(),
            last_yields: Vec::new(),
            last_pen_feed_upkeep: 0.0,
            last_raid_forfeit: 0.0,
            last_transfer_received: 0.0,
            last_transfer_sent: 0.0,
            upkeep_fund_mode: crate::intensification::UpkeepFundMode::default(),
        };
        let (cohort, allocation) = food_test_cohort(
            Scalar::from_f32(0.0),
            Scalar::from_f32(30.0),
            Scalar::from_f32(0.0),
            allocation,
        );
        let state = capture_food_state(&cohort, &allocation);
        assert_eq!(state.food_income, 0.0, "no telemetry → zero income");
        assert_eq!(state.labor_assignments.len(), 1);
        assert_eq!(state.labor_assignments[0].actual_yield, 0.0);
        assert_eq!(state.labor_assignments[0].sustainable_yield, 0.0);
        assert_eq!(state.labor_assignments[0].workers_needed, 0);
    }

    /// A `LaborTarget::Forage` **floor** reaches the wire (`docs/plan_harvest_floor.md`): the whole
    /// of what the player decides about pressure rides `LaborAssignmentState::floor`, verbatim. A
    /// floor **no retired stance named** is the interesting case — `0.42` cannot be produced by a
    /// default or by a label round-trip, so a value that landed there really came from the target.
    /// A floor none of the retired stances named, so a value that appears on the wire cannot have
    /// come from a default or a label.
    const UNNAMED_FLOOR: f32 = 0.42;

    #[test]
    fn forage_floor_reaches_the_snapshot_verbatim() {
        let target = LaborTarget::Forage {
            tile: UVec2::new(7, 9),
            floor: UNNAMED_FLOOR,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        };
        let assignment = LaborAssignment {
            target,
            workers: 6,
            kit: None,
        };
        let state = labor_assignment_to_state(
            &assignment,
            &SourceYield::ZERO,
            NOTHING_QUEUED_HERE.to_string(),
            assignment.kit_choice(&crate::equipment_config::EquipmentConfig::builtin()),
            // These fixtures assert on the floor and the build axis, not on the take ceiling.
            crate::fauna::NO_USEFUL_CREW,
        );
        assert_eq!(state.floor, UNNAMED_FLOOR, "the floor crosses verbatim");
        // Only the outbound leg is asserted now. `labor_allocation_from_state` was the decoder,
        // and it existed solely for `restore_world_from_snapshot` — the server never reads labor
        // assignments back off the wire, it reads them from the checkpoint. Keeping a decoder alive
        // for a test to call is the shape this arc removed, so the return leg went with it.
    }

    /// **The pressure and the BUILD reach the wire as two fields** (issue #442): `floor` carries the
    /// stance and `improvement` what is being raised, `""` when nothing is.
    ///
    /// **The row no longer STORES the second one** (`docs/plan_standing_upkeep.md` §2.5) — the queue
    /// is the declaration and the capture resolves it against the ground — so what this pins is that
    /// the two still arrive as two fields and that an empty one is a real answer rather than an
    /// omission.
    #[test]
    fn the_pressure_and_the_build_are_separate_wire_fields() {
        let assignment = LaborAssignment {
            target: LaborTarget::Forage {
                tile: UVec2::new(7, 9),
                floor: 0.15,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: 6,
            kit: None,
        };
        let state = labor_assignment_to_state(
            &assignment,
            &SourceYield::ZERO,
            crate::components::Improvement::Cultivate
                .as_str()
                .to_string(),
            assignment.kit_choice(&crate::equipment_config::EquipmentConfig::builtin()),
            // These fixtures assert on the floor and the build axis, not on the take ceiling.
            crate::fauna::NO_USEFUL_CREW,
        );
        assert_eq!(state.floor, 0.15, "the pressure rides `floor`");
        assert_eq!(
            state.improvement, "cultivate",
            "what is being raised rides its own field — the two axes never share one"
        );

        // A pure harvest says so with an empty string, not by omitting anything.
        let state = labor_assignment_to_state(
            &assignment,
            &SourceYield::ZERO,
            NOTHING_QUEUED_HERE.to_string(),
            assignment.kit_choice(&crate::equipment_config::EquipmentConfig::builtin()),
            // These fixtures assert on the floor and the build axis, not on the take ceiling.
            crate::fauna::NO_USEFUL_CREW,
        );
        assert_eq!(
            state.floor, 0.15,
            "…and the floor is untouched by the build axis"
        );
        assert_eq!(state.improvement, "", "nothing queued here");
    }

    /// **The wire's word for "this band has nothing queued on this source"** — an empty token, which
    /// is a real answer rather than an omission.
    const NOTHING_QUEUED_HERE: &str = "";

    #[test]
    fn power_metrics_from_grid_tracks_totals() {
        let mut grid = PowerGridState {
            total_supply: Scalar::from_f32(12.5),
            total_demand: Scalar::from_f32(10.0),
            total_storage: Scalar::from_f32(4.5),
            total_capacity: Scalar::from_f32(18.0),
            grid_stress_avg: 0.35,
            surplus_margin: 0.22,
            instability_alerts: 3,
            ..Default::default()
        };
        grid.incidents.push(PowerIncident {
            node_id: PowerNodeId(42),
            severity: GridIncidentSeverity::Critical,
            deficit: Scalar::from_f32(1.2),
        });
        grid.incidents.push(PowerIncident {
            node_id: PowerNodeId(99),
            severity: GridIncidentSeverity::Warning,
            deficit: Scalar::from_f32(0.4),
        });

        let telemetry = power_metrics_from_grid(&grid);
        assert_eq!(telemetry.total_supply, Scalar::from_f32(12.5).raw());
        assert_eq!(telemetry.total_demand, Scalar::from_f32(10.0).raw());
        assert_eq!(telemetry.total_storage, Scalar::from_f32(4.5).raw());
        assert_eq!(telemetry.total_capacity, Scalar::from_f32(18.0).raw());
        assert!((telemetry.grid_stress_avg - 0.35).abs() < f32::EPSILON);
        assert!((telemetry.surplus_margin - 0.22).abs() < f32::EPSILON);
        assert_eq!(telemetry.instability_alerts, 3);
        assert_eq!(telemetry.incidents.len(), 2);

        let mut saw_critical = false;
        let mut saw_warning = false;
        for incident in &telemetry.incidents {
            match incident.severity {
                PowerIncidentSeverity::Critical => {
                    saw_critical = true;
                    assert_eq!(incident.node_id, 42);
                    assert_eq!(incident.deficit, Scalar::from_f32(1.2).raw());
                }
                PowerIncidentSeverity::Warning => {
                    saw_warning = true;
                    assert_eq!(incident.node_id, 99);
                    assert_eq!(incident.deficit, Scalar::from_f32(0.4).raw());
                }
            }
        }
        assert!(saw_critical, "expected critical incident serialized");
        assert!(saw_warning, "expected warning incident serialized");
    }

    #[test]
    fn terrain_overlay_delta_updates_on_biome_change() {
        let base_tile = TileState {
            entity: 1,
            x: 0,
            y: 0,
            element: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::FERTILE,
            culture_layer: 0,
            mountain_kind: MountainKind::None,
            mountain_relief: 1.0,
            habitability: 0,
            graze_biomass: 0.0,
            graze_capacity: 0.0,
            graze_ecology_phase: GRAZE_PHASE_NONE,
            forage_capacity: 0.0,
            underlying_terrain: TerrainType::AlluvialPlain,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        };
        let base_overlay = TerrainOverlayState {
            width: 1,
            height: 1,
            samples: vec![TerrainSample {
                terrain: base_tile.terrain,
                tags: base_tile.terrain_tags,
                mountain_kind: base_tile.mountain_kind,
                relief_scale: base_tile.mountain_relief,
            }],
        };
        let base_snapshot = snapshot_with_overlay(1, base_tile.clone(), base_overlay);

        let mut history = SnapshotHistory::default();
        history.update(base_snapshot);

        let updated_tile = TileState {
            terrain: TerrainType::MangroveSwamp,
            terrain_tags: TerrainTags::COASTAL | TerrainTags::WETLAND,
            ..base_tile
        };
        let updated_overlay = TerrainOverlayState {
            width: 1,
            height: 1,
            samples: vec![TerrainSample {
                terrain: updated_tile.terrain,
                tags: updated_tile.terrain_tags,
                mountain_kind: updated_tile.mountain_kind,
                relief_scale: updated_tile.mountain_relief,
            }],
        };
        let updated_snapshot =
            snapshot_with_overlay(2, updated_tile.clone(), updated_overlay.clone());

        history.update(updated_snapshot);

        let delta = history
            .last_delta()
            .expect("delta captured after terrain change");
        let terrain_delta = delta
            .terrain
            .as_ref()
            .expect("terrain overlay delta emitted");

        assert_eq!(terrain_delta, &updated_overlay);
        assert_eq!(terrain_delta.samples.len(), 1);
        let sample = &terrain_delta.samples[0];
        assert_eq!(sample.terrain, updated_tile.terrain);
        assert_eq!(sample.tags, updated_tile.terrain_tags);

        let latest_snapshot = history.last_snapshot().expect("latest snapshot retained");
        assert_eq!(latest_snapshot.terrain, updated_overlay);
    }

    #[test]
    fn snapshot_history_records_power_metrics_delta() {
        let mut history = SnapshotHistory::default();

        let baseline = snapshot_with_power_metrics(1, PowerTelemetryState::default());
        history.update(baseline);

        let updated_metrics = PowerTelemetryState {
            total_supply: Scalar::from_f32(20.0).raw(),
            total_demand: Scalar::from_f32(15.0).raw(),
            total_storage: Scalar::from_f32(5.0).raw(),
            total_capacity: Scalar::from_f32(25.0).raw(),
            grid_stress_avg: 0.42,
            surplus_margin: -0.1,
            instability_alerts: 4,
            incidents: vec![
                PowerIncidentState {
                    node_id: 7,
                    severity: PowerIncidentSeverity::Critical,
                    deficit: Scalar::from_f32(2.3).raw(),
                },
                PowerIncidentState {
                    node_id: 11,
                    severity: PowerIncidentSeverity::Warning,
                    deficit: Scalar::from_f32(0.8).raw(),
                },
            ],
        };
        let updated_snapshot = snapshot_with_power_metrics(2, updated_metrics.clone());
        history.update(updated_snapshot);

        let delta = history
            .last_delta()
            .expect("delta captured after power metrics change");
        let power_delta = delta
            .power_metrics
            .as_ref()
            .expect("power metrics delta emitted");

        assert_eq!(
            power_delta.instability_alerts,
            updated_metrics.instability_alerts
        );
        assert_eq!(power_delta.incidents.len(), updated_metrics.incidents.len());
        assert!(
            (power_delta.grid_stress_avg - updated_metrics.grid_stress_avg).abs() < f32::EPSILON
        );
        assert!((power_delta.surplus_margin - updated_metrics.surplus_margin).abs() < f32::EPSILON);

        let latest_snapshot = history.last_snapshot().expect("latest snapshot retained");
        assert_eq!(latest_snapshot.power_metrics, updated_metrics);
    }

    #[test]
    fn great_discovery_snapshot_delta_tracks_changes() {
        let mut history = SnapshotHistory::default();

        let baseline = snapshot_with_discoveries(
            1,
            Vec::new(),
            Vec::new(),
            GreatDiscoveryTelemetryState::default(),
        );
        history.update(baseline);

        let discovery = GreatDiscoveryState {
            id: 7,
            faction: 3,
            field: KnowledgeField::Physics,
            tick: 2,
            publicly_deployed: true,
            effect_flags: 0b0101,
        };
        let progress = GreatDiscoveryProgressState {
            faction: 3,
            discovery: 7,
            progress: 500_000,
            observation_deficit: 2,
            eta_ticks: 4,
            covert: false,
        };
        let telemetry = GreatDiscoveryTelemetryState {
            total_resolved: 1,
            pending_candidates: 2,
            active_constellations: 1,
        };

        let updated = snapshot_with_discoveries(
            2,
            vec![discovery.clone()],
            vec![progress.clone()],
            telemetry.clone(),
        );
        history.update(updated);

        let delta = history
            .last_delta()
            .expect("delta captured after great discovery changes");

        assert_eq!(delta.great_discoveries, vec![discovery.clone()]);
        assert_eq!(delta.great_discovery_progress, vec![progress.clone()]);
        assert_eq!(delta.great_discovery_telemetry.as_ref(), Some(&telemetry));

        let latest = history.last_snapshot().expect("latest snapshot stored");
        assert_eq!(latest.great_discoveries, vec![discovery]);
        assert_eq!(latest.great_discovery_progress, vec![progress]);
        assert_eq!(latest.great_discovery_telemetry, telemetry);
    }

    /// The logistics and trade subsystems have no spatial weights any more (their networks were
    /// demolished — `docs/plan_contact_and_logistics.md` §As-built), so their intensity spreads
    /// FLAT. The tile ordering asserted here therefore comes from the two subsystems that still
    /// have a spatial signal — population (governance) and power (military) — which is exactly what
    /// the baseline pass is for.
    #[test]
    fn corruption_raster_allocates_intensity_and_baseline() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];

        let populations = vec![
            PopulationCohortState {
                entity: 100,
                home: 1,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 120,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                turns_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.3).raw(),
                generation: 0,
                faction: 0,
                knowledge_fragments: Vec::new(),
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
            PopulationCohortState {
                entity: 101,
                home: 2,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 80,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                turns_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.8).raw(),
                generation: 0,
                faction: 1,
                knowledge_fragments: Vec::new(),
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
        ];

        let power_nodes = vec![
            PowerNodeState {
                entity: 1,
                node_id: 1,
                generation: Scalar::from_f32(0.9).raw(),
                demand: Scalar::from_f32(0.4).raw(),
                efficiency: Scalar::one().raw(),
                storage_level: Scalar::zero().raw(),
                storage_capacity: Scalar::zero().raw(),
                stability: Scalar::one().raw(),
                surplus: Scalar::zero().raw(),
                deficit: Scalar::zero().raw(),
                incident_count: 0,
            },
            PowerNodeState {
                entity: 2,
                node_id: 2,
                generation: Scalar::from_f32(0.4).raw(),
                demand: Scalar::from_f32(0.2).raw(),
                efficiency: Scalar::one().raw(),
                storage_level: Scalar::zero().raw(),
                storage_capacity: Scalar::zero().raw(),
                stability: Scalar::one().raw(),
                surplus: Scalar::zero().raw(),
                deficit: Scalar::zero().raw(),
                incident_count: 0,
            },
        ];

        let mut ledger = CorruptionLedger::default();
        ledger.entries.push(CorruptionEntry {
            subsystem: CorruptionSubsystem::Logistics,
            intensity: Scalar::from_f32(0.6).raw(),
            ..CorruptionEntry::default()
        });
        ledger.entries.push(CorruptionEntry {
            subsystem: CorruptionSubsystem::Trade,
            intensity: Scalar::from_f32(0.3).raw(),
            ..CorruptionEntry::default()
        });

        let telemetry = CorruptionTelemetry::default();

        let overlays_config = SnapshotOverlaysConfig::default();
        let raster = corruption_raster_from_simulation(CorruptionRasterInputs {
            tiles: &tiles,
            populations: &populations,
            power_nodes: &power_nodes,
            corruption_signals: CorruptionSignals {
                ledger: &ledger,
                telemetry: &telemetry,
            },
            grid_size: UVec2::new(2, 1),
            overlays: &overlays_config,
        });

        assert_eq!(raster.width, 2);
        assert_eq!(raster.height, 1);
        assert_eq!(raster.samples.len(), 2);
        assert!(raster.samples[0] > 0);
        assert!(raster.samples[1] > 0);
        assert!(raster.samples[0] > raster.samples[1]);
    }

    /// A published band row, built the way `population_state` builds one: the fractional brackets
    /// resolved into whole people **once**, through the shared derivation. Building it any other way
    /// would be testing a rounding the capture does not do.
    fn demographics_cohort(
        faction: u32,
        size: u32,
        children: f32,
        working: f32,
        elders: f32,
    ) -> PopulationCohortState {
        let brackets = whole_age_brackets(
            size,
            available_workers(Scalar::from_f32(working)),
            i128::from(Scalar::from_f32(children).raw()),
            i128::from(Scalar::from_f32(elders).raw()),
        );
        PopulationCohortState {
            faction,
            size: brackets.head_count(),
            children: Scalar::from_f32(children).raw(),
            working: Scalar::from_f32(working).raw(),
            elders: Scalar::from_f32(elders).raw(),
            children_count: brackets.children,
            working_age: brackets.working,
            elders_count: brackets.elders,
            ..Default::default()
        }
    }

    #[test]
    fn snapshot_demographics_reconciles_with_band_totals() {
        // Independent rounding of 8.9/16.5/4.6 would overshoot to 9+17+5 = 31, but the band's
        // authoritative size is 30 and available_workers floors 16.5 to 16.
        let cohorts = vec![demographics_cohort(0, 30, 8.9, 16.5, 4.6)];
        let demographics = snapshot_demographics(&cohorts);
        assert_eq!(demographics.len(), 1);
        let d = &demographics[0];
        assert_eq!(d.faction, 0);
        assert_eq!(d.working, 16, "working matches Σ available_workers (floor)");
        assert_eq!(
            d.children + d.working + d.elders,
            30,
            "brackets sum to Σ size (client Pop matches band size)"
        );
        // Dependents 14 split ∝ 8.9:4.6 → children round(9.23)=9, elders remainder 5.
        assert_eq!(d.children, 9);
        assert_eq!(d.elders, 5);
    }

    #[test]
    fn snapshot_demographics_sums_multiple_bands_per_faction() {
        let cohorts = vec![
            demographics_cohort(2, 30, 8.9, 16.5, 4.6),
            demographics_cohort(2, 20, 5.4, 10.5, 4.1),
            // A different faction stays separate.
            demographics_cohort(7, 10, 2.0, 6.5, 1.5),
        ];
        let demographics = snapshot_demographics(&cohorts);
        assert_eq!(demographics.len(), 2);

        let f2 = demographics.iter().find(|d| d.faction == 2).unwrap();
        // Σ available_workers = floor(16.5) + floor(10.5) = 16 + 10 = 26.
        assert_eq!(f2.working, 26);
        // Σ size = 50.
        assert_eq!(f2.children + f2.working + f2.elders, 50);

        let f7 = demographics.iter().find(|d| d.faction == 7).unwrap();
        assert_eq!(f7.working, 6);
        assert_eq!(f7.children + f7.working + f7.elders, 10);
    }

    /// **The faction page is the sum of the bands it aggregates, per bracket** — not merely on the
    /// total. One derivation per band and a plain sum over them is what makes this true by
    /// construction; re-deriving the faction figures from the summed masses is what let the two
    /// disagree.
    #[test]
    fn snapshot_demographics_equals_the_sum_of_the_published_bands() {
        let cohorts = vec![
            demographics_cohort(3, 30, 8.9, 16.5, 4.6),
            demographics_cohort(3, 20, 5.4, 10.5, 4.1),
            demographics_cohort(3, 17, 0.0, 16.6, 0.0),
        ];
        let d = &snapshot_demographics(&cohorts)[0];
        let sum = |pick: fn(&PopulationCohortState) -> u32| cohorts.iter().map(pick).sum::<u32>();
        assert_eq!(d.children, sum(|c| c.children_count));
        assert_eq!(d.working, sum(|c| c.working_age));
        assert_eq!(d.elders, sum(|c| c.elders_count));
        assert_eq!(d.children + d.working + d.elders, sum(|c| c.size));
    }

    #[test]
    fn snapshot_demographics_clamps_workers_above_headcount() {
        // Degenerate: floored workers exceed size — dependents must clamp to zero, not underflow.
        let cohorts = vec![demographics_cohort(1, 5, 0.0, 9.9, 0.0)];
        let demographics = snapshot_demographics(&cohorts);
        let d = &demographics[0];
        assert_eq!(d.working, 5);
        assert_eq!(d.children, 0);
        assert_eq!(d.elders, 0);
    }

    #[test]
    fn snapshot_forage_patches_reports_cultivation_and_owner() {
        let mut registry = ForageRegistry::default();
        // A wild, untended patch: no cultivation, no owner.
        let wild = ForagePatch::new(UVec2::new(1, 0), 100.0);
        // A tended (cultivated) patch owned by faction 3.
        let mut tended = ForagePatch::new(UVec2::new(0, 1), 100.0);
        tended.complete_cultivation(
            FactionId(0),
            &crate::intensification::LadderConfig::builtin(),
        );
        tended.owner = Some(FactionId(3));
        registry.patches.insert(wild.tile, wild);
        registry.patches.insert(tended.tile, tended);

        let labor = LaborConfig::builtin();
        let patches = snapshot_forage_patches(
            &registry,
            &labor.forage,
            crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
                crate::equipment_config::EquipmentStat::ForageCarry,
                labor.forage.per_worker_biomass_capacity,
            ),
            &FloraConfig::builtin(),
            &LadderConfig::builtin(),
            &HashMap::new(),
            &HashMap::new(),
            // No tiles behind these fixture patches, so no ground is named here either: the row
            // presents no tender-load and its upkeep figures read the honest zero, the same
            // absent-means-nothing reading the composition takes below.
            &HashMap::new(),
            // No tiles behind these fixture patches, so the quote memo was never swept over them and
            // no composition is published — "unknown ground names no plants", never a fabricated
            // basket.
            &FloraQuoteCache::default(),
            // Nothing is queued in this fixture, so no patch names a builders kit.
            &crate::snapshot::subsistence::BuildKitIds::default(),
        );
        assert_eq!(patches.len(), 2);
        // Emitted in stable (y, x) order: (1,0) then (0,1).
        assert_eq!((patches[0].x, patches[0].y), (1, 0));
        assert_eq!((patches[1].x, patches[1].y), (0, 1));

        let w = &patches[0];
        assert!(!w.is_cultivated);
        assert_eq!(w.cultivation_progress, 0.0);
        assert_eq!(w.owner, None);

        let t = &patches[1];
        assert!(t.is_cultivated);
        assert!((t.cultivation_progress - 1.0).abs() < 1e-6);
        assert_eq!(t.owner, Some(3));
    }

    /// **EVERY PATCH PUBLISHES THE RUNG IT STANDS ON, all three of them.** The wire's `currentRung`
    /// is what lets a client answer *"has this been improved, and what is above it"* without reading
    /// `isCultivated`/`isField` in the right order, so the claim worth pinning is that each of the
    /// plant branch's three positions crosses as its own token — a bottom rung included, since `""`
    /// is not one of the three.
    ///
    /// **Asserted against `RungKey::…::wire_key()`, never a typed `"plant:tended"`.** A literal here
    /// would keep passing if the `<branch>:<id>` spelling moved under it, which is the one change
    /// that breaks every client parsing the token.
    #[test]
    fn snapshot_forage_patches_publish_the_rung_each_patch_stands_on() {
        let ladder = LadderConfig::builtin();
        let mut registry = ForageRegistry::default();
        // One patch per rung, laid along `y = 0` so the capture's stable `(y, x)` order is `x`.
        let wild = ForagePatch::new(UVec2::new(0, 0), 100.0);
        let mut tended = ForagePatch::new(UVec2::new(1, 0), 100.0);
        assert!(tended.complete_cultivation(FactionId(1), &ladder));
        let mut field = ForagePatch::new(UVec2::new(2, 0), 100.0);
        assert!(field.complete_field(FactionId(1), &ladder));
        for patch in [wild, tended, field] {
            registry.patches.insert(patch.tile, patch);
        }

        let labor = LaborConfig::builtin();
        let patches = snapshot_forage_patches(
            &registry,
            &labor.forage,
            crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
                crate::equipment_config::EquipmentStat::ForageCarry,
                labor.forage.per_worker_biomass_capacity,
            ),
            &FloraConfig::builtin(),
            &ladder,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &FloraQuoteCache::default(),
            &crate::snapshot::subsistence::BuildKitIds::default(),
        );

        let published: Vec<&str> = patches
            .iter()
            .map(|patch| patch.current_rung.as_str())
            .collect();
        assert_eq!(
            published,
            vec![
                RungKey::PlantWild.wire_key(),
                RungKey::PlantTended.wire_key(),
                RungKey::PlantField.wire_key(),
            ],
            "each patch must publish its OWN rung, bottom rung included"
        );
    }

    #[test]
    fn snapshot_intensification_knowledge_reports_learned_ladders() {
        let mut ledger = DiscoveryProgressLedger::default();
        // Faction 2 fully knows Cultivation and is partway to Herding.
        ledger.add_progress(FactionId(2), CULTIVATION_DISCOVERY_ID, Scalar::one());
        ledger.add_progress(FactionId(2), HERDING_DISCOVERY_ID, Scalar::from_f32(0.5));
        // Faction 5 has only unrelated discovery progress → no intensification row.
        ledger.add_progress(FactionId(5), 1, Scalar::one());

        let rows = snapshot_intensification_knowledge(&ledger);
        assert_eq!(rows.len(), 1, "only factions on the ladders appear");
        let f2 = &rows[0];
        assert_eq!(f2.faction, 2);
        assert!((f2.cultivation - 1.0).abs() < 1e-6);
        assert!((f2.herding - 0.5).abs() < 1e-6);
        assert_eq!(
            f2.foddering, 0.0,
            "a faction that never ran a pen has no fodder capability"
        );
    }

    /// **Foddering is published, and it alone keeps a faction's row alive.** It is not a rung gate,
    /// so nothing else in the row implies it — a faction whose only non-zero track is Foddering must
    /// still be emitted, or the client cannot tell "cannot bank this hay" from "no row at all".
    #[test]
    fn snapshot_intensification_knowledge_reports_foddering_on_its_own() {
        /// Partway to Foddering — a learning meter, so the row must carry the fraction rather than a
        /// known/unknown bit.
        const PARTIAL_FODDERING: f32 = 0.25;

        let mut ledger = DiscoveryProgressLedger::default();
        ledger.add_progress(FactionId(7), PENNING_DISCOVERY_ID, Scalar::one());
        ledger.add_progress(FactionId(7), FODDERING_DISCOVERY_ID, Scalar::one());
        // Faction 9 has NOTHING but Foddering progress — the all-zero skip must not drop it.
        ledger.add_progress(
            FactionId(9),
            FODDERING_DISCOVERY_ID,
            Scalar::from_f32(PARTIAL_FODDERING),
        );

        let rows = snapshot_intensification_knowledge(&ledger);
        assert_eq!(rows.len(), 2, "both factions are on the ladder");
        let penned = &rows[0];
        assert_eq!(penned.faction, 7);
        assert!((penned.foddering - 1.0).abs() < 1e-6);
        let fodder_only = &rows[1];
        assert_eq!(fodder_only.faction, 9);
        assert!((fodder_only.foddering - PARTIAL_FODDERING).abs() < 1e-6);
        assert_eq!(fodder_only.penning, 0.0);
    }

    #[test]
    fn herd_snapshot_reports_corralled_state() {
        use crate::fauna_config::SizeClass;
        let mut registry = HerdRegistry::default();
        let mut penned = Herd::new(
            "herd_pen".to_string(),
            "Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            50.0,
            100.0,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        );
        assert!(
            penned.corral_at(
                UVec2::new(4, 4),
                &crate::intensification::LadderConfig::builtin()
            ),
            "the fixture species must be pennable"
        );
        registry.herds.push(penned);
        // A second, un-penned herd stays mobile (corralled = false).
        registry.herds.push(Herd::new(
            "herd_wild".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            50.0,
            100.0,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );
        let pen = states.iter().find(|h| h.id == "herd_pen").unwrap();
        assert!(pen.corralled, "a penned herd reports corralled");
        let wild = states.iter().find(|h| h.id == "herd_wild").unwrap();
        assert!(!wild.corralled, "a mobile herd reports not corralled");
    }

    /// **THE ANIMAL TWIN of `snapshot_forage_patches_publish_the_rung_each_patch_stands_on`** — all
    /// three husbandry positions cross as their own token, so a client never has to read
    /// `domestication` against a threshold and then `corralled` to work out where a herd stands.
    ///
    /// Same rule on the assertion: compared against `RungKey::…::wire_key()`, because a hand-typed
    /// `"animal:pastoral"` would survive exactly the spelling change that breaks a client.
    #[test]
    fn herd_snapshot_publishes_the_rung_each_herd_stands_on() {
        use crate::fauna_config::SizeClass;
        let ladder = LadderConfig::builtin();
        // One species for all three, so the only thing differing between the rows is the position:
        // `Aurochs` is pennable, which is what lets one fixture reach the top of the branch.
        let herd = |id: &str| {
            Herd::new(
                id.to_string(),
                "Aurochs".to_string(),
                SizeClass::Big,
                vec![UVec2::new(4, 4)],
                50.0,
                100.0,
                0.0,
                0.05,
                SNAPSHOT_BODY_MASS,
            )
        };
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd("herd_wild"));
        // Rung 2 and rung 3 are both reached through the REAL mutators, so the husbandry ceiling and
        // the owner lock apply exactly as they do in play — no fabricated meter.
        let mut tamed = herd("herd_tamed");
        assert!(
            tamed.tame_outright(FactionId(1), &ladder),
            "the fixture species must be tameable"
        );
        registry.herds.push(tamed);
        let mut penned = herd("herd_penned");
        assert!(
            penned.corral_at(UVec2::new(4, 4), &ladder),
            "the fixture species must be pennable"
        );
        registry.herds.push(penned);

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );
        for (id, expected) in [
            ("herd_wild", RungKey::AnimalWild),
            ("herd_tamed", RungKey::AnimalPastoral),
            ("herd_penned", RungKey::AnimalPen),
        ] {
            let row = states.iter().find(|h| h.id == id).unwrap();
            assert_eq!(
                row.current_rung,
                expected.wire_key(),
                "{id} must publish its OWN rung, bottom rung included"
            );
        }
    }

    /// **The ownership-INDEPENDENT would-be herder count** (taming-startup-lag fix). A WILD (unowned)
    /// tameable herd exports `herders_needed_if_managed > 0` even though the ownership-gated
    /// `herders_needed` wire field reads 0 — so the client can floor the Tame-compose worker cap up
    /// front, before ownership is set in the Population stage. A `wild`-ceiling species exports 0 (it can
    /// never be tamed); a herd already managed exports the same value in both.
    #[test]
    fn herd_snapshot_reports_the_would_be_herder_count_ownership_independently() {
        use crate::fauna_config::{HusbandryCeiling, SizeClass};
        let herd = |id: &str| {
            Herd::new(
                id.to_string(),
                "Rabbit Warren".to_string(),
                SizeClass::Small,
                vec![UVec2::new(2, 2)],
                50.0, // biomass
                100.0,
                0.0,
                0.05,
                SNAPSHOT_BODY_MASS,
            )
        };
        let mut registry = HerdRegistry::default();
        // (1) A wild, UNOWNED, tameable herd (default `pen` ceiling, no owner).
        registry.herds.push(herd("herd_wild_tameable"));
        // (2) A wild-CEILING species (never tames).
        let mut untameable = herd("herd_untameable");
        untameable.husbandry_ceiling = HusbandryCeiling::Wild;
        registry.herds.push(untameable);
        // (3) A herd already managed (owned).
        // **Tamed through the real accrual**, not by setting `owner` alone: with one position per
        // herd, ownership is not a rung — a herd nobody has banked work into stands on `animal:wild`
        // and owes nothing, which is a state the sim cannot otherwise reach (the first accrual is
        // what records the owner).
        let mut managed = herd("herd_managed");
        assert!(managed.tame_outright(
            FactionId(0),
            &crate::intensification::LadderConfig::builtin()
        ));
        registry.herds.push(managed);

        let states = export_with(&registry, &all_seeing_ledger(64));
        let at = |id: &str| states.iter().find(|h| h.id == id).unwrap();

        // (1) wild + tameable: the would-be crew is real, the ownership-gated field is still 0.
        let wild = at("herd_wild_tameable");
        assert!(
            wild.herders_needed_if_managed > 0,
            "a tameable herd advertises the crew it WOULD need: {}",
            wild.herders_needed_if_managed
        );
        assert_eq!(
            wild.herders_needed, 0,
            "…while the ownership-gated field is still 0 for an unowned herd"
        );

        // (2) wild-ceiling: never tameable → 0.
        assert_eq!(
            at("herd_untameable").herders_needed_if_managed,
            0,
            "a wild-ceiling species can never be tamed, so it advertises no crew"
        );

        // (3) already managed: both agree.
        let managed = at("herd_managed");
        assert!(managed.herders_needed_if_managed > 0);
        assert_eq!(
            managed.herders_needed_if_managed, managed.herders_needed,
            "a managed herd's would-be count equals its live ownership-gated count"
        );
    }

    /// **⛔ THE ROW MUST AGREE WITH ITSELF ALL THE WAY UP A RUNG, ON BOTH WEBS** — the wire states
    /// `upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT)` on the herd row and on the
    /// patch row alike, and tells the client to do no arithmetic of its own
    /// (`sim_schema/schemas/snapshot.fbs`).
    ///
    /// **IT IS SWEPT, NOT SPOT-CHECKED, AND THAT IS THE POINT.** The demand *interpolates* on the
    /// source's ladder position on both webs, so the two terms agreed at the ends of the rung and
    /// came apart across the middle: the herd row published the bill through `herd_keeping_basis`
    /// (interpolated) and the crew through `herd_herders_needed` (the rung's **bare** rate), so a herd
    /// a tenth of the way up a `Tame` was billed `0.185` work and told to staff **two** keepers — and
    /// the player staffs two, because the panel says so. Checking `0.0` and `1.0` alone sees none of
    /// it. One guard covers both webs so a fix to one cannot regress the other.
    ///
    /// The two preconditions below are what stop it passing vacuously: a fixture whose bill happens
    /// to be a whole number at every sample satisfies the identity for free, and one whose head-count
    /// crew never differs from its bill's crew cannot tell the defect from the fix.
    #[test]
    fn the_published_upkeep_crew_is_the_ceil_of_the_published_bill_all_the_way_up_a_rung() {
        use crate::intensification::PER_WORKER_OUTPUT;

        /// **The sweep** — fractions of the rung being climbed, dense enough to land inside every
        /// whole-hand band a 1.5-load source crosses. It starts *above* zero deliberately: a source
        /// at `RUNG_UNSTARTED` stands on no keeping rung at all and both terms are honestly `0`,
        /// which is the one place the old reading was accidentally right.
        const UP_THE_RUNG: [f32; 11] = [0.05, 0.1, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.75, 0.9, 1.0];

        /// **How much keeping the source owes at the TOP of its rung, in worker-loads.** Chosen at
        /// `1.5` because it makes the two questions visibly different numbers: the head-count crew is
        /// `ceil(1.5) = 2` for the whole climb, while the interpolated bill only passes `1.0` past
        /// two-thirds of the way up — so most of the sweep sits where the defect lived, and the
        /// fractional-bill precondition is met by construction rather than by luck.
        const LOADS_AT_THE_TOP: f32 = 1.5;

        /// A tameable roster species with a small `animals_per_herder`, so the fixture's head count
        /// stays a plausible flock rather than an artifact.
        const SWEPT_SPECIES: &str = "Wild Aurochs";

        /// Slack on a published `f32` when asking whether a sample's bill is genuinely *fractional*.
        /// It is a tolerance on the **precondition** only — the identity itself is asserted exactly.
        const WHOLE_NUMBER_SLACK: f32 = 1e-4;

        /// Does `demand` sit strictly between two whole hands? The sweep is worthless without it.
        fn is_fractional(demand: f32) -> bool {
            (demand - demand.round()).abs() > WHOLE_NUMBER_SLACK
        }

        let ladder = LadderConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let labor = LaborConfig::builtin();

        // ---- THE ANIMAL WEB -----------------------------------------------------------------
        // A herd sized so its keeper load at the top of the pastoral rung is `LOADS_AT_THE_TOP`,
        // derived from the config rather than stated, so a roster retune moves the fixture with it.
        let aurochs_per_herder = fauna.animals_per_herder_for(SWEPT_SPECIES);
        let herd_biomass = LOADS_AT_THE_TOP * aurochs_per_herder * SNAPSHOT_BODY_MASS;
        let mut herd_crews: Vec<u32> = Vec::new();
        let mut herd_head_counts: Vec<u32> = Vec::new();
        let mut a_herd_bill_was_fractional = false;
        for fraction in UP_THE_RUNG {
            let mut herd = Herd::new(
                "herd_swept".to_string(),
                SWEPT_SPECIES.to_string(),
                crate::fauna_config::SizeClass::Big,
                vec![UVec2::new(2, 2)],
                herd_biomass,
                herd_biomass * 2.0,
                0.0,
                0.05,
                SNAPSHOT_BODY_MASS,
            );
            // **Tamed through the real accrual, then walked back down the rung** — ownership is
            // recorded by the accrual and `set_ladder_position` deliberately does not reconcile it,
            // so this is a genuinely *owned* herd part-way up its `Tame`, which is the state the
            // defect lived in.
            assert!(
                herd.tame_outright(VIEWER, &ladder),
                "the swept species must be tameable, or this fixture stands on no keeping rung"
            );
            let rung = herd.rung_cost(RungKey::AnimalPastoral, &ladder);
            herd.set_ladder_position(rung * fraction, &ladder);

            let mut registry = HerdRegistry::default();
            registry.herds.push(herd);
            let states = export_with(&registry, &all_seeing_ledger(64));
            let row = states.iter().find(|h| h.id == "herd_swept").unwrap();

            assert_eq!(
                row.upkeep_workers_needed,
                (row.upkeep_demand / PER_WORKER_OUTPUT).ceil() as u32,
                "HERD ROW at {fraction} up the pastoral rung: the wire promises \
                 upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT), and the row states \
                 a bill of {} against a crew of {}",
                row.upkeep_demand,
                row.upkeep_workers_needed
            );
            a_herd_bill_was_fractional |= is_fractional(row.upkeep_demand);
            herd_crews.push(row.upkeep_workers_needed);
            herd_head_counts.push(row.herders_needed);
        }
        assert!(
            a_herd_bill_was_fractional,
            "PRECONDITION: no sample billed the herd a fractional number of hands, so the identity \
             held for free — resize the fixture, this proves nothing: {herd_crews:?}"
        );

        // **`herdersNeeded` IS THE OTHER QUESTION, AND IT MUST NOT FOLLOW THE BILL.** It is the
        // head-count requirement — how many keepers a flock of this species and this size wants —
        // which is why `herdersNeededIfManaged` can quote it before a herd has any position at all
        // and why the hysteresis has one thing to damp. Interpolating it "so the row agrees" is the
        // fix this pins against.
        assert!(
            herd_head_counts.windows(2).all(|pair| pair[0] == pair[1]),
            "herdersNeeded must not slide with the ladder position — it is the head-count \
             requirement, not the bill's crew: {herd_head_counts:?}"
        );
        assert!(
            herd_crews
                .iter()
                .zip(&herd_head_counts)
                .any(|(bill, heads)| bill != heads),
            "PRECONDITION: the fixture's bill-crew and head-count never differ, so this sweep \
             cannot tell the two questions apart — bills {herd_crews:?} against heads \
             {herd_head_counts:?}"
        );

        // ---- THE PLANT WEB, THROUGH THE SAME GUARD ------------------------------------------
        // Already correct before this fix; here so a repair to one web cannot regress the other.
        let tile = UVec2::new(0, 3);
        let tile_capacity = LOADS_AT_THE_TOP * labor.forage.cultivation.capacity_per_tender;
        let tile_capacities = HashMap::from([(tile, tile_capacity)]);
        let tended_cost = ladder
            .rung(RungKey::PlantTended)
            .build_cost(crate::intensification::RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        let mut a_patch_bill_was_fractional = false;
        for fraction in UP_THE_RUNG {
            let mut patch = ForagePatch::new(tile, tile_capacity);
            patch.set_ladder_position(tended_cost * fraction, &ladder);
            let mut registry = ForageRegistry::default();
            registry.patches.insert(patch.tile, patch);

            let rows = snapshot_forage_patches(
                &registry,
                &labor.forage,
                crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
                    crate::equipment_config::EquipmentStat::ForageCarry,
                    labor.forage.per_worker_biomass_capacity,
                ),
                &FloraConfig::builtin(),
                &ladder,
                &HashMap::new(),
                &HashMap::new(),
                &tile_capacities,
                &FloraQuoteCache::default(),
                &crate::snapshot::subsistence::BuildKitIds::default(),
            );
            let row = &rows[0];
            assert_eq!(
                row.upkeep_workers_needed,
                (row.upkeep_demand / PER_WORKER_OUTPUT).ceil() as u32,
                "PATCH ROW at {fraction} up the tended rung: the wire promises the same identity, \
                 and the row states a bill of {} against a crew of {}",
                row.upkeep_demand,
                row.upkeep_workers_needed
            );
            a_patch_bill_was_fractional |= is_fractional(row.upkeep_demand);
        }
        assert!(
            a_patch_bill_was_fractional,
            "PRECONDITION: no sample billed the patch a fractional number of hands, so the plant \
             half of this guard held for free"
        );
    }

    /// A ledger in which [`VIEWER`] sees exactly `visible` and nothing else — the fog fixtures below.
    fn ledger_seeing(size: u32, visible: &[UVec2]) -> crate::visibility::VisibilityLedger {
        let mut ledger = crate::visibility::VisibilityLedger::default();
        let map = ledger.ensure_faction(VIEWER, size, size);
        for pos in visible {
            map.mark_active(pos.x, pos.y, 0);
        }
        ledger
    }

    /// One stationary wild herd standing at `pos`.
    fn herd_at(id: &str, pos: UVec2) -> Herd {
        use crate::fauna_config::SizeClass;
        Herd::new(
            id.to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![pos],
            50.0,
            100.0,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        )
    }

    fn export_with(
        registry: &HerdRegistry,
        ledger: &crate::visibility::VisibilityLedger,
    ) -> Vec<HerdTelemetryState> {
        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        export_herds(
            &telemetry,
            registry,
            &FaunaConfig::builtin(),
            &LaborConfig::builtin(),
            ledger,
        )
    }

    /// **The fog leak (#264).** Herd telemetry used to be exported unfiltered, so the wire handed the
    /// client every herd on the map and fog was decorative for fauna. A herd is now exported only if
    /// the viewer can see the tile it is standing on *this turn*.
    #[test]
    fn a_herd_on_unseen_ground_never_reaches_the_client() {
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd_at("herd_seen", UVec2::new(4, 4)));
        registry
            .herds
            .push(herd_at("herd_hidden", UVec2::new(40, 40)));

        let states = export_with(&registry, &ledger_seeing(64, &[UVec2::new(4, 4)]));

        assert_eq!(
            states.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            vec!["herd_seen"],
            "only the herd on visible ground crosses the wire"
        );
        assert_eq!(
            registry.herds.len(),
            2,
            "the authoritative registry is untouched — only the DISPLAY list is filtered"
        );
    }

    /// **Ownership is not a leak.** Your own tamed/penned animals are yours to track; hiding them
    /// would take a starving pen's warning off the panel because the herd drifted a hex out of sight.
    #[test]
    fn a_herd_the_viewer_owns_is_exported_even_in_the_dark() {
        let mut registry = HerdRegistry::default();
        let mut mine = herd_at("herd_mine", UVec2::new(40, 40));
        // The real accrual path, so the fixture cannot fabricate an ownership the sim would refuse.
        mine.tame_outright(VIEWER, &crate::intensification::LadderConfig::builtin());
        assert_eq!(
            mine.owner,
            Some(VIEWER),
            "the fixture species must be tameable"
        );
        registry.herds.push(mine);
        registry
            .herds
            .push(herd_at("herd_theirs", UVec2::new(41, 41)));

        // The viewer sees NOTHING — both herds stand in the dark.
        let states = export_with(&registry, &ledger_seeing(64, &[]));

        assert_eq!(
            states.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            vec!["herd_mine"],
            "the viewer's own herd is exported; an unowned herd in the dark is not"
        );
    }

    /// **Fails closed.** Before the first `calculate_visibility` — and on the turn after a rollback
    /// clears the ledger — the viewer has no faction map at all. `visibility_raster_from_ledger`
    /// answers that state with an all-unexplored (black) raster, so the herd list must answer it the
    /// same way rather than dumping the map onto a client that is rendering darkness.
    #[test]
    fn a_ledger_with_no_map_for_the_viewer_exports_no_herds() {
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd_at("herd_a", UVec2::new(4, 4)));
        registry.herds.push(herd_at("herd_b", UVec2::new(9, 9)));

        let states = export_with(&registry, &crate::visibility::VisibilityLedger::default());

        assert!(
            states.is_empty(),
            "an empty ledger hides every herd, matching the all-unexplored raster"
        );
    }

    /// **The fog switch is the server's, and it reaches the payload.** Fog of war is one
    /// server-owned setting (`SimulationConfig::fog_enabled`); with it off, the filter above stops
    /// running and every herd crosses. This is the ONLY place the reveal can happen — a client-side
    /// render flag cannot restore herds the sim already dropped from the wire.
    #[test]
    fn disabling_fog_exports_every_herd_including_unseen_unowned_ones() {
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd_at("herd_seen", UVec2::new(4, 4)));
        registry
            .herds
            .push(herd_at("herd_hidden", UVec2::new(40, 40)));

        // The same ledger and the same registry as the leak test above: only the switch differs.
        let ledger = ledger_seeing(64, &[UVec2::new(4, 4)]);
        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let export = |fog_enabled: bool| {
            export_herds_with_fog(
                &telemetry,
                &registry,
                &FaunaConfig::builtin(),
                &LaborConfig::builtin(),
                &ledger,
                fog_enabled,
            )
        };

        assert_eq!(
            export(true)
                .iter()
                .map(|h| h.id.as_str())
                .collect::<Vec<_>>(),
            vec!["herd_seen"],
            "with fog ON the unseen, unowned herd is still withheld"
        );
        assert_eq!(
            export(false)
                .iter()
                .map(|h| h.id.as_str())
                .collect::<Vec<_>>(),
            vec!["herd_seen", "herd_hidden"],
            "with fog OFF the unseen, unowned herd crosses the wire"
        );
    }

    /// The raster half of the same switch. It must agree with the herd list above by construction —
    /// a client cannot be handed herds it is told are standing on black tiles. Note this holds even
    /// with an EMPTY ledger, the state that otherwise fails closed to all-unexplored.
    #[test]
    fn disabling_fog_emits_an_all_active_visibility_raster() {
        let grid = UVec2::new(8, 4);
        let empty = crate::visibility::VisibilityLedger::default();

        let dark = visibility_raster_from_ledger(&empty, VIEWER, grid, true);
        assert!(
            dark.samples.iter().all(|sample| *sample == 0),
            "fog ON with no faction map is all-unexplored — the fail-closed state"
        );

        let revealed = visibility_raster_from_ledger(&empty, VIEWER, grid, false);
        assert_eq!(revealed.width, grid.x);
        assert_eq!(revealed.height, grid.y);
        assert_eq!(revealed.samples.len(), (grid.x * grid.y) as usize);
        assert!(
            revealed
                .samples
                .iter()
                .all(|sample| *sample == Scalar::SCALE),
            "fog OFF paints every tile Active regardless of the ledger"
        );
    }

    /// The heading arrow names a **second** tile, so it carries its own leak: a herd visible at the
    /// edge of your sight would otherwise tell you where it is walking in the dark.
    #[test]
    fn the_heading_arrow_is_withheld_when_the_next_hex_is_unseen() {
        let seen = UVec2::new(4, 4);
        let next = UVec2::new(5, 4);

        let mut registry = HerdRegistry::default();
        let mut herd = herd_at("herd_walker", seen);
        herd.next_pos = Some(next);
        registry.herds.push(herd);

        let hidden_next = export_with(&registry, &ledger_seeing(64, &[seen]));
        let state = &hidden_next[0];
        assert_eq!(
            (state.next_x, state.next_y),
            (-1, -1),
            "the herd is visible but its destination is not, so no arrow is published"
        );

        let both_visible = export_with(&registry, &ledger_seeing(64, &[seen, next]));
        let state = &both_visible[0];
        assert_eq!(
            (state.next_x, state.next_y),
            (next.x as i32, next.y as i32),
            "with both hexes visible the heading crosses unchanged"
        );
    }

    /// **Grazing 2b-iii — the ecological readout on the wire.** A herd exports its live derived
    /// carrying capacity K and the exact hex radius the sim grazes/derives K over. The radius is
    /// resolved from the `SpeciesDef` (migratory `loiter_radius`) exactly as `advance_herds` does, so a
    /// small/big/migratory species each reports the footprint the sim actually uses (0 / 1 /
    /// `loiter_radius`), and the client can reproduce the ring with `hex_range_tiles`.
    #[test]
    fn herd_snapshot_reports_carrying_capacity_and_graze_range_radius() {
        use crate::fauna_config::SizeClass;

        let fauna = FaunaConfig::builtin();
        let labor = LaborConfig::builtin();

        // One mobile herd per size class, each a real species so `species_by_display` resolves the
        // migratory `loiter_radius`. Distinct carrying capacities so the assertion is meaningful.
        let mut registry = HerdRegistry::default();
        registry.herds.push(Herd::new(
            "herd_small".to_string(),
            "Rabbit Warren".to_string(),
            SizeClass::Small,
            vec![UVec2::new(2, 2)],
            120.0,
            163.0,
            0.10,
            0.35,
            2.0,
        ));
        registry.herds.push(Herd::new(
            "herd_big".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            900.0,
            1352.0,
            0.05,
            0.10,
            60.0,
        ));
        registry.herds.push(Herd::new(
            "herd_migratory".to_string(),
            "Thunder Mammoths".to_string(),
            SizeClass::Migratory,
            vec![UVec2::new(6, 6)],
            8000.0,
            9000.0,
            0.011,
            0.04,
            800.0,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );

        for herd in &registry.herds {
            let exported = states.iter().find(|h| h.id == herd.id).unwrap();
            assert!(
                (exported.carrying_capacity - herd.carrying_capacity).abs() < 1e-6,
                "{}: exported K {} should equal the live K {}",
                herd.id,
                exported.carrying_capacity,
                herd.carrying_capacity,
            );
            let expected_radius = herd.graze_range_radius(fauna.species_by_display(&herd.species));
            assert_eq!(
                exported.graze_range_radius, expected_radius,
                "{}: exported graze range radius should equal graze_range_radius(def)",
                herd.id,
            );
        }

        // Pin the per-size expectations so a regression in the size_class → radius mapping is caught.
        let small = states.iter().find(|h| h.id == "herd_small").unwrap();
        assert_eq!(
            small.graze_range_radius, 0,
            "small game grazes its one tile"
        );
        let big = states.iter().find(|h| h.id == "herd_big").unwrap();
        assert_eq!(big.graze_range_radius, 1, "big game grazes radius 1");
        let migratory = states.iter().find(|h| h.id == "herd_migratory").unwrap();
        assert_eq!(
            migratory.graze_range_radius,
            fauna
                .species_by_display("Thunder Mammoths")
                .unwrap()
                .loiter_radius,
            "a migratory herd grazes its loiter_radius",
        );
    }

    /// **The predator's prey-sense ring on the wire** (Predators Phase 1a). A **carnivore** publishes
    /// `prey_sense_radius = fauna.predators.prey_sense_radius` (`> 0`) — the client's "this is a
    /// predator" signal AND its view-ring radius, since a carnivore's graze-range ring is meaningless
    /// (it hunts other herds). A **herbivore** publishes `0`, so the client keeps drawing its graze ring.
    #[test]
    fn herd_snapshot_reports_prey_sense_radius_for_carnivores_only() {
        use crate::fauna_config::SizeClass;

        let fauna = FaunaConfig::builtin();
        let labor = LaborConfig::builtin();

        let mut registry = HerdRegistry::default();
        // A wolf pack (carnivore) and a deer (herbivore) — both real roster species so
        // `species_by_display` resolves the diet.
        registry.herds.push(Herd::new(
            "pred_wolf".to_string(),
            "Grey Wolf Pack".to_string(),
            SizeClass::Big,
            vec![UVec2::new(3, 3)],
            50.0,
            100.0,
            0.0,
            0.15,
            60.0,
        ));
        registry.herds.push(Herd::new(
            "herd_deer".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(5, 5)],
            50.0,
            100.0,
            0.0,
            0.10,
            60.0,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        // Merge reconciliation: origin/main's fog-filter refactored `herd_snapshot_entries` to take
        // `HerdSnapshotInputs` (with a visibility ledger + viewer). Reuse the sibling helpers — an
        // all-seeing ledger, since this test is about WHAT a carnivore exports, not whether fog hides it.
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );

        let wolf = states.iter().find(|h| h.id == "pred_wolf").unwrap();
        assert_eq!(
            wolf.prey_sense_radius, fauna.predators.prey_sense_radius,
            "a carnivore publishes the predators' prey-sense radius",
        );
        assert!(
            wolf.prey_sense_radius > 0,
            "the shipped prey-sense radius is positive, so it doubles as the is-a-predator flag",
        );
        let deer = states.iter().find(|h| h.id == "herd_deer").unwrap();
        assert_eq!(
            deer.prey_sense_radius, 0,
            "a herbivore publishes 0 and the client keeps drawing its graze ring",
        );
    }

    /// **The pen as a managed population, on the wire.** A penned herd exports what it EATS
    /// (`pen_upkeep = pen.upkeep_per_biomass × biomass`) alongside its **gross** `corral_yield`, plus
    /// last turn's `pen_fed_fraction` (`< 1` = starving) — what the client needs for the herd drawer
    /// and the starving warning. A herd that is not penned is never starving.
    #[test]
    fn herd_snapshot_reports_the_pens_upkeep_and_fed_fraction() {
        use crate::fauna_config::SizeClass;
        const PEN_BIOMASS: f32 = 60.0;
        const HALF_FED: f32 = 0.5;

        let mut registry = HerdRegistry::default();
        let mut penned = Herd::new(
            "herd_pen".to_string(),
            "Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            PEN_BIOMASS,
            100.0,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        );
        assert!(
            penned.corral_at(
                UVec2::new(4, 4),
                &crate::intensification::LadderConfig::builtin()
            ),
            "the fixture species must be pennable"
        );
        // The keeper could only pay half the feed last turn → the herd is starving.
        penned.pen_fed_fraction = HALF_FED;
        registry.herds.push(penned);
        registry.herds.push(Herd::new(
            "herd_wild".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            50.0,
            100.0,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );

        let pen = states.iter().find(|h| h.id == "herd_pen").unwrap();
        let expected_upkeep = fauna.husbandry.pen.upkeep_per_biomass * PEN_BIOMASS;
        assert!(
            (pen.pen_upkeep - expected_upkeep).abs() < 1e-6,
            "the pen exports its feed demand at the herd's current biomass: {} vs {expected_upkeep}",
            pen.pen_upkeep
        );
        assert!((pen.pen_fed_fraction - HALF_FED).abs() < 1e-6);
        assert!(
            pen.corral_yield > 0.0,
            "the pen's gross managed yield rides alongside its upkeep"
        );

        let wild = states.iter().find(|h| h.id == "herd_wild").unwrap();
        assert_eq!(
            wild.pen_fed_fraction, 1.0,
            "a mobile herd is never starving"
        );
    }

    /// **The feed must be known at the moment the player DECIDES.** `penUpkeep` is answered for an
    /// **unpenned** herd too — the feed the pen *would* demand once built, at the herd's current
    /// biomass — because the pre-commit `Corral` row is by definition looking at a herd that is not yet
    /// penned. Quoting `corralYield` (the payoff) while reporting `penUpkeep = 0` (the running cost)
    /// would advertise a number the player will never bank: the same defect class as quoting the gross
    /// yield. The two are computed on the **same biomass basis**, so the client can just subtract.
    #[test]
    fn an_unpenned_herd_exports_the_feed_its_pen_would_demand() {
        use crate::fauna_config::SizeClass;
        /// Above the managed harvest's escapement point (`K/2`), so the pen has a positive projected
        /// yield to sit the projected feed *next to* — at or below `K/2` a pen honestly pays nothing
        /// until the herd rebuilds, and the row would be `0 → 0`.
        const BIOMASS: f32 = 900.0;
        const CAP: f32 = 1200.0;

        let mut registry = HerdRegistry::default();
        // A tamed but MOBILE herd — exactly what a player inspects while deciding whether to corral.
        let mut mobile = Herd::new(
            "herd_mobile".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(2, 2)],
            BIOMASS,
            CAP,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        );
        mobile.tame_outright(
            FactionId(0),
            &crate::intensification::LadderConfig::builtin(),
        );
        registry.herds.push(mobile);
        // The same herd, penned — its upkeep must read the same at the same biomass.
        let mut penned = Herd::new(
            "herd_penned".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(3, 3)],
            BIOMASS,
            CAP,
            0.0,
            0.05,
            SNAPSHOT_BODY_MASS,
        );
        penned.tame_outright(
            FactionId(0),
            &crate::intensification::LadderConfig::builtin(),
        );
        assert!(
            penned.corral_at(
                UVec2::new(3, 3),
                &crate::intensification::LadderConfig::builtin()
            ),
            "the fixture species must be pennable"
        );
        registry.herds.push(penned);

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let states = export_herds(
            &telemetry,
            &registry,
            &fauna,
            &labor,
            &all_seeing_ledger(64),
        );

        let expected = fauna.husbandry.pen.upkeep_per_biomass * BIOMASS;
        assert!(expected > 0.0);

        let mobile = states.iter().find(|h| h.id == "herd_mobile").unwrap();
        assert!(
            !mobile.corralled,
            "the herd under consideration is NOT penned"
        );
        assert!(
            (mobile.pen_upkeep - expected).abs() < 1e-6,
            "an unpenned herd must export the feed its pen WOULD demand \
             (upkeep_per_biomass × biomass = {expected}): got {}",
            mobile.pen_upkeep
        );
        assert!(
            mobile.corral_yield > 0.0,
            "the payoff is already projected for an unpenned herd — the cost must be too"
        );

        // A penned herd is unchanged, and reads the SAME upkeep at the same biomass: one field, one
        // meaning, so `corralYield − penUpkeep` is a valid subtraction on either side of the decision.
        let penned = states.iter().find(|h| h.id == "herd_penned").unwrap();
        assert!(penned.corralled);
        assert!(
            (penned.pen_upkeep - mobile.pen_upkeep).abs() < 1e-6,
            "penned and unpenned must agree at the same biomass: {} vs {}",
            penned.pen_upkeep,
            mobile.pen_upkeep
        );
    }
}
