use std::{cmp::max, collections::HashMap};

use bevy::{math::UVec2, prelude::*};
use log::debug;
use serde_json::json;

use crate::{
    components::{
        fragments_from_contract, fragments_to_contract, ElementKind, KnowledgeFragment,
        LogisticsLink, PendingMigration, PopulationCohort, PowerNode, Tile, TradeLink,
    },
    culture::{
        CultureEffectsCache, CultureManager, CultureSchismEvent, CultureTensionEvent,
        CultureTensionKind, CultureTensionRecord, CultureTraitAxis, CULTURE_TRAIT_AXES,
    },
    generations::GenerationRegistry,
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    orders::{FactionId, FactionRegistry},
    power::{
        PowerGridNodeTelemetry, PowerGridState, PowerIncident, PowerIncidentSeverity, PowerNodeId,
        PowerTopology,
    },
    resources::{
        CorruptionExposureRecord, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
        DiscoveryProgressLedger, SentimentAxisBias, SimulationConfig, SimulationTick, TileRegistry,
        TradeDiffusionRecord, TradeTelemetry,
    },
    scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero, Scalar},
    terrain::{terrain_definition, terrain_for_position},
};
use sim_runtime::{
    apply_openness_decay, merge_fragment_payload, scale_migration_fragments, CorruptionSubsystem,
    TradeLeakCurve,
};

#[derive(Event, Debug, Clone)]
pub struct TradeDiffusionEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
}

#[derive(Event, Debug, Clone)]
pub struct MigrationKnowledgeEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
}

fn corruption_multiplier(
    ledgers: &CorruptionLedgers,
    subsystem: CorruptionSubsystem,
    penalty: Scalar,
) -> Scalar {
    let raw_intensity = ledgers.total_intensity(subsystem).max(0);
    if raw_intensity == 0 {
        return Scalar::one();
    }
    let intensity = Scalar::from_raw(raw_intensity).clamp(Scalar::zero(), Scalar::one());
    let mut reduction = intensity * penalty;
    reduction = reduction.clamp(Scalar::zero(), scalar_from_f32(0.9));
    (Scalar::one() - reduction).clamp(scalar_from_f32(0.1), Scalar::one())
}

