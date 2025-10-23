use std::collections::HashMap;

use bevy::prelude::*;
use sim_runtime::{
    CultureLayerScope as SchemaLayerScope, CultureLayerState as SchemaCultureLayerState,
    CultureTensionState as SchemaCultureTensionState, CultureTraitAxis as SchemaCultureTraitAxis,
};

use crate::{
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    resources::SimulationTick,
    scalar::{scalar_from_f32, Scalar},
};

/// Number of trait axes defined for each culture vector.
pub const CULTURE_TRAIT_AXES: usize = 15;

/// Unique identifier for a culture layer instance.
pub type CultureLayerId = u32;

/// Opaque owner identifier encoded into snapshots.
///
/// Global layers use `0`, regional layers should encode their region id, and
/// local layers should encode the entity bits of the owning settlement/cohort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CultureOwner(pub u64);

impl CultureOwner {
    pub const GLOBAL: CultureOwner = CultureOwner(0);

    pub fn from_region(region_id: u32) -> Self {
        CultureOwner(region_id as u64)
    }

    pub fn from_entity(entity: Entity) -> Self {
        CultureOwner(entity.to_bits())
    }
}

/// Scope classification for a culture layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CultureLayerScope {
    Global,
    Regional,
    Local,
}

impl CultureLayerScope {
    pub fn elasticity(self) -> Scalar {
        match self {
            CultureLayerScope::Global => scalar_from_f32(0.10),
            CultureLayerScope::Regional => scalar_from_f32(0.25),
            CultureLayerScope::Local => scalar_from_f32(0.40),
        }
    }
}

/// Named axes as described in the game manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CultureTraitAxis {
    PassiveAggressive,
    OpenClosed,
    CollectivistIndividualist,
    TraditionalistRevisionist,
    HierarchicalEgalitarian,
    SyncreticPurist,
    AsceticIndulgent,
    PragmaticIdealistic,
    RationalistMystical,
    ExpansionistInsular,
    AdaptiveStubborn,
    HonorBoundOpportunistic,
    MeritOrientedLineageOriented,
    SecularDevout,
    PluralisticMonocultural,
}

impl CultureTraitAxis {
    pub const ALL: [CultureTraitAxis; CULTURE_TRAIT_AXES] = [
        CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist,
        CultureTraitAxis::TraditionalistRevisionist,
        CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented,
        CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural,
    ];

