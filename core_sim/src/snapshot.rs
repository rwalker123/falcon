use std::collections::HashMap;

use bevy::prelude::*;
use sim_proto::{
    encode_delta, encode_snapshot, LogisticsLinkState, PopulationCohortState, PowerNodeState,
    SnapshotHeader, TileState, WorldDelta, WorldSnapshot,
};

use crate::{
    components::{LogisticsLink, PopulationCohort, PowerNode, Tile},
    resources::SimulationTick,
};

#[derive(Resource, Default)]
pub struct SnapshotHistory {
    pub last_snapshot: Option<WorldSnapshot>,
    pub last_delta: Option<WorldDelta>,
    pub encoded_snapshot: Option<Vec<u8>>,
    pub encoded_delta: Option<Vec<u8>>,
    tiles: HashMap<u64, TileState>,
    logistics: HashMap<u64, LogisticsLinkState>,
    populations: HashMap<u64, PopulationCohortState>,
    power: HashMap<u64, PowerNodeState>,
}

pub fn capture_snapshot(
    tick: Res<SimulationTick>,
    tiles: Query<(Entity, &Tile)>,
    logistics_links: Query<(Entity, &LogisticsLink)>,
    populations: Query<(Entity, &PopulationCohort)>,
    power_nodes: Query<(Entity, &PowerNode)>,
    mut history: ResMut<SnapshotHistory>,
) {
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

impl SnapshotHistory {
    fn update(&mut self, snapshot: WorldSnapshot) -> WorldDelta {
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

        self.encoded_snapshot =
            Some(encode_snapshot(&snapshot).expect("snapshot serialization failed"));
        self.encoded_delta = Some(encode_delta(&delta).expect("delta serialization failed"));
        self.tiles = tiles_index;
        self.logistics = logistics_index;
        self.populations = populations_index;
        self.power = power_index;
        self.last_snapshot = Some(snapshot);
        self.last_delta = Some(delta.clone());
        delta
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