/// Spawn initial grid of tiles, logistics links, power nodes, and population cohorts.
pub fn spawn_initial_world(
    mut commands: Commands,
    config: Res<SimulationConfig>,
    registry: Res<GenerationRegistry>,
    tick: Res<SimulationTick>,
    mut culture: ResMut<CultureManager>,
) {
    let width = config.grid_size.x as usize;
    let height = config.grid_size.y as usize;
    let mut tiles = Vec::with_capacity(width * height);

    let _global_id = culture.ensure_global();
    let regional_id = culture.upsert_regional(0);
    if let Some(region_layer) = culture.regional_layer_mut_by_region(0) {
        let modifiers = region_layer.traits.modifier_mut();
        modifiers[CultureTraitAxis::OpenClosed.index()] = scalar_from_f32(0.12);
        modifiers[CultureTraitAxis::TraditionalistRevisionist.index()] = scalar_from_f32(-0.08);
        modifiers[CultureTraitAxis::ExpansionistInsular.index()] = scalar_from_f32(0.15);
        modifiers[CultureTraitAxis::SecularDevout.index()] = scalar_from_f32(0.05);
    }

    for y in 0..height {
        for x in 0..width {
            let position = UVec2::new(x as u32, y as u32);
            let element = ElementKind::from_grid(position);
            let (terrain, terrain_tags) = terrain_for_position(position, config.grid_size);
            let (generation, demand, efficiency) = element.power_profile();
            let base_mass = scalar_from_f32(1.0 + ((x + y) % 5) as f32 * 0.35);
            let node_id = PowerNodeId(y as u32 * config.grid_size.x + x as u32);
            let storage_capacity = (generation * scalar_from_f32(0.6) + scalar_from_f32(2.0))
                .clamp(scalar_from_f32(1.0), scalar_from_f32(40.0));
            let storage_level =
                (storage_capacity * scalar_from_f32(0.5)).clamp(scalar_zero(), storage_capacity);
            let tile_entity = commands
                .spawn((
                    Tile {
                        position,
                        element,
                        mass: base_mass,
                        temperature: config.ambient_temperature + element.thermal_bias(),
                        terrain,
                        terrain_tags,
                    },
                    PowerNode {
                        id: node_id,
                        base_generation: generation,
                        base_demand: demand,
                        generation,
                        demand,
                        efficiency,
                        storage_capacity,
                        storage_level,
                        stability: scalar_from_f32(0.85),
                        surplus: scalar_zero(),
                        deficit: scalar_zero(),
                        incident_count: 0,
                    },
                ))
                .id();
            tiles.push(tile_entity);

            culture.attach_local(tile_entity, regional_id);
            let modifiers = seeded_modifiers_for_position(position);
            culture.apply_initial_modifiers(tile_entity, modifiers);
        }
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let from_entity = tiles[idx];
            if x + 1 < width {
                let to_entity = tiles[y * width + (x + 1)];
                commands
                    .spawn(LogisticsLink {
                        from: from_entity,
                        to: to_entity,
                        capacity: config.base_link_capacity,
                        flow: scalar_zero(),
                    })
                    .insert(TradeLink {
                        from_faction: FactionId(0),
                        to_faction: FactionId(0),
                        throughput: scalar_zero(),
                        tariff: config.base_trade_tariff,
                        openness: config.base_trade_openness,
                        decay: config.trade_openness_decay,
                        leak_timer: config.trade_leak_max_ticks,
                        last_discovery: None,
                        pending_fragments: Vec::new(),
                    });
            }
            if y + 1 < height {
                let to_entity = tiles[(y + 1) * width + x];
                commands
                    .spawn(LogisticsLink {
                        from: from_entity,
                        to: to_entity,
                        capacity: config.base_link_capacity,
                        flow: scalar_zero(),
                    })
                    .insert(TradeLink {
                        from_faction: FactionId(0),
                        to_faction: FactionId(0),
                        throughput: scalar_zero(),
                        tariff: config.base_trade_tariff,
                        openness: config.base_trade_openness,
                        decay: config.trade_openness_decay,
                        leak_timer: config.trade_leak_max_ticks,
                        last_discovery: None,
                        pending_fragments: Vec::new(),
                    });
            }
        }
    }

    let stride = max(1, config.population_cluster_stride) as usize;
    let mut cohort_index = 0usize;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let tile_entity = tiles[y * width + x];
            commands.spawn(PopulationCohort {
                home: tile_entity,
                size: 1_000,
                morale: scalar_from_f32(0.6),
                generation: registry.assign_for_index(cohort_index),
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            });
            cohort_index += 1;
        }
    }

    let topology = PowerTopology::from_grid(
        &tiles,
        config.grid_size.x,
        config.grid_size.y,
        config.power_line_capacity,
    );
    commands.insert_resource(topology);

    commands.insert_resource(TileRegistry {
        tiles,
        width: config.grid_size.x,
        height: config.grid_size.y,
    });

    culture.reconcile(&tick, &InfluencerCultureResonance::default());
    let _ = culture.take_tension_events();
}

fn seeded_modifiers_for_position(position: UVec2) -> [Scalar; CULTURE_TRAIT_AXES] {
    let mut modifiers = [Scalar::zero(); CULTURE_TRAIT_AXES];
    let seed = position.x as i32 * 31 + position.y as i32 * 17;
    for (idx, slot) in modifiers.iter_mut().enumerate() {
        let wave = (((seed + idx as i32 * 13) % 23) - 11) as f32;
        let scaled = (wave / 23.0).clamp(-1.0, 1.0) * 0.2;
        *slot = scalar_from_f32(scaled);
    }
    modifiers
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
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    ledgers: Res<CorruptionLedgers>,
    mut links: Query<&mut LogisticsLink>,
    mut tiles: Query<&mut Tile>,
) {
    let corruption_factor = corruption_multiplier(
        &ledgers,
        CorruptionSubsystem::Logistics,
        config.corruption_logistics_penalty,
    );
    let flow_gain = (config.logistics_flow_gain
        * impacts.logistics_multiplier
        * effects.logistics_multiplier
        * corruption_factor)
        .clamp(scalar_from_f32(0.02), scalar_from_f32(0.5));
    for mut link in links.iter_mut() {
        let Ok([mut source, mut target]) = tiles.get_many_mut([link.from, link.to]) else {
            link.flow = scalar_zero();
            continue;
        };
        let source_profile = terrain_definition(source.terrain);
        let target_profile = terrain_definition(target.terrain);
        let penalty_avg =
            (source_profile.logistics_penalty + target_profile.logistics_penalty).max(0.05);
        let attrition_avg =
            (source_profile.attrition_rate + target_profile.attrition_rate).clamp(0.0, 0.95);
        let penalty_scalar = Scalar::from_f32(penalty_avg.max(0.1));
        let attrition_scalar = Scalar::from_f32(attrition_avg);
        let effective_gain = (flow_gain / penalty_scalar).clamp(scalar_from_f32(0.005), flow_gain);
        let capacity =
            ((link.capacity * corruption_factor) / penalty_scalar).max(scalar_from_f32(0.05));
        let gradient = source.mass - target.mass;
        let transfer_raw = (gradient * effective_gain).clamp(-capacity, capacity);
        let delivered = transfer_raw * (Scalar::one() - attrition_scalar);
        source.mass -= transfer_raw;
        target.mass += delivered;
        link.flow = delivered;
    }
}

