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
    /// Authoritative herd sim state — read to fill in what the lossy display `herds` telemetry
    /// lacks (carrying capacity, the yield forecast). The registry itself is not published; the
    /// checkpoint carries it (`SimState::herds`).
    pub herd_registry: Res<'w, HerdRegistry>,
    /// Authoritative depletable-forage sim state — read for the per-tile `forage_patches` readout
    /// and the "is there a patch here?" tile flag. Not published; the checkpoint carries it.
    pub forage_registry: Res<'w, ForageRegistry>,
    /// Authoritative graze/pasture sim state — read for the per-tile `TileState.graze_*` readout
    /// (graze is on nearly every land tile, so a per-patch list would be the wrong shape — see
    /// `graze.rs`). Not published as a registry; the checkpoint carries it.
    pub graze_registry: Res<'w, GrazeRegistry>,
    /// The Telling's narrative memory — read for the client-facing fork tier, stance and voice
    /// readouts. The ledger itself is not published; the checkpoint carries it.
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
    /// The directed ties contact left behind. Published filtered to the viewer's own edges; the
    /// checkpoint carries the ledger itself.
    pub connections: Res<'w, crate::connections::ConnectionLedger>,
    /// Every road in the world. Published filtered to the roads the viewer has explored; the
    /// checkpoint carries the ledger itself.
    pub roads: Res<'w, crate::routes::RoadRegistry>,
    /// Tile coords → tile entity, so a road's path can be priced through the same
    /// `TerrainDefinition::infrastructure_cost` sum the bill and the decay read.
    pub tile_registry: Res<'w, crate::resources::TileRegistry>,
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
    /// The per-tile flora quote memo — what grows on each patch tile and what each plant there would
    /// pay once committed to. `ResMut` because the capture is the thing that fills it: the quotes are
    /// a pure function of ground and config, so they are derived once per tile per world rather than
    /// re-derived every turn (#410). See `snapshot/flora_quotes.rs` for the input set the memo keys
    /// on and why it is not sim state.
    pub flora_quotes: ResMut<'w, FloraQuoteCache>,
    /// Fauna tuning (ecology / hunt / market / husbandry). Read at capture for each herd's
    /// **pre-commit yield forecast** (`fauna::hunt_forecast` — the client's live "Expected yield" +
    /// worker-stepper cap and the exported per-policy `hunt_policy_ceilings`), the per-cohort hunt
    /// throughput, and the pre-launch expedition trip estimates (see `core_sim/CLAUDE.md` →
    /// Scouting & Hunting Expeditions → Snapshot).
    pub fauna: Res<'w, crate::fauna_config::FaunaConfigHandle>,
    pub expedition: Res<'w, crate::expedition_config::ExpeditionConfigHandle>,
    /// The base human's intrinsic combat profile — the **unequipped** attack tier the minimal TOE's
    /// hunting kit lifts a band off (`docs/plan_hunt_through_combat.md` §4.8).
    pub creatures: Res<'w, crate::creatures_config::CreaturesConfigHandle>,
    /// The TOE kit table — the equipped attack tier, the unequipped haul tier, and the durability
    /// dials each band's `BandEquipment` wear is measured against.
    pub equipment: Res<'w, crate::equipment_config::EquipmentConfigHandle>,
    /// Resolver tuning — read so a herd's pre-commit forecast resolves the SAME fight the take will
    /// (`docs/plan_hunt_through_combat.md` §4).
    pub combat: Res<'w, crate::combat_config::CombatConfigHandle>,
    /// The materials table and the recipe book — read at capture for the two per-world catalogues
    /// and for every band's craft offers. The refusal is resolved here, not on the client, so the
    /// capture needs both tables in hand.
    pub materials: Res<'w, crate::materials_config::MaterialsConfigHandle>,
    pub recipes: Res<'w, crate::recipes_config::RecipesConfigHandle>,
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
    tiles: Indexed<u64, TileState>,
    populations: Indexed<u64, PopulationCohortState>,
    power: Indexed<u64, PowerNodeState>,
    power_metrics: Whole<PowerTelemetryState>,
    generations: Indexed<u16, GenerationState>,
    influencers: Indexed<u32, InfluentialIndividualState>,
    culture_layers: Indexed<u32, CultureLayerState>,
    culture_tensions: Whole<Vec<CultureTensionState>>,
    discovery_progress: Indexed<(u32, u32), DiscoveryProgressEntry>,
    great_discoveries: Indexed<(u32, u16), GreatDiscoveryState>,
    great_discovery_definitions: Whole<HashMap<u16, GreatDiscoveryDefinitionState>>,
    great_discovery_progress: Indexed<(u32, u16), GreatDiscoveryProgressState>,
    great_discovery_telemetry: Whole<GreatDiscoveryTelemetryState>,
    knowledge_ledger: Indexed<u64, KnowledgeLedgerEntryState>,
    knowledge_metrics: Whole<KnowledgeMetricsState>,
    knowledge_timeline: Whole<Vec<KnowledgeTimelineEventState>>,
    crisis_telemetry: Whole<CrisisTelemetryState>,
    crisis_overlay: Whole<CrisisOverlayState>,
    start_marker: Whole<Option<StartMarkerState>>,
    axis_bias: Whole<AxisBiasState>,
    sentiment: Whole<SentimentTelemetryState>,
    terrain_overlay: Whole<TerrainOverlayState>,
    sentiment_raster: Whole<ScalarRasterState>,
    corruption_raster: Whole<ScalarRasterState>,
    visibility_raster: Whole<ScalarRasterState>,
    /// Last published `SimulationConfig::fog_enabled`, so the auxiliary (axis-bias / sentiment)
    /// deltas below echo the live setting instead of the `bool` derived default (`false`).
    fog_enabled: bool,
    culture_raster: Whole<ScalarRasterState>,
    military_raster: Whole<ScalarRasterState>,
    moisture_raster: Whole<FloatRasterState>,
    elevation_overlay: Whole<ElevationOverlayState>,
    climate_bands: Whole<ClimateBandsState>,
    corruption: Whole<CorruptionLedger>,
    victory: Whole<VictorySnapshotState>,
    capability_flags: Whole<u32>,
    faction_inventory: Whole<Vec<SchemaFactionInventoryState>>,
    sedentarization: Whole<Vec<SchemaSedentarizationState>>,
    discovered_sites: Whole<Vec<SchemaDiscoveredSitesState>>,
    connections: Whole<Vec<ConnectionState>>,
    routes: Whole<Vec<RouteState>>,
    demographics: Whole<Vec<SchemaPopulationDemographicsState>>,
    forage_patches: Whole<Vec<ForagePatchState>>,
    intensification_knowledge: Whole<Vec<IntensificationKnowledgeState>>,
    /// The ladder's knowledge ROSTER — a per-world constant, so it diffs out on every turn after the
    /// first exactly as `kits` does.
    ladder_knowledge: Whole<Vec<LadderKnowledgeState>>,
    campaign_profiles: Whole<Vec<CampaignProfileState>>,
    /// The event log's baseline is a **cursor**, not a copy of the ring: the highest `seq` the
    /// client has been sent. See `snapshot::diff_appended`.
    ///
    /// It carries no [`Whole`] flag, and cannot want one: `held` exists for a section that can be
    /// changed and changed *back* within a tick, and an append-only log has no "back". A held frame
    /// leaves the cursor where the turn left it, so every later frame in the tick re-ships every row
    /// since that turn — the restatement is structural, not a flag.
    command_events: u64,
    command_events_retention_turns: Whole<u32>,
    pending_forks: Whole<Vec<PendingForksState>>,
    stance_axes: Whole<Vec<StanceState>>,
    voice_medium: Whole<Vec<VoiceMediumState>>,
    herds: Whole<Vec<HerdTelemetryState>>,
    food_modules: Whole<Vec<FoodModuleState>>,
    /// The kit roster and the two per-job defaults — per-world constants, so in practice they diff
    /// out on every frame after the first and are re-sent only when the world is rebuilt on new
    /// tuning. Diffed rather than always-sent for exactly that reason.
    kits: Whole<Vec<KitOptionState>>,
    default_hunt_kit_id: Whole<String>,
    default_forage_kit_id: Whole<String>,
    /// The two band-wide roles' defaults, diffed like the two above — per-world constants that
    /// re-send only on a world rebuild.
    default_scout_kit_id: Whole<String>,
    default_warrior_kit_id: Whole<String>,
    /// The serialized TOE config the Workbench's designer pages print — a per-world constant like
    /// the roster above, and diffed for the same reason: it is the largest string on the section
    /// and nothing about it changes between world rebuilds.
    equipment_config_json: Whole<String>,
    /// The crafting catalogues — per-world constants like the roster above, diffed for the same
    /// reason: they are re-sent only when the world is rebuilt on new tuning.
    materials: Whole<Vec<MaterialDefState>>,
    characteristic_bands: Whole<Vec<CharacteristicBandState>>,
    recipes: Whole<Vec<RecipeDefState>>,
    /// **Not** a per-world constant — a craft is *learned* — so this one really does change, and is
    /// diffed whole exactly like the ladder's own knowledge rows.
    craft_knowledge: Whole<Vec<CraftKnowledgeState>>,
    /// The route branch's rung catalog — a per-world constant, diffed out on every frame after the
    /// first exactly as the ladder's knowledge roster is.
    route_rungs: Whole<Vec<RouteRungState>>,
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
    baseline: &mut Indexed<u64, TileState>,
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
    layers: &mut Indexed<u32, CultureLayerState>,
    tensions: &mut Whole<Vec<CultureTensionState>>,
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
    nodes: &mut Indexed<u64, PowerNodeState>,
    metrics: &mut Whole<PowerTelemetryState>,
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
    sentiment: Option<ScalarRasterState>,
    corruption: Option<ScalarRasterState>,
    culture: Option<ScalarRasterState>,
    military: Option<ScalarRasterState>,
    visibility: Option<ScalarRasterState>,
}

