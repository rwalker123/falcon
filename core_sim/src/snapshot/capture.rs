use super::*;

use std::sync::OnceLock;

#[derive(SystemParam)]
pub(crate) struct GreatDiscoverySnapshotParam<'w, 's> {
    ledger: Res<'w, GreatDiscoveryLedger>,
    readiness: Res<'w, GreatDiscoveryReadiness>,
    telemetry: Res<'w, GreatDiscoveryTelemetry>,
    registry: Res<'w, GreatDiscoveryRegistry>,
    #[system_param(ignore)]
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
pub struct SnapshotContext<'w> {
    pub config: Res<'w, SimulationConfig>,
    pub tick: Res<'w, SimulationTick>,
    /// The monotonic world-build counter, stamped onto the snapshot header so a client can tell a
    /// freshly-generated world from a stale one the snapshot server replays. Always present (the
    /// idle boot app inserts a default `0`; the server overwrites it per world (re)build).
    pub world_epoch: Res<'w, WorldEpoch>,
    pub overlays: Res<'w, SnapshotOverlaysConfigHandle>,
    pub metrics: Res<'w, SimulationMetrics>,
    pub crisis_overlay: Res<'w, CrisisOverlayCache>,
    pub start_location: Res<'w, StartLocation>,
    pub herds: Res<'w, HerdTelemetry>,
    /// Authoritative herd sim state, captured into the rollback snapshot (`herd_registry`) so a
    /// rollback rewinds biomass / position / movement — the display `herds` telemetry alone is lossy.
    pub herd_registry: Res<'w, HerdRegistry>,
    /// Authoritative depletable-forage sim state, captured into the rollback snapshot
    /// (`forage_registry`) so a rollback rewinds patch biomass / ecology phase. Mirrors
    /// `herd_registry` — see the forage-depletion note in `core_sim/CLAUDE.md`.
    pub forage_registry: Res<'w, ForageRegistry>,
    /// Authoritative graze/pasture sim state, captured into the rollback snapshot (`graze_registry`)
    /// so a rollback rewinds grazing draw-down. The *client* readout rides `TileState.graze_*` (graze
    /// is on nearly every land tile, so a per-patch list would be the wrong shape) — see `graze.rs`.
    pub graze_registry: Res<'w, GrazeRegistry>,
    /// The Telling's narrative memory, captured into the rollback snapshot (`beat_ledger`) so a
    /// rollback past a beat lets that beat fire again — see `core_sim/src/telling/mod.rs`.
    pub beat_ledger: Res<'w, BeatLedger>,
    pub elevation: Res<'w, ElevationField>,
    pub moisture: Option<Res<'w, MoistureRaster>>,
    #[allow(dead_code)]
    pub map_presets: Res<'w, MapPresetsHandle>,
    pub campaign_label: Option<Res<'w, CampaignLabel>>,
    pub start_profiles: Res<'w, StartProfilesHandle>,
    pub victory: Res<'w, VictoryState>,
    pub faction_inventory: Res<'w, FactionInventory>,
    pub sedentarization: Res<'w, SedentarizationScore>,
    pub discovered_sites: Res<'w, DiscoveredSites>,
    pub sites_config: Res<'w, SitesConfigHandle>,
    pub food_sites: Res<'w, FoodSiteRegistry>,
    pub command_events: Res<'w, CommandEventLog>,
    pub capability_flags: Res<'w, CapabilityFlags>,
    pub visibility_ledger: Res<'w, crate::visibility::VisibilityLedger>,
    pub viewer_faction: Res<'w, crate::visibility::ViewerFaction>,
    pub demographics: Res<'w, DemographicsConfigHandle>,
    pub wellbeing: Res<'w, crate::wellbeing_config::WellbeingConfigHandle>,
    pub labor: Res<'w, crate::labor_config::LaborConfigHandle>,
    /// The flora roster. Read at capture so each forage patch can publish the **named plants its
    /// biome's capacity is made of** (`ForagePatchState::composition`) — derived from the roster's
    /// precomputed per-biome share table, never from per-patch state.
    pub flora: Res<'w, FloraConfigHandle>,
    /// The intensification ladder. Read at capture because both food webs' **pre-commit yield
    /// forecasts** quote the investment rungs' dipped ceiling (`Cultivate` / `Corral`) off the
    /// rung's `yield_fraction_while_building` — the same seam the take pays with, so forecast ==
    /// actual (see `core_sim/CLAUDE.md` → The Intensification Ladder).
    pub ladder: Res<'w, crate::intensification::LadderConfigHandle>,
    /// Fauna tuning (ecology / hunt / market / husbandry). Read at capture for each herd's
    /// **pre-commit yield forecast** (`fauna::hunt_forecast` — the client's live "Expected yield" +
    /// worker-stepper cap and the exported per-policy `hunt_policy_ceilings`), the per-cohort hunt
    /// throughput, and the pre-launch expedition trip estimates (see `core_sim/CLAUDE.md` →
    /// Scouting & Hunting Expeditions → Snapshot).
    pub fauna: Res<'w, crate::fauna_config::FaunaConfigHandle>,
    pub expedition: Res<'w, crate::expedition_config::ExpeditionConfigHandle>,
    pub settlement_stage: Res<'w, crate::settlement_stage_config::SettlementStageConfigHandle>,
    pub supply_membership: Res<'w, SupplyNetworkMembership>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
    /// How to write the capture result: record a new ring entry (turn path) or refresh the latest
    /// broadcast in place (post-command re-capture). Bundled here to keep `capture_snapshot` within
    /// Bevy's 16-arg system limit.
    pub capture_mode: Res<'w, SnapshotCaptureMode>,
}

#[derive(Clone)]
pub struct StoredSnapshot {
    pub tick: u64,
    pub snapshot: Arc<WorldSnapshot>,
    pub delta: Arc<WorldDelta>,
    /// `None` on every turn but the world's first — see [`StoredSnapshot::encode_flat`].
    pub encoded_snapshot_flat: Option<Arc<Vec<u8>>>,
}

impl StoredSnapshot {
    /// **A ring entry stores no encoded bytes at all on a steady-state turn.** The flat socket is
    /// the only socket ([`crate::network::broadcast_latest`]), and what it broadcasts per turn — the
    /// flat delta — is built by `publish` for immediate sending rather than retained: 256 ring
    /// entries holding a delta nobody re-reads cost ~24% of an 80×52 turn for nothing. The
    /// on-demand feed paths (`update_axis_bias` / `update_influencers` / `update_corruption`) build
    /// theirs the same way, locally, and return it.
    ///
    /// So the one encoding left here is `encode.flat_snapshot`, on a world's **first** publication
    /// only, profiled separately so the log says what it costs (see [`crate::turn_profile`]).
    fn new(snapshot: Arc<WorldSnapshot>, delta: Arc<WorldDelta>, flat_snapshot: bool) -> Self {
        // Only the world's FIRST publication pays for a flat snapshot; every later turn broadcasts
        // the flat delta instead. This is the 44%-of-a-turn line item from #384, and it is now the
        // baseline's cost rather than every turn's.
        let encoded_snapshot_flat = flat_snapshot.then(|| {
            let _s = crate::turn_profile::publish_scope("publish.encode.flat_snapshot");
            Arc::new(encode_snapshot_flatbuffer(snapshot.as_ref()))
        });
        Self {
            tick: snapshot.header.tick,
            snapshot,
            delta,
            encoded_snapshot_flat,
        }
    }

    /// The flat snapshot for this entry, encoding it if this turn did not — under delta streaming
    /// almost no entry carries one, and paying for every entry to serve the few ever asked for is
    /// exactly the dead work `.claude/rules/core_sim/turn-profiling.md` records removing once
    /// already.
    ///
    /// **This is a read of stored bytes, not a publication, so no broadcast path may use it.** The
    /// header here carries the sequence number this entry was published under, which goes stale the
    /// moment anything else publishes. Rollback and `Command::Resync` — the two paths that used to
    /// call it — go through [`SnapshotHistory::publish_full_frame`], which claims a live one. The
    /// remaining callers are integration tests asserting on encoded content rather than on sequence.
    pub fn encode_flat(&self) -> Arc<Vec<u8>> {
        match self.encoded_snapshot_flat.as_ref() {
            Some(bytes) => Arc::clone(bytes),
            None => Arc::new(encode_snapshot_flatbuffer(self.snapshot.as_ref())),
        }
    }
}

/// Which kind of publication a `publish` call is: a resolved turn, or a mid-tick recapture after a
/// world-mutating command. They differ only in whether the baseline is committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Publication {
    Turn,
    Recapture,
}

/// Everything publication owns: the diff baselines, the rollback ring, and the publication
/// sequence.
///
/// **This is not a Bevy resource and the turn thread never touches it.** It lives behind the mutex
/// inside [`crate::snapshot::SnapshotHistory`], which is the ECS-facing handle, and is mutated
/// almost exclusively by the publisher thread (#393). The exceptions are the rare, human-paced
/// paths — rollback, `Resync`, the auxiliary feed deltas — which the handle runs inline *after*
/// draining the publisher's queue, so they can never interleave with a frame in flight.
pub(crate) struct PublishState {
    capacity: usize,
    pub last_snapshot: Option<Arc<WorldSnapshot>>,
    pub last_delta: Option<Arc<WorldDelta>>,
    pub encoded_snapshot_flat: Option<Arc<Vec<u8>>>,
    /// The flat DELTA broadcast on the client's socket every turn after the first.
    pub encoded_delta_flat: Option<Arc<Vec<u8>>>,
    /// Where a published frame goes. `None` until the server attaches its socket, which is the
    /// normal state in tests and for the idle boot app — publication still happens, it simply has
    /// no audience.
    pub sink: Option<Arc<dyn FrameSink>>,
    /// The last published frame's own phase breakdown, drained from the publisher thread's
    /// accumulator (`turn_profile::publish_take`) and parked here because a thread-local cannot be
    /// read from the side that wants it. Empty until the first frame.
    pub last_publish_profile: Vec<crate::turn_profile::PhaseTiming>,
    /// `frameSeq` of the last frame published for this world. Fresh per world, because a rebuild
    /// constructs a brand-new `App` and therefore a brand-new history — which is also what makes
    /// "first publication" simply mean `frame_seq == 0`.
    frame_seq: u64,
    tiles: HashMap<u64, TileState>,
    logistics: HashMap<u64, LogisticsLinkState>,
    trade_links: HashMap<u64, TradeLinkState>,
    populations: HashMap<u64, PopulationCohortState>,
    power: HashMap<u64, PowerNodeState>,
    power_metrics: PowerTelemetryState,
    generations: HashMap<u16, GenerationState>,
    influencers: HashMap<u32, InfluentialIndividualState>,
    culture_layers: HashMap<u32, CultureLayerState>,
    culture_tensions: Vec<CultureTensionState>,
    discovery_progress: HashMap<(u32, u32), DiscoveryProgressEntry>,
    great_discoveries: HashMap<(u32, u16), GreatDiscoveryState>,
    great_discovery_definitions: HashMap<u16, GreatDiscoveryDefinitionState>,
    great_discovery_progress: HashMap<(u32, u16), GreatDiscoveryProgressState>,
    great_discovery_telemetry: GreatDiscoveryTelemetryState,
    knowledge_ledger: HashMap<u64, KnowledgeLedgerEntryState>,
    knowledge_metrics: KnowledgeMetricsState,
    knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    crisis_telemetry: CrisisTelemetryState,
    crisis_overlay: CrisisOverlayState,
    start_marker: Option<StartMarkerState>,
    axis_bias: AxisBiasState,
    sentiment: SentimentTelemetryState,
    terrain_overlay: TerrainOverlayState,
    logistics_raster: ScalarRasterState,
    sentiment_raster: ScalarRasterState,
    corruption_raster: ScalarRasterState,
    visibility_raster: ScalarRasterState,
    /// Last published `SimulationConfig::fog_enabled`, so the auxiliary (axis-bias / sentiment)
    /// deltas below echo the live setting instead of the `bool` derived default (`false`).
    fog_enabled: bool,
    culture_raster: ScalarRasterState,
    military_raster: ScalarRasterState,
    moisture_raster: FloatRasterState,
    elevation_overlay: ElevationOverlayState,
    climate_bands: ClimateBandsState,
    corruption: CorruptionLedger,
    victory: VictorySnapshotState,
    capability_flags: u32,
    faction_inventory: Vec<SchemaFactionInventoryState>,
    sedentarization: Vec<SchemaSedentarizationState>,
    discovered_sites: Vec<SchemaDiscoveredSitesState>,
    demographics: Vec<SchemaPopulationDemographicsState>,
    forage_patches: Vec<ForagePatchState>,
    intensification_knowledge: Vec<IntensificationKnowledgeState>,
    campaign_profiles: Vec<CampaignProfileState>,
    command_events: Vec<CommandEventState>,
    pending_forks: Vec<PendingForksState>,
    stance_axes: Vec<StanceState>,
    voice_medium: Vec<VoiceMediumState>,
    herds: Vec<HerdTelemetryState>,
    food_modules: Vec<FoodModuleState>,
    history: VecDeque<StoredSnapshot>,
}

