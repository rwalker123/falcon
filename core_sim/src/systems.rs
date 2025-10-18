use std::cmp::max;

use bevy::{math::UVec2, prelude::*};

use crate::{
    components::{ElementKind, LogisticsLink, PopulationCohort, PowerNode, Tile},
    resources::{SimulationConfig, SimulationTick, TileRegistry},
    scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero},
};

/// Spawn initial grid of tiles, logistics links, power nodes, and population cohorts.
pub fn spawn_initial_world(mut commands: Commands, config: Res<SimulationConfig>) {
    let width = config.grid_size.x as usize;
    let height = config.grid_size.y as usize;
    let mut tiles = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            let position = UVec2::new(x as u32, y as u32);
            let element = ElementKind::from_grid(position);
            let (generation, demand, efficiency) = element.power_profile();
            let base_mass = scalar_from_f32(1.0 + ((x + y) % 5) as f32 * 0.35);
            let tile_entity = commands
                .spawn((
                    Tile {
                        position,
                        element,
                        mass: base_mass,
                        temperature: config.ambient_temperature + element.thermal_bias(),
                    },
                    PowerNode {
                        generation,
                        demand,
                        efficiency,
                    },
                ))
                .id();
            tiles.push(tile_entity);
        }
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let from_entity = tiles[idx];
            if x + 1 < width {
                let to_entity = tiles[y * width + (x + 1)];
                commands.spawn(LogisticsLink {
                    from: from_entity,
                    to: to_entity,
                    capacity: config.base_link_capacity,
                    flow: scalar_zero(),
                });
            }
            if y + 1 < height {
                let to_entity = tiles[(y + 1) * width + x];
                commands.spawn(LogisticsLink {
                    from: from_entity,
                    to: to_entity,
                    capacity: config.base_link_capacity,
                    flow: scalar_zero(),
                });
            }
        }
    }

    let stride = max(1, config.population_cluster_stride) as usize;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let tile_entity = tiles[y * width + x];
            commands.spawn(PopulationCohort {
                home: tile_entity,
                size: 1_000,
                morale: scalar_from_f32(0.6),
            });
        }
    }

    commands.insert_resource(TileRegistry {
        tiles,
        width: config.grid_size.x,
        height: config.grid_size.y,
    });
}

/// Relax material temperatures and adjust masses using deterministic rules.
pub fn simulate_materials(config: Res<SimulationConfig>, mut tiles: Query<&mut Tile>) {
    for mut tile in tiles.iter_mut() {
        let target = config.ambient_temperature + tile.element.thermal_bias();
        let delta = (target - tile.temperature) * config.temperature_lerp;
        let conductivity = tile.element.conductivity();
        tile.temperature += delta * conductivity;
        let flux = tile.element.mass_flux() * config.mass_flux_epsilon;
        let new_mass = tile.mass + flux;
        tile.mass = new_mass.clamp(config.mass_bounds.0, config.mass_bounds.1);
    }
}

/// Move resources along logistics links based on mass gradients.
pub fn simulate_logistics(
    config: Res<SimulationConfig>,
    mut links: Query<&mut LogisticsLink>,
    mut tiles: Query<&mut Tile>,
) {
    for mut link in links.iter_mut() {
        let Ok([mut source, mut target]) = tiles.get_many_mut([link.from, link.to]) else {
            link.flow = scalar_zero();
            continue;
        };
        let gradient = source.mass - target.mass;
        let transfer = (gradient * config.logistics_flow_gain).clamp(-link.capacity, link.capacity);
        source.mass -= transfer;
        target.mass += transfer;
        link.flow = transfer;
    }
}

/// Update population cohorts based on environmental conditions.
pub fn simulate_population(
    config: Res<SimulationConfig>,
    tiles: Query<&Tile>,
    mut cohorts: Query<&mut PopulationCohort>,
) {
    let max_cap_scalar = scalar_from_u32(config.population_cap);
    for mut cohort in cohorts.iter_mut() {
        let Ok(tile) = tiles.get(cohort.home) else {
            cohort.morale = scalar_zero();
            continue;
        };
        let temp_diff = (tile.temperature - config.ambient_temperature).abs();
        let morale_delta =
            config.population_growth_rate - temp_diff * config.temperature_morale_penalty;
        cohort.morale = (cohort.morale + morale_delta).clamp(scalar_zero(), scalar_one());

        let growth_factor = (config.population_growth_rate - temp_diff * scalar_from_f32(0.0005))
            .clamp(scalar_from_f32(-0.05), scalar_from_f32(0.05));
        let current_size = scalar_from_u32(cohort.size);
        let new_size =
            (current_size * (scalar_one() + growth_factor)).clamp(scalar_zero(), max_cap_scalar);
        cohort.size = new_size.to_u32();
    }
}

/// Adjust power nodes in response to tile state and demand.
pub fn simulate_power(mut nodes: Query<(&Tile, &mut PowerNode)>, config: Res<SimulationConfig>) {
    for (tile, mut node) in nodes.iter_mut() {
        let efficiency_adjust =
            (config.ambient_temperature - tile.temperature) * config.power_adjust_rate;
        node.efficiency = (node.efficiency + efficiency_adjust * scalar_from_f32(0.01))
            .clamp(scalar_from_f32(0.5), scalar_from_f32(1.5));
        let net = node.generation * node.efficiency - node.demand;
        node.generation = (node.generation + net * scalar_from_f32(0.05))
            .clamp(scalar_zero(), config.max_power_generation);
        node.demand = (node.demand + (-net) * scalar_from_f32(0.03))
            .clamp(scalar_zero(), config.max_power_generation);
    }
}

/// Increment global tick counter after simulation step.
pub fn advance_tick(mut tick: ResMut<SimulationTick>) {
    tick.0 = tick.0.wrapping_add(1);
}
