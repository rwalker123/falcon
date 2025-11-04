use std::sync::Arc;

use bevy::{ecs::system::SystemState, prelude::*};
use core_sim::{
    scalar_from_f32, scalar_zero, simulate_power, CorruptionLedgers, CultureCorruptionConfig,
    CultureCorruptionConfigHandle, CultureEffectsCache, ElementKind, InfluencerImpacts,
    PowerGridState, PowerNode, PowerNodeId, PowerSimParams, PowerTopology, Scalar,
    SimulationConfig, Tile, TurnPipelineConfig, TurnPipelineConfigHandle,
};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use sim_runtime::{TerrainTags, TerrainType};

#[derive(Clone, Copy)]
struct NodeSpec {
    base_generation: f32,
    base_demand: f32,
    storage_capacity: f32,
    storage_level: f32,
}

fn configure_power_app(width: u32, height: u32, specs: &[NodeSpec], default_capacity: f32) -> App {
    let mut app = App::new();

    app.insert_resource(SimulationConfig::builtin());
    {
        let mut config = app.world.resource_mut::<SimulationConfig>();
        config.grid_size = UVec2::new(width, height);
        config.ambient_temperature = scalar_zero();
        config.power_generation_adjust_rate = 0.0;
        config.power_demand_adjust_rate = 0.0;
        config.power_storage_stability_bonus = 0.0;
        config.power_storage_efficiency = Scalar::one();
        config.power_storage_bleed = scalar_zero();
        config.power_adjust_rate = scalar_zero();
        config.max_power_generation = scalar_from_f32(200.0);
    }

    app.insert_resource(CultureEffectsCache::default());
    app.insert_resource(InfluencerImpacts::default());
    app.insert_resource(CorruptionLedgers::default());
    app.insert_resource(PowerGridState::default());
    app.insert_resource(CultureCorruptionConfigHandle::new(Arc::new(
        CultureCorruptionConfig::default(),
    )));
    app.insert_resource(TurnPipelineConfigHandle::new(Arc::new(
        TurnPipelineConfig::default(),
    )));

    let entities = spawn_power_nodes(&mut app.world, width, height, specs);
    let topology =
        PowerTopology::from_grid(&entities, width, height, scalar_from_f32(default_capacity));
    app.insert_resource(topology);

    app
}

fn spawn_power_nodes(
    world: &mut World,
    width: u32,
    height: u32,
    specs: &[NodeSpec],
) -> Vec<Entity> {
    assert_eq!(specs.len(), (width * height) as usize);
    let mut entities = Vec::with_capacity(specs.len());

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let spec = specs[idx];
            let entity = world
                .spawn((
                    Tile {
                        position: UVec2::new(x, y),
                        element: ElementKind::Ferrite,
                        mass: Scalar::one(),
                        temperature: scalar_zero(),
                        terrain: TerrainType::AlluvialPlain,
                        terrain_tags: TerrainTags::empty(),
                        mountain: None,
                    },
                    PowerNode {
                        id: PowerNodeId(idx as u32),
                        base_generation: Scalar::from_f32(spec.base_generation),
                        base_demand: Scalar::from_f32(spec.base_demand),
                        generation: Scalar::from_f32(spec.base_generation),
                        demand: Scalar::from_f32(spec.base_demand),
                        efficiency: Scalar::one(),
                        storage_capacity: Scalar::from_f32(spec.storage_capacity),
                        storage_level: Scalar::from_f32(spec.storage_level),
                        stability: Scalar::one(),
                        surplus: scalar_zero(),
                        deficit: scalar_zero(),
                        incident_count: 0,
                    },
                ))
                .id();
            entities.push(entity);
        }
    }

    entities
}

fn run_power_iteration(app: &mut App) {
    let mut system_state = SystemState::<PowerSimParams>::new(&mut app.world);
    {
        let params = system_state.get_mut(&mut app.world);
        simulate_power(params);
    }
    system_state.apply(&mut app.world);
}

fn balanced_specs(width: u32, height: u32) -> Vec<NodeSpec> {
    (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let base = 10.0 + ((x + y) % 3) as f32;
                NodeSpec {
                    base_generation: base,
                    base_demand: base,
                    storage_capacity: 6.0,
                    storage_level: 3.0,
                }
            })
        })
        .collect()
}

fn cascading_specs(width: u32, height: u32) -> Vec<NodeSpec> {
    (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let pattern = (x + 2 * y) % 4;
                match pattern {
                    0 => NodeSpec {
                        base_generation: 14.0,
                        base_demand: 6.0,
                        storage_capacity: 2.0,
                        storage_level: 0.5,
                    },
                    1 => NodeSpec {
                        base_generation: 8.0,
                        base_demand: 12.0,
                        storage_capacity: 1.0,
                        storage_level: 0.0,
                    },
                    2 => NodeSpec {
                        base_generation: 10.0,
                        base_demand: 9.0,
                        storage_capacity: 4.0,
                        storage_level: 1.5,
                    },
                    _ => NodeSpec {
                        base_generation: 6.0,
                        base_demand: 11.5,
                        storage_capacity: 3.0,
                        storage_level: 0.0,
                    },
                }
            })
        })
        .collect()
}

fn bench_power_phase(c: &mut Criterion) {
    let mut group = c.benchmark_group("power_stability");

    for size in [8u32, 16, 32] {
        group.bench_with_input(BenchmarkId::new("balanced", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let specs = balanced_specs(size, size);
                    configure_power_app(size, size, &specs, 5.0)
                },
                |mut app| {
                    run_power_iteration(&mut app);
                },
                BatchSize::SmallInput,
            );
        });

        group.bench_with_input(
            BenchmarkId::new("cascading_deficit", size),
            &size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let specs = cascading_specs(size, size);
                        configure_power_app(size, size, &specs, 4.0)
                    },
                    |mut app| {
                        run_power_iteration(&mut app);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(power_benches, bench_power_phase);
criterion_main!(power_benches);
