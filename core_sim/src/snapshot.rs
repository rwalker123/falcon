use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use bevy::prelude::*;
use log::warn;
use sim_runtime::{
    encode_delta, encode_delta_flatbuffer, encode_snapshot, encode_snapshot_flatbuffer,
    AxisBiasState, CorruptionLedger, CorruptionSubsystem, CultureLayerState, CultureTensionState,
    CultureTraitEntry, DiscoveryProgressEntry, GenerationState, InfluentialIndividualState,
    LogisticsLinkState, PendingMigrationState, PopulationCohortState, PowerNodeState,
    ScalarRasterState, SentimentAxisTelemetry, SentimentDriverCategory, SentimentDriverState,
    SentimentTelemetryState, SnapshotHeader, TerrainOverlayState, TerrainSample, TileState,
    TradeLinkKnowledge, TradeLinkState, WorldDelta, WorldSnapshot,
};

use crate::{
    components::{
        fragments_from_contract, fragments_to_contract, ElementKind, LogisticsLink,
        PendingMigration, PopulationCohort, PowerNode, Tile, TradeLink,
    },
    culture::{
        CultureEffectsCache, CultureLayer, CultureLayerScope as SimCultureLayerScope,
        CultureManager, CultureTensionKind as SimCultureTensionKind, CultureTensionRecord,
        CultureTraitAxis as SimCultureTraitAxis,
    },
    generations::{GenerationProfile, GenerationRegistry},
    influencers::{InfluencerImpacts, InfluentialRoster},
    orders::FactionId,
    resources::{
        CorruptionLedgers, CorruptionTelemetry, DiscoveryProgressLedger, SentimentAxisBias,
        SimulationConfig, SimulationTick, TileRegistry,
    },
    scalar::Scalar,
};

type EncodedBuffers = (Arc<Vec<u8>>, Arc<Vec<u8>>);

