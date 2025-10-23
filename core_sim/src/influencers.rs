use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::str::FromStr;

use bevy::prelude::*;
use rand::{rngs::SmallRng, Rng, SeedableRng};

use crate::{
    components::PopulationCohort,
    culture::{CultureTraitAxis, CULTURE_TRAIT_AXES},
    generations::{GenerationId, GenerationRegistry},
    resources::SentimentAxisBias,
    scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar},
};
use sim_runtime::{
    influence_domain_mask, CultureTraitAxis as SchemaCultureTraitAxis, InfluenceDomain,
    InfluenceLifecycle, InfluenceScopeKind, InfluencerCultureResonanceEntry,
    InfluentialIndividualState,
};

pub type InfluentialId = u32;

const MAX_INFLUENCERS: usize = 12;
const SUPPORT_DECAY: f32 = 0.85;
const SUPPRESSION_DECAY: f32 = 0.88;
const SPAWN_INTERVAL_MIN: u32 = 8;
const SPAWN_INTERVAL_MAX: u32 = 18;

const POTENTIAL_MIN_TICKS: u16 = 5;
const POTENTIAL_FIZZLE_TICKS: u16 = 12;
const POTENTIAL_FIZZLE_COHERENCE: f32 = 0.35;

const DORMANT_REMOVE_THRESHOLD: u16 = 50;

const BOOST_DECAY: f32 = 0.92;
const SUPPORT_NOTORIETY_GAIN: f32 = 0.08;
const SUPPORT_CHANNEL_GAIN: f32 = 0.35;
const SUPPORT_CHANNEL_MAX: f32 = 1.5;
const NOTORIETY_MIN: f32 = 0.05;
const NOTORIETY_MAX: f32 = 5.0;

const ALL_DOMAINS: [InfluenceDomain; 5] = [
    InfluenceDomain::Sentiment,
    InfluenceDomain::Discovery,
    InfluenceDomain::Logistics,
    InfluenceDomain::Production,
    InfluenceDomain::Humanitarian,
];

const CHANNEL_COUNT: usize = 4;

const CHANNEL_NAMES: [&str; CHANNEL_COUNT] = ["Popular", "Peer", "Institutional", "Humanitarian"];

#[derive(Debug, Clone, Copy)]
pub struct InfluencerCultureResonance {
    pub global: [Scalar; CULTURE_TRAIT_AXES],
    pub regional: [Scalar; CULTURE_TRAIT_AXES],
    pub local: [Scalar; CULTURE_TRAIT_AXES],
}