    pub fn index(self) -> usize {
        match self {
            CultureTraitAxis::PassiveAggressive => 0,
            CultureTraitAxis::OpenClosed => 1,
            CultureTraitAxis::CollectivistIndividualist => 2,
            CultureTraitAxis::TraditionalistRevisionist => 3,
            CultureTraitAxis::HierarchicalEgalitarian => 4,
            CultureTraitAxis::SyncreticPurist => 5,
            CultureTraitAxis::AsceticIndulgent => 6,
            CultureTraitAxis::PragmaticIdealistic => 7,
            CultureTraitAxis::RationalistMystical => 8,
            CultureTraitAxis::ExpansionistInsular => 9,
            CultureTraitAxis::AdaptiveStubborn => 10,
            CultureTraitAxis::HonorBoundOpportunistic => 11,
            CultureTraitAxis::MeritOrientedLineageOriented => 12,
            CultureTraitAxis::SecularDevout => 13,
            CultureTraitAxis::PluralisticMonocultural => 14,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CultureTensionKind {
    DriftWarning,
    AssimilationPush,
    SchismRisk,
}

#[derive(Debug, Clone)]
pub struct CultureTensionRecord {
    pub layer_id: CultureLayerId,
    pub scope: CultureLayerScope,
    pub owner: CultureOwner,
    pub kind: CultureTensionKind,
    pub magnitude: Scalar,
    pub timer: u16,
}

#[derive(Event, Debug, Clone)]
pub struct CultureTensionEvent {
    pub layer_id: CultureLayerId,
    pub scope: CultureLayerScope,
    pub owner: CultureOwner,
    pub kind: CultureTensionKind,
    pub magnitude: Scalar,
    pub timer: u16,
}

#[derive(Event, Debug, Clone)]
pub struct CultureSchismEvent {
    pub layer_id: CultureLayerId,
    pub scope: CultureLayerScope,
    pub owner: CultureOwner,
    pub magnitude: Scalar,
    pub timer: u16,
}

impl From<&CultureTensionRecord> for CultureTensionEvent {
    fn from(value: &CultureTensionRecord) -> Self {
        Self {
            layer_id: value.layer_id,
            scope: value.scope,
            owner: value.owner,
            kind: value.kind,
            magnitude: value.magnitude,
            timer: value.timer,
        }
    }
}

impl From<&CultureTensionRecord> for CultureSchismEvent {
    fn from(value: &CultureTensionRecord) -> Self {
        Self {
            layer_id: value.layer_id,
            scope: value.scope,
            owner: value.owner,
            magnitude: value.magnitude,
            timer: value.timer,
        }
    }
}

impl From<&CultureTensionEvent> for CultureTensionRecord {
    fn from(value: &CultureTensionEvent) -> Self {
        Self {
            layer_id: value.layer_id,
            scope: value.scope,
            owner: value.owner,
            kind: value.kind,
            magnitude: value.magnitude,
            timer: value.timer,
        }
    }
}

impl From<&CultureSchismEvent> for CultureTensionRecord {
    fn from(value: &CultureSchismEvent) -> Self {
        Self {
            layer_id: value.layer_id,
            scope: value.scope,
            owner: value.owner,
            kind: CultureTensionKind::SchismRisk,
            magnitude: value.magnitude,
            timer: value.timer,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CultureEffectsCache {
    pub logistics_multiplier: Scalar,
    pub morale_bias: Scalar,
    pub power_bonus: Scalar,
    pub knowledge_leak_multiplier: Scalar,
}

impl Default for CultureEffectsCache {
    fn default() -> Self {
        Self {
            logistics_multiplier: Scalar::one(),
            morale_bias: Scalar::zero(),
            power_bonus: Scalar::zero(),
            knowledge_leak_multiplier: Scalar::one(),
        }
    }
}

/// Stores baseline, modifier, and resolved trait values for a layer.
#[derive(Debug, Clone)]
pub struct CultureTraitVector {
    baseline: [Scalar; CULTURE_TRAIT_AXES],
    modifier: [Scalar; CULTURE_TRAIT_AXES],
    value: [Scalar; CULTURE_TRAIT_AXES],
}

impl CultureTraitVector {
    pub fn neutral() -> Self {
        Self {
            baseline: [Scalar::zero(); CULTURE_TRAIT_AXES],
            modifier: [Scalar::zero(); CULTURE_TRAIT_AXES],
            value: [Scalar::zero(); CULTURE_TRAIT_AXES],
        }
    }

    pub fn with_baseline(baseline: [Scalar; CULTURE_TRAIT_AXES]) -> Self {
        Self {
            value: baseline,
            baseline,
            modifier: [Scalar::zero(); CULTURE_TRAIT_AXES],
        }
    }

    pub fn values(&self) -> &[Scalar; CULTURE_TRAIT_AXES] {
        &self.value
    }

    pub fn baseline(&self) -> &[Scalar; CULTURE_TRAIT_AXES] {
        &self.baseline
    }

    pub fn baseline_mut(&mut self) -> &mut [Scalar; CULTURE_TRAIT_AXES] {
        &mut self.baseline
    }

    pub fn modifier(&self) -> &[Scalar; CULTURE_TRAIT_AXES] {
        &self.modifier
    }

    pub fn modifier_mut(&mut self) -> &mut [Scalar; CULTURE_TRAIT_AXES] {
        &mut self.modifier
    }

    pub fn set_modifier(&mut self, axis: CultureTraitAxis, value: Scalar) {
        self.modifier[axis.index()] = value;
    }

    pub fn update_value(&mut self, index: usize, value: Scalar) {
        self.value[index] = value;
    }
}

/// Book-keeping for divergence tracking against thresholds.
#[derive(Debug, Clone, Default)]
pub struct CultureDivergence {
    pub magnitude: Scalar,
    pub soft_threshold: Scalar,
    pub hard_threshold: Scalar,
    pub ticks_above_soft: u16,
    pub ticks_above_hard: u16,
}

/// Culture layer data structure.
#[derive(Debug, Clone)]
pub struct CultureLayer {
    pub id: CultureLayerId,
    pub scope: CultureLayerScope,
    pub owner: CultureOwner,
    pub parent: Option<CultureLayerId>,
    pub traits: CultureTraitVector,
    pub elasticity: Scalar,
    pub divergence: CultureDivergence,
    pub last_updated_tick: u64,
}

impl CultureLayer {
    pub fn new(id: CultureLayerId, scope: CultureLayerScope) -> Self {
        let elasticity = scope.elasticity();
        Self {
            id,
            scope,
            owner: CultureOwner::default(),
            parent: None,
            traits: CultureTraitVector::neutral(),
            elasticity,
            divergence: CultureDivergence {
                magnitude: Scalar::zero(),
                soft_threshold: scalar_from_f32(0.6),
                hard_threshold: scalar_from_f32(1.2),
                ticks_above_soft: 0,
                ticks_above_hard: 0,
            },
            last_updated_tick: 0,
        }
    }

    fn resolve_against(
        &mut self,
        parent_values: &[Scalar; CULTURE_TRAIT_AXES],
        resonance: Option<&[Scalar; CULTURE_TRAIT_AXES]>,
    ) {
        let elasticity = self.elasticity;
        for (idx, parent_value) in parent_values.iter().enumerate() {
            self.traits.baseline[idx] = *parent_value;
            let mut target = *parent_value + self.traits.modifier[idx];
            if let Some(extra) = resonance {
                target += extra[idx];
            }
            let current = self.traits.value[idx];
            let delta = (target - current) * elasticity;
            self.traits.update_value(idx, current + delta);
        }
    }

    fn evaluate_divergence(&mut self, parent_values: &[Scalar; CULTURE_TRAIT_AXES]) {
        let mut max_delta = Scalar::zero();
        for (idx, parent_value) in parent_values.iter().enumerate() {
            let diff = (self.traits.value[idx] - *parent_value).abs();
            if diff > max_delta {
                max_delta = diff;
            }
        }
        self.divergence.magnitude = max_delta;
    }

    fn tick_thresholds(&mut self) -> Option<CultureTensionKind> {
        let prev_soft = self.divergence.ticks_above_soft;
        let prev_hard = self.divergence.ticks_above_hard;
        let mut resolution_event = None;

        if self.divergence.magnitude >= self.divergence.hard_threshold {
            self.divergence.ticks_above_hard = self.divergence.ticks_above_hard.saturating_add(1);
            self.divergence.ticks_above_soft = self.divergence.ticks_above_soft.saturating_add(1);
        } else {
            if self.divergence.ticks_above_hard > 0 {
                resolution_event = Some(CultureTensionKind::AssimilationPush);
            }
            self.divergence.ticks_above_hard = 0;

            if self.divergence.magnitude >= self.divergence.soft_threshold {
                self.divergence.ticks_above_soft =
                    self.divergence.ticks_above_soft.saturating_add(1);
            } else {
                if self.divergence.ticks_above_soft > 0 && resolution_event.is_none() {
                    resolution_event = Some(CultureTensionKind::AssimilationPush);
                }
                self.divergence.ticks_above_soft = 0;
            }
        }

        if prev_hard == 0 && self.divergence.ticks_above_hard > 0 {
            return Some(CultureTensionKind::SchismRisk);
        }

        if prev_soft == 0 && self.divergence.ticks_above_soft > 0 {
            return Some(CultureTensionKind::DriftWarning);
        }

        resolution_event
    }
}

/// Tracks all culture layers and performs reconcile passes each tick.
#[derive(Resource, Debug, Default)]
pub struct CultureManager {
    next_id: CultureLayerId,
    global: Option<CultureLayer>,
    regional: HashMap<u32, CultureLayer>,
    locals: HashMap<u64, CultureLayer>,
    tension_events: Vec<CultureTensionRecord>,
}

impl CultureManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            global: None,
            regional: HashMap::new(),
            locals: HashMap::new(),
            tension_events: Vec::new(),
        }
    }

    pub fn ensure_global(&mut self) -> CultureLayerId {
        if let Some(layer) = &self.global {
            return layer.id;
        }
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Global);
        layer.owner = CultureOwner::GLOBAL;
        layer.traits = CultureTraitVector::neutral();
        self.global = Some(layer);
        id
    }

    pub fn upsert_regional(&mut self, region_id: u32) -> CultureLayerId {
        if let Some(layer) = self.regional.get(&region_id) {
            return layer.id;
        }
        let parent = self.ensure_global();
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Regional);
        layer.parent = Some(parent);
        layer.owner = CultureOwner::from_region(region_id);
        layer.traits = CultureTraitVector::neutral();
        self.regional.insert(region_id, layer);
        id
    }

    pub fn attach_local(
        &mut self,
        entity: Entity,
        parent_region: CultureLayerId,
    ) -> CultureLayerId {
        let owner = CultureOwner::from_entity(entity);
        if let Some(layer) = self.locals.get(&owner.0) {
            return layer.id;
        }
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Local);
        layer.parent = Some(parent_region);
        layer.owner = owner;
        layer.traits = CultureTraitVector::neutral();
        self.locals.insert(owner.0, layer);
        id
    }