const AXIS_NAMES: [&str; 4] = ["Knowledge", "Trust", "Equity", "Agency"];
const CHANNEL_LABELS: [&str; 4] = ["Popular", "Peer", "Institutional", "Humanitarian"];

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
    generations: HashMap<u16, GenerationState>,
    influencers: HashMap<u32, InfluentialIndividualState>,
    culture_layers: HashMap<u32, CultureLayerState>,
    culture_tensions: Vec<CultureTensionState>,
    discovery_progress: HashMap<(u32, u32), DiscoveryProgressEntry>,
    axis_bias: AxisBiasState,
    sentiment: SentimentTelemetryState,
    terrain_overlay: TerrainOverlayState,
    logistics_raster: ScalarRasterState,
    sentiment_raster: ScalarRasterState,
    corruption_raster: ScalarRasterState,
    fog_raster: ScalarRasterState,
    corruption: CorruptionLedger,
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
            generations: HashMap::new(),
            influencers: HashMap::new(),
            culture_layers: HashMap::new(),
            culture_tensions: Vec::new(),
            discovery_progress: HashMap::new(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            terrain_overlay: TerrainOverlayState::default(),
            logistics_raster: ScalarRasterState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            fog_raster: ScalarRasterState::default(),
            corruption: CorruptionLedger::default(),
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

        let corruption_state = snapshot.corruption.clone();
        let corruption_delta = if self.corruption == corruption_state {
            None
        } else {
            Some(corruption_state.clone())
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
            culture_layers: diff_new(&self.culture_layers, &culture_layers_index),
            removed_culture_layers: diff_removed(&self.culture_layers, &culture_layers_index),
            culture_tensions: delta_culture_tensions.clone(),
            discovery_progress: diff_new(&self.discovery_progress, &discovery_index),
        };

        let snapshot_arc = Arc::new(snapshot);
        let delta_arc = Arc::new(delta);
        let stored = StoredSnapshot::new(snapshot_arc.clone(), delta_arc.clone());

        self.tiles = tiles_index;
        self.logistics = logistics_index;
        self.trade_links = trade_index;
        self.populations = populations_index;
        self.power = power_index;
        self.generations = generations_index;
        self.influencers = influencers_index;
        self.culture_layers = culture_layers_index;
        self.axis_bias = axis_bias_state;
        self.sentiment = sentiment_state;
        self.terrain_overlay = terrain_state;
        self.logistics_raster = logistics_raster_state;
        self.sentiment_raster = sentiment_raster_state;
        self.corruption_raster = corruption_raster_state;
        self.fog_raster = fog_raster_state;
        self.corruption = corruption_state;
        self.culture_tensions = culture_tensions_state;
        self.discovery_progress = discovery_index;
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
        self.culture_tensions = entry.snapshot.culture_tensions.clone();

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
            axis_bias: Some(bias.clone()),
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            fog_raster: None,
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
            axis_bias: None,
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            fog_raster: None,
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
            axis_bias: None,
            sentiment: None,
            logistics_raster: None,
            sentiment_raster: None,
            corruption_raster: None,
            fog_raster: None,
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

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn capture_snapshot(
    config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    tiles: Query<(Entity, &Tile)>,
    logistics_links: Query<(Entity, &LogisticsLink, &TradeLink)>,
    populations: Query<(Entity, &PopulationCohort)>,
    power_nodes: Query<(Entity, &PowerNode)>,
    registry: Res<GenerationRegistry>,
    roster: Res<InfluentialRoster>,
    axis_bias: Res<SentimentAxisBias>,
    corruption_ledgers: Res<CorruptionLedgers>,
    corruption_telemetry: Res<CorruptionTelemetry>,
    discovery_progress: Res<DiscoveryProgressLedger>,
    culture: Res<CultureManager>,
    mut history: ResMut<SnapshotHistory>,
) {
    history.set_capacity(config.snapshot_history_limit.max(1));

    let mut tile_states: Vec<TileState> = tiles
        .iter()
        .map(|(entity, tile)| tile_state(entity, tile))
        .collect();
    tile_states.sort_unstable_by_key(|state| state.entity);

    let mut logistics_states: Vec<LogisticsLinkState> = Vec::new();
    let mut trade_states: Vec<TradeLinkState> = Vec::new();
    for (entity, link, trade) in logistics_links.iter() {
        logistics_states.push(logistics_state(entity, link));
        trade_states.push(trade_link_state(entity, link, trade));
    }
    logistics_states.sort_unstable_by_key(|state| state.entity);
    trade_states.sort_unstable_by_key(|state| state.entity);

    let mut population_states: Vec<PopulationCohortState> = populations
        .iter()
        .map(|(entity, cohort)| population_state(entity, cohort))
        .collect();
    population_states.sort_unstable_by_key(|state| state.entity);

    let mut power_states: Vec<PowerNodeState> = power_nodes
        .iter()
        .map(|(entity, node)| power_state(entity, node))
        .collect();
    power_states.sort_unstable_by_key(|state| state.entity);

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

    let terrain_overlay = terrain_overlay_from_tiles(&tile_states, config.grid_size);
    let logistics_raster =
        logistics_raster_from_links(&tile_states, &logistics_states, config.grid_size);
    let sentiment_raster =
        sentiment_raster_from_populations(&tile_states, &population_states, config.grid_size);
    let corruption_raster = corruption_raster_from_simulation(
        &tile_states,
        &trade_states,
        &population_states,
        &power_states,
        &logistics_raster,
        CorruptionSignals {
            ledger: corruption_ledgers.ledger(),
            telemetry: &corruption_telemetry,
        },
        config.grid_size,
    );
    let fog_raster = fog_raster_from_discoveries(
        &tile_states,
        &population_states,
        &discovery_progress,
        config.grid_size,
    );

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

    let header = SnapshotHeader::new(
        tick.0,
        tile_states.len(),
        logistics_states.len(),
        trade_states.len(),
        population_states.len(),
        power_states.len(),
        influencer_states.len(),
    );

    let snapshot = WorldSnapshot {
        header,
        tiles: tile_states,
        logistics: logistics_states,
        trade_links: trade_states,
        populations: population_states,
        power: power_states,
        terrain: terrain_overlay.clone(),
        logistics_raster: logistics_raster.clone(),
        sentiment_raster: sentiment_raster.clone(),
        corruption_raster: corruption_raster.clone(),
        fog_raster: fog_raster.clone(),
        axis_bias: axis_bias_state,
        sentiment: sentiment_state,
        generations: generation_states,
        corruption: corruption_ledgers.ledger().clone(),
        influencers: influencer_states,
        culture_layers: culture_layer_states,
        culture_tensions: culture_tension_states,
        discovery_progress: discovery_states,
    }
    .finalize();

    history.update(snapshot);
}

pub fn restore_world_from_snapshot(world: &mut World, snapshot: &WorldSnapshot) {
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
        });

        if let Some(power_state) = power_lookup.get(&tile_state.entity) {
            entity_mut.insert(PowerNode {
                generation: Scalar::from_raw(power_state.generation),
                demand: Scalar::from_raw(power_state.demand),
                efficiency: Scalar::from_raw(power_state.efficiency),
            });
        }

        tile_entity_lookup.insert(tile_state.entity, entity);
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

    // Rebuild population cohorts.
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
        world.spawn(PopulationCohort {
            home: home_entity,
            size: cohort_state.size,
            morale: Scalar::from_raw(cohort_state.morale),
            generation: cohort_state.generation,
            faction: FactionId(cohort_state.faction),
            knowledge: fragments_from_contract(&cohort_state.knowledge_fragments),
            migration,
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

    let roster_sentiment;
    let roster_logistics;
    let roster_morale;
    let roster_power;
    {
        let generation_registry_clone = world.resource::<GenerationRegistry>().clone();
        if let Some(mut roster) = world.get_resource_mut::<InfluentialRoster>() {
            roster.update_from_states(&snapshot.influencers);
        } else {
            let mut roster = InfluentialRoster::with_seed(0xA51C_E55E, &generation_registry_clone);
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
}

fn axis_bias_state_from_resource(bias: &SentimentAxisBias) -> AxisBiasState {
    let raw = bias.as_raw();
    AxisBiasState {
        knowledge: raw[0],
        trust: raw[1],
        equity: raw[2],
        agency: raw[3],
    }
}

fn influencer_label(state: &InfluentialIndividualState) -> String {
    if let Some(channel) = dominant_channel_label(state) {
        format!("Influencer {} ({})", state.name, channel)
    } else {
        format!("Influencer {}", state.name)
    }
}

fn dominant_channel_label(state: &InfluentialIndividualState) -> Option<&'static str> {
    let weights = [
        Scalar::from_raw(state.weight_popular),
        Scalar::from_raw(state.weight_peer),
        Scalar::from_raw(state.weight_institutional),
        Scalar::from_raw(state.weight_humanitarian),
    ];
    let supports = [
        Scalar::from_raw(state.support_popular),
        Scalar::from_raw(state.support_peer),
        Scalar::from_raw(state.support_institutional),
        Scalar::from_raw(state.support_humanitarian),
    ];
    let mut best_score = Scalar::zero();
    let mut best_idx: Option<usize> = None;
    for idx in 0..CHANNEL_LABELS.len() {
        let score = weights[idx] * supports[idx];
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| CHANNEL_LABELS[idx])
}

fn influencer_driver_weight(state: &InfluentialIndividualState) -> i64 {
    let weights = [
        Scalar::from_raw(state.weight_popular),
        Scalar::from_raw(state.weight_peer),
        Scalar::from_raw(state.weight_institutional),
        Scalar::from_raw(state.weight_humanitarian),
    ];
    let supports = [
        Scalar::from_raw(state.support_popular),
        Scalar::from_raw(state.support_peer),
        Scalar::from_raw(state.support_institutional),
        Scalar::from_raw(state.support_humanitarian),
    ];
    let mut best_score = Scalar::zero();
    for idx in 0..CHANNEL_LABELS.len() {
        let score = weights[idx] * supports[idx];
        if score > best_score {
            best_score = score;
        }
    }
    let clamped = if best_score <= Scalar::zero() {
        Scalar::one()
    } else {
        best_score.clamp(Scalar::from_f32(0.05), Scalar::one())
    };
    clamped.raw()
}

fn diff_new<K, T>(previous: &HashMap<K, T>, current: &HashMap<K, T>) -> Vec<T>
where
    K: Eq + Hash,
    T: Clone + PartialEq,
{
    current
        .iter()
        .filter_map(|(id, state)| match previous.get(id) {
            Some(prev) if prev == state => None,
            _ => Some(state.clone()),
        })
        .collect()
}

fn diff_removed<K, T>(previous: &HashMap<K, T>, current: &HashMap<K, T>) -> Vec<K>
where
    K: Eq + Hash + Copy,
{
    previous
        .keys()
        .filter(|id| !current.contains_key(id))
        .copied()
        .collect()
}

fn culture_layer_state(layer: &CultureLayer) -> CultureLayerState {
    let baseline = layer.traits.baseline();
    let modifier = layer.traits.modifier();
    let values = layer.traits.values();
    let mut traits = Vec::with_capacity(SimCultureTraitAxis::ALL.len());
    for axis in SimCultureTraitAxis::ALL {
        let idx = axis.index();
        traits.push(CultureTraitEntry {
            axis: map_trait_axis(axis),
            baseline: baseline[idx].raw(),
            modifier: modifier[idx].raw(),
            value: values[idx].raw(),
        });
    }
    CultureLayerState {
        id: layer.id,
        owner: layer.owner.0,
        parent: layer.parent.unwrap_or(0),
        scope: map_layer_scope(layer.scope),
        traits,
        divergence: layer.divergence.magnitude.raw(),
        soft_threshold: layer.divergence.soft_threshold.raw(),
        hard_threshold: layer.divergence.hard_threshold.raw(),
        ticks_above_soft: layer.divergence.ticks_above_soft,
        ticks_above_hard: layer.divergence.ticks_above_hard,
        last_updated_tick: layer.last_updated_tick,
    }
}

fn culture_tension_state(record: CultureTensionRecord) -> CultureTensionState {
    CultureTensionState {
        layer_id: record.layer_id,
        scope: map_layer_scope(record.scope),
        owner: record.owner.0,
        severity: record.magnitude.raw(),
        timer: record.timer,
        kind: map_tension_kind(record.kind),
    }
}

fn map_layer_scope(scope: SimCultureLayerScope) -> sim_runtime::CultureLayerScope {
    match scope {
        SimCultureLayerScope::Global => sim_runtime::CultureLayerScope::Global,
        SimCultureLayerScope::Regional => sim_runtime::CultureLayerScope::Regional,
        SimCultureLayerScope::Local => sim_runtime::CultureLayerScope::Local,
    }
}

fn map_trait_axis(axis: SimCultureTraitAxis) -> sim_runtime::CultureTraitAxis {
    match axis {
        SimCultureTraitAxis::PassiveAggressive => sim_runtime::CultureTraitAxis::PassiveAggressive,
        SimCultureTraitAxis::OpenClosed => sim_runtime::CultureTraitAxis::OpenClosed,
        SimCultureTraitAxis::CollectivistIndividualist => {
            sim_runtime::CultureTraitAxis::CollectivistIndividualist
        }
        SimCultureTraitAxis::TraditionalistRevisionist => {
            sim_runtime::CultureTraitAxis::TraditionalistRevisionist
        }
        SimCultureTraitAxis::HierarchicalEgalitarian => {
            sim_runtime::CultureTraitAxis::HierarchicalEgalitarian
        }
        SimCultureTraitAxis::SyncreticPurist => sim_runtime::CultureTraitAxis::SyncreticPurist,
        SimCultureTraitAxis::AsceticIndulgent => sim_runtime::CultureTraitAxis::AsceticIndulgent,
        SimCultureTraitAxis::PragmaticIdealistic => {
            sim_runtime::CultureTraitAxis::PragmaticIdealistic
        }
        SimCultureTraitAxis::RationalistMystical => {
            sim_runtime::CultureTraitAxis::RationalistMystical
        }
        SimCultureTraitAxis::ExpansionistInsular => {
            sim_runtime::CultureTraitAxis::ExpansionistInsular
        }
        SimCultureTraitAxis::AdaptiveStubborn => sim_runtime::CultureTraitAxis::AdaptiveStubborn,
        SimCultureTraitAxis::HonorBoundOpportunistic => {
            sim_runtime::CultureTraitAxis::HonorBoundOpportunistic
        }
        SimCultureTraitAxis::MeritOrientedLineageOriented => {
            sim_runtime::CultureTraitAxis::MeritOrientedLineageOriented
        }
        SimCultureTraitAxis::SecularDevout => sim_runtime::CultureTraitAxis::SecularDevout,
        SimCultureTraitAxis::PluralisticMonocultural => {
            sim_runtime::CultureTraitAxis::PluralisticMonocultural
        }
    }
}

fn map_tension_kind(kind: SimCultureTensionKind) -> sim_runtime::CultureTensionKind {
    match kind {
        SimCultureTensionKind::DriftWarning => sim_runtime::CultureTensionKind::DriftWarning,
        SimCultureTensionKind::AssimilationPush => {
            sim_runtime::CultureTensionKind::AssimilationPush
        }
        SimCultureTensionKind::SchismRisk => sim_runtime::CultureTensionKind::SchismRisk,
    }
}

fn terrain_overlay_from_tiles(tiles: &[TileState], grid_size: UVec2) -> TerrainOverlayState {
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }
    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![TerrainSample::default(); total];
    for tile in tiles {
        if tile.x >= width || tile.y >= height {
            continue;
        }
        let idx = (tile.y as usize) * (width as usize) + tile.x as usize;
        if idx < samples.len() {
            samples[idx] = TerrainSample {
                terrain: tile.terrain,
                tags: tile.terrain_tags,
            };
        }
    }
    TerrainOverlayState {
        width,
        height,
        samples,
    }
}

fn logistics_raster_from_links(
    tiles: &[TileState],
    logistics: &[LogisticsLinkState],
    grid_size: UVec2,
) -> ScalarRasterState {
    let mut tile_positions = HashMap::with_capacity(tiles.len());
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        tile_positions.insert(tile.entity, (tile.x, tile.y));
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![0i64; total];
    let mut counts = vec![0u32; total];

    for link in logistics {
        let flow = Scalar::from_raw(link.flow).abs().raw();
        if flow == 0 {
            continue;
        }
        if let Some(&(x, y)) = tile_positions.get(&link.from) {
            let idx = (y as usize) * (width as usize) + x as usize;
            if idx < samples.len() {
                samples[idx] = samples[idx].saturating_add(flow);
                counts[idx] = counts[idx].saturating_add(1);
            }
        }
        if let Some(&(x, y)) = tile_positions.get(&link.to) {
            let idx = (y as usize) * (width as usize) + x as usize;
            if idx < samples.len() {
                samples[idx] = samples[idx].saturating_add(flow);
                counts[idx] = counts[idx].saturating_add(1);
            }
        }
    }

    for (value, count) in samples.iter_mut().zip(counts.iter()) {
        if *count > 0 {
            let divisor = i64::from(*count);
            *value /= divisor;
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

const CORRUPTION_SUBSYSTEM_COUNT: usize = 4;

struct CorruptionSignals<'a> {
    ledger: &'a CorruptionLedger,
    telemetry: &'a CorruptionTelemetry,
}

fn corruption_raster_from_simulation(
    tiles: &[TileState],
    trade_links: &[TradeLinkState],
    populations: &[PopulationCohortState],
    power_nodes: &[PowerNodeState],
    logistics_raster: &ScalarRasterState,
    corruption_signals: CorruptionSignals<'_>,
    grid_size: UVec2,
) -> ScalarRasterState {
    let CorruptionSignals { ledger, telemetry } = corruption_signals;
    let mut width = logistics_raster.width.max(grid_size.x).max(1);
    let mut height = logistics_raster.height.max(grid_size.y).max(1);

    for tile in tiles {
        width = width.max(tile.x.saturating_add(1));
        height = height.max(tile.y.saturating_add(1));
    }

    let width_usize = width as usize;
    let height_usize = height as usize;
    let total = width_usize.saturating_mul(height_usize).max(1);

    let mut samples = vec![0i64; total];

    let mut tile_indices = HashMap::with_capacity(tiles.len());
    for tile in tiles {
        if tile.x < width && tile.y < height {
            let idx = (tile.y as usize) * width_usize + tile.x as usize;
            tile_indices.insert(tile.entity, idx);
        }
    }

    let mut logistics_weights = vec![0i64; total];
    if logistics_raster.width > 0
        && logistics_raster.height > 0
        && !logistics_raster.samples.is_empty()
    {
        let src_width = logistics_raster.width as usize;
        let src_height = logistics_raster.height as usize;
        let min_height = src_height.min(height_usize);
        let min_width = src_width.min(width_usize);
        for y in 0..min_height {
            let src_row = y * src_width;
            let dst_row = y * width_usize;
            for x in 0..min_width {
                let src_idx = src_row + x;
                let dst_idx = dst_row + x;
                if src_idx < logistics_raster.samples.len() && dst_idx < logistics_weights.len() {
                    logistics_weights[dst_idx] = logistics_raster.samples[src_idx].abs();
                }
            }
        }
    }

    let mut trade_weights = vec![0i64; total];
    for link in trade_links {
        let throughput = link.throughput.abs();
        if throughput <= 0 {
            continue;
        }
        for tile_id in [link.from_tile, link.to_tile] {
            if let Some(&idx) = tile_indices.get(&tile_id) {
                trade_weights[idx] = trade_weights[idx].saturating_add(throughput);
            }
        }
    }

    let mut military_weights = vec![0i64; total];
    for node in power_nodes {
        if let Some(&idx) = tile_indices.get(&node.entity) {
            let generation = node.generation.abs();
            let demand = node.demand.abs();
            let weight = generation.saturating_add(demand);
            if weight > 0 {
                military_weights[idx] = military_weights[idx].saturating_add(weight);
            }
        }
    }

    let mut governance_weights = vec![0i64; total];
    let scale_i128 = i128::from(Scalar::SCALE);
    for cohort in populations {
        if let Some(&idx) = tile_indices.get(&cohort.home) {
            let size = i64::from(cohort.size);
            if size <= 0 {
                continue;
            }
            let morale = Scalar::from_raw(cohort.morale).clamp(Scalar::zero(), Scalar::one());
            let morale_deficit = (Scalar::one() - morale).raw().max(0);
            let mut weighted =
                (i128::from(size) * (scale_i128 + i128::from(morale_deficit))) / scale_i128;
            if weighted > i128::from(i64::MAX) {
                weighted = i128::from(i64::MAX);
            }
            governance_weights[idx] = governance_weights[idx].saturating_add(weighted as i64);
        }
    }

    let mut subsystem_totals = [0i64; CORRUPTION_SUBSYSTEM_COUNT];
    for entry in &ledger.entries {
        let idx = entry.subsystem as usize;
        if idx >= subsystem_totals.len() {
            continue;
        }
        if entry.intensity > 0 {
            subsystem_totals[idx] = subsystem_totals[idx].saturating_add(entry.intensity);
        }
    }

    let mut subsystem_spikes = [0i64; CORRUPTION_SUBSYSTEM_COUNT];
    for record in telemetry.exposures_this_turn.iter() {
        let idx = record.subsystem as usize;
        if idx >= subsystem_spikes.len() {
            continue;
        }
        if record.intensity > 0 {
            subsystem_spikes[idx] = subsystem_spikes[idx].saturating_add(record.intensity);
        }
    }

    let logistics_idx = CorruptionSubsystem::Logistics as usize;
    let trade_idx = CorruptionSubsystem::Trade as usize;
    let military_idx = CorruptionSubsystem::Military as usize;
    let governance_idx = CorruptionSubsystem::Governance as usize;

    let logistic_intensity = subsystem_totals[logistics_idx]
        .saturating_add(subsystem_spikes[logistics_idx].saturating_mul(2));
    distribute_intensity(&mut samples, &logistics_weights, logistic_intensity);

    let trade_intensity =
        subsystem_totals[trade_idx].saturating_add(subsystem_spikes[trade_idx].saturating_mul(2));
    distribute_intensity(&mut samples, &trade_weights, trade_intensity);

    let military_intensity =
        subsystem_totals[military_idx].saturating_add(subsystem_spikes[military_idx]);
    distribute_intensity(&mut samples, &military_weights, military_intensity);

    let governance_intensity =
        subsystem_totals[governance_idx].saturating_add(subsystem_spikes[governance_idx]);
    distribute_intensity(&mut samples, &governance_weights, governance_intensity);

    let logistic_norm = normalize_weights_to_scalar(&logistics_weights);
    let trade_norm = normalize_weights_to_scalar(&trade_weights);
    let military_norm = normalize_weights_to_scalar(&military_weights);
    let governance_norm = normalize_weights_to_scalar(&governance_weights);

    let logistic_weight = Scalar::from_f32(0.35);
    let trade_weight = Scalar::from_f32(0.25);
    let military_weight = Scalar::from_f32(0.2);
    let governance_weight = Scalar::from_f32(0.2);

    for (idx, sample) in samples.iter_mut().enumerate() {
        let mut baseline = Scalar::zero();
        baseline += logistic_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * logistic_weight;
        baseline += trade_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * trade_weight;
        baseline += military_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * military_weight;
        baseline += governance_norm
            .get(idx)
            .copied()
            .unwrap_or_else(Scalar::zero)
            * governance_weight;
        if baseline.raw() != 0 {
            *sample = sample.saturating_add(baseline.raw());
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

fn normalize_weights_to_scalar(weights: &[i64]) -> Vec<Scalar> {
    if weights.is_empty() {
        return Vec::new();
    }
    let max_weight = weights.iter().copied().max().unwrap_or(0);
    if max_weight <= 0 {
        return vec![Scalar::zero(); weights.len()];
    }
    let max_value = i128::from(max_weight);
    weights
        .iter()
        .map(|&weight| {
            if weight <= 0 {
                Scalar::zero()
            } else {
                let mut ratio = (i128::from(weight) * i128::from(Scalar::SCALE)) / max_value;
                if ratio > i128::from(Scalar::SCALE) {
                    ratio = i128::from(Scalar::SCALE);
                }
                if ratio < 0 {
                    ratio = 0;
                }
                Scalar::from_raw(ratio as i64)
            }
        })
        .collect()
}

fn distribute_intensity(samples: &mut [i64], weights: &[i64], intensity_raw: i64) {
    if intensity_raw <= 0 || samples.is_empty() || samples.len() != weights.len() {
        return;
    }

    let total_weight: i128 = weights
        .iter()
        .map(|&w| i128::from(if w > 0 { w } else { 0 }))
        .sum();

    if total_weight == 0 {
        let len = samples.len() as i64;
        if len <= 0 {
            return;
        }
        let base_share = intensity_raw / len;
        for sample in samples.iter_mut() {
            *sample = sample.saturating_add(base_share);
        }
        let remainder = intensity_raw - base_share * len;
        if remainder != 0 {
            samples[0] = samples[0].saturating_add(remainder);
        }
        return;
    }

    let intensity = i128::from(intensity_raw);
    let mut allocated = 0i128;

    for (sample, &weight) in samples.iter_mut().zip(weights.iter()) {
        if weight <= 0 {
            continue;
        }
        let share = (intensity * i128::from(weight)) / total_weight;
        if share == 0 {
            continue;
        }
        allocated += share;
        let share_i64 = if share > i128::from(i64::MAX) {
            i64::MAX
        } else if share < i128::from(i64::MIN) {
            i64::MIN
        } else {
            share as i64
        };
        *sample = sample.saturating_add(share_i64);
    }

    let remainder = intensity - allocated;
    if remainder != 0 {
        if let Some((idx, _)) = weights.iter().enumerate().max_by_key(|(_, &w)| w) {
            if let Some(sample) = samples.get_mut(idx) {
                let remainder_i64 = if remainder > i128::from(i64::MAX) {
                    i64::MAX
                } else if remainder < i128::from(i64::MIN) {
                    i64::MIN
                } else {
                    remainder as i64
                };
                *sample = sample.saturating_add(remainder_i64);
            }
        }
    }
}

fn fog_raster_from_discoveries(
    tiles: &[TileState],
    populations: &[PopulationCohortState],
    discovery: &DiscoveryProgressLedger,
    grid_size: UVec2,
) -> ScalarRasterState {
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);

    let mut samples = vec![Scalar::one().raw(); total];
    let mut tile_indices = HashMap::with_capacity(tiles.len());
    for tile in tiles {
        if tile.x < width && tile.y < height {
            let idx = (tile.y as usize) * (width as usize) + tile.x as usize;
            tile_indices.insert(tile.entity, idx);
        }
    }

    let mut tile_faction_sizes: HashMap<u64, HashMap<u32, u64>> = HashMap::new();
    let mut tile_local_weighted: HashMap<u64, (i128, i128)> = HashMap::new();

    for cohort in populations {
        let size = u64::from(cohort.size);
        if size > 0 {
            let faction_map = tile_faction_sizes.entry(cohort.home).or_default();
            *faction_map.entry(cohort.faction).or_insert(0) += size;
        }

        if size == 0 {
            continue;
        }

        let fragments = &cohort.knowledge_fragments;
        let fragment_average_raw = if fragments.is_empty() {
            0i64
        } else {
            let mut total = Scalar::zero();
            for fragment in fragments {
                total += Scalar::from_raw(fragment.progress).clamp(Scalar::zero(), Scalar::one());
            }
            let count = fragments.len() as u32;
            if count == 0 {
                0
            } else {
                (total / Scalar::from_u32(count))
                    .clamp(Scalar::zero(), Scalar::one())
                    .raw()
            }
        };

        let weight = i128::from(size);
        let entry = tile_local_weighted.entry(cohort.home).or_insert((0, 0));
        entry.0 = entry
            .0
            .saturating_add(i128::from(fragment_average_raw) * weight);
        entry.1 = entry.1.saturating_add(weight);
    }

    let mut tile_local_average: HashMap<u64, Scalar> = HashMap::new();
    for (tile_entity, (weighted_sum, total_weight)) in tile_local_weighted {
        if total_weight <= 0 {
            continue;
        }
        let mut average = weighted_sum / total_weight;
        if average < 0 {
            average = 0;
        }
        let scale = i128::from(Scalar::SCALE);
        if average > scale {
            average = scale;
        }
        tile_local_average.insert(tile_entity, Scalar::from_raw(average as i64));
    }

    let mut tile_controllers: HashMap<u64, u32> = HashMap::new();
    for (tile_entity, faction_map) in &tile_faction_sizes {
        let mut best: Option<(u32, u64)> = None;
        for (&faction, &count) in faction_map.iter() {
            best = match best {
                None => Some((faction, count)),
                Some((best_faction, best_count)) => {
                    if count > best_count || (count == best_count && faction < best_faction) {
                        Some((faction, count))
                    } else {
                        Some((best_faction, best_count))
                    }
                }
            };
        }
        if let Some((faction, _)) = best {
            tile_controllers.insert(*tile_entity, faction);
        }
    }

    let blend_half = Scalar::from_f32(0.5);

    for tile in tiles {
        let Some(&idx) = tile_indices.get(&tile.entity) else {
            continue;
        };

        let global_cov = tile_controllers.get(&tile.entity).and_then(|&faction| {
            discovery
                .progress
                .get(&FactionId(faction))
                .and_then(|entries| {
                    if entries.is_empty() {
                        return None;
                    }
                    let mut total = Scalar::zero();
                    let mut count = 0u32;
                    for value in entries.values() {
                        if value.raw() <= 0 {
                            continue;
                        }
                        total += (*value).clamp(Scalar::zero(), Scalar::one());
                        count = count.saturating_add(1);
                    }
                    if count == 0 {
                        None
                    } else {
                        Some((total / Scalar::from_u32(count)).clamp(Scalar::zero(), Scalar::one()))
                    }
                })
        });

        let local_cov = tile_local_average.get(&tile.entity).copied();

        let coverage = match (global_cov, local_cov) {
            (Some(g), Some(l)) => ((g + l) * blend_half).clamp(Scalar::zero(), Scalar::one()),
            (Some(g), None) => g,
            (None, Some(l)) => l,
            (None, None) => Scalar::zero(),
        };

        let fog = (Scalar::one() - coverage).clamp(Scalar::zero(), Scalar::one());
        samples[idx] = fog.raw();
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

fn sentiment_raster_from_populations(
    tiles: &[TileState],
    populations: &[PopulationCohortState],
    grid_size: UVec2,
) -> ScalarRasterState {
    let mut tile_positions = HashMap::with_capacity(tiles.len());
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        tile_positions.insert(tile.entity, (tile.x, tile.y));
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut weighted = vec![0i128; total];
    let mut weights = vec![0i128; total];

    for cohort in populations {
        let Some(&(x, y)) = tile_positions.get(&cohort.home) else {
            continue;
        };
        let idx = (y as usize) * (width as usize) + x as usize;
        if idx >= weighted.len() {
            continue;
        }
        let morale = Scalar::from_raw(cohort.morale);
        let size = i128::from(cohort.size);
        weighted[idx] = weighted[idx].saturating_add(i128::from(morale.raw()) * size);
        weights[idx] = weights[idx].saturating_add(size);
    }

    let mut samples = vec![0i64; total];
    for (idx, sample) in samples.iter_mut().enumerate() {
        let weight = weights[idx];
        if weight > 0 {
            *sample = (weighted[idx] / weight) as i64;
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

fn tile_state(entity: Entity, tile: &Tile) -> TileState {
    TileState {
        entity: entity.to_bits(),
        x: tile.position.x,
        y: tile.position.y,
        element: u8::from(tile.element),
        mass: tile.mass.raw(),
        temperature: tile.temperature.raw(),
        terrain: tile.terrain,
        terrain_tags: tile.terrain_tags,
    }
}

fn logistics_state(entity: Entity, link: &LogisticsLink) -> LogisticsLinkState {
    LogisticsLinkState {
        entity: entity.to_bits(),
        from: link.from.to_bits(),
        to: link.to.to_bits(),
        capacity: link.capacity.raw(),
        flow: link.flow.raw(),
    }
}

fn trade_link_state(entity: Entity, link: &LogisticsLink, trade: &TradeLink) -> TradeLinkState {
    TradeLinkState {
        entity: entity.to_bits(),
        from_faction: trade.from_faction.0,
        to_faction: trade.to_faction.0,
        throughput: trade.throughput.raw(),
        tariff: trade.tariff.raw(),
        knowledge: TradeLinkKnowledge {
            openness: trade.openness.raw(),
            leak_timer: trade.leak_timer,
            last_discovery: trade.last_discovery.unwrap_or_default(),
            decay: trade.decay.raw(),
        },
        from_tile: link.from.to_bits(),
        to_tile: link.to.to_bits(),
        pending_fragments: fragments_to_contract(&trade.pending_fragments),
    }
}

fn trade_link_from_state(state: &TradeLinkState) -> TradeLink {
    TradeLink {
        from_faction: FactionId(state.from_faction),
        to_faction: FactionId(state.to_faction),
        throughput: Scalar::from_raw(state.throughput),
        tariff: Scalar::from_raw(state.tariff),
        openness: Scalar::from_raw(state.knowledge.openness),
        decay: Scalar::from_raw(state.knowledge.decay),
        leak_timer: state.knowledge.leak_timer,
        last_discovery: if state.knowledge.last_discovery == 0 {
            None
        } else {
            Some(state.knowledge.last_discovery)
        },
        pending_fragments: fragments_from_contract(&state.pending_fragments),
    }
}

fn pending_migration_to_state(migration: &PendingMigration) -> PendingMigrationState {
    PendingMigrationState {
        destination: migration.destination.0,
        eta: migration.eta,
        fragments: fragments_to_contract(&migration.fragments),
    }
}

fn pending_migration_from_state(state: &PendingMigrationState) -> PendingMigration {
    PendingMigration {
        destination: FactionId(state.destination),
        eta: state.eta,
        fragments: fragments_from_contract(&state.fragments),
    }
}

fn discovery_progress_entries(ledger: &DiscoveryProgressLedger) -> Vec<DiscoveryProgressEntry> {
    let mut entries: Vec<DiscoveryProgressEntry> = Vec::new();
    for (faction_id, discoveries) in ledger.progress.iter() {
        for (discovery_id, progress) in discoveries.iter() {
            let raw = progress.raw();
            if raw <= 0 {
                continue;
            }
            entries.push(DiscoveryProgressEntry {
                faction: faction_id.0,
                discovery: *discovery_id,
                progress: raw,
            });
        }
    }
    entries.sort_unstable_by(|a, b| (a.faction, a.discovery).cmp(&(b.faction, b.discovery)));
    entries
}

fn population_state(entity: Entity, cohort: &PopulationCohort) -> PopulationCohortState {
    let migration = cohort.migration.as_ref().map(pending_migration_to_state);
    PopulationCohortState {
        entity: entity.to_bits(),
        home: cohort.home.to_bits(),
        size: cohort.size,
        morale: cohort.morale.raw(),
        generation: cohort.generation,
        faction: cohort.faction.0,
        knowledge_fragments: fragments_to_contract(&cohort.knowledge),
        migration,
    }
}

fn power_state(entity: Entity, node: &PowerNode) -> PowerNodeState {
    PowerNodeState {
        entity: entity.to_bits(),
        generation: node.generation.raw(),
        demand: node.demand.raw(),
        efficiency: node.efficiency.raw(),
    }
}

fn generation_state(profile: &GenerationProfile) -> GenerationState {
    let [knowledge, trust, equity, agency] = profile.bias.to_scaled();
    GenerationState {
        id: profile.id,
        name: profile.name.clone(),
        bias_knowledge: knowledge,
        bias_trust: trust,
        bias_equity: equity,
        bias_agency: agency,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        orders::FactionId,
        resources::{CorruptionTelemetry, DiscoveryProgressLedger},
        scalar::Scalar,
    };
    use bevy::math::UVec2;
    use sim_runtime::{
        CorruptionEntry, CorruptionSubsystem, KnownTechFragment, TerrainTags, TerrainType,
        TradeLinkKnowledge,
    };

    fn tile(entity: u64, x: u32, y: u32) -> TileState {
        TileState {
            entity,
            x,
            y,
            element: 0,
            mass: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
        }
    }

    #[test]
    fn corruption_raster_allocates_intensity_and_baseline() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];

        let logistics_raster = ScalarRasterState {
            width: 2,
            height: 1,
            samples: vec![Scalar::from_f32(1.2).raw(), Scalar::from_f32(0.2).raw()],
        };

        let trade_links = vec![TradeLinkState {
            entity: 10,
            from_faction: 0,
            to_faction: 1,
            throughput: Scalar::from_f32(0.6).raw(),
            tariff: 0,
            knowledge: TradeLinkKnowledge::default(),
            from_tile: 2,
            to_tile: 2,
            pending_fragments: Vec::new(),
        }];

        let populations = vec![
            PopulationCohortState {
                entity: 100,
                home: 1,
                size: 120,
                morale: Scalar::from_f32(0.3).raw(),
                generation: 0,
                faction: 0,
                knowledge_fragments: Vec::new(),
                migration: None,
            },
            PopulationCohortState {
                entity: 101,
                home: 2,
                size: 80,
                morale: Scalar::from_f32(0.8).raw(),
                generation: 0,
                faction: 1,
                knowledge_fragments: Vec::new(),
                migration: None,
            },
        ];

        let power_nodes = vec![
            PowerNodeState {
                entity: 1,
                generation: Scalar::from_f32(0.9).raw(),
                demand: Scalar::from_f32(0.4).raw(),
                efficiency: Scalar::one().raw(),
            },
            PowerNodeState {
                entity: 2,
                generation: Scalar::from_f32(0.4).raw(),
                demand: Scalar::from_f32(0.2).raw(),
                efficiency: Scalar::one().raw(),
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

        let raster = corruption_raster_from_simulation(
            &tiles,
            &trade_links,
            &populations,
            &power_nodes,
            &logistics_raster,
            CorruptionSignals {
                ledger: &ledger,
                telemetry: &telemetry,
            },
            UVec2::new(2, 1),
        );

        assert_eq!(raster.width, 2);
        assert_eq!(raster.height, 1);
        assert_eq!(raster.samples.len(), 2);
        assert!(raster.samples[0] > 0);
        assert!(raster.samples[1] > 0);
        assert!(raster.samples[0] > raster.samples[1]);
    }

    #[test]
    fn fog_raster_reflects_discovery_progress() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];

        let populations = vec![
            PopulationCohortState {
                entity: 200,
                home: 1,
                size: 150,
                morale: Scalar::from_f32(0.5).raw(),
                generation: 0,
                faction: 0,
                knowledge_fragments: vec![KnownTechFragment {
                    discovery_id: 1,
                    progress: Scalar::from_f32(0.6).raw(),
                    fidelity: Scalar::one().raw(),
                }],
                migration: None,
            },
            PopulationCohortState {
                entity: 201,
                home: 2,
                size: 60,
                morale: Scalar::from_f32(0.7).raw(),
                generation: 0,
                faction: 1,
                knowledge_fragments: Vec::new(),
                migration: None,
            },
        ];

        let mut discovery = DiscoveryProgressLedger::default();
        discovery.add_progress(FactionId(0), 1, Scalar::from_f32(0.8));
        discovery.add_progress(FactionId(0), 2, Scalar::from_f32(0.4));

        let fog = fog_raster_from_discoveries(&tiles, &populations, &discovery, UVec2::new(2, 1));

        assert_eq!(fog.width, 2);
        assert_eq!(fog.height, 1);
        assert!(fog.samples[0] < Scalar::one().raw());
        assert_eq!(fog.samples[1], Scalar::one().raw());
    }
}
