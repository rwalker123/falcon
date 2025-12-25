use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bevy::prelude::*;

use crate::{
    hashing::FnvHasher,
    metrics::SimulationMetrics,
    orders::FactionId,
    power::PowerDiscoveryEffects,
    resources::{DiplomacyLeverage, DiscoveryProgressLedger, PendingCrisisSeeds, SimulationTick},
    scalar::{scalar_one, scalar_zero, Scalar},
    CapabilityFlags,
};

use rand::{distributions::uniform::SampleUniform, rngs::SmallRng, Rng, SeedableRng};

use serde::Deserialize;
use sim_runtime::{
    GreatDiscoveryDefinitionState, GreatDiscoveryProgressState, GreatDiscoveryRequirementState,
    GreatDiscoveryState, GreatDiscoveryTelemetryState, KnowledgeField,
};
use thiserror::Error;

pub mod effect_flags {
    pub const POWER: u32 = 1 << 0;
    pub const CRISIS: u32 = 1 << 1;
    pub const DIPLOMACY: u32 = 1 << 2;
    pub const FORCED_PUBLICATION: u32 = 1 << 3;
}

pub const BUILTIN_GREAT_DISCOVERY_CATALOG: &str =
    include_str!("data/great_discovery_definitions.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum NumericBand<T> {
    Scalar(T),
    Range { min: T, max: T },
}

impl<T> Default for NumericBand<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::Scalar(T::default())
    }
}

impl<T> NumericBand<T>
where
    T: Copy + PartialOrd,
{
    fn bounds(&self) -> (T, T) {
        let (min, max) = match self {
            NumericBand::Scalar(value) => (*value, *value),
            NumericBand::Range { min, max } => (*min, *max),
        };
        if min <= max {
            (min, max)
        } else {
            (max, min)
        }
    }
}

impl<T> NumericBand<T>
where
    T: Copy + PartialOrd + PartialEq + SampleUniform,
{
    fn sample(&self, rng: &mut SmallRng) -> T {
        let (min, max) = self.bounds();
        if min == max {
            min
        } else {
            rng.gen_range(min..=max)
        }
    }
}

#[derive(Debug, Error)]
pub enum GreatDiscoveryCatalogError {
    #[error("failed to parse Great Discovery catalog: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("duplicate Great Discovery definition id {id}")]
    DuplicateDefinition { id: u16 },
    #[error("unknown Great Discovery effect flag '{flag}' in definition {id}")]
    UnknownEffectFlag { id: u16, flag: String },
}