/// The baselines the raster section owns, borrowed disjointly out of [`PublishState`].
struct RasterBaselines<'a> {
    terrain: &'a mut Whole<TerrainOverlayState>,
    moisture: &'a mut Whole<FloatRasterState>,
    elevation: &'a mut Whole<ElevationOverlayState>,
    climate_bands: &'a mut Whole<ClimateBandsState>,
    sentiment: &'a mut Whole<ScalarRasterState>,
    corruption: &'a mut Whole<ScalarRasterState>,
    culture: &'a mut Whole<ScalarRasterState>,
    military: &'a mut Whole<ScalarRasterState>,
    visibility: &'a mut Whole<ScalarRasterState>,
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
    ledger: &'a mut Indexed<u64, KnowledgeLedgerEntryState>,
    metrics: &'a mut Whole<KnowledgeMetricsState>,
    timeline: &'a mut Whole<Vec<KnowledgeTimelineEventState>>,
    discovery_progress: &'a mut Indexed<(u32, u32), DiscoveryProgressEntry>,
    great_discoveries: &'a mut Indexed<(u32, u16), GreatDiscoveryState>,
    great_discovery_progress: &'a mut Indexed<(u32, u16), GreatDiscoveryProgressState>,
    great_discovery_definitions: &'a mut Whole<HashMap<u16, GreatDiscoveryDefinitionState>>,
    great_discovery_telemetry: &'a mut Whole<GreatDiscoveryTelemetryState>,
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
    telemetry: &mut Whole<CrisisTelemetryState>,
    overlay: &mut Whole<CrisisOverlayState>,
    victory: &mut Whole<VictorySnapshotState>,
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
    command_events_retention_turns: Option<u32>,
    pending_forks: Option<Vec<PendingForksState>>,
    stance_axes: Option<Vec<StanceState>>,
    voice_medium: Option<Vec<VoiceMediumState>>,
    faction_inventory: Option<Vec<SchemaFactionInventoryState>>,
    sedentarization: Option<Vec<SchemaSedentarizationState>>,
    discovered_sites: Option<Vec<SchemaDiscoveredSitesState>>,
    connections: Option<Vec<ConnectionState>>,
    routes: Option<Vec<RouteState>>,
    demographics: Option<Vec<SchemaPopulationDemographicsState>>,
    intensification_knowledge: Option<Vec<IntensificationKnowledgeState>>,
    ladder_knowledge: Option<Vec<LadderKnowledgeState>>,
    start_marker: Option<StartMarkerState>,
}

/// The baselines the campaign section owns.
struct CampaignBaselines<'a> {
    profiles: &'a mut Whole<Vec<CampaignProfileState>>,
    command_events: &'a mut u64,
    command_events_retention_turns: &'a mut Whole<u32>,
    pending_forks: &'a mut Whole<Vec<PendingForksState>>,
    stance_axes: &'a mut Whole<Vec<StanceState>>,
    voice_medium: &'a mut Whole<Vec<VoiceMediumState>>,
    faction_inventory: &'a mut Whole<Vec<SchemaFactionInventoryState>>,
    sedentarization: &'a mut Whole<Vec<SchemaSedentarizationState>>,
    discovered_sites: &'a mut Whole<Vec<SchemaDiscoveredSitesState>>,
    connections: &'a mut Whole<Vec<ConnectionState>>,
    routes: &'a mut Whole<Vec<RouteState>>,
    demographics: &'a mut Whole<Vec<SchemaPopulationDemographicsState>>,
    intensification_knowledge: &'a mut Whole<Vec<IntensificationKnowledgeState>>,
    ladder_knowledge: &'a mut Whole<Vec<LadderKnowledgeState>>,
    start_marker: &'a mut Whole<Option<StartMarkerState>>,
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
        // The one append-only section: rows above the cursor, never the whole ring.
        command_events: diff_appended(baseline.command_events, &snapshot.command_events, write),
        command_events_retention_turns: diff_whole(
            baseline.command_events_retention_turns,
            &snapshot.command_events_retention_turns,
            write,
        ),
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
        connections: diff_whole(baseline.connections, &snapshot.connections, write),
        routes: diff_whole(baseline.routes, &snapshot.routes, write),
        demographics: diff_whole(baseline.demographics, &snapshot.demographics, write),
        intensification_knowledge: diff_whole(
            baseline.intensification_knowledge,
            &snapshot.intensification_knowledge,
            write,
        ),
        ladder_knowledge: diff_whole(baseline.ladder_knowledge, &snapshot.ladder_knowledge, write),
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
    kits: Option<Vec<KitOptionState>>,
    default_hunt_kit_id: Option<String>,
    default_forage_kit_id: Option<String>,
    default_scout_kit_id: Option<String>,
    default_warrior_kit_id: Option<String>,
    equipment_config_json: Option<String>,
    materials: Option<Vec<MaterialDefState>>,
    characteristic_bands: Option<Vec<CharacteristicBandState>>,
    recipes: Option<Vec<RecipeDefState>>,
    craft_knowledge: Option<Vec<CraftKnowledgeState>>,
    route_rungs: Option<Vec<RouteRungState>>,
}