/// Diffuse knowledge along trade links using openness-derived leak timers.
pub fn trade_knowledge_diffusion(
    config: Res<SimulationConfig>,
    mut telemetry: ResMut<TradeTelemetry>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    ledgers: Res<CorruptionLedgers>,
    tick: Res<SimulationTick>,
    mut events: EventWriter<TradeDiffusionEvent>,
    mut links: Query<(&LogisticsLink, &mut TradeLink)>,
) {
    telemetry.reset_turn();
    let leak_curve = TradeLeakCurve::new(
        config.trade_leak_min_ticks,
        config.trade_leak_max_ticks,
        config.trade_leak_exponent,
    );
    let trade_multiplier = corruption_multiplier(
        &ledgers,
        CorruptionSubsystem::Trade,
        config.corruption_trade_penalty,
    );
    let tariff_base = config.base_trade_tariff;

    for (logistics, mut trade) in links.iter_mut() {
        trade.throughput = logistics.flow * trade_multiplier;
        trade.tariff = (tariff_base * trade_multiplier).clamp(scalar_zero(), tariff_base);
        trade.openness = trade.openness.clamp(scalar_zero(), scalar_one());
        trade.openness = Scalar::from_raw(apply_openness_decay(
            trade.openness.raw(),
            trade.decay.raw(),
        ));

        if trade.leak_timer > 0 {
            trade.leak_timer = trade.leak_timer.saturating_sub(1);
        }

        if trade.leak_timer == 0 {
            let fragment = if !trade.pending_fragments.is_empty() {
                trade.pending_fragments.remove(0)
            } else {
                let discovery_id = trade
                    .last_discovery
                    .unwrap_or((trade.from_faction.0 << 8) | trade.to_faction.0);
                KnowledgeFragment::new(discovery_id, config.trade_leak_progress, Scalar::one())
            };

            let delta = fragment.progress;
            if delta > scalar_zero() {
                let discovery_id = fragment.discovery_id;
                let _ = discovery.add_progress(trade.to_faction, discovery_id, delta);
                telemetry.tech_diffusion_applied =
                    telemetry.tech_diffusion_applied.saturating_add(1);
                telemetry.push_record(TradeDiffusionRecord {
                    tick: tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta,
                    via_migration: false,
                });
                events.send(TradeDiffusionEvent {
                    tick: tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta,
                    via_migration: false,
                });
                trade.last_discovery = Some(discovery_id);
            }

            trade.leak_timer = leak_curve.ticks_for_openness(trade.openness.raw());
            if trade.leak_timer == 0 {
                trade.leak_timer = config.trade_leak_min_ticks.max(1);
            }
        }
    }
}