    fn allocate_id(&mut self) -> CultureLayerId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    pub fn apply_initial_modifiers(
        &mut self,
        entity: Entity,
        modifiers: [Scalar; CULTURE_TRAIT_AXES],
    ) {
        if let Some(layer) = self.locals.get_mut(&CultureOwner::from_entity(entity).0) {
            layer
                .traits
                .modifier_mut()
                .iter_mut()
                .zip(modifiers.into_iter())
                .for_each(|(slot, value)| *slot = value);
        }
    }

    pub fn reconcile(&mut self, tick: &SimulationTick, resonance: &InfluencerCultureResonance) {
        if self.global.is_none() && self.regional.is_empty() && self.locals.is_empty() {
            return;
        }

        self.tension_events.clear();
        let mut pending_events = Vec::new();

        let mut global_values = [Scalar::zero(); CULTURE_TRAIT_AXES];
        if let Some(global) = &mut self.global {
            let baseline_values = *global.traits.values();
            *global.traits.baseline_mut() = baseline_values;
            for idx in 0..CULTURE_TRAIT_AXES {
                let target = (baseline_values[idx] + resonance.global[idx])
                    .clamp(scalar_from_f32(-2.5), scalar_from_f32(2.5));
                global.traits.update_value(idx, target);
                global_values[idx] = target;
            }
            global.divergence.magnitude = Scalar::zero();
            global.divergence.ticks_above_soft = 0;
            global.divergence.ticks_above_hard = 0;
            global.last_updated_tick = tick.0;
        }

        let regional_resonance = if !self.regional.is_empty() {
            let factor = scalar_from_f32(1.0 / self.regional.len() as f32);
            Some(resonance.regional.map(|value| value * factor))
        } else {
            None
        };

        for layer in self.regional.values_mut() {
            *layer.traits.baseline_mut() = global_values;
            layer.resolve_against(&global_values, regional_resonance.as_ref());
            layer.evaluate_divergence(&global_values);
            let alert = layer.tick_thresholds();
            layer.last_updated_tick = tick.0;
            if let Some(kind) = alert {
                pending_events.push(Self::build_tension_record(layer, kind));
            }
        }

        let mut regional_values: HashMap<CultureLayerId, [Scalar; CULTURE_TRAIT_AXES]> =
            HashMap::with_capacity(self.regional.len());
        for layer in self.regional.values() {
            regional_values.insert(layer.id, *layer.traits.values());
        }

        let local_resonance = if !self.locals.is_empty() {
            let factor = scalar_from_f32(1.0 / self.locals.len() as f32);
            Some(resonance.local.map(|value| value * factor))
        } else {
            None
        };

        for layer in self.locals.values_mut() {
            let Some(parent_id) = layer.parent else {
                continue;
            };
            let Some(parent_values) = regional_values.get(&parent_id) else {
                continue;
            };
            layer.resolve_against(parent_values, local_resonance.as_ref());
            layer.evaluate_divergence(parent_values);
            let alert = layer.tick_thresholds();
            layer.last_updated_tick = tick.0;
            if let Some(kind) = alert {
                pending_events.push(Self::build_tension_record(layer, kind));
            }
        }

        self.tension_events.extend(pending_events);
    }

