use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::prelude::*;
use sim_runtime::{
    CultureLayerScope as SchemaLayerScope, CultureLayerState as SchemaCultureLayerState,
    CultureTensionState as SchemaCultureTensionState, CultureTraitAxis as SchemaCultureTraitAxis,
};

use crate::{
    belief::BeliefRegistry,
    belief_config::BeliefConfigHandle,
    components::{BandId, PopulationCohort, ResidentBand, Tile},
    connections::ConnectionLedger,
    culture_corruption_config::{
        CultureCorruptionConfigHandle, CulturePropagationSettings,
        DEFAULT_BAND_CHARACTER_AMPLITUDE, DEFAULT_BAND_ELASTICITY, DEFAULT_BAND_HARD_TRIGGER_TICKS,
        DEFAULT_BAND_SOFT_TRIGGER_TICKS, DEFAULT_RESONANCE_RESPONSE,
    },
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    orders::FactionId,
    provinces::ProvinceMap,
    resources::SimulationTick,
    scalar::{scalar_from_f32, Scalar},
    wellbeing_config::WellbeingConfigHandle,
};

/// Number of trait axes defined for each culture vector.
pub const CULTURE_TRAIT_AXES: usize = 15;

/// The half-width of the trait range: every axis runs `-CULTURE_TRAIT_SPAN..=CULTURE_TRAIT_SPAN`. The
/// global layer clamps to it, and contact drift reads a band's Purist value against it.
pub const CULTURE_TRAIT_SPAN: f32 = 2.5;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
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

    /// The owner key for a band's own culture layer.
    ///
    /// A [`BandId`] is already durable across a checkpoint restore, so it *is* the key — no tag bit
    /// is needed, because band layers live in their own map (`CultureManager::bands`) and are never
    /// looked up by the tile lookups. That separation is what guarantees a band can never be
    /// returned as a tile's layer, or walked as one.
    pub fn from_band(band: BandId) -> Self {
        CultureOwner(band.0)
    }
}

/// The regional layer a tile or band falls back to when it sits on no province — worldgen mints it
/// up front (`upsert_regional`) so the fallback is always resolvable.
pub const FALLBACK_CULTURE_REGION_ID: u32 = 0;

/// Scope classification for a culture layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CultureLayerScope {
    Global,
    Regional,
    Local,
    /// A band's own culture, carried with the band as it moves. Parented to the regional layer of
    /// the province the band currently stands in.
    Band,
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

/// Stable snake_case key for a culture axis, forming `culture.axis.<key>` (and the `ancestor_pull`
/// keys in `belief_config.json`). Written out rather than derived from the enum's debug name so the wire-visible content vocabulary can never
/// shift under a rename.
pub const fn culture_axis_key(axis: CultureTraitAxis) -> &'static str {
    match axis {
        CultureTraitAxis::PassiveAggressive => "passive_aggressive",
        CultureTraitAxis::OpenClosed => "open_closed",
        CultureTraitAxis::CollectivistIndividualist => "collectivist_individualist",
        CultureTraitAxis::TraditionalistRevisionist => "traditionalist_revisionist",
        CultureTraitAxis::HierarchicalEgalitarian => "hierarchical_egalitarian",
        CultureTraitAxis::SyncreticPurist => "syncretic_purist",
        CultureTraitAxis::AsceticIndulgent => "ascetic_indulgent",
        CultureTraitAxis::PragmaticIdealistic => "pragmatic_idealistic",
        CultureTraitAxis::RationalistMystical => "rationalist_mystical",
        CultureTraitAxis::ExpansionistInsular => "expansionist_insular",
        CultureTraitAxis::AdaptiveStubborn => "adaptive_stubborn",
        CultureTraitAxis::HonorBoundOpportunistic => "honor_bound_opportunistic",
        CultureTraitAxis::MeritOrientedLineageOriented => "merit_oriented_lineage_oriented",
        CultureTraitAxis::SecularDevout => "secular_devout",
        CultureTraitAxis::PluralisticMonocultural => "pluralistic_monocultural",
    }
}

