use super::*;

pub(crate) fn axis_bias_state_from_resource(bias: &SentimentAxisBias) -> AxisBiasState {
    let raw = bias.as_raw();
    AxisBiasState {
        knowledge: raw[0],
        trust: raw[1],
        equity: raw[2],
        agency: raw[3],
    }
}

pub(crate) fn influencer_label(state: &InfluentialIndividualState) -> String {
    if let Some(channel) = dominant_channel_label(state) {
        format!("Influencer {} ({})", state.name, channel)
    } else {
        format!("Influencer {}", state.name)
    }
}

pub(crate) fn dominant_channel_label(state: &InfluentialIndividualState) -> Option<&'static str> {
    let weights = [
        Scalar::from_raw(state.weight_popular),
        Scalar::from_raw(state.weight_peer),
        Scalar::from_raw(state.weight_institutional),
        Scalar::from_raw(state.weight_humanitarian),
    ];
    let supports = [
        Scalar::from_raw(state.support_popular),
        Scalar::from_raw(state.support_peer),
        Scalar::from_raw(state.support_institutional),
        Scalar::from_raw(state.support_humanitarian),
    ];
    let mut best_score = Scalar::zero();
    let mut best_idx: Option<usize> = None;
    for idx in 0..CHANNEL_LABELS.len() {
        let score = weights[idx] * supports[idx];
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| CHANNEL_LABELS[idx])
}

pub(crate) fn influencer_driver_weight(state: &InfluentialIndividualState) -> i64 {
    let weights = [
        Scalar::from_raw(state.weight_popular),
        Scalar::from_raw(state.weight_peer),
        Scalar::from_raw(state.weight_institutional),
        Scalar::from_raw(state.weight_humanitarian),
    ];
    let supports = [
        Scalar::from_raw(state.support_popular),
        Scalar::from_raw(state.support_peer),
        Scalar::from_raw(state.support_institutional),
        Scalar::from_raw(state.support_humanitarian),
    ];
    let mut best_score = Scalar::zero();
    for idx in 0..CHANNEL_LABELS.len() {
        let score = weights[idx] * supports[idx];
        if score > best_score {
            best_score = score;
        }
    }
    let clamped = if best_score <= Scalar::zero() {
        Scalar::one()
    } else {
        best_score.clamp(Scalar::from_f32(0.05), Scalar::one())
    };
    clamped.raw()
}

pub(crate) fn culture_layer_state(layer: &CultureLayer) -> CultureLayerState {
    let baseline = layer.traits.baseline();
    let modifier = layer.traits.modifier();
    let values = layer.traits.values();
    let mut traits = Vec::with_capacity(SimCultureTraitAxis::ALL.len());
    for axis in SimCultureTraitAxis::ALL {
        let idx = axis.index();
        traits.push(CultureTraitEntry {
            axis: map_trait_axis(axis),
            baseline: baseline[idx].raw(),
            modifier: modifier[idx].raw(),
            value: values[idx].raw(),
        });
    }
    CultureLayerState {
        id: layer.id,
        owner: layer.owner.0,
        parent: layer.parent.unwrap_or(0),
        scope: map_layer_scope(layer.scope),
        traits,
        divergence: layer.divergence.magnitude.raw(),
        soft_threshold: layer.divergence.soft_threshold.raw(),
        hard_threshold: layer.divergence.hard_threshold.raw(),
        ticks_above_soft: layer.divergence.ticks_above_soft,
        ticks_above_hard: layer.divergence.ticks_above_hard,
        last_updated_tick: layer.last_updated_tick,
    }
}

pub(crate) fn culture_tension_state(record: CultureTensionRecord) -> CultureTensionState {
    CultureTensionState {
        layer_id: record.layer_id,
        scope: map_layer_scope(record.scope),
        owner: record.owner.0,
        severity: record.magnitude.raw(),
        timer: record.timer,
        kind: map_tension_kind(record.kind),
    }
}

pub(crate) fn map_layer_scope(scope: SimCultureLayerScope) -> sim_runtime::CultureLayerScope {
    match scope {
        SimCultureLayerScope::Global => sim_runtime::CultureLayerScope::Global,
        SimCultureLayerScope::Regional => sim_runtime::CultureLayerScope::Regional,
        SimCultureLayerScope::Local => sim_runtime::CultureLayerScope::Local,
    }
}

