use super::*;

#[derive(Event, Debug, Clone)]
pub struct TradeDiffusionEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
}

#[derive(SystemParam)]
pub struct LogisticsSimParams<'w, 's> {
    pub config: Res<'w, SimulationConfig>,
    pub impacts: Res<'w, InfluencerImpacts>,
    pub effects: Res<'w, CultureEffectsCache>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
    pub links: Query<'w, 's, (Entity, &'static mut LogisticsLink)>,
    pub tiles: Query<'w, 's, &'static mut Tile>,
}

#[derive(SystemParam)]
pub struct TradeDiffusionParams<'w, 's> {
    pub config: Res<'w, SimulationConfig>,
    pub telemetry: ResMut<'w, TradeTelemetry>,
    pub discovery: ResMut<'w, DiscoveryProgressLedger>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
    pub tick: Res<'w, SimulationTick>,
    pub events: EventWriter<'w, TradeDiffusionEvent>,
    pub links: Query<'w, 's, (&'static LogisticsLink, &'static mut TradeLink)>,
    pub tiles: Query<'w, 's, &'static Tile>,
    pub herd_density: Res<'w, HerdDensityMap>,
}

/// Relax material temperatures and adjust masses using deterministic rules. The relaxation target is
/// the tile's latitude + elevation + jitter climate temperature (recomputed deterministically from
/// its position/elevation/element), so the field converges to the climate model rather than the old
/// element checkerboard. Worldgen seeds each tile at exactly this value, so turn 1 has no jump.
pub fn simulate_materials(
    config: Res<SimulationConfig>,
    elevation: Res<ElevationField>,
    mut tiles: Query<&mut Tile>,
) {
    let grid_height = config.grid_size.y;
    for mut tile in tiles.iter_mut() {
        let above_sea = elevation.above_sea_normalized(tile.position.x, tile.position.y);
        let target = climate_temperature(
            tile.position.y,
            grid_height,
            above_sea,
            tile.element,
            &config.climate,
        );
        let delta = (target - tile.temperature) * config.temperature_lerp;
        let conductivity = tile.element.conductivity();
        tile.temperature += delta * conductivity;
        let flux = tile.element.mass_flux() * config.mass_flux_epsilon;
        let new_mass = tile.mass + flux;
        tile.mass = new_mass.clamp(config.mass_bounds.0, config.mass_bounds.1);
    }
}

/// Move resources along logistics links based on mass gradients.
pub fn simulate_logistics(mut params: LogisticsSimParams) {
    let logistics_cfg = params.pipeline_config.config().logistics();
    let corruption_cfg = params.severity_config.config().corruption();
    let corruption_factor = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Logistics,
        params.config.corruption_logistics_penalty,
        corruption_cfg,
    );
    let flow_gain = (params.config.logistics_flow_gain
        * params.impacts.logistics_multiplier
        * params.effects.logistics_multiplier
        * corruption_factor)
        .clamp(logistics_cfg.flow_gain_min(), logistics_cfg.flow_gain_max());
    let mut links: Vec<_> = params.links.iter_mut().collect();
    links.sort_by_key(|(entity, _)| entity.to_bits());
    for (_, mut link) in links {
        let Ok([mut source, mut target]) = params.tiles.get_many_mut([link.from, link.to]) else {
            link.flow = scalar_zero();
            continue;
        };
        let source_profile = terrain_definition(source.terrain);
        let target_profile = terrain_definition(target.terrain);
        let penalty_avg = (source_profile.logistics_penalty + target_profile.logistics_penalty)
            .max(logistics_cfg.penalty_min());
        let attrition_avg = (source_profile.attrition_rate + target_profile.attrition_rate)
            .clamp(0.0, logistics_cfg.attrition_max());
        let penalty_scalar = Scalar::from_f32(penalty_avg.max(logistics_cfg.penalty_scalar_min()));
        let attrition_scalar = Scalar::from_f32(attrition_avg);
        let effective_gain =
            (flow_gain / penalty_scalar).clamp(logistics_cfg.effective_gain_min(), flow_gain);
        let capacity = ((link.capacity * corruption_factor) / penalty_scalar)
            .max(logistics_cfg.capacity_min());
        let gradient = source.mass - target.mass;
        let transfer_raw = (gradient * effective_gain).clamp(-capacity, capacity);
        let delivered = transfer_raw * (Scalar::one() - attrition_scalar);
        source.mass -= transfer_raw;
        target.mass += delivered;
        link.flow = delivered;
    }
}