    fn build_tension_record(
        layer: &CultureLayer,
        kind: CultureTensionKind,
    ) -> CultureTensionRecord {
        let timer = match kind {
            CultureTensionKind::SchismRisk => layer.divergence.ticks_above_hard,
            CultureTensionKind::DriftWarning => layer.divergence.ticks_above_soft,
            CultureTensionKind::AssimilationPush => 0,
        };
        CultureTensionRecord {
            layer_id: layer.id,
            scope: layer.scope,
            owner: layer.owner,
            kind,
            magnitude: layer.divergence.magnitude,
            timer,
        }
    }

    pub fn take_tension_events(&mut self) -> Vec<CultureTensionRecord> {
        std::mem::take(&mut self.tension_events)
    }

    pub fn active_tensions(&self) -> Vec<CultureTensionRecord> {
        let mut records = Vec::new();

        if let Some(global) = &self.global {
            self.collect_active(global, &mut records);
        }
        for layer in self.regional.values() {
            self.collect_active(layer, &mut records);
        }
        for layer in self.locals.values() {
            self.collect_active(layer, &mut records);
        }

        records
    }

    fn collect_active(&self, layer: &CultureLayer, out: &mut Vec<CultureTensionRecord>) {
        if layer.divergence.magnitude >= layer.divergence.soft_threshold
            || layer.divergence.ticks_above_soft > 0
        {
            let kind = if layer.divergence.ticks_above_hard > 0 {
                CultureTensionKind::SchismRisk
            } else {
                CultureTensionKind::DriftWarning
            };
            let timer = if layer.divergence.ticks_above_hard > 0 {
                layer.divergence.ticks_above_hard
            } else {
                layer.divergence.ticks_above_soft
            };
            out.push(CultureTensionRecord {
                layer_id: layer.id,
                scope: layer.scope,
                owner: layer.owner,
                kind,
                magnitude: layer.divergence.magnitude,
                timer,
            });
        }
    }

