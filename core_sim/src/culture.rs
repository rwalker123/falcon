use std::collections::HashMap;

use bevy::prelude::*;
use sim_runtime::{
    CultureLayerScope as SchemaLayerScope, CultureLayerState as SchemaCultureLayerState,
    CultureTensionState as SchemaCultureTensionState, CultureTraitAxis as SchemaCultureTraitAxis,
};

use crate::{
    components::BandId,
    culture_corruption_config::{CulturePropagationSettings, DEFAULT_RESONANCE_RESPONSE},
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
/// Global layers use `0`, regional layers encode their region id, and local layers encode the
/// **tile** they sit on — every local layer is attached by worldgen to a tile entity
/// (`attach_local`), so `(x, y)` is the owner's natural key.
///
/// **This used to be `entity.to_bits()`, and that was the single largest rollback defect in the
/// sim.** Restoring a checkpoint despawns and respawns every tile, so bevy hands back fresh
/// generations and every local layer was orphaned: `tiles[].culture_layer` collapsed to `0` for
/// all 384 tiles, `culture_raster` went to zero, and `reconcile_culture_layers` then minted a
/// second set of layers for the new entities. A position cannot be renumbered, so the key holds
/// across a restore by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CultureOwner(pub u64);

/// Set on tile-owned keys so they occupy a range disjoint from region ids and band ids. Without it
/// tile `(0, 0)` would encode to `0` and collide with [`CultureOwner::GLOBAL`].
const TILE_OWNER_TAG: u64 = 1 << 63;

impl CultureOwner {
    pub const GLOBAL: CultureOwner = CultureOwner(0);

    pub fn from_region(region_id: u32) -> Self {
        CultureOwner(region_id as u64)
    }

    /// The owner key for a tile-local layer.
    pub fn from_tile(position: UVec2) -> Self {
        CultureOwner(TILE_OWNER_TAG | ((position.y as u64) << 32) | position.x as u64)
    }

    /// The owner key for a band-local layer.
    ///
    /// **No band owns a layer today** — `attach_local` is only ever called with a tile — so this
    /// resolves to nothing and the faction rollup in `telling::signals` falls through to the global
    /// layer, which is exactly what it did when the key was the band's entity bits. Bands carrying
    /// their own culture is issue #407; this is the identity half only, kept so the rollup names a
    /// stable band rather than a number that changed on every restore.
    pub fn from_band(band: BandId) -> Self {
        CultureOwner(band.0)
    }
}

/// Scope classification for a culture layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CultureLayerScope {
    Global,
    Regional,
    Local,
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
#[derive(Debug, Clone)]
pub struct CultureDivergence {
    pub magnitude: Scalar,
    pub soft_threshold: Scalar,
    pub hard_threshold: Scalar,
    pub ticks_above_soft: u16,
    pub ticks_above_hard: u16,
    pub soft_trigger_ticks: u16,
    pub hard_trigger_ticks: u16,
}

impl Default for CultureDivergence {
    fn default() -> Self {
        Self {
            magnitude: Scalar::zero(),
            soft_threshold: scalar_from_f32(0.6),
            hard_threshold: scalar_from_f32(1.2),
            ticks_above_soft: 0,
            ticks_above_hard: 0,
            soft_trigger_ticks: 1,
            hard_trigger_ticks: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ScopeSettings {
    elasticity: Scalar,
    soft_threshold: Scalar,
    hard_threshold: Scalar,
    soft_trigger_ticks: u16,
    hard_trigger_ticks: u16,
}

impl ScopeSettings {
    fn new(
        elasticity: f32,
        soft_threshold: f32,
        hard_threshold: f32,
        soft_trigger_ticks: u16,
        hard_trigger_ticks: u16,
    ) -> Self {
        Self {
            elasticity: scalar_from_f32(elasticity),
            soft_threshold: scalar_from_f32(soft_threshold),
            hard_threshold: scalar_from_f32(hard_threshold),
            soft_trigger_ticks,
            hard_trigger_ticks,
        }
    }

    fn default_for(scope: CultureLayerScope) -> Self {
        match scope {
            CultureLayerScope::Global => Self::new(0.10, 0.6, 1.2, 1, 1),
            CultureLayerScope::Regional => Self::new(0.25, 0.6, 1.2, 1, 1),
            CultureLayerScope::Local => Self::new(0.40, 0.6, 1.2, 1, 1),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CultureManagerSettings {
    global: ScopeSettings,
    regional: ScopeSettings,
    local: ScopeSettings,
    /// Per-turn rate of the low-pass on influencer resonance — the "culture is slow" dial.
    /// See `CulturePropagationSettings::resonance_response`.
    resonance_response: Scalar,
}

impl Default for CultureManagerSettings {
    fn default() -> Self {
        Self {
            resonance_response: scalar_from_f32(DEFAULT_RESONANCE_RESPONSE),
            global: ScopeSettings::default_for(CultureLayerScope::Global),
            regional: ScopeSettings::default_for(CultureLayerScope::Regional),
            local: ScopeSettings::default_for(CultureLayerScope::Local),
        }
    }
}

impl CultureManagerSettings {
    fn from_propagation(config: &CulturePropagationSettings) -> Self {
        Self {
            resonance_response: scalar_from_f32(config.resonance_response()),
            global: ScopeSettings::new(
                config.global().elasticity(),
                config.global().soft_threshold(),
                config.global().hard_threshold(),
                config.global().soft_trigger_ticks(),
                config.global().hard_trigger_ticks(),
            ),
            regional: ScopeSettings::new(
                config.regional().elasticity(),
                config.regional().soft_threshold(),
                config.regional().hard_threshold(),
                config.regional().soft_trigger_ticks(),
                config.regional().hard_trigger_ticks(),
            ),
            local: ScopeSettings::new(
                config.local().elasticity(),
                config.local().soft_threshold(),
                config.local().hard_threshold(),
                config.local().soft_trigger_ticks(),
                config.local().hard_trigger_ticks(),
            ),
        }
    }

    fn scope(&self, scope: CultureLayerScope) -> ScopeSettings {
        match scope {
            CultureLayerScope::Global => self.global,
            CultureLayerScope::Regional => self.regional,
            CultureLayerScope::Local => self.local,
        }
    }
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
        let settings = ScopeSettings::default_for(scope);
        let mut layer = Self {
            id,
            scope,
            owner: CultureOwner::default(),
            parent: None,
            traits: CultureTraitVector::neutral(),
            elasticity: settings.elasticity,
            divergence: CultureDivergence::default(),
            last_updated_tick: 0,
        };
        layer.apply_scope_settings(settings, false);
        layer
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

        let soft_trigger = self.divergence.soft_trigger_ticks.max(1);
        let hard_trigger = self.divergence.hard_trigger_ticks.max(1);

        if prev_hard < hard_trigger && self.divergence.ticks_above_hard >= hard_trigger {
            return Some(CultureTensionKind::SchismRisk);
        }

        if prev_soft < soft_trigger && self.divergence.ticks_above_soft >= soft_trigger {
            return Some(CultureTensionKind::DriftWarning);
        }

        resolution_event
    }

    fn apply_scope_settings(&mut self, settings: ScopeSettings, preserve_thresholds: bool) {
        self.elasticity = settings.elasticity;
        if !preserve_thresholds {
            self.divergence.soft_threshold = settings.soft_threshold;
            self.divergence.hard_threshold = settings.hard_threshold;
        }
        self.divergence.soft_trigger_ticks = settings.soft_trigger_ticks;
        self.divergence.hard_trigger_ticks = settings.hard_trigger_ticks;
    }
}

/// Tracks all culture layers and performs reconcile passes each tick.
#[derive(Resource, Debug)]
pub struct CultureManager {
    next_id: CultureLayerId,
    global: Option<CultureLayer>,
    regional: HashMap<u32, CultureLayer>,
    locals: HashMap<u64, CultureLayer>,
    tension_events: Vec<CultureTensionRecord>,
    settings: CultureManagerSettings,
    /// Influencer resonance as culture actually feels it: the raw vector low-passed at
    /// `settings.resonance_response`.
    ///
    /// **This is what stops culture re-rolling every few turns.** The raw resonance moves as
    /// influencers rise and fall, and feeding it straight into each layer's target made the target
    /// move faster than elasticity could track — layers oscillated and the gap to their parent
    /// GREW over 40 turns rather than closing (`docs/plan_delta_streaming.md` §3.6). Smoothing the
    /// driver fixes the cause; lowering elasticity would only have added lag, because in steady
    /// state a chaser's step size equals its target's velocity no matter how slowly it chases.
    smoothed_resonance: InfluencerCultureResonance,
}

/// Everything [`CultureManager`] holds **except its settings** — see
/// [`crate::influencers::InfluentialRosterCheckpoint`] for why config stays out of a checkpoint.
///
/// `CultureManagerSettings` is the subtle one: it is config held **by value**, not behind an `Arc`,
/// so a search for `Arc<*Config>` misses it entirely. It is derived at boot from
/// `culture_corruption_config`, and cloning the manager whole would carry it.
#[derive(Debug, Clone)]
pub struct CultureManagerCheckpoint {
    next_id: CultureLayerId,
    global: Option<CultureLayer>,
    regional: HashMap<u32, CultureLayer>,
    locals: HashMap<u64, CultureLayer>,
    tension_events: Vec<CultureTensionRecord>,
    smoothed_resonance: InfluencerCultureResonance,
}

impl CultureManager {
    /// Snapshot the manager's state, leaving its settings behind.
    pub fn checkpoint(&self) -> CultureManagerCheckpoint {
        CultureManagerCheckpoint {
            next_id: self.next_id,
            global: self.global.clone(),
            regional: self.regional.clone(),
            locals: self.locals.clone(),
            tension_events: self.tension_events.clone(),
            smoothed_resonance: self.smoothed_resonance,
        }
    }

    /// Restore state, keeping the settings currently attached.
    pub fn restore_checkpoint(&mut self, checkpoint: &CultureManagerCheckpoint) {
        self.next_id = checkpoint.next_id;
        self.global = checkpoint.global.clone();
        self.regional = checkpoint.regional.clone();
        self.locals = checkpoint.locals.clone();
        self.tension_events = checkpoint.tension_events.clone();
        self.smoothed_resonance = checkpoint.smoothed_resonance;
    }
}

impl CultureManager {
    pub fn new() -> Self {
        Self::with_settings(CultureManagerSettings::default())
    }

    pub(crate) fn with_settings(settings: CultureManagerSettings) -> Self {
        Self {
            next_id: 1,
            global: None,
            regional: HashMap::new(),
            locals: HashMap::new(),
            tension_events: Vec::new(),
            settings,
            smoothed_resonance: InfluencerCultureResonance::default(),
        }
    }

    pub fn from_config(config: &CulturePropagationSettings) -> Self {
        Self::with_settings(CultureManagerSettings::from_propagation(config))
    }

    pub fn ensure_global(&mut self) -> CultureLayerId {
        if let Some(layer) = &self.global {
            return layer.id;
        }
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Global);
        layer.apply_scope_settings(self.settings.scope(CultureLayerScope::Global), false);
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
        layer.apply_scope_settings(self.settings.scope(CultureLayerScope::Regional), false);
        layer.parent = Some(parent);
        layer.owner = CultureOwner::from_region(region_id);
        layer.traits = CultureTraitVector::neutral();
        self.regional.insert(region_id, layer);
        id
    }

    pub fn attach_local(
        &mut self,
        position: UVec2,
        parent_region: CultureLayerId,
    ) -> CultureLayerId {
        let owner = CultureOwner::from_tile(position);
        if let Some(layer) = self.locals.get(&owner.0) {
            return layer.id;
        }
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Local);
        layer.parent = Some(parent_region);
        layer.owner = owner;
        layer.apply_scope_settings(self.settings.scope(CultureLayerScope::Local), false);
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
        position: UVec2,
        modifiers: [Scalar; CULTURE_TRAIT_AXES],
    ) {
        if let Some(layer) = self.locals.get_mut(&CultureOwner::from_tile(position).0) {
            layer
                .traits
                .modifier_mut()
                .iter_mut()
                .zip(modifiers)
                .for_each(|(slot, value)| *slot = value);
        }
    }

    pub fn reconcile(&mut self, tick: &SimulationTick, resonance: &InfluencerCultureResonance) {
        if self.global.is_none() && self.regional.is_empty() && self.locals.is_empty() {
            return;
        }

        tracing::trace!(
            target: "culture.reconcile",
            tick = tick.0,
            has_global = self.global.is_some(),
            regional_layers = self.regional.len(),
            local_layers = self.locals.len()
        );

        self.tension_events.clear();
        let mut pending_events = Vec::new();

        // Culture responds to SUSTAINED influencer pressure, not to this turn's roster. Smoothing
        // the driver is what makes culture slow; see `smoothed_resonance`.
        let rate = self.settings.resonance_response;
        let smooth = |current: &mut [Scalar; CULTURE_TRAIT_AXES],
                      raw: &[Scalar; CULTURE_TRAIT_AXES]| {
            for idx in 0..CULTURE_TRAIT_AXES {
                current[idx] += (raw[idx] - current[idx]) * rate;
            }
        };
        smooth(&mut self.smoothed_resonance.global, &resonance.global);
        smooth(&mut self.smoothed_resonance.regional, &resonance.regional);
        smooth(&mut self.smoothed_resonance.local, &resonance.local);
        let resonance = &self.smoothed_resonance;

        let mut global_values = [Scalar::zero(); CULTURE_TRAIT_AXES];
        if let Some(global) = &mut self.global {
            // The global layer offsets from its FOUNDING trait vector, which is what `baseline`
            // holds and why nothing overwrites it here.
            //
            // It used to read `baseline = own current value` and then set `value = baseline +
            // resonance`, which is `value += resonance` every turn: a pure integrator with no
            // decay. Measured, it ran the global culture from 0 to the ±2.5 clamp in ~40 turns on
            // a default start, and since every regional layer chases global and every local layer
            // chases its region, the whole culture tree was dragged along and never settled — the
            // cause of both the runaway trait drift and 4201/4201 layers in every delta
            // (`docs/plan_delta_streaming.md` §3.6).
            //
            // Offsetting from a fixed origin and RELAXING toward it makes the global layer behave
            // like the other two scopes: a bounded offset from a stable reference, moving only as
            // fast as the resonance does.
            let origin = *global.traits.baseline();
            let elasticity = global.elasticity;
            for idx in 0..CULTURE_TRAIT_AXES {
                let target = (origin[idx] + resonance.global[idx])
                    .clamp(scalar_from_f32(-2.5), scalar_from_f32(2.5));
                let current = global.traits.values()[idx];
                global
                    .traits
                    .update_value(idx, current + (target - current) * elasticity);
                global_values[idx] = global.traits.values()[idx];
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
                let record = Self::build_tension_record(layer, kind);
                tracing::debug!(
                    target: "culture.tension",
                    kind = ?record.kind,
                    scope = ?record.scope,
                    owner = record.owner.0,
                    layer_id = record.layer_id,
                    magnitude = record.magnitude.to_f32(),
                    timer = record.timer,
                    "regional culture tension triggered"
                );
                pending_events.push(record);
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
                let record = Self::build_tension_record(layer, kind);
                tracing::debug!(
                    target: "culture.tension",
                    kind = ?record.kind,
                    scope = ?record.scope,
                    owner = record.owner.0,
                    layer_id = record.layer_id,
                    magnitude = record.magnitude.to_f32(),
                    timer = record.timer,
                    "local culture tension triggered"
                );
                pending_events.push(record);
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
        let soft_trigger = layer.divergence.soft_trigger_ticks.max(1);
        let hard_trigger = layer.divergence.hard_trigger_ticks.max(1);
        let hard_active = layer.divergence.ticks_above_hard >= hard_trigger;
        let soft_active = layer.divergence.ticks_above_soft >= soft_trigger;

        if hard_active || soft_active {
            let kind = if hard_active {
                CultureTensionKind::SchismRisk
            } else {
                CultureTensionKind::DriftWarning
            };
            let timer = if hard_active {
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
            layer.apply_scope_settings(self.settings.scope(scope), true);

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

    pub fn local_layer_by_owner(&self, owner: CultureOwner) -> Option<&CultureLayer> {
        self.locals.get(&owner.0)
    }

    pub fn global_layer(&self) -> Option<&CultureLayer> {
        self.global.as_ref()
    }

    /// **The faction-level culture rollup**: a population-weighted average of `bands`' local
    /// layers, per trait axis.
    ///
    /// Local layers key off a band's `Entity`, so a faction's culture is only ever the aggregate
    /// of the bands that carry it — weight belongs on population, because a 200-person band should
    /// speak louder than a 12-person one. Callers pass **resident** bands only (an expedition is
    /// detached and doesn't vote).
    ///
    /// Falls back to the global layer for any faction with no local layers (or no people), which
    /// is the reading `The Telling`'s `culture.axis.*` signals used before this existed.
    pub fn faction_trait_average(
        &self,
        bands: &[(CultureOwner, u32)],
    ) -> [f32; CULTURE_TRAIT_AXES] {
        let mut totals = [0.0f32; CULTURE_TRAIT_AXES];
        let mut weight_total = 0.0f32;
        for (owner, people) in bands {
            let (Some(layer), weight) = (self.local_layer_by_owner(*owner), *people as f32) else {
                continue;
            };
            if weight <= 0.0 {
                continue;
            }
            for (index, value) in layer.traits.values().iter().enumerate() {
                totals[index] += value.to_f32() * weight;
            }
            weight_total += weight;
        }
        if weight_total > 0.0 {
            for total in totals.iter_mut() {
                *total /= weight_total;
            }
            return totals;
        }
        self.global_layer()
            .map(|layer| {
                let mut values = [0.0f32; CULTURE_TRAIT_AXES];
                for (index, value) in layer.traits.values().iter().enumerate() {
                    values[index] = value.to_f32();
                }
                values
            })
            .unwrap_or(totals)
    }

    pub fn global_layer_mut(&mut self) -> Option<&mut CultureLayer> {
        self.global.as_mut()
    }
}

impl Default for CultureManager {
    fn default() -> Self {
        Self::new()
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
        tracing::info!(
            target: "culture.tension",
            kind = ?record.kind,
            scope = ?record.scope,
            owner = record.owner.0,
            layer_id = record.layer_id,
            magnitude = record.magnitude.to_f32(),
            timer = record.timer,
            "culture tension event emitted"
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::influencers::InfluencerCultureResonance;

    fn default_resonance() -> InfluencerCultureResonance {
        InfluencerCultureResonance::default()
    }

    fn settings(global: ScopeSettings, regional: ScopeSettings) -> CultureManagerSettings {
        CultureManagerSettings {
            global,
            regional,
            ..Default::default()
        }
    }

    #[test]
    fn drift_warning_respects_trigger_ticks() {
        let mut manager = CultureManager::with_settings(settings(
            ScopeSettings::new(1.0, 0.0, 1.0, 1, 1),
            ScopeSettings::new(1.0, 0.2, 1.0, 3, 5),
        ));
        let resonance = default_resonance();

        manager.ensure_global();
        let region = 1;
        manager.upsert_regional(region);
        {
            let region = manager
                .regional_layer_mut_by_region(region)
                .expect("regional layer should exist");
            region
                .traits
                .set_modifier(CultureTraitAxis::OpenClosed, scalar_from_f32(1.0));
        }

        manager.reconcile(&SimulationTick(1), &resonance);
        assert!(
            manager.take_tension_events().is_empty(),
            "drift event should wait for trigger ticks"
        );

        manager.reconcile(&SimulationTick(2), &resonance);
        assert!(
            manager.take_tension_events().is_empty(),
            "drift event should still wait for trigger ticks"
        );

        manager.reconcile(&SimulationTick(3), &resonance);
        let events = manager.take_tension_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, CultureTensionKind::DriftWarning);
        assert_eq!(events[0].timer, 3);
    }

    #[test]
    fn schism_requires_multiple_ticks() {
        let mut manager = CultureManager::with_settings(settings(
            ScopeSettings::new(1.0, 0.0, 1.0, 1, 1),
            ScopeSettings::new(1.0, 0.2, 0.5, 1, 2),
        ));
        let resonance = default_resonance();

        manager.ensure_global();
        let region = 7;
        manager.upsert_regional(region);
        {
            let region = manager
                .regional_layer_mut_by_region(region)
                .expect("regional layer should exist");
            region
                .traits
                .set_modifier(CultureTraitAxis::OpenClosed, scalar_from_f32(1.0));
        }

        manager.reconcile(&SimulationTick(1), &resonance);
        let first_events = manager.take_tension_events();
        assert_eq!(first_events.len(), 1);
        assert_eq!(first_events[0].kind, CultureTensionKind::DriftWarning);

        manager.reconcile(&SimulationTick(2), &resonance);
        let second_events = manager.take_tension_events();
        assert!(
            second_events
                .iter()
                .any(|event| event.kind == CultureTensionKind::SchismRisk),
            "schism risk should trigger after configured hard trigger ticks"
        );
    }

    #[test]
    fn assimilation_push_emitted_on_resolution() {
        let mut manager = CultureManager::with_settings(settings(
            ScopeSettings::new(1.0, 0.0, 1.0, 1, 1),
            ScopeSettings::new(1.0, 0.2, 1.0, 1, 2),
        ));
        let resonance = default_resonance();

        manager.ensure_global();
        let region = 21;
        manager.upsert_regional(region);
        {
            let region = manager
                .regional_layer_mut_by_region(region)
                .expect("regional layer should exist");
            region
                .traits
                .set_modifier(CultureTraitAxis::OpenClosed, scalar_from_f32(1.0));
        }

        manager.reconcile(&SimulationTick(1), &resonance);
        let initial_events = manager.take_tension_events();
        assert_eq!(initial_events.len(), 1);
        assert_eq!(initial_events[0].kind, CultureTensionKind::DriftWarning);

        {
            let region = manager
                .regional_layer_mut_by_region(region)
                .expect("regional layer should exist");
            region
                .traits
                .set_modifier(CultureTraitAxis::OpenClosed, Scalar::zero());
        }

        manager.reconcile(&SimulationTick(2), &resonance);
        let resolve_events = manager.take_tension_events();
        assert!(
            resolve_events
                .iter()
                .any(|event| event.kind == CultureTensionKind::AssimilationPush),
            "assimilation push should emit when divergence resolves"
        );
    }
}

#[cfg(test)]
mod global_layer_tests {
    use super::*;

    /// **The global layer must not integrate its resonance.** It used to compute
    /// `target = own_current_value + resonance` and assign it, which is `value += resonance` every
    /// turn — a pure accumulator with no decay. Measured on a default start it ran the global
    /// culture from 0 to the ±2.5 clamp in ~40 turns, dragging every regional and local layer with
    /// it (`docs/plan_delta_streaming.md` §3.6).
    ///
    /// The property that rules that out: under a CONSTANT resonance the global layer must settle
    /// at a bounded offset, not walk away. Pinned rather than left to the tuning dials, because a
    /// runaway is invisible for the first dozen turns and looks like "culture is evolving".
    #[test]
    fn a_constant_resonance_settles_the_global_layer_instead_of_accumulating() {
        let mut manager = CultureManager::new();
        manager.ensure_global();

        let mut resonance = InfluencerCultureResonance::default();
        resonance.global[0] = scalar_from_f32(0.05);

        let mut last = 0.0f32;
        for tick in 1..=200u64 {
            manager.reconcile(&SimulationTick(tick), &resonance);
            last = manager
                .global
                .as_ref()
                .expect("global layer")
                .traits
                .values()[0]
                .to_f32();
        }

        // Settles AT the offset the resonance describes, not somewhere far past it.
        assert!(
            (last - 0.05).abs() < 0.005,
            "global culture should converge on origin+resonance (0.05), got {last}"
        );
        assert!(
            last < 0.5,
            "a 0.05 resonance must never accumulate into a large value — got {last}"
        );
    }
}
