use super::*;

#[derive(SystemParam)]
pub struct PowerSimParams<'w, 's> {
    pub nodes: Query<'w, 's, (Entity, &'static Tile, &'static mut PowerNode)>,
    pub config: Res<'w, SimulationConfig>,
    pub topology: Res<'w, PowerTopology>,
    pub grid_state: ResMut<'w, PowerGridState>,
    pub impacts: Res<'w, InfluencerImpacts>,
    pub effects: Res<'w, CultureEffectsCache>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
}

/// Adjust power nodes in response to tile state and demand.
pub fn simulate_power(mut params: PowerSimParams) {
    #[derive(Clone, Copy)]
    struct NodeCalc {
        entity: Entity,
        id: PowerNodeId,
        generation: Scalar,
        demand: Scalar,
        storage_capacity: Scalar,
        storage_level: Scalar,
        net: Scalar,
        incident_count: u32,
    }

    #[derive(Clone, Copy)]
    struct NodeResult {
        entity: Entity,
        id: PowerNodeId,
        surplus: Scalar,
        deficit: Scalar,
        storage_level: Scalar,
        storage_capacity: Scalar,
        stability: Scalar,
        incident_count: u32,
        generation: Scalar,
        demand: Scalar,
    }

    let config = &*params.config;
    let impacts = &*params.impacts;
    let effects = &*params.effects;
    let topology = params.topology.as_ref();
    let grid_state = params.grid_state.as_mut();

    let corruption_cfg = params.severity_config.config().corruption();
    let power_cfg = params.pipeline_config.config().power();
    let corruption_factor = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Military,
        config.corruption_military_penalty,
        corruption_cfg,
    );

    let mut node_calcs: Vec<NodeCalc> = Vec::with_capacity(params.nodes.iter().len());
    let mut node_index: HashMap<PowerNodeId, usize> =
        HashMap::with_capacity(params.nodes.iter().len());

    for (entity, tile, mut node) in params.nodes.iter_mut() {
        let efficiency_adjust =
            (config.ambient_temperature - tile.temperature) * config.power_adjust_rate;
        node.efficiency = (node.efficiency
            + efficiency_adjust * power_cfg.efficiency_adjust_scale())
        .clamp(power_cfg.efficiency_floor(), config.max_power_efficiency);

        let influence_bonus = (impacts.power_bonus + effects.power_bonus).clamp(
            scalar_from_f32(config.min_power_influence),
            scalar_from_f32(config.max_power_influence),
        );

        let effective_generation = (node.base_generation * node.efficiency + influence_bonus)
            .clamp(scalar_zero(), config.max_power_generation);
        let target_demand = (node.base_demand
            - influence_bonus * power_cfg.influence_demand_reduction())
        .clamp(scalar_zero(), config.max_power_generation);
        let net = (effective_generation - target_demand) * corruption_factor;

        node.generation = (node.base_generation
            + net * scalar_from_f32(config.power_generation_adjust_rate))
        .clamp(scalar_zero(), config.max_power_generation);
        node.demand = (node.base_demand - net * scalar_from_f32(config.power_demand_adjust_rate))
            .clamp(scalar_zero(), config.max_power_generation);

        let net_supply = node.generation - node.demand;

        let next_index = node_calcs.len();
        node_calcs.push(NodeCalc {
            entity,
            id: node.id,
            generation: node.generation,
            demand: node.demand,
            storage_capacity: node.storage_capacity,
            storage_level: node.storage_level,
            net: net_supply,
            incident_count: node.incident_count,
        });
        node_index.insert(node.id, next_index);
    }

    let node_count = node_calcs.len();
    if node_count == 0 {
        grid_state.reset();
        return;
    }

    let mut nets: Vec<Scalar> = node_calcs.iter().map(|node| node.net).collect();
    let mut storage_levels: Vec<Scalar> = node_calcs
        .iter()
        .map(|node| {
            node.storage_level
                .clamp(scalar_zero(), node.storage_capacity)
        })
        .collect();

    if topology.node_count() == node_count {
        for idx in 0..node_count {
            if nets[idx] <= scalar_zero() {
                continue;
            }
            for neighbour in topology.neighbours(node_calcs[idx].id) {
                let Some(&n_idx) = node_index.get(neighbour) else {
                    continue;
                };
                if nets[n_idx] >= scalar_zero() {
                    continue;
                }
                let needed = (-nets[n_idx]).clamp(scalar_zero(), topology.default_capacity);
                if needed <= scalar_zero() {
                    continue;
                }
                let available = nets[idx].min(topology.default_capacity);
                if available <= scalar_zero() {
                    continue;
                }
                let transfer = if available < needed {
                    available
                } else {
                    needed
                };
                if transfer > scalar_zero() {
                    nets[idx] -= transfer;
                    nets[n_idx] += transfer;
                }
            }
        }
    }

    let storage_efficiency = config.power_storage_efficiency.clamp(
        power_cfg.storage_efficiency_min(),
        power_cfg.storage_efficiency_max(),
    );
    let storage_bleed = config
        .power_storage_bleed
        .clamp(scalar_zero(), power_cfg.storage_bleed_max());

    for idx in 0..node_count {
        if nets[idx] > scalar_zero() {
            let capacity_left = (node_calcs[idx].storage_capacity - storage_levels[idx])
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            if capacity_left > scalar_zero() {
                let mut charge = nets[idx].min(capacity_left);
                charge *= storage_efficiency;
                storage_levels[idx] = (storage_levels[idx] + charge)
                    .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
                nets[idx] -= charge;
            }
        } else if nets[idx] < scalar_zero() && storage_levels[idx] > scalar_zero() {
            let needed = (-nets[idx]).clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            let discharge = storage_levels[idx].min(needed);
            let delivered = discharge * storage_efficiency;
            storage_levels[idx] = (storage_levels[idx] - discharge)
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            nets[idx] += delivered;
        }

        if storage_levels[idx] > scalar_zero() && storage_bleed > scalar_zero() {
            let bleed = storage_levels[idx] * storage_bleed;
            storage_levels[idx] = (storage_levels[idx] - bleed)
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
        }
    }

    let warn_threshold = config
        .power_instability_warn
        .clamp(scalar_zero(), Scalar::one());
    let critical_threshold = config
        .power_instability_critical
        .clamp(scalar_zero(), Scalar::one());

    let mut results: Vec<NodeResult> = Vec::with_capacity(node_count);
    let mut incidents: Vec<PowerIncident> = Vec::new();
    let mut stress_sum = 0.0f32;
    let mut total_supply = scalar_zero();
    let mut total_demand = scalar_zero();
    let mut total_storage = scalar_zero();
    let mut total_capacity = scalar_zero();
    let mut alert_count: u32 = 0;

    for idx in 0..node_count {
        let demand = node_calcs[idx]
            .demand
            .clamp(scalar_zero(), config.max_power_generation);
        let generation = node_calcs[idx]
            .generation
            .clamp(scalar_zero(), config.max_power_generation);

        let surplus = if nets[idx] > scalar_zero() {
            nets[idx]
        } else {
            scalar_zero()
        };

        let deficit = if nets[idx] < scalar_zero() {
            -nets[idx]
        } else {
            scalar_zero()
        };

        let fulfilled = if deficit >= demand {
            scalar_zero()
        } else {
            demand - deficit
        };

        let mut stability = if demand > scalar_zero() {
            (fulfilled / demand).clamp(scalar_zero(), Scalar::one())
        } else {
            Scalar::one()
        };

        if storage_levels[idx] > scalar_zero() && demand > scalar_zero() {
            let reserve_ratio = (storage_levels[idx] / demand).clamp(scalar_zero(), Scalar::one());
            stability = (stability
                + reserve_ratio * scalar_from_f32(config.power_storage_stability_bonus))
            .clamp(scalar_zero(), Scalar::one());
        }

        let mut incident_count = node_calcs[idx].incident_count;
        if stability < critical_threshold {
            incidents.push(PowerIncident {
                node_id: node_calcs[idx].id,
                severity: PowerIncidentSeverity::Critical,
                deficit,
            });
            incident_count = incident_count.saturating_add(1);
            alert_count = alert_count.saturating_add(1);
        } else if stability < warn_threshold {
            incidents.push(PowerIncident {
                node_id: node_calcs[idx].id,
                severity: PowerIncidentSeverity::Warning,
                deficit,
            });
            alert_count = alert_count.saturating_add(1);
        }

        stress_sum += if demand > scalar_zero() {
            (deficit / demand).to_f32().clamp(0.0, 1.0)
        } else {
            0.0
        };

        total_supply += generation;
        total_demand += demand;
        total_storage += storage_levels[idx];
        total_capacity += node_calcs[idx].storage_capacity;

        results.push(NodeResult {
            entity: node_calcs[idx].entity,
            id: node_calcs[idx].id,
            surplus,
            deficit,
            storage_level: storage_levels[idx],
            storage_capacity: node_calcs[idx].storage_capacity,
            stability,
            incident_count,
            generation,
            demand,
        });
    }

    grid_state.reset();
    grid_state.total_supply = total_supply;
    grid_state.total_demand = total_demand;
    grid_state.total_storage = total_storage;
    grid_state.total_capacity = total_capacity;
    grid_state.instability_alerts = alert_count;
    grid_state.incidents = incidents;
    grid_state.grid_stress_avg = (stress_sum / node_count as f32).clamp(0.0, 1.0);
    let demand_f32 = total_demand.to_f32().max(1.0);
    let surplus_margin = ((total_supply + total_storage).to_f32() / demand_f32) - 1.0;
    grid_state.surplus_margin = surplus_margin;

    for node in &results {
        grid_state.nodes.insert(
            node.id,
            PowerGridNodeTelemetry {
                entity: node.entity,
                node_id: node.id,
                supply: node.generation,
                demand: node.demand,
                storage_level: node.storage_level,
                storage_capacity: node.storage_capacity,
                stability: node.stability,
                surplus: node.surplus,
                deficit: node.deficit,
                incident_count: node.incident_count,
            },
        );
    }

    let mut result_lookup: HashMap<Entity, NodeResult> = results
        .into_iter()
        .map(|node| (node.entity, node))
        .collect();

    for (entity, _tile, mut node) in params.nodes.iter_mut() {
        if let Some(result) = result_lookup.remove(&entity) {
            node.storage_level = result.storage_level;
            node.storage_capacity = result.storage_capacity;
            node.stability = result.stability;
            node.surplus = result.surplus;
            node.deficit = result.deficit;
            node.incident_count = result.incident_count;
        }
    }
}

