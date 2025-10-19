use bevy::prelude::Resource;
use rand::{rngs::SmallRng, Rng, SeedableRng};

use crate::scalar::{scalar_from_f32, Scalar};
use sim_proto::GenerationState;

pub type GenerationId = u16;

#[derive(Clone, Copy, Debug)]
pub struct GenerationBias {
    pub knowledge: Scalar,
    pub trust: Scalar,
    pub equity: Scalar,
    pub agency: Scalar,
}

impl GenerationBias {
    pub fn from_scaled(values: [i64; 4]) -> Self {
        Self {
            knowledge: Scalar::from_raw(values[0]),
            trust: Scalar::from_raw(values[1]),
            equity: Scalar::from_raw(values[2]),
            agency: Scalar::from_raw(values[3]),
        }
    }

    pub fn to_scaled(self) -> [i64; 4] {
        [
            self.knowledge.raw(),
            self.trust.raw(),
            self.equity.raw(),
            self.agency.raw(),
        ]
    }
}

#[derive(Clone, Debug)]
pub struct GenerationProfile {
    pub id: GenerationId,
    pub name: String,
    pub bias: GenerationBias,
}

#[derive(Resource, Clone)]
pub struct GenerationRegistry {
    profiles: Vec<GenerationProfile>,
}

impl GenerationRegistry {
    pub fn with_seed(seed: u64, count: usize) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        let adjectives = [
            "Radiant", "Silent", "Verdant", "Iron", "Auric", "Cerulean", "Gilded", "Umbral",
            "Vivid", "Harmonic", "Velvet", "Solar",
        ];
        let nouns = [
            "Bloom", "Horizon", "Forge", "Symphony", "Mantle", "Flux", "Fable", "Pulse", "Drift",
            "Concord", "Spire", "Eclipse",
        ];

        let mut profiles = Vec::with_capacity(count.max(1));
        for id in 0..count.max(1) {
            let adj = adjectives[rng.gen_range(0..adjectives.len())];
            let noun = nouns[rng.gen_range(0..nouns.len())];
            let ordinal = match id {
                0 => "I",
                1 => "II",
                2 => "III",
                3 => "IV",
                4 => "V",
                5 => "VI",
                6 => "VII",
                7 => "VIII",
                8 => "IX",
                9 => "X",
                _ => "",
            };
            let name = if ordinal.is_empty() {
                format!("{adj} {noun}")
            } else {
                format!("{adj} {noun} {}", ordinal)
            };

            let bias = GenerationBias {
                knowledge: random_bias(&mut rng),
                trust: random_bias(&mut rng),
                equity: random_bias(&mut rng),
                agency: random_bias(&mut rng),
            };

            profiles.push(GenerationProfile {
                id: id as GenerationId,
                name,
                bias,
            });
        }
        Self { profiles }
    }

    pub fn profiles(&self) -> &[GenerationProfile] {
        &self.profiles
    }

    pub fn profile(&self, id: GenerationId) -> Option<&GenerationProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn assign_for_index(&self, index: usize) -> GenerationId {
        if self.profiles.is_empty() {
            0
        } else {
            self.profiles[index % self.profiles.len()].id
        }
    }

    pub fn update_from_states(&mut self, states: &[GenerationState]) {
        if states.is_empty() {
            return;
        }
        self.profiles = states
            .iter()
            .map(|state| GenerationProfile {
                id: state.id,
                name: state.name.clone(),
                bias: GenerationBias::from_scaled([
                    state.bias_knowledge,
                    state.bias_trust,
                    state.bias_equity,
                    state.bias_agency,
                ]),
            })
            .collect();
    }

    pub fn from_states(states: &[GenerationState]) -> Self {
        let mut registry = Self {
            profiles: Vec::new(),
        };
        registry.update_from_states(states);
        registry
    }
}

fn random_bias(rng: &mut SmallRng) -> Scalar {
    let value: f32 = rng.gen_range(-0.15..=0.15);
    scalar_from_f32(value)
}