/// Publish trade telemetry counters for downstream logging/metrics.
pub fn publish_trade_telemetry(telemetry: Res<TradeTelemetry>, tick: Res<SimulationTick>) {
    let snapshot = json!({
        "tick": tick.0,
        "tech_diffusion_applied": telemetry.tech_diffusion_applied,
        "migration_transfers": telemetry.migration_transfers,
        "records": telemetry
            .records
            .iter()
            .take(24)
            .map(|record| {
                json!({
                    "from": record.from.0,
                    "to": record.to.0,
                    "discovery": record.discovery_id,
                    "delta": record.delta.to_f32(),
                    "via_migration": record.via_migration,
                })
            })
            .collect::<Vec<_>>(),
        "records_truncated": telemetry.records.len().saturating_sub(24),
    });

    match serde_json::to_string(&snapshot) {
        Ok(payload) => debug!("trade.telemetry {}", payload),
        Err(_) => debug!(
            "trade.telemetry tick={} trade.tech_diffusion_applied={} trade.migration_transfers={} records={}",
            tick.0,
            telemetry.tech_diffusion_applied,
            telemetry.migration_transfers,
            telemetry.records.len()
        ),
    }
}

/// Update population cohorts based on environmental conditions.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn simulate_population(
    config: Res<SimulationConfig>,
    registry: Res<FactionRegistry>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    tiles: Query<&Tile>,
    mut cohorts: Query<&mut PopulationCohort>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut telemetry: ResMut<TradeTelemetry>,
    mut trade_events: EventWriter<TradeDiffusionEvent>,
    mut migration_events: EventWriter<MigrationKnowledgeEvent>,
    tick: Res<SimulationTick>,
) {
    let max_cap_scalar = scalar_from_u32(config.population_cap);
    for mut cohort in cohorts.iter_mut() {
        let Ok(tile) = tiles.get(cohort.home) else {
            cohort.morale = scalar_zero();
            continue;
        };
        let terrain_profile = terrain_definition(tile.terrain);
        let temp_diff = (tile.temperature - config.ambient_temperature).abs();
        let terrain_attrition_penalty = Scalar::from_f32(terrain_profile.attrition_rate * 0.2);
        let terrain_hardness_penalty =
            Scalar::from_f32((terrain_profile.logistics_penalty - 1.0).max(0.0) * 0.05);
        let morale_delta = config.population_growth_rate
            - temp_diff * config.temperature_morale_penalty
            + impacts.morale_delta
            + effects.morale_bias
            - terrain_attrition_penalty
            - terrain_hardness_penalty;
        cohort.morale = (cohort.morale + morale_delta).clamp(scalar_zero(), scalar_one());

        let growth_base = config.population_growth_rate - temp_diff * scalar_from_f32(0.0005)
            + impacts.morale_delta * scalar_from_f32(0.4)
            + effects.morale_bias * scalar_from_f32(0.5)
            - terrain_attrition_penalty * scalar_from_f32(0.5);
        let growth_factor = growth_base.clamp(scalar_from_f32(-0.06), scalar_from_f32(0.06));
        let current_size = scalar_from_u32(cohort.size);
        let new_size =
            (current_size * (scalar_one() + growth_factor)).clamp(scalar_zero(), max_cap_scalar);
        cohort.size = new_size.to_u32();

        if cohort.migration.is_none()
            && cohort.morale > scalar_from_f32(0.78)
            && !cohort.knowledge.is_empty()
        {
            if let Some(&destination) = registry
                .factions
                .iter()
                .find(|&&faction| faction != cohort.faction)
            {
                let source_contract = fragments_to_contract(&cohort.knowledge);
                let scaled = scale_migration_fragments(
                    &source_contract,
                    config.migration_fragment_scaling.raw(),
                    config.migration_fidelity_floor.raw(),
                );
                if !scaled.is_empty() {
                    cohort.migration = Some(PendingMigration {
                        destination,
                        eta: 2,
                        fragments: fragments_from_contract(&scaled),
                    });
                }
            }
        }

        if let Some(mut migration) = cohort.migration.take() {
            if migration.eta > 0 {
                migration.eta -= 1;
            }

            if migration.eta == 0 {
                let source_faction = cohort.faction;
                for fragment in &migration.fragments {
                    if fragment.progress <= scalar_zero() {
                        continue;
                    }
                    let delta = fragment.progress;
                    discovery.add_progress(migration.destination, fragment.discovery_id, delta);
                    telemetry.tech_diffusion_applied =
                        telemetry.tech_diffusion_applied.saturating_add(1);
                    telemetry.migration_transfers = telemetry.migration_transfers.saturating_add(1);
                    telemetry.push_record(TradeDiffusionRecord {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                    });
                    trade_events.send(TradeDiffusionEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                    });
                    migration_events.send(MigrationKnowledgeEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                    });
                }

                let payload_contract = fragments_to_contract(&migration.fragments);
                let mut knowledge_contract = fragments_to_contract(&cohort.knowledge);
                merge_fragment_payload(
                    &mut knowledge_contract,
                    &payload_contract,
                    Scalar::one().raw(),
                );
                cohort.knowledge = fragments_from_contract(&knowledge_contract);
                cohort.faction = migration.destination;
            } else {
                cohort.migration = Some(migration);
            }
        }
    }
}