/// React to culture tension events by nudging sentiment and diplomacy telemetry.
pub fn process_culture_events(
    mut sentiment: ResMut<SentimentAxisBias>,
    mut diplomacy: ResMut<DiplomacyLeverage>,
    mut tension_events: EventReader<CultureTensionEvent>,
    mut schism_events: EventReader<CultureSchismEvent>,
    severity_config: Res<CultureCorruptionConfigHandle>,
) {
    let config = severity_config.config();
    let culture_cfg = config.culture();
    let trust_axis = culture_cfg.trust_axis();
    let drift_tuning = culture_cfg.drift_warning();
    let assimilation_tuning = culture_cfg.assimilation_push();
    let schism_tuning = culture_cfg.schism_risk();

    for event in tension_events.read() {
        match event.kind {
            CultureTensionKind::DriftWarning => {
                let delta = drift_tuning.delta_for_magnitude(event.magnitude);
                sentiment.apply_incident_delta(trust_axis, -delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::AssimilationPush => {
                let delta = assimilation_tuning.delta_for_magnitude(event.magnitude);
                sentiment.apply_incident_delta(trust_axis, delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::SchismRisk => {
                // Handled in the dedicated schism pass below.
            }
        }
    }

    for event in schism_events.read() {
        let delta = schism_tuning.delta_for_magnitude(event.magnitude);
        sentiment.apply_incident_delta(trust_axis, -delta);
        diplomacy.push_culture_signal(CultureTensionRecord::from(event));
    }
}

/// Increment global tick counter after simulation step.
pub fn advance_tick(mut tick: ResMut<SimulationTick>) {
    tick.0 = tick.0.wrapping_add(1);
}

/// Resolve corruption timers, apply sentiment penalties, and emit telemetry.
pub fn process_corruption(
    mut ledgers: ResMut<CorruptionLedgers>,
    mut sentiment: ResMut<SentimentAxisBias>,
    mut telemetry: ResMut<CorruptionTelemetry>,
    mut diplomacy: ResMut<DiplomacyLeverage>,
    severity_config: Res<CultureCorruptionConfigHandle>,
    tick: Res<SimulationTick>,
) {
    telemetry.reset_turn();

    let ledger = ledgers.ledger_mut();
    let mut resolved: Vec<u64> = Vec::new();
    let corruption_cfg = severity_config.config().corruption();
    let trust_idx = corruption_cfg.trust_axis();
    let (delta_min, delta_max) = corruption_cfg.sentiment_delta_bounds();

    for entry in ledger.entries.iter_mut() {
        if entry.exposure_timer > 0 {
            entry.exposure_timer = entry.exposure_timer.saturating_sub(1);
        }

        if entry.exposure_timer == 0 {
            let mut record = CorruptionExposureRecord {
                incident_id: entry.incident_id,
                subsystem: entry.subsystem,
                intensity: entry.intensity,
                trust_delta: 0,
            };

            let delta = Scalar::from_raw(entry.intensity).clamp(delta_min, delta_max);
            record.trust_delta = (-delta).raw();
            telemetry.record_exposure(record.clone());
            diplomacy.push(record.clone());

            sentiment.apply_incident_delta(trust_idx, -delta);

            ledger.reputation_modifier = ledger.reputation_modifier.saturating_sub(entry.intensity);
            entry.last_update_tick = tick.0;
            resolved.push(entry.incident_id);
        }
    }

    for incident_id in resolved {
        ledger.remove_incident(incident_id);
    }

    telemetry.active_incidents = ledger.entry_count();
}

#[cfg(test)]
mod power_tests {
    use super::*;
    use crate::{CultureCorruptionConfig, TurnPipelineConfig};
    use bevy::{
        ecs::system::SystemState,
        prelude::{App, Entity, UVec2, World},
    };
    use sim_runtime::{TerrainTags, TerrainType};
    use std::sync::Arc;

    #[derive(Clone, Copy)]
    struct NodeSpec {
        base_generation: f32,
        base_demand: f32,
        storage_capacity: f32,
        storage_level: f32,
        incident_count: u32,
    }

    impl NodeSpec {
        fn new(base_generation: f32, base_demand: f32) -> Self {
            Self {
                base_generation,
                base_demand,
                storage_capacity: 0.0,
                storage_level: 0.0,
                incident_count: 0,
            }
        }
    }

    fn configure_simulation(app: &mut App, grid_size: UVec2) {
        app.insert_resource(SimulationConfig::builtin());
        {
            let mut config = app.world.resource_mut::<SimulationConfig>();
            config.grid_size = grid_size;
            config.power_generation_adjust_rate = 0.0;
            config.power_demand_adjust_rate = 0.0;
            config.power_storage_stability_bonus = 0.0;
            config.power_storage_efficiency = Scalar::one();
            config.power_storage_bleed = scalar_zero();
            config.power_adjust_rate = scalar_zero();
            config.max_power_generation = scalar_from_f32(100.0);
            config.power_instability_warn = scalar_from_f32(0.8);
            config.power_instability_critical = scalar_from_f32(0.5);
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
    }

    fn spawn_power_nodes(
        world: &mut World,
        width: u32,
        height: u32,
        specs: &[NodeSpec],
    ) -> Vec<Entity> {
        assert_eq!(specs.len(), (width * height) as usize);
        let ambient_temperature = world.resource::<SimulationConfig>().ambient_temperature;
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
                            temperature: ambient_temperature,
                            terrain: TerrainType::AlluvialPlain,
                            terrain_tags: TerrainTags::empty(),
                            underlying_terrain: None,
                            mountain: None,
                            river_edges: 0,
                            river_inflow: 0,
                            river_channel: 0,
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
                            incident_count: spec.incident_count,
                        },
                    ))
                    .id();
                entities.push(entity);
            }
        }

        entities
    }

    fn run_power_system(app: &mut App) {
        let mut system_state = SystemState::<PowerSimParams>::new(&mut app.world);
        {
            let params = system_state.get_mut(&mut app.world);
            simulate_power(params);
        }
        system_state.apply(&mut app.world);
    }

    #[test]
    fn simulate_power_emits_expected_incidents_for_stability_thresholds() {
        let mut app = App::new();
        configure_simulation(&mut app, UVec2::new(3, 1));

        let specs = vec![
            NodeSpec::new(10.0, 6.0),
            NodeSpec::new(7.0, 10.0),
            NodeSpec::new(3.0, 10.0),
        ];

        let entities = spawn_power_nodes(&mut app.world, 3, 1, &specs);
        let topology = PowerTopology::from_grid(&entities, 3, 1, scalar_zero());
        app.insert_resource(topology);

        run_power_system(&mut app);

        let grid_state = app.world.resource::<PowerGridState>();
        assert_eq!(grid_state.instability_alerts, 2);
        assert_eq!(grid_state.incidents.len(), 2);

        let mut warning_count = 0;
        let mut critical_count = 0;
        for incident in &grid_state.incidents {
            match incident.severity {
                PowerIncidentSeverity::Warning => warning_count += 1,
                PowerIncidentSeverity::Critical => critical_count += 1,
            }
        }
        assert_eq!(warning_count, 1);
        assert_eq!(critical_count, 1);

        let warn_node = grid_state
            .nodes
            .get(&PowerNodeId(1))
            .expect("warn node telemetry present");
        let critical_node = grid_state
            .nodes
            .get(&PowerNodeId(2))
            .expect("critical node telemetry present");

        assert!((warn_node.stability.to_f32() - 0.7).abs() < 1e-6);
        assert!((critical_node.stability.to_f32() - 0.3).abs() < 1e-6);
        assert_eq!(critical_node.incident_count, 1);

        let _ = grid_state;

        let warn_component = app
            .world
            .entity(entities[1])
            .get::<PowerNode>()
            .expect("warn node component");
        let critical_component = app
            .world
            .entity(entities[2])
            .get::<PowerNode>()
            .expect("critical node component");

        assert_eq!(warn_component.incident_count, 0);
        assert_eq!(critical_component.incident_count, 1);
    }

    #[test]
    fn simulate_power_redistributes_surplus_along_topology() {
        let mut app = App::new();
        configure_simulation(&mut app, UVec2::new(2, 2));

        let specs = vec![
            NodeSpec::new(12.0, 4.0),
            NodeSpec::new(5.0, 10.0),
            NodeSpec::new(3.0, 6.0),
            NodeSpec::new(4.0, 4.0),
        ];

        let entities = spawn_power_nodes(&mut app.world, 2, 2, &specs);
        let topology = PowerTopology::from_grid(&entities, 2, 2, scalar_from_f32(4.0));
        app.insert_resource(topology);

        run_power_system(&mut app);

        let grid_state = app.world.resource::<PowerGridState>();
        let node_a = grid_state
            .nodes
            .get(&PowerNodeId(0))
            .expect("node A telemetry");
        let node_b = grid_state
            .nodes
            .get(&PowerNodeId(1))
            .expect("node B telemetry");
        let node_c = grid_state
            .nodes
            .get(&PowerNodeId(2))
            .expect("node C telemetry");

        assert!((node_a.surplus.to_f32() - 1.0).abs() < 1e-6);
        assert!((node_b.deficit.to_f32() - 1.0).abs() < 1e-6);
        assert!(node_c.deficit.to_f32().abs() < 1e-6);
        assert!(node_c.surplus.to_f32().abs() < 1e-6);
        assert_eq!(grid_state.instability_alerts, 0);
    }
}