/// Fauna and flora: the herd roster, the forage patches, and the food-module map.
#[allow(clippy::too_many_arguments)] // one baseline slot per diffed section
fn diff_subsistence(
    herds: &mut Whole<Vec<HerdTelemetryState>>,
    forage_patches: &mut Whole<Vec<ForagePatchState>>,
    food_modules: &mut Whole<Vec<FoodModuleState>>,
    kits: &mut Whole<Vec<KitOptionState>>,
    default_hunt_kit_id: &mut Whole<String>,
    default_forage_kit_id: &mut Whole<String>,
    default_scout_kit_id: &mut Whole<String>,
    default_warrior_kit_id: &mut Whole<String>,
    equipment_config_json: &mut Whole<String>,
    materials: &mut Whole<Vec<MaterialDefState>>,
    characteristic_bands: &mut Whole<Vec<CharacteristicBandState>>,
    recipes: &mut Whole<Vec<RecipeDefState>>,
    craft_knowledge: &mut Whole<Vec<CraftKnowledgeState>>,
    route_rungs: &mut Whole<Vec<RouteRungState>>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> SubsistenceParts {
    SubsistenceParts {
        herds: diff_whole(herds, &snapshot.herds, write),
        forage_patches: diff_whole(forage_patches, &snapshot.forage_patches, write),
        food_modules: diff_whole(food_modules, &snapshot.food_modules, write),
        kits: diff_whole(kits, &snapshot.kits, write),
        default_hunt_kit_id: diff_whole(default_hunt_kit_id, &snapshot.default_hunt_kit_id, write),
        default_forage_kit_id: diff_whole(
            default_forage_kit_id,
            &snapshot.default_forage_kit_id,
            write,
        ),
        default_scout_kit_id: diff_whole(
            default_scout_kit_id,
            &snapshot.default_scout_kit_id,
            write,
        ),
        default_warrior_kit_id: diff_whole(
            default_warrior_kit_id,
            &snapshot.default_warrior_kit_id,
            write,
        ),
        equipment_config_json: diff_whole(
            equipment_config_json,
            &snapshot.equipment_config_json,
            write,
        ),
        materials: diff_whole(materials, &snapshot.materials, write),
        characteristic_bands: diff_whole(
            characteristic_bands,
            &snapshot.characteristic_bands,
            write,
        ),
        recipes: diff_whole(recipes, &snapshot.recipes, write),
        craft_knowledge: diff_whole(craft_knowledge, &snapshot.craft_knowledge, write),
        route_rungs: diff_whole(route_rungs, &snapshot.route_rungs, write),
    }
}

#[derive(Debug, Default)]
struct PeopleParts {
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
    populations: &'a mut Indexed<u64, PopulationCohortState>,
    generations: &'a mut Indexed<u16, GenerationState>,
    influencers: &'a mut Indexed<u32, InfluentialIndividualState>,
    axis_bias: &'a mut Whole<AxisBiasState>,
    sentiment: &'a mut Whole<SentimentTelemetryState>,
    corruption: &'a mut Whole<CorruptionLedger>,
    capability_flags: &'a mut Whole<u32>,
}

/// People and the networks between them: cohorts, generations, influencers, and the faction-wide
/// scalars that ride with them.
fn diff_people(
    baseline: PeopleBaselines<'_>,
    snapshot: &WorldSnapshot,
    write: Baseline,
) -> PeopleParts {
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
            tiles: Indexed::default(),
            populations: Indexed::default(),
            power: Indexed::default(),
            power_metrics: Whole::default(),
            generations: Indexed::default(),
            influencers: Indexed::default(),
            culture_layers: Indexed::default(),
            culture_tensions: Whole::default(),
            discovery_progress: Indexed::default(),
            great_discoveries: Indexed::default(),
            great_discovery_definitions: Whole::default(),
            great_discovery_progress: Indexed::default(),
            great_discovery_telemetry: Whole::default(),
            knowledge_ledger: Indexed::default(),
            knowledge_metrics: Whole::default(),
            knowledge_timeline: Whole::default(),
            crisis_telemetry: Whole::default(),
            crisis_overlay: Whole::default(),
            start_marker: Whole::default(),
            axis_bias: Whole::default(),
            sentiment: Whole::default(),
            terrain_overlay: Whole::default(),
            sentiment_raster: Whole::default(),
            corruption_raster: Whole::default(),
            visibility_raster: Whole::default(),
            fog_enabled: true,
            culture_raster: Whole::default(),
            military_raster: Whole::default(),
            moisture_raster: Whole::default(),
            elevation_overlay: Whole::default(),
            climate_bands: Whole::default(),
            corruption: Whole::default(),
            victory: Whole::default(),
            capability_flags: Whole::default(),
            faction_inventory: Whole::default(),
            sedentarization: Whole::default(),
            discovered_sites: Whole::default(),
            connections: Whole::default(),
            routes: Whole::default(),
            demographics: Whole::default(),
            forage_patches: Whole::default(),
            intensification_knowledge: Whole::default(),
            ladder_knowledge: Whole::default(),
            campaign_profiles: Whole::default(),
            // A fresh world has sent nothing, so every event ever pushed is "appended since".
            command_events: 0,
            command_events_retention_turns: Whole::default(),
            pending_forks: Whole::default(),
            stance_axes: Whole::default(),
            voice_medium: Whole::default(),
            herds: Whole::default(),
            food_modules: Whole::default(),
            kits: Whole::default(),
            materials: Whole::default(),
            characteristic_bands: Whole::default(),
            recipes: Whole::default(),
            craft_knowledge: Whole::default(),
            route_rungs: Whole::default(),
            default_hunt_kit_id: Whole::default(),
            default_forage_kit_id: Whole::default(),
            default_scout_kit_id: Whole::default(),
            default_warrior_kit_id: Whole::default(),
            equipment_config_json: Whole::default(),
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
            command_events_retention_turns,
            pending_forks,
            stance_axes,
            voice_medium,
            faction_inventory,
            sedentarization,
            discovered_sites,
            connections,
            routes,
            demographics,
            intensification_knowledge,
            ladder_knowledge,
            start_marker,
            herds,
            forage_patches,
            food_modules,
            kits,
            materials,
            characteristic_bands,
            recipes,
            craft_knowledge,
            route_rungs,
            default_hunt_kit_id,
            default_forage_kit_id,
            default_scout_kit_id,
            default_warrior_kit_id,
            equipment_config_json,
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
                            command_events_retention_turns,
                            pending_forks,
                            stance_axes,
                            voice_medium,
                            faction_inventory,
                            sedentarization,
                            discovered_sites,
                            connections,
                            routes,
                            demographics,
                            intensification_knowledge,
                            ladder_knowledge,
                            start_marker,
                        },
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    subsistence_parts = diff_subsistence(
                        herds,
                        forage_patches,
                        food_modules,
                        kits,
                        default_hunt_kit_id,
                        default_forage_kit_id,
                        default_scout_kit_id,
                        default_warrior_kit_id,
                        equipment_config_json,
                        materials,
                        characteristic_bands,
                        recipes,
                        craft_knowledge,
                        route_rungs,
                        captured,
                        write,
                    )
                });
                scope.spawn(|_| {
                    people_parts = diff_people(
                        PeopleBaselines {
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
            command_events_retention_turns: campaign_parts.command_events_retention_turns,
            pending_forks: campaign_parts.pending_forks,
            stance_axes: campaign_parts.stance_axes,
            voice_medium: campaign_parts.voice_medium,
            faction_inventory: campaign_parts.faction_inventory,
            sedentarization: campaign_parts.sedentarization,
            discovered_sites: campaign_parts.discovered_sites,
            connections: campaign_parts.connections,
            routes: campaign_parts.routes,
            demographics: campaign_parts.demographics,
            intensification_knowledge: campaign_parts.intensification_knowledge,
            ladder_knowledge: campaign_parts.ladder_knowledge,
            start_marker: campaign_parts.start_marker,
            herds: subsistence_parts.herds,
            forage_patches: subsistence_parts.forage_patches,
            food_modules: subsistence_parts.food_modules,
            kits: subsistence_parts.kits,
            materials: subsistence_parts.materials,
            characteristic_bands: subsistence_parts.characteristic_bands,
            recipes: subsistence_parts.recipes,
            craft_knowledge: subsistence_parts.craft_knowledge,
            route_rungs: subsistence_parts.route_rungs,
            default_hunt_kit_id: subsistence_parts.default_hunt_kit_id,
            default_forage_kit_id: subsistence_parts.default_forage_kit_id,
            default_scout_kit_id: subsistence_parts.default_scout_kit_id,
            default_warrior_kit_id: subsistence_parts.default_warrior_kit_id,
            equipment_config_json: subsistence_parts.equipment_config_json,
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
                // **And DROP the entry's cached encoding, which describes the pre-recapture world.**
                // Refreshing `snapshot` without clearing this left `StoredSnapshot`'s two views of one
                // entry disagreeing: `.snapshot` saw the command's mutation and `.encode_flat()` — which
                // returns these bytes when present — still answered with the world's FIRST publication.
                // Its only callers are tests asserting on encoded content, so the cost was silent: a
                // wire-level assertion read a frame from before the fixture had finished building, and
                // passed or failed on the wrong world. `None` rather than a re-encode on purpose — a
                // recapture must not pay the full-snapshot encoding #384 took off the turn path, and
                // `encode_flat` already encodes on demand for the rare reader that wants one. Symmetric
                // with `self.encoded_snapshot_flat = None` on the line above, for the same reason.
                back.encoded_snapshot_flat = None;
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
        self.tiles.reset(
            entry
                .snapshot
                .tiles
                .iter()
                .map(|state| (state.entity, state.clone()))
                .collect(),
        );
        self.populations.reset(
            entry
                .snapshot
                .populations
                .iter()
                .map(|state| (state.entity, state.clone()))
                .collect(),
        );
        self.power.reset(
            entry
                .snapshot
                .power
                .iter()
                .map(|state| (state.entity, state.clone()))
                .collect(),
        );
        self.generations.reset(
            entry
                .snapshot
                .generations
                .iter()
                .map(|state| (state.id, state.clone()))
                .collect(),
        );
        self.influencers.reset(
            entry
                .snapshot
                .influencers
                .iter()
                .map(|state| (state.id, state.clone()))
                .collect(),
        );
        self.culture_layers.reset(
            entry
                .snapshot
                .culture_layers
                .iter()
                .map(|state| (state.id, state.clone()))
                .collect(),
        );
        self.corruption.reset(entry.snapshot.corruption.clone());
        self.axis_bias.reset(entry.snapshot.axis_bias.clone());
        self.sentiment.reset(entry.snapshot.sentiment.clone());
        self.terrain_overlay.reset(entry.snapshot.terrain.clone());
        self.sentiment_raster
            .reset(entry.snapshot.sentiment_raster.clone());
        self.corruption_raster
            .reset(entry.snapshot.corruption_raster.clone());
        self.visibility_raster
            .reset(entry.snapshot.visibility_raster.clone());
        self.fog_enabled = entry.snapshot.fog_enabled;
        self.culture_raster
            .reset(entry.snapshot.culture_raster.clone());
        self.military_raster
            .reset(entry.snapshot.military_raster.clone());
        self.moisture_raster
            .reset(entry.snapshot.moisture_raster.clone());
        self.culture_tensions
            .reset(entry.snapshot.culture_tensions.clone());
        self.discovery_progress.reset(
            entry
                .snapshot
                .discovery_progress
                .iter()
                .map(|state| ((state.faction, state.discovery), state.clone()))
                .collect(),
        );
        self.victory.reset(entry.snapshot.victory.clone());
        self.faction_inventory
            .reset(entry.snapshot.faction_inventory.clone());
        self.sedentarization
            .reset(entry.snapshot.sedentarization.clone());
        self.discovered_sites
            .reset(entry.snapshot.discovered_sites.clone());
        self.connections.reset(entry.snapshot.connections.clone());
        self.routes.reset(entry.snapshot.routes.clone());
        self.demographics.reset(entry.snapshot.demographics.clone());
        self.forage_patches
            .reset(entry.snapshot.forage_patches.clone());
        self.intensification_knowledge
            .reset(entry.snapshot.intensification_knowledge.clone());
        self.ladder_knowledge
            .reset(entry.snapshot.ladder_knowledge.clone());
        self.campaign_profiles
            .reset(entry.snapshot.campaign_profiles.clone());
        // Rewind the cursor to the newest event the restored frame carries — a rollback un-sends
        // everything after it, and a cursor left ahead would suppress the re-send.
        self.command_events = entry
            .snapshot
            .command_events
            .iter()
            .map(|state| state.seq)
            .max()
            .unwrap_or(0);
        self.command_events_retention_turns
            .reset(entry.snapshot.command_events_retention_turns);
        self.pending_forks
            .reset(entry.snapshot.pending_forks.clone());
        self.stance_axes.reset(entry.snapshot.stance_axes.clone());
        self.voice_medium.reset(entry.snapshot.voice_medium.clone());
        self.herds.reset(entry.snapshot.herds.clone());
        self.food_modules.reset(entry.snapshot.food_modules.clone());
        self.kits.reset(entry.snapshot.kits.clone());
        self.materials.reset(entry.snapshot.materials.clone());
        self.characteristic_bands
            .reset(entry.snapshot.characteristic_bands.clone());
        self.recipes.reset(entry.snapshot.recipes.clone());
        self.craft_knowledge
            .reset(entry.snapshot.craft_knowledge.clone());
        self.route_rungs.reset(entry.snapshot.route_rungs.clone());
        self.default_hunt_kit_id
            .reset(entry.snapshot.default_hunt_kit_id.clone());
        self.default_forage_kit_id
            .reset(entry.snapshot.default_forage_kit_id.clone());
        self.default_scout_kit_id
            .reset(entry.snapshot.default_scout_kit_id.clone());
        self.default_warrior_kit_id
            .reset(entry.snapshot.default_warrior_kit_id.clone());
        self.equipment_config_json
            .reset(entry.snapshot.equipment_config_json.clone());
        self.great_discoveries.reset(
            entry
                .snapshot
                .great_discoveries
                .iter()
                .map(|state| ((state.faction, state.id), state.clone()))
                .collect(),
        );
        self.great_discovery_progress.reset(
            entry
                .snapshot
                .great_discovery_progress
                .iter()
                .map(|state| ((state.faction, state.discovery), state.clone()))
                .collect(),
        );
        self.great_discovery_telemetry
            .reset(entry.snapshot.great_discovery_telemetry.clone());
        self.knowledge_ledger.reset(
            entry
                .snapshot
                .knowledge_ledger
                .iter()
                .map(|state| {
                    (
                        encode_ledger_key(FactionId(state.owner_faction), state.discovery_id),
                        state.clone(),
                    )
                })
                .collect(),
        );
        self.knowledge_metrics
            .reset(entry.snapshot.knowledge_metrics.clone());
        self.knowledge_timeline
            .reset(entry.snapshot.knowledge_timeline.clone());
        self.crisis_telemetry
            .reset(entry.snapshot.crisis_telemetry.clone());
        self.crisis_overlay
            .reset(entry.snapshot.crisis_overlay.clone());
        self.elevation_overlay
            .reset(entry.snapshot.elevation_overlay.clone());
        self.start_marker.reset(entry.snapshot.start_marker.clone());
        self.capability_flags.reset(entry.snapshot.capability_flags);

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
    /// only heal once some later publication refreshes the ring entry. Any publication at all dates
    /// the number stored on an entry: a mid-tick recapture claims one on every world-mutating command,
    /// and an auxiliary delta (`update_axis_bias` and friends) claims one without touching the ring
    /// at all.
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
        if self.axis_bias.baseline() == &bias {
            return None;
        }

        // An auxiliary feed delta publishes this section whole and commits it in the same breath,
        // so it re-baselines rather than assigning: whatever a held frame left outstanding on this
        // section is superseded by the value going out here.
        self.axis_bias.reset(bias.clone());

        let header = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.header.clone())
            .unwrap_or_default();

        let delta = WorldDelta {
            header,
            tiles: Vec::new(),
            removed_tiles: Vec::new(),
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
            command_events_retention_turns: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            kits: None,
            materials: None,
            characteristic_bands: None,
            recipes: None,
            craft_knowledge: None,
            route_rungs: None,
            default_hunt_kit_id: None,
            default_forage_kit_id: None,
            default_scout_kit_id: None,
            default_warrior_kit_id: None,
            equipment_config_json: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            connections: None,
            routes: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            ladder_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: Some(bias.clone()),
            sentiment: None,
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
            command_events_retention_turns: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            kits: None,
            materials: None,
            characteristic_bands: None,
            recipes: None,
            craft_knowledge: None,
            route_rungs: None,
            default_hunt_kit_id: None,
            default_forage_kit_id: None,
            default_scout_kit_id: None,
            default_warrior_kit_id: None,
            equipment_config_json: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            connections: None,
            routes: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            ladder_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: None,
            sentiment: None,
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
        if self.corruption.baseline() == &ledger {
            return None;
        }

        // Re-baselines for the same reason as `update_axis_bias` above.
        self.corruption.reset(ledger.clone());

        let header = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.header.clone())
            .unwrap_or_default();

        let delta = WorldDelta {
            header,
            tiles: Vec::new(),
            removed_tiles: Vec::new(),
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
            command_events_retention_turns: None,
            pending_forks: None,
            stance_axes: None,
            voice_medium: None,
            herds: None,
            food_modules: None,
            kits: None,
            materials: None,
            characteristic_bands: None,
            recipes: None,
            craft_knowledge: None,
            route_rungs: None,
            default_hunt_kit_id: None,
            default_forage_kit_id: None,
            default_scout_kit_id: None,
            default_warrior_kit_id: None,
            equipment_config_json: None,
            faction_inventory: None,
            sedentarization: None,
            discovered_sites: None,
            connections: None,
            routes: None,
            demographics: None,
            forage_patches: None,
            intensification_knowledge: None,
            ladder_knowledge: None,
            knowledge_timeline: None,
            crisis_telemetry: None,
            crisis_overlay: None,
            moisture_raster: None,
            elevation_overlay: None,
            climate_bands: None,
            start_marker: None,
            axis_bias: None,
            sentiment: None,
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

/// **The kit roster for the wire** — one row per `equipment.json` kit, carrying the tiers that kit
/// grants a party whose components are all **fresh** (`BandEquipment::start_stocked`).
///
/// The tiers are resolved through the **same three seams** the take path reads
/// (`hunter_profile` / `hunt_per_worker_biomass_capacity` / `forage_per_worker_biomass_capacity`),
/// so the picker's numbers cannot drift from what sending that kit actually buys. It is a fresh-kit
/// statement on purpose: what a given band's *wear* then does to those tiers is that band's own row.
fn kit_roster_states(
    equipment: &crate::equipment_config::EquipmentConfig,
    labor: &crate::labor_config::LaborConfig,
    kit_levers: &crate::snapshot::population::BandKitLevers<'_>,
) -> Vec<KitOptionState> {
    let fresh = BandEquipment::start_stocked(equipment);
    equipment
        .kits()
        .iter()
        .map(|definition| {
            let choice = equipment
                .kit(&definition.id)
                .expect("a roster entry resolves by its own id");
            // **Through the same seam the per-band rows resolve through** — this one over `fresh`,
            // `population_state`'s `kit_tiers` over the band's live ledger. One arithmetic.
            let tiers = equipment.resolve_kit_tiers(
                kit_levers.person_intrinsic,
                labor.hunt.per_worker_biomass_capacity,
                labor.forage.per_worker_biomass_capacity,
                labor.scout.vantage_range as f32,
                &choice,
                &fresh,
            );
            KitOptionState {
                id: definition.id.clone(),
                display_name: definition.display_name.clone(),
                jobs: definition
                    .jobs
                    .iter()
                    .map(|job| job.as_str().to_string())
                    .collect(),
                attack: tiers.attack,
                hunt_carry_per_worker_biomass: tiers.hunt_carry_per_worker_biomass,
                forage_carry_per_worker_biomass: tiers.forage_carry_per_worker_biomass,
                // **The scout vantage's tier.** Not what a *band* currently sees — a fresh kit's
                // reach, exactly like the three above, so the picker renders the kit and not the
                // band that happens to be selected.
                scout_vantage_range: tiers.scout_vantage_range,
                // **The retired multiplier's slot, held at its neutral** — the stat is an
                // additive per-worker contribution now (`buildWorkPerWorker` beside it), and a
                // number in these units would read as a rate on a field the client renders as one.
                build_rate: sim_schema::RETIRED_BUILD_RATE,
                build_work_per_worker: tiers.build_work_per_worker,
                // **WHICH WEB THAT WORTH IS FOR**, `""` for a kit carrying no build tool. The pair
                // is one reading: a hoe is worth `+0.5` per worker per turn on a Cultivate and
                // nothing at all on a `Tame`, so a picker greys the kit where the branches disagree
                // rather than quoting an uplift the sim will never pay.
                build_work_branch: tiers
                    .build_work_branch
                    .map(|branch| branch.as_str().to_string())
                    .unwrap_or_default(),
                // **The attack's size window**, so the client's pre-launch gate resolves this kit
                // against the quarry in front of it rather than against the kit's best case. `0` on
                // either end is unbounded, which every weapon but the passive device is.
                attack_min_body_mass: tiers.attack_min_body_mass,
                attack_max_body_mass: tiers.attack_max_body_mass,
                dispersion: tiers.dispersion,
                exposure: tiers.exposure,
                // **WHICH ITEMS THIS KIT CARRIES** — the definition's `uses` list verbatim, in config
                // order, off the *definition* rather than the resolved `KitChoice` so the published
                // order is the roster's own. The tiers above are numbers and name no item, so without
                // this a durability readout has to guess which component produced them — and the
                // guess was `attack → "spears"`, which quoted a Trapping party the spears' condition.
                item_ids: definition.uses.clone(),
            }
        })
        .collect()
}

/// **The effective TOE config as one JSON string** — the Workbench designer surface's read-only
/// catalogue, and the one place this schema ships a blob instead of typed fields.
///
/// Serializing the **struct**, never the file, is what makes it honest twice over: it states what
/// the sim is actually running (an `EQUIPMENT_CONFIG_PATH` override included), and the file's
/// `_comment*` keys are not struct fields so they never reach the wire.
///
/// A designer page must not be able to fail a frame, so a serialization error publishes the empty
/// string and warns — the client renders "no catalogue" and every other section still ships.
fn serialize_equipment_config(equipment: &crate::equipment_config::EquipmentConfig) -> String {
    match serde_json::to_string(equipment) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(
                target: "core_sim::snapshot",
                %error,
                "equipment config could not be serialized for the designer surface"
            );
            String::new()
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
        Option<&'static BandId>,
        Option<&'static BandEquipment>,
        Option<&'static crate::components::BandBench>,
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

/// The two readings of [`crate::fauna::herd_default_hunt_kit`]'s source axis, named so the
/// per-species quote table says which half it is building rather than passing a bare `true`.
const HERD_ON_THE_RANGE: bool = false;
const HERD_IN_A_PEN: bool = true;

/// **Assemble one quarry's [`QuotedParty`]** — the fight tier and the kit id, both resolved from the
/// *same* `kit` against the *same* `wear`, so a herd row cannot publish one kit's id beside another
/// kit's attack.
///
/// **The hunter profile is passed in rather than resolved here**, because which of the two named
/// resolvers applies is the caller's decision and must stay visible at the call site: a per-species
/// party resolves [`crate::equipment_config::EquipmentConfig::hunter_profile_against`], the
/// no-species fallback resolves `hunter_profile_unbounded`. Folding that choice in here would be a
/// third resolver that picks for you, which is exactly what the two names exist to prevent.
fn quoted_party_for(
    equipment: &crate::equipment_config::EquipmentConfig,
    combat: &crate::combat_config::CombatConfig,
    kit: &crate::equipment_config::KitChoice,
    wear: &BandEquipment,
    hunter: crate::combat::CombatStats,
) -> QuotedParty {
    QuotedParty {
        // **UNIFORM, and deliberately so.** These rows are priced against the fresh *reference*
        // ledger (`BandEquipment::start_stocked`), which states liveness and not counts — the row
        // describes what a kit buys against this quarry, not how much of that kit any band owns.
        // A band's own coverage reaches its take through `advance_labor_allocation`.
        party: crate::fauna::HuntingParty::uniform(
            hunter,
            combat.tuning(),
            combat.hunt_injury_damage_per_animal * equipment.exposure(kit, wear),
            equipment.dispersion(kit, wear),
        ),
        kit_id: kit.id().to_string(),
    }
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn capture_snapshot(
    ctx: SnapshotContext,
    tiles: Query<(Entity, &Tile, Option<&FoodModuleTag>)>,
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
    // Config resolution before any world data is read: destructuring the context, `.get()`ing the
    // hot-reloadable config handles, and composing the morale-pressure struct the tile sweep needs.
    // O(1) in world size — it touches no entity.
    let prelude_scope = crate::turn_profile::scope("snapshot.build.prelude");
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
        connections,
        roads,
        tile_registry,
        viewer_faction,
        demographics,
        wellbeing,
        labor,
        flora,
        ladder,
        mut flora_quotes,
        fauna,
        expedition,
        creatures,
        equipment,
        combat,
        materials,
        recipes,
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
    drop(prelude_scope);
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
    // **What grows on each patch tile** (the flora quote memo) — the named plants its forage capacity
    // decomposes into and what each would pay once committed to, resolved through the ONE
    // `forage::tile_flora_composition` seam (the twin of `tile_forage_capacity`), so a navigable
    // hex's *two* capacity terms are both named and the wire cannot disagree with the table.
    //
    // **The quotes are a pure function of ground and config, so this sweep only DERIVES the ones
    // whose ground it has not already seen** (#410). Every tile still passes through the memo, which
    // re-derives the moment its terrain moves or a config is reloaded — see
    // `snapshot/flora_quotes.rs`, which owns the input set and the invalidation.
    //
    // Patches only, for both — the client asks "why can't I sow *here*?" / "what grows here?" of a
    // tile it is looking at, and a patch is on every food-bearing tile there is (see
    // core_sim/CLAUDE.md → the Field).
    let patches_scope = crate::turn_profile::scope("snapshot.build.patches");
    let field_rung = ladder_config.rung(RungKey::PlantField);
    let grid = config.grid_size;
    let wrap_horizontal = config.map_topology.wrap_horizontal;
    let mut sow_site_refusals: HashMap<UVec2, SiteRefusal> = HashMap::new();
    // **The size of the land under each patch** — the tile's own forage `K`, through the one
    // `forage::tile_forage_capacity` seam. Every plant upkeep figure the patch row publishes is
    // quoted per **tender-load** of it (`forage::patch_tender_loads`), the plant twin of a herd's
    // keeper-load, so the row needs the ground and not just the patch. Collected in this sweep for
    // the same reason `sow_site_refusals` is: the tiles are here and the readout is not.
    let mut tile_capacities: HashMap<UVec2, f32> = HashMap::new();
    let mut flora_sweep = flora_quotes.sweep(
        &flora_config,
        &labor_config,
        &ladder_config,
        config.map_seed,
        grid,
    );
    for tile in patch_tiles {
        tile_capacities.insert(
            tile.position,
            crate::forage::tile_forage_capacity(&labor_config.forage, tile),
        );
        let fresh_water = tile_is_fresh_watered(tile, grid.x, grid.y, wrap_horizontal, |coord| {
            tile_tags.get(coord)
        });
        if let Some(refusal) = rung_site_refusal(
            field_rung,
            tile,
            &labor_config.forage,
            food_sites.is_site(tile.position),
            fresh_water,
        ) {
            sow_site_refusals.insert(tile.position, refusal);
        }
        flora_sweep.quotes(tile);
    }
    drop(patches_scope);

    // Everything that finishes the tile vector once both sweeps are done: the food-module site
    // list, the per-tile culture-layer stamp (one `local_layer_by_owner` lookup per tile), the sort
    // into entity order, and the entity → coord index the population/expedition readouts join
    // against. Per tile, plus per food site.
    let tile_index_scope = crate::turn_profile::scope("snapshot.build.tile_index");
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
        // Keyed on the tile's POSITION, because that is what `attach_local` files a local layer
        // under (`CultureOwner::from_tile`). Keying on `tile.entity` — the entity bits — looks up a
        // disjoint range (`from_tile` always sets bit 63), so it missed on every tile of every
        // snapshot and `culture_layer` shipped as a uniform `0`.
        let owner = CultureOwner::from_tile(UVec2::new(tile.x, tile.y));
        if let Some(layer) = culture.local_layer_by_owner(owner) {
            tile.culture_layer = layer.id;
        }
    }
    tile_states.sort_unstable_by_key(|state| state.entity);
    let tile_positions: HashMap<u64, UVec2> = tile_states
        .iter()
        .map(|state| (state.entity, UVec2::new(state.x, state.y)))
        .collect();
    drop(tile_index_scope);

    // The per-cohort readout: two walks of the population query (the coord index, then the states),
    // each of which derives travel/scout/expedition figures rather than copying them. Per cohort,
    // and the expedition-delivery forecast inside it is a forward sim per in-flight party.
    let populations_scope = crate::turn_profile::scope("snapshot.build.populations");
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
    // **The minimal TOE levers**, resolved once for every cohort: the kit table plus the two
    // *equipped* tiers that live outside `equipment.json` (one home per fact) — the bare-handed
    // `person` profile and `labor_config`'s kitted haul rate. What varies per band is only its
    // `BandEquipment` wear, which `population_state` resolves against these.
    let equipment_config = equipment.get();
    let combat_config = combat.get();
    // A detached party fights at the `expedition_danger_multiplier`-scaled lethality, exactly as
    // `advance_expeditions` resolves it — so the in-flight ETA and the turn agree. Through the one
    // named constructor rather than a fourth copy of the multiply (`CombatConfig::expedition_tuning`).
    let expedition_combat_tuning = combat_config.expedition_tuning();
    let kit_levers = crate::snapshot::population::BandKitLevers {
        config: &equipment_config,
        person_intrinsic: creatures.get().person(),
        baseline_haul_rate: labor_config.hunt.per_worker_biomass_capacity,
        baseline_gather_rate: labor_config.forage.per_worker_biomass_capacity,
        equipped_vantage_range: labor_config.scout.vantage_range as f32,
    };
    // **The crafting readout's config half, resolved ONCE for the capture.** `craftOffers` is
    // bands × recipes, and everything that is a function of the recipe alone — its group, its bench
    // material, the tool that bounds it, the material's own word — is a constant across that
    // product. Hoisting it is what keeps the per-band pass to the band's own three questions.
    let materials_config = materials.get();
    let recipes_config = recipes.get();
    let craft_offer_plans =
        crate::snapshot::crafting::plan_craft_offers(&recipes_config, &equipment_config);
    let knowledge_threshold = ladder_config.knowledge.completion_threshold;
    // **Per FACTION, not per band** — every band of a faction knows the same crafts, so a per-band
    // resolution would be one discovery-ledger walk per band for one answer.
    let known_crafts_by_faction: std::collections::HashMap<
        crate::orders::FactionId,
        std::collections::BTreeMap<String, bool>,
    > = populations
        .iter()
        .map(|(_, cohort, _, _, _, _, _, _)| cohort.faction)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|faction| {
            (
                faction,
                crate::snapshot::crafting::known_crafts(
                    &materials_config,
                    &discovery_progress,
                    faction,
                    knowledge_threshold,
                ),
            )
        })
        .collect();
    // **A faction with no ledger row knows nothing**, which is the opening state of every campaign:
    // none of the three crafts ships known. An empty map is that answer, and it is a `static` rather
    // than a per-band allocation because the fallback is taken on the first turn of every game.
    static NO_CRAFTS_KNOWN: std::sync::LazyLock<std::collections::BTreeMap<String, bool>> =
        std::sync::LazyLock::new(std::collections::BTreeMap::new);
    let expedition_levers = ExpeditionLevers {
        hunt_per_worker_carry: expedition_cfg.hunt.per_worker_carry,
        trade_per_worker_carry: expedition_cfg.trade.per_worker_carry,
        trade_material_carry_weight: expedition_cfg.trade.material_carry_weight,
        // **The EQUIPPED reference rate, resolved through the item table's default tier** — an
        // outfitting lever is quoted for a party that leaves kitted, and `labor_config`'s key is the
        // sledless baseline now.
        hunt_per_worker_provisions: hunt_per_worker_provisions(
            equipment_config.equipped_reference(
                crate::equipment_config::EquipmentStat::HuntCarry,
                labor_config.hunt.per_worker_biomass_capacity,
            ),
            &fauna_config,
        ),
        hunt_viability_warn_turns: expedition_cfg.hunt.viability_warn_turns,
        hunt_forecast_horizon_turns: expedition_cfg.hunt.forecast_horizon_turns,
        band_move_tiles_per_turn: labor_config.band_move_tiles_per_turn,
        settle_min_founding_workers: expedition_cfg.settle.min_founding_workers,
        settle_parent_min_workers: expedition_cfg.settle.parent_min_workers,
    };
    // A cohort → live-tile map so an in-flight expedition can find its home band's CURRENT tile
    // (bands are nomadic). The `populations` query is read-only, so iterating it twice is fine.
    let cohort_positions: std::collections::HashMap<Entity, UVec2> = populations
        .iter()
        .filter_map(|(entity, cohort, _, _, _, _, _, _)| {
            tile_positions
                .get(&cohort.current_tile.to_bits())
                .copied()
                .map(|p| (entity, p))
        })
        .collect();
    let mut population_states: Vec<PopulationCohortState> = populations
        .iter()
        .map(
            |(entity, cohort, allocation, travel, expedition, band_id, equipment, bench)| {
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
                    // **This party's own fighting tier** — the kit it was SENT OUT WITH masked over
                    // its `BandEquipment` wear, through the same seams `advance_expeditions` reads,
                    // so the ETA projects the take the party can actually make: bare-handed if it
                    // left bare-handed, and stepped down once its spears are gone.
                    let party_wear = equipment.cloned().unwrap_or_else(|| {
                        BandEquipment::start_stocked_for(
                            &equipment_config,
                            available_workers(cohort.working) as f32,
                        )
                    });
                    // **The party's TARGET, so a mass-bounded weapon is judged against the animal it
                    // was actually sent after.** A party whose mission names no herd (a scout) has no
                    // quarry, and its ETA is a travel figure rather than a take — the unbounded
                    // reading is the honest one there.
                    let expedition_quarry_mass = match &exp.mission {
                        crate::components::ExpeditionMission::Hunt { fauna_id, .. }
                        | crate::components::ExpeditionMission::Deny { fauna_id, .. } => {
                            herd_registry.find(fauna_id).map(|herd| herd.body_mass)
                        }
                        _ => None,
                    };
                    // **How the party's own gear divides it** — the same seam
                    // `advance_expeditions` resolves the live turn through, so the ETA projects the
                    // crews the party actually fields rather than a uniformly-armed one.
                    let coverage = equipment_config.coverage(
                        &exp.kit,
                        available_workers(cohort.working) as f32,
                        &party_wear,
                    );
                    let party = crate::fauna::PartyResolution {
                        equipment: &equipment_config,
                        coverage: &coverage,
                        wear: &party_wear,
                        intrinsic: kit_levers.person_intrinsic,
                        tuning: expedition_combat_tuning,
                        hunt_injury_damage_per_animal: combat_config.hunt_injury_damage_per_animal,
                    }
                    .party_against(match expedition_quarry_mass {
                        Some(mass) => crate::equipment_config::Quarry::Mass(mass),
                        None => crate::equipment_config::Quarry::Any,
                    });
                    // And the same kit's haul tier — the ETA has to project what THIS party can drag
                    // home, not what a kitted one could.
                    let party_haul = coverage.weighted_rate(|kit| {
                        equipment_config.hunt_per_worker_biomass_capacity(
                            kit_levers.baseline_haul_rate,
                            kit,
                            &party_wear,
                        )
                    });
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
                        &party,
                        party_haul,
                        config.grid_size.x,
                        config.map_topology.wrap_horizontal,
                    )
                });
                population_state(PopulationStateInputs {
                    entity,
                    band_id,
                    cohort,
                    allocation,
                    expedition,
                    current_position: current_pos,
                    is_traveling,
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
                    equipment,
                    kit_levers: &kit_levers,
                    // The take model's roster and the fight's dials, for each hunt row's
                    // `hunt_useful_workers`.
                    hunt_crew_levers: &crate::snapshot::population::HuntCrewLevers {
                        fauna: &fauna_config,
                        combat: &combat_config,
                        // The bare carry rate a **corralled** row's collection curve is resolved
                        // against; a stalked row's kill curve never reads it.
                        baseline_haul_rate: labor_config.hunt.per_worker_biomass_capacity,
                    },
                    bench,
                    // **This band's faction decides which crafts are known**, so the memo is keyed
                    // per faction and resolved lazily — one entry per faction that owns a band,
                    // not one per band.
                    craft_inputs: &crate::snapshot::crafting::BandCraftInputs {
                        materials: &materials_config,
                        equipment: &equipment_config,
                        plans: &craft_offer_plans,
                        known_crafts: known_crafts_by_faction
                            .get(&cohort.faction)
                            .unwrap_or(&NO_CRAFTS_KNOWN),
                        recipes: &recipes_config,
                        // **The ladder's reference job**, resolved once per capture — an equipment
                        // life gauge quotes a build's wear in *gardens' worth*, not in bare work
                        // units, and the garden is the `plant:tended` rung's own `work_cost`.
                        reference_build_cost: ladder_config.reference_build_cost(),
                    },
                    build_sources: &crate::snapshot::population::BuildSourceInputs {
                        forage: &forage_registry,
                        herds: &herd_registry,
                    },
                })
            },
        )
        .collect();
    population_states.sort_unstable_by_key(|state| state.entity);
    drop(populations_scope);

    // Power nodes plus the grid-wide metrics aggregate. Per power node.
    let power_scope = crate::turn_profile::scope("snapshot.build.power");
    let mut power_states: Vec<PowerNodeState> = power_nodes
        .iter()
        .map(|(entity, node)| power_state(entity, node))
        .collect();
    power_states.sort_unstable_by_key(|state| state.entity);

    let power_metrics = power_metrics_from_grid(&power_grid);
    drop(power_scope);

    // Ledger-shaped readouts that walk a resource, not the world: the knowledge ledger's three
    // payload vectors, the generation registry, and the influential roster. Per ledger entry.
    let ledgers_scope = crate::turn_profile::scope("snapshot.build.ledgers");
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
    drop(ledgers_scope);

    // The culture layer/tension lists, copied off `CultureManager`. Per culture layer, and the
    // local layers are one-per-owned-tile, so this one tracks the map.
    let culture_scope = crate::turn_profile::scope("snapshot.build.culture");
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

    drop(culture_scope);

    // The discovery ladder's four readouts plus its telemetry. Per catalogued discovery — a content
    // count, so it grows when the catalog does, never with the map.
    let discovery_scope = crate::turn_profile::scope("snapshot.build.discovery");
    let discovery_states = discovery_progress_entries(&discovery_progress);
    let great_discovery_definition_states = snapshot_definitions(&gds.registry);
    let great_discovery_states = snapshot_discoveries(&gds.ledger);
    let great_discovery_progress_states = snapshot_progress(&gds.readiness);
    let great_discovery_telemetry_state = snapshot_telemetry(&gds.ledger, &gds.telemetry);
    drop(discovery_scope);

    // The contiguous full-grid raster block: terrain, sentiment, corruption, culture,
    // military, visibility. The moisture/elevation overlays are built further down (they need
    // state assembled in between), so they re-enter this same label there — hence `rasters` reports
    // two calls per capture.
    let raster_scope = crate::turn_profile::scope("snapshot.build.rasters");
    let terrain_overlay = terrain_overlay_from_tiles(&tile_states, config.grid_size);
    let sentiment_raster =
        sentiment_raster_from_populations(&tile_states, &population_states, config.grid_size);
    let corruption_raster = corruption_raster_from_simulation(CorruptionRasterInputs {
        tiles: &tile_states,
        populations: &population_states,
        power_nodes: &power_states,
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

    // The four sentiment axes and their itemised driver lists — a derived attribution built by
    // walking the policy/incident/influencer sources and formatting a label per non-zero
    // contribution. Per (influencer + corruption exposure) × 4 axes, so it tracks roster size.
    let sentiment_scope = crate::turn_profile::scope("snapshot.build.sentiment");
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
    drop(sentiment_scope);

    // The crisis heatmap + annotations, cloned wholesale out of the overlay resource. A full-map
    // `Vec` copy, so it belongs with the rasters in shape even though it is not built here.
    let crisis_scope = crate::turn_profile::scope("snapshot.build.crisis");
    let crisis_telemetry_state = crisis_telemetry_state_from_metrics(&metrics.crisis);
    let crisis_overlay_state = CrisisOverlayState {
        heatmap: crisis_overlay.raster.clone(),
        annotations: crisis_overlay.annotations.clone(),
    };
    drop(crisis_scope);

    // Counts, build id and campaign label. O(1).
    let header_scope = crate::turn_profile::scope("snapshot.build.header");
    let mut header = SnapshotHeader::new(
        tick.0,
        tile_states.len(),
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
    drop(header_scope);

    // Second entry into `snapshot.build.rasters` (see the block above) — the two remaining
    // full-grid overlays, which could not be built with the others.
    let raster_scope = crate::turn_profile::scope("snapshot.build.rasters");
    let moisture_overlay_state =
        moisture_overlay_from_resource(moisture.as_ref().map(|res| res.as_ref()), config.grid_size);

    let elevation_overlay_state =
        elevation_overlay_from_field(elevation.as_ref(), config.grid_size);
    drop(raster_scope);
    // The remaining readouts, each a resource → wire-state conversion. Split into `herds`,
    // `forage_patches` and `readouts` because the first two are the ones with a world-sized
    // denominator (herds; forage patches ≈ food-bearing tiles) while the rest are per-faction or
    // per-content-item.
    let readouts_scope = crate::turn_profile::scope("snapshot.build.readouts");
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
    // whether a herd is on visible ground. The unfiltered sim record is the `HerdRegistry` itself,
    // which the checkpoint carries; the snapshot is the view only.
    //
    // Per herd, and derived rather than copied: each entry resolves distance/reach/visibility
    // against the viewer's fog before it is emitted.
    let herds_scope = crate::turn_profile::scope("snapshot.build.herds");
    // **The kit each herd's tables are priced at, resolved once PER SPECIES × SOURCE AXIS.** The
    // default is a pure function of quarry × roster × *is this herd penned*
    // (`fauna::herd_default_hunt_kit`), so resolving it per herd would re-score the same roster for
    // every herd of the same animal; each map is keyed by the display name the herd's own `species`
    // string carries. **Two maps rather than one**, because the axis is a property of the herd and
    // the species is a property of the roster: the range map answers every wild/pastoral herd and
    // the pen map answers a corralled one, so a lookup is still one probe.
    // A fresh ledger to price every kit against — a herd row describes the KIT, not any band's wear
    // on it, and the default itself is resolved at the fresh tier for the same reason.
    let quoted_wear = BandEquipment::start_stocked(&equipment_config);
    let quote_species = |species: &crate::fauna_config::SpeciesDef, corralled: bool| {
        let kit = crate::fauna::herd_default_hunt_kit(
            &equipment_config,
            kit_levers.person_intrinsic,
            species,
            corralled,
        );
        (
            species.display_name.clone(),
            quoted_party_for(
                &equipment_config,
                &combat_config,
                &kit,
                &quoted_wear,
                // **BOUNDED, against this species** — one party per herd is still one party,
                // but it now knows what it is hunting, so a mass-bounded weapon is priced only
                // where it can actually hold the animal. This is what a per-herd resolution
                // buys that the single unbounded party could not express.
                equipment_config.hunter_profile_against(
                    kit_levers.person_intrinsic,
                    &kit,
                    &quoted_wear,
                    species.body_mass,
                ),
            ),
        )
    };
    let quoted_parties: HashMap<String, QuotedParty> = fauna_config
        .species
        .values()
        .map(|species| quote_species(species, HERD_ON_THE_RANGE))
        .collect();
    let penned_parties: HashMap<String, QuotedParty> = fauna_config
        .species
        .values()
        .map(|species| quote_species(species, HERD_IN_A_PEN))
        .collect();
    // **The fallback for a herd whose species the roster cannot resolve** — the hunt job's default,
    // resolved UNBOUNDED because there is no quarry to test a bound against. `EquipmentConfig::
    // validate` rejects a mass-bounded attack in that kit for exactly this reason, so the unbounded
    // resolution here cannot quote a weapon against animals it could not touch.
    let fallback_kit = equipment_config.default_kit(crate::equipment_config::KitJob::Hunt);
    let quoted_fallback = quoted_party_for(
        &equipment_config,
        &combat_config,
        &fallback_kit,
        &quoted_wear,
        equipment_config.hunter_profile_unbounded(
            kit_levers.person_intrinsic,
            &fallback_kit,
            &quoted_wear,
        ),
    );
    // **THE LIVE BUILDERS KIT PER QUEUED SOURCE**, resolved once for both source tables
    // (`docs/plan_standing_upkeep.md` §4.7a ②). It is read off the bands' **queues**, not off the
    // patch/herd scratch beside it: a `build_kit` command is answered by a recapture in the same
    // dispatch, so a turn-written field would show the pick a whole turn late.
    let build_kit_ids = crate::snapshot::subsistence::resolve_build_kit_ids(
        populations
            .iter()
            .filter_map(|(_, _, allocation, ..)| allocation),
        &forage_registry,
        &herd_registry,
        &equipment_config,
    );
    // **THE LIVE KEEPING KIT PER WORKED SOURCE**, on the same rule one account over
    // (`docs/plan_standing_upkeep.md` §2.7): the keeping kit is a property of the band's **row**, so
    // it is read off the rows rather than off the patch/herd scratch, and an `upkeep_kit` command is
    // answered by a recapture in the same dispatch.
    let upkeep_kit_ids = crate::snapshot::subsistence::resolve_upkeep_kits(
        populations
            .iter()
            .filter_map(|(_, _, allocation, ..)| allocation),
        &equipment_config,
    );
    let herd_states = herd_snapshot_entries(HerdSnapshotInputs {
        telemetry: &herds,
        registry: &herd_registry,
        fauna: &fauna_config,
        ladder: &ladder_config,
        // **The EQUIPPED reference haul rate, off the item table's default tier** — a herd row has
        // no band to resolve a sled tier against, and `labor_config`'s key is the sledless baseline
        // since the carries moved onto their tiers.
        equipped_haul_rate: equipment_config.equipped_reference(
            crate::equipment_config::EquipmentStat::HuntCarry,
            labor_config.hunt.per_worker_biomass_capacity,
        ),
        grid_size: config.grid_size,
        wrap_horizontal: config.map_topology.wrap_horizontal,
        // **The graze layer the destination-capacity quote is struck over** — the same registry the
        // live `K` is summed from, so the two are one seam at two standings.
        graze: &graze_registry,
        visibility: &visibility_ledger,
        viewer: viewer_faction.0,
        fog_enabled: config.fog_enabled,
        // **THIS QUARRY'S own default kit, deliberately** — the herd row is a fact about the herd
        // and has no band to ask, but it can ask the *animal*, so each species' row is quoted at
        // the kit its compose sheet opens on and **publishes which**. A fresh kit
        // (`BandEquipment::start_stocked`), because the row describes the kit rather than
        // any band's wear on it.
        //
        // **This prices the per-worker YIELD row only.** The two pre-launch estimate tables that
        // used to be quoted here are gone — `crate::forecast_query` answers them per band, per kit,
        // per exact party and floor, on demand — and with them went the sled tier only they read
        // and `range_sigmas` (the denial readout's band width).
        parties: &quoted_parties,
        penned_parties: &penned_parties,
        fallback_party: &quoted_fallback,
        build_kits: &build_kit_ids,
        upkeep_kits: &upkeep_kit_ids,
    });
    drop(herds_scope);
    let faction_inventory_state = snapshot_faction_inventory(&faction_inventory);
    let sedentarization_state = snapshot_sedentarization(&sedentarization);
    let discovered_sites_state = snapshot_discovered_sites(&discovered_sites, &sites_config);
    // **Faction is a property of the ENDPOINT** — resolved here, once, so the connection ledger
    // itself never carries one. An edge whose observer band is gone resolves to nothing and is
    // filtered out rather than published against a guessed faction.
    let band_factions: HashMap<BandId, FactionId> = populations
        .iter()
        .filter_map(|(_, cohort, _, _, _, band_id, _, _)| {
            band_id.map(|band| (*band, cohort.faction))
        })
        .collect();
    let connections_state = crate::snapshot::connections::connection_states(
        &connections,
        &band_factions,
        viewer_faction.0,
    );
    // **THE ROADS THE VIEWER HAS EXPLORED** — fog-gated on `Discovered` rather than the herd list's
    // `Active`, because a road does not wander off. See `snapshot::routes::route_states`.
    let route_states = crate::snapshot::routes::route_states(
        &roads,
        &visibility_ledger,
        viewer_faction.0,
        config.fog_enabled,
        &ladder_config,
        |pos| {
            tile_registry
                .index(pos.x, pos.y)
                .and_then(|entity| tiles.get(entity).ok())
                .map(|(_, tile, _)| tile.terrain)
        },
    );
    let demographics_state = snapshot_demographics(&population_states);
    // Per forage patch — one per food-bearing tile — and every entry re-derives the rung ladder's
    // quotes for that patch, so this one is both map-sized and derivation-heavy.
    let forage_patches_scope = crate::turn_profile::scope("snapshot.build.forage_patches");
    let forage_patches_state = snapshot_forage_patches(
        &forage_registry,
        &labor_config.forage,
        // The gather twin of the herd row's haul rate above — the basket's equipped tier, because a
        // patch has no band either.
        equipment_config.equipped_reference(
            crate::equipment_config::EquipmentStat::ForageCarry,
            labor_config.forage.per_worker_biomass_capacity,
        ),
        &flora_config,
        &ladder_config,
        &seasonal_weights,
        &sow_site_refusals,
        &tile_capacities,
        &flora_quotes,
        &build_kit_ids,
        &upkeep_kit_ids,
    );
    drop(forage_patches_scope);
    let intensification_knowledge_state =
        snapshot_intensification_knowledge(&discovery_progress, &ladder_config);
    let ladder_knowledge_state = snapshot_ladder_knowledge(&ladder_config);
    // **THE ROUTE BRANCH'S RUNG CATALOG** — what a road may become, beside what there is to learn.
    // A per-world constant like the roster above, so it diffs out after the first frame.
    let route_rung_state = snapshot_route_rungs(&ladder_config);
    let command_events_state = command_events_to_state(&command_events);
    // The Telling's client-facing fork tier + stance readout (BTree-backed, so already ordered).
    let pending_forks_state = snapshot_pending_forks(&beat_ledger);
    let stance_axes_state = snapshot_stance_axes(&beat_ledger);
    let voice_medium_state = snapshot_voice_medium(&beat_ledger);
    let victory_snapshot_state = victory_snapshot_from_resource(&victory);
    let capability_bits = capability_flags.bits();
    drop(readouts_scope);

    // The struct literal itself. Every field the publisher compares whole-section is `.clone()`d
    // into it, so this is a second full copy of the rasters, the herd list, the forage patches and
    // the crisis heatmap — proportional to the assembled snapshot's own byte size.
    let assemble_scope = crate::turn_profile::scope("snapshot.build.assemble");
    // **THE KIT ROSTER, once per world** — the picker's list plus the tiers each kit grants, so the
    // client renders real numbers without a second copy of the TOE table. A per-world constant, so
    // it diffs out on every frame after the first.
    let kit_states = kit_roster_states(&equipment_config, &labor_config, &kit_levers);
    let equipment_config_json = serialize_equipment_config(&equipment_config);
    // **The three per-world catalogues plus the learned one.** The first three are `Whole` baselines
    // like the kit roster and diff out on every frame after the first; `craft_knowledge` genuinely
    // moves, because a craft is learned by making things.
    let material_catalogue =
        crate::snapshot::crafting::material_catalogue(&materials_config, &equipment_config);
    let characteristic_band_catalogue =
        crate::snapshot::crafting::characteristic_band_catalogue(&materials_config);
    let recipe_catalogue =
        crate::snapshot::crafting::recipe_catalogue(&recipes_config, &equipment_config);
    let craft_knowledge_states = crate::snapshot::crafting::craft_knowledge_states(
        &materials_config,
        &discovery_progress,
        knowledge_threshold,
    );
    let assembled = WorldSnapshot {
        header,
        kits: kit_states,
        materials: material_catalogue,
        characteristic_bands: characteristic_band_catalogue,
        recipes: recipe_catalogue,
        craft_knowledge: craft_knowledge_states,
        equipment_config_json,
        default_hunt_kit_id: equipment_config
            .default_kit_id(crate::equipment_config::KitJob::Hunt)
            .to_string(),
        default_forage_kit_id: equipment_config
            .default_kit_id(crate::equipment_config::KitJob::Forage)
            .to_string(),
        default_scout_kit_id: equipment_config
            .default_kit_id(crate::equipment_config::KitJob::Scout)
            .to_string(),
        default_warrior_kit_id: equipment_config
            .default_kit_id(crate::equipment_config::KitJob::Warrior)
            .to_string(),
        tiles: tile_states,
        populations: population_states,
        power: power_states,
        power_metrics: power_metrics.clone(),
        terrain: terrain_overlay.clone(),
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
        food_modules: food_module_states.clone(),
        campaign_profiles: campaign_profiles_state,
        faction_inventory: faction_inventory_state.clone(),
        sedentarization: sedentarization_state.clone(),
        discovered_sites: discovered_sites_state.clone(),
        connections: connections_state.clone(),
        routes: route_states.clone(),
        demographics: demographics_state.clone(),
        forage_patches: forage_patches_state.clone(),
        intensification_knowledge: intensification_knowledge_state.clone(),
        ladder_knowledge: ladder_knowledge_state.clone(),
        route_rungs: route_rung_state.clone(),
        command_events: command_events_state.clone(),
        command_events_retention_turns: command_events.retention_turns() as u32,
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
    drop(assemble_scope);
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
    // **Nothing publishes while a rollback is replaying.** `capture_snapshot` is gated on
    // `not_replaying` in the turn schedule, but this path reaches it through `run_system_once`,
    // which invokes the system directly — run conditions are a schedule feature and do not apply.
    // So the schedule's gate covers the turn path and nothing else.
    //
    // The check is *here*, not at the call sites, for the same reason the command seam is uniform:
    // a curated list of "callers that must not publish during replay" is a thing to forget, and any
    // caller reaching this during replay is wrong by definition — a rollback is one publication.
    if world
        .get_resource::<crate::sim_state::Replaying>()
        .is_some_and(|replaying| replaying.0)
    {
        return;
    }
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = true;
    world.run_system_once(capture_snapshot);
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = false;
}
