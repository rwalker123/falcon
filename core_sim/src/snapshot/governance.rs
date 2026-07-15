use super::*;

pub(crate) fn crisis_metric_kind_to_schema(
    kind: InternalCrisisMetricKind,
) -> SchemaCrisisMetricKind {
    match kind {
        InternalCrisisMetricKind::R0 => SchemaCrisisMetricKind::R0,
        InternalCrisisMetricKind::GridStressPct => SchemaCrisisMetricKind::GridStressPct,
        InternalCrisisMetricKind::UnauthorizedQueuePct => {
            SchemaCrisisMetricKind::UnauthorizedQueuePct
        }
        InternalCrisisMetricKind::SwarmsActive => SchemaCrisisMetricKind::SwarmsActive,
        InternalCrisisMetricKind::PhageDensity => SchemaCrisisMetricKind::PhageDensity,
    }
}

pub(crate) fn crisis_severity_band_to_schema(
    band: InternalCrisisSeverityBand,
) -> SchemaCrisisSeverityBand {
    match band {
        InternalCrisisSeverityBand::Safe => SchemaCrisisSeverityBand::Safe,
        InternalCrisisSeverityBand::Warn => SchemaCrisisSeverityBand::Warn,
        InternalCrisisSeverityBand::Critical => SchemaCrisisSeverityBand::Critical,
    }
}

pub(crate) fn crisis_history_to_schema(
    history: &[InternalCrisisTrendSample],
) -> Vec<SchemaCrisisTrendSample> {
    history
        .iter()
        .map(|sample| SchemaCrisisTrendSample {
            tick: sample.tick,
            value: sample.value,
        })
        .collect()
}

pub(crate) fn crisis_telemetry_state_from_metrics(
    snapshot: &InternalCrisisMetricsSnapshot,
) -> CrisisTelemetryState {
    let gauges = snapshot
        .gauges
        .iter()
        .map(|gauge| CrisisGaugeState {
            kind: crisis_metric_kind_to_schema(gauge.kind),
            raw: gauge.raw,
            ema: gauge.ema,
            trend_5t: gauge.trend_5t,
            warn_threshold: gauge.warn_threshold,
            critical_threshold: gauge.critical_threshold,
            last_updated_tick: gauge.last_updated_tick,
            stale_ticks: gauge.stale_ticks,
            band: crisis_severity_band_to_schema(gauge.band),
            history: crisis_history_to_schema(&gauge.history),
        })
        .collect();

    CrisisTelemetryState {
        gauges,
        modifiers_active: snapshot.modifiers_active,
        foreshock_incidents: snapshot.foreshock_incidents,
        containment_incidents: snapshot.containment_incidents,
        warnings_active: snapshot.warnings_active,
        criticals_active: snapshot.criticals_active,
    }
}

pub(crate) const CORRUPTION_SUBSYSTEM_COUNT: usize = 4;

pub(crate) struct CorruptionSignals<'a> {
    pub(crate) ledger: &'a CorruptionLedger,
    pub(crate) telemetry: &'a CorruptionTelemetry,
}

pub(crate) struct CorruptionRasterInputs<'a> {
    pub(crate) tiles: &'a [TileState],
    pub(crate) trade_links: &'a [TradeLinkState],
    pub(crate) populations: &'a [PopulationCohortState],
    pub(crate) power_nodes: &'a [PowerNodeState],
    pub(crate) logistics_raster: &'a ScalarRasterState,
    pub(crate) corruption_signals: CorruptionSignals<'a>,
    pub(crate) grid_size: UVec2,
    pub(crate) overlays: &'a SnapshotOverlaysConfig,
}

