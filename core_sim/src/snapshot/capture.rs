use super::*;

pub(crate) type EncodedBuffers = (Arc<Vec<u8>>, Arc<Vec<u8>>);

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
    pub fog_reveals: Res<'w, FogRevealLedger>,
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
    pub encoded_snapshot: Arc<Vec<u8>>,
    pub encoded_delta: Arc<Vec<u8>>,
    pub encoded_snapshot_flat: Arc<Vec<u8>>,
    pub encoded_delta_flat: Arc<Vec<u8>>,
}

impl StoredSnapshot {
    fn new(snapshot: Arc<WorldSnapshot>, delta: Arc<WorldDelta>) -> Self {
        let encoded_snapshot =
            Arc::new(encode_snapshot(snapshot.as_ref()).expect("snapshot serialization failed"));
        let encoded_delta =
            Arc::new(encode_delta(delta.as_ref()).expect("delta serialization failed"));
        let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(snapshot.as_ref()));
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta.as_ref()));
        Self {
            tick: snapshot.header.tick,
            snapshot,
            delta,
            encoded_snapshot,
            encoded_delta,
            encoded_snapshot_flat,
            encoded_delta_flat,
        }
    }
}

#[derive(Resource)]
pub struct SnapshotHistory {
    capacity: usize,
    pub last_snapshot: Option<Arc<WorldSnapshot>>,
    pub last_delta: Option<Arc<WorldDelta>>,
    pub encoded_snapshot: Option<Arc<Vec<u8>>>,
    pub encoded_delta: Option<Arc<Vec<u8>>>,
    pub encoded_snapshot_flat: Option<Arc<Vec<u8>>>,
    pub encoded_delta_flat: Option<Arc<Vec<u8>>>,
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
    fog_raster: ScalarRasterState,
    visibility_raster: ScalarRasterState,
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
    command_events: Vec<CommandEventState>,
    pending_forks: Vec<PendingForksState>,
    stance_axes: Vec<StanceState>,
    voice_medium: Vec<VoiceMediumState>,
    herds: Vec<HerdTelemetryState>,
    food_modules: Vec<FoodModuleState>,
    history: VecDeque<StoredSnapshot>,
}

impl Default for SnapshotHistory {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl SnapshotHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            last_snapshot: None,
            last_delta: None,
            encoded_snapshot: None,
            encoded_delta: None,
            encoded_snapshot_flat: None,
            encoded_delta_flat: None,
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
            fog_raster: ScalarRasterState::default(),
            visibility_raster: ScalarRasterState::default(),
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
            command_events: Vec::new(),
            pending_forks: Vec::new(),
            stance_axes: Vec::new(),
            voice_medium: Vec::new(),
            herds: Vec::new(),
            food_modules: Vec::new(),
            history: VecDeque::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        self.prune();
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    pub fn latest_entry(&self) -> Option<StoredSnapshot> {
        self.history.back().cloned()
    }

    pub fn entry(&self, tick: u64) -> Option<StoredSnapshot> {
        self.history
            .iter()
            .find(|entry| entry.tick == tick)
            .cloned()
    }

