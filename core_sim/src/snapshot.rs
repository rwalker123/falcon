use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bevy::prelude::*;
use log::warn;
use sim_proto::{
    encode_delta, encode_snapshot, LogisticsLinkState, PopulationCohortState, PowerNodeState,
    SnapshotHeader, TileState, WorldDelta, WorldSnapshot,
};

use crate::{
    components::{ElementKind, LogisticsLink, PopulationCohort, PowerNode, Tile},
    resources::{SimulationConfig, SimulationTick, TileRegistry},
    scalar::Scalar,
};

#[derive(Clone)]
pub struct StoredSnapshot {
    pub tick: u64,
    pub snapshot: Arc<WorldSnapshot>,
    pub delta: Arc<WorldDelta>,
    pub encoded_snapshot: Arc<Vec<u8>>,
    pub encoded_delta: Arc<Vec<u8>>,
}

impl StoredSnapshot {
    fn new(snapshot: Arc<WorldSnapshot>, delta: Arc<WorldDelta>) -> Self {
        let encoded_snapshot =
            Arc::new(encode_snapshot(snapshot.as_ref()).expect("snapshot serialization failed"));
        let encoded_delta =
            Arc::new(encode_delta(delta.as_ref()).expect("delta serialization failed"));
        Self {
            tick: snapshot.header.tick,
            snapshot,
            delta,
            encoded_snapshot,
            encoded_delta,
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
    tiles: HashMap<u64, TileState>,
    logistics: HashMap<u64, LogisticsLinkState>,
    populations: HashMap<u64, PopulationCohortState>,
    power: HashMap<u64, PowerNodeState>,
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
            tiles: HashMap::new(),
            logistics: HashMap::new(),
            populations: HashMap::new(),
            power: HashMap::new(),
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
        };

        let snapshot_arc = Arc::new(snapshot);
        let delta_arc = Arc::new(delta);
        let stored = StoredSnapshot::new(snapshot_arc.clone(), delta_arc.clone());

        self.tiles = tiles_index;
        self.logistics = logistics_index;
        self.populations = populations_index;
        self.power = power_index;
        self.last_snapshot = Some(snapshot_arc);
        self.last_delta = Some(delta_arc);
        self.encoded_snapshot = Some(stored.encoded_snapshot.clone());
        self.encoded_delta = Some(stored.encoded_delta.clone());
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

        self.last_snapshot = Some(entry.snapshot.clone());
        self.last_delta = Some(entry.delta.clone());
        self.encoded_snapshot = Some(entry.encoded_snapshot.clone());
        self.encoded_delta = Some(entry.encoded_delta.clone());

        while let Some(back) = self.history.back() {
            if back.tick > entry.tick {
                self.history.pop_back();
            } else {
                break;
            }
        }
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

    let header = SnapshotHeader::new(
        tick.0,
        tile_states.len(),
        logistics_states.len(),
        population_states.len(),
        power_states.len(),
    );

    let snapshot = WorldSnapshot {
        header,
        tiles: tile_states,
        logistics: logistics_states,
        populations: population_states,
        power: power_states,
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
}

fn diff_new<T>(previous: &HashMap<u64, T>, current: &HashMap<u64, T>) -> Vec<T>
where
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

fn diff_removed<T>(previous: &HashMap<u64, T>, current: &HashMap<u64, T>) -> Vec<u64> {
    previous
        .keys()
        .filter(|id| !current.contains_key(id))
        .cloned()
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