/// The axis a snake_case key (see [`culture_axis_key`]) names, if any.
pub fn culture_axis_from_key(key: &str) -> Option<CultureTraitAxis> {
    CultureTraitAxis::ALL
        .into_iter()
        .find(|axis| culture_axis_key(*axis) == key)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CultureTensionKind {
    DriftWarning,
    AssimilationPush,
    SchismRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            // See `DEFAULT_BAND_ELASTICITY` / `DEFAULT_BAND_*_TRIGGER_TICKS` for why a band is
            // slower than a tile and why it is the one scope whose triggers wait.
            CultureLayerScope::Band => Self::new(
                DEFAULT_BAND_ELASTICITY,
                0.6,
                1.2,
                DEFAULT_BAND_SOFT_TRIGGER_TICKS,
                DEFAULT_BAND_HARD_TRIGGER_TICKS,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CultureManagerSettings {
    global: ScopeSettings,
    regional: ScopeSettings,
    local: ScopeSettings,
    band: ScopeSettings,
    /// Per-turn rate of the low-pass on influencer resonance — the "culture is slow" dial.
    /// See `CulturePropagationSettings::resonance_response`.
    resonance_response: Scalar,
    /// Amplitude of the per-band character offset seeded by [`seeded_modifiers_for_band`].
    /// See `CulturePropagationSettings::band_character_amplitude`.
    band_character_amplitude: f32,
}

impl Default for CultureManagerSettings {
    fn default() -> Self {
        Self {
            resonance_response: scalar_from_f32(DEFAULT_RESONANCE_RESPONSE),
            band_character_amplitude: DEFAULT_BAND_CHARACTER_AMPLITUDE,
            global: ScopeSettings::default_for(CultureLayerScope::Global),
            regional: ScopeSettings::default_for(CultureLayerScope::Regional),
            local: ScopeSettings::default_for(CultureLayerScope::Local),
            band: ScopeSettings::default_for(CultureLayerScope::Band),
        }
    }
}

impl CultureManagerSettings {
    fn from_propagation(config: &CulturePropagationSettings) -> Self {
        Self {
            resonance_response: scalar_from_f32(config.resonance_response()),
            band_character_amplitude: config.band_character_amplitude(),
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
            band: ScopeSettings::new(
                config.band().elasticity(),
                config.band().soft_threshold(),
                config.band().hard_threshold(),
                config.band().soft_trigger_ticks(),
                config.band().hard_trigger_ticks(),
            ),
        }
    }

    fn scope(&self, scope: CultureLayerScope) -> ScopeSettings {
        match scope {
            CultureLayerScope::Global => self.global,
            CultureLayerScope::Regional => self.regional,
            CultureLayerScope::Local => self.local,
            CultureLayerScope::Band => self.band,
        }
    }
}

/// Culture layer data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Band-owned layers, keyed by `CultureOwner::from_band(band).0` (i.e. `BandId.0`).
    ///
    /// **Deliberately a map of its own, not a second population of `locals`.** The tile lookups
    /// (`local_layer_by_owner`, `local_layers`) are what the snapshot walks per tile and per raster
    /// sample; a shared map would make "is this owner a tile or a band?" a runtime question that a
    /// caller could get wrong. Two maps make it a type-level one.
    bands: HashMap<u64, CultureLayer>,
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
    /// The ancestor pull the last [`Self::reconcile`] applied to each band, keyed like `bands`. What
    /// the snapshot publishes as `cultureAncestorPull`: stored, not recomputed at capture, because
    /// `simulate_population` moves anchors, hop counts and belief AFTER the reconcile, so a
    /// recompute from the cohort would be next turn's pull, not the one the layer was just given.
    applied_band_pull: BTreeMap<u64, [Scalar; CULTURE_TRAIT_AXES]>,
    /// The strongest contact pull the last [`Self::apply_contact_drift`] gave each band, keyed like
    /// `bands`. Published as the band's `cultureDrift*` wire fields; stored, not recomputed at
    /// capture, for `applied_band_pull`'s reason. A band that took no pull is absent.
    applied_contact_pull: BTreeMap<u64, ContactPull>,
    /// The bands (keyed like `bands`) that may break away: in the drift-warning state and passing
    /// [`may_break_away`]. Published as `cultureBreakAwayRisk`; stored for `applied_band_pull`'s
    /// reason.
    break_away_risk: BTreeSet<u64>,
}

/// One mutual tie between two resident bands, as contact drift reads it: `tie` is the WEAKER of
/// the two directed strengths (`docs/plan_contact_and_logistics.md` §"Settled by #530").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactTie {
    pub a: BandId,
    pub b: BandId,
    pub tie: Scalar,
}

/// A band's strongest contact pull **from another people's band** this turn: the band it drifted
/// toward, the axis that moved most, and that axis's signed delta (the amount added to the band's
/// modifier). A pull from a band of its own people moves the culture and is never published here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactPull {
    pub source: u64,
    pub axis: usize,
    pub delta: Scalar,
}

/// Everything [`CultureManager`] holds **except its settings** — see
/// [`crate::influencers::InfluentialRosterCheckpoint`] for why config stays out of a checkpoint.
///
/// `CultureManagerSettings` is the subtle one: it is config held **by value**, not behind an `Arc`,
/// so a search for `Arc<*Config>` misses it entirely. It is derived at boot from
/// `culture_corruption_config`, and cloning the manager whole would carry it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CultureManagerCheckpoint {
    next_id: CultureLayerId,
    global: Option<CultureLayer>,
    regional: HashMap<u32, CultureLayer>,
    locals: HashMap<u64, CultureLayer>,
    /// Band layers ride the checkpoint like every other layer — that is how a band's culture
    /// survives a rollback without needing a `Resource`/`Component` of its own.
    bands: HashMap<u64, CultureLayer>,
    tension_events: Vec<CultureTensionRecord>,
    smoothed_resonance: InfluencerCultureResonance,
    applied_band_pull: BTreeMap<u64, [Scalar; CULTURE_TRAIT_AXES]>,
    applied_contact_pull: BTreeMap<u64, ContactPull>,
    break_away_risk: BTreeSet<u64>,
}