    pub fn update(&mut self, snapshot: WorldSnapshot) {
        let mut tiles_index = HashMap::with_capacity(snapshot.tiles.len());
        for state in &snapshot.tiles {
            tiles_index.insert(state.entity, state.clone());
        }

        let mut logistics_index = HashMap::with_capacity(snapshot.logistics.len());
        for state in &snapshot.logistics {
            logistics_index.insert(state.entity, state.clone());
        }

        let mut trade_index = HashMap::with_capacity(snapshot.trade_links.len());
        for state in &snapshot.trade_links {
            trade_index.insert(state.entity, state.clone());
        }

        let mut populations_index = HashMap::with_capacity(snapshot.populations.len());
        for state in &snapshot.populations {
            populations_index.insert(state.entity, state.clone());
        }

        let mut power_index = HashMap::with_capacity(snapshot.power.len());
        for state in &snapshot.power {
            power_index.insert(state.entity, state.clone());
        }

        let mut generations_index = HashMap::with_capacity(snapshot.generations.len());
        for state in &snapshot.generations {
            generations_index.insert(state.id, state.clone());
        }

        let mut influencers_index = HashMap::with_capacity(snapshot.influencers.len());
        for state in &snapshot.influencers {
            influencers_index.insert(state.id, state.clone());
        }

        let mut culture_layers_index = HashMap::with_capacity(snapshot.culture_layers.len());
        for state in &snapshot.culture_layers {
            culture_layers_index.insert(state.id, state.clone());
        }

        let mut discovery_index = HashMap::with_capacity(snapshot.discovery_progress.len());
        for entry in &snapshot.discovery_progress {
            discovery_index.insert((entry.faction, entry.discovery), entry.clone());
        }

        let axis_bias_state = snapshot.axis_bias.clone();
        let axis_bias_delta = if self.axis_bias == axis_bias_state {
            None
        } else {
            Some(axis_bias_state.clone())
        };

        let sentiment_state = snapshot.sentiment.clone();
        let sentiment_delta = if self.sentiment == sentiment_state {
            None
        } else {
            Some(sentiment_state.clone())
        };

        let culture_tensions_state = snapshot.culture_tensions.clone();
        let delta_culture_tensions = if self.culture_tensions == culture_tensions_state {
            Vec::new()
        } else {
            culture_tensions_state.clone()
        };

        let terrain_state = snapshot.terrain.clone();
        let terrain_delta = if self.terrain_overlay == terrain_state {
            None
        } else {
            Some(terrain_state.clone())
        };

        let moisture_state = snapshot.moisture_raster.clone();
        let moisture_delta = if self.moisture_raster == moisture_state {
            None
        } else {
            Some(moisture_state.clone())
        };

        let elevation_state = snapshot.elevation_overlay.clone();
        let elevation_delta = if self.elevation_overlay == elevation_state {
            None
        } else {
            Some(elevation_state.clone())
        };

        // A per-map constant: it changes only on (re)generation, so the delta re-sends it just then.
        let climate_bands_state = snapshot.climate_bands;
        let climate_bands_delta = if self.climate_bands == climate_bands_state {
            None
        } else {
            Some(climate_bands_state)
        };

        let start_marker_state = snapshot.start_marker.clone();
        let start_marker_delta = if self.start_marker == start_marker_state {
            None
        } else {
            start_marker_state.clone()
        };

        let logistics_raster_state = snapshot.logistics_raster.clone();
        let logistics_raster_delta = if self.logistics_raster == logistics_raster_state {
            None
        } else {
            Some(logistics_raster_state.clone())
        };

        let sentiment_raster_state = snapshot.sentiment_raster.clone();
        let sentiment_raster_delta = if self.sentiment_raster == sentiment_raster_state {
            None
        } else {
            Some(sentiment_raster_state.clone())
        };

        let corruption_raster_state = snapshot.corruption_raster.clone();
        let corruption_raster_delta = if self.corruption_raster == corruption_raster_state {
            None
        } else {
            Some(corruption_raster_state.clone())
        };

        let fog_raster_state = snapshot.fog_raster.clone();
        let fog_raster_delta = if self.fog_raster == fog_raster_state {
            None
        } else {
            Some(fog_raster_state.clone())
        };

        let visibility_raster_state = snapshot.visibility_raster.clone();
        let visibility_raster_delta = if self.visibility_raster == visibility_raster_state {
            None
        } else {
            Some(visibility_raster_state.clone())
        };

        let culture_raster_state = snapshot.culture_raster.clone();
        let culture_raster_delta = if self.culture_raster == culture_raster_state {
            None
        } else {
            Some(culture_raster_state.clone())
        };

        let military_raster_state = snapshot.military_raster.clone();
        let military_raster_delta = if self.military_raster == military_raster_state {
            None
        } else {
            Some(military_raster_state.clone())
        };

        let mut great_discovery_definitions_index =
            HashMap::with_capacity(snapshot.great_discovery_definitions.len());
        for state in &snapshot.great_discovery_definitions {
            great_discovery_definitions_index.insert(state.id, state.clone());
        }
        let great_discovery_definitions_delta =
            if self.great_discovery_definitions == great_discovery_definitions_index {
                None
            } else {
                Some(snapshot.great_discovery_definitions.clone())
            };

        let mut great_discoveries_index = HashMap::with_capacity(snapshot.great_discoveries.len());
        for state in &snapshot.great_discoveries {
            great_discoveries_index.insert((state.faction, state.id), state.clone());
        }

        let mut great_discovery_progress_index =
            HashMap::with_capacity(snapshot.great_discovery_progress.len());
        for state in &snapshot.great_discovery_progress {
            great_discovery_progress_index.insert((state.faction, state.discovery), state.clone());
        }

        let great_discovery_telemetry_state = snapshot.great_discovery_telemetry.clone();
        let great_discovery_telemetry_delta =
            if self.great_discovery_telemetry == great_discovery_telemetry_state {
                None
            } else {
                Some(great_discovery_telemetry_state.clone())
            };

        let power_metrics_state = snapshot.power_metrics.clone();
        let power_metrics_delta = if self.power_metrics == power_metrics_state {
            None
        } else {
            Some(power_metrics_state.clone())
        };

        let corruption_state = snapshot.corruption.clone();
        let corruption_delta = if self.corruption == corruption_state {
            None
        } else {
            Some(corruption_state.clone())
        };

        let mut knowledge_ledger_index = HashMap::with_capacity(snapshot.knowledge_ledger.len());
        for entry in &snapshot.knowledge_ledger {
            knowledge_ledger_index.insert(
                encode_ledger_key(FactionId(entry.owner_faction), entry.discovery_id),
                entry.clone(),
            );
        }

        let knowledge_metrics_state = snapshot.knowledge_metrics.clone();
        let knowledge_metrics_delta = if self.knowledge_metrics == knowledge_metrics_state {
            None
        } else {
            Some(knowledge_metrics_state.clone())
        };

        let knowledge_timeline_delta = if self.knowledge_timeline == snapshot.knowledge_timeline {
            Vec::new()
        } else {
            snapshot.knowledge_timeline.clone()
        };

        let crisis_telemetry_state = snapshot.crisis_telemetry.clone();
        let crisis_telemetry_delta = if self.crisis_telemetry == crisis_telemetry_state {
            None
        } else {
            Some(crisis_telemetry_state.clone())
        };

        let crisis_overlay_state = snapshot.crisis_overlay.clone();
        let crisis_overlay_delta = if self.crisis_overlay == crisis_overlay_state {
            None
        } else {
            Some(crisis_overlay_state.clone())
        };

        let victory_state = snapshot.victory.clone();
        let victory_delta = if self.victory == victory_state {
            None
        } else {
            Some(victory_state.clone())
        };
        let faction_inventory_state = snapshot.faction_inventory.clone();
        let faction_inventory_delta = if self.faction_inventory == faction_inventory_state {
            None
        } else {
            Some(faction_inventory_state.clone())
        };
        let sedentarization_state = snapshot.sedentarization.clone();
        let sedentarization_delta = if self.sedentarization == sedentarization_state {
            None
        } else {
            Some(sedentarization_state.clone())
        };
        let discovered_sites_state = snapshot.discovered_sites.clone();
        let discovered_sites_delta = if self.discovered_sites == discovered_sites_state {
            None
        } else {
            Some(discovered_sites_state.clone())
        };
        let demographics_state = snapshot.demographics.clone();
        let demographics_delta = if self.demographics == demographics_state {
            None
        } else {
            Some(demographics_state.clone())
        };
        let forage_patches_state = snapshot.forage_patches.clone();
        let forage_patches_delta = if self.forage_patches == forage_patches_state {
            None
        } else {
            Some(forage_patches_state.clone())
        };
        let intensification_knowledge_state = snapshot.intensification_knowledge.clone();
        let intensification_knowledge_delta =
            if self.intensification_knowledge == intensification_knowledge_state {
                None
            } else {
                Some(intensification_knowledge_state.clone())
            };
        let command_events_state = snapshot.command_events.clone();
        let command_events_delta = if self.command_events == command_events_state {
            None
        } else {
            Some(command_events_state.clone())
        };
        let pending_forks_state = snapshot.pending_forks.clone();
        let pending_forks_delta = if self.pending_forks == pending_forks_state {
            None
        } else {
            Some(pending_forks_state.clone())
        };
        let stance_axes_state = snapshot.stance_axes.clone();
        let stance_axes_delta = if self.stance_axes == stance_axes_state {
            None
        } else {
            Some(stance_axes_state.clone())
        };
        let voice_medium_state = snapshot.voice_medium.clone();
        let voice_medium_delta = if self.voice_medium == voice_medium_state {
            None
        } else {
            Some(voice_medium_state.clone())
        };
        let capability_flags_state = snapshot.capability_flags;
        let capability_flags_delta = if self.capability_flags == capability_flags_state {
            None
        } else {
            Some(capability_flags_state)
        };
        let herd_state = snapshot.herds.clone();
        let herds_delta = if self.herds == herd_state {
            None
        } else {
            Some(herd_state.clone())
        };
        let food_modules_state = snapshot.food_modules.clone();
        let food_modules_delta = if self.food_modules == food_modules_state {
            None
        } else {
            Some(food_modules_state.clone())
        };

        let delta = WorldDelta {
            header: snapshot.header.clone(),
            tiles: diff_new(&self.tiles, &tiles_index),
            removed_tiles: diff_removed(&self.tiles, &tiles_index),
            logistics: diff_new(&self.logistics, &logistics_index),
            removed_logistics: diff_removed(&self.logistics, &logistics_index),
            trade_links: diff_new(&self.trade_links, &trade_index),
            removed_trade_links: diff_removed(&self.trade_links, &trade_index),
            populations: diff_new(&self.populations, &populations_index),
            removed_populations: diff_removed(&self.populations, &populations_index),
            power: diff_new(&self.power, &power_index),
            removed_power: diff_removed(&self.power, &power_index),
            power_metrics: power_metrics_delta.clone(),
            great_discovery_definitions: great_discovery_definitions_delta.clone(),
            great_discoveries: diff_new(&self.great_discoveries, &great_discoveries_index),
            great_discovery_progress: diff_new(
                &self.great_discovery_progress,
                &great_discovery_progress_index,
            ),
            great_discovery_telemetry: great_discovery_telemetry_delta.clone(),
            knowledge_ledger: diff_new(&self.knowledge_ledger, &knowledge_ledger_index),
            removed_knowledge_ledger: diff_removed(&self.knowledge_ledger, &knowledge_ledger_index),
            knowledge_metrics: knowledge_metrics_delta.clone(),
            victory: victory_delta.clone(),
            capability_flags: capability_flags_delta,
            command_events: command_events_delta.clone(),
            pending_forks: pending_forks_delta.clone(),
            stance_axes: stance_axes_delta.clone(),
            voice_medium: voice_medium_delta.clone(),
            faction_inventory: faction_inventory_delta.clone(),
            sedentarization: sedentarization_delta.clone(),
            discovered_sites: discovered_sites_delta.clone(),
            demographics: demographics_delta.clone(),
            forage_patches: forage_patches_delta.clone(),
            intensification_knowledge: intensification_knowledge_delta.clone(),
            herds: herds_delta.clone(),
            food_modules: food_modules_delta.clone(),
            knowledge_timeline: knowledge_timeline_delta.clone(),
            crisis_telemetry: crisis_telemetry_delta.clone(),
            crisis_overlay: crisis_overlay_delta.clone(),
            moisture_raster: moisture_delta.clone(),
            elevation_overlay: elevation_delta.clone(),
            climate_bands: climate_bands_delta,
            start_marker: start_marker_delta.clone(),
            axis_bias: axis_bias_delta,
            sentiment: sentiment_delta.clone(),
            generations: diff_new(&self.generations, &generations_index),
            removed_generations: diff_removed(&self.generations, &generations_index),
            corruption: corruption_delta.clone(),
            influencers: diff_new(&self.influencers, &influencers_index),
            removed_influencers: diff_removed(&self.influencers, &influencers_index),
            terrain: terrain_delta.clone(),
            logistics_raster: logistics_raster_delta.clone(),
            sentiment_raster: sentiment_raster_delta.clone(),
            corruption_raster: corruption_raster_delta.clone(),
            fog_raster: fog_raster_delta.clone(),
            culture_raster: culture_raster_delta.clone(),
            military_raster: military_raster_delta.clone(),
            culture_layers: diff_new(&self.culture_layers, &culture_layers_index),
            removed_culture_layers: diff_removed(&self.culture_layers, &culture_layers_index),
            culture_tensions: delta_culture_tensions.clone(),
            discovery_progress: diff_new(&self.discovery_progress, &discovery_index),
            visibility_raster: visibility_raster_delta.clone(),
        };

        let snapshot_arc = Arc::new(snapshot);
        let delta_arc = Arc::new(delta);
        let stored = StoredSnapshot::new(snapshot_arc.clone(), delta_arc.clone());

        self.tiles = tiles_index;
        self.logistics = logistics_index;
        self.trade_links = trade_index;
        self.populations = populations_index;
        self.power = power_index;
        self.power_metrics = power_metrics_state;
        self.great_discovery_definitions = great_discovery_definitions_index;
        self.great_discoveries = great_discoveries_index;
        self.great_discovery_progress = great_discovery_progress_index;
        self.great_discovery_telemetry = great_discovery_telemetry_state;
        self.knowledge_ledger = knowledge_ledger_index;
        self.knowledge_metrics = knowledge_metrics_state;
        self.knowledge_timeline = snapshot_arc.knowledge_timeline.clone();
        self.crisis_telemetry = crisis_telemetry_state;
        self.crisis_overlay = crisis_overlay_state;
        self.generations = generations_index;
        self.influencers = influencers_index;
        self.culture_layers = culture_layers_index;
        self.axis_bias = axis_bias_state;
        self.sentiment = sentiment_state;
        self.terrain_overlay = terrain_state;
        self.elevation_overlay = elevation_state;
        self.climate_bands = climate_bands_state;
        self.start_marker = start_marker_state;
        self.logistics_raster = logistics_raster_state;
        self.sentiment_raster = sentiment_raster_state;
        self.corruption_raster = corruption_raster_state;
        self.fog_raster = fog_raster_state;
        self.visibility_raster = visibility_raster_state;
        self.culture_raster = culture_raster_state;
        self.military_raster = military_raster_state;
        self.moisture_raster = moisture_state;
        self.corruption = corruption_state;
        self.culture_tensions = culture_tensions_state;
        self.discovery_progress = discovery_index;
        self.victory = victory_state;
        self.capability_flags = capability_flags_state;
        self.faction_inventory = faction_inventory_state;
        self.sedentarization = sedentarization_state;
        self.discovered_sites = discovered_sites_state;
        self.demographics = demographics_state;
        self.forage_patches = forage_patches_state;
        self.intensification_knowledge = intensification_knowledge_state;
        self.command_events = command_events_state;
        self.pending_forks = pending_forks_state;
        self.stance_axes = stance_axes_state;
        self.voice_medium = voice_medium_state;
        self.herds = herd_state;
        self.food_modules = food_modules_state;
        self.last_snapshot = Some(snapshot_arc);
        self.last_delta = Some(delta_arc);
        self.encoded_snapshot = Some(stored.encoded_snapshot.clone());
        self.encoded_delta = Some(stored.encoded_delta.clone());
        self.encoded_snapshot_flat = Some(stored.encoded_snapshot_flat.clone());
        self.encoded_delta_flat = Some(stored.encoded_delta_flat.clone());
        self.history.push_back(stored);
        self.prune();
    }