impl Default for InfluencerCultureResonance {
    fn default() -> Self {
        Self {
            global: [scalar_zero(); CULTURE_TRAIT_AXES],
            regional: [scalar_zero(); CULTURE_TRAIT_AXES],
            local: [scalar_zero(); CULTURE_TRAIT_AXES],
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct InfluencerImpacts {
    pub logistics_multiplier: Scalar,
    pub morale_delta: Scalar,
    pub power_bonus: Scalar,
    pub culture_resonance: InfluencerCultureResonance,
}

impl Default for InfluencerImpacts {
    fn default() -> Self {
        Self {
            logistics_multiplier: scalar_one(),
            morale_delta: scalar_zero(),
            power_bonus: scalar_zero(),
            culture_resonance: InfluencerCultureResonance::default(),
        }
    }
}

impl InfluencerImpacts {
    pub fn set_from_totals(&mut self, logistics: Scalar, morale: Scalar, power: Scalar) {
        let base = scalar_one();
        let multiplier = (base + logistics).clamp(scalar_from_f32(0.6), scalar_from_f32(1.6));
        self.logistics_multiplier = multiplier;
        self.morale_delta = morale.clamp(scalar_from_f32(-0.3), scalar_from_f32(0.4));
        self.power_bonus = power.clamp(scalar_from_f32(-0.25), scalar_from_f32(0.35));
    }

    pub fn set_culture_resonance(&mut self, resonance: InfluencerCultureResonance) {
        self.culture_resonance = resonance;
    }

    pub fn culture_resonance(&self) -> InfluencerCultureResonance {
        self.culture_resonance
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InfluencerStatus {
    Potential,
    Active,
    Dormant,
}

impl From<InfluenceLifecycle> for InfluencerStatus {
    fn from(value: InfluenceLifecycle) -> Self {
        match value {
            InfluenceLifecycle::Potential => InfluencerStatus::Potential,
            InfluenceLifecycle::Active => InfluencerStatus::Active,
            InfluenceLifecycle::Dormant => InfluencerStatus::Dormant,
        }
    }
}

impl From<InfluencerStatus> for InfluenceLifecycle {
    fn from(value: InfluencerStatus) -> Self {
        match value {
            InfluencerStatus::Potential => InfluenceLifecycle::Potential,
            InfluencerStatus::Active => InfluenceLifecycle::Active,
            InfluencerStatus::Dormant => InfluenceLifecycle::Dormant,
        }
    }
}

impl InfluencerStatus {
    fn priority(self) -> u8 {
        match self {
            InfluencerStatus::Active => 0,
            InfluencerStatus::Potential => 1,
            InfluencerStatus::Dormant => 2,
        }
    }
}

#[derive(Debug, Clone)]
struct InfluentialIndividual {
    id: InfluentialId,
    name: String,
    domains_mask: u32,
    scope: InfluenceScopeKind,
    generation_scope: Option<GenerationId>,
    audience_generations: Vec<GenerationId>,
    status: InfluencerStatus,
    coherence: Scalar,
    ticks_in_status: u16,
    influence: Scalar,
    baseline_growth: Scalar,
    growth_rate: Scalar,
    notoriety: Scalar,
    sentiment_weights: [Scalar; 4],
    logistics_weight: Scalar,
    morale_weight: Scalar,
    power_weight: Scalar,
    support_charge: Scalar,
    suppress_pressure: Scalar,
    channel_weights: [Scalar; 4],
    channel_support: [Scalar; 4],
    channel_boosts: [Scalar; 4],
    sentiment_output: [Scalar; 4],
    culture_weights: [Scalar; CULTURE_TRAIT_AXES],
    culture_output: [Scalar; CULTURE_TRAIT_AXES],
    logistics_output: Scalar,
    morale_output: Scalar,
    power_output: Scalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportChannel {
    Popular = 0,
    Peer = 1,
    Institutional = 2,
    Humanitarian = 3,
}

impl SupportChannel {
    pub fn as_str(self) -> &'static str {
        CHANNEL_NAMES[self as usize]
    }

    pub fn parse(value: &str) -> Option<Self> {
        SupportChannel::from_str(value).ok()
    }
}

impl FromStr for SupportChannel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "popular" | "pop" | "mass" => Ok(SupportChannel::Popular),
            "peer" | "prestige" | "research" => Ok(SupportChannel::Peer),
            "institutional" | "institution" | "industrial" | "inst" => {
                Ok(SupportChannel::Institutional)
            }
            "humanitarian" | "hum" | "civic" => Ok(SupportChannel::Humanitarian),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
struct ScopeThreshold {
    promote_coherence: f32,
    promote_notoriety: f32,
    demote_coherence: f32,
    promote_ticks: u16,
    demote_ticks: u16,
}

fn scope_threshold(scope: InfluenceScopeKind) -> ScopeThreshold {
    match scope {
        InfluenceScopeKind::Local => ScopeThreshold {
            promote_coherence: 0.45,
            promote_notoriety: 0.40,
            demote_coherence: 0.25,
            promote_ticks: 3,
            demote_ticks: 6,
        },
        InfluenceScopeKind::Regional => ScopeThreshold {
            promote_coherence: 0.60,
            promote_notoriety: 0.60,
            demote_coherence: 0.30,
            promote_ticks: 4,
            demote_ticks: 7,
        },
        InfluenceScopeKind::Global | InfluenceScopeKind::Generation => ScopeThreshold {
            promote_coherence: 0.72,
            promote_notoriety: 0.75,
            demote_coherence: 0.35,
            promote_ticks: 5,
            demote_ticks: 8,
        },
    }
}

fn next_scope(scope: InfluenceScopeKind) -> Option<InfluenceScopeKind> {
    match scope {
        InfluenceScopeKind::Local => Some(InfluenceScopeKind::Regional),
        InfluenceScopeKind::Regional => Some(InfluenceScopeKind::Global),
        InfluenceScopeKind::Global | InfluenceScopeKind::Generation => None,
    }
}

impl InfluentialIndividual {
    fn from_state(state: &InfluentialIndividualState) -> Self {
        let mut culture_weights = [scalar_zero(); CULTURE_TRAIT_AXES];
        let mut culture_output = [scalar_zero(); CULTURE_TRAIT_AXES];
        for entry in &state.culture_resonance {
            let axis = schema_axis_to_local(entry.axis);
            let idx = axis.index();
            culture_weights[idx] = Scalar::from_raw(entry.weight);
            culture_output[idx] = Scalar::from_raw(entry.output);
        }

        Self {
            id: state.id,
            name: state.name.clone(),
            domains_mask: state.domains,
            scope: state.scope,
            generation_scope: if state.generation_scope
                == InfluentialIndividualState::NO_GENERATION_SCOPE
            {
                None
            } else {
                Some(state.generation_scope)
            },
            audience_generations: state.audience_generations.clone(),
            status: InfluencerStatus::from(state.lifecycle),
            coherence: Scalar::from_raw(state.coherence),
            ticks_in_status: state.ticks_in_status,
            influence: Scalar::from_raw(state.influence),
            baseline_growth: Scalar::from_raw(state.baseline_growth),
            growth_rate: Scalar::from_raw(state.growth_rate),
            notoriety: Scalar::from_raw(state.notoriety),
            sentiment_weights: [
                Scalar::from_raw(state.sentiment_weight_knowledge),
                Scalar::from_raw(state.sentiment_weight_trust),
                Scalar::from_raw(state.sentiment_weight_equity),
                Scalar::from_raw(state.sentiment_weight_agency),
            ],
            logistics_weight: Scalar::from_raw(state.logistics_weight),
            morale_weight: Scalar::from_raw(state.morale_weight),
            power_weight: Scalar::from_raw(state.power_weight),
            support_charge: Scalar::from_raw(state.support_charge),
            suppress_pressure: Scalar::from_raw(state.suppress_pressure),
            sentiment_output: [
                Scalar::from_raw(state.sentiment_knowledge),
                Scalar::from_raw(state.sentiment_trust),
                Scalar::from_raw(state.sentiment_equity),
                Scalar::from_raw(state.sentiment_agency),
            ],
            logistics_output: Scalar::from_raw(state.logistics_bonus),
            morale_output: Scalar::from_raw(state.morale_bonus),
            power_output: Scalar::from_raw(state.power_bonus),
            channel_weights: [
                Scalar::from_raw(state.weight_popular),
                Scalar::from_raw(state.weight_peer),
                Scalar::from_raw(state.weight_institutional),
                Scalar::from_raw(state.weight_humanitarian),
            ],
            channel_support: [
                Scalar::from_raw(state.support_popular),
                Scalar::from_raw(state.support_peer),
                Scalar::from_raw(state.support_institutional),
                Scalar::from_raw(state.support_humanitarian),
            ],
            channel_boosts: [scalar_zero(); CHANNEL_COUNT],
            culture_weights,
            culture_output,
        }
    }

    fn to_state(&self) -> InfluentialIndividualState {
        let culture_resonance = CultureTraitAxis::ALL
            .iter()
            .filter_map(|axis| {
                let axis_value = *axis;
                let idx = axis_value.index();
                let weight = self.culture_weights[idx].raw();
                let output = self.culture_output[idx].raw();
                if weight == 0 && output == 0 {
                    None
                } else {
                    Some(InfluencerCultureResonanceEntry {
                        axis: local_axis_to_schema(axis_value),
                        weight,
                        output,
                    })
                }
            })
            .collect();

        InfluentialIndividualState {
            id: self.id,
            name: self.name.clone(),
            influence: self.influence.raw(),
            growth_rate: self.growth_rate.raw(),
            baseline_growth: self.baseline_growth.raw(),
            notoriety: self.notoriety.raw(),
            sentiment_knowledge: self.sentiment_output[0].raw(),
            sentiment_trust: self.sentiment_output[1].raw(),
            sentiment_equity: self.sentiment_output[2].raw(),
            sentiment_agency: self.sentiment_output[3].raw(),
            sentiment_weight_knowledge: self.sentiment_weights[0].raw(),
            sentiment_weight_trust: self.sentiment_weights[1].raw(),
            sentiment_weight_equity: self.sentiment_weights[2].raw(),
            sentiment_weight_agency: self.sentiment_weights[3].raw(),
            logistics_bonus: self.logistics_output.raw(),
            morale_bonus: self.morale_output.raw(),
            power_bonus: self.power_output.raw(),
            logistics_weight: self.logistics_weight.raw(),
            morale_weight: self.morale_weight.raw(),
            power_weight: self.power_weight.raw(),
            support_charge: self.support_charge.raw(),
            suppress_pressure: self.suppress_pressure.raw(),
            domains: self.domains_mask,
            scope: self.scope,
            generation_scope: self
                .generation_scope
                .unwrap_or(InfluentialIndividualState::NO_GENERATION_SCOPE),
            supported: self.support_charge > scalar_from_f32(0.05),
            suppressed: self.suppress_pressure > scalar_from_f32(0.05),
            lifecycle: InfluenceLifecycle::from(self.status),
            coherence: self.coherence.raw(),
            ticks_in_status: self.ticks_in_status,
            audience_generations: self.audience_generations.clone(),
            support_popular: self.channel_support[SupportChannel::Popular as usize].raw(),
            support_peer: self.channel_support[SupportChannel::Peer as usize].raw(),
            support_institutional: self.channel_support[SupportChannel::Institutional as usize]
                .raw(),
            support_humanitarian: self.channel_support[SupportChannel::Humanitarian as usize].raw(),
            weight_popular: self.channel_weights[SupportChannel::Popular as usize].raw(),
            weight_peer: self.channel_weights[SupportChannel::Peer as usize].raw(),
            weight_institutional: self.channel_weights[SupportChannel::Institutional as usize]
                .raw(),
            weight_humanitarian: self.channel_weights[SupportChannel::Humanitarian as usize].raw(),
            culture_resonance,
        }
    }

    fn coherence_factor(&self) -> Scalar {
        match self.status {
            InfluencerStatus::Active => scalar_one(),
            InfluencerStatus::Potential => {
                self.coherence.clamp(scalar_zero(), scalar_from_f32(0.6))
            }
            InfluencerStatus::Dormant => scalar_zero(),
        }
    }
}

#[derive(Resource)]
pub struct InfluentialRoster {
    rng: SmallRng,
    individuals: Vec<InfluentialIndividual>,
    next_id: InfluentialId,
    spawn_cooldown: u32,
    last_sentiment: [Scalar; 4],
    last_logistics: Scalar,
    last_morale: Scalar,
    last_power: Scalar,
    last_culture: InfluencerCultureResonance,
}

impl InfluentialRoster {
    pub fn with_seed(seed: u64, registry: &GenerationRegistry) -> Self {
        let mut roster = Self {
            rng: SmallRng::seed_from_u64(seed),
            individuals: Vec::new(),
            next_id: 1,
            spawn_cooldown: SPAWN_INTERVAL_MIN,
            last_sentiment: [scalar_zero(); 4],
            last_logistics: scalar_zero(),
            last_morale: scalar_zero(),
            last_power: scalar_zero(),
            last_culture: InfluencerCultureResonance::default(),
        };
        roster.bootstrap(registry);
        roster
    }

    fn bootstrap(&mut self, registry: &GenerationRegistry) {
        let mut seeded = 0usize;
        for profile in registry.profiles().iter().take(3) {
            let _ = self.spawn_influencer(
                None,
                Some(profile.id),
                Some(profile.name.as_str()),
                registry,
            );
            seeded += 1;
        }
        while seeded < 3 {
            let _ = self.spawn_influencer(None, None, None, registry);
            seeded += 1;
        }
        self.recompute_outputs();
    }

    pub fn tick(
        &mut self,
        registry: &GenerationRegistry,
        manual_axes: [Scalar; 4],
        generation_shares: &HashMap<GenerationId, f32>,
    ) {
        self.advance_influence();
        self.update_lifecycle(manual_axes, generation_shares, registry);

        if self.spawn_cooldown > 0 {
            self.spawn_cooldown -= 1;
        }
        if self.spawn_cooldown == 0 && self.individuals.len() < MAX_INFLUENCERS {
            let _ = self.spawn_influencer(None, None, None, registry);
        }

        self.recompute_outputs();
        self.backfill_if_needed(registry);
    }

    fn advance_influence(&mut self) {
        for individual in &mut self.individuals {
            for boost in &mut individual.channel_boosts.iter_mut() {
                *boost = damp_scalar(*boost, BOOST_DECAY);
            }
            let momentum = individual.momentum() * scalar_from_f32(0.05);
            let baseline_pull =
                (individual.baseline_growth - individual.growth_rate) * scalar_from_f32(0.10);
            individual.growth_rate = (individual.growth_rate + momentum + baseline_pull)
                .clamp(scalar_from_f32(-0.20), scalar_from_f32(0.32));

            individual.influence = (individual.influence + individual.growth_rate)
                .clamp(scalar_from_f32(-2.8), scalar_from_f32(3.5));

            individual.support_charge = damp_scalar(individual.support_charge, SUPPORT_DECAY);
            individual.suppress_pressure =
                damp_scalar(individual.suppress_pressure, SUPPRESSION_DECAY);
        }
    }

    fn update_lifecycle(
        &mut self,
        manual_axes: [Scalar; 4],
        generation_shares: &HashMap<GenerationId, f32>,
        registry: &GenerationRegistry,
    ) {
        let manual: [f32; 4] = manual_axes.map(Scalar::to_f32);
        let knowledge_env = ((manual[0] + 1.0) * 0.5).clamp(0.0, 1.0);
        let trust_env = ((manual[1] + 1.0) * 0.5).clamp(0.0, 1.0);
        let equity_env = ((manual[2] + 1.0) * 0.5).clamp(0.0, 1.0);
        let agency_env = ((manual[3] + 1.0) * 0.5).clamp(0.0, 1.0);

        let mut removals: HashSet<InfluentialId> = HashSet::new();

        for individual in &mut self.individuals {
            if individual.audience_generations.is_empty() {
                if let Some(gen) = individual.generation_scope {
                    individual.audience_generations.push(gen);
                } else if let Some(first) = registry.profiles().first() {
                    individual.audience_generations.push(first.id);
                }
            }

            let alignment = {
                let mut total = 0.0f32;
                for (axis, desired_raw) in manual.iter().enumerate() {
                    let desired = desired_raw.clamp(-1.0, 1.0);
                    let projected = (individual.sentiment_weights[axis] * individual.influence)
                        .to_f32()
                        .clamp(-1.0, 1.0);
                    let diff = (desired - projected).abs().min(2.0);
                    total += 1.0 - (diff * 0.5);
                }
                (total / 4.0).clamp(0.0, 1.0)
            };

            let audience_factor =
                if individual.audience_generations.is_empty() || generation_shares.is_empty() {
                    0.3
                } else {
                    let sum: f32 = individual
                        .audience_generations
                        .iter()
                        .map(|gen| generation_shares.get(gen).copied().unwrap_or(0.0))
                        .sum();
                    (sum / individual.audience_generations.len() as f32).clamp(0.0, 1.0)
                };

            let support_bonus = (individual.support_charge.to_f32() * 0.08).clamp(0.0, 0.3);
            let suppress_penalty = (individual.suppress_pressure.to_f32() * 0.08).clamp(0.0, 0.3);

            let mut channel_scores = [0.0f32; CHANNEL_COUNT];
            channel_scores[SupportChannel::Popular as usize] =
                (alignment + support_bonus - suppress_penalty).clamp(0.0, 1.0);
            channel_scores[SupportChannel::Peer as usize] = ((knowledge_env * 0.7)
                + alignment * 0.2
                + audience_factor * 0.1
                + individual.channel_boosts[SupportChannel::Peer as usize].to_f32())
            .clamp(0.0, 1.0);
            channel_scores[SupportChannel::Institutional as usize] = ((agency_env * 0.5)
                + equity_env * 0.3
                + audience_factor * 0.2
                + individual.channel_boosts[SupportChannel::Institutional as usize].to_f32())
            .clamp(0.0, 1.0);
            channel_scores[SupportChannel::Humanitarian as usize] = ((trust_env * 0.5)
                + audience_factor * 0.3
                + alignment * 0.2
                + individual.channel_boosts[SupportChannel::Humanitarian as usize].to_f32())
            .clamp(0.0, 1.0);

            for (idx, value) in channel_scores.iter().enumerate() {
                individual.channel_support[idx] = Scalar::from_f32(*value);
            }

            let mut coherence_scalar = scalar_zero();
            for idx in 0..CHANNEL_COUNT {
                coherence_scalar +=
                    individual.channel_weights[idx] * individual.channel_support[idx];
            }
            coherence_scalar = coherence_scalar.clamp(scalar_zero(), scalar_one());
            individual.coherence = coherence_scalar;
            let coherence = coherence_scalar.to_f32().clamp(0.0, 1.0);

            individual.ticks_in_status = individual.ticks_in_status.saturating_add(1);

            let weighted_popular = channel_scores[SupportChannel::Popular as usize]
                * individual.channel_weights[SupportChannel::Popular as usize].to_f32();
            let weighted_peer = channel_scores[SupportChannel::Peer as usize]
                * individual.channel_weights[SupportChannel::Peer as usize].to_f32();
            let weighted_institutional = channel_scores[SupportChannel::Institutional as usize]
                * individual.channel_weights[SupportChannel::Institutional as usize].to_f32();
            let weighted_humanitarian = channel_scores[SupportChannel::Humanitarian as usize]
                * individual.channel_weights[SupportChannel::Humanitarian as usize].to_f32();

            let notoriety_delta = weighted_popular * 0.04
                + weighted_peer * 0.03
                + weighted_institutional * 0.02
                + weighted_humanitarian * 0.025
                + support_bonus * 0.15;

            if individual.status != InfluencerStatus::Dormant {
                let new_value = individual.notoriety.to_f32() + notoriety_delta;
                individual.notoriety = clamp_notoriety_value(new_value);
            } else {
                let new_value = (individual.notoriety.to_f32() - 0.02).max(NOTORIETY_MIN);
                individual.notoriety = clamp_notoriety_value(new_value);
            }

            let thresholds = scope_threshold(individual.scope);

            match individual.status {
                InfluencerStatus::Potential => {
                    if individual.ticks_in_status >= POTENTIAL_MIN_TICKS
                        && coherence >= thresholds.promote_coherence
                        && individual.notoriety.to_f32() >= thresholds.promote_notoriety
                    {
                        individual.status = InfluencerStatus::Active;
                        individual.ticks_in_status = 0;
                    } else if individual.ticks_in_status >= POTENTIAL_FIZZLE_TICKS
                        && coherence < POTENTIAL_FIZZLE_COHERENCE
                    {
                        individual.status = InfluencerStatus::Dormant;
                        individual.ticks_in_status = 0;
                    }
                }
                InfluencerStatus::Active => {
                    if coherence < thresholds.demote_coherence
                        && individual.ticks_in_status >= thresholds.demote_ticks
                    {
                        individual.status = InfluencerStatus::Dormant;
                        individual.ticks_in_status = 0;
                    } else if let Some(next_scope) = next_scope(individual.scope) {
                        let next_threshold = scope_threshold(next_scope);
                        if individual.ticks_in_status >= thresholds.promote_ticks
                            && coherence >= next_threshold.promote_coherence
                            && individual.notoriety.to_f32() >= next_threshold.promote_notoriety
                        {
                            individual.scope = next_scope;
                            individual.status = InfluencerStatus::Potential;
                            individual.ticks_in_status = 0;
                        }
                    }
                }
                InfluencerStatus::Dormant => {
                    if coherence >= thresholds.promote_coherence
                        && individual.notoriety.to_f32() >= thresholds.promote_notoriety * 0.9
                    {
                        individual.status = InfluencerStatus::Potential;
                        individual.ticks_in_status = 0;
                    } else if individual.ticks_in_status >= DORMANT_REMOVE_THRESHOLD
                        && coherence < POTENTIAL_FIZZLE_COHERENCE
                        && individual.notoriety.to_f32() <= NOTORIETY_MIN + 0.05
                    {
                        removals.insert(individual.id);
                    }
                }
            }
        }

        if !removals.is_empty() {
            self.individuals
                .retain(|individual| !removals.contains(&individual.id));
        }
    }

    fn backfill_if_needed(&mut self, registry: &GenerationRegistry) {
        while self.individuals.len() < 3 {
            let gen = registry.assign_for_index(self.individuals.len());
            let _ = self.spawn_influencer(None, Some(gen), None, registry);
        }
    }

    fn spawn_influencer(
        &mut self,
        scope_override: Option<InfluenceScopeKind>,
        generation: Option<GenerationId>,
        label_hint: Option<&str>,
        registry: &GenerationRegistry,
    ) -> Option<InfluentialId> {
        if self.individuals.len() >= MAX_INFLUENCERS {
            return None;
        }

        let id = self.next_id;
        self.next_id += 1;

        let scope = scope_override.unwrap_or_else(|| match self.rng.gen_range(0..100) {
            0..=35 => InfluenceScopeKind::Local,
            36..=70 => InfluenceScopeKind::Regional,
            71..=90 => InfluenceScopeKind::Global,
            _ => InfluenceScopeKind::Generation,
        });

        let generation_scope = match scope {
            InfluenceScopeKind::Generation => generation.or_else(|| {
                if self.individuals.is_empty() {
                    Some(0)
                } else {
                    Some(
                        self.individuals[self.rng.gen_range(0..self.individuals.len())]
                            .generation_scope
                            .unwrap_or(0),
                    )
                }
            }),
            _ => None,
        };

        let domains = select_domains(&mut self.rng);
        let domains_mask = influence_domain_mask(&domains);

        let sentiment_weights = generate_sentiment_weights(&mut self.rng, &domains);
        let logistics_weight = domain_weight(
            &mut self.rng,
            &domains,
            InfluenceDomain::Logistics,
            0.03,
            0.06,
        );
        let morale_weight = domain_weight(
            &mut self.rng,
            &domains,
            InfluenceDomain::Humanitarian,
            0.02,
            0.05,
        );
        let power_weight = domain_weight(
            &mut self.rng,
            &domains,
            InfluenceDomain::Production,
            0.03,
            0.07,
        ) + domain_weight(
            &mut self.rng,
            &domains,
            InfluenceDomain::Discovery,
            0.015,
            0.04,
        );

        let baseline_growth = scalar_from_f32(self.rng.gen_range(0.01..0.04));
        let influence = scalar_from_f32(self.rng.gen_range(0.2..0.6));
        let notoriety = scalar_from_f32(self.rng.gen_range(0.1..0.35));

        let name = label_hint
            .map(|s| s.to_string())
            .unwrap_or_else(|| generate_name(&mut self.rng, scope));

        let mut audience_generations = Vec::new();
        if let Some(gen) = generation_scope {
            audience_generations.push(gen);
        }
        if audience_generations.is_empty() {
            let profiles = registry.profiles();
            if !profiles.is_empty() {
                let sample_count = profiles.len().min(2);
                let mut selected = HashSet::new();
                while selected.len() < sample_count {
                    let idx = self.rng.gen_range(0..profiles.len());
                    selected.insert(profiles[idx].id);
                }
                audience_generations.extend(selected);
            }
        }

        let channel_weights = resolve_channel_weights(domains_mask);
        let channel_support = channel_weights
            .map(|weight| (weight * scalar_from_f32(0.3)).clamp(scalar_zero(), scalar_one()));
        let culture_weights = generate_culture_resonance_weights(&mut self.rng, &domains);

        let individual = InfluentialIndividual {
            id,
            name,
            domains_mask,
            scope,
            generation_scope,
            audience_generations,
            status: InfluencerStatus::Potential,
            coherence: scalar_from_f32(0.18),
            ticks_in_status: 0,
            influence,
            baseline_growth,
            growth_rate: baseline_growth,
            notoriety,
            sentiment_weights,
            logistics_weight,
            morale_weight,
            power_weight,
            support_charge: scalar_zero(),
            suppress_pressure: scalar_zero(),
            channel_weights,
            channel_support,
            channel_boosts: [scalar_zero(); CHANNEL_COUNT],
            sentiment_output: [scalar_zero(); 4],
            culture_weights,
            culture_output: [scalar_zero(); CULTURE_TRAIT_AXES],
            logistics_output: scalar_zero(),
            morale_output: scalar_zero(),
            power_output: scalar_zero(),
        };

        self.individuals.push(individual);
        self.spawn_cooldown = self.rng.gen_range(SPAWN_INTERVAL_MIN..=SPAWN_INTERVAL_MAX);
        self.recompute_outputs();
        Some(id)
    }

    pub fn force_spawn(
        &mut self,
        scope: Option<InfluenceScopeKind>,
        generation: Option<GenerationId>,
        registry: &GenerationRegistry,
    ) -> Option<InfluentialId> {
        self.spawn_influencer(scope, generation, None, registry)
    }

    pub fn apply_support(&mut self, id: InfluentialId, magnitude: Scalar) -> bool {
        if let Some(individual) = self.individuals.iter_mut().find(|item| item.id == id) {
            individual.support_charge =
                (individual.support_charge + magnitude).clamp(scalar_zero(), scalar_from_f32(5.0));
            individual.suppress_pressure = (individual.suppress_pressure
                - magnitude * scalar_from_f32(0.25))
            .clamp(scalar_zero(), scalar_from_f32(5.0));
            let notoriety_gain = magnitude.to_f32()
                * SUPPORT_NOTORIETY_GAIN
                * individual.channel_weights[SupportChannel::Popular as usize]
                    .to_f32()
                    .max(0.25);
            individual.notoriety =
                clamp_notoriety_value(individual.notoriety.to_f32() + notoriety_gain);
            individual.ticks_in_status = 0;
            true
        } else {
            false
        }
    }

    pub fn apply_suppress(&mut self, id: InfluentialId, magnitude: Scalar) -> bool {
        if let Some(individual) = self.individuals.iter_mut().find(|item| item.id == id) {
            individual.suppress_pressure = (individual.suppress_pressure + magnitude)
                .clamp(scalar_zero(), scalar_from_f32(5.0));
            individual.support_charge = (individual.support_charge
                - magnitude * scalar_from_f32(0.25))
            .clamp(scalar_zero(), scalar_from_f32(5.0));
            let notoriety_loss = magnitude.to_f32() * (SUPPORT_NOTORIETY_GAIN * 0.6);
            individual.notoriety = clamp_notoriety_value(
                (individual.notoriety.to_f32() - notoriety_loss).max(NOTORIETY_MIN),
            );
            individual.ticks_in_status = 0;
            true
        } else {
            false
        }
    }

    pub fn apply_channel_support(
        &mut self,
        id: InfluentialId,
        channel: SupportChannel,
        magnitude: Scalar,
    ) -> bool {
        if let Some(individual) = self.individuals.iter_mut().find(|item| item.id == id) {
            let idx = channel as usize;
            let boost = (individual.channel_boosts[idx]
                + magnitude * scalar_from_f32(SUPPORT_CHANNEL_GAIN))
            .clamp(scalar_zero(), scalar_from_f32(SUPPORT_CHANNEL_MAX));
            individual.channel_boosts[idx] = boost;

            let notoriety_gain = magnitude.to_f32()
                * SUPPORT_NOTORIETY_GAIN
                * individual.channel_weights[idx].to_f32().max(0.2);
            individual.notoriety =
                clamp_notoriety_value(individual.notoriety.to_f32() + notoriety_gain);
            individual.ticks_in_status = 0;
            true
        } else {
            false
        }
    }

    pub fn states(&self) -> Vec<InfluentialIndividualState> {
        let mut states: Vec<_> = self
            .individuals
            .iter()
            .map(|item| item.to_state())
            .collect();
        states.sort_unstable_by(|a, b| {
            let pa = InfluencerStatus::from(a.lifecycle).priority();
            let pb = InfluencerStatus::from(b.lifecycle).priority();
            pa.cmp(&pb)
                .then_with(|| b.coherence.cmp(&a.coherence))
                .then_with(|| b.influence.cmp(&a.influence))
                .then_with(|| a.id.cmp(&b.id))
        });
        states
    }

    pub fn sentiment_totals(&self) -> [Scalar; 4] {
        self.last_sentiment
    }

    pub fn logistics_total(&self) -> Scalar {
        self.last_logistics
    }

    pub fn morale_total(&self) -> Scalar {
        self.last_morale
    }

    pub fn power_total(&self) -> Scalar {
        self.last_power
    }

    pub fn culture_resonance(&self) -> InfluencerCultureResonance {
        self.last_culture
    }

    pub fn update_from_states(&mut self, states: &[InfluentialIndividualState]) {
        self.individuals = states
            .iter()
            .map(InfluentialIndividual::from_state)
            .collect();
        self.next_id = self
            .individuals
            .iter()
            .map(|item| item.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.spawn_cooldown = SPAWN_INTERVAL_MIN;
        self.recompute_outputs();
    }

    fn recompute_outputs(&mut self) {
        let mut sentiment = [scalar_zero(); 4];
        let mut logistics = scalar_zero();
        let mut morale = scalar_zero();
        let mut power = scalar_zero();
        let mut culture_global = [scalar_zero(); CULTURE_TRAIT_AXES];
        let mut culture_regional = [scalar_zero(); CULTURE_TRAIT_AXES];
        let mut culture_local = [scalar_zero(); CULTURE_TRAIT_AXES];

        for individual in &mut self.individuals {
            let factor = individual.coherence_factor();
            for (axis, output) in individual.sentiment_output.iter_mut().enumerate() {
                let base = individual.sentiment_weights[axis] * individual.influence;
                *output = (base * factor).clamp(scalar_from_f32(-0.75), scalar_from_f32(0.75));
                sentiment[axis] += *output;
            }
            for (axis, output_slot) in individual.culture_output.iter_mut().enumerate() {
                let base = individual.culture_weights[axis] * individual.influence;
                let output = (base * factor).clamp(scalar_from_f32(-0.6), scalar_from_f32(0.6));
                *output_slot = output;
                match individual.scope {
                    InfluenceScopeKind::Local => culture_local[axis] += output,
                    InfluenceScopeKind::Regional => culture_regional[axis] += output,
                    InfluenceScopeKind::Global | InfluenceScopeKind::Generation => {
                        culture_global[axis] += output
                    }
                }
            }
            individual.logistics_output =
                (individual.logistics_weight * individual.influence * factor)
                    .clamp(scalar_from_f32(-0.4), scalar_from_f32(0.6));
            individual.morale_output = (individual.morale_weight * individual.influence * factor)
                .clamp(scalar_from_f32(-0.3), scalar_from_f32(0.4));
            individual.power_output = (individual.power_weight * individual.influence * factor)
                .clamp(scalar_from_f32(-0.3), scalar_from_f32(0.45));

            logistics += individual.logistics_output;
            morale += individual.morale_output;
            power += individual.power_output;
        }

        self.last_sentiment =
            sentiment.map(|value| value.clamp(scalar_from_f32(-1.2), scalar_from_f32(1.2)));
        self.last_logistics = logistics.clamp(scalar_from_f32(-0.5), scalar_from_f32(0.8));
        self.last_morale = morale.clamp(scalar_from_f32(-0.3), scalar_from_f32(0.5));
        self.last_power = power.clamp(scalar_from_f32(-0.3), scalar_from_f32(0.5));
        let min_culture = scalar_from_f32(-1.0);
        let max_culture = scalar_from_f32(1.0);
        self.last_culture = InfluencerCultureResonance {
            global: clamp_culture_array(culture_global, min_culture, max_culture),
            regional: clamp_culture_array(culture_regional, min_culture, max_culture),
            local: clamp_culture_array(culture_local, min_culture, max_culture),
        };
    }
}

impl InfluentialIndividual {
    fn momentum(&self) -> Scalar {
        self.support_charge - self.suppress_pressure
    }
}

fn damp_scalar(value: Scalar, factor: f32) -> Scalar {
    let decay = scalar_from_f32(factor);
    let result = value * decay;
    if result.abs() < scalar_from_f32(0.01) {
        scalar_zero()
    } else {
        result
    }
}

fn clamp_culture_array(
    mut values: [Scalar; CULTURE_TRAIT_AXES],
    min: Scalar,
    max: Scalar,
) -> [Scalar; CULTURE_TRAIT_AXES] {
    for value in values.iter_mut() {
        *value = (*value).clamp(min, max);
    }
    values
}

fn select_domains(rng: &mut SmallRng) -> Vec<InfluenceDomain> {
    let domain_pool = [
        InfluenceDomain::Sentiment,
        InfluenceDomain::Discovery,
        InfluenceDomain::Logistics,
        InfluenceDomain::Production,
        InfluenceDomain::Humanitarian,
    ];
    let domain_count = if rng.gen_bool(0.2) { 3 } else { 2 };
    let mut domains = Vec::new();
    while domains.len() < domain_count {
        let candidate = domain_pool[rng.gen_range(0..domain_pool.len())];
        if !domains.contains(&candidate) {
            domains.push(candidate);
        }
    }
    domains
}

fn generate_sentiment_weights(rng: &mut SmallRng, domains: &[InfluenceDomain]) -> [Scalar; 4] {
    let mut weights = [scalar_zero(); 4];
    for weight in &mut weights {
        let base = rng.gen_range(-0.2..0.2);
        *weight = scalar_from_f32(base);
    }
    for domain in domains {
        match domain {
            InfluenceDomain::Sentiment => {
                weights[0] += scalar_from_f32(rng.gen_range(0.1..0.25));
                weights[1] += scalar_from_f32(rng.gen_range(0.05..0.2));
            }
            InfluenceDomain::Humanitarian => {
                weights[1] += scalar_from_f32(rng.gen_range(0.05..0.18));
                weights[2] += scalar_from_f32(rng.gen_range(0.05..0.18));
            }
            InfluenceDomain::Logistics => {
                weights[2] += scalar_from_f32(rng.gen_range(0.02..0.1));
                weights[3] += scalar_from_f32(rng.gen_range(0.04..0.12));
            }
            InfluenceDomain::Discovery => {
                weights[0] += scalar_from_f32(rng.gen_range(0.08..0.2));
                weights[3] += scalar_from_f32(rng.gen_range(0.04..0.12));
            }
            InfluenceDomain::Production => {
                weights[2] += scalar_from_f32(rng.gen_range(0.06..0.18));
                weights[3] += scalar_from_f32(rng.gen_range(0.02..0.1));
            }
        };
    }
    weights
}

fn push_culture_weight(
    rng: &mut SmallRng,
    weights: &mut [Scalar; CULTURE_TRAIT_AXES],
    axis: CultureTraitAxis,
    min: f32,
    max: f32,
) {
    let magnitude = rng.gen_range(min..max);
    let direction = if rng.gen_bool(0.55) { 1.0 } else { -1.0 };
    let idx = axis.index();
    weights[idx] += scalar_from_f32(direction * magnitude);
}

fn generate_culture_resonance_weights(
    rng: &mut SmallRng,
    domains: &[InfluenceDomain],
) -> [Scalar; CULTURE_TRAIT_AXES] {
    let mut weights = [scalar_zero(); CULTURE_TRAIT_AXES];
    for domain in domains {
        match domain {
            InfluenceDomain::Sentiment => {
                push_culture_weight(rng, &mut weights, CultureTraitAxis::OpenClosed, 0.12, 0.28);
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::CollectivistIndividualist,
                    0.08,
                    0.22,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::PluralisticMonocultural,
                    0.10,
                    0.24,
                );
            }
            InfluenceDomain::Discovery => {
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::RationalistMystical,
                    0.10,
                    0.26,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::PragmaticIdealistic,
                    0.08,
                    0.20,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::AdaptiveStubborn,
                    0.08,
                    0.20,
                );
            }
            InfluenceDomain::Logistics => {
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::HierarchicalEgalitarian,
                    0.10,
                    0.22,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::PragmaticIdealistic,
                    0.06,
                    0.18,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::TraditionalistRevisionist,
                    0.05,
                    0.16,
                );
            }
            InfluenceDomain::Production => {
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::TraditionalistRevisionist,
                    0.08,
                    0.20,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::MeritOrientedLineageOriented,
                    0.10,
                    0.24,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::AsceticIndulgent,
                    0.06,
                    0.18,
                );
            }
            InfluenceDomain::Humanitarian => {
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::SecularDevout,
                    0.10,
                    0.24,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::SyncreticPurist,
                    0.08,
                    0.22,
                );
                push_culture_weight(
                    rng,
                    &mut weights,
                    CultureTraitAxis::HonorBoundOpportunistic,
                    0.06,
                    0.18,
                );
            }
        }
    }

    for _ in 0..2 {
        let idx = rng.gen_range(0..CULTURE_TRAIT_AXES);
        let jitter = rng.gen_range(-0.06..0.06);
        weights[idx] += scalar_from_f32(jitter);
    }

    let min = scalar_from_f32(-0.6);
    let max = scalar_from_f32(0.6);
    for entry in weights.iter_mut() {
        *entry = (*entry).clamp(min, max);
    }

    weights
}

fn domain_weight(
    rng: &mut SmallRng,
    domains: &[InfluenceDomain],
    target: InfluenceDomain,
    min: f32,
    max: f32,
) -> Scalar {
    if domains.contains(&target) {
        scalar_from_f32(rng.gen_range(min..max))
    } else {
        scalar_zero()
    }
}

fn generate_name(rng: &mut SmallRng, scope: InfluenceScopeKind) -> String {
    const HONORIFICS: &[&str] = &[
        "Archivist",
        "Marshal",
        "Oracle",
        "Architect",
        "Luminary",
        "Curator",
        "Navigator",
        "Synthesist",
        "Herald",
        "Mediator",
    ];
    const SURNAMES: &[&str] = &[
        "Vey", "Tal", "Risan", "Kade", "Zorin", "Nare", "Soltis", "Dae", "Meret", "Quill",
    ];

    let honorific = HONORIFICS[rng.gen_range(0..HONORIFICS.len())];
    let surname = SURNAMES[rng.gen_range(0..SURNAMES.len())];
    match scope {
        InfluenceScopeKind::Local => format!("{honorific} {} of Eastreach", surname),
        InfluenceScopeKind::Regional => format!("{honorific} {} of the Meridian", surname),
        InfluenceScopeKind::Global => format!("{honorific} {} Prime", surname),
        InfluenceScopeKind::Generation => format!("{honorific} {} of the Epoch", surname),
    }
}

fn clamp_notoriety_value(value: f32) -> Scalar {
    scalar_from_f32(value.clamp(NOTORIETY_MIN, NOTORIETY_MAX))
}

fn domain_channel_weights(domain: InfluenceDomain) -> [f32; CHANNEL_COUNT] {
    match domain {
        InfluenceDomain::Sentiment => [0.65, 0.1, 0.1, 0.15],
        InfluenceDomain::Discovery => [0.15, 0.6, 0.15, 0.1],
        InfluenceDomain::Logistics => [0.2, 0.1, 0.55, 0.15],
        InfluenceDomain::Production => [0.2, 0.1, 0.5, 0.2],
        InfluenceDomain::Humanitarian => [0.35, 0.1, 0.1, 0.45],
    }
}

fn resolve_channel_weights(domains_mask: u32) -> [Scalar; CHANNEL_COUNT] {
    let mut weights = [0.0f32; CHANNEL_COUNT];
    let mut domain_count = 0.0f32;
    for domain in ALL_DOMAINS {
        if domains_mask & domain.bit() != 0 {
            let contribution = domain_channel_weights(domain);
            for (idx, value) in contribution.iter().enumerate() {
                weights[idx] += *value;
            }
            domain_count += 1.0;
        }
    }
    if domain_count <= f32::EPSILON {
        weights = [0.6, 0.15, 0.15, 0.1];
    }
    let total: f32 = weights.iter().sum();
    if total <= f32::EPSILON {
        [scalar_from_f32(0.25); CHANNEL_COUNT]
    } else {
        weights
            .iter()
            .map(|value| scalar_from_f32(value / total))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or([scalar_from_f32(0.25); CHANNEL_COUNT])
    }
}

fn schema_axis_to_local(axis: SchemaCultureTraitAxis) -> CultureTraitAxis {
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

fn local_axis_to_schema(axis: CultureTraitAxis) -> SchemaCultureTraitAxis {
    match axis {
        CultureTraitAxis::PassiveAggressive => SchemaCultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed => SchemaCultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist => {
            SchemaCultureTraitAxis::CollectivistIndividualist
        }
        CultureTraitAxis::TraditionalistRevisionist => {
            SchemaCultureTraitAxis::TraditionalistRevisionist
        }
        CultureTraitAxis::HierarchicalEgalitarian => {
            SchemaCultureTraitAxis::HierarchicalEgalitarian
        }
        CultureTraitAxis::SyncreticPurist => SchemaCultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent => SchemaCultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic => SchemaCultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical => SchemaCultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular => SchemaCultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn => SchemaCultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic => {
            SchemaCultureTraitAxis::HonorBoundOpportunistic
        }
        CultureTraitAxis::MeritOrientedLineageOriented => {
            SchemaCultureTraitAxis::MeritOrientedLineageOriented
        }
        CultureTraitAxis::SecularDevout => SchemaCultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural => {
            SchemaCultureTraitAxis::PluralisticMonocultural
        }
    }
}

pub fn tick_influencers(
    mut roster: ResMut<InfluentialRoster>,
    registry: Res<GenerationRegistry>,
    cohorts: Query<&PopulationCohort>,
    mut impacts: ResMut<InfluencerImpacts>,
    mut axis_bias: ResMut<SentimentAxisBias>,
) {
    let manual_axes = axis_bias.manual_environment();

    let mut generation_totals: HashMap<GenerationId, u64> = HashMap::new();
    let mut total_population: u64 = 0;
    for cohort in cohorts.iter() {
        let entry = generation_totals.entry(cohort.generation).or_insert(0);
        *entry += cohort.size as u64;
        total_population += cohort.size as u64;
    }

    let mut generation_shares: HashMap<GenerationId, f32> = HashMap::new();
    if total_population > 0 {
        for (gen, value) in generation_totals {
            generation_shares.insert(
                gen,
                (value as f32 / total_population as f32).clamp(0.0, 1.0),
            );
        }
    }

    roster.tick(&registry, manual_axes, &generation_shares);

    let sentiment = roster.sentiment_totals();
    let logistics = roster.logistics_total();
    let morale = roster.morale_total();
    let power = roster.power_total();
    let culture = roster.culture_resonance();

    axis_bias.set_influencer(sentiment);
    impacts.set_from_totals(logistics, morale, power);
    impacts.set_culture_resonance(culture);
}