/// How many threads the publisher's diff fan-out may use.
///
/// Deliberately a small fixed number rather than `num_cpus`. The publisher runs **concurrently with
/// the next turn** by construction, so a pool as wide as the machine buys its own milliseconds by
/// taking cores from the simulation it is racing — and the simulation is the half that has to keep
/// scaling as systems are added. Today the sim is ~5% of a turn, so a greedy publisher costs nothing
/// visible; sizing the pool now is what stops that from being discovered later, in a profile nobody
/// takes.
///
/// Four covers the three map-sized sections (tiles, culture layers, power) plus a worker for the
/// tail of small ones, which is where the work-stealing has something to steal.
const DIFF_POOL_THREADS: usize = 4;

/// The publisher's own rayon pool, built once per process.
///
/// Process-wide rather than per world: `cargo test` runs many worlds at once, each with its own
/// publisher thread, and one bounded pool shared between them is the point — a pool *per* world
/// would multiply the core budget by the number of worlds and defeat the bound.
static DIFF_POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();

/// Run `work` on the publisher's bounded pool, falling back to the caller's thread if the pool could
/// not be built. A pool that failed to construct is not a reason to stop publishing.
fn in_diff_pool<R: Send>(work: impl FnOnce() -> R + Send) -> R {
    let pool = DIFF_POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(DIFF_POOL_THREADS)
            .thread_name(|index| format!("snapshot-diff-{index}"))
            .build()
            .map_err(|err| log::error!("snapshot diff pool unavailable, diffing serially: {err}"))
            .ok()
    });
    match pool {
        Some(pool) => pool.install(work),
        None => work(),
    }
}

// ---------------------------------------------------------------------------------------------
// The snapshot's SECTIONS: one group of collections per subsystem, each diffed as a unit and
// spawned as one task by `PublishState::publish`. A section is a `*Parts` output struct, an
// optional `*Baselines` borrow bundle, and a `diff_*` function between them.
//
// The partition is SEMANTIC, not cost-balanced. A cost-balanced one has to be re-measured every
// time a subsystem grows or a raster is added; a semantic one keeps its meaning as the weights
// move, and the scheduler is what balances it — `rayon::scope` work-steals, so an unequal section
// is absorbed rather than baked into the partition.
//
// ADDING A SNAPSHOT COLLECTION IS: a baseline field, a line in its section's `*Parts`, a line in
// its section's `diff_*`, a line in the assembly. No new task, no re-balancing, no measurement.
// That registration property is what this shape is for.
//
// Two rules hold for every section, and both are load-bearing:
//
// * A section reads and writes ONLY its own baselines. That is what makes the `&mut` borrows
//   disjoint by construction and the tasks free of shared mutable state. A section needing another
//   section's baseline would mean the partition is wrong, not that a lock is needed.
// * NO `crate::turn_profile::publish_scope` inside a section. The publisher profiler's accumulator
//   is thread-local, so a span opened on a pool worker folds into an accumulator nothing ever
//   drains — the label would vanish from the frame's profile and leak into the next frame on that
//   worker. The single `publish.diff` scope stays on the publisher thread, around the whole
//   fan-out.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct TileParts {
    sent: Vec<TileState>,
    removed: Vec<u64>,
}

