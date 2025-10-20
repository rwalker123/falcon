use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use bevy::prelude::*;
use log::warn;
use sim_runtime::{
    encode_delta, encode_delta_flatbuffer, encode_snapshot, encode_snapshot_flatbuffer,
    AxisBiasState, CorruptionLedger, GenerationState, InfluentialIndividualState, LogisticsLinkState,
    PopulationCohortState, PowerNodeState, SentimentAxisTelemetry, SentimentDriverCategory,
    SentimentDriverState, SentimentTelemetryState, SnapshotHeader, TileState, WorldDelta,
    WorldSnapshot,
};

use crate::{
    components::{ElementKind, LogisticsLink, PopulationCohort, PowerNode, Tile},
    generations::{GenerationProfile, GenerationRegistry},
    influencers::{InfluencerImpacts, InfluentialRoster},
    resources::{
        CorruptionLedgers, CorruptionTelemetry, SentimentAxisBias, SimulationConfig,
        SimulationTick, TileRegistry,
    },
    scalar::Scalar,
};

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
    populations: HashMap<u64, PopulationCohortState>,
    power: HashMap<u64, PowerNodeState>,
    generations: HashMap<u16, GenerationState>,
    influencers: HashMap<u32, InfluentialIndividualState>,
    axis_bias: AxisBiasState,
    sentiment: SentimentTelemetryState,
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
            populations: HashMap::new(),
            power: HashMap::new(),
            generations: HashMap::new(),
            influencers: HashMap::new(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
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
        };

        let snapshot_arc = Arc::new(snapshot);
        let delta_arc = Arc::new(delta);
        let stored = StoredSnapshot::new(snapshot_arc.clone(), delta_arc.clone());

        self.tiles = tiles_index;
        self.logistics = logistics_index;
        self.populations = populations_index;
        self.power = power_index;
        self.generations = generations_index;
        self.influencers = influencers_index;
        self.axis_bias = axis_bias_state;
        self.sentiment = sentiment_state;
        self.corruption = corruption_state;
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
        self.corruption = entry.snapshot.corruption.clone();
        self.axis_bias = entry.snapshot.axis_bias.clone();
        self.sentiment = entry.snapshot.sentiment.clone();

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

    pub fn update_axis_bias(&mut self, bias: AxisBiasState) -> Option<(Arc<Vec<u8>>, Arc<Vec<u8>>)> {
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
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            axis_bias: Some(bias.clone()),
            sentiment: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: None,
            influencers: Vec::new(),
            removed_influencers: Vec::new(),
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
    ) -> Option<(Arc<Vec<u8>>, Arc<Vec<u8>>)> {
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
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            axis_bias: None,
            sentiment: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: None,
            influencers: added.clone(),
            removed_influencers: removed.clone(),
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

    pub fn update_corruption(
        &mut self,
        ledger: CorruptionLedger,
    ) -> Option<(Arc<Vec<u8>>, Arc<Vec<u8>>)> {
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
            populations: Vec::new(),
            removed_populations: Vec::new(),
            power: Vec::new(),
            removed_power: Vec::new(),
            axis_bias: None,
            sentiment: None,
            generations: Vec::new(),
            removed_generations: Vec::new(),
            corruption: Some(ledger.clone()),
            influencers: Vec::new(),
            removed_influencers: Vec::new(),
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

pub fn capture_snapshot(
    config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    tiles: Query<(Entity, &Tile)>,
    logistics_links: Query<(Entity, &LogisticsLink)>,
    populations: Query<(Entity, &PopulationCohort)>,
    power_nodes: Query<(Entity, &PowerNode)>,
    registry: Res<GenerationRegistry>,
    roster: Res<InfluentialRoster>,
    axis_bias: Res<SentimentAxisBias>,
    corruption_ledgers: Res<CorruptionLedgers>,
    corruption_telemetry: Res<CorruptionTelemetry>,
    mut history: ResMut<SnapshotHistory>,
) {
    history.set_capacity(config.snapshot_history_limit.max(1));

    let mut tile_states: Vec<TileState> = tiles
        .iter()
        .map(|(entity, tile)| tile_state(entity, tile))
        .collect();
    tile_states.sort_unstable_by_key(|state| state.entity);

    let mut logistics_states: Vec<LogisticsLinkState> = logistics_links
        .iter()
        .map(|(entity, link)| logistics_state(entity, link))
        .collect();
    logistics_states.sort_unstable_by_key(|state| state.entity);

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
        population_states.len(),
        power_states.len(),
        influencer_states.len(),
    );

    let snapshot = WorldSnapshot {
        header,
        tiles: tile_states,
        logistics: logistics_states,
        populations: population_states,
        power: power_states,
        axis_bias: axis_bias_state,
        sentiment: sentiment_state,
        generations: generation_states,
        corruption: corruption_ledgers.ledger().clone(),
        influencers: influencer_states,
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

        world.spawn(LogisticsLink {
            from: from_entity,
            to: to_entity,
            capacity: Scalar::from_raw(link_state.capacity),
            flow: Scalar::from_raw(link_state.flow),
        });
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
        world.spawn(PopulationCohort {
            home: home_entity,
            size: cohort_state.size,
            morale: Scalar::from_raw(cohort_state.morale),
            generation: cohort_state.generation,
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
        .map(|id| *id)
        .collect()
}

fn tile_state(entity: Entity, tile: &Tile) -> TileState {
    TileState {
        entity: entity.to_bits(),
        x: tile.position.x,
        y: tile.position.y,
        element: u8::from(tile.element),
        mass: tile.mass.raw(),
        temperature: tile.temperature.raw(),
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

fn population_state(entity: Entity, cohort: &PopulationCohort) -> PopulationCohortState {
    PopulationCohortState {
        entity: entity.to_bits(),
        home: cohort.home.to_bits(),
        size: cohort.size,
        morale: cohort.morale.raw(),
        generation: cohort.generation,
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