    pub fn reset_to_entry(&mut self, entry: &StoredSnapshot) {
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
        self.fog_raster = entry.snapshot.fog_raster.clone();
        self.visibility_raster = entry.snapshot.visibility_raster.clone();
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
        self.encoded_snapshot = Some(entry.encoded_snapshot.clone());
        self.encoded_delta = Some(entry.encoded_delta.clone());
        self.encoded_snapshot_flat = Some(entry.encoded_snapshot_flat.clone());
        self.encoded_delta_flat = Some(entry.encoded_delta_flat.clone());

        while let Some(back) = self.history.back() {
            if back.tick > entry.tick {
                self.history.pop_back();
            } else {
                break;
            }
        }
    }

    pub fn update_axis_bias(&mut self, bias: AxisBiasState) -> Option<EncodedBuffers> {
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
            knowledge_timeline: Vec::new(),
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
            fog_raster: None,
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
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: None,
        };

        let delta_arc = Arc::new(delta);
        let encoded_delta =
            Arc::new(encode_delta(delta_arc.as_ref()).expect("axis bias delta encoding failed"));
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());
        self.encoded_delta = Some(encoded_delta.clone());
        self.encoded_delta_flat = Some(encoded_delta_flat.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.axis_bias = bias.clone();
            let snapshot = snapshot.finalize();
            let encoded_snapshot =
                Arc::new(encode_snapshot(&snapshot).expect("axis bias snapshot encoding failed"));
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot = Some(encoded_snapshot.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc;
                back.encoded_snapshot = encoded_snapshot;
                back.encoded_snapshot_flat = encoded_snapshot_flat;
            }
        }

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
            back.encoded_delta = encoded_delta.clone();
            back.encoded_delta_flat = encoded_delta_flat.clone();
        }

        Some((encoded_delta, encoded_delta_flat))
    }

    pub fn update_command_events(
        &mut self,
        events: Vec<CommandEventState>,
    ) -> Option<EncodedBuffers> {
        if self.command_events == events {
            return None;
        }

        let last_snapshot = self.last_snapshot.as_ref()?;

        self.command_events = events.clone();

        let mut snapshot = (**last_snapshot).clone();
        snapshot.command_events = events.clone();

        let snapshot_arc = Arc::new(snapshot);
        let encoded_snapshot = Arc::new(
            encode_snapshot(snapshot_arc.as_ref()).expect("command event snapshot encoding failed"),
        );
        let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(snapshot_arc.as_ref()));

        self.last_snapshot = Some(snapshot_arc.clone());
        self.encoded_snapshot = Some(encoded_snapshot.clone());
        self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
        if let Some(back) = self.history.back_mut() {
            back.snapshot = snapshot_arc.clone();
            back.encoded_snapshot = encoded_snapshot.clone();
            back.encoded_snapshot_flat = encoded_snapshot_flat.clone();
        }

        Some((encoded_snapshot, encoded_snapshot_flat))
    }

    /// Refresh the latest broadcast snapshot from a **freshly-captured** `WorldSnapshot` of the
    /// current world, **in place** — mirroring [`Self::update_command_events`] but replacing the
    /// whole world state, not just the feed. Used by the post-command re-capture path so a
    /// world-mutating command (expedition launch, `move_band`, `assign_labor`, …) is reflected in
    /// the client's snapshot immediately, without waiting for the next turn.
    ///
    /// Like `update_command_events` this updates `last_snapshot` + the current back ring entry (so a
    /// rollback to this tick restores the post-command world) and re-encodes for broadcast, but it
    /// **does not** push a new ring entry or touch the delta baselines (`self.tiles`/`populations`/…)
    /// — so the rollback ring stays one-entry-per-tick and the next turn's delta still carries these
    /// structural changes (a redundant but idempotent re-send on top of this full snapshot). Never
    /// advances the turn or the `TurnQueue`.
    pub fn refresh_latest(&mut self, snapshot: WorldSnapshot) -> Option<EncodedBuffers> {
        let snapshot_arc = Arc::new(snapshot);
        let encoded_snapshot = Arc::new(
            encode_snapshot(snapshot_arc.as_ref()).expect("recapture snapshot encoding failed"),
        );
        let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(snapshot_arc.as_ref()));

        self.last_snapshot = Some(snapshot_arc.clone());
        self.encoded_snapshot = Some(encoded_snapshot.clone());
        self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
        if let Some(back) = self.history.back_mut() {
            back.snapshot = snapshot_arc.clone();
            back.encoded_snapshot = encoded_snapshot.clone();
            back.encoded_snapshot_flat = encoded_snapshot_flat.clone();
        }

        Some((encoded_snapshot, encoded_snapshot_flat))
    }
    pub fn update_influencers(
        &mut self,
        states: Vec<InfluentialIndividualState>,
    ) -> Option<EncodedBuffers> {
        let mut index = HashMap::with_capacity(states.len());
        for state in &states {
            index.insert(state.id, state.clone());
        }

        if index == self.influencers {
            return None;
        }

        let added = diff_new(&self.influencers, &index);
        let removed = diff_removed(&self.influencers, &index);

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
            knowledge_timeline: Vec::new(),
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
            fog_raster: None,
            culture_raster: None,
            military_raster: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: None,
            influencers: added.clone(),
            removed_influencers: removed.clone(),
            terrain: None,
            culture_layers: Vec::new(),
            removed_culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: None,
        };

        let delta_arc = Arc::new(delta);
        let encoded_delta =
            Arc::new(encode_delta(delta_arc.as_ref()).expect("influencer delta encoding failed"));
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());
        self.encoded_delta = Some(encoded_delta.clone());
        self.encoded_delta_flat = Some(encoded_delta_flat.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.influencers = states.clone();
            snapshot.header.influencer_count = states.len() as u32;
            let snapshot = snapshot.finalize();
            let encoded_snapshot =
                Arc::new(encode_snapshot(&snapshot).expect("influencer snapshot encoding failed"));
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot = Some(encoded_snapshot.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc.clone();
                back.encoded_snapshot = encoded_snapshot.clone();
                back.encoded_snapshot_flat = encoded_snapshot_flat.clone();
            }
        }

        self.influencers = index;

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
            back.encoded_delta = encoded_delta.clone();
            back.encoded_delta_flat = encoded_delta_flat.clone();
        }

        Some((encoded_delta, encoded_delta_flat))
    }

    pub fn update_corruption(&mut self, ledger: CorruptionLedger) -> Option<EncodedBuffers> {
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
            knowledge_timeline: Vec::new(),
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
            fog_raster: None,
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
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: None,
        };

        let delta_arc = Arc::new(delta);
        let encoded_delta =
            Arc::new(encode_delta(delta_arc.as_ref()).expect("corruption delta encoding failed"));
        let encoded_delta_flat = Arc::new(encode_delta_flatbuffer(delta_arc.as_ref()));
        self.last_delta = Some(delta_arc.clone());
        self.encoded_delta = Some(encoded_delta.clone());
        self.encoded_delta_flat = Some(encoded_delta_flat.clone());

        if let Some(previous_snapshot) = self.last_snapshot.take() {
            let mut snapshot = (*previous_snapshot).clone();
            snapshot.corruption = ledger.clone();
            let snapshot = snapshot.finalize();
            let encoded_snapshot =
                Arc::new(encode_snapshot(&snapshot).expect("corruption snapshot encoding failed"));
            let encoded_snapshot_flat = Arc::new(encode_snapshot_flatbuffer(&snapshot));
            let snapshot_arc = Arc::new(snapshot);
            self.last_snapshot = Some(snapshot_arc.clone());
            self.encoded_snapshot = Some(encoded_snapshot.clone());
            self.encoded_snapshot_flat = Some(encoded_snapshot_flat.clone());
            if let Some(back) = self.history.back_mut() {
                back.snapshot = snapshot_arc.clone();
                back.encoded_snapshot = encoded_snapshot.clone();
                back.encoded_snapshot_flat = encoded_snapshot_flat.clone();
            }
        }

        if let Some(back) = self.history.back_mut() {
            back.delta = delta_arc.clone();
            back.encoded_delta = encoded_delta.clone();
            back.encoded_delta_flat = encoded_delta_flat.clone();
        }

        Some((encoded_delta, encoded_delta_flat))
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
        fog_reveals,
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
    history.set_capacity(config.snapshot_history_limit.max(1));

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
    // Per-tile terrain tags, keyed by coord — what the fresh-water half of the `plant:field` rung's
    // site rule reads about a tile's NEIGHBOURS (`forage::tile_is_fresh_watered`). Collected in this
    // pass so the refusal sweep below is one more read of the same query rather than a second walk of
    // the world.
    let mut tile_tags: HashMap<UVec2, sim_runtime::TerrainTags> = HashMap::new();
    for (entity, tile, food_module) in tiles.iter() {
        tile_states.push(tile_state(
            entity,
            tile,
            &morale_pressure_cfg,
            graze_registry.patch(tile.position),
            &labor_config.forage,
        ));
        tile_tags.insert(tile.position, tile.terrain_tags);
        if let Some(module) = food_module {
            seasonal_weights.insert(tile.position, module.seasonal_weight);
        }
    }
    // **Why the ground under each patch will not take seed** — the `plant:field` rung's own
    // `site_requirement`, resolved through the SAME `RungSiteRequirement::refusal` seam the `sow`
    // command (`validate_sow`) and the labor arm's placement gate use, so the wire, the rejection and
    // the sim can never disagree about which ground is farmable. Only refusals are stored: a coord
    // absent from the map is ground that takes seed (the `seasonal_weights` convention).
    //
    // Patches only — the client asks "why can't I sow *here*?" of a tile it is looking at, and a
    // patch is on every food-bearing tile there is (see core_sim/CLAUDE.md → the Field).
    let sow_site_refusals: HashMap<UVec2, SiteRefusal> = {
        let field_rung = ladder_config.rung(RungKey::PlantField);
        let grid = config.grid_size;
        let wrap_horizontal = config.map_topology.wrap_horizontal;
        tiles
            .iter()
            .filter(|(_, tile, _)| forage_registry.patch(tile.position).is_some())
            .filter_map(|(_, tile, _)| {
                let fresh_water =
                    tile_is_fresh_watered(tile, grid.x, grid.y, wrap_horizontal, |coord| {
                        tile_tags.get(&coord).copied()
                    });
                rung_site_refusal(field_rung, tile, &labor_config.forage, fresh_water)
                    .map(|refusal| (tile.position, refusal))
            })
            .collect()
    };
    // **What grows on each patch tile** — the named plants its forage capacity decomposes into,
    // resolved through the ONE `forage::tile_flora_composition` seam (the twin of
    // `tile_forage_capacity`), so a navigable hex's *two* capacity terms are both named and the wire
    // cannot disagree with the table. Patch tiles only, mirroring the `sow_site_refusals` sweep above.
    let flora_compositions: HashMap<UVec2, Vec<FloraShareInfo>> = tiles
        .iter()
        .filter(|(_, tile, _)| forage_registry.patch(tile.position).is_some())
        .map(|(_, tile, _)| {
            let shares = tile_flora_composition(&flora_config, &labor_config.forage, tile)
                .iter()
                .map(|share| {
                    let def = &flora_config.species[&share.species];
                    FloraShareInfo {
                        species: share.species.clone(),
                        display_name: def.display_name.clone(),
                        share: share.share,
                        // **Which rungs this plant can EVER climb** (Flora Roster S1) — its own
                        // `cultivation_ceiling`, straight off the roster, so the client's crop
                        // picker can grey out what is impossible without holding a roster of its
                        // own. Species-global: it says nothing about whether this tile is a good
                        // place for it — `share` answers that, and a legal-but-marginal crop is
                        // exactly the loss §4.3 leaves the player free to choose.
                        can_cultivate: def.cultivation_ceiling.allows_cultivate(),
                        can_sow: def.cultivation_ceiling.allows_sow(),
                        // **Is committing THIS tile to THIS plant worth it?** — the one number the
                        // crop-picker decision turns on, per rung. Resolved through
                        // `forage::commit_yield_ratio`, the same concentration seam the sim's own
                        // committed-patch payoff reads, so the quote and the payout cannot disagree
                        // (`docs/plan_flora_roster.md` §4.3). `0` = cannot climb that rung.
                        cultivate_yield_ratio: commit_yield_ratio(
                            &share.species,
                            share.share,
                            &flora_config,
                            &labor_config.forage,
                            RungKey::PlantTended,
                        ),
                        sow_yield_ratio: commit_yield_ratio(
                            &share.species,
                            share.share,
                            &flora_config,
                            &labor_config.forage,
                            RungKey::PlantField,
                        ),
                    }
                })
                .collect();
            (tile.position, shares)
        })
        .collect();
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
            population_state(
                entity,
                cohort,
                allocation,
                expedition,
                home_pos,
                current_pos,
                is_traveling,
                stockpile_radius,
                start_position,
                &faction_inventory,
                &demographics_config,
                &wellbeing_config,
                &supply_membership,
                band_work_range,
                scout_vantage_distance,
                &expedition_levers,
                &settlement_stage_config,
                travel_target,
                hunt_reach,
            )
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
    let fog_raster = fog_raster_from_discoveries(
        &tile_states,
        &population_states,
        &discovery_progress,
        config.grid_size,
        overlays_config.as_ref(),
        start_location.as_ref(),
        fog_reveals.as_ref(),
        tick.0,
    );
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
    let visibility_raster =
        visibility_raster_from_ledger(&visibility_ledger, viewer_faction.0, config.grid_size);

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

    let moisture_overlay_state =
        moisture_overlay_from_resource(moisture.as_ref().map(|res| res.as_ref()), config.grid_size);

    let elevation_overlay_state =
        elevation_overlay_from_field(elevation.as_ref(), config.grid_size);
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
    let herd_states = herd_snapshot_entries(
        &herds,
        &herd_registry,
        &fauna_config,
        &ladder_config,
        &labor_config,
        &expedition_cfg,
        config.grid_size,
        config.map_topology.wrap_horizontal,
    );
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

    let snapshot = WorldSnapshot {
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
        fog_raster: fog_raster.clone(),
        culture_raster: culture_raster.clone(),
        military_raster: military_raster.clone(),
        visibility_raster: visibility_raster.clone(),
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
    }
    .finalize();

    // Turn path: record a fresh ring entry (`update`). Post-command re-capture path
    // (`SnapshotCaptureMode::refresh_in_place`): refresh the latest broadcast + back ring entry in
    // place so a mid-turn command's world mutation reaches the client now, without pushing a ring
    // entry / advancing the turn.
    if capture_mode.refresh_in_place {
        history.refresh_latest(snapshot);
    } else {
        history.update(snapshot);
    }
}

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
/// immediately. The server broadcasts `SnapshotHistory::encoded_snapshot` / `encoded_snapshot_flat`
/// afterward. Kept in this module so `capture_snapshot`'s private `SystemParam` types stay internal.
pub fn recapture_snapshot_in_place(world: &mut World) {
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = true;
    world.run_system_once(capture_snapshot);
    world.resource_mut::<SnapshotCaptureMode>().refresh_in_place = false;
}