    pub fn compute_effects(&self) -> CultureEffectsCache {
        let mut effects = CultureEffectsCache::default();
        let Some(global) = self.global_layer() else {
            return effects;
        };

        let values = global.traits.values();
        let open_bias = values[CultureTraitAxis::OpenClosed.index()]
            .to_f32()
            .clamp(-1.5, 1.5);
        let aggression = values[CultureTraitAxis::PassiveAggressive.index()].to_f32();
        let collectivist = values[CultureTraitAxis::CollectivistIndividualist.index()].to_f32();
        let pragmatic = values[CultureTraitAxis::PragmaticIdealistic.index()].to_f32();
        let devout = values[CultureTraitAxis::SecularDevout.index()].to_f32();
        let purist = values[CultureTraitAxis::SyncreticPurist.index()].to_f32();
        let pluralistic = values[CultureTraitAxis::PluralisticMonocultural.index()].to_f32();

        let logistics_bias = (1.0 + open_bias * 0.25 - aggression * 0.05).clamp(0.5, 1.6);
        effects.logistics_multiplier = scalar_from_f32(logistics_bias);

        let morale_bias =
            (collectivist * 0.015 - aggression * 0.01 + devout * 0.008).clamp(-0.08, 0.08);
        effects.morale_bias = scalar_from_f32(morale_bias);

        let power_bonus = (pragmatic * 0.02 + aggression * 0.01).clamp(-0.12, 0.12);
        effects.power_bonus = scalar_from_f32(power_bonus);

        let knowledge_base =
            (1.0 - purist * 0.08 + open_bias * 0.05 + (-pluralistic) * 0.06).clamp(0.5, 1.5);
        effects.knowledge_leak_multiplier = scalar_from_f32(knowledge_base);

        effects
    }

    pub fn restore_from_snapshot(
        &mut self,
        layers: &[SchemaCultureLayerState],
        _tensions: &[SchemaCultureTensionState],
    ) {
        self.global = None;
        self.regional.clear();
        self.locals.clear();
        self.tension_events.clear();

        let next_id = layers.iter().map(|layer| layer.id).max().unwrap_or(0);
        self.next_id = next_id.wrapping_add(1).max(1);

        for state in layers {
            let scope = from_schema_scope(state.scope);
            let mut layer = CultureLayer::new(state.id, scope);
            layer.owner = CultureOwner(state.owner);
            layer.parent = if state.parent == 0 {
                None
            } else {
                Some(state.parent)
            };

            let mut baseline_values = [Scalar::zero(); CULTURE_TRAIT_AXES];
            let mut modifier_values = [Scalar::zero(); CULTURE_TRAIT_AXES];
            let mut resolved_values = [Scalar::zero(); CULTURE_TRAIT_AXES];
            for entry in &state.traits {
                let axis = from_schema_axis(entry.axis);
                let idx = axis.index();
                baseline_values[idx] = Scalar::from_raw(entry.baseline);
                modifier_values[idx] = Scalar::from_raw(entry.modifier);
                resolved_values[idx] = Scalar::from_raw(entry.value);
            }
            *layer.traits.baseline_mut() = baseline_values;
            *layer.traits.modifier_mut() = modifier_values;
            for (idx, value) in resolved_values.iter().enumerate() {
                layer.traits.update_value(idx, *value);
            }

            layer.divergence.magnitude = Scalar::from_raw(state.divergence);
            layer.divergence.soft_threshold = Scalar::from_raw(state.soft_threshold);
            layer.divergence.hard_threshold = Scalar::from_raw(state.hard_threshold);
            layer.divergence.ticks_above_soft = state.ticks_above_soft;
            layer.divergence.ticks_above_hard = state.ticks_above_hard;
            layer.last_updated_tick = state.last_updated_tick;

            match scope {
                CultureLayerScope::Global => {
                    self.global = Some(layer);
                }
                CultureLayerScope::Regional => {
                    let region_id = state.owner as u32;
                    self.regional.insert(region_id, layer);
                }
                CultureLayerScope::Local => {
                    self.locals.insert(state.owner, layer);
                }
            }
        }
    }