/// Diffuse knowledge along trade links using openness-derived leak timers.
pub fn trade_knowledge_diffusion(mut params: TradeDiffusionParams) {
    params.telemetry.reset_turn();
    let leak_curve = TradeLeakCurve::new(
        params.config.trade_leak_min_ticks,
        params.config.trade_leak_max_ticks,
        params.config.trade_leak_exponent,
    );
    let corruption_cfg = params.severity_config.config().corruption();
    let trade_multiplier = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Trade,
        params.config.corruption_trade_penalty,
        corruption_cfg,
    );
    let trade_cfg = params.pipeline_config.config().trade();
    let tariff_base = params.config.base_trade_tariff;

    for (logistics, mut trade) in params.links.iter_mut() {
        trade.throughput = logistics.flow * trade_multiplier;
        let tariff_max = tariff_base * trade_cfg.tariff_max_scalar();
        trade.tariff = (tariff_base * trade_multiplier).clamp(trade_cfg.tariff_min(), tariff_max);
        trade.openness = trade.openness.clamp(scalar_zero(), scalar_one());
        trade.openness = Scalar::from_raw(apply_openness_decay(
            trade.openness.raw(),
            trade.decay.raw(),
        ));

        if trade.leak_timer > 0 {
            trade.leak_timer = trade.leak_timer.saturating_sub(1);
        }

        let density_hint = match (
            params.tiles.get(logistics.from),
            params.tiles.get(logistics.to),
        ) {
            (Ok(from_tile), Ok(to_tile)) => params
                .herd_density
                .normalized_pair_average(from_tile.position, to_tile.position),
            _ => params.herd_density.normalized_average(),
        };

        if trade.leak_timer == 0 {
            let fragment = if !trade.pending_fragments.is_empty() {
                trade.pending_fragments.remove(0)
            } else {
                let discovery_id = trade
                    .last_discovery
                    .unwrap_or((trade.from_faction.0 << 8) | trade.to_faction.0);
                KnowledgeFragment::new(
                    discovery_id,
                    params.config.trade_leak_progress,
                    Scalar::one(),
                )
            };

            let delta = fragment.progress;
            if delta > scalar_zero() {
                let discovery_id = fragment.discovery_id;
                let density_multiplier =
                    scalar_from_f32(1.0 + density_hint * HERD_TRADE_DIFFUSION_BONUS);
                let adjusted_delta =
                    (delta * density_multiplier).clamp(Scalar::zero(), Scalar::one());
                let _ =
                    params
                        .discovery
                        .add_progress(trade.to_faction, discovery_id, adjusted_delta);
                params.telemetry.tech_diffusion_applied =
                    params.telemetry.tech_diffusion_applied.saturating_add(1);
                params.telemetry.push_record(TradeDiffusionRecord {
                    tick: params.tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta: adjusted_delta,
                    via_migration: false,
                    herd_density: density_hint,
                });
                params.events.send(TradeDiffusionEvent {
                    tick: params.tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta: adjusted_delta,
                    via_migration: false,
                });
                trade.last_discovery = Some(discovery_id);
            }

            trade.leak_timer = leak_curve.ticks_for_openness(trade.openness.raw());
            if trade.leak_timer == 0 {
                trade.leak_timer = params.config.trade_leak_min_ticks.max(1);
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
                        "herd_density": record.herd_density,
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