/// The map's tiles: the largest single collection, one entry per hex.
fn diff_tiles(
    baseline: &mut HashMap<u64, TileState>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> TileParts {
    let (sent, removed) = diff_new_tiles(baseline, &snapshot.tiles, write);
    TileParts { sent, removed }
}

#[derive(Debug, Default)]
struct CultureParts {
    sent: Vec<CultureLayerState>,
    removed: Vec<u32>,
    tensions: Option<Vec<CultureTensionState>>,
}

/// Culture: the per-tile layer grid (map-sized, on the published-state deadband) and the tension
/// roster that rides with it.
fn diff_culture(
    layers: &mut HashMap<u32, CultureLayerState>,
    tensions: &mut Vec<CultureTensionState>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> CultureParts {
    let (sent, removed) = diff_new_culture_layers(layers, &snapshot.culture_layers, write);
    CultureParts {
        sent,
        removed,
        tensions: diff_whole(tensions, &snapshot.culture_tensions, write),
    }
}

#[derive(Debug, Default)]
struct PowerParts {
    sent: Vec<PowerNodeState>,
    removed: Vec<u64>,
    metrics: Option<PowerTelemetryState>,
}

/// Power: one node per tile, so map-sized like the two above, plus the grid's telemetry block.
fn diff_power(
    nodes: &mut HashMap<u64, PowerNodeState>,
    metrics: &mut PowerTelemetryState,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> PowerParts {
    let (sent, removed) = diff_new(nodes, &snapshot.power, |state| state.entity, write);
    PowerParts {
        sent,
        removed,
        metrics: diff_whole(metrics, &snapshot.power_metrics, write),
    }
}

/// Every whole-map raster and overlay. Each is one `Vec` the size of the map, so the comparison is
/// cheap and the clone — taken only when it differs — is not.
#[derive(Debug, Default)]
struct RasterParts {
    terrain: Option<TerrainOverlayState>,
    moisture: Option<FloatRasterState>,
    elevation: Option<ElevationOverlayState>,
    climate_bands: Option<ClimateBandsState>,
    logistics: Option<ScalarRasterState>,
    sentiment: Option<ScalarRasterState>,
    corruption: Option<ScalarRasterState>,
    culture: Option<ScalarRasterState>,
    military: Option<ScalarRasterState>,
    visibility: Option<ScalarRasterState>,
}

/// The baselines the raster section owns, borrowed disjointly out of [`PublishState`].
struct RasterBaselines<'a> {
    terrain: &'a mut TerrainOverlayState,
    moisture: &'a mut FloatRasterState,
    elevation: &'a mut ElevationOverlayState,
    climate_bands: &'a mut ClimateBandsState,
    logistics: &'a mut ScalarRasterState,
    sentiment: &'a mut ScalarRasterState,
    corruption: &'a mut ScalarRasterState,
    culture: &'a mut ScalarRasterState,
    military: &'a mut ScalarRasterState,
    visibility: &'a mut ScalarRasterState,
}

fn diff_rasters(
    baseline: RasterBaselines<'_>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> RasterParts {
    RasterParts {
        terrain: diff_whole(baseline.terrain, &snapshot.terrain, write),
        moisture: diff_whole(baseline.moisture, &snapshot.moisture_raster, write),
        elevation: diff_whole(baseline.elevation, &snapshot.elevation_overlay, write),
        // A per-map constant: it changes only on (re)generation, so the delta re-sends it just then.
        climate_bands: diff_whole(baseline.climate_bands, &snapshot.climate_bands, write),
        logistics: diff_whole(baseline.logistics, &snapshot.logistics_raster, write),
        sentiment: diff_whole(baseline.sentiment, &snapshot.sentiment_raster, write),
        corruption: diff_whole(baseline.corruption, &snapshot.corruption_raster, write),
        culture: diff_whole(baseline.culture, &snapshot.culture_raster, write),
        military: diff_whole(baseline.military, &snapshot.military_raster, write),
        visibility: diff_whole(baseline.visibility, &snapshot.visibility_raster, write),
    }
}

#[derive(Debug, Default)]
struct KnowledgeParts {
    ledger: Vec<KnowledgeLedgerEntryState>,
    removed_ledger: Vec<u64>,
    metrics: Option<KnowledgeMetricsState>,
    timeline: Option<Vec<KnowledgeTimelineEventState>>,
    discovery_progress: Vec<DiscoveryProgressEntry>,
    great_discoveries: Vec<GreatDiscoveryState>,
    great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    great_discovery_definitions: Option<Vec<GreatDiscoveryDefinitionState>>,
    great_discovery_telemetry: Option<GreatDiscoveryTelemetryState>,
}

/// The baselines the knowledge section owns.
struct KnowledgeBaselines<'a> {
    ledger: &'a mut HashMap<u64, KnowledgeLedgerEntryState>,
    metrics: &'a mut KnowledgeMetricsState,
    timeline: &'a mut Vec<KnowledgeTimelineEventState>,
    discovery_progress: &'a mut HashMap<(u32, u32), DiscoveryProgressEntry>,
    great_discoveries: &'a mut HashMap<(u32, u16), GreatDiscoveryState>,
    great_discovery_progress: &'a mut HashMap<(u32, u16), GreatDiscoveryProgressState>,
    great_discovery_definitions: &'a mut HashMap<u16, GreatDiscoveryDefinitionState>,
    great_discovery_telemetry: &'a mut GreatDiscoveryTelemetryState,
}

/// Knowledge, espionage and great discoveries.
///
/// `discovery_progress`, `great_discoveries` and `great_discovery_progress` have no `removed_*`
/// counterpart on the wire, so their removals are dropped after the baseline has been pruned — the
/// entry leaves the baseline, the client simply is not told (it never was).
fn diff_knowledge(
    baseline: KnowledgeBaselines<'_>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> KnowledgeParts {
    let (ledger, removed_ledger) = diff_new(
        baseline.ledger,
        &snapshot.knowledge_ledger,
        |entry| encode_ledger_key(FactionId(entry.owner_faction), entry.discovery_id),
        write,
    );
    let (discovery_progress, _) = diff_new(
        baseline.discovery_progress,
        &snapshot.discovery_progress,
        |entry| (entry.faction, entry.discovery),
        write,
    );
    let (great_discoveries, _) = diff_new(
        baseline.great_discoveries,
        &snapshot.great_discoveries,
        |state| (state.faction, state.id),
        write,
    );
    let (great_discovery_progress, _) = diff_new(
        baseline.great_discovery_progress,
        &snapshot.great_discovery_progress,
        |state| (state.faction, state.discovery),
        write,
    );
    KnowledgeParts {
        ledger,
        removed_ledger,
        metrics: diff_whole(baseline.metrics, &snapshot.knowledge_metrics, write),
        timeline: diff_whole(baseline.timeline, &snapshot.knowledge_timeline, write),
        discovery_progress,
        great_discoveries,
        great_discovery_progress,
        great_discovery_definitions: diff_whole_indexed(
            baseline.great_discovery_definitions,
            &snapshot.great_discovery_definitions,
            |state| state.id,
            write,
        ),
        great_discovery_telemetry: diff_whole(
            baseline.great_discovery_telemetry,
            &snapshot.great_discovery_telemetry,
            write,
        ),
    }
}

#[derive(Debug, Default)]
struct CrisisParts {
    telemetry: Option<CrisisTelemetryState>,
    overlay: Option<CrisisOverlayState>,
    victory: Option<VictorySnapshotState>,
}

/// Crisis and victory — the two whole-section telemetry blocks that decide how a campaign ends.
fn diff_crisis(
    telemetry: &mut CrisisTelemetryState,
    overlay: &mut CrisisOverlayState,
    victory: &mut VictorySnapshotState,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> CrisisParts {
    CrisisParts {
        telemetry: diff_whole(telemetry, &snapshot.crisis_telemetry, write),
        overlay: diff_whole(overlay, &snapshot.crisis_overlay, write),
        victory: diff_whole(victory, &snapshot.victory, write),
    }
}

#[derive(Debug, Default)]
struct CampaignParts {
    profiles: Option<Vec<CampaignProfileState>>,
    command_events: Option<Vec<CommandEventState>>,
    pending_forks: Option<Vec<PendingForksState>>,
    stance_axes: Option<Vec<StanceState>>,
    voice_medium: Option<Vec<VoiceMediumState>>,
    faction_inventory: Option<Vec<SchemaFactionInventoryState>>,
    sedentarization: Option<Vec<SchemaSedentarizationState>>,
    discovered_sites: Option<Vec<SchemaDiscoveredSitesState>>,
    demographics: Option<Vec<SchemaPopulationDemographicsState>>,
    intensification_knowledge: Option<Vec<IntensificationKnowledgeState>>,
    start_marker: Option<StartMarkerState>,
}

/// The baselines the campaign section owns.
struct CampaignBaselines<'a> {
    profiles: &'a mut Vec<CampaignProfileState>,
    command_events: &'a mut Vec<CommandEventState>,
    pending_forks: &'a mut Vec<PendingForksState>,
    stance_axes: &'a mut Vec<StanceState>,
    voice_medium: &'a mut Vec<VoiceMediumState>,
    faction_inventory: &'a mut Vec<SchemaFactionInventoryState>,
    sedentarization: &'a mut Vec<SchemaSedentarizationState>,
    discovered_sites: &'a mut Vec<SchemaDiscoveredSitesState>,
    demographics: &'a mut Vec<SchemaPopulationDemographicsState>,
    intensification_knowledge: &'a mut Vec<IntensificationKnowledgeState>,
    start_marker: &'a mut Option<StartMarkerState>,
}

/// Campaign and the Telling: the profile roster, the stance vector, the fork queue, and the
/// per-faction readouts the campaign panels render.
fn diff_campaign(
    baseline: CampaignBaselines<'_>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> CampaignParts {
    CampaignParts {
        profiles: diff_whole(baseline.profiles, &snapshot.campaign_profiles, write),
        command_events: diff_whole(baseline.command_events, &snapshot.command_events, write),
        pending_forks: diff_whole(baseline.pending_forks, &snapshot.pending_forks, write),
        stance_axes: diff_whole(baseline.stance_axes, &snapshot.stance_axes, write),
        voice_medium: diff_whole(baseline.voice_medium, &snapshot.voice_medium, write),
        faction_inventory: diff_whole(
            baseline.faction_inventory,
            &snapshot.faction_inventory,
            write,
        ),
        sedentarization: diff_whole(baseline.sedentarization, &snapshot.sedentarization, write),
        discovered_sites: diff_whole(baseline.discovered_sites, &snapshot.discovered_sites, write),
        demographics: diff_whole(baseline.demographics, &snapshot.demographics, write),
        intensification_knowledge: diff_whole(
            baseline.intensification_knowledge,
            &snapshot.intensification_knowledge,
            write,
        ),
        // `Option` on both sides, so the delta carries the INNER option: `None` here means
        // unchanged, and a marker that was cleared arrives as `Some(None)` flattened to `None` —
        // the same conflation this field has always had.
        start_marker: diff_whole(baseline.start_marker, &snapshot.start_marker, write).flatten(),
    }
}

#[derive(Debug, Default)]
struct SubsistenceParts {
    herds: Option<Vec<HerdTelemetryState>>,
    forage_patches: Option<Vec<ForagePatchState>>,
    food_modules: Option<Vec<FoodModuleState>>,
}

/// Fauna and flora: the herd roster, the forage patches, and the food-module map.
fn diff_subsistence(
    herds: &mut Vec<HerdTelemetryState>,
    forage_patches: &mut Vec<ForagePatchState>,
    food_modules: &mut Vec<FoodModuleState>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> SubsistenceParts {
    SubsistenceParts {
        herds: diff_whole(herds, &snapshot.herds, write),
        forage_patches: diff_whole(forage_patches, &snapshot.forage_patches, write),
        food_modules: diff_whole(food_modules, &snapshot.food_modules, write),
    }
}

#[derive(Debug, Default)]
struct PeopleParts {
    logistics: Vec<LogisticsLinkState>,
    removed_logistics: Vec<u64>,
    trade_links: Vec<TradeLinkState>,
    removed_trade_links: Vec<u64>,
    populations: Vec<PopulationCohortState>,
    removed_populations: Vec<u64>,
    generations: Vec<GenerationState>,
    removed_generations: Vec<u16>,
    influencers: Vec<InfluentialIndividualState>,
    removed_influencers: Vec<u32>,
    axis_bias: Option<AxisBiasState>,
    sentiment: Option<SentimentTelemetryState>,
    corruption: Option<CorruptionLedger>,
    capability_flags: Option<u32>,
}

/// The baselines the people-and-network section owns.
struct PeopleBaselines<'a> {
    logistics: &'a mut HashMap<u64, LogisticsLinkState>,
    trade_links: &'a mut HashMap<u64, TradeLinkState>,
    populations: &'a mut HashMap<u64, PopulationCohortState>,
    generations: &'a mut HashMap<u16, GenerationState>,
    influencers: &'a mut HashMap<u32, InfluentialIndividualState>,
    axis_bias: &'a mut AxisBiasState,
    sentiment: &'a mut SentimentTelemetryState,
    corruption: &'a mut CorruptionLedger,
    capability_flags: &'a mut u32,
}

/// People and the networks between them: cohorts, generations, influencers, the logistics and trade
/// graphs, and the faction-wide scalars that ride with them.
fn diff_people(
    baseline: PeopleBaselines<'_>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> PeopleParts {
    let (logistics, removed_logistics) = diff_new(
        baseline.logistics,
        &snapshot.logistics,
        |state| state.entity,
        write,
    );
    let (trade_links, removed_trade_links) = diff_new(
        baseline.trade_links,
        &snapshot.trade_links,
        |state| state.entity,
        write,
    );
    let (populations, removed_populations) = diff_new(
        baseline.populations,
        &snapshot.populations,
        |state| state.entity,
        write,
    );
    let (generations, removed_generations) = diff_new(
        baseline.generations,
        &snapshot.generations,
        |state| state.id,
        write,
    );
    let (influencers, removed_influencers) = diff_new(
        baseline.influencers,
        &snapshot.influencers,
        |state| state.id,
        write,
    );
    PeopleParts {
        logistics,
        removed_logistics,
        trade_links,
        removed_trade_links,
        populations,
        removed_populations,
        generations,
        removed_generations,
        influencers,
        removed_influencers,
        axis_bias: diff_whole(baseline.axis_bias, &snapshot.axis_bias, write),
        sentiment: diff_whole(baseline.sentiment, &snapshot.sentiment, write),
        corruption: diff_whole(baseline.corruption, &snapshot.corruption, write),
        capability_flags: diff_whole(baseline.capability_flags, &snapshot.capability_flags, write),
    }
}

impl PublishState {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            last_snapshot: None,
            last_delta: None,
            encoded_snapshot_flat: None,
            encoded_delta_flat: None,
            sink: None,
            last_publish_profile: Vec::new(),
            frame_seq: 0,
            tiles: HashMap::new(),
            logistics: HashMap::new(),
            trade_links: HashMap::new(),
            populations: HashMap::new(),
            power: HashMap::new(),
            power_metrics: PowerTelemetryState::default(),
            generations: HashMap::new(),
            influencers: HashMap::new(),
            culture_layers: HashMap::new(),
            culture_tensions: Vec::new(),
            discovery_progress: HashMap::new(),
            great_discoveries: HashMap::new(),
            great_discovery_definitions: HashMap::new(),
            great_discovery_progress: HashMap::new(),
            great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
            knowledge_ledger: HashMap::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            knowledge_timeline: Vec::new(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            start_marker: None,
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            terrain_overlay: TerrainOverlayState::default(),
            logistics_raster: ScalarRasterState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            visibility_raster: ScalarRasterState::default(),
            fog_enabled: true,
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            climate_bands: ClimateBandsState::default(),
            corruption: CorruptionLedger::default(),
            victory: VictorySnapshotState::default(),
            capability_flags: 0,
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            pending_forks: Vec::new(),
            stance_axes: Vec::new(),
            voice_medium: Vec::new(),
            herds: Vec::new(),
            food_modules: Vec::new(),
            history: VecDeque::new(),
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        self.prune();
    }

    pub(crate) fn len(&self) -> usize {
        self.history.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    pub(crate) fn latest_entry(&self) -> Option<StoredSnapshot> {
        self.history.back().cloned()
    }

    pub(crate) fn entry(&self, tick: u64) -> Option<StoredSnapshot> {
        self.history
            .iter()
            .find(|entry| entry.tick == tick)
            .cloned()
    }

    /// Hash, diff, encode and record one captured world, returning the frame to put on the wire.
    ///
    /// [`Publication::Turn`] is a resolved turn: diff against the committed baseline, commit the
    /// new baseline, push a rollback ring entry.
    ///
    /// [`Publication::Recapture`] is a mid-tick re-capture after a world-mutating command. Same
    /// diff, but it deliberately does **not** commit the baseline or push a ring entry (the
    /// rollback ring stays one-entry-per-tick). That is what makes these deltas *cumulative*: each
    /// one is `baseline(last turn) → now`, so each is a superset of the last, applying them in
    /// order is idempotent, and missing an intermediate one is harmless. It also means the next
    /// turn's delta still carries everything the command changed.
    ///
    /// It used to re-encode a FULL flat snapshot instead — per world-mutating command, so a player
    /// assigning labor to three sources and moving a band paid four full encodes, which is the
    /// cost that arc removed from the turn path re-entering by the side door.
    ///
    /// **The return value states the publication rule** — the full flat frame when this world has
    /// one pending, otherwise the flat delta — in the one place that knows which was produced.
    /// It used to live in `network::broadcast_latest`, reading the two fields back off this struct
    /// after the fact; keeping it here is what stops a later turn re-broadcasting a stale full
    /// snapshot.
    pub(crate) fn publish(
        &mut self,
        snapshot: WorldSnapshot,
        kind: Publication,
    ) -> Option<Arc<Vec<u8>>> {
        // No content hash is stamped. `WorldSnapshot::finalize` used to run here and bincode-encode
        // the whole world to produce `header.hash` — ~1.0 ms a frame for a value nothing read (see
        // `SnapshotHeader::hash`). Retired in #393 rather than merely moved off the turn thread,
        // because moving dead work still pays for it.

        // The baselines are mutated IN PLACE by the fan-out below, so the recapture path states its
        // intent up front rather than by declining to store a returned map: a mid-tick recapture
        // holds the baseline where the last resolved turn left it, which is what makes its deltas
        // cumulative.
        let write = match kind {
            Publication::Turn => Baseline::Advance,
            Publication::Recapture => Baseline::Hold,
        };

        // Destructure the baselines into disjoint `&mut` borrows, one bundle per section. This is
        // what lets the sections run concurrently without a lock: each borrow names different fields
        // of the same struct, so the compiler proves the disjointness the partition claims.
        let PublishState {
            tiles: tiles_baseline,
            culture_layers: culture_layers_baseline,
            culture_tensions: culture_tensions_baseline,
            power: power_baseline,
            power_metrics: power_metrics_baseline,
            terrain_overlay,
            moisture_raster,
            elevation_overlay,
            climate_bands,
            logistics_raster,
            sentiment_raster,
            corruption_raster,
            culture_raster,
            military_raster,
            visibility_raster,
            knowledge_ledger,
            knowledge_metrics,
            knowledge_timeline,
            discovery_progress,
            great_discoveries,
            great_discovery_progress,
            great_discovery_definitions,
            great_discovery_telemetry,
            crisis_telemetry,
            crisis_overlay,
            victory,
            campaign_profiles,
            command_events,
            pending_forks,
            stance_axes,
            voice_medium,
            faction_inventory,
            sedentarization,
            discovered_sites,
            demographics,
            intensification_knowledge,
            start_marker,
            herds,
            forage_patches,
            food_modules,
            logistics,
            trade_links,
            populations,
            generations,
            influencers,
            axis_bias,
            sentiment,
            corruption,
            capability_flags,
            fog_enabled,
            ..
        } = &mut *self;

        let mut tile_parts = TileParts::default();
        let mut culture_parts = CultureParts::default();
        let mut power_parts = PowerParts::default();
        let mut raster_parts = RasterParts::default();
        let mut knowledge_parts = KnowledgeParts::default();
        let mut crisis_parts = CrisisParts::default();
        let mut campaign_parts = CampaignParts::default();
        let mut subsistence_parts = SubsistenceParts::default();
        let mut people_parts = PeopleParts::default();

        // The one scope over the whole fan-out, opened and closed on the publisher thread — see the
        // section registry above for why no task may open one of its own.
        let diff_scope = crate::turn_profile::publish_scope("publish.diff");
        let captured = &snapshot;
        in_diff_pool(|| {
            rayon::scope(|scope| {
                scope.spawn(|_| tile_parts = diff_tiles(tiles_baseline, captured, write));
                scope.spawn(|_| {
                    culture_parts = diff_culture(
                        culture_layers_baseline,
                        culture_tensions_baseline,
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    power_parts =
                        diff_power(power_baseline, power_metrics_baseline, captured, write)
                });
                scope.spawn(|_| {
                    raster_parts = diff_rasters(
                        RasterBaselines {
                            terrain: terrain_overlay,
                            moisture: moisture_raster,
                            elevation: elevation_overlay,
                            climate_bands,
                            logistics: logistics_raster,
                            sentiment: sentiment_raster,
                            corruption: corruption_raster,
                            culture: culture_raster,
                            military: military_raster,
                            visibility: visibility_raster,
                        },
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    knowledge_parts = diff_knowledge(
                        KnowledgeBaselines {
                            ledger: knowledge_ledger,
                            metrics: knowledge_metrics,
                            timeline: knowledge_timeline,
                            discovery_progress,
                            great_discoveries,
                            great_discovery_progress,
                            great_discovery_definitions,
                            great_discovery_telemetry,
                        },
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    crisis_parts =
                        diff_crisis(crisis_telemetry, crisis_overlay, victory, captured, write)
                });
                scope.spawn(|_| {
                    campaign_parts = diff_campaign(
                        CampaignBaselines {
                            profiles: campaign_profiles,
                            command_events,
                            pending_forks,
                            stance_axes,
                            voice_medium,
                            faction_inventory,
                            sedentarization,
                            discovered_sites,
                            demographics,
                            intensification_knowledge,
                            start_marker,
                        },
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    subsistence_parts =
                        diff_subsistence(herds, forage_patches, food_modules, captured, write)
                });
                scope.spawn(|_| {
                    people_parts = diff_people(
                        PeopleBaselines {
                            logistics,
                            trade_links,
                            populations,
                            generations,
                            influencers,
                            axis_bias,
                            sentiment,
                            corruption,
                            capability_flags,
                        },
                        captured,
                        write,
                    )
                });
            });
        });

        // Carried on EVERY delta rather than diffed (the wire default is `true`, so an omitted value
        // would silently re-enable fog one delta after it was turned off), but the baseline still
        // tracks it for the auxiliary feed deltas.
        if write == Baseline::Advance {
            *fog_enabled = snapshot.fog_enabled;
        }

        // The wire layout is the `.fbs` schema's; the order here is grouped by section so the
        // assembly reads as the inverse of the fan-out.
        let delta = WorldDelta {
            header: snapshot.header.clone(),
            tiles: tile_parts.sent,
            removed_tiles: tile_parts.removed,
            culture_layers: culture_parts.sent,
            removed_culture_layers: culture_parts.removed,
            culture_tensions: culture_parts.tensions,
            power: power_parts.sent,
            removed_power: power_parts.removed,
            power_metrics: power_parts.metrics,
            terrain: raster_parts.terrain,
            moisture_raster: raster_parts.moisture,
            elevation_overlay: raster_parts.elevation,
            climate_bands: raster_parts.climate_bands,
            logistics_raster: raster_parts.logistics,
            sentiment_raster: raster_parts.sentiment,
            corruption_raster: raster_parts.corruption,
            culture_raster: raster_parts.culture,
            military_raster: raster_parts.military,
            visibility_raster: raster_parts.visibility,
            knowledge_ledger: knowledge_parts.ledger,
            removed_knowledge_ledger: knowledge_parts.removed_ledger,
            knowledge_metrics: knowledge_parts.metrics,
            knowledge_timeline: knowledge_parts.timeline,
            discovery_progress: knowledge_parts.discovery_progress,
            great_discoveries: knowledge_parts.great_discoveries,
            great_discovery_progress: knowledge_parts.great_discovery_progress,
            great_discovery_definitions: knowledge_parts.great_discovery_definitions,
            great_discovery_telemetry: knowledge_parts.great_discovery_telemetry,
            crisis_telemetry: crisis_parts.telemetry,
            crisis_overlay: crisis_parts.overlay,
            victory: crisis_parts.victory,
            campaign_profiles: campaign_parts.profiles,
            command_events: campaign_parts.command_events,
            pending_forks: campaign_parts.pending_forks,
            stance_axes: campaign_parts.stance_axes,
            voice_medium: campaign_parts.voice_medium,
            faction_inventory: campaign_parts.faction_inventory,
            sedentarization: campaign_parts.sedentarization,
            discovered_sites: campaign_parts.discovered_sites,
            demographics: campaign_parts.demographics,
            intensification_knowledge: campaign_parts.intensification_knowledge,
            start_marker: campaign_parts.start_marker,
            herds: subsistence_parts.herds,
            forage_patches: subsistence_parts.forage_patches,
            food_modules: subsistence_parts.food_modules,
            logistics: people_parts.logistics,
            removed_logistics: people_parts.removed_logistics,
            trade_links: people_parts.trade_links,
            removed_trade_links: people_parts.removed_trade_links,
            populations: people_parts.populations,
            removed_populations: people_parts.removed_populations,
            generations: people_parts.generations,
            removed_generations: people_parts.removed_generations,
            influencers: people_parts.influencers,
            removed_influencers: people_parts.removed_influencers,
            axis_bias: people_parts.axis_bias,
            sentiment: people_parts.sentiment,
            corruption: people_parts.corruption,
            capability_flags: people_parts.capability_flags,
            fog_enabled: snapshot.fog_enabled,
        };

        drop(diff_scope);

        // Claim this frame's place in the publication sequence. `base_frame_seq == 0` means no
        // frame has been published for this world yet, so this turn is the BASELINE and must go
        // out as a full snapshot — a first-turn delta is not equivalent to one, because a field
        // that happens to equal its default compares unchanged and is never sent.
        let (frame_seq, base_frame_seq) = self.next_publication();
        let first_publication = base_frame_seq == 0;
        let mut snapshot = snapshot;
        snapshot.header.frame_seq = frame_seq;
        let mut delta = delta;
        delta.header.frame_seq = frame_seq;
        delta.header.base_frame_seq = base_frame_seq;
        let snapshot_arc = Arc::new(snapshot);
        let delta_arc = Arc::new(delta);
        let stored =
            StoredSnapshot::new(snapshot_arc.clone(), delta_arc.clone(), first_publication);
        let encoded_delta_flat = {
            let _s = crate::turn_profile::publish_scope("publish.encode.flat_delta");
            Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()))
        };

        if kind == Publication::Recapture {
            // Re-baseline the ring's CURRENT entry so a rollback to this tick restores the
            // post-command world, then stop: no baseline commit, no new ring entry.
            self.last_snapshot = Some(snapshot_arc);
            self.last_delta = Some(delta_arc);
            self.encoded_snapshot_flat = None;
            self.encoded_delta_flat = Some(encoded_delta_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = stored.snapshot.clone();
            }
            return Some(encoded_delta_flat);
        }

        // Every baseline this frame advances was advanced in place by the fan-out above, under
        // `Baseline::Advance`. What is left here is the publication bookkeeping.
        self.last_snapshot = Some(snapshot_arc);
        self.last_delta = Some(delta_arc);
        // `None` on every turn but the world's first, which is what makes `broadcast_latest`'s
        // "full frame if there is one, else the delta" a statement of the publication rule rather
        // than a preference — and what stops a later turn re-broadcasting a stale full snapshot.
        self.encoded_snapshot_flat = stored.encoded_snapshot_flat.clone();
        self.encoded_delta_flat = Some(encoded_delta_flat.clone());
        self.history.push_back(stored);
        self.prune();

        // The publication rule: a world's first frame goes out whole, every later one as a delta.
        Some(
            self.encoded_snapshot_flat
                .clone()
                .unwrap_or(encoded_delta_flat),
        )
    }

    /// Claim the next publication sequence, returning `(frame_seq, base_frame_seq)`.
    ///
    /// Counts **publications, not ticks**: `recapture_and_broadcast` publishes mid-tick on every
    /// world-mutating command, so several frames share a tick and tick-continuity could not detect
    /// a gap (`docs/plan_delta_streaming.md` §3.1).
    fn next_publication(&mut self) -> (u64, u64) {
        let base = self.frame_seq;
        self.frame_seq = base + 1;
        (self.frame_seq, base)
    }

    pub(crate) fn reset_to_entry(&mut self, entry: &StoredSnapshot) {
        self.tiles = entry
            .snapshot
            .tiles
            .iter()
            .map(|state| (state.entity, state.clone()))
            .collect();
        self.logistics = entry
            .snapshot
            .logistics
            .iter()
            .map(|state| (state.entity, state.clone()))
            .collect();
        self.populations = entry
            .snapshot
            .populations
            .iter()
            .map(|state| (state.entity, state.clone()))
            .collect();
        self.power = entry
            .snapshot
            .power
            .iter()
            .map(|state| (state.entity, state.clone()))
            .collect();
        self.generations = entry
            .snapshot
            .generations
            .iter()
            .map(|state| (state.id, state.clone()))
            .collect();
        self.influencers = entry
            .snapshot
            .influencers
            .iter()
            .map(|state| (state.id, state.clone()))
            .collect();
        self.culture_layers = entry
            .snapshot
            .culture_layers
            .iter()
            .map(|state| (state.id, state.clone()))
            .collect();
        self.corruption = entry.snapshot.corruption.clone();
        self.axis_bias = entry.snapshot.axis_bias.clone();
        self.sentiment = entry.snapshot.sentiment.clone();
        self.terrain_overlay = entry.snapshot.terrain.clone();
        self.logistics_raster = entry.snapshot.logistics_raster.clone();
        self.sentiment_raster = entry.snapshot.sentiment_raster.clone();
        self.corruption_raster = entry.snapshot.corruption_raster.clone();
        self.visibility_raster = entry.snapshot.visibility_raster.clone();
        self.fog_enabled = entry.snapshot.fog_enabled;
        self.culture_raster = entry.snapshot.culture_raster.clone();
        self.military_raster = entry.snapshot.military_raster.clone();
        self.moisture_raster = entry.snapshot.moisture_raster.clone();
        self.culture_tensions = entry.snapshot.culture_tensions.clone();
        self.discovery_progress = entry
            .snapshot
            .discovery_progress
            .iter()
            .map(|state| ((state.faction, state.discovery), state.clone()))
            .collect();
        self.victory = entry.snapshot.victory.clone();
        self.faction_inventory = entry.snapshot.faction_inventory.clone();
        self.sedentarization = entry.snapshot.sedentarization.clone();
        self.discovered_sites = entry.snapshot.discovered_sites.clone();
        self.demographics = entry.snapshot.demographics.clone();
        self.forage_patches = entry.snapshot.forage_patches.clone();
        self.intensification_knowledge = entry.snapshot.intensification_knowledge.clone();
        self.campaign_profiles = entry.snapshot.campaign_profiles.clone();
        self.command_events = entry.snapshot.command_events.clone();
        self.pending_forks = entry.snapshot.pending_forks.clone();
        self.stance_axes = entry.snapshot.stance_axes.clone();
        self.voice_medium = entry.snapshot.voice_medium.clone();
        self.herds = entry.snapshot.herds.clone();
        self.food_modules = entry.snapshot.food_modules.clone();
        self.great_discoveries = entry
            .snapshot
            .great_discoveries
            .iter()
            .map(|state| ((state.faction, state.id), state.clone()))
            .collect();
        self.great_discovery_progress = entry
            .snapshot
            .great_discovery_progress
            .iter()
            .map(|state| ((state.faction, state.discovery), state.clone()))
            .collect();
        self.great_discovery_telemetry = entry.snapshot.great_discovery_telemetry.clone();
        self.knowledge_ledger = entry
            .snapshot
            .knowledge_ledger
            .iter()
            .map(|state| {
                (
                    encode_ledger_key(FactionId(state.owner_faction), state.discovery_id),
                    state.clone(),
                )
            })
            .collect();
        self.knowledge_metrics = entry.snapshot.knowledge_metrics.clone();
        self.knowledge_timeline = entry.snapshot.knowledge_timeline.clone();
        self.crisis_telemetry = entry.snapshot.crisis_telemetry.clone();
        self.crisis_overlay = entry.snapshot.crisis_overlay.clone();
        self.elevation_overlay = entry.snapshot.elevation_overlay.clone();
        self.start_marker = entry.snapshot.start_marker.clone();
        self.capability_flags = entry.snapshot.capability_flags;

        self.last_snapshot = Some(entry.snapshot.clone());
        self.last_delta = Some(entry.delta.clone());
        self.encoded_snapshot_flat = entry.encoded_snapshot_flat.clone();
        // Defensive: a delta encoded against the baseline we just rewound past describes a
        // transition that no longer happened, and it names a `base_frame_seq` the client can no
        // longer be holding. No call site can reach it today — every `broadcast_latest` follows a
        // `publish`, which overwrites this — but leaving a frame here that is only safe because of
        // ordering elsewhere is a trap, and dropping it is free.
        self.encoded_delta_flat = None;

        while let Some(back) = self.history.back() {
            if back.tick > entry.tick {
                self.history.pop_back();
            } else {
                break;
            }
        }
    }

    /// Publish `entry` as a full flat frame, **claiming a fresh publication sequence number** for it
    /// exactly as any other publication does.
    ///
    /// Two callers, and both re-baseline the client on a whole world: **rollback** (the world moved
    /// backwards) and **`Command::Resync`** (the client could not apply a delta and asked for a
    /// complete world instead).
    ///
    /// **Neither may reuse a number stored on the ring entry.** The counter is deliberately never
    /// rewound — it numbers publications, not ticks, and replaying a number would make two different
    /// frames indistinguishable to the client. A frame stamped with a *stale* number leaves the
    /// client baselined behind the server: the next `next_publication` names the current number as
    /// its base, `WorldCache::accepts` rejects that delta, and the client asks for a resync. Stamping
    /// a live number makes the client's applied seq equal the server's current one, which is exactly
    /// what the next delta's `base_frame_seq` will name.
    ///
    /// It matters most on the **resync** path, because resync is the *recovery* path: a resync answer
    /// carrying a stale number opens the very sequence gap it was sent to close, and the client can
    /// only heal once some later publication refreshes the ring entry. The entry's stored numbers go
    /// stale in two ways — a mid-tick recapture refreshes `history.back().snapshot` but **not** its
    /// cached `encoded_snapshot_flat`, and an auxiliary delta (`update_axis_bias` and friends) claims
    /// a sequence number without touching the ring at all.
    ///
    /// `base_frame_seq` stays `0`: a full snapshot names no base, matching the baseline path.
    ///
    /// The stored `encoded_snapshot_flat`, if the entry had one, cannot be reused — its header
    /// carries the old number — so this always re-encodes.
    pub(crate) fn publish_full_frame(&mut self, entry: &StoredSnapshot) -> Arc<Vec<u8>> {
        let (frame_seq, _base_frame_seq) = self.next_publication();
        let mut snapshot = entry.snapshot.as_ref().clone();
        snapshot.header.frame_seq = frame_seq;
        snapshot.header.base_frame_seq = 0;
        // No profiling scope, deliberately: this runs INLINE on the caller's thread (rollback and
        // `Resync` both hold the publisher's queue drained), and a `publish_scope` there would
        // accumulate into a thread-local nothing ever drains. Both call sites already log the
        // frame's size and tick.
        let encoded = Arc::new(encode_snapshot_flatbuffer(&snapshot));
        // This IS the latest flat frame for the world now, and it is the only encoding of it that
        // carries the live sequence number — so anything that later reaches for "the newest full
        // frame" (`broadcast_latest`) sends this one rather than the entry's stale-seq bytes.
        self.last_snapshot = Some(Arc::new(snapshot));
        self.encoded_snapshot_flat = Some(encoded.clone());
        encoded
    }

    pub(crate) fn update_axis_bias(&mut self, bias: AxisBiasState) -> Option<Arc<Vec<u8>>> {
        if self.axis_bias == bias {
            return None;
        }

        self.axis_bias = bias.clone();

        let header = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.header.clone())
            .unwrap_or_default();

        let delta = WorldDelta {
            header,
            tiles: Vec::new(),
            removed_tiles: Vec::new(),
            logistics: Vec::new(),
            removed_logistics: Vec::new(),
            trade_links: Vec::new(),
            removed_trade_links: Vec::new(),
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            power_metrics: None,
            great_discovery_definitions: None,
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: None,
            knowledge_ledger: Vec::new(),
            removed_knowledge_ledger: Vec::new(),
            knowledge_metrics: None,
            victory: None,
            capability_flags: None,
            campaign_profiles: None,
            command_events: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: Some(bias.clone()),
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            culture_raster: None,
            military_raster: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: None,
            influencers: Vec::new(),
            removed_influencers: Vec::new(),
            terrain: None,
            culture_layers: Vec::new(),
            removed_culture_layers: Vec::new(),
            culture_tensions: None,
            discovery_progress: Vec::new(),
            visibility_raster: None,
            fog_enabled: self.fog_enabled,
        };

        // An on-demand feed frame is a PUBLICATION like any other, so it takes the next sequence
        // number. Skipping it would leave the client's `base_frame_seq` check comparing against a
        // frame it never saw and rejecting every subsequent turn delta.
        let (frame_seq, base_frame_seq) = self.next_publication();
        let mut delta = delta;
        delta.header.frame_seq = frame_seq;
        delta.header.base_frame_seq = base_frame_seq;
        let delta_arc = Arc::new(delta);
        // Built for the caller to broadcast immediately and deliberately NOT stored: a ring entry
        // holding a delta nobody re-reads is the per-turn encode this arc removed, re-added by the
        // side door (see `StoredSnapshot::new`).
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.axis_bias = bias.clone();
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc;
                back.encoded_snapshot_flat = Some(encoded_snapshot_flat);
            }
        }

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
        }

        Some(encoded_delta_flat)
    }

    /// Publish an auxiliary **influencer** frame: a delta carrying only the roster change, plus a
    /// refresh of `last_snapshot` and the back ring entry so a rollback to this tick restores it.
    ///
    /// Like its siblings ([`Self::update_axis_bias`], [`Self::update_corruption`]) it does **not**
    /// push a new ring entry or touch the delta baselines (`self.tiles`/`populations`/…), so the
    /// ring stays one-entry-per-tick and the next turn's delta re-sends the change harmlessly.
    /// Never advances the turn or the `TurnQueue`.
    pub(crate) fn update_influencers(
        &mut self,
        states: Vec<InfluentialIndividualState>,
    ) -> Option<Arc<Vec<u8>>> {
        // The influencer baseline IS advanced here — it is this frame's whole subject — while every
        // other baseline is left alone, which is what the doc comment above means by not touching
        // them. Nothing to publish when the roster did not move.
        let (added, removed) = diff_new(
            &mut self.influencers,
            &states,
            |state| state.id,
            Baseline::Advance,
        );
        if added.is_empty() && removed.is_empty() {
            return None;
        }

        let mut header = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.header.clone())
            .unwrap_or_default();
        header.influencer_count = states.len() as u32;

        let delta = WorldDelta {
            header,
            tiles: Vec::new(),
            removed_tiles: Vec::new(),
            logistics: Vec::new(),
            removed_logistics: Vec::new(),
            trade_links: Vec::new(),
            removed_trade_links: Vec::new(),
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            power_metrics: None,
            great_discovery_definitions: None,
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: None,
            knowledge_ledger: Vec::new(),
            removed_knowledge_ledger: Vec::new(),
            knowledge_metrics: None,
            victory: None,
            capability_flags: None,
            campaign_profiles: None,
            command_events: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: None,
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            culture_raster: None,
            military_raster: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: None,
            influencers: added,
            removed_influencers: removed,
            terrain: None,
            culture_layers: Vec::new(),
            removed_culture_layers: Vec::new(),
            culture_tensions: None,
            discovery_progress: Vec::new(),
            visibility_raster: None,
            fog_enabled: self.fog_enabled,
        };

        // An on-demand feed frame is a PUBLICATION like any other, so it takes the next sequence
        // number. Skipping it would leave the client's `base_frame_seq` check comparing against a
        // frame it never saw and rejecting every subsequent turn delta.
        let (frame_seq, base_frame_seq) = self.next_publication();
        let mut delta = delta;
        delta.header.frame_seq = frame_seq;
        delta.header.base_frame_seq = base_frame_seq;
        let delta_arc = Arc::new(delta);
        // Built for the caller to broadcast immediately and deliberately NOT stored: a ring entry
        // holding a delta nobody re-reads is the per-turn encode this arc removed, re-added by the
        // side door (see `StoredSnapshot::new`).
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.influencers = states.clone();
            snapshot.header.influencer_count = states.len() as u32;
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc.clone();
                back.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            }
        }

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
        }

        Some(encoded_delta_flat)
    }

    pub(crate) fn update_corruption(&mut self, ledger: CorruptionLedger) -> Option<Arc<Vec<u8>>> {
        if self.corruption == ledger {
            return None;
        }

        self.corruption = ledger.clone();

        let header = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.header.clone())
            .unwrap_or_default();

        let delta = WorldDelta {
            header,
            tiles: Vec::new(),
            removed_tiles: Vec::new(),
            logistics: Vec::new(),
            removed_logistics: Vec::new(),
            trade_links: Vec::new(),
            removed_trade_links: Vec::new(),
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            power_metrics: None,
            great_discovery_definitions: None,
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: None,
            knowledge_ledger: Vec::new(),
            removed_knowledge_ledger: Vec::new(),
            knowledge_metrics: None,
            victory: None,
            capability_flags: None,
            campaign_profiles: None,
            command_events: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: None,
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            culture_raster: None,
            military_raster: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: Some(ledger.clone()),
            influencers: Vec::new(),
            removed_influencers: Vec::new(),
            terrain: None,
            culture_layers: Vec::new(),
            removed_culture_layers: Vec::new(),
            culture_tensions: None,
            discovery_progress: Vec::new(),
            visibility_raster: None,
            fog_enabled: self.fog_enabled,
        };

        // An on-demand feed frame is a PUBLICATION like any other, so it takes the next sequence
        // number. Skipping it would leave the client's `base_frame_seq` check comparing against a
        // frame it never saw and rejecting every subsequent turn delta.
        let (frame_seq, base_frame_seq) = self.next_publication();
        let mut delta = delta;
        delta.header.frame_seq = frame_seq;
        delta.header.base_frame_seq = base_frame_seq;
        let delta_arc = Arc::new(delta);
        // Built for the caller to broadcast immediately and deliberately NOT stored: a ring entry
        // holding a delta nobody re-reads is the per-turn encode this arc removed, re-added by the
        // side door (see `StoredSnapshot::new`).
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.corruption = ledger.clone();
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc.clone();
                back.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            }
        }

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
        }

        Some(encoded_delta_flat)
    }

    fn prune(&mut self) {
        while self.history.len() > self.capacity {
            self.history.pop_front();
        }
    }
}