    pub fn regional_layers(&self) -> impl Iterator<Item = &CultureLayer> {
        self.regional.values()
    }

    pub fn regional_layer_mut_by_region(&mut self, region_id: u32) -> Option<&mut CultureLayer> {
        self.regional.get_mut(&region_id)
    }

    pub fn local_layers(&self) -> impl Iterator<Item = &CultureLayer> {
        self.locals.values()
    }

    pub fn local_layer_mut_by_owner(&mut self, owner: CultureOwner) -> Option<&mut CultureLayer> {
        self.locals.get_mut(&owner.0)
    }

    pub fn global_layer(&self) -> Option<&CultureLayer> {
        self.global.as_ref()
    }

    pub fn global_layer_mut(&mut self) -> Option<&mut CultureLayer> {
        self.global.as_mut()
    }
}

/// System wrapper that performs the reconcile pass each turn.
pub fn reconcile_culture_layers(
    mut manager: ResMut<CultureManager>,
    tick: Res<SimulationTick>,
    mut effects: ResMut<CultureEffectsCache>,
    mut tension_writer: EventWriter<CultureTensionEvent>,
    mut schism_writer: EventWriter<CultureSchismEvent>,
    impacts: Res<InfluencerImpacts>,
) {
    let resonance = impacts.culture_resonance();
    manager.reconcile(&tick, &resonance);
    *effects = manager.compute_effects();

    let records = manager.take_tension_events();
    for record in records.iter() {
        tension_writer.send(record.into());
        if record.kind == CultureTensionKind::SchismRisk {
            schism_writer.send(record.into());
        }
    }
}

fn from_schema_scope(scope: SchemaLayerScope) -> CultureLayerScope {
    match scope {
        SchemaLayerScope::Global => CultureLayerScope::Global,
        SchemaLayerScope::Regional => CultureLayerScope::Regional,
        SchemaLayerScope::Local => CultureLayerScope::Local,
    }
}

fn from_schema_axis(axis: SchemaCultureTraitAxis) -> CultureTraitAxis {
    match axis {
        SchemaCultureTraitAxis::PassiveAggressive => CultureTraitAxis::PassiveAggressive,
        SchemaCultureTraitAxis::OpenClosed => CultureTraitAxis::OpenClosed,
        SchemaCultureTraitAxis::CollectivistIndividualist => {
            CultureTraitAxis::CollectivistIndividualist
        }
        SchemaCultureTraitAxis::TraditionalistRevisionist => {
            CultureTraitAxis::TraditionalistRevisionist
        }
        SchemaCultureTraitAxis::HierarchicalEgalitarian => {
            CultureTraitAxis::HierarchicalEgalitarian
        }
        SchemaCultureTraitAxis::SyncreticPurist => CultureTraitAxis::SyncreticPurist,
        SchemaCultureTraitAxis::AsceticIndulgent => CultureTraitAxis::AsceticIndulgent,
        SchemaCultureTraitAxis::PragmaticIdealistic => CultureTraitAxis::PragmaticIdealistic,
        SchemaCultureTraitAxis::RationalistMystical => CultureTraitAxis::RationalistMystical,
        SchemaCultureTraitAxis::ExpansionistInsular => CultureTraitAxis::ExpansionistInsular,
        SchemaCultureTraitAxis::AdaptiveStubborn => CultureTraitAxis::AdaptiveStubborn,
        SchemaCultureTraitAxis::HonorBoundOpportunistic => {
            CultureTraitAxis::HonorBoundOpportunistic
        }
        SchemaCultureTraitAxis::MeritOrientedLineageOriented => {
            CultureTraitAxis::MeritOrientedLineageOriented
        }
        SchemaCultureTraitAxis::SecularDevout => CultureTraitAxis::SecularDevout,
        SchemaCultureTraitAxis::PluralisticMonocultural => {
            CultureTraitAxis::PluralisticMonocultural
        }
    }
}
