use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::{
    great_discovery::GreatDiscoveryId,
    scalar::{scalar_zero, Scalar},
};

/// Identifier assigned to each power node in the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PowerNodeId(pub u32);

impl PowerNodeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Telemetry captured for each power node after the power phase resolves.
#[derive(Debug, Clone)]
pub struct PowerGridNodeTelemetry {
    pub entity: Entity,
    pub node_id: PowerNodeId,
    pub supply: Scalar,
    pub demand: Scalar,
    pub storage_level: Scalar,
    pub storage_capacity: Scalar,
    pub stability: Scalar,
    pub surplus: Scalar,
    pub deficit: Scalar,
    pub incident_count: u32,
}

impl Default for PowerGridNodeTelemetry {
    fn default() -> Self {
        Self {
            entity: Entity::from_bits(0),
            node_id: PowerNodeId(0),
            supply: scalar_zero(),
            demand: scalar_zero(),
            storage_level: scalar_zero(),
            storage_capacity: scalar_zero(),
            stability: Scalar::one(),
            surplus: scalar_zero(),
            deficit: scalar_zero(),
            incident_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerIncidentSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct PowerIncident {
    pub node_id: PowerNodeId,
    pub severity: PowerIncidentSeverity,
    pub deficit: Scalar,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PowerDiscoveryEffects {
    unlocked: HashSet<GreatDiscoveryId>,
}

impl PowerDiscoveryEffects {
    pub fn register(&mut self, id: GreatDiscoveryId) -> bool {
        self.unlocked.insert(id)
    }

    pub fn contains(&self, id: GreatDiscoveryId) -> bool {
        self.unlocked.contains(&id)
    }
}

/// Aggregated power grid state exported to telemetry and snapshot layers.
#[derive(Resource, Debug, Clone, Default)]
pub struct PowerGridState {
    pub nodes: HashMap<PowerNodeId, PowerGridNodeTelemetry>,
    pub total_supply: Scalar,
    pub total_demand: Scalar,
    pub total_storage: Scalar,
    pub total_capacity: Scalar,
    pub grid_stress_avg: f32,
    pub surplus_margin: f32,
    pub instability_alerts: u32,
    pub incidents: Vec<PowerIncident>,
}

impl PowerGridState {
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.total_supply = scalar_zero();
        self.total_demand = scalar_zero();
        self.total_storage = scalar_zero();
        self.total_capacity = scalar_zero();
        self.grid_stress_avg = 0.0;
        self.surplus_margin = 0.0;
        self.instability_alerts = 0;
        self.incidents.clear();
    }
}

/// Static representation of power line adjacency across the simulation grid.
#[derive(Resource, Debug, Clone, Default)]
pub struct PowerTopology {
    pub node_entities: Vec<Entity>,
    pub adjacency: Vec<Vec<PowerNodeId>>,
    pub default_capacity: Scalar,
}

impl PowerTopology {
    pub fn from_grid(
        entities: &[Entity],
        width: u32,
        height: u32,
        default_capacity: Scalar,
    ) -> Self {
        let count = entities.len();
        let mut adjacency = vec![Vec::new(); count];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let mut neighbours = Vec::with_capacity(4);
                if x > 0 {
                    neighbours.push(PowerNodeId(y * width + (x - 1)));
                }
                if x + 1 < width {
                    neighbours.push(PowerNodeId(y * width + (x + 1)));
                }
                if y > 0 {
                    neighbours.push(PowerNodeId((y - 1) * width + x));
                }
                if y + 1 < height {
                    neighbours.push(PowerNodeId((y + 1) * width + x));
                }
                adjacency[idx] = neighbours;
            }
        }

        Self {
            node_entities: entities.to_vec(),
            adjacency,
            default_capacity,
        }
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.node_entities.len()
    }

    #[inline]
    pub fn neighbours(&self, id: PowerNodeId) -> &[PowerNodeId] {
        self.adjacency
            .get(id.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