impl CultureManager {
    /// Snapshot the manager's state, leaving its settings behind.
    pub fn checkpoint(&self) -> CultureManagerCheckpoint {
        CultureManagerCheckpoint {
            next_id: self.next_id,
            global: self.global.clone(),
            regional: self.regional.clone(),
            locals: self.locals.clone(),
            bands: self.bands.clone(),
            tension_events: self.tension_events.clone(),
            smoothed_resonance: self.smoothed_resonance,
            applied_band_pull: self.applied_band_pull.clone(),
            applied_contact_pull: self.applied_contact_pull.clone(),
            break_away_risk: self.break_away_risk.clone(),
        }
    }

    /// Restore state, keeping the settings currently attached.
    pub fn restore_checkpoint(&mut self, checkpoint: &CultureManagerCheckpoint) {
        self.next_id = checkpoint.next_id;
        self.global = checkpoint.global.clone();
        self.regional = checkpoint.regional.clone();
        self.locals = checkpoint.locals.clone();
        self.bands = checkpoint.bands.clone();
        self.tension_events = checkpoint.tension_events.clone();
        self.smoothed_resonance = checkpoint.smoothed_resonance;
        self.applied_band_pull = checkpoint.applied_band_pull.clone();
        self.applied_contact_pull = checkpoint.applied_contact_pull.clone();
        self.break_away_risk = checkpoint.break_away_risk.clone();
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
            bands: HashMap::new(),
            tension_events: Vec::new(),
            settings,
            smoothed_resonance: InfluencerCultureResonance::default(),
            applied_band_pull: BTreeMap::new(),
            applied_contact_pull: BTreeMap::new(),
            break_away_risk: BTreeSet::new(),
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

    /// Give `band` a culture layer parented to `parent_region`. Idempotent — an existing layer's id
    /// is returned untouched, so the reconcile system can call this every turn.
    ///
    /// The new layer is **seeded from the parent province's current values**, not from neutral: a
    /// band that appears in (or forks inside) a long-diverged province starts *assimilated* to it.
    /// Seeding neutral would make every new band maximally diverged from its own home and trip a
    /// schism for existing. If the parent region is missing the seed falls back to neutral.
    pub fn attach_band(&mut self, band: BandId, parent_region: CultureLayerId) -> CultureLayerId {
        let seed = self
            .regional_layer_by_id(parent_region)
            .map(|parent| *parent.traits.values());
        self.insert_band_layer(band, parent_region, seed)
    }

    /// [`attach_band`](Self::attach_band) for a band that **came from somewhere** — the half a band
    /// fission splits off ([`crate::systems::split_band_from_parent`]). Same layer, parented on the
    /// province the new band stands in; only the trait seed differs, and it is `source`'s current
    /// values rather than that province's.
    ///
    /// That is the migration rule applied to the other way of getting somewhere: `set_band_parent`
    /// lets a band that *walks* twenty tiles keep the culture it arrived with and chase its new
    /// province at the band scope's elasticity. A splinter seeded off the province instead would
    /// snap to the locals the moment it formed — the people who left would become the people they
    /// left. Parenting on the province is what makes it lag toward them instead.
    ///
    /// **The character offset is NOT inherited.** The new band gets its own
    /// [`seeded_modifiers_for_band`], because that offset is the only reason two bands ever diverge;
    /// copying the parent's would make the splinter a permanent clone of the band it came from.
    ///
    /// If `source` has no layer of its own the seed falls back to the province, i.e. plain
    /// `attach_band` — the honest seed when the parent cannot be resolved.
    pub fn attach_band_from_source(
        &mut self,
        band: BandId,
        parent_region: CultureLayerId,
        source: BandId,
    ) -> CultureLayerId {
        match self
            .band_layer_by_owner(CultureOwner::from_band(source))
            .map(|layer| *layer.traits.values())
        {
            Some(seed) => self.insert_band_layer(band, parent_region, Some(seed)),
            None => self.attach_band(band, parent_region),
        }
    }

    /// The one construction both `attach_band` flavours run through: a band-scope layer parented on
    /// `parent_region`, seeded from `seed` (neutral when there is none) and carrying its own
    /// character offset. Idempotent — an existing layer's id comes back untouched.
    fn insert_band_layer(
        &mut self,
        band: BandId,
        parent_region: CultureLayerId,
        seed: Option<[Scalar; CULTURE_TRAIT_AXES]>,
    ) -> CultureLayerId {
        let owner = CultureOwner::from_band(band);
        if let Some(layer) = self.bands.get(&owner.0) {
            return layer.id;
        }
        let id = self.allocate_id();
        let mut layer = CultureLayer::new(id, CultureLayerScope::Band);
        layer.parent = Some(parent_region);
        layer.owner = owner;
        layer.apply_scope_settings(self.settings.scope(CultureLayerScope::Band), false);
        layer.traits = match seed {
            Some(values) => CultureTraitVector::with_baseline(values),
            None => CultureTraitVector::neutral(),
        };
        *layer.traits.modifier_mut() =
            seeded_modifiers_for_band(band, self.settings.band_character_amplitude);
        self.bands.insert(owner.0, layer);
        id
    }

    /// Drop a band's layer — the band is gone, or is no longer a resident band.
    pub fn detach_band(&mut self, band: BandId) {
        self.bands.remove(&CultureOwner::from_band(band).0);
    }

    /// Re-home a band's layer on a new province, **leaving its traits alone**. That is the whole
    /// point: a migrating band keeps the culture it arrived with and then chases its new province
    /// at the band scope's elasticity, so moving lags instead of snapping.
    pub fn set_band_parent(&mut self, band: BandId, parent_region: CultureLayerId) {
        if let Some(layer) = self.bands.get_mut(&CultureOwner::from_band(band).0) {
            layer.parent = Some(parent_region);
        }
    }

    pub fn band_layer_by_owner(&self, owner: CultureOwner) -> Option<&CultureLayer> {
        self.bands.get(&owner.0)
    }

    /// The ancestor pull the last reconcile applied to the band layer `owner`, per axis; `None` when
    /// that band took no pull.
    pub fn applied_band_pull(&self, owner: CultureOwner) -> Option<&[Scalar; CULTURE_TRAIT_AXES]> {
        self.applied_band_pull.get(&owner.0)
    }

    /// Whether band `owner` may break away, as of the last reconcile system pass.
    pub fn break_away_risk(&self, owner: CultureOwner) -> bool {
        self.break_away_risk.contains(&owner.0)
    }

    /// The bands whose layer is in the drift-warning state (`ticks_above_soft` at its trigger) or
    /// the hard state (`ticks_above_hard` at its trigger). **A level, not an edge**: it holds every
    /// turn the layer stays there, unlike the `SchismRisk` record, which fires once on crossing.
    fn bands_in_strain(&self, hard_only: bool) -> Vec<u64> {
        self.bands
            .iter()
            .filter(|(_, layer)| {
                let hard =
                    layer.divergence.ticks_above_hard >= layer.divergence.hard_trigger_ticks.max(1);
                let soft =
                    layer.divergence.ticks_above_soft >= layer.divergence.soft_trigger_ticks.max(1);
                hard || (soft && !hard_only)
            })
            .map(|(owner, _)| *owner)
            .collect()
    }

    /// The strongest contact pull the last [`Self::apply_contact_drift`] gave band `owner`; `None`
    /// when that band took none.
    pub fn applied_contact_pull(&self, owner: CultureOwner) -> Option<&ContactPull> {
        self.applied_contact_pull.get(&owner.0)
    }

    /// **Contact drift** — two bands that know each other grow alike. For each mutual tie, each
    /// side's MODIFIER moves toward the other's value:
    ///
    /// `pull_A = rate x tie x receptiveness_A x weight_B / (weight_A + weight_B) x (value_B - value_A)`
    ///
    /// with `receptiveness_A = 1 - clamp(purist_A / CULTURE_TRAIT_SPAN, -1, 1)`. Every value is read
    /// from a snapshot taken before anything is written, so the order ties are visited in changes
    /// nothing; the sums are over `BTreeMap`s so even the float accumulation order is fixed.
    ///
    /// `weights` is each resident band's headcount keyed by `BandId.0`; a tie naming a band with no
    /// layer or no weight is skipped. `rate == 0` writes nothing (and clears the published pulls).
    ///
    /// **Every mutual tie pulls, kin included, but only a pull from ANOTHER people is published**
    /// (`applied_contact_pull`): `factions` is each band's people as it stands now, and a source is
    /// a candidate for the published pull only when both bands' peoples are known and differ. Kin
    /// drift keeps a people together and is not news to the player.
    pub fn apply_contact_drift(
        &mut self,
        rate: f32,
        factions: &BTreeMap<u64, FactionId>,
        weights: &BTreeMap<u64, Scalar>,
        ties: &[ContactTie],
    ) {
        self.applied_contact_pull.clear();
        if rate <= 0.0 {
            return;
        }
        let purist_axis = CultureTraitAxis::SyncreticPurist.index();
        let start: BTreeMap<u64, [Scalar; CULTURE_TRAIT_AXES]> = self
            .bands
            .iter()
            .map(|(owner, layer)| (*owner, *layer.traits.values()))
            .collect();
        // receiver -> source -> per-axis pull.
        let mut pulls: BTreeMap<u64, BTreeMap<u64, [f32; CULTURE_TRAIT_AXES]>> = BTreeMap::new();
        for tie in ties {
            let (Some(values_a), Some(values_b)) = (start.get(&tie.a.0), start.get(&tie.b.0))
            else {
                continue;
            };
            let (Some(weight_a), Some(weight_b)) = (weights.get(&tie.a.0), weights.get(&tie.b.0))
            else {
                continue;
            };
            let total = weight_a.to_f32() + weight_b.to_f32();
            if total <= 0.0 || tie.tie <= Scalar::zero() {
                continue;
            }
            for (receiver, receiver_values, giver, giver_values, giver_weight) in [
                (tie.a, values_a, tie.b, values_b, weight_b),
                (tie.b, values_b, tie.a, values_a, weight_a),
            ] {
                let purist = receiver_values[purist_axis].to_f32();
                let receptiveness = 1.0 - (purist / CULTURE_TRAIT_SPAN).clamp(-1.0, 1.0);
                let factor =
                    rate * tie.tie.to_f32() * receptiveness * giver_weight.to_f32() / total;
                let entry = pulls
                    .entry(receiver.0)
                    .or_default()
                    .entry(giver.0)
                    .or_insert([0.0; CULTURE_TRAIT_AXES]);
                for idx in 0..CULTURE_TRAIT_AXES {
                    entry[idx] += factor * (giver_values[idx] - receiver_values[idx]).to_f32();
                }
            }
        }
        for (receiver, sources) in pulls {
            let Some(layer) = self.bands.get_mut(&receiver) else {
                continue;
            };
            let mut total = [0.0f32; CULTURE_TRAIT_AXES];
            let mut strongest: Option<(u64, f32)> = None;
            for (source, pull) in &sources {
                for idx in 0..CULTURE_TRAIT_AXES {
                    total[idx] += pull[idx];
                }
                let magnitude: f32 = pull.iter().map(|v| v.abs()).sum();
                // Ascending source order and a strict `>`: the lower BandId wins a tie.
                let foreign = matches!(
                    (factions.get(&receiver), factions.get(source)),
                    (Some(mine), Some(theirs)) if mine != theirs
                );
                if foreign && magnitude > 0.0 && strongest.is_none_or(|(_, best)| magnitude > best)
                {
                    strongest = Some((*source, magnitude));
                }
            }
            for (slot, delta) in layer.traits.modifier_mut().iter_mut().zip(total) {
                *slot += scalar_from_f32(delta);
            }
            if let Some((source, _)) = strongest {
                let pull = &sources[&source];
                let mut axis = 0;
                for idx in 1..CULTURE_TRAIT_AXES {
                    if pull[idx].abs() > pull[axis].abs() {
                        axis = idx;
                    }
                }
                self.applied_contact_pull.insert(
                    receiver,
                    ContactPull {
                        source,
                        axis,
                        delta: scalar_from_f32(pull[axis]),
                    },
                );
            }
        }
    }

    pub fn band_layer_mut_by_owner(&mut self, owner: CultureOwner) -> Option<&mut CultureLayer> {
        self.bands.get_mut(&owner.0)
    }

    pub fn band_layers(&self) -> impl Iterator<Item = &CultureLayer> {
        self.bands.values()
    }

    fn regional_layer_by_id(&self, id: CultureLayerId) -> Option<&CultureLayer> {
        self.regional.values().find(|layer| layer.id == id)
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

    /// `band_pull` is each band's ancestor-pull target offset ([`band_ancestor_pulls`], keyed by the
    /// band's culture owner key); a band absent from it takes no pull.
    pub fn reconcile(
        &mut self,
        tick: &SimulationTick,
        resonance: &InfluencerCultureResonance,
        band_pull: &BTreeMap<u64, [Scalar; CULTURE_TRAIT_AXES]>,
    ) {
        self.applied_band_pull = band_pull.clone();
        if self.global.is_none()
            && self.regional.is_empty()
            && self.locals.is_empty()
            && self.bands.is_empty()
        {
            return;
        }

        tracing::trace!(
            target: "culture.reconcile",
            tick = tick.0,
            has_global = self.global.is_some(),
            regional_layers = self.regional.len(),
            local_layers = self.locals.len(),
            band_layers = self.bands.len()
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
                let target = (origin[idx] + resonance.global[idx]).clamp(
                    scalar_from_f32(-CULTURE_TRAIT_SPAN),
                    scalar_from_f32(CULTURE_TRAIT_SPAN),
                );
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

        for layer in self.bands.values_mut() {
            let Some(parent_id) = layer.parent else {
                continue;
            };
            let Some(parent_values) = regional_values.get(&parent_id) else {
                continue;
            };
            // **No direct influencer resonance, deliberately.** A band feels influencers through
            // its province, which chased the regional channel a few lines up. Giving bands a
            // channel of their own would mean changing how influencers *attribute* resonance in the
            // first place (`InfluencerCultureResonance` has exactly three), which is a different
            // arc. What a band DOES take beyond its province is the pull of its own dead
            // (`band_ancestor_pulls`) — a target offset from the band's belief, not influencer
            // resonance, passed through the same extra-offset slot.
            layer.resolve_against(parent_values, band_pull.get(&layer.owner.0));
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
                    "band culture tension triggered"
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
        // Band layers are deliberately absent: this list is the snapshot's `culture_tensions`
        // payload, and the wire enum (`sim_schema::CultureLayerScope`) has no `Band` member. A
        // band's tensions still reach the sim through `take_tension_events` each turn.

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
        // `bands` is NOT cleared: band layers are not published, so a snapshot carries no evidence
        // about them and dropping them here would be a deletion on no evidence. A rollback restores
        // them through `restore_checkpoint`, which does carry them.

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
                // Unreachable today — `from_schema_scope` cannot produce `Band`, because the wire
                // enum has no such member. Kept total so adding one lands the layer in the right
                // map rather than silently in `locals`.
                CultureLayerScope::Band => {
                    self.bands.insert(state.owner, layer);
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

    /// **The faction-level culture rollup**: a population-weighted average of the layers the
    /// faction's bands carry, per trait axis.
    ///
    /// Reads the **band** map (`CultureOwner::from_band`), so a faction's culture is the aggregate
    /// of the bands that live it — weight belongs on population, because a 200-person band should
    /// speak louder than a 12-person one. Callers pass **resident** bands only (an expedition is
    /// detached and doesn't vote).
    ///
    /// Falls back to the global layer for any faction with no band layers (or no people), which
    /// is the reading `The Telling`'s `culture.axis.*` signals used before this existed.
    pub fn faction_trait_average(
        &self,
        bands: &[(CultureOwner, u32)],
    ) -> [f32; CULTURE_TRAIT_AXES] {
        let mut totals = [0.0f32; CULTURE_TRAIT_AXES];
        let mut weight_total = 0.0f32;
        for (owner, people) in bands {
            let (Some(layer), weight) = (self.band_layer_by_owner(*owner), *people as f32) else {
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

/// Spreads consecutive band ids apart before they are folded into a wave — the single-id twin of
/// the tile seed's `x * 31 + y * 17`.
const BAND_SEED_STRIDE: u64 = 31;

/// Decorrelates the axes of one band, so a band is not simply the same offset fifteen times.
const BAND_AXIS_STRIDE: i64 = 13;

/// Fold period of the wave. Prime, and coprime with both strides, so the pattern does not repeat
/// across the small band ids a campaign actually issues.
const BAND_WAVE_PERIOD: i64 = 23;

/// The per-band **character**: one offset per trait axis, symmetric about zero, a cheap
/// deterministic function of the band's durable id. Mirrors `seeded_modifiers_for_position` in
/// shape.
///
/// **This is load-bearing. Without a per-band modifier every band converges on its province and
/// they are all identical** — the faction rollup would degenerate into a population-weighted
/// average of provinces, and no band could ever diverge from its parent far enough to schism. The
/// offset is what makes two bands standing in one province two cultures.
///
/// `amplitude` is the config lever `culture.propagation.band_character_amplitude`: it sets how far
/// bands drift and therefore how often schism fires.
pub fn seeded_modifiers_for_band(band: BandId, amplitude: f32) -> [Scalar; CULTURE_TRAIT_AXES] {
    let mut modifiers = [Scalar::zero(); CULTURE_TRAIT_AXES];
    let seed = (band.0.wrapping_mul(BAND_SEED_STRIDE) % BAND_WAVE_PERIOD as u64) as i64;
    // Centre the fold so the wave straddles zero: `0..PERIOD-1` minus half a period.
    let centre = BAND_WAVE_PERIOD / 2;
    for (idx, slot) in modifiers.iter_mut().enumerate() {
        let wave = ((seed + idx as i64 * BAND_AXIS_STRIDE) % BAND_WAVE_PERIOD) - centre;
        let scaled = (wave as f32 / BAND_WAVE_PERIOD as f32).clamp(-1.0, 1.0) * amplitude;
        *slot = scalar_from_f32(scaled);
    }
    modifiers
}

/// **The one resolution from a map position to the region a culture layer parents on.** A tile off
/// every province — or a world with no [`ProvinceMap`] at all — falls back to
/// [`FALLBACK_CULTURE_REGION_ID`], which worldgen always mints.
///
/// Shared by [`reconcile_band_culture_layers`] (every turn, for the band that stands there) and by
/// band fission ([`crate::systems::split_band_from_parent`], for the tile the split happened on), so
/// a new band's province and its first reconcile's province can never disagree.
pub fn culture_region_at(province_map: Option<&ProvinceMap>, position: UVec2) -> u32 {
    province_map
        .and_then(|map| map.province_at(position.x, position.y))
        .unwrap_or(FALLBACK_CULTURE_REGION_ID)
}

/// Keeps the band culture layers in step with the live set of **resident** bands, ahead of
/// [`reconcile_culture_layers`] each turn.
///
/// Three cases, and they are the whole system: a resident band with no layer gets one parented to
/// its current province, a layer whose band is gone (or has become an expedition) is dropped, and a
/// resident band standing in a different province than its layer's parent is re-homed — traits
/// intact, which is what makes a migration lag.
///
/// **A band split off another arrives with its layer already attached** — `split_band_from_parent`
/// calls `CultureManager::attach_band_from_source` at the split, so the "no layer" case here never
/// sees it and a splinter is never seeded from the province rather than from its parent.
///
/// **Only `ResidentBand` owns a layer.** An expedition is detached and does not vote in the faction
/// rollup (`faction_trait_average`'s contract), so it must not carry a culture either.
///
/// It runs in `TurnStage::Influence`, before `advance_band_movement` in `Population`, so the
/// province a band is reconciled against is the one it *starts* the turn on. Worldgen-spawned bands
/// therefore get their layers on the first `Update`, before the turn's snapshot is captured.
pub fn reconcile_band_culture_layers(
    mut manager: ResMut<CultureManager>,
    province_map: Option<Res<ProvinceMap>>,
    bands: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
    tiles: Query<&Tile>,
) {
    let mut live: Vec<u64> = Vec::with_capacity(bands.iter().len());
    for (band, cohort) in bands.iter() {
        // Liveness is decided by the query alone, ahead of the tile resolve: a band left out of
        // `live` is read by the stale sweep below as a dead band and has its layer detached, and
        // re-attachment reseeds traits from the province rather than restoring the drift, timers
        // and divergence the band had accumulated. A band whose tile does not resolve is therefore
        // simply not re-homed this turn — its layer is left exactly as it stands. The reverse
        // order is harmless: the sweep only removes layers that exist, so a live band with no
        // layer yet costs nothing.
        live.push(band.0);
        let Ok(tile) = tiles.get(cohort.current_tile) else {
            continue;
        };
        let region_id = culture_region_at(province_map.as_deref(), tile.position);
        let parent = manager.upsert_regional(region_id);

        let current_parent = manager
            .band_layer_by_owner(CultureOwner::from_band(*band))
            .map(|layer| layer.parent);
        match current_parent {
            None => {
                manager.attach_band(*band, parent);
            }
            Some(existing) if existing != Some(parent) => {
                manager.set_band_parent(*band, parent);
            }
            Some(_) => {}
        }
    }

    let stale: Vec<BandId> = manager
        .band_layers()
        .filter(|layer| !live.contains(&layer.owner.0))
        .map(|layer| BandId(layer.owner.0))
        .collect();
    for band in stale {
        manager.detach_band(band);
    }
}

/// **The ancestor pull** — each resident band's per-axis target offset `tie × ancestor_pull[axis]`,
/// keyed by the band's culture owner key (`.claude/rules/core_sim/belief.md` → "The ancestor pull").
/// The tie `s × r` is read from what the band already stores: its `belief_anchor` (for `s`) and
/// the hop count the culture term was priced from, `last_belief_relay_hops` (for `r`), through the
/// one formula (`CultureConfig::ancestor_tie`). A band with no tie is left out of the map.
/// Stateless: the offset is recomputed every turn and nothing accumulates.
pub fn band_ancestor_pulls<'a>(
    bands: impl IntoIterator<Item = (&'a BandId, &'a PopulationCohort)>,
    belief: &BeliefRegistry,
    culture: &crate::wellbeing_config::CultureConfig,
    full_tie_pull: &[f32; CULTURE_TRAIT_AXES],
) -> BTreeMap<u64, [Scalar; CULTURE_TRAIT_AXES]> {
    let mut pulls = BTreeMap::new();
    for (band, cohort) in bands {
        let hops = crate::belief_relay::hops_from_wire(
            cohort.belief_anchor,
            cohort.last_belief_relay_hops,
        );
        let tie = culture.ancestor_tie(cohort.belief_anchor.map(|anchor| belief.get(anchor)), hops);
        // No tie, or a lever that names no axis, is no pull: the band is left out, not given zeros.
        if tie <= 0.0 || full_tie_pull.iter().all(|offset| *offset == 0.0) {
            continue;
        }
        pulls.insert(
            CultureOwner::from_band(*band).0,
            full_tie_pull.map(|offset| scalar_from_f32(tie * offset)),
        );
    }
    pulls
}

/// **Every mutual tie between two of the given resident bands**, at the weaker of its two directed
/// strengths, in `(a, b)` order with `a < b`. A one-way tie, a parked edge (strength zero) on
/// either side, and a tie to a band not in `residents` are all left out.
pub fn mutual_contact_ties(
    ledger: &ConnectionLedger,
    residents: &BTreeMap<u64, Scalar>,
) -> Vec<ContactTie> {
    let mut ties = Vec::new();
    for (key, forward) in ledger.iter() {
        if key.observer >= key.subject
            || !residents.contains_key(&key.observer.0)
            || !residents.contains_key(&key.subject.0)
        {
            continue;
        }
        let Some(back) = ledger.get(&crate::connections::ConnectionKey::new(
            key.subject,
            key.observer,
        )) else {
            continue;
        };
        let tie = forward.strength.min(back.strength);
        if tie > Scalar::zero() {
            ties.push(ContactTie {
                a: key.observer,
                b: key.subject,
                tie,
            });
        }
    }
    ties
}

/// **The bands whose held schism asks to split off as their own people** (#702), recorded by
/// [`reconcile_culture_layers`] in `TurnStage::Influence` and drained by
/// `systems::advance_culture_splits` in the Population chain the same turn. Each entry is the band
/// and the people it belonged to when culture judged it, so a band independence (or a defection)
/// already moved this turn is recognised and skipped.
#[derive(Resource, Debug, Default, Clone)]
pub struct CultureSplitQueue {
    pub bands: BTreeMap<BandId, FactionId>,
}

/// **The one rule for "this band may break away"** (#702), shared by the split queue and the
/// published warning so they cannot disagree: the band's Syncretic<->Purist value is above
/// `split_min_purist` AND its people has at least one other resident band.
pub fn may_break_away(purist: f32, split_min_purist: f32, has_sibling: bool) -> bool {
    purist > split_min_purist && has_sibling
}

/// System wrapper that performs the reconcile pass each turn.
#[allow(clippy::too_many_arguments)]
pub fn reconcile_culture_layers(
    mut manager: ResMut<CultureManager>,
    tick: Res<SimulationTick>,
    mut effects: ResMut<CultureEffectsCache>,
    mut tension_writer: EventWriter<CultureTensionEvent>,
    mut schism_writer: EventWriter<CultureSchismEvent>,
    impacts: Res<InfluencerImpacts>,
    bands: Query<(&BandId, &PopulationCohort), With<ResidentBand>>,
    belief: Res<BeliefRegistry>,
    wellbeing: Res<WellbeingConfigHandle>,
    belief_config: Res<BeliefConfigHandle>,
    connections: Res<ConnectionLedger>,
    culture_config: Res<CultureCorruptionConfigHandle>,
    mut splits: ResMut<CultureSplitQueue>,
) {
    let resonance = impacts.culture_resonance();
    // Contact drift writes the band modifiers BEFORE the reconcile, so this turn's resolve sees it.
    // It reads last turn's ledger (`advance_connections` runs after Influence).
    let drift_rate = culture_config.config().culture().contact_drift().rate();
    if drift_rate > 0.0 || !manager.applied_contact_pull.is_empty() {
        let weights: BTreeMap<u64, Scalar> = bands
            .iter()
            .map(|(band, cohort)| (band.0, cohort.total()))
            .collect();
        let factions: BTreeMap<u64, FactionId> = bands
            .iter()
            .map(|(band, cohort)| (band.0, cohort.faction))
            .collect();
        let ties = mutual_contact_ties(&connections, &weights);
        manager.apply_contact_drift(drift_rate, &factions, &weights, &ties);
    }
    let band_pull = band_ancestor_pulls(
        bands.iter(),
        &belief,
        &wellbeing.get().culture,
        &belief_config.get().ancestor_pull_vector(),
    );
    manager.reconcile(&tick, &resonance, &band_pull);
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

    // **A purist band whose strain held past the hard threshold asks to split off** (#702). An
    // accepting band (purist at or below the lever) absorbs it and stays; a people of one band has
    // nothing to break away from.
    splits.bands.clear();
    let min_purist = culture_config.config().culture().split_min_purist();
    let may_break = |manager: &CultureManager, band: BandId| -> Option<FactionId> {
        let layer = manager.band_layer_by_owner(CultureOwner::from_band(band))?;
        let purist = layer.traits.values()[CultureTraitAxis::SyncreticPurist.index()].to_f32();
        let (_, cohort) = bands.iter().find(|(id, _)| **id == band)?;
        let has_sibling = bands
            .iter()
            .any(|(id, other)| *id != band && other.faction == cohort.faction);
        may_break_away(purist, min_purist, has_sibling).then_some(cohort.faction)
    };
    // **Both read one level state and one predicate**: the warning is any band in the warning or
    // hard state that `may_break`; the split is any band in the hard state that `may_break`, every
    // turn it stays there. The `SchismRisk` record is an edge (it fires once, on crossing) and is
    // not what queues a split — a band that crossed hard while it could not break away, and can
    // later, must still go.
    let at_risk: BTreeSet<u64> = manager
        .bands_in_strain(false)
        .into_iter()
        .filter(|owner| may_break(&manager, BandId(*owner)).is_some())
        .collect();
    manager.break_away_risk = at_risk;
    for owner in manager.bands_in_strain(true) {
        let band = BandId(owner);
        if let Some(faction) = may_break(&manager, band) {
            splits.bands.insert(band, faction);
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

        manager.reconcile(&SimulationTick(1), &resonance, &BTreeMap::new());
        assert!(
            manager.take_tension_events().is_empty(),
            "drift event should wait for trigger ticks"
        );

        manager.reconcile(&SimulationTick(2), &resonance, &BTreeMap::new());
        assert!(
            manager.take_tension_events().is_empty(),
            "drift event should still wait for trigger ticks"
        );

        manager.reconcile(&SimulationTick(3), &resonance, &BTreeMap::new());
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

        manager.reconcile(&SimulationTick(1), &resonance, &BTreeMap::new());
        let first_events = manager.take_tension_events();
        assert_eq!(first_events.len(), 1);
        assert_eq!(first_events[0].kind, CultureTensionKind::DriftWarning);

        manager.reconcile(&SimulationTick(2), &resonance, &BTreeMap::new());
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

        manager.reconcile(&SimulationTick(1), &resonance, &BTreeMap::new());
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

        manager.reconcile(&SimulationTick(2), &resonance, &BTreeMap::new());
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
            manager.reconcile(&SimulationTick(tick), &resonance, &BTreeMap::new());
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
