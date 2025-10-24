//! Shared runtime utilities for Shadow-Scale.
//!
//! This crate re-exports the data contracts from `sim_schema` and hosts helper
//! routines that operate on those contracts (validation, transforms, command
//! utilities) without depending on the full Bevy runtime in `core_sim`.

use std::cmp::{max, min};

pub use sim_schema::*;

pub mod scripting;
pub use scripting::{
    capability_registry, CapabilityRegistry, CapabilitySpec, ScriptManifestRef, SessionAccess,
    SimScriptState,
};

/// Fixed-point scaling constant shared with `core_sim::Scalar`.
pub const FIXED_POINT_SCALE: i64 = 1_000_000;

/// Clamp a fixed-point value between two bounds.
pub fn clamp_fixed(value: i64, min_value: i64, max_value: i64) -> i64 {
    if value < min_value {
        min_value
    } else if value > max_value {
        max_value
    } else {
        value
    }
}

/// Multiply two fixed-point values (scaled by [`FIXED_POINT_SCALE`]).
pub fn fixed_mul(lhs: i64, rhs: i64) -> i64 {
    ((lhs as i128 * rhs as i128) / FIXED_POINT_SCALE as i128) as i64
}

/// Represents the tuning curve that maps openness to leak timers in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeLeakCurve {
    pub min_ticks: u32,
    pub max_ticks: u32,
    pub exponent: f32,
}

impl TradeLeakCurve {
    pub const fn new(min_ticks: u32, max_ticks: u32, exponent: f32) -> Self {
        Self {
            min_ticks,
            max_ticks,
            exponent,
        }
    }

    /// Resolve a leak timer (in ticks) based on the provided openness value.
    pub fn ticks_for_openness(&self, openness_raw: i64) -> u32 {
        let min_ticks = min(self.min_ticks, self.max_ticks);
        let max_ticks = max(self.min_ticks, self.max_ticks);
        if max_ticks == 0 {
            return 0;
        }

        let openness = (openness_raw as f64 / FIXED_POINT_SCALE as f64).clamp(0.0, 1.0);
        let exponent = if self.exponent <= 0.0 {
            1.0
        } else {
            self.exponent as f64
        };
        let blend = openness.powf(exponent);
        let ticks = (max_ticks as f64 * (1.0 - blend)) + (min_ticks as f64 * blend);
        ticks.round().clamp(min_ticks as f64, max_ticks as f64) as u32
    }
}

/// Apply per-tick openness decay, ensuring the result remains within [0, 1].
pub fn apply_openness_decay(openness_raw: i64, decay_raw: i64) -> i64 {
    let openness = clamp_fixed(openness_raw, 0, FIXED_POINT_SCALE);
    let decay = clamp_fixed(decay_raw, 0, FIXED_POINT_SCALE);
    clamp_fixed(openness - decay, 0, FIXED_POINT_SCALE)
}

/// Scale a set of known technology fragments for migration payload synthesis.
pub fn scale_migration_fragments(
    source: &[KnownTechFragment],
    scaling_raw: i64,
    fidelity_floor_raw: i64,
) -> Vec<KnownTechFragment> {
    if source.is_empty() {
        return Vec::new();
    }

    let scaling = clamp_fixed(scaling_raw, 0, FIXED_POINT_SCALE);
    if scaling == 0 {
        return Vec::new();
    }
    let fidelity_floor = clamp_fixed(fidelity_floor_raw, 0, FIXED_POINT_SCALE);

    let mut payload: Vec<KnownTechFragment> = source
        .iter()
        .filter_map(|fragment| {
            if fragment.progress <= 0 {
                return None;
            }
            let scaled_progress =
                clamp_fixed(fixed_mul(fragment.progress, scaling), 0, FIXED_POINT_SCALE);
            if scaled_progress == 0 {
                return None;
            }
            let base_fidelity = if fragment.fidelity > 0 {
                fragment.fidelity
            } else {
                FIXED_POINT_SCALE
            };
            let mut fidelity = fixed_mul(base_fidelity, scaling);
            fidelity = clamp_fixed(fidelity, fidelity_floor, FIXED_POINT_SCALE);
            Some(KnownTechFragment {
                discovery_id: fragment.discovery_id,
                progress: scaled_progress,
                fidelity,
            })
        })
        .collect();

    payload.sort_by_key(|fragment| fragment.discovery_id);
    payload
}

/// Merge migration payload fragments into an existing fragment list.
pub fn merge_fragment_payload(
    destination: &mut Vec<KnownTechFragment>,
    payload: &[KnownTechFragment],
    cap_raw: i64,
) {
    if payload.is_empty() {
        return;
    }

    let cap = clamp_fixed(cap_raw, 0, FIXED_POINT_SCALE);
    for fragment in payload {
        if fragment.progress <= 0 {
            continue;
        }
        if let Some(existing) = destination
            .iter_mut()
            .find(|entry| entry.discovery_id == fragment.discovery_id)
        {
            existing.progress = clamp_fixed(existing.progress + fragment.progress, 0, cap);
            existing.fidelity = max(existing.fidelity, fragment.fidelity);
        } else {
            let mut clone = fragment.clone();
            clone.progress = clamp_fixed(clone.progress, 0, cap);
            destination.push(clone);
        }
    }

    destination.sort_by_key(|fragment| fragment.discovery_id);
}