pub(crate) fn map_trait_axis(axis: SimCultureTraitAxis) -> sim_runtime::CultureTraitAxis {
    match axis {
        SimCultureTraitAxis::PassiveAggressive => sim_runtime::CultureTraitAxis::PassiveAggressive,
        SimCultureTraitAxis::OpenClosed => sim_runtime::CultureTraitAxis::OpenClosed,
        SimCultureTraitAxis::CollectivistIndividualist => {
            sim_runtime::CultureTraitAxis::CollectivistIndividualist
        }
        SimCultureTraitAxis::TraditionalistRevisionist => {
            sim_runtime::CultureTraitAxis::TraditionalistRevisionist
        }
        SimCultureTraitAxis::HierarchicalEgalitarian => {
            sim_runtime::CultureTraitAxis::HierarchicalEgalitarian
        }
        SimCultureTraitAxis::SyncreticPurist => sim_runtime::CultureTraitAxis::SyncreticPurist,
        SimCultureTraitAxis::AsceticIndulgent => sim_runtime::CultureTraitAxis::AsceticIndulgent,
        SimCultureTraitAxis::PragmaticIdealistic => {
            sim_runtime::CultureTraitAxis::PragmaticIdealistic
        }
        SimCultureTraitAxis::RationalistMystical => {
            sim_runtime::CultureTraitAxis::RationalistMystical
        }
        SimCultureTraitAxis::ExpansionistInsular => {
            sim_runtime::CultureTraitAxis::ExpansionistInsular
        }
        SimCultureTraitAxis::AdaptiveStubborn => sim_runtime::CultureTraitAxis::AdaptiveStubborn,
        SimCultureTraitAxis::HonorBoundOpportunistic => {
            sim_runtime::CultureTraitAxis::HonorBoundOpportunistic
        }
        SimCultureTraitAxis::MeritOrientedLineageOriented => {
            sim_runtime::CultureTraitAxis::MeritOrientedLineageOriented
        }
        SimCultureTraitAxis::SecularDevout => sim_runtime::CultureTraitAxis::SecularDevout,
        SimCultureTraitAxis::PluralisticMonocultural => {
            sim_runtime::CultureTraitAxis::PluralisticMonocultural
        }
    }
}

pub(crate) fn map_tension_kind(kind: SimCultureTensionKind) -> sim_runtime::CultureTensionKind {
    match kind {
        SimCultureTensionKind::DriftWarning => sim_runtime::CultureTensionKind::DriftWarning,
        SimCultureTensionKind::AssimilationPush => {
            sim_runtime::CultureTensionKind::AssimilationPush
        }
        SimCultureTensionKind::SchismRisk => sim_runtime::CultureTensionKind::SchismRisk,
    }
}

pub(crate) fn culture_raster_from_layers(
    tiles: &[TileState],
    culture: &CultureManager,
    grid_size: UVec2,
    overlays: &SnapshotOverlaysConfig,
) -> ScalarRasterState {
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![0i64; total];

    let culture_cfg = overlays.culture();
    let hard_step = culture_cfg.hard_tick_bonus_step();
    let hard_cap = culture_cfg.hard_tick_bonus_cap();
    let soft_step = culture_cfg.soft_tick_bonus_step();
    let soft_cap = culture_cfg.soft_tick_bonus_cap();

    for tile in tiles {
        if tile.x >= width || tile.y >= height {
            continue;
        }
        let idx = (tile.y as usize) * (width as usize) + tile.x as usize;
        if idx >= samples.len() {
            continue;
        }
        let owner = CultureOwner(tile.entity);
        let Some(layer) = culture.local_layer_by_owner(owner) else {
            continue;
        };
        let magnitude = layer.divergence.magnitude.abs();
        let hard_threshold = if layer.divergence.hard_threshold.raw() > 0 {
            layer.divergence.hard_threshold
        } else {
            Scalar::one()
        };
        let mut ratio = (magnitude / hard_threshold).clamp(Scalar::zero(), Scalar::one());
        if layer.divergence.ticks_above_hard > 0 {
            let boost = Scalar::from_f32(layer.divergence.ticks_above_hard as f32 * hard_step)
                .clamp(Scalar::zero(), Scalar::from_f32(hard_cap));
            ratio = (ratio + boost).clamp(Scalar::zero(), Scalar::one());
        } else if layer.divergence.ticks_above_soft > 0 {
            let boost = Scalar::from_f32(layer.divergence.ticks_above_soft as f32 * soft_step)
                .clamp(Scalar::zero(), Scalar::from_f32(soft_cap));
            ratio = (ratio + boost).clamp(Scalar::zero(), Scalar::one());
        }
        samples[idx] = ratio.raw();
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

pub(crate) fn sentiment_raster_from_populations(
    tiles: &[TileState],
    populations: &[PopulationCohortState],
    grid_size: UVec2,
) -> ScalarRasterState {
    let mut tile_positions = HashMap::with_capacity(tiles.len());
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        tile_positions.insert(tile.entity, (tile.x, tile.y));
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut weighted = vec![0i128; total];
    let mut weights = vec![0i128; total];

    for cohort in populations {
        let Some(&(x, y)) = tile_positions.get(&cohort.home) else {
            continue;
        };
        let idx = (y as usize) * (width as usize) + x as usize;
        if idx >= weighted.len() {
            continue;
        }
        let morale = Scalar::from_raw(cohort.morale);
        let size = i128::from(cohort.size);
        weighted[idx] = weighted[idx].saturating_add(i128::from(morale.raw()) * size);
        weights[idx] = weights[idx].saturating_add(size);
    }

    let mut samples = vec![0i64; total];
    for (idx, sample) in samples.iter_mut().enumerate() {
        let weight = weights[idx];
        if weight > 0 {
            *sample = (weighted[idx] / weight) as i64;
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}
