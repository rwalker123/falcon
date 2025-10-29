use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bevy::{ecs::system::SystemParam, prelude::*};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    knowledge_ledger::{
        CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
        KnowledgeLedgerEntry,
    },
    metrics::SimulationMetrics,
    orders::{FactionId, FactionRegistry},
    scalar::{scalar_from_f32, scalar_zero, Scalar},
};
use log::{info, warn};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use sim_runtime::KnowledgeCountermeasureKind;

pub const BUILTIN_ESPIONAGE_AGENT_CATALOG: &str = include_str!("data/espionage_agents.json");
pub const BUILTIN_ESPIONAGE_MISSION_CATALOG: &str = include_str!("data/espionage_missions.json");
pub const BUILTIN_ESPIONAGE_CONFIG: &str = include_str!("data/espionage_config.json");

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EspionageAgentId(pub String);

impl EspionageAgentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EspionageMissionId(pub String);

impl EspionageMissionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EspionageAgentHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EspionageMissionInstanceId(pub u64);

#[derive(Debug, Clone)]
pub struct EspionageAgentTemplate {
    pub id: EspionageAgentId,
    pub name: String,
    pub stealth: Scalar,
    pub recon: Scalar,
    pub counter_intel: Scalar,
    pub tags: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EspionageMissionKind {
    Probe,
    CounterIntel,
}

#[derive(Debug, Clone)]
pub struct EspionageMissionCountermeasure {
    pub kind: sim_runtime::KnowledgeCountermeasureKind,
    pub potency: Scalar,
    pub upkeep: Scalar,
    pub duration_ticks: u16,
}

#[derive(Debug, Clone)]
pub struct EspionageMissionTemplate {
    pub id: EspionageMissionId,
    pub name: String,
    pub kind: EspionageMissionKind,
    pub resolution_ticks: u16,
    pub base_success: Scalar,
    pub success_threshold: Scalar,
    pub stealth_weight: Scalar,
    pub recon_weight: Scalar,
    pub counter_intel_weight: Scalar,
    pub fidelity_gain: Scalar,
    pub suspicion_on_success: Scalar,
    pub suspicion_on_failure: Scalar,
    pub cell_gain_on_success: u8,
    pub countermeasure: Option<EspionageMissionCountermeasure>,
    pub suspicion_relief: Scalar,
    pub fidelity_suppression: Scalar,
    pub note: Option<String>,
    pub target_tier_min: Option<u8>,
    pub target_tier_max: Option<u8>,
    pub generated: bool,
}

impl EspionageMissionTemplate {
    pub fn is_valid_for_tier(&self, tier: Option<u8>) -> bool {
        if let Some(tier) = tier {
            if let Some(min) = self.target_tier_min {
                if tier < min {
                    return false;
                }
            }
            if let Some(max) = self.target_tier_max {
                if tier > max {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug, Error)]
pub enum EspionageCatalogError {
    #[error("failed to parse espionage agent catalog: {0}")]
    ParseAgents(#[from] serde_json::Error),
    #[error("duplicate espionage agent id '{0}'")]
    DuplicateAgent(String),
    #[error("failed to parse espionage mission catalog: {0}")]
    ParseMissions(serde_json::Error),
    #[error("duplicate espionage mission id '{0}'")]
    DuplicateMission(String),
    #[error("unknown countermeasure kind '{kind}' for mission '{mission}'")]
    UnknownCountermeasureKind { mission: String, kind: String },
    #[error("failed to parse espionage balance config: {0}")]
    ParseConfig(serde_json::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct EspionageBalanceConfig {
    #[serde(default)]
    security_posture_penalties: SecurityPosturePenalties,
    #[serde(default)]
    probe_resolution: ProbeResolutionTuning,
    #[serde(default)]
    counter_intel_resolution: CounterIntelResolutionTuning,
    #[serde(default)]
    counter_intel_budget: CounterIntelBudgetConfig,
    #[serde(default)]
    agent_generator_defaults: AgentGeneratorDefaults,
    #[serde(default)]
    mission_generator_defaults: MissionGeneratorDefaults,
    #[serde(default)]
    queue_defaults: EspionageQueueDefaults,
}

impl EspionageBalanceConfig {
    pub fn security_penalty(&self, posture: sim_runtime::KnowledgeSecurityPosture) -> Scalar {
        self.security_posture_penalties.penalty(posture)
    }

    pub fn probe_resolution(&self) -> &ProbeResolutionTuning {
        &self.probe_resolution
    }

    pub fn counter_intel_resolution(&self) -> &CounterIntelResolutionTuning {
        &self.counter_intel_resolution
    }

    pub fn counter_intel_budget(&self) -> &CounterIntelBudgetConfig {
        &self.counter_intel_budget
    }

    pub fn agent_defaults(&self) -> &AgentGeneratorDefaults {
        &self.agent_generator_defaults
    }

    pub fn mission_defaults(&self) -> &MissionGeneratorDefaults {
        &self.mission_generator_defaults
    }

    pub fn queue_defaults(&self) -> &EspionageQueueDefaults {
        &self.queue_defaults
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CounterIntelBudgetConfig {
    initial_reserve: f32,
    max_reserve: f32,
    regen_per_tick: f32,
    sweep_cost: f32,
    min_reserve: f32,
    lenient_suspicion_threshold: f32,
    lenient_progress_threshold: u16,
    hardened_progress_threshold: u16,
}

impl Default for CounterIntelBudgetConfig {
    fn default() -> Self {
        Self {
            initial_reserve: 4.0,
            max_reserve: 8.0,
            regen_per_tick: 1.0,
            sweep_cost: 2.0,
            min_reserve: 1.0,
            lenient_suspicion_threshold: 0.6,
            lenient_progress_threshold: 95,
            hardened_progress_threshold: 60,
        }
    }
}

impl CounterIntelBudgetConfig {
    pub fn initial_reserve(&self) -> Scalar {
        scalar_from_f32(self.initial_reserve)
    }

    pub fn max_reserve(&self) -> Scalar {
        scalar_from_f32(self.max_reserve)
    }

    pub fn regen_per_tick(&self) -> Scalar {
        scalar_from_f32(self.regen_per_tick)
    }

    pub fn sweep_cost(&self) -> Scalar {
        scalar_from_f32(self.sweep_cost)
    }

    pub fn min_reserve(&self) -> Scalar {
        scalar_from_f32(self.min_reserve)
    }

    pub fn lenient_suspicion_threshold(&self) -> Scalar {
        scalar_from_f32(self.lenient_suspicion_threshold)
    }

    pub fn lenient_progress_threshold(&self) -> u16 {
        self.lenient_progress_threshold
    }

    pub fn hardened_progress_threshold(&self) -> u16 {
        self.hardened_progress_threshold
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityPosturePenalties {
    minimal: f32,
    standard: f32,
    hardened: f32,
    black_vault: f32,
}

impl Default for SecurityPosturePenalties {
    fn default() -> Self {
        Self {
            minimal: 0.0,
            standard: 0.15,
            hardened: 0.3,
            black_vault: 0.45,
        }
    }
}

impl SecurityPosturePenalties {
    fn penalty(&self, posture: sim_runtime::KnowledgeSecurityPosture) -> Scalar {
        let value = match posture {
            sim_runtime::KnowledgeSecurityPosture::Minimal => self.minimal,
            sim_runtime::KnowledgeSecurityPosture::Standard => self.standard,
            sim_runtime::KnowledgeSecurityPosture::Hardened => self.hardened,
            sim_runtime::KnowledgeSecurityPosture::BlackVault => self.black_vault,
        };
        Scalar::from_f32(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ProbeResolutionTuning {
    recon_fidelity_bonus: f32,
    suspicion_floor: f32,
    failure_extra_suspicion: f32,
    partial_margin: f32,
    partial_fidelity_scalar: f32,
    partial_suspicion_scalar: f32,
    failure_misinformation_fidelity: f32,
}

impl Default for ProbeResolutionTuning {
    fn default() -> Self {
        Self {
            recon_fidelity_bonus: 0.1,
            suspicion_floor: 0.05,
            failure_extra_suspicion: 0.05,
            partial_margin: 0.1,
            partial_fidelity_scalar: 0.5,
            partial_suspicion_scalar: 0.6,
            failure_misinformation_fidelity: -0.1,
        }
    }
}

impl ProbeResolutionTuning {
    fn recon_fidelity_bonus(&self) -> Scalar {
        Scalar::from_f32(self.recon_fidelity_bonus)
    }

    fn suspicion_floor(&self) -> Scalar {
        Scalar::from_f32(self.suspicion_floor)
    }

    fn failure_extra_suspicion(&self) -> Scalar {
        Scalar::from_f32(self.failure_extra_suspicion)
    }

    fn partial_margin(&self) -> f32 {
        self.partial_margin.max(0.0)
    }

    fn partial_fidelity_scalar(&self) -> f32 {
        self.partial_fidelity_scalar.clamp(0.0, 1.0)
    }

    fn partial_suspicion_scalar(&self) -> f32 {
        self.partial_suspicion_scalar.clamp(0.0, 1.0)
    }

    fn failure_misinformation_fidelity(&self) -> Scalar {
        Scalar::from_f32(self.failure_misinformation_fidelity)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CounterIntelResolutionTuning {
    security_penalty_factor: f32,
    default_sweep_potency: f32,
    default_sweep_upkeep: f32,
    default_sweep_duration: u16,
    suspicion_relief: f32,
}

impl Default for CounterIntelResolutionTuning {
    fn default() -> Self {
        Self {
            security_penalty_factor: 0.5,
            default_sweep_potency: 0.3,
            default_sweep_upkeep: 0.05,
            default_sweep_duration: 2,
            suspicion_relief: 0.25,
        }
    }
}

impl CounterIntelResolutionTuning {
    fn security_penalty_factor(&self) -> Scalar {
        Scalar::from_f32(self.security_penalty_factor)
    }

    fn default_countermeasure(&self) -> EspionageMissionCountermeasure {
        EspionageMissionCountermeasure {
            kind: sim_runtime::KnowledgeCountermeasureKind::CounterIntelSweep,
            potency: Scalar::from_f32(self.default_sweep_potency),
            upkeep: Scalar::from_f32(self.default_sweep_upkeep),
            duration_ticks: self.default_sweep_duration,
        }
    }

    fn suspicion_relief(&self) -> Scalar {
        Scalar::from_f32(self.suspicion_relief.max(0.0))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AgentGeneratorDefaults {
    stealth_min: f32,
    stealth_max: f32,
    recon_min: f32,
    recon_max: f32,
    counter_intel_min: f32,
    counter_intel_max: f32,
}

impl Default for AgentGeneratorDefaults {
    fn default() -> Self {
        Self {
            stealth_min: 0.3,
            stealth_max: 0.8,
            recon_min: 0.3,
            recon_max: 0.8,
            counter_intel_min: 0.2,
            counter_intel_max: 0.6,
        }
    }
}

impl AgentGeneratorDefaults {
    fn stealth_range(&self) -> (f32, f32) {
        (self.stealth_min, self.stealth_max)
    }

    fn recon_range(&self) -> (f32, f32) {
        (self.recon_min, self.recon_max)
    }

    fn counter_intel_range(&self) -> (f32, f32) {
        (self.counter_intel_min, self.counter_intel_max)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MissionGeneratorDefaults {
    resolution_ticks_min: u16,
    resolution_ticks_max: u16,
    base_success_min: f32,
    base_success_max: f32,
    success_threshold_min: f32,
    success_threshold_max: f32,
    fidelity_gain_min: f32,
    fidelity_gain_max: f32,
    suspicion_on_success_min: f32,
    suspicion_on_success_max: f32,
    suspicion_on_failure_min: f32,
    suspicion_on_failure_max: f32,
    cell_gain_on_success_min: u8,
    cell_gain_on_success_max: u8,
    suspicion_relief_min: f32,
    suspicion_relief_max: f32,
    fidelity_suppression_min: f32,
    fidelity_suppression_max: f32,
}

impl Default for MissionGeneratorDefaults {
    fn default() -> Self {
        Self {
            resolution_ticks_min: 1,
            resolution_ticks_max: 2,
            base_success_min: 0.45,
            base_success_max: 0.65,
            success_threshold_min: 0.5,
            success_threshold_max: 0.75,
            fidelity_gain_min: 0.18,
            fidelity_gain_max: 0.3,
            suspicion_on_success_min: 0.1,
            suspicion_on_success_max: 0.2,
            suspicion_on_failure_min: 0.3,
            suspicion_on_failure_max: 0.4,
            cell_gain_on_success_min: 1,
            cell_gain_on_success_max: 2,
            suspicion_relief_min: 0.2,
            suspicion_relief_max: 0.35,
            fidelity_suppression_min: 0.08,
            fidelity_suppression_max: 0.16,
        }
    }
}

impl MissionGeneratorDefaults {
    fn resolution_ticks(&self) -> (u16, u16) {
        (self.resolution_ticks_min, self.resolution_ticks_max)
    }

    fn base_success(&self) -> (f32, f32) {
        (self.base_success_min, self.base_success_max)
    }

    fn success_threshold(&self) -> (f32, f32) {
        (self.success_threshold_min, self.success_threshold_max)
    }

    fn fidelity_gain(&self) -> (f32, f32) {
        (self.fidelity_gain_min, self.fidelity_gain_max)
    }

    fn suspicion_on_success(&self) -> (f32, f32) {
        (self.suspicion_on_success_min, self.suspicion_on_success_max)
    }

    fn suspicion_on_failure(&self) -> (f32, f32) {
        (self.suspicion_on_failure_min, self.suspicion_on_failure_max)
    }

    fn cell_gain_on_success(&self) -> (u8, u8) {
        (self.cell_gain_on_success_min, self.cell_gain_on_success_max)
    }

    fn suspicion_relief(&self) -> (f32, f32) {
        (self.suspicion_relief_min, self.suspicion_relief_max)
    }

    fn fidelity_suppression(&self) -> (f32, f32) {
        (self.fidelity_suppression_min, self.fidelity_suppression_max)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EspionageQueueDefaults {
    pub scheduled_tick_offset: u64,
    pub target_tier: Option<u8>,
}

#[derive(Resource, Debug)]
pub struct EspionageCatalog {
    agents: HashMap<EspionageAgentId, EspionageAgentTemplate>,
    agent_order: Vec<EspionageAgentId>,
    missions: HashMap<EspionageMissionId, EspionageMissionTemplate>,
    generators: Vec<EspionageAgentGenerator>,
    config: Arc<EspionageBalanceConfig>,
}

impl EspionageCatalog {
    pub fn load_builtin() -> Result<Self, EspionageCatalogError> {
        Self::load_from_str(
            BUILTIN_ESPIONAGE_AGENT_CATALOG,
            BUILTIN_ESPIONAGE_MISSION_CATALOG,
            BUILTIN_ESPIONAGE_CONFIG,
        )
    }

    pub fn load_from_str(
        agent_json: &str,
        mission_json: &str,
        config_json: &str,
    ) -> Result<Self, EspionageCatalogError> {
        let agent_catalog: EspionageAgentCatalog =
            serde_json::from_str(agent_json).map_err(EspionageCatalogError::ParseAgents)?;
        let mission_catalog: EspionageMissionCatalog =
            serde_json::from_str(mission_json).map_err(EspionageCatalogError::ParseMissions)?;
        let config: Arc<EspionageBalanceConfig> = Arc::new(
            serde_json::from_str(config_json).map_err(EspionageCatalogError::ParseConfig)?,
        );

        let mut agents = HashMap::new();
        let mut agent_order = Vec::new();
        let mut generators = Vec::new();
        for entry in agent_catalog.agents {
            let id = EspionageAgentId::new(entry.id.clone());
            if agents.contains_key(&id) {
                return Err(EspionageCatalogError::DuplicateAgent(entry.id));
            }
            let generator_config = entry.generator.clone();
            let template = EspionageAgentTemplate {
                id: id.clone(),
                name: entry.name,
                stealth: scalar_from_f32(entry.stealth.unwrap_or(0.5)),
                recon: scalar_from_f32(entry.recon.unwrap_or(0.5)),
                counter_intel: scalar_from_f32(entry.counter_intel.unwrap_or(0.5)),
                tags: entry.tags.unwrap_or_default(),
                note: entry.note,
            };

            if generator_config.is_none() {
                agent_order.push(id.clone());
            } else if let Some(generator_entry) = generator_config {
                generators.push(EspionageAgentGenerator::from_catalog_entry(
                    &template,
                    generator_entry,
                    config.agent_defaults(),
                ));
            }

            agents.insert(id, template);
        }

        let mut missions = HashMap::new();
        for entry in mission_catalog.missions {
            let id = EspionageMissionId::new(entry.id.clone());
            if missions.contains_key(&id) {
                return Err(EspionageCatalogError::DuplicateMission(entry.id));
            }

            let countermeasure = if let Some(countermeasure) = entry.countermeasure.clone() {
                let kind = match countermeasure.kind.as_deref() {
                    None | Some("counter_intel_sweep") => {
                        sim_runtime::KnowledgeCountermeasureKind::CounterIntelSweep
                    }
                    Some("security_investment") => {
                        sim_runtime::KnowledgeCountermeasureKind::SecurityInvestment
                    }
                    Some("misinformation") => {
                        sim_runtime::KnowledgeCountermeasureKind::Misinformation
                    }
                    Some("knowledge_debt_relief") => {
                        sim_runtime::KnowledgeCountermeasureKind::KnowledgeDebtRelief
                    }
                    Some(other) => {
                        return Err(EspionageCatalogError::UnknownCountermeasureKind {
                            mission: entry.id.clone(),
                            kind: other.to_string(),
                        })
                    }
                };
                Some(EspionageMissionCountermeasure {
                    kind,
                    potency: scalar_from_f32(countermeasure.potency.unwrap_or(0.2)),
                    upkeep: scalar_from_f32(countermeasure.upkeep.unwrap_or(0.05)),
                    duration_ticks: countermeasure.duration_ticks.unwrap_or(2),
                })
            } else {
                None
            };

            let base_template = EspionageMissionTemplate {
                id: id.clone(),
                name: entry.name.clone(),
                kind: entry.kind.unwrap_or(EspionageMissionKind::Probe),
                resolution_ticks: entry.resolution_ticks.unwrap_or(1),
                base_success: scalar_from_f32(entry.base_success.unwrap_or(0.5)),
                success_threshold: scalar_from_f32(entry.success_threshold.unwrap_or(0.5)),
                stealth_weight: scalar_from_f32(entry.stealth_weight.unwrap_or(0.4)),
                recon_weight: scalar_from_f32(entry.recon_weight.unwrap_or(0.4)),
                counter_intel_weight: scalar_from_f32(entry.counter_intel_weight.unwrap_or(0.6)),
                fidelity_gain: scalar_from_f32(entry.fidelity_gain.unwrap_or(0.2)),
                suspicion_on_success: scalar_from_f32(entry.suspicion_on_success.unwrap_or(0.1)),
                suspicion_on_failure: scalar_from_f32(entry.suspicion_on_failure.unwrap_or(0.25)),
                cell_gain_on_success: entry.cell_gain_on_success.unwrap_or(1),
                countermeasure: countermeasure.clone(),
                suspicion_relief: scalar_from_f32(entry.suspicion_relief.unwrap_or(0.25)),
                fidelity_suppression: scalar_from_f32(entry.fidelity_suppression.unwrap_or(0.1)),
                note: entry.note.clone(),
                target_tier_min: entry.target_tier_min,
                target_tier_max: entry.target_tier_max,
                generated: false,
            };

            if let Some(generator_entry) = entry.generator.clone() {
                if generator_entry.enabled && generator_entry.variant_count > 0 {
                    let generator = EspionageMissionGenerator::from_catalog_entry(
                        &base_template,
                        generator_entry,
                        config.mission_defaults(),
                    );
                    for variant in generator.generate_variants(&base_template) {
                        if missions.contains_key(&variant.id) {
                            return Err(EspionageCatalogError::DuplicateMission(
                                variant.id.0.clone(),
                            ));
                        }
                        missions.insert(variant.id.clone(), variant);
                    }
                }
            }

            missions.insert(id, base_template);
        }

        Ok(Self {
            agents,
            agent_order,
            missions,
            generators,
            config,
        })
    }

    pub fn agent(&self, id: &EspionageAgentId) -> Option<&EspionageAgentTemplate> {
        self.agents.get(id)
    }

    pub fn mission(&self, id: &EspionageMissionId) -> Option<&EspionageMissionTemplate> {
        self.missions.get(id)
    }

    pub fn missions(&self) -> impl Iterator<Item = &EspionageMissionTemplate> {
        self.missions.values()
    }

    pub fn agents(&self) -> impl Iterator<Item = &EspionageAgentTemplate> {
        self.agent_order.iter().filter_map(|id| self.agents.get(id))
    }

    pub fn generators(&self) -> impl Iterator<Item = &EspionageAgentGenerator> {
        self.generators.iter()
    }

    pub fn config(&self) -> &EspionageBalanceConfig {
        self.config.as_ref()
    }

    pub fn update_queue_defaults(&mut self, defaults: EspionageQueueDefaults) {
        let config = Arc::make_mut(&mut self.config);
        config.queue_defaults = defaults;
    }

    pub fn update_agent_generator(
        &mut self,
        template_id: &str,
        enabled: Option<bool>,
        per_faction: Option<u8>,
    ) -> bool {
        if let Some(generator) = self
            .generators
            .iter_mut()
            .find(|generator| generator.id.0 == template_id)
        {
            if let Some(value) = enabled {
                generator.enabled = value;
            }
            if let Some(value) = per_faction {
                generator.per_faction = value;
            }
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct EspionageAgent {
    pub handle: EspionageAgentHandle,
    pub template_id: EspionageAgentId,
    pub name: String,
    pub stealth: Scalar,
    pub recon: Scalar,
    pub counter_intel: Scalar,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub assignment: AgentAssignment,
    pub generated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAssignment {
    Available,
    Assigned(EspionageMissionInstanceId),
}

#[derive(Resource, Debug, Default)]
pub struct EspionageRoster {
    agents: HashMap<FactionId, Vec<EspionageAgent>>,
    next_agent_handle: u32,
}

impl EspionageRoster {
    pub fn seed_from_catalog(&mut self, factions: &[FactionId], catalog: &EspionageCatalog) {
        for faction in factions {
            let already_seeded = self
                .agents
                .get(faction)
                .map(|roster| !roster.is_empty())
                .unwrap_or(false);
            if already_seeded {
                continue;
            }

            let mut additions: Vec<EspionageAgent> = Vec::new();
            for template in catalog.agents() {
                let handle = self.allocate_handle();
                additions.push(EspionageAgent {
                    handle,
                    template_id: template.id.clone(),
                    name: template.name.clone(),
                    stealth: template.stealth,
                    recon: template.recon,
                    counter_intel: template.counter_intel,
                    tags: template.tags.clone(),
                    note: template.note.clone(),
                    assignment: AgentAssignment::Available,
                    generated: false,
                });
            }

            for generator in catalog.generators() {
                if !generator.is_enabled() {
                    continue;
                }
                let mut rng = generator.rng_for_faction(*faction);
                for variant_index in 0..generator.per_faction {
                    let handle = self.allocate_handle();
                    additions.push(generator.generate_agent(
                        handle,
                        *faction,
                        variant_index,
                        &mut rng,
                    ));
                }
            }
            self.agents
                .entry(*faction)
                .or_default()
                .extend(additions.into_iter());
        }
    }

    pub fn agents_for(&self, faction: FactionId) -> &[EspionageAgent] {
        self.agents
            .get(&faction)
            .map(|agents| agents.as_slice())
            .unwrap_or(&[])
    }

    pub fn agent_mut(
        &mut self,
        faction: FactionId,
        handle: EspionageAgentHandle,
    ) -> Option<&mut EspionageAgent> {
        self.agents
            .get_mut(&faction)
            .and_then(|agents| agents.iter_mut().find(|agent| agent.handle == handle))
    }

    pub fn agent(
        &self,
        faction: FactionId,
        handle: EspionageAgentHandle,
    ) -> Option<&EspionageAgent> {
        self.agents
            .get(&faction)
            .and_then(|agents| agents.iter().find(|agent| agent.handle == handle))
    }

    pub fn refresh_generated_agents(&mut self, catalog: &EspionageCatalog, factions: &[FactionId]) {
        for faction in factions {
            let mut additions: Vec<EspionageAgent> = Vec::new();
            for generator in catalog.generators() {
                if !generator.is_enabled() {
                    continue;
                }
                let mut rng = generator.rng_for_faction(*faction);
                for variant_index in 0..generator.per_faction {
                    let handle = self.allocate_handle();
                    additions.push(generator.generate_agent(
                        handle,
                        *faction,
                        variant_index,
                        &mut rng,
                    ));
                }
            }

            let roster_entry = self.agents.entry(*faction).or_default();
            roster_entry.retain(|agent| !agent.generated);
            roster_entry.extend(additions);
        }
    }
}

impl EspionageRoster {
    fn allocate_handle(&mut self) -> EspionageAgentHandle {
        let handle = EspionageAgentHandle(self.next_agent_handle);
        self.next_agent_handle = self.next_agent_handle.wrapping_add(1);
        handle
    }
}

#[derive(Resource, Debug, Default)]
pub struct EspionageMissionState {
    active: Vec<ScheduledEspionageMission>,
    next_instance: u64,
}

#[derive(Debug, Clone)]
pub struct QueueMissionParams {
    pub mission_id: EspionageMissionId,
    pub owner: FactionId,
    pub target_owner: FactionId,
    pub discovery_id: u32,
    pub agent: EspionageAgentHandle,
    pub target_tier: Option<u8>,
    pub scheduled_tick: u64,
}

impl EspionageMissionState {
    pub fn queue_mission(
        &mut self,
        catalog: &EspionageCatalog,
        roster: &mut EspionageRoster,
        params: QueueMissionParams,
    ) -> Result<EspionageMissionInstanceId, QueueMissionError> {
        let QueueMissionParams {
            mission_id,
            owner,
            target_owner,
            discovery_id,
            agent: agent_handle,
            target_tier,
            scheduled_tick,
        } = params;

        let mission = catalog
            .mission(&mission_id)
            .ok_or_else(|| QueueMissionError::UnknownMission(mission_id.0.clone()))?;

        if !mission.is_valid_for_tier(target_tier) {
            return Err(QueueMissionError::TargetTierMismatch {
                mission: mission_id.0.clone(),
                target_tier,
            });
        }

        let agent =
            roster
                .agent_mut(owner, agent_handle)
                .ok_or(QueueMissionError::UnknownAgent {
                    faction: owner,
                    handle: agent_handle,
                })?;

        match agent.assignment {
            AgentAssignment::Available => {
                let instance_id = self.allocate_instance();
                agent.assignment = AgentAssignment::Assigned(instance_id);
                self.active.push(ScheduledEspionageMission {
                    instance_id,
                    mission_id: mission_id.clone(),
                    owner,
                    target_owner,
                    discovery_id,
                    agent: agent_handle,
                    ticks_remaining: mission.resolution_ticks.max(1),
                    scheduled_tick,
                    note: mission.note.clone(),
                });
                Ok(instance_id)
            }
            AgentAssignment::Assigned(existing) => Err(QueueMissionError::AgentUnavailable {
                faction: owner,
                handle: agent_handle,
                mission: existing,
            }),
        }
    }

    fn allocate_instance(&mut self) -> EspionageMissionInstanceId {
        let instance = EspionageMissionInstanceId(self.next_instance);
        self.next_instance = self.next_instance.wrapping_add(1);
        instance
    }

    pub fn missions(&self) -> &[ScheduledEspionageMission] {
        &self.active
    }

    pub fn missions_mut(&mut self) -> &mut Vec<ScheduledEspionageMission> {
        &mut self.active
    }
}

#[derive(Debug, Clone)]
pub struct ScheduledEspionageMission {
    pub instance_id: EspionageMissionInstanceId,
    pub mission_id: EspionageMissionId,
    pub owner: FactionId,
    pub target_owner: FactionId,
    pub discovery_id: u32,
    pub agent: EspionageAgentHandle,
    pub ticks_remaining: u16,
    pub scheduled_tick: u64,
    pub note: Option<String>,
}

#[derive(Debug, Error)]
pub enum QueueMissionError {
    #[error("mission '{0}' not found in catalog")]
    UnknownMission(String),
    #[error("agent {handle:?} not found for faction {faction:?}")]
    UnknownAgent {
        faction: FactionId,
        handle: EspionageAgentHandle,
    },
    #[error("agent {handle:?} for faction {faction:?} already assigned to mission {mission:?}")]
    AgentUnavailable {
        faction: FactionId,
        handle: EspionageAgentHandle,
        mission: EspionageMissionInstanceId,
    },
    #[error("no available agents for faction {faction:?}")]
    NoAgentAvailable { faction: FactionId },
    #[error("mission '{mission}' cannot target tier {target_tier:?}")]
    TargetTierMismatch {
        mission: String,
        target_tier: Option<u8>,
    },
}

#[derive(Debug, Default)]
struct MissionOutcome {
    probe_event: Option<EspionageProbeEvent>,
    sweep_event: Option<CounterIntelSweepEvent>,
}

#[derive(Debug, Clone)]
pub struct EspionageAgentGenerator {
    pub id: EspionageAgentId,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub per_faction: u8,
    pub stealth_range: StatRange,
    pub recon_range: StatRange,
    pub counter_intel_range: StatRange,
    pub tags: Vec<String>,
    pub name_pool: Vec<String>,
    pub note: Option<String>,
    pub seed_offset: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct StatRange {
    min: f32,
    max: f32,
}

impl StatRange {
    fn new(min: f32, max: f32) -> Self {
        let clamped_min = min.clamp(0.0, 1.0);
        let clamped_max = max.clamp(0.0, 1.0);
        if clamped_min <= clamped_max {
            Self {
                min: clamped_min,
                max: clamped_max,
            }
        } else {
            Self {
                min: clamped_max,
                max: clamped_min,
            }
        }
    }

    fn sample(&self, rng: &mut SmallRng) -> Scalar {
        if (self.max - self.min).abs() <= f32::EPSILON {
            return Scalar::from_f32(self.min);
        }
        let value = rng.gen_range(self.min..=self.max);
        Scalar::from_f32(value)
    }
}

impl EspionageAgentGenerator {
    fn from_catalog_entry(
        template: &EspionageAgentTemplate,
        generator: EspionageAgentGeneratorEntry,
        defaults: &AgentGeneratorDefaults,
    ) -> Self {
        let stealth_range = resolve_stat_range(
            generator.stealth,
            Some((template.stealth.to_f32(), template.stealth.to_f32())),
            defaults.stealth_range(),
        );
        let recon_range = resolve_stat_range(
            generator.recon,
            Some((template.recon.to_f32(), template.recon.to_f32())),
            defaults.recon_range(),
        );
        let counter_intel_range = resolve_stat_range(
            generator.counter_intel,
            Some((
                template.counter_intel.to_f32(),
                template.counter_intel.to_f32(),
            )),
            defaults.counter_intel_range(),
        );

        let mut tags = generator.tags.unwrap_or_else(|| template.tags.clone());
        if !tags.iter().any(|tag| tag == "generated") {
            tags.push("generated".to_string());
        }

        let note = generator.note.or_else(|| template.note.clone());
        let seed_offset = generator
            .seed_offset
            .unwrap_or_else(|| hash_identifier(&template.id.0));

        Self {
            id: template.id.clone(),
            display_name: Some(template.name.clone()),
            enabled: generator.enabled,
            per_faction: generator.per_faction,
            stealth_range,
            recon_range,
            counter_intel_range,
            tags,
            name_pool: generator.name_pool.unwrap_or_default(),
            note,
            seed_offset,
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled && self.per_faction > 0
    }

    fn rng_for_faction(&self, faction: FactionId) -> SmallRng {
        let seed = self.seed_offset ^ ((faction.0 as u64) << 32);
        SmallRng::seed_from_u64(seed)
    }

    fn generate_agent(
        &self,
        handle: EspionageAgentHandle,
        faction: FactionId,
        variant_index: u8,
        rng: &mut SmallRng,
    ) -> EspionageAgent {
        let stealth = self.stealth_range.sample(rng);
        let recon = self.recon_range.sample(rng);
        let counter_intel = self.counter_intel_range.sample(rng);

        let base_name = if let Some(name) = self.name_pool.choose(rng) {
            name.clone()
        } else if let Some(display_name) = &self.display_name {
            display_name.clone()
        } else {
            format!("Generated Agent {}", self.id.0)
        };

        let final_name = format!("{} [{}-{}]", base_name, faction.0, variant_index + 1);

        EspionageAgent {
            handle,
            template_id: self.id.clone(),
            name: final_name,
            stealth,
            recon,
            counter_intel,
            tags: self.tags.clone(),
            note: self
                .note
                .clone()
                .or_else(|| Some(format!("Generated from {}", self.id.0))),
            assignment: AgentAssignment::Available,
            generated: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EspionageMissionGenerator {
    variant_count: u8,
    id_pattern: Option<String>,
    name_pool: Vec<String>,
    note_pool: Vec<String>,
    base_note: Option<String>,
    seed_offset: u64,
    resolution_ticks: IntRangeU16,
    base_success: StatRange,
    success_threshold: StatRange,
    fidelity_gain: StatRange,
    suspicion_on_success: StatRange,
    suspicion_on_failure: StatRange,
    cell_gain_on_success: IntRangeU8,
    suspicion_relief: StatRange,
    fidelity_suppression: StatRange,
}

impl EspionageMissionGenerator {
    fn from_catalog_entry(
        base: &EspionageMissionTemplate,
        generator: EspionageMissionGeneratorEntry,
        defaults: &MissionGeneratorDefaults,
    ) -> Self {
        let resolution_ticks = resolve_u16_range(
            generator.resolution_ticks,
            Some((base.resolution_ticks, base.resolution_ticks)),
            defaults.resolution_ticks(),
        );
        let base_success = resolve_stat_range(
            generator.base_success,
            Some((base.base_success.to_f32(), base.base_success.to_f32())),
            defaults.base_success(),
        );
        let success_threshold = resolve_stat_range(
            generator.success_threshold,
            Some((
                base.success_threshold.to_f32(),
                base.success_threshold.to_f32(),
            )),
            defaults.success_threshold(),
        );
        let fidelity_gain = resolve_stat_range(
            generator.fidelity_gain,
            Some((base.fidelity_gain.to_f32(), base.fidelity_gain.to_f32())),
            defaults.fidelity_gain(),
        );
        let suspicion_on_success = resolve_stat_range(
            generator.suspicion_on_success,
            Some((
                base.suspicion_on_success.to_f32(),
                base.suspicion_on_success.to_f32(),
            )),
            defaults.suspicion_on_success(),
        );
        let suspicion_on_failure = resolve_stat_range(
            generator.suspicion_on_failure,
            Some((
                base.suspicion_on_failure.to_f32(),
                base.suspicion_on_failure.to_f32(),
            )),
            defaults.suspicion_on_failure(),
        );
        let cell_gain_on_success = resolve_u8_range(
            generator.cell_gain_on_success,
            Some((base.cell_gain_on_success, base.cell_gain_on_success)),
            defaults.cell_gain_on_success(),
        );
        let suspicion_relief = resolve_stat_range(
            generator.suspicion_relief,
            Some((
                base.suspicion_relief.to_f32(),
                base.suspicion_relief.to_f32(),
            )),
            defaults.suspicion_relief(),
        );
        let fidelity_suppression = resolve_stat_range(
            generator.fidelity_suppression,
            Some((
                base.fidelity_suppression.to_f32(),
                base.fidelity_suppression.to_f32(),
            )),
            defaults.fidelity_suppression(),
        );

        let seed_offset = generator
            .seed_offset
            .unwrap_or_else(|| hash_identifier(&base.id.0));

        Self {
            variant_count: generator.variant_count,
            id_pattern: generator.id_pattern,
            name_pool: generator.name_pool.unwrap_or_default(),
            note_pool: generator.note_pool.unwrap_or_default(),
            base_note: generator.note.or_else(|| base.note.clone()),
            seed_offset,
            resolution_ticks,
            base_success,
            success_threshold,
            fidelity_gain,
            suspicion_on_success,
            suspicion_on_failure,
            cell_gain_on_success,
            suspicion_relief,
            fidelity_suppression,
        }
    }

    fn generate_variants(&self, base: &EspionageMissionTemplate) -> Vec<EspionageMissionTemplate> {
        let mut variants = Vec::with_capacity(self.variant_count as usize);
        for index in 0..self.variant_count {
            let mut rng = self.rng_for_variant(index);
            variants.push(self.build_variant(base, index, &mut rng));
        }
        variants
    }

    fn rng_for_variant(&self, index: u8) -> SmallRng {
        let seed = self.seed_offset ^ ((index as u64 + 1) << 12);
        SmallRng::seed_from_u64(seed)
    }

    fn build_variant(
        &self,
        base: &EspionageMissionTemplate,
        index: u8,
        rng: &mut SmallRng,
    ) -> EspionageMissionTemplate {
        let variant_id = self.variant_id(&base.id, index);
        let variant_name = self.variant_name(&base.name, rng, index);
        let note = self.variant_note(rng, &base.note);

        EspionageMissionTemplate {
            id: variant_id,
            name: variant_name,
            kind: base.kind,
            resolution_ticks: self.resolution_ticks.sample(rng),
            base_success: self.base_success.sample(rng),
            success_threshold: self.success_threshold.sample(rng),
            stealth_weight: base.stealth_weight,
            recon_weight: base.recon_weight,
            counter_intel_weight: base.counter_intel_weight,
            fidelity_gain: self.fidelity_gain.sample(rng),
            suspicion_on_success: self.suspicion_on_success.sample(rng),
            suspicion_on_failure: self.suspicion_on_failure.sample(rng),
            cell_gain_on_success: self.cell_gain_on_success.sample(rng),
            countermeasure: base.countermeasure.clone(),
            suspicion_relief: self.suspicion_relief.sample(rng),
            fidelity_suppression: self.fidelity_suppression.sample(rng),
            note,
            target_tier_min: base.target_tier_min,
            target_tier_max: base.target_tier_max,
            generated: true,
        }
    }

    fn variant_id(&self, base_id: &EspionageMissionId, index: u8) -> EspionageMissionId {
        let base = &base_id.0;
        let value = if let Some(pattern) = &self.id_pattern {
            pattern
                .replace("{base}", base)
                .replace("{index}", &(index as usize + 1).to_string())
        } else {
            format!("{}::{}", base, index as usize + 1)
        };
        EspionageMissionId(value)
    }

    fn variant_name(&self, base_name: &str, rng: &mut SmallRng, index: u8) -> String {
        if let Some(label) = self.name_pool.choose(rng) {
            format!("{} — {}", base_name, label)
        } else {
            format!("{} Variant {}", base_name, index as usize + 1)
        }
    }

    fn variant_note(&self, rng: &mut SmallRng, base_note: &Option<String>) -> Option<String> {
        if let Some(note) = self.note_pool.choose(rng) {
            Some(note.clone())
        } else if let Some(note) = &self.base_note {
            Some(note.clone())
        } else {
            base_note.clone()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct IntRangeU16 {
    min: u16,
    max: u16,
}

impl IntRangeU16 {
    fn new(min: u16, max: u16) -> Self {
        if min <= max {
            Self { min, max }
        } else {
            Self { min: max, max: min }
        }
    }

    fn sample(&self, rng: &mut SmallRng) -> u16 {
        if self.min == self.max {
            self.min
        } else {
            rng.gen_range(self.min..=self.max)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct IntRangeU8 {
    min: u8,
    max: u8,
}

impl IntRangeU8 {
    fn new(min: u8, max: u8) -> Self {
        if min <= max {
            Self { min, max }
        } else {
            Self { min: max, max: min }
        }
    }

    fn sample(&self, rng: &mut SmallRng) -> u8 {
        if self.min == self.max {
            self.min
        } else {
            rng.gen_range(self.min..=self.max)
        }
    }
}

fn resolve_stat_range(
    band: Option<GeneratorStatBandEntry>,
    fallback: Option<(f32, f32)>,
    default: (f32, f32),
) -> StatRange {
    let (fallback_min, fallback_max) = fallback.unwrap_or(default);
    let min = band
        .as_ref()
        .and_then(|entry| entry.min)
        .unwrap_or(fallback_min);
    let max = band
        .as_ref()
        .and_then(|entry| entry.max)
        .unwrap_or(fallback_max);
    StatRange::new(min, max)
}

fn resolve_u16_range(
    band: Option<GeneratorU16BandEntry>,
    fallback: Option<(u16, u16)>,
    default: (u16, u16),
) -> IntRangeU16 {
    let (fallback_min, fallback_max) = fallback.unwrap_or(default);
    let min = band
        .as_ref()
        .and_then(|entry| entry.min)
        .unwrap_or(fallback_min);
    let max = band
        .as_ref()
        .and_then(|entry| entry.max)
        .unwrap_or(fallback_max);
    IntRangeU16::new(min, max)
}

fn resolve_u8_range(
    band: Option<GeneratorU8BandEntry>,
    fallback: Option<(u8, u8)>,
    default: (u8, u8),
) -> IntRangeU8 {
    let (fallback_min, fallback_max) = fallback.unwrap_or(default);
    let min = band
        .as_ref()
        .and_then(|entry| entry.min)
        .unwrap_or(fallback_min);
    let max = band
        .as_ref()
        .and_then(|entry| entry.max)
        .unwrap_or(fallback_max);
    IntRangeU8::new(min, max)
}

fn hash_identifier(identifier: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    identifier.hash(&mut hasher);
    hasher.finish()
}

pub fn initialise_espionage_roster(
    mut roster: ResMut<EspionageRoster>,
    catalog: Res<EspionageCatalog>,
    factions: Res<FactionRegistry>,
) {
    roster.seed_from_catalog(&factions.factions, &catalog);
}

#[derive(Resource, Debug)]
pub struct CounterIntelBudgets {
    reserves: HashMap<FactionId, Scalar>,
}

impl CounterIntelBudgets {
    pub fn new(factions: &[FactionId], config: &CounterIntelBudgetConfig) -> Self {
        let mut reserves = HashMap::new();
        let initial = config.initial_reserve();
        for faction in factions {
            reserves.insert(*faction, initial);
        }
        Self { reserves }
    }

    pub fn regenerate(&mut self, config: &CounterIntelBudgetConfig) {
        let regen = config.regen_per_tick();
        let max_reserve = config.max_reserve();
        if regen <= Scalar::zero() {
            return;
        }
        for value in self.reserves.values_mut() {
            let mut updated = *value + regen;
            if updated > max_reserve {
                updated = max_reserve;
            }
            *value = updated;
        }
    }

    pub fn available(&self, faction: FactionId) -> Scalar {
        self.reserves
            .get(&faction)
            .copied()
            .unwrap_or_else(scalar_zero)
    }

    pub fn set_reserve(
        &mut self,
        faction: FactionId,
        amount: Scalar,
        config: &CounterIntelBudgetConfig,
    ) -> Scalar {
        let mut value = amount;
        if value < Scalar::zero() {
            value = Scalar::zero();
        }
        let max_reserve = config.max_reserve();
        if value > max_reserve {
            value = max_reserve;
        }
        self.reserves.insert(faction, value);
        value
    }

    pub fn adjust_reserve(
        &mut self,
        faction: FactionId,
        delta: Scalar,
        config: &CounterIntelBudgetConfig,
    ) -> Scalar {
        let current = self.available(faction);
        let mut value = current + delta;
        if value < Scalar::zero() {
            value = Scalar::zero();
        }
        let max_reserve = config.max_reserve();
        if value > max_reserve {
            value = max_reserve;
        }
        self.reserves.insert(faction, value);
        value
    }

    pub fn try_spend(
        &mut self,
        faction: FactionId,
        config: &CounterIntelBudgetConfig,
        policy: SecurityPolicy,
    ) -> Result<Scalar, Scalar> {
        let cost = config.sweep_cost();
        let min_reserve = config.min_reserve();
        let allow_overdraft = matches!(policy, SecurityPolicy::Crisis);
        let entry = self.reserves.entry(faction).or_insert_with(scalar_zero);
        if allow_overdraft {
            let remaining = if *entry > cost {
                *entry - cost
            } else {
                Scalar::zero()
            };
            *entry = remaining;
            return Ok(cost);
        }

        let available = *entry;
        if available < cost {
            return Err(available);
        }

        let remaining = available - cost;
        if remaining < min_reserve {
            return Err(available);
        }

        *entry = remaining;
        Ok(cost)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityPolicy {
    Lenient,
    Standard,
    Hardened,
    Crisis,
}

#[derive(Resource, Debug)]
pub struct FactionSecurityPolicies {
    policies: HashMap<FactionId, SecurityPolicy>,
    default_policy: SecurityPolicy,
}

#[derive(SystemParam)]
pub struct CounterIntelScheduleParams<'w> {
    pub tick: Res<'w, crate::resources::SimulationTick>,
    pub catalog: Res<'w, EspionageCatalog>,
    pub ledger: Res<'w, KnowledgeLedger>,
    pub roster: ResMut<'w, EspionageRoster>,
    pub missions: ResMut<'w, EspionageMissionState>,
    pub budgets: ResMut<'w, CounterIntelBudgets>,
    pub policies: Res<'w, FactionSecurityPolicies>,
    pub metrics: ResMut<'w, SimulationMetrics>,
}

impl FactionSecurityPolicies {
    pub fn new(factions: &[FactionId], default_policy: SecurityPolicy) -> Self {
        let mut policies = HashMap::new();
        for faction in factions {
            policies.insert(*faction, default_policy);
        }
        Self {
            policies,
            default_policy,
        }
    }

    pub fn policy(&self, faction: FactionId) -> SecurityPolicy {
        self.policies
            .get(&faction)
            .copied()
            .unwrap_or(self.default_policy)
    }

    pub fn set_policy(&mut self, faction: FactionId, policy: SecurityPolicy) {
        self.policies.insert(faction, policy);
    }
}

struct CounterIntelCandidate {
    owner: FactionId,
    discovery_id: u32,
    tier: Option<u8>,
    infiltration_active: bool,
    total_suspicion: Scalar,
    max_suspicion: Scalar,
    progress_percent: u16,
}

impl CounterIntelCandidate {
    fn from_entry(entry: &KnowledgeLedgerEntry) -> Self {
        let infiltration_active = !entry.infiltrations.is_empty();
        let mut total_suspicion = Scalar::zero();
        let mut max_suspicion = Scalar::zero();
        for inf in &entry.infiltrations {
            total_suspicion += inf.suspicion;
            if inf.suspicion > max_suspicion {
                max_suspicion = inf.suspicion;
            }
        }
        let tier = if entry.tier > 0 {
            Some(entry.tier)
        } else {
            None
        };

        Self {
            owner: entry.owner_faction,
            discovery_id: entry.discovery_id,
            tier,
            infiltration_active,
            total_suspicion,
            max_suspicion,
            progress_percent: entry.progress_percent,
        }
    }
}

fn has_active_counterintel_countermeasure(entry: &KnowledgeLedgerEntry) -> bool {
    entry.countermeasures.iter().any(|cm| {
        cm.kind == KnowledgeCountermeasureKind::CounterIntelSweep && cm.remaining_ticks > 0
    })
}

fn has_pending_counterintel_mission(
    missions: &EspionageMissionState,
    catalog: &EspionageCatalog,
    owner: FactionId,
    discovery_id: u32,
) -> bool {
    missions.missions().iter().any(|mission| {
        mission.owner == owner
            && mission.discovery_id == discovery_id
            && catalog
                .mission(&mission.mission_id)
                .map(|template| template.kind == EspionageMissionKind::CounterIntel)
                .unwrap_or(false)
    })
}

fn select_counterintel_template<'a>(
    templates: &'a [&EspionageMissionTemplate],
    target_tier: Option<u8>,
) -> Option<&'a EspionageMissionTemplate> {
    templates
        .iter()
        .copied()
        .filter(|template| template.is_valid_for_tier(target_tier))
        .max_by(|a, b| {
            a.base_success
                .cmp(&b.base_success)
                .then_with(|| a.counter_intel_weight.cmp(&b.counter_intel_weight))
        })
}

fn pick_best_counter_agent(
    roster: &EspionageRoster,
    faction: FactionId,
) -> Option<EspionageAgentHandle> {
    roster
        .agents_for(faction)
        .iter()
        .filter(|agent| matches!(agent.assignment, AgentAssignment::Available))
        .max_by(|a, b| {
            a.counter_intel
                .cmp(&b.counter_intel)
                .then_with(|| a.recon.cmp(&b.recon))
        })
        .map(|agent| agent.handle)
}

const STANDARD_PROGRESS_THRESHOLD: u16 = 70;

fn policy_allows_auto_queue(
    policy: SecurityPolicy,
    candidate: &CounterIntelCandidate,
    config: &CounterIntelBudgetConfig,
) -> bool {
    match policy {
        SecurityPolicy::Standard => {
            candidate.infiltration_active
                || candidate.progress_percent >= STANDARD_PROGRESS_THRESHOLD
        }
        SecurityPolicy::Hardened => {
            candidate.infiltration_active
                || candidate.progress_percent >= config.hardened_progress_threshold()
                || candidate.tier.map(|tier| tier >= 2).unwrap_or(false)
        }
        SecurityPolicy::Lenient => {
            (candidate.infiltration_active
                && candidate.max_suspicion >= config.lenient_suspicion_threshold())
                || candidate.progress_percent >= config.lenient_progress_threshold()
        }
        SecurityPolicy::Crisis => {
            candidate.infiltration_active
                || candidate.progress_percent >= config.hardened_progress_threshold()
        }
    }
}

fn log_budget_shortfall(
    faction: FactionId,
    discovery_id: u32,
    available: Scalar,
    required: Scalar,
) {
    warn!(
        target: "shadow_scale::espionage",
        "counter_intel_budget.insufficient faction={} discovery_id={} available={:.3} required={:.3}",
        faction,
        discovery_id,
        available.to_f32(),
        required.to_f32()
    );
}

pub fn schedule_counter_intel_missions(params: CounterIntelScheduleParams) {
    let CounterIntelScheduleParams {
        tick,
        catalog,
        ledger,
        mut roster,
        mut missions,
        mut budgets,
        policies,
        mut metrics,
    } = params;

    let counter_templates: Vec<&EspionageMissionTemplate> = catalog
        .missions()
        .filter(|mission| mission.kind == EspionageMissionKind::CounterIntel)
        .collect();

    if counter_templates.is_empty() {
        return;
    }

    let budget_config = catalog.config().counter_intel_budget().clone();

    let mut candidates: Vec<CounterIntelCandidate> = ledger
        .entries()
        .filter(|entry| !has_active_counterintel_countermeasure(entry))
        .map(CounterIntelCandidate::from_entry)
        .collect();

    if candidates.is_empty() {
        return;
    }

    candidates.sort_by(|a, b| {
        b.infiltration_active
            .cmp(&a.infiltration_active)
            .then_with(|| b.total_suspicion.cmp(&a.total_suspicion))
            .then_with(|| b.max_suspicion.cmp(&a.max_suspicion))
            .then_with(|| b.progress_percent.cmp(&a.progress_percent))
            .then_with(|| a.discovery_id.cmp(&b.discovery_id))
    });

    for candidate in candidates {
        let policy = policies.policy(candidate.owner);
        if !policy_allows_auto_queue(policy, &candidate, &budget_config) {
            continue;
        }

        if has_pending_counterintel_mission(
            missions.as_ref(),
            catalog.as_ref(),
            candidate.owner,
            candidate.discovery_id,
        ) {
            continue;
        }

        let Some(template) = select_counterintel_template(&counter_templates, candidate.tier)
        else {
            continue;
        };

        let Some(agent_handle) = pick_best_counter_agent(&roster, candidate.owner) else {
            continue;
        };

        let spend_result = budgets.try_spend(candidate.owner, &budget_config, policy);
        let cost = match spend_result {
            Ok(cost) => cost,
            Err(available) => {
                log_budget_shortfall(
                    candidate.owner,
                    candidate.discovery_id,
                    available,
                    budget_config.sweep_cost(),
                );
                continue;
            }
        };

        let params = QueueMissionParams {
            mission_id: template.id.clone(),
            owner: candidate.owner,
            target_owner: candidate.owner,
            discovery_id: candidate.discovery_id,
            agent: agent_handle,
            target_tier: candidate.tier,
            scheduled_tick: tick.0,
        };

        if missions
            .queue_mission(catalog.as_ref(), &mut roster, params)
            .is_err()
        {
            // refund on failure to queue
            budgets
                .reserves
                .entry(candidate.owner)
                .and_modify(|reserve| {
                    *reserve = (*reserve + cost).min(budget_config.max_reserve())
                });
            continue;
        }

        metrics.knowledge_counterintel_budget_spent += cost.to_f32() as f64;
        info!(
            target: "shadow_scale::espionage",
            "counter_intel_budget.spent faction={} discovery_id={} cost={:.3} policy={:?} available_after={:.3}",
            candidate.owner,
            candidate.discovery_id,
            cost.to_f32(),
            policy,
            budgets.available(candidate.owner).to_f32()
        );
    }
}

pub fn refresh_counter_intel_budgets(
    catalog: Res<EspionageCatalog>,
    mut budgets: ResMut<CounterIntelBudgets>,
) {
    budgets.regenerate(catalog.config().counter_intel_budget());
}

pub fn resolve_espionage_missions(
    tick: Res<crate::resources::SimulationTick>,
    catalog: Res<EspionageCatalog>,
    mut roster: ResMut<EspionageRoster>,
    mut missions: ResMut<EspionageMissionState>,
    mut probe_writer: EventWriter<EspionageProbeEvent>,
    mut sweep_writer: EventWriter<CounterIntelSweepEvent>,
    ledger: Res<KnowledgeLedger>,
) {
    let mut resolved_instances: Vec<EspionageMissionInstanceId> = Vec::new();

    for mission in missions.active.iter_mut() {
        if tick.0 < mission.scheduled_tick {
            continue;
        }
        if mission.ticks_remaining > 0 {
            mission.ticks_remaining -= 1;
        }
        if mission.ticks_remaining == 0 {
            let outcome = determine_mission_outcome(
                tick.0,
                &catalog,
                &ledger,
                mission,
                roster
                    .agent(mission.owner, mission.agent)
                    .expect("assigned agent should exist"),
            );

            if let Some(probe) = outcome.probe_event {
                probe_writer.send(probe);
            }
            if let Some(sweep) = outcome.sweep_event {
                sweep_writer.send(sweep);
            }

            if let Some(agent) = roster.agent_mut(mission.owner, mission.agent) {
                agent.assignment = AgentAssignment::Available;
            }

            resolved_instances.push(mission.instance_id);
        }
    }

    missions
        .active
        .retain(|mission| !resolved_instances.contains(&mission.instance_id));
}

fn determine_mission_outcome(
    tick: u64,
    catalog: &EspionageCatalog,
    ledger: &KnowledgeLedger,
    mission: &ScheduledEspionageMission,
    agent: &EspionageAgent,
) -> MissionOutcome {
    let mut outcome = MissionOutcome::default();
    let template = catalog
        .mission(&mission.mission_id)
        .expect("mission definition should exist");
    let config = catalog.config();
    let probe_tuning = config.probe_resolution();
    let counter_tuning = config.counter_intel_resolution();

    let security_posture = ledger
        .entry(mission.target_owner, mission.discovery_id)
        .map(|entry| entry.security_posture)
        .unwrap_or(sim_runtime::KnowledgeSecurityPosture::Standard);

    let security_penalty = config.security_penalty(security_posture);

    match template.kind {
        EspionageMissionKind::Probe => {
            let suspicion_penalty = ledger
                .entry(mission.target_owner, mission.discovery_id)
                .and_then(|entry| {
                    entry
                        .infiltrations
                        .iter()
                        .find(|infiltration| infiltration.faction == mission.owner)
                })
                .map(|record| record.suspicion)
                .unwrap_or_else(scalar_zero);

            let success_score = template.base_success
                + agent.stealth * template.stealth_weight
                + agent.recon * template.recon_weight
                - security_penalty
                - suspicion_penalty;

            let success_threshold = template.success_threshold;
            let partial_threshold = if probe_tuning.partial_margin() > 0.0 {
                Scalar::from_f32(
                    (success_threshold.to_f32() - probe_tuning.partial_margin()).max(0.0),
                )
            } else {
                success_threshold
            };

            let base_fidelity_gain =
                template.fidelity_gain + agent.recon * probe_tuning.recon_fidelity_bonus();
            let suspicion_floor = probe_tuning.suspicion_floor();
            let mut base_suspicion_gain = template.suspicion_on_success - agent.stealth;
            if base_suspicion_gain < suspicion_floor {
                base_suspicion_gain = suspicion_floor;
            }

            if success_score >= success_threshold {
                outcome.probe_event = Some(EspionageProbeEvent {
                    owner: mission.target_owner,
                    discovery_id: mission.discovery_id,
                    infiltrator: mission.owner,
                    fidelity_gain: base_fidelity_gain,
                    suspicion_gain: base_suspicion_gain,
                    cells: template.cell_gain_on_success,
                    tick,
                    note: mission
                        .note
                        .clone()
                        .or_else(|| Some(format!("{} succeeded", template.name))),
                });
            } else if success_score >= partial_threshold {
                let fidelity_scalar = Scalar::from_f32(probe_tuning.partial_fidelity_scalar());
                let suspicion_scalar = Scalar::from_f32(probe_tuning.partial_suspicion_scalar());
                let mut partial_suspicion_gain = base_suspicion_gain * suspicion_scalar;
                if partial_suspicion_gain < suspicion_floor {
                    partial_suspicion_gain = suspicion_floor;
                }
                let partial_fidelity_gain = base_fidelity_gain * fidelity_scalar;
                let partial_cells = (template.cell_gain_on_success as u16)
                    .div_ceil(2)
                    .max(1)
                    .min(u8::MAX as u16) as u8;

                outcome.probe_event = Some(EspionageProbeEvent {
                    owner: mission.target_owner,
                    discovery_id: mission.discovery_id,
                    infiltrator: mission.owner,
                    fidelity_gain: partial_fidelity_gain,
                    suspicion_gain: partial_suspicion_gain,
                    cells: partial_cells,
                    tick,
                    note: mission
                        .note
                        .clone()
                        .or_else(|| Some(format!("{} achieved partial success", template.name))),
                });
            } else {
                outcome.probe_event = Some(EspionageProbeEvent {
                    owner: mission.target_owner,
                    discovery_id: mission.discovery_id,
                    infiltrator: mission.owner,
                    fidelity_gain: probe_tuning.failure_misinformation_fidelity(),
                    suspicion_gain: template.suspicion_on_failure
                        + security_penalty
                        + probe_tuning.failure_extra_suspicion(),
                    cells: 0,
                    tick,
                    note: mission
                        .note
                        .clone()
                        .or_else(|| Some(format!("{} detected (misinformation)", template.name))),
                });
            }
        }
        EspionageMissionKind::CounterIntel => {
            let success_score = template.base_success
                + agent.counter_intel * template.counter_intel_weight
                - security_penalty * counter_tuning.security_penalty_factor();

            if success_score >= template.success_threshold {
                let countermeasure = template
                    .countermeasure
                    .clone()
                    .unwrap_or_else(|| counter_tuning.default_countermeasure());

                let cleared_faction =
                    ledger
                        .entry(mission.owner, mission.discovery_id)
                        .and_then(|entry| {
                            entry
                                .infiltrations
                                .iter()
                                .max_by_key(|inf| inf.suspicion)
                                .map(|inf| inf.faction)
                        });

                outcome.sweep_event = Some(CounterIntelSweepEvent {
                    owner: mission.owner,
                    discovery_id: mission.discovery_id,
                    countermeasure: KnowledgeCountermeasure {
                        kind: countermeasure.kind,
                        potency: countermeasure.potency,
                        upkeep: countermeasure.upkeep,
                        remaining_ticks: countermeasure.duration_ticks,
                    },
                    tick,
                    note: mission
                        .note
                        .clone()
                        .or_else(|| Some(format!("{} succeeded", template.name))),
                    cleared_faction,
                    suspicion_relief: counter_tuning.suspicion_relief(),
                });
            } else {
                // No direct ledger effect beyond the failed attempt note.
            }
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge_ledger::{
        self, InfiltrationRecord, KnowledgeLedger, KnowledgeLedgerConfigHandle,
        KnowledgeLedgerEntry,
    };
    use crate::metrics::SimulationMetrics;
    use crate::orders::FactionRegistry;
    use crate::resources::SimulationTick;
    use bevy::app::App;
    use bevy::ecs::event::Events;
    use bevy::ecs::world::Mut;
    use bevy_ecs::system::RunSystemOnce;

    fn setup_app_with_catalog(factions: &[FactionId]) -> App {
        let mut app = App::new();
        app.add_event::<EspionageProbeEvent>();
        app.add_event::<CounterIntelSweepEvent>();

        let catalog = EspionageCatalog::load_builtin().expect("catalog parses");
        let mut roster = EspionageRoster::default();
        roster.seed_from_catalog(factions, &catalog);

        let knowledge_config_handle = KnowledgeLedgerConfigHandle::load_builtin();
        let knowledge_ledger = KnowledgeLedger::with_config(knowledge_config_handle.get());

        app.insert_resource(SimulationTick(0));
        app.insert_resource(SimulationMetrics::default());
        app.insert_resource(knowledge_config_handle);
        app.insert_resource(knowledge_ledger);
        app.insert_resource(FactionRegistry::new(factions.to_vec()));
        let budget_config = catalog.config().counter_intel_budget().clone();
        app.insert_resource(catalog);
        app.insert_resource(roster);
        app.insert_resource(EspionageMissionState::default());
        app.insert_resource(CounterIntelBudgets::new(factions, &budget_config));
        app.insert_resource(FactionSecurityPolicies::new(
            factions,
            SecurityPolicy::Standard,
        ));

        app
    }

    #[test]
    fn catalog_loads_builtin_content() {
        let catalog = EspionageCatalog::load_builtin().expect("catalog parses");
        assert!(
            catalog.agents().count() > 0,
            "expected at least one agent template"
        );
        assert!(
            catalog.missions().count() > 0,
            "expected at least one mission template"
        );
        assert!(
            catalog.generators().count() > 0,
            "expected at least one generated agent template"
        );
    }

    #[test]
    fn probe_mission_emits_probe_event() {
        let infiltrator = FactionId(1);
        let target = FactionId(0);
        let mut app = setup_app_with_catalog(&[target, infiltrator]);

        let agent_handle = {
            let roster = app.world.resource::<EspionageRoster>();
            let agents = roster.agents_for(infiltrator);
            assert!(!agents.is_empty(), "seeded roster should contain agents");
            let agent = agents
                .iter()
                .find(|agent| !agent.generated)
                .expect("expected at least one handcrafted agent");
            agent.handle
        };

        let mission_id = EspionageMissionId::new("trade_intercept");

        app.world
            .resource_scope(|world, mut mission_state: Mut<EspionageMissionState>| {
                world.resource_scope(|world, mut roster: Mut<EspionageRoster>| {
                    let catalog = world.resource::<EspionageCatalog>();
                    mission_state
                        .queue_mission(
                            catalog,
                            &mut roster,
                            QueueMissionParams {
                                mission_id: mission_id.clone(),
                                owner: infiltrator,
                                target_owner: target,
                                discovery_id: 101,
                                agent: agent_handle,
                                target_tier: Some(1),
                                scheduled_tick: 0,
                            },
                        )
                        .expect("mission queued");
                });
            });

        app.world.run_system_once(resolve_espionage_missions);

        {
            let missions = app.world.resource::<EspionageMissionState>();
            assert!(
                missions.missions().is_empty(),
                "mission should resolve in a single tick"
            );
        }

        {
            let roster = app.world.resource::<EspionageRoster>();
            let agent = roster
                .agent(infiltrator, agent_handle)
                .expect("agent should exist");
            assert!(
                matches!(agent.assignment, AgentAssignment::Available),
                "agent should be available after mission resolves"
            );
        }

        let events: Vec<EspionageProbeEvent> = {
            let mut probes = app.world.resource_mut::<Events<EspionageProbeEvent>>();
            probes.drain().collect()
        };

        assert_eq!(events.len(), 1, "expected a single probe event");
        let event = &events[0];
        assert_eq!(event.owner, target);
        assert_eq!(event.infiltrator, infiltrator);
        assert!(event.cells > 0, "successful mission should add cells");
        assert!(
            event.fidelity_gain > scalar_zero(),
            "successful mission should gain fidelity"
        );
    }

    #[test]
    fn counter_intel_mission_emits_sweep_and_applies_countermeasure() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            let (entry, probe_event) = {
                let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
                let mut entry = KnowledgeLedgerEntry::new(owner, 77, config_handle.get().as_ref());
                entry.security_posture = sim_runtime::KnowledgeSecurityPosture::Standard;
                let probe_event = EspionageProbeEvent {
                    owner,
                    discovery_id: 77,
                    infiltrator: FactionId(42),
                    fidelity_gain: Scalar::from_f32(0.4),
                    suspicion_gain: Scalar::from_f32(0.6),
                    cells: 2,
                    tick: 0,
                    note: None,
                };
                (entry, probe_event)
            };
            let mut ledger = app.world.resource_mut::<KnowledgeLedger>();
            ledger.upsert_entry(entry);
            ledger.record_espionage_probe(probe_event);
        }

        let agent_handle = {
            let roster = app.world.resource::<EspionageRoster>();
            let agents = roster.agents_for(owner);
            assert!(!agents.is_empty(), "expected defensive agents for owner");
            let defensive_agent = agents
                .iter()
                .find(|agent| !agent.generated && agent.tags.contains(&"counter_intel".to_string()))
                .or_else(|| {
                    agents
                        .iter()
                        .find(|agent| agent.tags.contains(&"counter_intel".to_string()))
                })
                .expect("expected counter-intel capable agent");
            defensive_agent.handle
        };

        let mission_id = EspionageMissionId::new("rapid_response_sweep");

        app.world
            .resource_scope(|world, mut mission_state: Mut<EspionageMissionState>| {
                world.resource_scope(|world, mut roster: Mut<EspionageRoster>| {
                    let catalog = world.resource::<EspionageCatalog>();
                    mission_state
                        .queue_mission(
                            catalog,
                            &mut roster,
                            QueueMissionParams {
                                mission_id: mission_id.clone(),
                                owner,
                                target_owner: owner,
                                discovery_id: 77,
                                agent: agent_handle,
                                target_tier: None,
                                scheduled_tick: 0,
                            },
                        )
                        .expect("mission queued");
                });
            });

        app.world.run_system_once(resolve_espionage_missions);

        let sweep_events: Vec<CounterIntelSweepEvent> = {
            let mut sweeps = app.world.resource_mut::<Events<CounterIntelSweepEvent>>();
            let drained: Vec<_> = sweeps.drain().collect();
            for event in &drained {
                sweeps.send(event.clone());
            }
            drained
        };

        assert_eq!(sweep_events.len(), 1, "expected one sweep event");
        let sweep = &sweep_events[0];
        assert_eq!(sweep.owner, owner);
        assert_eq!(sweep.discovery_id, 77);
        assert!(
            sweep.countermeasure.potency > scalar_zero(),
            "sweep should apply positive potency"
        );
        assert!(
            sweep.suspicion_relief > scalar_zero(),
            "counter-intel sweeps should relieve suspicion"
        );
        assert!(
            sweep.cleared_faction.is_some(),
            "sweep should identify a cleared infiltration"
        );

        app.world
            .run_system_once(knowledge_ledger::process_espionage_events);

        {
            let ledger = app.world.resource::<KnowledgeLedger>();
            let entry = ledger
                .entry(owner, 77)
                .expect("entry should exist after sweep");
            assert_eq!(
                entry.countermeasures.len(),
                1,
                "countermeasure should be registered"
            );
            assert_eq!(
                entry.countermeasures[0].remaining_ticks,
                sweep.countermeasure.remaining_ticks
            );
            assert!(
                entry.infiltrations.is_empty(),
                "infiltration cells should be cleared"
            );
        }
    }

    #[test]
    fn auto_scheduler_queues_counter_intel_for_active_infiltration() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let config = config_handle.get();
            let mut entry = KnowledgeLedgerEntry::new(owner, 55, config.as_ref());
            entry.tier = 2;
            entry.register_infiltration(
                InfiltrationRecord {
                    faction: FactionId(99),
                    blueprint_fidelity: scalar_from_f32(0.4),
                    suspicion: scalar_from_f32(0.7),
                    cells: 2,
                    last_activity_tick: 0,
                },
                config.max_suspicion(),
            );
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        let (mission_id, agent_handle) = {
            let missions = app.world.resource::<EspionageMissionState>();
            assert_eq!(
                missions.missions().len(),
                1,
                "expected scheduler to enqueue a mission"
            );
            let scheduled = &missions.missions()[0];
            assert_eq!(scheduled.owner, owner);
            (scheduled.mission_id.clone(), scheduled.agent)
        };

        {
            let catalog = app.world.resource::<EspionageCatalog>();
            let template = catalog
                .mission(&mission_id)
                .expect("scheduled mission template should exist");
            assert_eq!(template.kind, EspionageMissionKind::CounterIntel);
        }

        {
            let roster = app.world.resource::<EspionageRoster>();
            let agent = roster
                .agent(owner, agent_handle)
                .expect("agent should exist after scheduling");
            assert!(matches!(agent.assignment, AgentAssignment::Assigned(_)));
        }
    }

    #[test]
    fn auto_scheduler_respects_active_countermeasures() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let config = config_handle.get();
            let mut entry = KnowledgeLedgerEntry::new(owner, 91, config.as_ref());
            entry.register_infiltration(
                InfiltrationRecord {
                    faction: FactionId(77),
                    blueprint_fidelity: scalar_from_f32(0.3),
                    suspicion: scalar_from_f32(0.6),
                    cells: 1,
                    last_activity_tick: 0,
                },
                config.max_suspicion(),
            );
            entry.countermeasures.push(KnowledgeCountermeasure {
                kind: KnowledgeCountermeasureKind::CounterIntelSweep,
                potency: scalar_from_f32(0.3),
                upkeep: scalar_from_f32(0.05),
                remaining_ticks: 2,
            });
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        let missions = app.world.resource::<EspionageMissionState>();
        assert!(
            missions.missions().is_empty(),
            "scheduler should avoid queuing when a sweep countermeasure is active"
        );
    }

    #[test]
    fn auto_scheduler_skips_when_budget_insufficient() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let mut entry = KnowledgeLedgerEntry::new(owner, 101, config_handle.get().as_ref());
            entry.tier = 2;
            entry.register_infiltration(
                InfiltrationRecord {
                    faction: FactionId(99),
                    blueprint_fidelity: scalar_from_f32(0.4),
                    suspicion: scalar_from_f32(0.6),
                    cells: 1,
                    last_activity_tick: 0,
                },
                config_handle.get().max_suspicion(),
            );
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        {
            let config = app
                .world
                .resource::<EspionageCatalog>()
                .config()
                .counter_intel_budget()
                .clone();
            let sweep_cost = config.sweep_cost();
            app.world.resource_mut::<CounterIntelBudgets>().set_reserve(
                owner,
                sweep_cost / Scalar::from_f32(4.0),
                &config,
            );
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        let missions = app.world.resource::<EspionageMissionState>();
        assert!(
            missions.missions().is_empty(),
            "budget shortfall should block scheduling"
        );
    }

    #[test]
    fn lenient_policy_requires_high_suspicion() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            app.world
                .resource_mut::<FactionSecurityPolicies>()
                .set_policy(owner, SecurityPolicy::Lenient);
        }

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let mut entry = KnowledgeLedgerEntry::new(owner, 111, config_handle.get().as_ref());
            entry.tier = 1;
            entry.progress_percent = 72;
            entry.register_infiltration(
                InfiltrationRecord {
                    faction: FactionId(42),
                    blueprint_fidelity: scalar_from_f32(0.2),
                    suspicion: scalar_from_f32(0.2),
                    cells: 1,
                    last_activity_tick: 0,
                },
                config_handle.get().max_suspicion(),
            );
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        {
            let missions = app.world.resource::<EspionageMissionState>();
            assert!(
                missions.missions().is_empty(),
                "lenient policy should not auto-queue with low suspicion"
            );
        }

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let mut entry = KnowledgeLedgerEntry::new(owner, 111, config_handle.get().as_ref());
            entry.tier = 1;
            entry.progress_percent = 72;
            entry.register_infiltration(
                InfiltrationRecord {
                    faction: FactionId(42),
                    blueprint_fidelity: scalar_from_f32(0.2),
                    suspicion: scalar_from_f32(0.85),
                    cells: 1,
                    last_activity_tick: 0,
                },
                config_handle.get().max_suspicion(),
            );
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        let missions = app.world.resource::<EspionageMissionState>();
        assert_eq!(
            missions.missions().len(),
            1,
            "high suspicion should trigger scheduling"
        );
    }

    #[test]
    fn hardened_policy_protects_high_tier_without_infiltration() {
        let owner = FactionId(0);
        let mut app = setup_app_with_catalog(&[owner]);

        {
            app.world
                .resource_mut::<FactionSecurityPolicies>()
                .set_policy(owner, SecurityPolicy::Hardened);
        }

        {
            let config_handle = app.world.resource::<KnowledgeLedgerConfigHandle>();
            let mut entry = KnowledgeLedgerEntry::new(owner, 211, config_handle.get().as_ref());
            entry.tier = 3;
            entry.progress_percent = 40;
            app.world
                .resource_mut::<KnowledgeLedger>()
                .upsert_entry(entry);
        }

        app.world.run_system_once(schedule_counter_intel_missions);

        let missions = app.world.resource::<EspionageMissionState>();
        assert_eq!(
            missions.missions().len(),
            1,
            "hardened policy should protect high-tier secrets"
        );
    }

    #[test]
    fn generated_agents_respect_configured_bands() {
        let faction = FactionId(2);
        let app = setup_app_with_catalog(&[faction]);
        let (stealth_min, stealth_max, recon_min, recon_max, counter_min, counter_max) = {
            let catalog = app.world.resource::<EspionageCatalog>();
            let defaults = catalog.config().agent_defaults();
            let (stealth_min, stealth_max) = defaults.stealth_range();
            let (recon_min, recon_max) = defaults.recon_range();
            let (counter_min, counter_max) = defaults.counter_intel_range();
            (
                stealth_min,
                stealth_max,
                recon_min,
                recon_max,
                counter_min,
                counter_max,
            )
        };
        let roster = app.world.resource::<EspionageRoster>();
        let agents = roster.agents_for(faction);
        let generated: Vec<_> = agents.iter().filter(|agent| agent.generated).collect();
        assert!(
            !generated.is_empty(),
            "expected generated agents to be seeded"
        );
        for agent in generated {
            let stealth = agent.stealth.to_f32();
            let recon = agent.recon.to_f32();
            let counter_intel = agent.counter_intel.to_f32();
            assert!(
                stealth >= stealth_min - 0.001 && stealth <= stealth_max + 0.001,
                "stealth {:.3} out of configured band",
                stealth
            );
            assert!(
                recon >= recon_min - 0.001 && recon <= recon_max + 0.001,
                "recon {:.3} out of configured band",
                recon
            );
            assert!(
                counter_intel >= counter_min - 0.001 && counter_intel <= counter_max + 0.001,
                "counter-intel {:.3} out of configured band",
                counter_intel
            );
            assert!(
                agent.tags.iter().any(|tag| tag == "generated"),
                "generated agents should include the generated tag"
            );
        }
    }

    #[test]
    fn generated_missions_respect_configured_bands() {
        let catalog = EspionageCatalog::load_builtin().expect("catalog parses");
        let mission_defaults = catalog.config().mission_defaults().clone();
        let (base_success_min, base_success_max) = mission_defaults.base_success();
        let (success_threshold_min, success_threshold_max) = mission_defaults.success_threshold();
        let (fidelity_gain_min, fidelity_gain_max) = mission_defaults.fidelity_gain();
        let (suspicion_success_min, suspicion_success_max) =
            mission_defaults.suspicion_on_success();
        let (suspicion_failure_min, suspicion_failure_max) =
            mission_defaults.suspicion_on_failure();
        let (cell_gain_min, cell_gain_max) = mission_defaults.cell_gain_on_success();
        let (suspicion_relief_min, suspicion_relief_max) = mission_defaults.suspicion_relief();
        let (fidelity_suppression_min, fidelity_suppression_max) =
            mission_defaults.fidelity_suppression();
        let (resolution_min, resolution_max) = mission_defaults.resolution_ticks();
        let generated: Vec<&EspionageMissionTemplate> = catalog
            .missions()
            .filter(|mission| mission.generated)
            .collect();
        assert!(
            !generated.is_empty(),
            "expected generated missions to be present"
        );

        for mission in generated {
            let base_success = mission.base_success.to_f32();
            let success_threshold = mission.success_threshold.to_f32();
            let fidelity_gain = mission.fidelity_gain.to_f32();
            let suspicion_success = mission.suspicion_on_success.to_f32();
            let suspicion_failure = mission.suspicion_on_failure.to_f32();
            let suspicion_relief = mission.suspicion_relief.to_f32();
            let fidelity_suppression = mission.fidelity_suppression.to_f32();

            assert!(
                base_success >= base_success_min - 0.001
                    && base_success <= base_success_max + 0.001,
                "base_success {:.3} out of band",
                base_success
            );
            assert!(
                success_threshold >= success_threshold_min - 0.001
                    && success_threshold <= success_threshold_max + 0.001,
                "success_threshold {:.3} out of band",
                success_threshold
            );
            assert!(
                fidelity_gain >= fidelity_gain_min - 0.001
                    && fidelity_gain <= fidelity_gain_max + 0.001,
                "fidelity_gain {:.3} out of band",
                fidelity_gain
            );
            assert!(
                suspicion_success >= suspicion_success_min - 0.001
                    && suspicion_success <= suspicion_success_max + 0.001,
                "suspicion_on_success {:.3} out of band",
                suspicion_success
            );
            assert!(
                suspicion_failure >= suspicion_failure_min - 0.001
                    && suspicion_failure <= suspicion_failure_max + 0.001,
                "suspicion_on_failure {:.3} out of band",
                suspicion_failure
            );
            assert!(
                suspicion_relief >= suspicion_relief_min - 0.001
                    && suspicion_relief <= suspicion_relief_max + 0.001,
                "suspicion_relief {:.3} out of band",
                suspicion_relief
            );
            assert!(
                fidelity_suppression >= fidelity_suppression_min - 0.001
                    && fidelity_suppression <= fidelity_suppression_max + 0.001,
                "fidelity_suppression {:.3} out of band",
                fidelity_suppression
            );
            assert!(
                mission.cell_gain_on_success >= cell_gain_min
                    && mission.cell_gain_on_success <= cell_gain_max,
                "cell_gain_on_success {} out of band",
                mission.cell_gain_on_success
            );
            assert!(
                mission.resolution_ticks >= resolution_min
                    && mission.resolution_ticks <= resolution_max,
                "resolution_ticks {} out of band",
                mission.resolution_ticks
            );
        }
    }
}

#[derive(Deserialize)]
struct EspionageAgentCatalog {
    agents: Vec<EspionageAgentCatalogEntry>,
}

#[derive(Deserialize)]
struct EspionageAgentCatalogEntry {
    id: String,
    name: String,
    #[serde(default)]
    stealth: Option<f32>,
    #[serde(default)]
    recon: Option<f32>,
    #[serde(default)]
    counter_intel: Option<f32>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    generator: Option<EspionageAgentGeneratorEntry>,
}

#[derive(Deserialize)]
struct EspionageMissionCatalog {
    missions: Vec<EspionageMissionCatalogEntry>,
}

#[derive(Deserialize)]
struct EspionageMissionCatalogEntry {
    id: String,
    name: String,
    #[serde(default)]
    kind: Option<EspionageMissionKind>,
    #[serde(default)]
    resolution_ticks: Option<u16>,
    #[serde(default)]
    base_success: Option<f32>,
    #[serde(default)]
    success_threshold: Option<f32>,
    #[serde(default)]
    stealth_weight: Option<f32>,
    #[serde(default)]
    recon_weight: Option<f32>,
    #[serde(default)]
    counter_intel_weight: Option<f32>,
    #[serde(default)]
    fidelity_gain: Option<f32>,
    #[serde(default)]
    suspicion_on_success: Option<f32>,
    #[serde(default)]
    suspicion_on_failure: Option<f32>,
    #[serde(default)]
    cell_gain_on_success: Option<u8>,
    #[serde(default)]
    countermeasure: Option<EspionageMissionCountermeasureEntry>,
    #[serde(default)]
    suspicion_relief: Option<f32>,
    #[serde(default)]
    fidelity_suppression: Option<f32>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    target_tier_min: Option<u8>,
    #[serde(default)]
    target_tier_max: Option<u8>,
    #[serde(default)]
    generator: Option<EspionageMissionGeneratorEntry>,
}

#[derive(Deserialize, Clone)]
struct EspionageMissionCountermeasureEntry {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    potency: Option<f32>,
    #[serde(default)]
    upkeep: Option<f32>,
    #[serde(default)]
    duration_ticks: Option<u16>,
}

#[derive(Deserialize, Clone)]
struct EspionageMissionGeneratorEntry {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    variant_count: u8,
    #[serde(default)]
    id_pattern: Option<String>,
    #[serde(default)]
    name_pool: Option<Vec<String>>,
    #[serde(default)]
    note_pool: Option<Vec<String>>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    resolution_ticks: Option<GeneratorU16BandEntry>,
    #[serde(default)]
    base_success: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    success_threshold: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    fidelity_gain: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    suspicion_on_success: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    suspicion_on_failure: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    cell_gain_on_success: Option<GeneratorU8BandEntry>,
    #[serde(default)]
    suspicion_relief: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    fidelity_suppression: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    seed_offset: Option<u64>,
}

#[derive(Deserialize, Clone)]
struct EspionageAgentGeneratorEntry {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    per_faction: u8,
    #[serde(default)]
    stealth: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    recon: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    counter_intel: Option<GeneratorStatBandEntry>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    name_pool: Option<Vec<String>>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    seed_offset: Option<u64>,
}

#[derive(Deserialize, Clone, Copy)]
struct GeneratorStatBandEntry {
    #[serde(default)]
    min: Option<f32>,
    #[serde(default)]
    max: Option<f32>,
}

#[derive(Deserialize, Clone, Copy)]
struct GeneratorU16BandEntry {
    #[serde(default)]
    min: Option<u16>,
    #[serde(default)]
    max: Option<u16>,
}

#[derive(Deserialize, Clone, Copy)]
struct GeneratorU8BandEntry {
    #[serde(default)]
    min: Option<u8>,
    #[serde(default)]
    max: Option<u8>,
}

const fn default_true() -> bool {
    true
}