pub(crate) type PopulationSnapshotQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PopulationCohort,
        Option<&'static LaborAllocation>,
        Option<&'static BandTravel>,
        Option<&'static Expedition>,
    ),
>;

/// Every tile's terrain tags, indexed by grid position rather than hashed — the fresh-water rule
/// (`forage::tile_is_fresh_watered`) reads a tile's SIX neighbours, so this is ~6 lookups per patch
/// tile and a `HashMap` probe per neighbour was most of what the refusal sweep cost.
///
/// The cell stays `Option<TerrainTags>`: "no tile there" and "a tile carrying no tags" are different
/// readings, and this capture does not conflate absent with zero (the `seasonal_weights` convention).
struct TerrainTagGrid {
    width: u32,
    tags: Vec<Option<sim_runtime::TerrainTags>>,
}

impl TerrainTagGrid {
    /// Sized from the world's own `grid_size`, so it is exactly one cell per tile the query can yield.
    fn new(grid_size: UVec2) -> Self {
        Self {
            width: grid_size.x,
            tags: vec![None; (grid_size.x as usize) * (grid_size.y as usize)],
        }
    }

    fn index(&self, pos: UVec2) -> Option<usize> {
        if pos.x >= self.width {
            return None;
        }
        let index = (pos.y as usize) * (self.width as usize) + (pos.x as usize);
        (index < self.tags.len()).then_some(index)
    }