#[derive(Debug, Clone, Deserialize)]
struct GreatDiscoveryCatalogEntry {
    id: u16,
    name: String,
    field: KnowledgeField,
    #[serde(default)]
    requirements: Vec<GreatDiscoveryCatalogRequirement>,
    #[serde(default)]
    observation_threshold: NumericBand<u32>,
    #[serde(default)]
    cooldown_ticks: NumericBand<u16>,
    #[serde(default)]
    freshness_window: Option<NumericBand<u16>>,
    #[serde(default)]
    effect_flags: Vec<String>,
    #[serde(default)]
    effect_flag_bits: Option<u32>,
    #[serde(default)]
    covert_until_public: bool,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    effects_summary: Vec<String>,
    #[serde(default)]
    observation_notes: Option<String>,
    #[serde(default)]
    leak_profile: Option<String>,
    #[serde(default)]
    seed_offset: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GreatDiscoveryCatalogRequirement {
    discovery_id: u32,
    #[serde(default = "default_requirement_weight")]
    weight: NumericBand<f32>,
    #[serde(default)]
    minimum_progress: NumericBand<f32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[inline]
fn default_requirement_weight() -> NumericBand<f32> {
    NumericBand::Scalar(1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GreatDiscoveryId(pub u16);

impl GreatDiscoveryId {
    pub const fn new(id: u16) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone)]
pub struct ConstellationRequirement {
    pub discovery_id: u32,
    pub weight: Scalar,
    pub minimum_progress: Scalar,
}

impl ConstellationRequirement {
    pub fn new(discovery_id: u32, weight: Scalar, minimum_progress: Scalar) -> Self {
        Self {
            discovery_id,
            weight,
            minimum_progress,
        }
    }

    fn weight_or_one(&self) -> Scalar {
        if self.weight <= scalar_zero() {
            scalar_one()
        } else {
            self.weight
        }
    }
}

#[derive(Debug, Clone)]
pub struct GreatDiscoveryDefinition {
    pub id: GreatDiscoveryId,
    pub name: String,
    pub field: KnowledgeField,
    pub requirements: Vec<ConstellationRequirement>,
    pub observation_threshold: u32,
    pub cooldown_ticks: u16,
    pub freshness_window: Option<u16>,
    pub effect_flags: u32,
    pub covert_until_public: bool,
    weight_total: Scalar,
}

impl GreatDiscoveryDefinition {
    #[allow(clippy::too_many_arguments)] // Definition metadata fields are provided explicitly for clarity.
    pub fn new(
        id: GreatDiscoveryId,
        name: impl Into<String>,
        field: KnowledgeField,
        requirements: Vec<ConstellationRequirement>,
        observation_threshold: u32,
        cooldown_ticks: u16,
        freshness_window: Option<u16>,
        effect_flags: u32,
        covert_until_public: bool,
    ) -> Self {
        let mut weight_total = scalar_zero();
        for req in &requirements {
            weight_total += req.weight_or_one();
        }
        if weight_total <= scalar_zero() {
            weight_total = scalar_one();
        }
        Self {
            id,
            name: name.into(),
            field,
            requirements,
            observation_threshold,
            cooldown_ticks,
            freshness_window,
            effect_flags,
            covert_until_public,
            weight_total,
        }
    }

    pub fn weight_total(&self) -> Scalar {
        self.weight_total
    }
}

fn collect_effect_flags(
    id: u16,
    names: &[String],
    bits: Option<u32>,
) -> Result<u32, GreatDiscoveryCatalogError> {
    let mut value = bits.unwrap_or(0);
    for name in names {
        value |= parse_effect_flag(id, name)?;
    }
    Ok(value)
}

fn parse_effect_flag(id: u16, flag: &str) -> Result<u32, GreatDiscoveryCatalogError> {
    let normalized = flag.trim().to_ascii_uppercase().replace(['-', ' '], "_");
    match normalized.as_str() {
        "POWER" => Ok(effect_flags::POWER),
        "CRISIS" => Ok(effect_flags::CRISIS),
        "DIPLOMACY" => Ok(effect_flags::DIPLOMACY),
        "FORCED_PUBLICATION" | "FORCEDPUBLICATION" => Ok(effect_flags::FORCED_PUBLICATION),
        other => Err(GreatDiscoveryCatalogError::UnknownEffectFlag {
            id,
            flag: other.to_owned(),
        }),
    }
}

fn resolve_catalog_entry(
    entry: &GreatDiscoveryCatalogEntry,
) -> Result<(GreatDiscoveryDefinition, GreatDiscoveryDefinitionMetadata), GreatDiscoveryCatalogError>
{
    let effect_flags = collect_effect_flags(entry.id, &entry.effect_flags, entry.effect_flag_bits)?;
    let id = GreatDiscoveryId(entry.id);
    let seed = hash_identifier(&entry.id) ^ entry.seed_offset.unwrap_or(0);
    let mut rng = SmallRng::seed_from_u64(seed);

    let observation_threshold = entry.observation_threshold.sample(&mut rng);
    let cooldown_ticks = entry.cooldown_ticks.sample(&mut rng);
    let freshness_window = entry
        .freshness_window
        .as_ref()
        .map(|band| band.sample(&mut rng));

    let mut requirement_defs = Vec::with_capacity(entry.requirements.len());
    let mut requirement_meta = Vec::with_capacity(entry.requirements.len());
    for requirement in &entry.requirements {
        let mut weight = requirement.weight.sample(&mut rng);
        if weight <= 0.0 {
            weight = 1.0;
        }

        let minimum = requirement
            .minimum_progress
            .sample(&mut rng)
            .clamp(0.0, 1.0);

        requirement_defs.push(ConstellationRequirement::new(
            requirement.discovery_id,
            Scalar::from_f32(weight),
            Scalar::from_f32(minimum),
        ));
        requirement_meta.push(GreatDiscoveryRequirementMetadata {
            discovery_id: requirement.discovery_id,
            name: requirement.name.clone(),
            summary: requirement.summary.clone(),
            weight,
            minimum_progress: minimum,
        });
    }

    let definition = GreatDiscoveryDefinition::new(
        id,
        entry.name.clone(),
        entry.field,
        requirement_defs,
        observation_threshold,
        cooldown_ticks,
        freshness_window,
        effect_flags,
        entry.covert_until_public,
    );

    let metadata = GreatDiscoveryDefinitionMetadata {
        id,
        name: entry.name.clone(),
        field: entry.field,
        tier: entry.tier.clone(),
        summary: entry.summary.clone(),
        tags: entry.tags.clone(),
        observation_threshold,
        cooldown_ticks,
        freshness_window,
        effect_flags,
        covert_until_public: entry.covert_until_public,
        effects_summary: entry.effects_summary.clone(),
        observation_notes: entry.observation_notes.clone(),
        leak_profile: entry.leak_profile.clone(),
        requirements: requirement_meta,
    };

    Ok((definition, metadata))
}

fn hash_identifier<T: Hash>(identifier: &T) -> u64 {
    let mut hasher = FnvHasher::new();
    identifier.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone)]
pub struct GreatDiscoveryRequirementMetadata {
    pub discovery_id: u32,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub weight: f32,
    pub minimum_progress: f32,
}

#[derive(Debug, Clone)]
pub struct GreatDiscoveryDefinitionMetadata {
    pub id: GreatDiscoveryId,
    pub name: String,
    pub field: KnowledgeField,
    pub tier: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub observation_threshold: u32,
    pub cooldown_ticks: u16,
    pub freshness_window: Option<u16>,
    pub effect_flags: u32,
    pub covert_until_public: bool,
    pub effects_summary: Vec<String>,
    pub observation_notes: Option<String>,
    pub leak_profile: Option<String>,
    pub requirements: Vec<GreatDiscoveryRequirementMetadata>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GreatDiscoveryRegistry {
    definitions: HashMap<GreatDiscoveryId, GreatDiscoveryDefinition>,
    metadata: HashMap<GreatDiscoveryId, GreatDiscoveryDefinitionMetadata>,
}

impl GreatDiscoveryRegistry {
    pub fn load_catalog_from_str(
        &mut self,
        catalog: &str,
    ) -> Result<usize, GreatDiscoveryCatalogError> {
        let entries: Vec<GreatDiscoveryCatalogEntry> = serde_json::from_str(catalog)?;
        let mut added = 0;
        for entry in entries {
            let id = GreatDiscoveryId(entry.id);
            if self.definitions.contains_key(&id) {
                return Err(GreatDiscoveryCatalogError::DuplicateDefinition { id: entry.id });
            }
            let (definition, metadata) = resolve_catalog_entry(&entry)?;
            self.register(definition);
            self.metadata.insert(id, metadata);
            added += 1;
        }
        Ok(added)
    }

    pub fn register(&mut self, definition: GreatDiscoveryDefinition) {
        self.definitions.insert(definition.id, definition);
    }

    pub fn definition(&self, id: &GreatDiscoveryId) -> Option<&GreatDiscoveryDefinition> {
        self.definitions.get(id)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &GreatDiscoveryDefinition> {
        self.definitions.values()
    }

    pub fn metadata(&self, id: &GreatDiscoveryId) -> Option<&GreatDiscoveryDefinitionMetadata> {
        self.metadata.get(id)
    }

    pub fn metadata_entries(&self) -> impl Iterator<Item = &GreatDiscoveryDefinitionMetadata> {
        self.metadata.values()
    }

    pub fn restore_from_states(&mut self, states: &[GreatDiscoveryDefinitionState]) {
        self.definitions.clear();
        self.metadata.clear();

        for state in states {
            let mut requirement_defs = Vec::with_capacity(state.requirements.len());
            let mut requirement_meta = Vec::with_capacity(state.requirements.len());

            for req in &state.requirements {
                let weight = if req.weight <= 0.0 { 1.0 } else { req.weight };
                let minimum = req.minimum_progress.clamp(0.0, 1.0);
                requirement_defs.push(ConstellationRequirement::new(
                    req.discovery,
                    Scalar::from_f32(weight),
                    Scalar::from_f32(minimum),
                ));
                requirement_meta.push(GreatDiscoveryRequirementMetadata {
                    discovery_id: req.discovery,
                    name: req.name.clone(),
                    summary: req.summary.clone(),
                    weight,
                    minimum_progress: minimum,
                });
            }

            let id = GreatDiscoveryId(state.id);
            let definition = GreatDiscoveryDefinition::new(
                id,
                state.name.clone(),
                state.field,
                requirement_defs,
                state.observation_threshold,
                state.cooldown_ticks,
                state.freshness_window,
                state.effect_flags,
                state.covert_until_public,
            );

            let metadata = GreatDiscoveryDefinitionMetadata {
                id,
                name: state.name.clone(),
                field: state.field,
                tier: state.tier.clone(),
                summary: state.summary.clone(),
                tags: state.tags.clone(),
                observation_threshold: state.observation_threshold,
                cooldown_ticks: state.cooldown_ticks,
                freshness_window: state.freshness_window,
                effect_flags: state.effect_flags,
                covert_until_public: state.covert_until_public,
                effects_summary: state.effects_summary.clone(),
                observation_notes: state.observation_notes.clone(),
                leak_profile: state.leak_profile.clone(),
                requirements: requirement_meta,
            };

            self.definitions.insert(id, definition);
            self.metadata.insert(id, metadata);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConstellationProgress {
    pub progress: Scalar,
    pub observation_deficit: u32,
    pub last_progress_tick: u64,
    pub cooldown_remaining: u16,
    pub resolved: bool,
    pub covert: bool,
}

impl ConstellationProgress {
    fn new(covert: bool) -> Self {
        Self {
            progress: scalar_zero(),
            observation_deficit: 0,
            last_progress_tick: 0,
            cooldown_remaining: 0,
            resolved: false,
            covert,
        }
    }

    fn eta_ticks(&self) -> u32 {
        if self.resolved || self.progress >= scalar_one() {
            return 0;
        }
        let remaining = (scalar_one() - self.progress).to_f32();
        (remaining * 10.0).ceil().max(0.0) as u32
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GreatDiscoveryReadiness {
    per_faction: HashMap<FactionId, HashMap<GreatDiscoveryId, ConstellationProgress>>,
}

impl GreatDiscoveryReadiness {
    fn entry_mut(
        &mut self,
        faction: FactionId,
        definition: &GreatDiscoveryDefinition,
    ) -> &mut ConstellationProgress {
        let faction_entry = self.per_faction.entry(faction).or_default();
        faction_entry
            .entry(definition.id)
            .or_insert_with(|| ConstellationProgress::new(definition.covert_until_public))
    }

    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (FactionId, &HashMap<GreatDiscoveryId, ConstellationProgress>)> {
        self.per_faction
            .iter()
            .map(|(faction, map)| (*faction, map))
    }

    pub(crate) fn iter_mut(
        &mut self,
    ) -> impl Iterator<
        Item = (
            FactionId,
            &mut HashMap<GreatDiscoveryId, ConstellationProgress>,
        ),
    > {
        self.per_faction
            .iter_mut()
            .map(|(faction, map)| (*faction, map))
    }

    pub fn rebuild_from_states(
        &mut self,
        registry: &GreatDiscoveryRegistry,
        progress_states: &[GreatDiscoveryProgressState],
    ) {
        self.per_faction.clear();
        for state in progress_states {
            let faction = FactionId(state.faction);
            let id = GreatDiscoveryId(state.discovery);
            let covert = registry
                .definition(&id)
                .map(|def| def.covert_until_public)
                .unwrap_or(state.covert);
            let entry = self.per_faction.entry(faction).or_default();
            let mut progress = ConstellationProgress::new(covert);
            progress.progress = Scalar::from_raw(state.progress);
            progress.observation_deficit = state.observation_deficit;
            progress.covert = state.covert;
            entry.insert(id, progress);
        }

        for (_faction, entries) in self.per_faction.iter_mut() {
            for definition in registry.definitions() {
                entries
                    .entry(definition.id)
                    .or_insert_with(|| ConstellationProgress::new(definition.covert_until_public));
            }
            entries
                .values_mut()
                .for_each(|entry| entry.cooldown_remaining = 0);
        }
    }

    pub fn mark_resolved(&mut self, faction: FactionId, id: GreatDiscoveryId) {
        if let Some(entry) = self
            .per_faction
            .get_mut(&faction)
            .and_then(|map| map.get_mut(&id))
        {
            entry.resolved = true;
            entry.progress = scalar_one();
            entry.cooldown_remaining = 0;
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ObservationFieldState {
    counts: HashMap<KnowledgeField, u32>,
}

impl ObservationFieldState {
    fn add(&mut self, field: KnowledgeField, amount: u32) {
        let entry = self.counts.entry(field).or_default();
        *entry = entry.saturating_add(amount);
    }

    fn set(&mut self, field: KnowledgeField, value: u32) {
        self.counts.insert(field, value);
    }

    fn total(&self, field: KnowledgeField) -> u32 {
        self.counts.get(&field).copied().unwrap_or(0)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ObservationLedger {
    entries: HashMap<FactionId, ObservationFieldState>,
}

impl ObservationLedger {
    pub fn add_observations(&mut self, faction: FactionId, field: KnowledgeField, amount: u32) {
        if amount == 0 {
            return;
        }
        self.entries.entry(faction).or_default().add(field, amount);
    }

    pub fn set_observations(&mut self, faction: FactionId, field: KnowledgeField, value: u32) {
        self.entries.entry(faction).or_default().set(field, value);
    }

    pub fn total_for(&self, faction: FactionId, field: KnowledgeField) -> u32 {
        self.entries
            .get(&faction)
            .map(|state| state.total(field))
            .unwrap_or(0)
    }

    pub fn factions(&self) -> impl Iterator<Item = FactionId> + '_ {
        self.entries.keys().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreatDiscoveryRecord {
    pub id: GreatDiscoveryId,
    pub faction: FactionId,
    pub field: KnowledgeField,
    pub tick: u64,
    pub publicly_deployed: bool,
    pub effect_flags: u32,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GreatDiscoveryLedger {
    records: Vec<GreatDiscoveryRecord>,
    index: HashSet<(FactionId, GreatDiscoveryId)>,
}

impl GreatDiscoveryLedger {
    pub fn push(&mut self, record: GreatDiscoveryRecord) {
        if self.index.insert((record.faction, record.id)) {
            self.records.push(record);
        }
    }

    pub fn contains(&self, faction: FactionId, id: GreatDiscoveryId) -> bool {
        self.index.contains(&(faction, id))
    }

    pub fn records(&self) -> &[GreatDiscoveryRecord] {
        &self.records
    }

    pub fn mark_public(&mut self, faction: FactionId, id: GreatDiscoveryId) {
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.faction == faction && record.id == id)
        {
            record.publicly_deployed = true;
        }
    }

    pub fn replace_with_states(&mut self, states: &[GreatDiscoveryState]) {
        self.records.clear();
        self.index.clear();
        for state in states {
            let record = GreatDiscoveryRecord {
                id: GreatDiscoveryId(state.id),
                faction: FactionId(state.faction),
                field: state.field,
                tick: state.tick,
                publicly_deployed: state.publicly_deployed,
                effect_flags: state.effect_flags,
            };
            self.records.push(record.clone());
            self.index.insert((record.faction, record.id));
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct GreatDiscoveryFlag {
    pub id: GreatDiscoveryId,
    pub faction: FactionId,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct GreatDiscoveryTelemetry {
    pub pending_candidates: u32,
    pub active_constellations: u32,
}

impl GreatDiscoveryTelemetry {
    pub fn set_from_state(&mut self, state: &GreatDiscoveryTelemetryState) {
        self.pending_candidates = state.pending_candidates;
        self.active_constellations = state.active_constellations;
    }
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GreatDiscoveryCandidateEvent {
    pub faction: FactionId,
    pub discovery: GreatDiscoveryId,
}

#[derive(Event, Debug, Clone)]
pub struct GreatDiscoveryResolvedEvent {
    pub record: GreatDiscoveryRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GreatDiscoveryEffectKind {
    Power,
    Crisis,
    Diplomacy,
}

#[derive(Event, Debug, Clone)]
pub struct GreatDiscoveryEffectEvent {
    pub kind: GreatDiscoveryEffectKind,
    pub record: GreatDiscoveryRecord,
}

pub fn apply_capability_effects(
    mut flags: ResMut<CapabilityFlags>,
    mut effects: EventReader<GreatDiscoveryEffectEvent>,
) {
    for event in effects.read() {
        match event.kind {
            GreatDiscoveryEffectKind::Power => {
                flags.insert(CapabilityFlags::POWER);
            }
            GreatDiscoveryEffectKind::Crisis => {
                flags.insert(CapabilityFlags::MEGAPROJECTS);
            }
            GreatDiscoveryEffectKind::Diplomacy => {
                flags.insert(CapabilityFlags::ESPIONAGE_T2);
            }
        }
    }
}
pub fn collect_observation_signals(
    mut readiness: ResMut<GreatDiscoveryReadiness>,
    registry: Res<GreatDiscoveryRegistry>,
) {
    for (_, progresses) in readiness.iter_mut() {
        for progress in progresses.values_mut() {
            if progress.cooldown_remaining > 0 {
                progress.cooldown_remaining = progress.cooldown_remaining.saturating_sub(1);
            }
        }
        for definition in registry.definitions() {
            progresses
                .entry(definition.id)
                .or_insert_with(|| ConstellationProgress::new(definition.covert_until_public));
        }
    }
}

pub fn update_constellation_progress(
    registry: Res<GreatDiscoveryRegistry>,
    discovery_progress: Res<DiscoveryProgressLedger>,
    observation: Res<ObservationLedger>,
    mut readiness: ResMut<GreatDiscoveryReadiness>,
    mut telemetry: ResMut<GreatDiscoveryTelemetry>,
    tick: Res<SimulationTick>,
) {
    let mut factions: HashSet<FactionId> = HashSet::new();
    factions.extend(discovery_progress.progress.keys().copied());
    factions.extend(observation.factions());
    factions.extend(readiness.per_faction.keys().copied());

    let mut active = 0u32;

    for faction in factions {
        for definition in registry.definitions() {
            let state = readiness.entry_mut(faction, definition);
            if state.resolved {
                continue;
            }

            let progress = evaluate_constellation(definition, faction, &discovery_progress);
            if progress > state.progress {
                state.last_progress_tick = tick.0;
            }
            state.progress = progress;
            let observed = observation.total_for(faction, definition.field);
            state.observation_deficit = definition.observation_threshold.saturating_sub(observed);

            if state.progress > scalar_zero() {
                active = active.saturating_add(1);
            }

            if let Some(window) = definition.freshness_window {
                if tick.0.saturating_sub(state.last_progress_tick) > window as u64 {
                    state.progress = scalar_zero();
                }
            }
        }
    }

    telemetry.active_constellations = active;
}

pub fn screen_great_discovery_candidates(
    registry: Res<GreatDiscoveryRegistry>,
    readiness: Res<GreatDiscoveryReadiness>,
    ledger: Res<GreatDiscoveryLedger>,
    mut telemetry: ResMut<GreatDiscoveryTelemetry>,
    mut events: EventWriter<GreatDiscoveryCandidateEvent>,
) {
    let mut pending = 0u32;
    let mut factions: Vec<_> = readiness.per_faction.keys().copied().collect();
    factions.sort();

    for faction in factions {
        let entries = &readiness.per_faction[&faction];
        let mut discoveries: Vec<_> = entries.keys().copied().collect();
        discoveries.sort();

        for id in discoveries {
            let progress = &entries[&id];
            let Some(_definition) = registry.definition(&id) else {
                continue;
            };
            if progress.resolved
                || progress.cooldown_remaining > 0
                || progress.observation_deficit > 0
                || progress.progress < scalar_one()
                || ledger.contains(faction, id)
            {
                continue;
            }

            pending = pending.saturating_add(1);
            events.send(GreatDiscoveryCandidateEvent {
                faction,
                discovery: id,
            });
        }
    }
    telemetry.pending_candidates = pending;
}

#[allow(clippy::too_many_arguments)] // Bevy system signature pulls required ECS resources/events.
pub fn resolve_great_discovery(
    registry: Res<GreatDiscoveryRegistry>,
    mut readiness: ResMut<GreatDiscoveryReadiness>,
    mut ledger: ResMut<GreatDiscoveryLedger>,
    mut events: EventReader<GreatDiscoveryCandidateEvent>,
    mut resolved_writer: EventWriter<GreatDiscoveryResolvedEvent>,
    mut effect_writer: EventWriter<GreatDiscoveryEffectEvent>,
    mut power_effects: ResMut<PowerDiscoveryEffects>,
    mut crisis_seeds: ResMut<PendingCrisisSeeds>,
    mut diplomacy: ResMut<DiplomacyLeverage>,
    tick: Res<SimulationTick>,
) {
    for event in events.read() {
        let Some(definition) = registry.definition(&event.discovery) else {
            continue;
        };
        if ledger.contains(event.faction, event.discovery) {
            continue;
        }

        let record = GreatDiscoveryRecord {
            id: event.discovery,
            faction: event.faction,
            field: definition.field,
            tick: tick.0,
            publicly_deployed: false,
            effect_flags: definition.effect_flags,
        };
        ledger.push(record.clone());

        if let Some(faction_entry) = readiness.per_faction.get_mut(&event.faction) {
            if let Some(progress) = faction_entry.get_mut(&event.discovery) {
                progress.resolved = true;
                progress.progress = scalar_one();
                progress.cooldown_remaining = definition.cooldown_ticks;
            }
        }

        if definition.effect_flags & effect_flags::POWER != 0 && power_effects.register(record.id) {
            effect_writer.send(GreatDiscoveryEffectEvent {
                kind: GreatDiscoveryEffectKind::Power,
                record: record.clone(),
            });
        }

        if definition.effect_flags & effect_flags::CRISIS != 0 {
            crisis_seeds.push(record.faction, record.id.0);
            effect_writer.send(GreatDiscoveryEffectEvent {
                kind: GreatDiscoveryEffectKind::Crisis,
                record: record.clone(),
            });
        }

        if definition.effect_flags & effect_flags::DIPLOMACY != 0 {
            diplomacy.push_great_discovery(record.faction, record.id.0);
            effect_writer.send(GreatDiscoveryEffectEvent {
                kind: GreatDiscoveryEffectKind::Diplomacy,
                record: record.clone(),
            });
        }

        resolved_writer.send(GreatDiscoveryResolvedEvent { record });
    }
}

pub fn propagate_diffusion_impacts(
    registry: Res<GreatDiscoveryRegistry>,
    mut resolved_events: EventReader<GreatDiscoveryResolvedEvent>,
    mut discovery_progress: ResMut<DiscoveryProgressLedger>,
    mut ledger: ResMut<GreatDiscoveryLedger>,
) {
    for event in resolved_events.read() {
        if let Some(definition) = registry.definition(&event.record.id) {
            if definition.effect_flags & effect_flags::FORCED_PUBLICATION != 0 {
                ledger.mark_public(event.record.faction, event.record.id);
            }

            for requirement in &definition.requirements {
                discovery_progress.add_progress(
                    event.record.faction,
                    requirement.discovery_id,
                    scalar_one(),
                );
            }
        }
    }
}

pub fn export_great_discovery_metrics(
    ledger: Res<GreatDiscoveryLedger>,
    telemetry: Res<GreatDiscoveryTelemetry>,
    metrics: Option<ResMut<SimulationMetrics>>,
) {
    if let Some(mut metrics) = metrics {
        metrics.great_discoveries_total = ledger.records.len() as u32;
        metrics.great_discovery_candidates = telemetry.pending_candidates;
        metrics.great_discovery_active = telemetry.active_constellations;
    }
}

pub fn snapshot_discoveries(ledger: &GreatDiscoveryLedger) -> Vec<GreatDiscoveryState> {
    let mut states: Vec<GreatDiscoveryState> = ledger
        .records()
        .iter()
        .map(|record| GreatDiscoveryState {
            id: record.id.0,
            faction: record.faction.0,
            field: record.field,
            tick: record.tick,
            publicly_deployed: record.publicly_deployed,
            effect_flags: record.effect_flags,
        })
        .collect();
    states.sort_unstable_by(|a, b| (a.faction, a.id).cmp(&(b.faction, b.id)));
    states
}

pub fn snapshot_progress(readiness: &GreatDiscoveryReadiness) -> Vec<GreatDiscoveryProgressState> {
    let mut states: Vec<GreatDiscoveryProgressState> = Vec::new();
    for (faction, entries) in readiness.iter() {
        for (id, progress) in entries {
            if progress.resolved {
                continue;
            }
            states.push(GreatDiscoveryProgressState {
                faction: faction.0,
                discovery: id.0,
                progress: progress.progress.raw(),
                observation_deficit: progress.observation_deficit,
                eta_ticks: progress.eta_ticks(),
                covert: progress.covert,
            });
        }
    }
    states.sort_unstable_by(|a, b| (a.faction, a.discovery).cmp(&(b.faction, b.discovery)));
    states
}

pub fn snapshot_definitions(
    registry: &GreatDiscoveryRegistry,
) -> Vec<GreatDiscoveryDefinitionState> {
    let mut states: Vec<GreatDiscoveryDefinitionState> = registry
        .metadata_entries()
        .map(|meta| {
            let requirements: Vec<GreatDiscoveryRequirementState> = meta
                .requirements
                .iter()
                .map(|req| GreatDiscoveryRequirementState {
                    discovery: req.discovery_id,
                    weight: req.weight,
                    minimum_progress: req.minimum_progress,
                    name: req.name.clone(),
                    summary: req.summary.clone(),
                })
                .collect();
            GreatDiscoveryDefinitionState {
                id: meta.id.0,
                name: meta.name.clone(),
                field: meta.field,
                tier: meta.tier.clone(),
                summary: meta.summary.clone(),
                tags: meta.tags.clone(),
                observation_threshold: meta.observation_threshold,
                cooldown_ticks: meta.cooldown_ticks,
                freshness_window: meta.freshness_window,
                effect_flags: meta.effect_flags,
                covert_until_public: meta.covert_until_public,
                effects_summary: meta.effects_summary.clone(),
                observation_notes: meta.observation_notes.clone(),
                leak_profile: meta.leak_profile.clone(),
                requirements,
            }
        })
        .collect();
    states.sort_unstable_by_key(|state| state.id);
    states
}

pub fn snapshot_telemetry(
    ledger: &GreatDiscoveryLedger,
    telemetry: &GreatDiscoveryTelemetry,
) -> GreatDiscoveryTelemetryState {
    GreatDiscoveryTelemetryState {
        total_resolved: ledger.records.len() as u32,
        pending_candidates: telemetry.pending_candidates,
        active_constellations: telemetry.active_constellations,
    }
}

fn evaluate_constellation(
    definition: &GreatDiscoveryDefinition,
    faction: FactionId,
    ledger: &DiscoveryProgressLedger,
) -> Scalar {
    if definition.requirements.is_empty() {
        return scalar_one();
    }

    let mut accum = scalar_zero();
    for requirement in &definition.requirements {
        let weight = requirement.weight_or_one();
        let progress = ledger.get_progress(faction, requirement.discovery_id);
        if progress <= requirement.minimum_progress {
            continue;
        }
        let span = scalar_one() - requirement.minimum_progress;
        if span <= scalar_zero() {
            accum += weight;
            continue;
        }
        let delta = progress - requirement.minimum_progress;
        if delta <= scalar_zero() {
            continue;
        }
        let normalized = (delta.clamp(scalar_zero(), span)) / span;
        accum += (normalized * weight).clamp(scalar_zero(), weight);
    }

    (accum / definition.weight_total()).clamp(scalar_zero(), scalar_one())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        power::PowerDiscoveryEffects,
        resources::{DiplomacyLeverage, PendingCrisisSeeds},
    };
    use bevy::{app::App, prelude::Events};
    use bevy_ecs::system::RunSystemOnce;
    use serde_json::from_str;

    fn scalar(val: f32) -> Scalar {
        Scalar::from_f32(val)
    }

    #[test]
    fn load_catalog_populates_registry() {
        let json = r#"[{
            "id": 4096,
            "name": "Catalog Test",
            "field": "Physics",
            "requirements": [
                {"discovery_id": 101, "weight": 1.0, "minimum_progress": 0.5}
            ],
            "observation_threshold": 3,
            "cooldown_ticks": 5,
            "freshness_window": 7,
            "effect_flags": ["power", "diplomacy"],
            "covert_until_public": true
        }]"#;

        let mut registry = GreatDiscoveryRegistry::default();
        let loaded = registry
            .load_catalog_from_str(json)
            .expect("catalog should parse");
        assert_eq!(loaded, 1);

        let definition = registry
            .definition(&GreatDiscoveryId(4096))
            .expect("definition should be present");
        assert_eq!(definition.name, "Catalog Test");
        assert_eq!(definition.field, KnowledgeField::Physics);
        assert_eq!(definition.requirements.len(), 1);
        assert_eq!(definition.observation_threshold, 3);
        assert_eq!(definition.cooldown_ticks, 5);
        assert_eq!(definition.freshness_window, Some(7));
        assert!(definition.covert_until_public);
        assert_eq!(
            definition.effect_flags & effect_flags::POWER,
            effect_flags::POWER
        );
        assert_eq!(
            definition.effect_flags & effect_flags::DIPLOMACY,
            effect_flags::DIPLOMACY
        );

        let metadata = registry
            .metadata(&GreatDiscoveryId(4096))
            .expect("metadata should be stored");
        assert_eq!(metadata.name, "Catalog Test");
        assert_eq!(metadata.observation_threshold, 3);
        assert!(metadata.covert_until_public);
        assert_eq!(metadata.requirements.len(), 1);
        assert_eq!(metadata.requirements[0].discovery_id, 101);

        let definition_states = snapshot_definitions(&registry);
        assert_eq!(definition_states.len(), 1);
        let state = &definition_states[0];
        assert_eq!(state.id, 4096);
        assert_eq!(state.field, KnowledgeField::Physics);
        assert_eq!(state.observation_threshold, 3);
        assert_eq!(state.cooldown_ticks, 5);
        assert_eq!(state.freshness_window, Some(7));
        assert!(state.covert_until_public);
        assert_eq!(state.requirements.len(), 1);
        assert_eq!(state.requirements[0].discovery, 101);
        assert!((state.requirements[0].weight - 1.0).abs() < f32::EPSILON);
        assert!((state.requirements[0].minimum_progress - 0.5).abs() < f32::EPSILON);

        let mut restored = GreatDiscoveryRegistry::default();
        restored.restore_from_states(&definition_states);

        let restored_definition = restored
            .definition(&GreatDiscoveryId(4096))
            .expect("definition restored from snapshot state");
        assert_eq!(restored_definition.name, "Catalog Test");
        assert_eq!(restored_definition.observation_threshold, 3);

        let restored_metadata = restored
            .metadata(&GreatDiscoveryId(4096))
            .expect("metadata restored from snapshot state");
        assert_eq!(restored_metadata.observation_threshold, 3);
        assert_eq!(restored_metadata.requirements.len(), 1);
        assert_eq!(restored_metadata.requirements[0].discovery_id, 101);
    }

    #[test]
    fn range_bands_sample_within_bounds() {
        let entry: GreatDiscoveryCatalogEntry = from_str(
            r#"{
            "id": 4097,
            "name": "Range Test",
            "field": "Physics",
            "observation_threshold": {"min": 2, "max": 4},
            "cooldown_ticks": {"min": 3, "max": 7},
            "freshness_window": {"min": 5, "max": 9},
            "requirements": [
                {
                    "discovery_id": 303,
                    "weight": {"min": 0.5, "max": 1.5},
                    "minimum_progress": {"min": 0.2, "max": 0.8}
                }
            ],
            "effect_flags": ["power"],
            "covert_until_public": false
        }"#,
        )
        .expect("entry parses");

        let (definition, metadata) =
            resolve_catalog_entry(&entry).expect("entry resolves successfully");

        assert!((2..=4).contains(&definition.observation_threshold));
        assert!(metadata.observation_threshold >= 2);
        assert!(metadata.observation_threshold <= 4);
        assert!(definition.cooldown_ticks >= 3 && definition.cooldown_ticks <= 7);
        assert!(metadata.cooldown_ticks >= 3 && metadata.cooldown_ticks <= 7);
        assert!(definition.freshness_window.unwrap() >= 5);
        assert!(definition.freshness_window.unwrap() <= 9);

        let requirement = definition
            .requirements
            .first()
            .expect("definition has requirement");
        let requirement_meta = metadata
            .requirements
            .first()
            .expect("metadata has requirement");

        let weight = requirement.weight.to_f32();
        assert!((0.5..=1.5).contains(&weight));
        assert!((0.5..=1.5).contains(&requirement_meta.weight));
        assert!((weight - requirement_meta.weight).abs() < 1e-6);

        let minimum_progress = requirement.minimum_progress.to_f32();
        assert!((0.2..=0.8).contains(&minimum_progress));
        assert!((0.2..=0.8).contains(&requirement_meta.minimum_progress));
        assert!((minimum_progress - requirement_meta.minimum_progress).abs() < 1e-6);
    }

    #[test]
    fn sampling_is_deterministic_with_seed_offset_control() {
        let entry: GreatDiscoveryCatalogEntry = from_str(
            r#"{
            "id": 4098,
            "name": "Deterministic Test",
            "field": "Physics",
            "observation_threshold": {"min": 4, "max": 8},
            "cooldown_ticks": {"min": 2, "max": 6},
            "requirements": [
                {
                    "discovery_id": 404,
                    "weight": {"min": 0.4, "max": 0.9},
                    "minimum_progress": {"min": 0.1, "max": 0.9}
                }
            ],
            "effect_flags": [],
            "covert_until_public": false
        }"#,
        )
        .expect("entry parses");

        let (definition_a, metadata_a) =
            resolve_catalog_entry(&entry).expect("deterministic resolution succeeds");
        let (definition_b, metadata_b) =
            resolve_catalog_entry(&entry).expect("repeat resolution stays deterministic");

        assert_eq!(
            definition_a.observation_threshold,
            definition_b.observation_threshold
        );
        assert_eq!(definition_a.cooldown_ticks, definition_b.cooldown_ticks);
        assert_eq!(
            metadata_a.observation_threshold,
            metadata_b.observation_threshold
        );
        assert_eq!(metadata_a.cooldown_ticks, metadata_b.cooldown_ticks);
        assert_eq!(
            metadata_a.requirements[0].weight,
            metadata_b.requirements[0].weight
        );
        assert_eq!(
            metadata_a.requirements[0].minimum_progress,
            metadata_b.requirements[0].minimum_progress
        );

        let mut offset_entry = entry.clone();
        offset_entry.seed_offset = Some(1337);
        let (definition_c, metadata_c) =
            resolve_catalog_entry(&offset_entry).expect("offset entry resolves");

        let seed_shifted_same = definition_a.observation_threshold
            == definition_c.observation_threshold
            && definition_a.cooldown_ticks == definition_c.cooldown_ticks
            && (metadata_a.requirements[0].weight - metadata_c.requirements[0].weight).abs() < 1e-6
            && (metadata_a.requirements[0].minimum_progress
                - metadata_c.requirements[0].minimum_progress)
                .abs()
                < 1e-6;
        assert!(
            !seed_shifted_same,
            "changing the seed offset should alter at least one sampled value"
        );
    }

    #[test]
    fn evaluate_constellation_weighs_requirements() {
        let definition = GreatDiscoveryDefinition::new(
            GreatDiscoveryId(1),
            "Test",
            KnowledgeField::Physics,
            vec![
                ConstellationRequirement::new(1, scalar(1.0), scalar_zero()),
                ConstellationRequirement::new(2, scalar(1.0), scalar_zero()),
            ],
            2,
            0,
            None,
            0,
            false,
        );

        let mut ledger = DiscoveryProgressLedger::default();
        ledger.add_progress(FactionId(0), 1, scalar(0.5));
        ledger.add_progress(FactionId(0), 2, scalar(0.25));

        let progress = evaluate_constellation(&definition, FactionId(0), &ledger);
        assert!((progress.to_f32() - 0.375).abs() < f32::EPSILON);
    }

    #[test]
    fn candidates_require_observation_threshold() {
        let mut app = App::new();
        app.add_event::<GreatDiscoveryCandidateEvent>();
        app.add_event::<GreatDiscoveryResolvedEvent>();
        app.add_event::<GreatDiscoveryEffectEvent>();

        app.insert_resource(GreatDiscoveryRegistry::default());
        app.insert_resource(DiscoveryProgressLedger::default());
        app.insert_resource(ObservationLedger::default());
        app.insert_resource(GreatDiscoveryReadiness::default());
        app.insert_resource(GreatDiscoveryTelemetry::default());
        app.insert_resource(GreatDiscoveryLedger::default());
        app.insert_resource(PowerDiscoveryEffects::default());
        app.insert_resource(PendingCrisisSeeds::default());
        app.insert_resource(DiplomacyLeverage::default());
        app.insert_resource(SimulationTick(0));

        {
            let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
            registry.register(GreatDiscoveryDefinition::new(
                GreatDiscoveryId(1),
                "Observation Gate",
                KnowledgeField::Physics,
                vec![ConstellationRequirement::new(
                    42,
                    scalar(1.0),
                    scalar_zero(),
                )],
                3,
                0,
                None,
                0,
                false,
            ));
        }

        {
            let mut progress = app.world.resource_mut::<DiscoveryProgressLedger>();
            progress.add_progress(FactionId(0), 42, scalar_one());
        }

        {
            let mut observation = app.world.resource_mut::<ObservationLedger>();
            observation.set_observations(FactionId(0), KnowledgeField::Physics, 1);
        }

        app.world.run_system_once(collect_observation_signals);
        app.world.run_system_once(update_constellation_progress);
        app.world.run_system_once(screen_great_discovery_candidates);

        {
            let mut events = app
                .world
                .resource_mut::<Events<GreatDiscoveryCandidateEvent>>();
            assert_eq!(
                events.drain().count(),
                0,
                "candidates gated by observations"
            );
        }

        {
            let mut observation = app.world.resource_mut::<ObservationLedger>();
            observation.set_observations(FactionId(0), KnowledgeField::Physics, 3);
        }

        app.world.run_system_once(collect_observation_signals);
        app.world.run_system_once(update_constellation_progress);
        app.world.run_system_once(screen_great_discovery_candidates);

        {
            let mut events = app
                .world
                .resource_mut::<Events<GreatDiscoveryCandidateEvent>>();
            assert_eq!(
                events.drain().count(),
                1,
                "observation gate lifts once threshold met"
            );
        }
    }

    #[test]
    fn freshness_window_resets_progress_without_new_discovery() {
        let mut app = App::new();
        app.add_event::<GreatDiscoveryCandidateEvent>();
        app.add_event::<GreatDiscoveryResolvedEvent>();
        app.add_event::<GreatDiscoveryEffectEvent>();

        app.insert_resource(GreatDiscoveryRegistry::default());
        app.insert_resource(DiscoveryProgressLedger::default());
        app.insert_resource(ObservationLedger::default());
        app.insert_resource(GreatDiscoveryReadiness::default());
        app.insert_resource(GreatDiscoveryTelemetry::default());
        app.insert_resource(GreatDiscoveryLedger::default());
        app.insert_resource(PowerDiscoveryEffects::default());
        app.insert_resource(PendingCrisisSeeds::default());
        app.insert_resource(DiplomacyLeverage::default());
        app.insert_resource(SimulationTick(0));

        {
            let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
            registry.register(GreatDiscoveryDefinition::new(
                GreatDiscoveryId(7),
                "Freshness",
                KnowledgeField::Chemistry,
                vec![ConstellationRequirement::new(
                    55,
                    scalar(1.0),
                    scalar_zero(),
                )],
                0,
                0,
                Some(2),
                0,
                false,
            ));
        }

        {
            let mut progress = app.world.resource_mut::<DiscoveryProgressLedger>();
            progress.add_progress(FactionId(0), 55, scalar(0.6));
        }

        app.world.run_system_once(collect_observation_signals);
        app.world.run_system_once(update_constellation_progress);

        {
            let readiness = app.world.resource::<GreatDiscoveryReadiness>();
            let (_, entry_map) = readiness
                .iter()
                .next()
                .expect("entry exists after progress");
            let entry = entry_map
                .get(&GreatDiscoveryId(7))
                .expect("progress stored for discovery");
            assert!(
                entry.progress > scalar_zero(),
                "progress recorded before decay"
            );
        }

        {
            let mut tick = app.world.resource_mut::<SimulationTick>();
            tick.0 = 5;
        }

        app.world.run_system_once(collect_observation_signals);
        app.world.run_system_once(update_constellation_progress);

        {
            let readiness = app.world.resource::<GreatDiscoveryReadiness>();
            let (_, entry_map) = readiness.iter().next().expect("entry persists after decay");
            let entry = entry_map
                .get(&GreatDiscoveryId(7))
                .expect("progress entry persists");
            assert_eq!(
                entry.progress,
                scalar_zero(),
                "freshness window resets stale progress"
            );
        }
    }

    #[test]
    fn resolve_applies_effect_hooks() {
        let mut app = App::new();
        app.add_event::<GreatDiscoveryCandidateEvent>();
        app.add_event::<GreatDiscoveryResolvedEvent>();
        app.add_event::<GreatDiscoveryEffectEvent>();

        app.insert_resource(GreatDiscoveryRegistry::default());
        app.insert_resource(DiscoveryProgressLedger::default());
        app.insert_resource(ObservationLedger::default());
        app.insert_resource(GreatDiscoveryReadiness::default());
        app.insert_resource(GreatDiscoveryTelemetry::default());
        app.insert_resource(GreatDiscoveryLedger::default());
        app.insert_resource(PowerDiscoveryEffects::default());
        app.insert_resource(PendingCrisisSeeds::default());
        app.insert_resource(DiplomacyLeverage::default());
        app.insert_resource(SimulationTick(4));

        let definition = GreatDiscoveryDefinition::new(
            GreatDiscoveryId(11),
            "Effectful",
            KnowledgeField::Data,
            vec![ConstellationRequirement::new(
                7,
                scalar_one(),
                scalar_zero(),
            )],
            0,
            0,
            None,
            effect_flags::POWER | effect_flags::CRISIS | effect_flags::DIPLOMACY,
            false,
        );

        {
            let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
            registry.register(definition);
        }

        {
            let mut readiness = app.world.resource_mut::<GreatDiscoveryReadiness>();
            let faction_entry = readiness.per_faction.entry(FactionId(0)).or_default();
            let mut progress = ConstellationProgress::new(false);
            progress.progress = scalar_one();
            progress.observation_deficit = 0;
            faction_entry.insert(GreatDiscoveryId(11), progress);
        }

        {
            let mut candidates = app
                .world
                .resource_mut::<Events<GreatDiscoveryCandidateEvent>>();
            candidates.send(GreatDiscoveryCandidateEvent {
                faction: FactionId(0),
                discovery: GreatDiscoveryId(11),
            });
        }

        app.world.run_system_once(resolve_great_discovery);

        {
            let power_effects = app.world.resource::<PowerDiscoveryEffects>();
            assert!(power_effects.contains(GreatDiscoveryId(11)));
        }

        {
            let seeds = app.world.resource::<PendingCrisisSeeds>();
            assert!(seeds.seeds.contains(&(FactionId(0), 11)));
        }

        {
            let diplomacy = app.world.resource::<DiplomacyLeverage>();
            assert!(diplomacy
                .great_discoveries
                .iter()
                .any(|(faction, discovery)| *faction == FactionId(0) && *discovery == 11));
        }

        let effect_count = app
            .world
            .resource_mut::<Events<GreatDiscoveryEffectEvent>>()
            .drain()
            .count();
        assert_eq!(effect_count, 3);
    }

    #[test]
    fn propagate_marks_publication_and_reinforces_requirements() {
        let mut app = App::new();
        app.add_event::<GreatDiscoveryResolvedEvent>();

        app.insert_resource(GreatDiscoveryRegistry::default());
        app.insert_resource(DiscoveryProgressLedger::default());
        app.insert_resource(GreatDiscoveryLedger::default());

        let definition = GreatDiscoveryDefinition::new(
            GreatDiscoveryId(21),
            "Publication",
            KnowledgeField::Biology,
            vec![ConstellationRequirement::new(
                9,
                scalar_one(),
                scalar_zero(),
            )],
            0,
            0,
            None,
            effect_flags::FORCED_PUBLICATION,
            false,
        );

        {
            let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
            registry.register(definition);
        }

        let base_record = GreatDiscoveryRecord {
            id: GreatDiscoveryId(21),
            faction: FactionId(2),
            field: KnowledgeField::Biology,
            tick: 10,
            publicly_deployed: false,
            effect_flags: effect_flags::FORCED_PUBLICATION,
        };

        {
            let mut ledger = app.world.resource_mut::<GreatDiscoveryLedger>();
            ledger.push(base_record.clone());
        }

        {
            let mut events = app
                .world
                .resource_mut::<Events<GreatDiscoveryResolvedEvent>>();
            events.send(GreatDiscoveryResolvedEvent {
                record: base_record,
            });
        }

        app.world.run_system_once(propagate_diffusion_impacts);

        {
            let ledger = app.world.resource::<GreatDiscoveryLedger>();
            let record = ledger
                .records()
                .iter()
                .find(|record| record.id == GreatDiscoveryId(21))
                .expect("record exists");
            assert!(record.publicly_deployed);
        }

        {
            let progress = app.world.resource::<DiscoveryProgressLedger>();
            let amount = progress.get_progress(FactionId(2), 9);
            assert_eq!(amount, scalar_one());
        }
    }
}