pub(crate) fn corruption_raster_from_simulation(
    inputs: CorruptionRasterInputs<'_>,
) -> ScalarRasterState {
    let CorruptionRasterInputs {
        tiles,
        trade_links,
        populations,
        power_nodes,
        logistics_raster,
        corruption_signals,
        grid_size,
        overlays,
    } = inputs;
    let CorruptionSignals { ledger, telemetry } = corruption_signals;
    let overlay_cfg = overlays.corruption();
    let mut width = logistics_raster.width.max(grid_size.x).max(1);
    let mut height = logistics_raster.height.max(grid_size.y).max(1);

    for tile in tiles {
        width = width.max(tile.x.saturating_add(1));
        height = height.max(tile.y.saturating_add(1));
    }

    let width_usize = width as usize;
    let height_usize = height as usize;
    let total = width_usize.saturating_mul(height_usize).max(1);

    let mut samples = vec![0i64; total];

    let mut tile_indices = HashMap::with_capacity(tiles.len());
    for tile in tiles {
        if tile.x < width && tile.y < height {
            let idx = (tile.y as usize) * width_usize + tile.x as usize;
            tile_indices.insert(tile.entity, idx);
        }
    }

    let mut logistics_weights = vec![0i64; total];
    if logistics_raster.width > 0
        && logistics_raster.height > 0
        && !logistics_raster.samples.is_empty()
    {
        let src_width = logistics_raster.width as usize;
        let src_height = logistics_raster.height as usize;
        let min_height = src_height.min(height_usize);
        let min_width = src_width.min(width_usize);
        for y in 0..min_height {
            let src_row = y * src_width;
            let dst_row = y * width_usize;
            for x in 0..min_width {
                let src_idx = src_row + x;
                let dst_idx = dst_row + x;
                if src_idx < logistics_raster.samples.len() && dst_idx < logistics_weights.len() {
                    logistics_weights[dst_idx] = logistics_raster.samples[src_idx].abs();
                }
            }
        }
    }

    let mut trade_weights = vec![0i64; total];
    for link in trade_links {
        let throughput = link.throughput.abs();
        if throughput <= 0 {
            continue;
        }
        for tile_id in [link.from_tile, link.to_tile] {
            if let Some(&idx) = tile_indices.get(&tile_id) {
                trade_weights[idx] = trade_weights[idx].saturating_add(throughput);
            }
        }
    }

    let mut military_weights = vec![0i64; total];
    for node in power_nodes {
        if let Some(&idx) = tile_indices.get(&node.entity) {
            let generation = node.generation.abs();
            let demand = node.demand.abs();
            let weight = generation.saturating_add(demand);
            if weight > 0 {
                military_weights[idx] = military_weights[idx].saturating_add(weight);
            }
        }
    }

    let mut governance_weights = vec![0i64; total];
    let scale_i128 = i128::from(Scalar::SCALE);
    for cohort in populations {
        if let Some(&idx) = tile_indices.get(&cohort.home) {
            let size = i64::from(cohort.size);
            if size <= 0 {
                continue;
            }
            let morale = Scalar::from_raw(cohort.morale).clamp(Scalar::zero(), Scalar::one());
            let morale_deficit = (Scalar::one() - morale).raw().max(0);
            let mut weighted =
                (i128::from(size) * (scale_i128 + i128::from(morale_deficit))) / scale_i128;
            if weighted > i128::from(i64::MAX) {
                weighted = i128::from(i64::MAX);
            }
            governance_weights[idx] = governance_weights[idx].saturating_add(weighted as i64);
        }
    }

    let mut subsystem_totals = [0i64; CORRUPTION_SUBSYSTEM_COUNT];
    for entry in &ledger.entries {
        let idx = entry.subsystem as usize;
        if idx >= subsystem_totals.len() {
            continue;
        }
        if entry.intensity > 0 {
            subsystem_totals[idx] = subsystem_totals[idx].saturating_add(entry.intensity);
        }
    }

    let mut subsystem_spikes = [0i64; CORRUPTION_SUBSYSTEM_COUNT];
    for record in telemetry.exposures_this_turn.iter() {
        let idx = record.subsystem as usize;
        if idx >= subsystem_spikes.len() {
            continue;
        }
        if record.intensity > 0 {
            subsystem_spikes[idx] = subsystem_spikes[idx].saturating_add(record.intensity);
        }
    }

    let logistics_idx = CorruptionSubsystem::Logistics as usize;
    let trade_idx = CorruptionSubsystem::Trade as usize;
    let military_idx = CorruptionSubsystem::Military as usize;
    let governance_idx = CorruptionSubsystem::Governance as usize;

    let logistic_intensity = subsystem_totals[logistics_idx].saturating_add(scale_spike(
        subsystem_spikes[logistics_idx],
        overlay_cfg.logistics_spike_multiplier(),
    ));
    distribute_intensity(&mut samples, &logistics_weights, logistic_intensity);

    let trade_intensity = subsystem_totals[trade_idx].saturating_add(scale_spike(
        subsystem_spikes[trade_idx],
        overlay_cfg.trade_spike_multiplier(),
    ));
    distribute_intensity(&mut samples, &trade_weights, trade_intensity);

    let military_intensity = subsystem_totals[military_idx].saturating_add(scale_spike(
        subsystem_spikes[military_idx],
        overlay_cfg.military_spike_multiplier(),
    ));
    distribute_intensity(&mut samples, &military_weights, military_intensity);

    let governance_intensity = subsystem_totals[governance_idx].saturating_add(scale_spike(
        subsystem_spikes[governance_idx],
        overlay_cfg.governance_spike_multiplier(),
    ));
    distribute_intensity(&mut samples, &governance_weights, governance_intensity);

    let logistic_norm = normalize_weights_to_scalar(&logistics_weights);
    let trade_norm = normalize_weights_to_scalar(&trade_weights);
    let military_norm = normalize_weights_to_scalar(&military_weights);
    let governance_norm = normalize_weights_to_scalar(&governance_weights);

    let logistic_weight = overlay_cfg.logistics_weight();
    let trade_weight = overlay_cfg.trade_weight();
    let military_weight = overlay_cfg.military_weight();
    let governance_weight = overlay_cfg.governance_weight();

    for (idx, sample) in samples.iter_mut().enumerate() {
        let mut baseline = Scalar::zero();
        baseline += logistic_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * logistic_weight;
        baseline += trade_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * trade_weight;
        baseline += military_norm.get(idx).copied().unwrap_or_else(Scalar::zero) * military_weight;
        baseline += governance_norm
            .get(idx)
            .copied()
            .unwrap_or_else(Scalar::zero)
            * governance_weight;
        if baseline.raw() != 0 {
            *sample = sample.saturating_add(baseline.raw());
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

pub(crate) fn normalize_weights_to_scalar(weights: &[i64]) -> Vec<Scalar> {
    if weights.is_empty() {
        return Vec::new();
    }
    let max_weight = weights.iter().copied().max().unwrap_or(0);
    if max_weight <= 0 {
        return vec![Scalar::zero(); weights.len()];
    }
    let max_value = i128::from(max_weight);
    weights
        .iter()
        .map(|&weight| {
            if weight <= 0 {
                Scalar::zero()
            } else {
                let mut ratio = (i128::from(weight) * i128::from(Scalar::SCALE)) / max_value;
                if ratio > i128::from(Scalar::SCALE) {
                    ratio = i128::from(Scalar::SCALE);
                }
                if ratio < 0 {
                    ratio = 0;
                }
                Scalar::from_raw(ratio as i64)
            }
        })
        .collect()
}

pub(crate) fn scale_spike(value: i64, multiplier: f32) -> i64 {
    if value == 0 {
        return 0;
    }
    if multiplier == 1.0 {
        return value;
    }
    if multiplier == 0.0 {
        return 0;
    }
    let scaled = (value as f64) * (multiplier as f64);
    if scaled.is_nan() || scaled == 0.0 {
        return 0;
    }
    if scaled > i64::MAX as f64 {
        i64::MAX
    } else if scaled < i64::MIN as f64 {
        i64::MIN
    } else {
        scaled.round() as i64
    }
}

pub(crate) fn distribute_intensity(samples: &mut [i64], weights: &[i64], intensity_raw: i64) {
    if intensity_raw <= 0 || samples.is_empty() || samples.len() != weights.len() {
        return;
    }

    let total_weight: i128 = weights
        .iter()
        .map(|&w| i128::from(if w > 0 { w } else { 0 }))
        .sum();

    if total_weight == 0 {
        let len = samples.len() as i64;
        if len <= 0 {
            return;
        }
        let base_share = intensity_raw / len;
        for sample in samples.iter_mut() {
            *sample = sample.saturating_add(base_share);
        }
        let remainder = intensity_raw - base_share.saturating_mul(len);
        if remainder != 0 {
            samples[0] = samples[0].saturating_add(remainder);
        }
        return;
    }

    let intensity = i128::from(intensity_raw);
    let mut allocated = 0i128;

    for (sample, &weight) in samples.iter_mut().zip(weights.iter()) {
        if weight <= 0 {
            continue;
        }
        let share = (intensity * i128::from(weight)) / total_weight;
        if share == 0 {
            continue;
        }
        allocated += share;
        let share_i64 = if share > i128::from(i64::MAX) {
            i64::MAX
        } else if share < i128::from(i64::MIN) {
            i64::MIN
        } else {
            share as i64
        };
        *sample = sample.saturating_add(share_i64);
    }

    let remainder = intensity - allocated;
    if remainder != 0 {
        if let Some((idx, _)) = weights.iter().enumerate().max_by_key(|(_, &w)| w) {
            if let Some(sample) = samples.get_mut(idx) {
                let remainder_i64 = if remainder > i128::from(i64::MAX) {
                    i64::MAX
                } else if remainder < i128::from(i64::MIN) {
                    i64::MIN
                } else {
                    remainder as i64
                };
                *sample = sample.saturating_add(remainder_i64);
            }
        }
    }
}

pub(crate) fn power_state(entity: Entity, node: &PowerNode) -> PowerNodeState {
    PowerNodeState {
        entity: entity.to_bits(),
        node_id: node.id.0,
        generation: node.generation.raw(),
        demand: node.demand.raw(),
        efficiency: node.efficiency.raw(),
        storage_level: node.storage_level.raw(),
        storage_capacity: node.storage_capacity.raw(),
        stability: node.stability.raw(),
        surplus: node.surplus.raw(),
        deficit: node.deficit.raw(),
        incident_count: node.incident_count,
    }
}

pub(crate) fn power_metrics_from_grid(grid: &PowerGridState) -> PowerTelemetryState {
    let incidents: Vec<PowerIncidentState> = grid
        .incidents
        .iter()
        .map(|incident| PowerIncidentState {
            node_id: incident.node_id.0,
            severity: match incident.severity {
                GridIncidentSeverity::Warning => PowerIncidentSeverity::Warning,
                GridIncidentSeverity::Critical => PowerIncidentSeverity::Critical,
            },
            deficit: incident.deficit.raw(),
        })
        .collect();

    PowerTelemetryState {
        total_supply: grid.total_supply.raw(),
        total_demand: grid.total_demand.raw(),
        total_storage: grid.total_storage.raw(),
        total_capacity: grid.total_capacity.raw(),
        grid_stress_avg: grid.grid_stress_avg,
        surplus_margin: grid.surplus_margin,
        instability_alerts: grid.instability_alerts,
        incidents,
    }
}