    fn set(&mut self, pos: UVec2, tags: sim_runtime::TerrainTags) {
        if let Some(index) = self.index(pos) {
            self.tags[index] = Some(tags);
        }
    }

    /// `None` for an off-map coord as well as for an unwritten cell — a neighbour walk runs off the
    /// edge of a non-wrapping map, and that must read as "no tile", not panic. A tile that simply
    /// carries no tags is `Some(TerrainTags::empty())`, never `None`; that is the whole point of the
    /// `Option`.
    fn get(&self, pos: UVec2) -> Option<sim_runtime::TerrainTags> {
        self.index(pos).and_then(|index| self.tags[index])
    }
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn capture_snapshot(
    ctx: SnapshotContext,
    tiles: Query<(Entity, &Tile, Option<&FoodModuleTag>)>,
    logistics_links: Query<(Entity, &LogisticsLink, &TradeLink)>,
    populations: PopulationSnapshotQuery,
    power_nodes: Query<(Entity, &PowerNode)>,
    power_grid: Res<PowerGridState>,
    knowledge_ledger: Res<KnowledgeLedger>,
    registry: Res<GenerationRegistry>,
    roster: Res<InfluentialRoster>,
    axis_bias: Res<SentimentAxisBias>,
    corruption_ledgers: Res<CorruptionLedgers>,
    corruption_telemetry: Res<CorruptionTelemetry>,
    discovery_progress: Res<DiscoveryProgressLedger>,
    gds: GreatDiscoverySnapshotParam,
    culture: Res<CultureManager>,
    mut history: ResMut<SnapshotHistory>,
) {
    // Whole-capture profiling. `snapshot.build` covers assembling the `WorldSnapshot` and CONTAINS
    // its `snapshot.build.*` sub-scopes — the profiler's labels nest flat, so a parent includes its
    // children (see `crate::turn_profile`). Hashing, history diffing and encoding are separate
    // top-level labels below.
    let build_scope = crate::turn_profile::scope("snapshot.build");
    let SnapshotContext {
        config,
        tick,
        world_epoch,
        overlays,
        metrics,
        crisis_overlay,
        start_location,
        herds,
        herd_registry,
        forage_registry,
        graze_registry,
        beat_ledger,
        elevation,
        moisture,
        map_presets: _,
        campaign_label,
        start_profiles,
        victory,
        faction_inventory,
        sedentarization,
        discovered_sites,
        sites_config,
        food_sites,
        command_events,
        capability_flags,
        visibility_ledger,
        viewer_faction,
        demographics,
        wellbeing,
        labor,
        flora,
        ladder,
        fauna,
        expedition,
        settlement_stage,
        supply_membership,
        pipeline_config,
        capture_mode,
    } = ctx;
    let overlays_config = overlays.get();

    let population_cfg = pipeline_config.config().population();
    // Same place-based morale config the sim uses, so `habitability` matches the applied drain.
    let morale_pressure_cfg = MoralePressureConfig {
        ambient_temperature: config.ambient_temperature,
        temperature_morale_penalty: config.temperature_morale_penalty,
        temperature_morale_tolerance: config.temperature_morale_tolerance,
        attrition_penalty_scale: population_cfg.attrition_penalty_scale(),
        hardness_penalty_scale: population_cfg.hardness_penalty_scale(),
    };

    // Forage potential (per-tile) is read from the biome table here, so the labor config is resolved
    // ahead of the tile loop (it is reused for the labor/expedition readouts further down).
    let labor_config = labor.get();
    let flora_config = flora.get();
    let ladder_config = ladder.get();
    let mut tile_states: Vec<TileState> = Vec::new();
    let mut food_module_states: Vec<FoodModuleState> = Vec::new();
    let start_position = start_location.position();
    let stockpile_radius = config
        .start_profile_overrides
        .stockpile_access_radius
        .unwrap_or(DEFAULT_STOCKPILE_ACCESS_RADIUS);
    // Per-tile seasonal gather weight, keyed by coord — the same `FoodModuleTag::seasonal_weight` the
    // Forage arm of `advance_labor_allocation` folds into `forage_take`'s worker cap. The forage
    // patch forecast (below) needs it to report a per-worker yield that matches what the sim pays.
    let mut seasonal_weights: HashMap<UVec2, f32> = HashMap::new();
    // Per-tile terrain tags, grid-indexed — what the fresh-water half of the `plant:field` rung's
    // site rule reads about a tile's NEIGHBOURS (`forage::tile_is_fresh_watered`). Collected in this
    // pass so the patch sweep below reads a finished grid rather than walking the world again.
    let mut tile_tags = TerrainTagGrid::new(config.grid_size);
    // The patch tiles, picked out of the one full sweep — the subset BOTH readouts below are about,
    // so `forage_registry.patch()` is asked once per tile instead of once per readout. Borrowing out
    // of `tiles.iter()` is sound here: the query is read-only and outlives every use of this vec.
    let mut patch_tiles: Vec<&Tile> = Vec::new();
    {
        // Sweep 1 of 2 — the ONLY walk of the full tile query.
        let _s = crate::turn_profile::scope("snapshot.build.tiles");
        for (entity, tile, food_module) in tiles.iter() {
            tile_states.push(tile_state(
                entity,
                tile,
                &morale_pressure_cfg,
                graze_registry.patch(tile.position),
                &labor_config.forage,
            ));
            tile_tags.set(tile.position, tile.terrain_tags);
            if let Some(module) = food_module {
                seasonal_weights.insert(tile.position, module.seasonal_weight);
            }
            if forage_registry.patch(tile.position).is_some() {
                patch_tiles.push(tile);
            }
        }
    }
    // Sweep 2 of 2 — the patch tiles only, both per-patch readouts built in ONE walk of the subset
    // sweep 1 picked out. It cannot fold into sweep 1: `tile_is_fresh_watered` reads a tile's
    // NEIGHBOURS' tags, so it needs the finished `tile_tags` grid. Held as an explicit guard rather
    // than a wrapping block so the (long) composition closure below keeps its indentation.
    //
    // **Why the ground under each patch will not take seed** (`sow_site_refusals`) — the
    // `plant:field` rung's own `site_requirement`, resolved through the SAME
    // `RungSiteRequirement::refusal` seam the `sow` command (`validate_sow`) and the labor arm's
    // placement gate use, so the wire, the rejection and the sim can never disagree about which
    // ground is farmable. Only refusals are stored: a coord absent from the map is ground that takes
    // seed (the `seasonal_weights` convention).
    //
    // **What grows on each patch tile** (`flora_compositions`) — the named plants its forage capacity
    // decomposes into, resolved through the ONE `forage::tile_flora_composition` seam (the twin of
    // `tile_forage_capacity`), so a navigable hex's *two* capacity terms are both named and the wire
    // cannot disagree with the table.
    //
    // Patches only, for both — the client asks "why can't I sow *here*?" / "what grows here?" of a
    // tile it is looking at, and a patch is on every food-bearing tile there is (see
    // core_sim/CLAUDE.md → the Field).
    let patches_scope = crate::turn_profile::scope("snapshot.build.patches");
    let field_rung = ladder_config.rung(RungKey::PlantField);
    let grid = config.grid_size;
    let wrap_horizontal = config.map_topology.wrap_horizontal;
    let mut sow_site_refusals: HashMap<UVec2, SiteRefusal> = HashMap::new();
    let mut flora_compositions: HashMap<UVec2, Vec<FloraShareInfo>> = HashMap::new();
    for tile in patch_tiles {
        let fresh_water = tile_is_fresh_watered(tile, grid.x, grid.y, wrap_horizontal, |coord| {
            tile_tags.get(coord)
        });
        if let Some(refusal) =
            rung_site_refusal(field_rung, tile, &labor_config.forage, fresh_water)
        {
            sow_site_refusals.insert(tile.position, refusal);
        }
        // The quotes below are taken against **this tile's own `K`** — never the live patch's,
        // which may already be concentrated by an existing commitment — and at the standing crop
        // each rung *settles* at, so they answer "what would this ground pay once this crop is
        // established here" rather than pricing a 25-turn investment off one transient turn.
        let tile_capacity = tile_forage_capacity(&labor_config.forage, tile);
        // What this tile pays left wild — the denominator every ratio on this tile divides by,
        // resolved once.
        let wild = wild_payoff(
            tile.position,
            tile_capacity,
            &flora_config,
            &labor_config.forage,
            FORECAST_OUTPUT_MULTIPLIER,
        );
        let shares =
            tile_flora_composition(&flora_config, &labor_config.forage, tile, config.map_seed)
                .iter()
                .map(|share| {
                    let def = &flora_config.species[&share.species];
                    // **What this tile would pay per turn once committed to THIS plant**, per rung —
                    // through `forage::commit_payoff`, which builds the patch the sim would have and
                    // asks the *same* payoff functions the sim quotes and pays each rung with
                    // (`tended_provisions` / `field_provisions`). Nothing is re-derived here, which
                    // is what stops the published number and the payout from drifting.
                    let payoff = |rung| {
                        commit_payoff(
                            tile.position,
                            tile_capacity,
                            &share.species,
                            share.share,
                            &flora_config,
                            &labor_config.forage,
                            FORECAST_OUTPUT_MULTIPLIER,
                            rung,
                        )
                    };
                    let cultivate = payoff(RungKey::PlantTended);
                    let sow = payoff(RungKey::PlantField);
                    FloraShareInfo {
                        species: share.species.clone(),
                        display_name: def.display_name.clone(),
                        share: share.share,
                        // **Which rungs this plant can EVER climb** (Flora Roster S1) — its own
                        // `cultivation_ceiling`, straight off the roster, so the client's crop
                        // picker can grey out what is impossible without holding a roster of its
                        // own. Species-global: it says nothing about whether this tile is a good
                        // place for it — the payoff/ratio below answer that, and a legal-but-marginal
                        // crop is exactly the loss §4.3 leaves the player free to choose.
                        can_cultivate: def.cultivation_ceiling.allows_cultivate(),
                        can_sow: def.cultivation_ceiling.allows_sow(),
                        cultivate_payoff: cultivate,
                        sow_payoff: sow,
                        // **Is it worth it?** — the same payoffs over the same wild payoff, so the
                        // ratio can never disagree with the numbers it relates.
                        cultivate_yield_ratio: commit_yield_ratio(cultivate, wild),
                        sow_yield_ratio: commit_yield_ratio(sow, wild),
                        // **What a hay Field of this plant would pay into the FODDER account** (F3) —
                        // through the same `commit_fodder_payoff` seam the sim's `field_fodder` pays
                        // with, so the picker can show hay's value where `sow_yield_ratio` reads 0×.
                        // `0` for a staple (no fodder in its vector) or a plant that cannot Sow here.
                        sow_fodder_payoff: commit_fodder_payoff(
                            tile.position,
                            tile_capacity,
                            &share.species,
                            share.share,
                            &flora_config,
                            &labor_config.forage,
                            FORECAST_OUTPUT_MULTIPLIER,
                        ),
                        // **What a cash-crop Field of this plant would pay into the TRADE account**
                        // (F4) — the exact trade twin, through the same `commit_trade_payoff` seam
                        // the sim's `field_trade_goods` pays with, so the picker can show a cash
                        // crop's value where `sow_yield_ratio` reads 0×. `0` for a staple/hay or a
                        // plant that cannot Sow here.
                        sow_trade_payoff: commit_trade_payoff(
                            tile.position,
                            tile_capacity,
                            &share.species,
                            share.share,
                            &flora_config,
                            &labor_config.forage,
                            FORECAST_OUTPUT_MULTIPLIER,
                        ),
                    }
                })
                .collect();
        flora_compositions.insert(tile.position, shares);
    }
    drop(patches_scope);

    for site in food_sites.sites() {
        food_module_states.push(FoodModuleState {
            x: site.position.x,
            y: site.position.y,
            module: site.module.as_str().to_string(),
            kind: site.kind.as_str().to_string(),
            seasonal_weight: site.seasonal_weight,
        });
    }
    for tile in tile_states.iter_mut() {
        let owner = CultureOwner(tile.entity);
        if let Some(layer) = culture.local_layer_by_owner(owner) {
            tile.culture_layer = layer.id;
        }
    }
    tile_states.sort_unstable_by_key(|state| state.entity);
    let tile_positions: HashMap<u64, UVec2> = tile_states
        .iter()
        .map(|state| (state.entity, UVec2::new(state.x, state.y)))
        .collect();

    let mut logistics_states: Vec<LogisticsLinkState> = Vec::new();
    let mut trade_states: Vec<TradeLinkState> = Vec::new();
    for (entity, link, trade) in logistics_links.iter() {
        logistics_states.push(logistics_state(entity, link));
        trade_states.push(trade_link_state(entity, link, trade));
    }
    logistics_states.sort_unstable_by_key(|state| state.entity);
    trade_states.sort_unstable_by_key(|state| state.entity);

    let demographics_config = demographics.get();
    let wellbeing_config = wellbeing.get();
    let settlement_stage_config = settlement_stage.get();
    // Global labor config today (identical for every band); the work-range ring is surfaced
    // per-band so the client reads it off the selected band (future-proof if bands diverge).
    let band_work_range = labor_config.band_work_range;
    // Effective hunt reach (= `band_work_range + hunt_leash_tiles`, the leash a Hunt lapses past),
    // echoed per-band so the client offers a local hunt vs a hunting expedition by herd distance.
    let hunt_reach = labor_config.hunt_reach();
    // Expedition levers echoed per-cohort — same idiom as `band_work_range`: global config today,
    // surfaced per-band so the client reads them off the selected band. Populated for EVERY cohort
    // (the outfit UI lives on the resident-band panel, not on the expedition).
    let expedition_cfg = expedition.get();
    let fauna_config = fauna.get();
    let expedition_levers = ExpeditionLevers {
        max_party_size: expedition_cfg.max_party_size,
        hunt_per_worker_carry: expedition_cfg.hunt.per_worker_carry,
        hunt_per_worker_provisions: hunt_per_worker_provisions(&labor_config, &fauna_config),
        hunt_viability_warn_turns: expedition_cfg.hunt.viability_warn_turns,
        band_move_tiles_per_turn: labor_config.band_move_tiles_per_turn,
    };
    // A cohort → live-tile map so an in-flight expedition can find its home band's CURRENT tile
    // (bands are nomadic). The `populations` query is read-only, so iterating it twice is fine.
    let cohort_positions: std::collections::HashMap<Entity, UVec2> = populations
        .iter()
        .filter_map(|(entity, cohort, _, _, _)| {
            tile_positions
                .get(&cohort.current_tile.to_bits())
                .copied()
                .map(|p| (entity, p))
        })
        .collect();
    let mut population_states: Vec<PopulationCohortState> = populations
        .iter()
        .map(|(entity, cohort, allocation, travel, expedition)| {
            let home_pos = tile_positions.get(&cohort.home.to_bits()).copied();
            let current_pos = tile_positions.get(&cohort.current_tile.to_bits()).copied();
            // A band is "traveling" while a `move_band` order is still en route to its target.
            let is_traveling = travel
                .map(|t| current_pos.map(|p| p != t.target).unwrap_or(true))
                .unwrap_or(false);
            // The `BandTravel` destination (for the client's target-hex display); `None` → 0,0.
            let travel_target = travel.map(|t| t.target);
            // Local scout: scouts are now forward observers posting vantage points out from the
            // band. Carry the effective vantage distance (how far the vantage ring is posted, `0`
            // with no scouts), using the same helper the visibility pass applies, so the field
            // stays coherent for the client.
            let scout_workers = allocation
                .map(|alloc| alloc.workers_on(&LaborTarget::Scout))
                .unwrap_or(0);
            let scout_vantage_distance = labor_config.scout.vantage_distance(scout_workers);
            // The in-flight delivery forecast for a live hunting party (`None` for a scout or a
            // normal band). Reuses the raid forward-sim seeded with the party's current haul.
            let expedition_delivery = expedition.and_then(|exp| {
                let party_pos = current_pos?;
                let home_pos = cohort_positions.get(&exp.home_band).copied();
                crate::systems::expedition_delivery(
                    exp,
                    cohort.stores.get(FOOD).to_f32(),
                    available_workers(cohort.working),
                    party_pos,
                    home_pos,
                    &herd_registry,
                    &fauna_config,
                    &labor_config,
                    &expedition_cfg,
                    config.grid_size.x,
                    config.map_topology.wrap_horizontal,
                )
            });
            population_state(PopulationStateInputs {
                entity,
                cohort,
                allocation,
                expedition,
                home_position: home_pos,
                current_position: current_pos,
                is_traveling,
                stockpile_radius,
                start_position,
                inventory: &faction_inventory,
                demographics: &demographics_config,
                wellbeing: &wellbeing_config,
                supply_membership: &supply_membership,
                work_range: band_work_range,
                raid_radius: fauna_config.predators.raid_radius,
                scout_vantage_distance,
                expedition_levers: &expedition_levers,
                settlement_stage_config: &settlement_stage_config,
                travel_target,
                hunt_reach,
                expedition_delivery,
            })
        })
        .collect();
    population_states.sort_unstable_by_key(|state| state.entity);

    let mut power_states: Vec<PowerNodeState> = power_nodes
        .iter()
        .map(|(entity, node)| power_state(entity, node))
        .collect();
    power_states.sort_unstable_by_key(|state| state.entity);

    let power_metrics = power_metrics_from_grid(&power_grid);
    let KnowledgeSnapshotPayload {
        entries: knowledge_ledger_states,
        timeline: knowledge_timeline_states,
        metrics: knowledge_metrics_state,
    } = knowledge_ledger.snapshot_payload();

    let mut generation_states: Vec<GenerationState> =
        registry.profiles().iter().map(generation_state).collect();
    generation_states.sort_unstable_by_key(|state| state.id);

    let mut influencer_states: Vec<InfluentialIndividualState> = roster.states();
    influencer_states.sort_unstable_by_key(|state| state.id);

    let mut culture_layer_states: Vec<CultureLayerState> = Vec::new();
    if let Some(global_layer) = culture.global_layer() {
        culture_layer_states.push(culture_layer_state(global_layer));
    }
    for layer in culture.regional_layers() {
        culture_layer_states.push(culture_layer_state(layer));
    }
    for layer in culture.local_layers() {
        culture_layer_states.push(culture_layer_state(layer));
    }
    culture_layer_states.sort_unstable_by_key(|state| state.id);

    let mut culture_tension_states: Vec<CultureTensionState> = culture
        .active_tensions()
        .into_iter()
        .map(culture_tension_state)
        .collect();
    culture_tension_states.sort_unstable_by(|a, b| {
        (a.layer_id, a.kind as u8, a.timer).cmp(&(b.layer_id, b.kind as u8, b.timer))
    });

    let discovery_states = discovery_progress_entries(&discovery_progress);
    let great_discovery_definition_states = snapshot_definitions(&gds.registry);
    let great_discovery_states = snapshot_discoveries(&gds.ledger);
    let great_discovery_progress_states = snapshot_progress(&gds.readiness);
    let great_discovery_telemetry_state = snapshot_telemetry(&gds.ledger, &gds.telemetry);

    // The contiguous full-grid raster block: terrain, logistics, sentiment, corruption, culture,
    // military, visibility. The moisture/elevation overlays are built further down (they need
    // state assembled in between), so they re-enter this same label there — hence `rasters` reports
    // two calls per capture.
    let raster_scope = crate::turn_profile::scope("snapshot.build.rasters");
    let terrain_overlay = terrain_overlay_from_tiles(&tile_states, config.grid_size);
    let logistics_raster =
        logistics_raster_from_links(&tile_states, &logistics_states, config.grid_size);
    let sentiment_raster =
        sentiment_raster_from_populations(&tile_states, &population_states, config.grid_size);
    let corruption_raster = corruption_raster_from_simulation(CorruptionRasterInputs {
        tiles: &tile_states,
        trade_links: &trade_states,
        populations: &population_states,
        power_nodes: &power_states,
        logistics_raster: &logistics_raster,
        corruption_signals: CorruptionSignals {
            ledger: corruption_ledgers.ledger(),
            telemetry: &corruption_telemetry,
        },
        grid_size: config.grid_size,
        overlays: overlays_config.as_ref(),
    });
    let culture_raster = culture_raster_from_layers(
        &tile_states,
        culture.as_ref(),
        config.grid_size,
        overlays_config.as_ref(),
    );
    let military_raster = military_raster_from_state(
        &tile_states,
        &population_states,
        &power_states,
        &logistics_raster,
        config.grid_size,
        overlays_config.as_ref(),
    );
    let visibility_raster = visibility_raster_from_ledger(
        &visibility_ledger,
        viewer_faction.0,
        config.grid_size,
        config.fog_enabled,
    );
    drop(raster_scope);

    let policy_axes = axis_bias.policy_values();
    let incident_axes = axis_bias.incident_values();
    let influencer_axes = roster.sentiment_totals();
    let combined_axes = axis_bias.combined();

    let policy_raw = policy_axes.map(Scalar::raw);
    let incident_raw = incident_axes.map(Scalar::raw);
    let influencer_raw = influencer_axes.map(Scalar::raw);
    let combined_raw = combined_axes.map(Scalar::raw);

    let mut axis_drivers: [Vec<SentimentDriverState>; 4] = std::array::from_fn(|_| Vec::new());

    for idx in 0..4 {
        let value = policy_raw[idx];
        if value != 0 {
            axis_drivers[idx].push(SentimentDriverState {
                category: SentimentDriverCategory::Policy,
                label: format!("Policy Lever ({})", AXIS_NAMES[idx]),
                value,
                weight: Scalar::one().raw(),
            });
        }
    }

    let mut incident_driver_totals = [0i64; 4];
    for record in corruption_telemetry.exposures_this_turn.iter() {
        if record.trust_delta == 0 {
            continue;
        }
        let idx = 1usize;
        incident_driver_totals[idx] += record.trust_delta;
        axis_drivers[idx].push(SentimentDriverState {
            category: SentimentDriverCategory::Incident,
            label: format!(
                "Corruption Exposure #{} ({:?})",
                record.incident_id, record.subsystem
            ),
            value: record.trust_delta,
            weight: Scalar::one().raw(),
        });
    }

    for idx in 0..4 {
        let remainder = incident_raw[idx] - incident_driver_totals[idx];
        if remainder != 0 {
            axis_drivers[idx].push(SentimentDriverState {
                category: SentimentDriverCategory::Incident,
                label: format!("Incident Carryover ({})", AXIS_NAMES[idx]),
                value: remainder,
                weight: Scalar::one().raw(),
            });
        }
    }

    for state in &influencer_states {
        let contributions = [
            state.sentiment_knowledge,
            state.sentiment_trust,
            state.sentiment_equity,
            state.sentiment_agency,
        ];
        let label_base = influencer_label(state);
        let weight = influencer_driver_weight(state);
        for (idx, value) in contributions.iter().enumerate() {
            if *value == 0 {
                continue;
            }
            axis_drivers[idx].push(SentimentDriverState {
                category: SentimentDriverCategory::Influencer,
                label: format!("{} · {}", label_base, AXIS_NAMES[idx]),
                value: *value,
                weight,
            });
        }
    }

    let mut drivers_iter = axis_drivers.into_iter();
    let knowledge_drivers = drivers_iter.next().unwrap_or_default();
    let trust_drivers = drivers_iter.next().unwrap_or_default();
    let equity_drivers = drivers_iter.next().unwrap_or_default();
    let agency_drivers = drivers_iter.next().unwrap_or_default();

    let sentiment_state = SentimentTelemetryState {
        knowledge: SentimentAxisTelemetry {
            policy: policy_raw[0],
            incidents: incident_raw[0],
            influencers: influencer_raw[0],
            total: combined_raw[0],
            drivers: knowledge_drivers,
        },
        trust: SentimentAxisTelemetry {
            policy: policy_raw[1],
            incidents: incident_raw[1],
            influencers: influencer_raw[1],
            total: combined_raw[1],
            drivers: trust_drivers,
        },
        equity: SentimentAxisTelemetry {
            policy: policy_raw[2],
            incidents: incident_raw[2],
            influencers: influencer_raw[2],
            total: combined_raw[2],
            drivers: equity_drivers,
        },
        agency: SentimentAxisTelemetry {
            policy: policy_raw[3],
            incidents: incident_raw[3],
            influencers: influencer_raw[3],
            total: combined_raw[3],
            drivers: agency_drivers,
        },
    };

    let axis_bias_state = axis_bias_state_from_resource(&axis_bias);
    let crisis_telemetry_state = crisis_telemetry_state_from_metrics(&metrics.crisis);
    let crisis_overlay_state = CrisisOverlayState {
        heatmap: crisis_overlay.raster.clone(),
        annotations: crisis_overlay.annotations.clone(),
    };

    let mut header = SnapshotHeader::new(
        tick.0,
        tile_states.len(),
        logistics_states.len(),
        trade_states.len(),
        population_states.len(),
        power_states.len(),
        influencer_states.len(),
    );
    header.wrap_horizontal = config.map_topology.wrap_horizontal;
    header.server_build = crate::BUILD_ID.to_string();
    header.world_epoch = world_epoch.0;

    if let Some(label_res) = campaign_label.as_ref() {
        let label = label_res.as_ref();
        header.campaign_label = Some(label.to_snapshot());
    }

    let start_marker_state = start_location
        .position()
        .map(|pos| StartMarkerState { x: pos.x, y: pos.y });

    // Second entry into `snapshot.build.rasters` (see the block above) — the two remaining
    // full-grid overlays, which could not be built with the others.
    let raster_scope = crate::turn_profile::scope("snapshot.build.rasters");
    let moisture_overlay_state =
        moisture_overlay_from_resource(moisture.as_ref().map(|res| res.as_ref()), config.grid_size);

    let elevation_overlay_state =
        elevation_overlay_from_field(elevation.as_ref(), config.grid_size);
    drop(raster_scope);
    // The climate-band cut points ride the snapshot beside the other worldgen overlays
    // (`docs/plan_climate_authority.md` §8.3): the sim owns them, the client renders the band it is
    // told. A per-map constant read straight off the active `ClimateConfig`.
    let climate_bands_state = ClimateBandsState {
        polar_max_temp: config.climate.polar_max_temp,
        boreal_max_temp: config.climate.boreal_max_temp,
        temperate_max_temp: config.climate.temperate_max_temp,
    };
    let campaign_profiles_state: Vec<_> = snapshot_profiles(&start_profiles)
        .into_iter()
        .map(|entry| entry.to_schema())
        .collect();
    // The client's DISPLAY herd list, fog-filtered for the viewer faction — the same ledger and the
    // same faction `visibility_raster` below is rendered from, so the two can never disagree about
    // whether a herd is on visible ground. The authoritative `herd_registry_states` below is
    // deliberately UNFILTERED: it is sim state (rollback + `export_map` ground truth), not a view.
    let herd_states = herd_snapshot_entries(HerdSnapshotInputs {
        telemetry: &herds,
        registry: &herd_registry,
        fauna: &fauna_config,
        ladder: &ladder_config,
        labor: &labor_config,
        expedition: &expedition_cfg,
        grid_size: config.grid_size,
        wrap_horizontal: config.map_topology.wrap_horizontal,
        visibility: &visibility_ledger,
        viewer: viewer_faction.0,
        fog_enabled: config.fog_enabled,
    });
    // Authoritative herd state for rollback (distinct from the lossy display `herd_states` above),
    // sorted deterministically by herd id like the generation states.
    let mut herd_registry_states: Vec<HerdState> =
        herd_registry.entries().iter().map(herd_state).collect();
    herd_registry_states.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    // Authoritative depletable-forage state for rollback, sorted deterministically by tile coord
    // (HashMap iteration order is unstable). Mirrors the herd-registry capture above.
    let mut forage_registry_states: Vec<ForageState> =
        forage_registry.patches.values().map(forage_state).collect();
    forage_registry_states.sort_unstable_by_key(|state| (state.y, state.x));
    // Authoritative graze/pasture state for rollback, same coord-sorted shape as the forage registry.
    // (The *client* readout is on `TileState`, captured above — this is the sim record only.)
    let mut graze_registry_states: Vec<GrazeState> =
        graze_registry.patches.values().map(graze_state).collect();
    graze_registry_states.sort_unstable_by_key(|state| (state.y, state.x));
    // The Telling's narrative memory. Already deterministically ordered (BTree-backed), so it
    // needs no sort of its own.
    let beat_ledger_state = beat_ledger.to_state();
    let faction_inventory_state = snapshot_faction_inventory(&faction_inventory);
    let sedentarization_state = snapshot_sedentarization(&sedentarization);
    let discovered_sites_state = snapshot_discovered_sites(&discovered_sites, &sites_config);
    let demographics_state = snapshot_demographics(&population_states);
    let forage_patches_state = snapshot_forage_patches(
        &forage_registry,
        &labor_config.forage,
        &flora_config,
        &ladder_config,
        &seasonal_weights,
        &sow_site_refusals,
        &flora_compositions,
    );
    let intensification_knowledge_state = snapshot_intensification_knowledge(&discovery_progress);
    let command_events_state = command_events_to_state(&command_events);
    // The Telling's client-facing fork tier + stance readout (BTree-backed, so already ordered).
    let pending_forks_state = snapshot_pending_forks(&beat_ledger);
    let stance_axes_state = snapshot_stance_axes(&beat_ledger);
    let voice_medium_state = snapshot_voice_medium(&beat_ledger);
    let victory_snapshot_state = victory_snapshot_from_resource(&victory);
    let capability_bits = capability_flags.bits();

    let assembled = WorldSnapshot {
        header,
        tiles: tile_states,
        logistics: logistics_states,
        trade_links: trade_states,
        populations: population_states,
        power: power_states,
        power_metrics: power_metrics.clone(),
        terrain: terrain_overlay.clone(),
        logistics_raster: logistics_raster.clone(),
        sentiment_raster: sentiment_raster.clone(),
        corruption_raster: corruption_raster.clone(),
        culture_raster: culture_raster.clone(),
        military_raster: military_raster.clone(),
        visibility_raster: visibility_raster.clone(),
        fog_enabled: config.fog_enabled,
        moisture_raster: moisture_overlay_state.clone(),
        elevation_overlay: elevation_overlay_state.clone(),
        climate_bands: climate_bands_state,
        start_marker: start_marker_state.clone(),
        victory: victory_snapshot_state.clone(),
        herds: herd_states.clone(),
        herd_registry: herd_registry_states,
        forage_registry: forage_registry_states,
        graze_registry: graze_registry_states,
        beat_ledger: beat_ledger_state,
        food_modules: food_module_states.clone(),
        campaign_profiles: campaign_profiles_state,
        faction_inventory: faction_inventory_state.clone(),
        sedentarization: sedentarization_state.clone(),
        discovered_sites: discovered_sites_state.clone(),
        demographics: demographics_state.clone(),
        forage_patches: forage_patches_state.clone(),
        intensification_knowledge: intensification_knowledge_state.clone(),
        command_events: command_events_state.clone(),
        pending_forks: pending_forks_state.clone(),
        stance_axes: stance_axes_state.clone(),
        voice_medium: voice_medium_state.clone(),
        capability_flags: capability_bits,
        axis_bias: axis_bias_state,
        sentiment: sentiment_state,
        generations: generation_states,
        corruption: corruption_ledgers.ledger().clone(),
        influencers: influencer_states,
        culture_layers: culture_layer_states,
        culture_tensions: culture_tension_states,
        discovery_progress: discovery_states,
        great_discovery_definitions: great_discovery_definition_states.clone(),
        great_discoveries: great_discovery_states,
        great_discovery_progress: great_discovery_progress_states,
        great_discovery_telemetry: great_discovery_telemetry_state,
        knowledge_ledger: knowledge_ledger_states,
        knowledge_timeline: knowledge_timeline_states,
        knowledge_metrics: knowledge_metrics_state,
        crisis_telemetry: crisis_telemetry_state.clone(),
        crisis_overlay: crisis_overlay_state.clone(),
    };
    drop(build_scope);

    // **The turn thread's last act on this snapshot.** Hashing, diffing, encoding and the socket
    // write are all pure functions of the world just assembled and of publication's own state, so
    // they run on the publisher thread (#393) and turn latency stops depending on them. What is
    // left here is a move onto a bounded channel.
    //
    // Turn path: record a fresh ring entry (`update`). Post-command re-capture path
    // (`SnapshotCaptureMode::refresh_in_place`): refresh the latest broadcast + back ring entry in
    // place so a mid-turn command's world mutation reaches the client now, without pushing a ring
    // entry / advancing the turn.
    let _handoff_scope = crate::turn_profile::scope("snapshot.handoff");
    if capture_mode.refresh_in_place {
        history.refresh_latest(assembled);
    } else {
        history.update(assembled);
    }
}

/// How many published frames the publisher keeps.
///
/// **One.** This ring used to be `SimulationConfig::checkpoint_history_turns` deep — 256 full
/// `WorldSnapshot`s, which measured at **1.68 GB** resident on an 80×52 map and had never been
/// measured before the checkpoint arc put a number beside it. Its only historical reader was
/// rollback, fetching the stored view at the target tick to re-baseline the client; rollback now
/// recaptures that frame from the world it just restored, which carries the same information
/// (`a_rollback_produces_the_world_that_tick_had`) and cannot disagree with it.
///
/// What remains needs only the latest entry: `latest_entry` for resync and `export_map`, and the
/// delta baseline, which tracks the previous publication rather than the ring.
/// `SimulationConfig::checkpoint_history_turns` now governs
/// [`crate::sim_state::CheckpointHistory`] alone — one depth knob, one history of worlds. It is set
/// once at construction (`build_headless_app`) rather than re-asserted every turn.
pub(crate) const PUBLICATION_RING_DEPTH: usize = 1;

/// Selects how [`capture_snapshot`] writes its result: the normal turn path records a fresh ring
/// entry (`false`); the post-command re-capture path refreshes the latest broadcast snapshot in
/// place (`true`) so a world-mutating command is reflected immediately without corrupting the
/// rollback ring. Toggled by the server around a `run_system_once(capture_snapshot)`.
#[derive(bevy::prelude::Resource, Debug, Clone, Copy, Default)]
pub struct SnapshotCaptureMode {
    pub refresh_in_place: bool,
}

/// Re-capture the current world into the latest broadcast snapshot **in place** — no ring-entry
/// push, no turn/`TurnQueue` advance. Runs [`capture_snapshot`] with
/// `SnapshotCaptureMode::refresh_in_place` toggled on, so a mid-turn command's world mutation
/// (expedition launch, `move_band`, `assign_labor`, …) is reflected in the client's snapshot
/// immediately. The server broadcasts the result afterward (`broadcast_latest`, off
/// `SnapshotHistory::encoded_snapshot_flat` / `encoded_delta_flat`). Kept in this module so `capture_snapshot`'s private `SystemParam` types stay internal.
pub fn recapture_snapshot_in_place(world: &mut World) {
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = true;
    world.run_system_once(capture_snapshot);
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = false;
}