pub fn restore_world_from_snapshot(world: &mut World, snapshot: &WorldSnapshot) {
    let knowledge_config = if let Some(handle) = world.get_resource::<KnowledgeLedgerConfigHandle>()
    {
        handle.get()
    } else {
        let parsed = Arc::new(
            KnowledgeLedgerConfig::from_json_str(BUILTIN_KNOWLEDGE_LEDGER_CONFIG)
                .expect("knowledge ledger config should parse"),
        );
        world.insert_resource(KnowledgeLedgerConfigHandle::new(parsed.clone()));
        parsed
    };

    if let Some(mut ledger) = world.get_resource_mut::<KnowledgeLedger>() {
        ledger.apply_config(knowledge_config.clone());
        ledger.sync_from_snapshot(snapshot);
    } else {
        let mut ledger = KnowledgeLedger::with_config(knowledge_config.clone());
        ledger.sync_from_snapshot(snapshot);
        world.insert_resource(ledger);
    }

    let start_marker_position = snapshot
        .start_marker
        .as_ref()
        .map(|marker| UVec2::new(marker.x, marker.y));
    if let Some(mut start_loc) = world.get_resource_mut::<StartLocation>() {
        *start_loc = StartLocation::new(start_marker_position);
    } else {
        world.insert_resource(StartLocation::new(start_marker_position));
    }

    let capability_flags = CapabilityFlags::from_bits_truncate(snapshot.capability_flags);
    if let Some(mut flags) = world.get_resource_mut::<CapabilityFlags>() {
        *flags = capability_flags;
    } else {
        world.insert_resource(capability_flags);
    }

    let moisture_raster = MoistureRaster::from_state(&snapshot.moisture_raster);
    if let Some(mut existing) = world.get_resource_mut::<MoistureRaster>() {
        *existing = moisture_raster;
    } else {
        world.insert_resource(moisture_raster);
    }

    // Reset per-faction Fog of War. WorldSnapshot doesn't carry the visibility
    // ledger, and with permanent-memory FoW (decay disabled by default) a rollback
    // would otherwise retain tiles discovered on ticks *after* the restore point,
    // leaking future knowledge. Clearing both resources rebuilds visibility from the
    // restored unit positions on the next turn (see calculate_visibility). The sweep
    // tracker is cleared too so corridor sweeps don't reference pre-rollback tiles.
    world.insert_resource(crate::visibility::VisibilityLedger::default());
    world.insert_resource(crate::visibility::VisibilitySweepTracker::default());

    // Rebuild the per-faction discovered-sites registry from the snapshot so a rollback neither
    // un-discovers a site nor retains discoveries made after the restore point (the same
    // future-knowledge concern as the fog reset above). The `SiteTag`s themselves are worldgen
    // tile tags (like `FoodModuleTag`) and are not rebuilt here; the registry is the durable
    // record of what has been found.
    let restored_sites = snapshot.discovered_sites.iter().flat_map(|state| {
        let faction = FactionId(state.faction);
        state
            .sites
            .iter()
            .map(move |site| (faction, UVec2::new(site.x, site.y), site.site_id.clone()))
    });
    if let Some(mut discovered) = world.get_resource_mut::<DiscoveredSites>() {
        discovered.rebuild_from(restored_sites);
    } else {
        let mut discovered = DiscoveredSites::default();
        discovered.rebuild_from(restored_sites);
        world.insert_resource(discovered);
    }

    // Despawn existing entities.
    let existing_tiles: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<Tile>>();
        query.iter(world).collect()
    };
    for entity in existing_tiles {
        let _ = world.despawn(entity);
    }

    let existing_logistics: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<LogisticsLink>>();
        query.iter(world).collect()
    };
    for entity in existing_logistics {
        let _ = world.despawn(entity);
    }

    let existing_populations: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<PopulationCohort>>();
        query.iter(world).collect()
    };
    for entity in existing_populations {
        let _ = world.despawn(entity);
    }

    // Rebuild tiles (and attached power nodes).
    let power_lookup: HashMap<u64, &PowerNodeState> = snapshot
        .power
        .iter()
        .map(|state| (state.entity, state))
        .collect();

    let mut tile_entity_lookup: HashMap<u64, Entity> = HashMap::with_capacity(snapshot.tiles.len());
    let mut tile_position_lookup: HashMap<(u32, u32), Entity> =
        HashMap::with_capacity(snapshot.tiles.len());
    let grid_size = world
        .get_resource::<SimulationConfig>()
        .map(|config| config.grid_size)
        .unwrap_or(UVec2::new(0, 0));

    for tile_state in &snapshot.tiles {
        let element = ElementKind::from_u8(tile_state.element).unwrap_or(ElementKind::Ferrite);
        let mut entity_mut = world.spawn_empty();
        let entity = entity_mut.id();
        entity_mut.insert(Tile {
            position: UVec2::new(tile_state.x, tile_state.y),
            element,
            mass: Scalar::from_raw(tile_state.mass),
            temperature: Scalar::from_raw(tile_state.temperature),
            terrain: tile_state.terrain,
            terrain_tags: tile_state.terrain_tags,
            // The wire carries `resource_terrain()` (the real ground). It only *means* a preserved
            // underlying biome on a navigable hex; elsewhere it equals `terrain`, so reconstruct the
            // `Option` from the navigability signal to keep "Some only on NavigableRiver" true.
            underlying_terrain: (tile_state.terrain == sim_runtime::TerrainType::NavigableRiver)
                .then_some(tile_state.underlying_terrain),
            mountain: mountain_metadata_from_state(
                tile_state.mountain_kind,
                tile_state.mountain_relief,
            ),
            river_edges: tile_state.river_edges,
            river_inflow: tile_state.river_inflow,
            river_channel: tile_state.river_channel,
        });

        if let Some(power_state) = power_lookup.get(&tile_state.entity) {
            let generation = Scalar::from_raw(power_state.generation);
            let demand = Scalar::from_raw(power_state.demand);
            entity_mut.insert(PowerNode {
                id: PowerNodeId(power_state.node_id),
                base_generation: generation,
                base_demand: demand,
                generation,
                demand,
                efficiency: Scalar::from_raw(power_state.efficiency),
                storage_capacity: Scalar::from_raw(power_state.storage_capacity),
                storage_level: Scalar::from_raw(power_state.storage_level),
                stability: Scalar::from_raw(power_state.stability),
                surplus: Scalar::from_raw(power_state.surplus),
                deficit: Scalar::from_raw(power_state.deficit),
                incident_count: power_state.incident_count,
            });
        }

        tile_entity_lookup.insert(tile_state.entity, entity);
        tile_position_lookup.insert((tile_state.x, tile_state.y), entity);
    }

    // Rebuild logistics links.
    let trade_lookup: HashMap<u64, &TradeLinkState> = snapshot
        .trade_links
        .iter()
        .map(|state| (state.entity, state))
        .collect();

    for link_state in &snapshot.logistics {
        let Some(&from_entity) = tile_entity_lookup.get(&link_state.from) else {
            warn!(
                "Skipping logistics link {} due to missing from entity {}",
                link_state.entity, link_state.from
            );
            continue;
        };
        let Some(&to_entity) = tile_entity_lookup.get(&link_state.to) else {
            warn!(
                "Skipping logistics link {} due to missing to entity {}",
                link_state.entity, link_state.to
            );
            continue;
        };

        let mut entity_mut = world.spawn_empty();
        entity_mut.insert(LogisticsLink {
            from: from_entity,
            to: to_entity,
            capacity: Scalar::from_raw(link_state.capacity),
            flow: Scalar::from_raw(link_state.flow),
        });
        if let Some(trade_state) = trade_lookup.get(&link_state.entity) {
            entity_mut.insert(trade_link_from_state(trade_state));
        } else {
            entity_mut.insert(TradeLink::default());
        }
    }

    // Rebuild population cohorts. Track old→new cohort entity mapping so an expedition's
    // `home_band` (stored as the OLD band's entity bits) can be resolved to the freshly-spawned
    // band in a second pass (the home band may spawn after the expedition in the list).
    let mut cohort_entity_lookup: HashMap<u64, Entity> =
        HashMap::with_capacity(snapshot.populations.len());
    let mut deferred_expeditions: Vec<(Entity, &PopulationCohortState)> = Vec::new();
    for cohort_state in &snapshot.populations {
        let Some(&home_entity) = tile_entity_lookup.get(&cohort_state.home) else {
            warn!(
                "Skipping population cohort {} due to missing home entity {}",
                cohort_state.entity, cohort_state.home
            );
            continue;
        };
        let migration = cohort_state
            .migration
            .as_ref()
            .map(pending_migration_from_state);
        // Look up current_tile from saved position, falling back to home if not found
        let current_tile = tile_position_lookup
            .get(&(cohort_state.current_x, cohort_state.current_y))
            .copied()
            .unwrap_or(home_entity);
        let mut stores = LocalStore::new();
        for entry in &cohort_state.stores {
            stores.set(&entry.item, Scalar::from_raw(entry.quantity));
        }
        let mut spawned = world.spawn(PopulationCohort {
            home: home_entity,
            current_tile,
            size: cohort_state.size,
            children: Scalar::from_raw(cohort_state.children),
            working: Scalar::from_raw(cohort_state.working),
            elders: Scalar::from_raw(cohort_state.elders),
            stores,
            morale: Scalar::from_raw(cohort_state.morale),
            // Derived per-turn (not snapshot-persisted); recomputed on the next `simulate_population`.
            last_food_consumption: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: MoraleContributions::default(),
            discontent_fraction: scalar_zero(),
            // Grievance is a multi-turn accumulator, so it IS persisted (like `age_turns`) — a
            // rollback must not silently wipe brewing unrest.
            grievance: Scalar::from_raw(cohort_state.grievance),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: cohort_state.age_turns,
            generation: cohort_state.generation,
            faction: FactionId(cohort_state.faction),
            knowledge: fragments_from_contract(&cohort_state.knowledge_fragments),
            migration,
        });
        // Restore the labor allocation (rollback → exact per-source staffing). Every band carries
        // one; an empty vector rehydrates to a fully-idle band.
        spawned.insert(labor_allocation_from_state(&cohort_state.labor_assignments));
        let new_entity = spawned.id();
        cohort_entity_lookup.insert(cohort_state.entity, new_entity);
        if cohort_state.is_expedition {
            // Expedition markers are re-attached in the second pass (home band must exist first).
            deferred_expeditions.push((new_entity, cohort_state));
        } else {
            // Positive `ResidentBand` marker — a real band participating in the population/settlement
            // arc (demographics/migration/sedentarization/supply). Restored so the `With<ResidentBand>`
            // systems keep running after a rollback.
            spawned.insert(ResidentBand);
        }
    }

    // Second pass: re-attach `Expedition` to rolled-back in-flight parties, resolving `home_band`
    // from the OLD band's entity bits via the mapping above. A missing home band is logged and
    // skipped (the party rehydrates as a bare cohort) rather than panicking.
    for (entity, cohort_state) in deferred_expeditions {
        let Some(&home_band) = cohort_entity_lookup.get(&cohort_state.home_band_entity) else {
            warn!(
                "Skipping expedition {} re-attach: home band {} not found in snapshot",
                cohort_state.entity, cohort_state.home_band_entity
            );
            continue;
        };
        let pending_reveal = cohort_state
            .pending_reveal_x
            .iter()
            .zip(cohort_state.pending_reveal_y.iter())
            .map(|(&x, &y)| UVec2::new(x, y))
            .collect();
        world.entity_mut(entity).insert(Expedition {
            home_band,
            mission: ExpeditionMission::from_wire(
                &cohort_state.expedition_mission,
                &cohort_state.expedition_target_herd,
                &cohort_state.expedition_hunt_policy,
            ),
            phase: ExpeditionPhase::from_wire(&cohort_state.expedition_phase),
            announced: cohort_state.expedition_announced,
            pending_reveal,
        });
    }

    // Update tile registry.
    let mut sorted_tiles: Vec<&TileState> = snapshot.tiles.iter().collect();
    sorted_tiles.sort_by_key(|state| {
        let y = state.y as u64;
        let x = state.x as u64;
        (y << 32) | x
    });
    let registry_tiles: Vec<Entity> = sorted_tiles
        .into_iter()
        .filter_map(|state| tile_entity_lookup.get(&state.entity).copied())
        .collect();

    if let Some(mut registry) = world.get_resource_mut::<TileRegistry>() {
        registry.width = grid_size.x;
        registry.height = grid_size.y;
        registry.tiles = registry_tiles;
    } else {
        world.insert_resource(TileRegistry {
            tiles: registry_tiles,
            width: grid_size.x,
            height: grid_size.y,
        });
    }

    if let Some(mut generation_registry) = world.get_resource_mut::<GenerationRegistry>() {
        generation_registry.update_from_states(&snapshot.generations);
    } else {
        world.insert_resource(GenerationRegistry::from_states(&snapshot.generations));
    }

    // Rebuild the authoritative herd registry from the snapshot so a rollback rewinds herd biomass /
    // position / movement / ecology — not just the lossy display telemetry. Mirrors the
    // generation-registry round-trip above.
    if let Some(mut herd_registry) = world.get_resource_mut::<HerdRegistry>() {
        herd_registry.update_from_states(&snapshot.herd_registry);
    } else {
        world.insert_resource(HerdRegistry::from_states(&snapshot.herd_registry));
    }
    // Rebuild the derived herd structures the same way `advance_herds` does post-loop, so the
    // density map and display telemetry aren't stale for a turn after the restore.
    {
        let herd_registry_clone = world.resource::<HerdRegistry>().clone();
        if let Some(mut density) = world.get_resource_mut::<HerdDensityMap>() {
            density.rebuild(grid_size, &herd_registry_clone);
        }
        if let Some(mut telemetry) = world.get_resource_mut::<HerdTelemetry>() {
            telemetry.entries = herd_registry_clone.snapshot_entries();
        }
    }

    // Rebuild the authoritative depletable-forage registry so a rollback rewinds per-patch biomass /
    // ecology phase — the forage counterpart of the herd round-trip above.
    if let Some(mut forage_registry) = world.get_resource_mut::<ForageRegistry>() {
        forage_registry.update_from_states(&snapshot.forage_registry);
    } else {
        world.insert_resource(ForageRegistry::from_states(&snapshot.forage_registry));
    }

    // Rebuild the authoritative graze/pasture registry, same round-trip. Note this also means
    // `spawn_initial_graze`'s "already populated → skip" guard sees a restored world as seeded.
    if let Some(mut graze_registry) = world.get_resource_mut::<GrazeRegistry>() {
        graze_registry.update_from_states(&snapshot.graze_registry);
    } else {
        world.insert_resource(GrazeRegistry::from_states(&snapshot.graze_registry));
    }

    // Rebuild The Telling's narrative memory. **Restore, not just capture**: without this a
    // rollback past a beat leaves it marked fired and it could never fire again.
    world.insert_resource(BeatLedger::from_state(&snapshot.beat_ledger));

    let influencer_config = if let Some(handle) = world.get_resource::<InfluencerConfigHandle>() {
        handle.get()
    } else {
        let parsed = Arc::new(
            InfluencerBalanceConfig::from_json_str(BUILTIN_INFLUENCER_CONFIG)
                .expect("influencer config should parse"),
        );
        world.insert_resource(InfluencerConfigHandle::new(parsed.clone()));
        parsed
    };

    let roster_sentiment;
    let roster_logistics;
    let roster_morale;
    let roster_power;
    {
        let generation_registry_clone = world.resource::<GenerationRegistry>().clone();
        if let Some(mut roster) = world.get_resource_mut::<InfluentialRoster>() {
            roster.apply_config(influencer_config.clone());
            roster.update_from_states(&snapshot.influencers);
        } else {
            let mut roster = InfluentialRoster::with_seed(
                0xA51C_E55E,
                &generation_registry_clone,
                influencer_config.clone(),
            );
            roster.update_from_states(&snapshot.influencers);
            world.insert_resource(roster);
        }
    }
    {
        let roster = world.resource::<InfluentialRoster>();
        roster_sentiment = roster.sentiment_totals();
        roster_logistics = roster.logistics_total();
        roster_morale = roster.morale_total();
        roster_power = roster.power_total();
    }

    if let Some(mut impacts) = world.get_resource_mut::<InfluencerImpacts>() {
        impacts.set_from_totals(roster_logistics, roster_morale, roster_power);
    } else {
        let mut impacts = InfluencerImpacts::default();
        impacts.set_from_totals(roster_logistics, roster_morale, roster_power);
        world.insert_resource(impacts);
    }

    if let Some(mut ledgers) = world.get_resource_mut::<CorruptionLedgers>() {
        *ledgers.ledger_mut() = snapshot.corruption.clone();
    } else {
        let mut ledgers = CorruptionLedgers::default();
        *ledgers.ledger_mut() = snapshot.corruption.clone();
        world.insert_resource(ledgers);
    }

    if let Some(new_effects) =
        world
            .get_resource_mut::<CultureManager>()
            .map(|mut culture_manager| {
                culture_manager
                    .restore_from_snapshot(&snapshot.culture_layers, &snapshot.culture_tensions);
                culture_manager.compute_effects()
            })
    {
        if let Some(mut effects_res) = world.get_resource_mut::<CultureEffectsCache>() {
            *effects_res = new_effects;
        } else {
            world.insert_resource(new_effects);
        }
    }

    let policy_bias = [
        Scalar::from_raw(snapshot.sentiment.knowledge.policy),
        Scalar::from_raw(snapshot.sentiment.trust.policy),
        Scalar::from_raw(snapshot.sentiment.equity.policy),
        Scalar::from_raw(snapshot.sentiment.agency.policy),
    ];
    let incident_bias = [
        Scalar::from_raw(snapshot.sentiment.knowledge.incidents),
        Scalar::from_raw(snapshot.sentiment.trust.incidents),
        Scalar::from_raw(snapshot.sentiment.equity.incidents),
        Scalar::from_raw(snapshot.sentiment.agency.incidents),
    ];

    if let Some(mut bias_res) = world.get_resource_mut::<SentimentAxisBias>() {
        bias_res.reset_to_state(policy_bias, incident_bias);
        bias_res.set_influencer(roster_sentiment);
    } else {
        let mut bias_res = SentimentAxisBias::default();
        bias_res.reset_to_state(policy_bias, incident_bias);
        bias_res.set_influencer(roster_sentiment);
        world.insert_resource(bias_res);
    }

    let mut discovery_ledger_res = DiscoveryProgressLedger::default();
    for entry in &snapshot.discovery_progress {
        let faction = FactionId(entry.faction);
        let progress = Scalar::from_raw(entry.progress);
        discovery_ledger_res.add_progress(faction, entry.discovery, progress);
    }
    world.insert_resource(discovery_ledger_res);

    if !snapshot.great_discovery_definitions.is_empty() {
        if let Some(mut registry) = world.get_resource_mut::<GreatDiscoveryRegistry>() {
            registry.restore_from_states(&snapshot.great_discovery_definitions);
        } else {
            let mut registry = GreatDiscoveryRegistry::default();
            registry.restore_from_states(&snapshot.great_discovery_definitions);
            world.insert_resource(registry);
        }
    } else if world.get_resource::<GreatDiscoveryRegistry>().is_none() {
        world.insert_resource(GreatDiscoveryRegistry::default());
    }

    let registry_clone = world
        .get_resource::<GreatDiscoveryRegistry>()
        .cloned()
        .unwrap_or_default();

    if let Some(mut ledger) = world.get_resource_mut::<GreatDiscoveryLedger>() {
        ledger.replace_with_states(&snapshot.great_discoveries);
    } else {
        let mut ledger = GreatDiscoveryLedger::default();
        ledger.replace_with_states(&snapshot.great_discoveries);
        world.insert_resource(ledger);
    }

    if let Some(mut readiness) = world.get_resource_mut::<GreatDiscoveryReadiness>() {
        readiness.rebuild_from_states(&registry_clone, &snapshot.great_discovery_progress);
        for state in &snapshot.great_discoveries {
            readiness.mark_resolved(FactionId(state.faction), GreatDiscoveryId(state.id));
        }
    } else {
        let mut readiness = GreatDiscoveryReadiness::default();
        readiness.rebuild_from_states(&registry_clone, &snapshot.great_discovery_progress);
        for state in &snapshot.great_discoveries {
            readiness.mark_resolved(FactionId(state.faction), GreatDiscoveryId(state.id));
        }
        world.insert_resource(readiness);
    }

    if let Some(mut telemetry) = world.get_resource_mut::<GreatDiscoveryTelemetry>() {
        telemetry.set_from_state(&snapshot.great_discovery_telemetry);
    } else {
        let mut telemetry = GreatDiscoveryTelemetry::default();
        telemetry.set_from_state(&snapshot.great_discovery_telemetry);
        world.insert_resource(telemetry);
    }
}
