use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::{
    great_discovery::GreatDiscoveryId,
    scalar::{scalar_zero, Scalar},
};

/// Identifier assigned to each power node in the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PowerNodeId(pub u32);

impl PowerNodeId {
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Telemetry captured for each power node after the power phase resolves.
///
/// **Keyed by [`PowerNodeId`], never by `Entity`.** This carried an `entity: Entity` alongside
/// `node_id` until it was found to be the only `Entity` anywhere in the `SimState` closure, in
/// breach of the checkpoint's first construction rule — and one that nothing read, so a restore
/// reinstated a handle naming a tile that had just been despawned. `PowerNodeId` is `y * width + x`
/// and is the key this map is already stored under, so there was nothing for the entity to say
/// that the id did not say durably.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerGridNodeTelemetry {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerIncidentSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
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
///
/// **Everything here is keyed by position, never by `Entity`.** A [`PowerNodeId`] is
/// `y * width + x`, and [`Self::adjacency`] is indexed by it, so the whole structure holds across a
/// checkpoint restore — which renumbers every tile entity — by construction.
///
/// This used to carry a `node_entities: Vec<Entity>` whose only use anywhere was its `.len()`. A
/// restore despawns and respawns every tile and never rebuilt the vector, so all of its handles
/// were stale afterwards; the grid still routed correctly only because nothing dereferenced them.
/// Storing the count directly removes the stale handles rather than refreshing them, and takes a
/// world-sized `Vec<Entity>` (130 KiB at 160x104) out of the resource with it.
#[derive(Resource, Debug, Clone, Default)]
pub struct PowerTopology {
    /// How many nodes the grid had when this topology was built. `simulate_power` compares it
    /// against the live node count and skips inter-node transfer when they disagree, which is the
    /// guard against routing over an adjacency table built for a different world.
    node_count: usize,
    pub adjacency: Vec<Vec<PowerNodeId>>,
    pub default_capacity: Scalar,
}

impl PowerTopology {
    pub fn from_grid(node_count: usize, width: u32, height: u32, default_capacity: Scalar) -> Self {
        let count = node_count;
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
            node_count,
            adjacency,
            default_capacity,
        }
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    #[inline]
    pub fn neighbours(&self, id: PowerNodeId) -> &[PowerNodeId] {
        self.adjacency
            .get(id.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
