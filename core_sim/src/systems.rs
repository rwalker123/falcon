use std::cmp::max;

use bevy::{math::UVec2, prelude::*};

use crate::{
    components::{ElementKind, LogisticsLink, PopulationCohort, PowerNode, Tile},
    culture::{
        CultureEffectsCache, CultureManager, CultureSchismEvent, CultureTensionEvent,
        CultureTensionKind, CultureTensionRecord, CultureTraitAxis, CULTURE_TRAIT_AXES,
    },
    generations::GenerationRegistry,
    influencers::InfluencerImpacts,
    resources::{
        CorruptionExposureRecord, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
        SentimentAxisBias, SimulationConfig, SimulationTick, TileRegistry,
    },
    scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero, Scalar},
    terrain::{terrain_definition, terrain_for_position},
};

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
                        generation,
                        demand,
                        efficiency,
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
    let mut cohort_index = 0usize;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let tile_entity = tiles[y * width + x];
            commands.spawn(PopulationCohort {
                home: tile_entity,
                size: 1_000,
                morale: scalar_from_f32(0.6),
                generation: registry.assign_for_index(cohort_index),
            });
            cohort_index += 1;
        }
    }

    commands.insert_resource(TileRegistry {
        tiles,
        width: config.grid_size.x,
        height: config.grid_size.y,
    });

    culture.reconcile(&tick);
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
    mut links: Query<&mut LogisticsLink>,
    mut tiles: Query<&mut Tile>,
) {
    let flow_gain =
        (config.logistics_flow_gain * impacts.logistics_multiplier * effects.logistics_multiplier)
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
        let capacity = (link.capacity / penalty_scalar).max(scalar_from_f32(0.05));
        let gradient = source.mass - target.mass;
        let transfer_raw = (gradient * effective_gain).clamp(-capacity, capacity);
        let delivered = transfer_raw * (Scalar::one() - attrition_scalar);
        source.mass -= transfer_raw;
        target.mass += delivered;
        link.flow = delivered;
    }
}

/// Update population cohorts based on environmental conditions.
pub fn simulate_population(
    config: Res<SimulationConfig>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    tiles: Query<&Tile>,
    mut cohorts: Query<&mut PopulationCohort>,
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
    }
}

/// Adjust power nodes in response to tile state and demand.
pub fn simulate_power(
    mut nodes: Query<(&Tile, &mut PowerNode)>,
    config: Res<SimulationConfig>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
) {
    for (tile, mut node) in nodes.iter_mut() {
        let efficiency_adjust =
            (config.ambient_temperature - tile.temperature) * config.power_adjust_rate;
        node.efficiency = (node.efficiency + efficiency_adjust * scalar_from_f32(0.01))
            .clamp(scalar_from_f32(0.5), scalar_from_f32(1.5));
        let net = node.generation * node.efficiency - node.demand
            + impacts.power_bonus
            + effects.power_bonus;
        node.generation = (node.generation + net * scalar_from_f32(0.05))
            .clamp(scalar_zero(), config.max_power_generation);
        node.demand = (node.demand + (-net) * scalar_from_f32(0.03))
            .clamp(scalar_zero(), config.max_power_generation);
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