/// Adjust power nodes in response to tile state and demand.
pub fn simulate_power(
    mut nodes: Query<(Entity, &Tile, &mut PowerNode)>,
    config: Res<SimulationConfig>,
    topology: Res<PowerTopology>,
    mut grid_state: ResMut<PowerGridState>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    ledgers: Res<CorruptionLedgers>,
) {
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

    let corruption_factor = corruption_multiplier(
        &ledgers,
        CorruptionSubsystem::Military,
        config.corruption_military_penalty,
    );

    let mut node_calcs: Vec<NodeCalc> = Vec::with_capacity(nodes.iter().len());

    for (entity, tile, mut node) in nodes.iter_mut() {
        let efficiency_adjust =
            (config.ambient_temperature - tile.temperature) * config.power_adjust_rate;
        node.efficiency = (node.efficiency + efficiency_adjust * scalar_from_f32(0.01))
            .clamp(scalar_from_f32(0.5), config.max_power_efficiency);

        let influence_bonus = (impacts.power_bonus + effects.power_bonus)
            .clamp(
                scalar_from_f32(config.min_power_influence),
                scalar_from_f32(config.max_power_influence),
            );

        let effective_generation = (node.base_generation * node.efficiency + influence_bonus)
            .clamp(scalar_zero(), config.max_power_generation);
        let target_demand = (node.base_demand - influence_bonus * scalar_from_f32(0.25))
            .clamp(scalar_zero(), config.max_power_generation);
        let net = (effective_generation - target_demand) * corruption_factor;

        node.generation = (node.base_generation + net * scalar_from_f32(config.power_generation_adjust_rate))
            .clamp(scalar_zero(), config.max_power_generation);
        node.demand = (node.base_demand - net * scalar_from_f32(config.power_demand_adjust_rate))
            .clamp(scalar_zero(), config.max_power_generation);

        let net_supply = node.generation - node.demand;

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
                let n_idx = neighbour.index();
                if n_idx >= node_count {
                    continue;
                }
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

    let storage_efficiency = config
        .power_storage_efficiency
        .clamp(scalar_from_f32(0.1), scalar_from_f32(1.0));
    let storage_bleed = config
        .power_storage_bleed
        .clamp(scalar_zero(), scalar_from_f32(0.25));

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
            stability = (stability + reserve_ratio * scalar_from_f32(0.25))
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

    for (entity, _tile, mut node) in nodes.iter_mut() {
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
) {
    const TRUST_AXIS: usize = 1;

    for event in tension_events.read() {
        let severity = event.magnitude.to_f32().abs().clamp(0.0, 3.0);
        match event.kind {
            CultureTensionKind::DriftWarning => {
                let delta = scalar_from_f32((severity * 0.02).clamp(0.0, 0.08));
                sentiment.apply_incident_delta(TRUST_AXIS, -delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::AssimilationPush => {
                let delta = scalar_from_f32((severity * 0.01).clamp(0.0, 0.05));
                sentiment.apply_incident_delta(TRUST_AXIS, delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::SchismRisk => {
                // Handled in the dedicated schism pass below.
            }
        }
    }

    for event in schism_events.read() {
        let severity = event.magnitude.to_f32().abs().clamp(0.5, 4.0);
        let delta = scalar_from_f32((severity * 0.03).clamp(0.05, 0.15));
        sentiment.apply_incident_delta(TRUST_AXIS, -delta);
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
    tick: Res<SimulationTick>,
) {
    telemetry.reset_turn();

    let ledger = ledgers.ledger_mut();
    let mut resolved: Vec<u64> = Vec::new();

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

            let trust_idx = 1;
            let delta = Scalar::from_raw(entry.intensity)
                .clamp(Scalar::from_f32(-0.5), Scalar::from_f32(0.5));
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
