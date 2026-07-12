//! Depletable forage patches (Intensification §0-ii — "forage parity with hunting").
//!
//! Transposes the herd biomass / logistic-regrowth model (`fauna.rs`) onto worked forage tiles.
//! Every `FoodModuleTag` tile gains a live per-patch `{ biomass, carrying_capacity, ecology_phase }`
//! (`ForagePatch`) held in the authoritative `ForageRegistry` resource, keyed by tile coord.
//! Foraging **draws the patch down** (`forage_take`), and `advance_forage_regrowth` regrows it each
//! turn toward `carrying_capacity`. The patch's state round-trips through the rollback snapshot via
//! the shared `sim_schema::ForageState`/`EcologyState` records — the same 0-i persistence pattern
//! the `HerdRegistry` uses (`ForageRegistry::from_states`/`update_from_states`).
//!
//! Unlike a wild herd, a patch uses **pure logistic regrowth** (no Allee / critical-depensation
//! crash) and **never despawns** — plants reseed, so a depleted (feral) patch always recovers. The
//! Allee branch of `net_biomass_delta` is still used to size the **Sustain** gather ceiling (so a
//! collapsed patch yields no sustainable surplus). Foraging honors the full policy axis
//! (Sustain/Surplus/Market/Eradicate — §0-iii, parity with hunting): the `LaborTarget::Forage`
//! policy flows through `advance_labor_allocation` into `forage_take`, and a Market gather sells its
//! take as trade goods. Cultivation (`progress`/`owner` on a patch) is Phase 1 — those
//! `EcologyState` fields stay `0.0`/`None` here.

use std::collections::HashMap;

use bevy::prelude::*;
use sim_schema::ForageState;

use crate::{
    components::{FollowPolicy, Tile},
    fauna::{classify_ecology_phase, logistic_regrowth, net_biomass_delta, EcologyPhase},
    fauna_config::EcologyConfig,
    food::FoodModuleTag,
    labor_config::{ForageLaborConfig, LaborConfigHandle},
    scalar::{scalar_from_f32, Scalar},
};

/// A live depletable forage patch on a `FoodModuleTag` tile. Mirrors the herd biomass model's
/// ecology subset; `progress`/`owner` (cultivation) are Phase 1, so they are not carried here.
#[derive(Debug, Clone)]
pub struct ForagePatch {
    /// Tile the patch sits on (its registry key).
    pub tile: UVec2,
    /// Live gatherable stock, drawn down by `forage_take`, regrown by `advance_forage_regrowth`.
    pub biomass: f32,
    /// Per-patch carrying cap that biomass regrows toward (flat default; per-`FoodModule` later).
    pub carrying_capacity: f32,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from biomass vs
    /// `carrying_capacity`. Lights the client over-forage readout the same way herds do.
    pub ecology_phase: EcologyPhase,
}

impl ForagePatch {
    /// A fresh patch at full biomass (= carrying capacity). Phase is `Thriving` until refreshed
    /// against the ecology config.
    pub fn new(tile: UVec2, carrying_capacity: f32) -> Self {
        Self {
            tile,
            biomass: carrying_capacity,
            carrying_capacity,
            ecology_phase: EcologyPhase::Thriving,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the forage ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ForageRegistry {
    /// Live patches keyed by tile coord. Iteration order is non-deterministic; the snapshot capture
    /// sorts by coord for a stable rollback record.
    pub patches: HashMap<UVec2, ForagePatch>,
}

impl ForageRegistry {
    pub fn patch(&self, tile: UVec2) -> Option<&ForagePatch> {
        self.patches.get(&tile)
    }

    pub fn patch_mut(&mut self, tile: UVec2) -> Option<&mut ForagePatch> {
        self.patches.get_mut(&tile)
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn len(&self) -> usize {
        self.patches.len()
    }

    /// Rebuild the authoritative patch map from a rollback snapshot's `ForageState`s (clear +
    /// rebuild), mirroring `HerdRegistry::update_from_states`. Restores per-patch biomass / phase so
    /// a rollback rewinds forage depletion, not just display state.
    pub fn update_from_states(&mut self, states: &[ForageState]) {
        self.patches = states
            .iter()
            .map(|state| {
                let patch = forage_patch_from_state(state);
                (patch.tile, patch)
            })
            .collect();
    }

    /// Construct a registry directly from snapshot `ForageState`s (mirrors
    /// `HerdRegistry::from_states`).
    pub fn from_states(states: &[ForageState]) -> Self {
        let mut registry = Self::default();
        registry.update_from_states(states);
        registry
    }
}

/// Reconstruct a live `ForagePatch` from its snapshot mirror (the rollback restore side of
/// `snapshot::forage_state`). The `progress`/`owner` `EcologyState` fields are ignored in Phase 0.
fn forage_patch_from_state(state: &ForageState) -> ForagePatch {
    ForagePatch {
        tile: UVec2::new(state.x, state.y),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
    }
}

/// Seed a full patch on every `FoodModuleTag` tile at Startup (idempotent — a world that already
/// carries patches, e.g. after a rollback restore, is skipped). Runs in the Startup chain after
/// `spawn_initial_world` has stamped the food-module tags. Mirrors `spawn_initial_herds`.
pub fn spawn_initial_forage(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    tiles: Query<(&Tile, &FoodModuleTag)>,
) {
    if !registry.patches.is_empty() {
        return;
    }
    let labor = labor_config.get();
    let forage = &labor.forage;
    for (tile, _module) in tiles.iter() {
        let mut patch = ForagePatch::new(tile.position, forage.carrying_capacity);
        patch.refresh_ecology_phase(&forage.ecology);
        registry.patches.insert(tile.position, patch);
    }
}

/// Per-turn forage regrowth (`TurnStage::Logistics`, alongside `advance_herds`): regrow every patch
/// toward its carrying capacity and refresh its ecology phase. Patches never despawn.
pub fn advance_forage_regrowth(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
) {
    let labor = labor_config.get();
    let ecology = &labor.forage.ecology;
    for patch in registry.patches.values_mut() {
        regrow_patch(patch, ecology);
    }
}

/// Apply one turn of **pure logistic** regrowth toward the patch's carrying capacity and refresh its
/// ecology phase. Unlike a wild herd (`fauna::regrow_biomass`, which crashes below the Allee
/// threshold and despawns), a patch has no critical-depensation crash — a depleted (feral) patch
/// always recovers, and patches never despawn.
fn regrow_patch(patch: &mut ForagePatch, ecology: &EcologyConfig) {
    let cap = patch.carrying_capacity;
    let delta = logistic_regrowth(patch.biomass, cap, ecology.regrowth_rate);
    patch.biomass = (patch.biomass + delta).clamp(0.0, cap);
    patch.refresh_ecology_phase(ecology);
}

/// The forage counterpart of `fauna::hunt_take`: resolve the per-policy ecology ceiling, cap it by
/// the gathering crew's throughput (`workers × per_worker_biomass_capacity × seasonal`), clamp to
/// the patch's remaining biomass, **subtract it from the patch**, and convert the take to provisions
/// (× the caller's productivity `output_multiplier`). Returns the provisions gathered.
///
/// Policy ceilings mirror `hunt_take` (§0-iii — forage parity with hunting): **Sustain** = one
/// turn's net regrowth (`net_biomass_delta(..).max(0.0)`, a collapsed patch yields nothing and a
/// healthy patch stays healthy); **Surplus** = that × `surplus_multiplier` (overdraws a healthy
/// patch → slow decline); **Market** = `market.take_fraction × biomass` (a commercial share → fast
/// depletion; the caller sells the take as trade goods); **Eradicate** = `eradicate.take_fraction ×
/// biomass` (strip the patch, no floor). All are then throughput-capped and clamped to biomass.
pub(crate) fn forage_take(
    patch: &mut ForagePatch,
    workers: u32,
    policy: FollowPolicy,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    seasonal: f32,
) -> Scalar {
    let ecology = &forage.ecology;
    // Per-policy ecology ceiling: Sustain = net regrowth (skim), Surplus = that × multiplier,
    // Market = a commercial share, Eradicate = an aggressive strip. Market/Eradicate deplete a
    // healthy patch; Sustain keeps it healthy by default.
    let policy_ceiling = match policy {
        FollowPolicy::Sustain => {
            net_biomass_delta(patch.biomass, patch.carrying_capacity, ecology).max(0.0)
        }
        FollowPolicy::Surplus => {
            net_biomass_delta(patch.biomass, patch.carrying_capacity, ecology).max(0.0)
                * forage.surplus_multiplier
        }
        FollowPolicy::Market => forage.market.take_fraction * patch.biomass,
        FollowPolicy::Eradicate => forage.eradicate.take_fraction * patch.biomass,
    };
    // Gather throughput caps the take (seasonal folded in); clamp to the patch's remaining biomass.
    let worker_cap = workers as f32 * forage.per_worker_biomass_capacity * seasonal.max(0.0);
    let take = worker_cap
        .min(policy_ceiling)
        .max(0.0)
        .clamp(0.0, patch.biomass);
    patch.biomass -= take;
    // FOOD income is fully fractional (a few foragers may gather < 1 provision/turn).
    scalar_from_f32(take * forage.provisions_per_biomass * output_multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_schema::EcologyState;

    /// A forage config with an easily-reasoned-about cap and dynamics for the unit tests.
    fn test_forage_config() -> ForageLaborConfig {
        ForageLaborConfig::default()
    }

    #[test]
    fn sustain_on_thriving_patch_holds_and_matches_sustainable() {
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        // Seed above cap/2 (Thriving) where net regrowth is positive. Above the logistic peak a
        // Sustain skim leaves the patch nearer the peak, so it holds/recovers turn over turn.
        let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
        patch.biomass = 0.8 * cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // Sustain with plenty of workers: take is ceiling-capped to net regrowth, not throughput.
        let biomass_before = patch.biomass;
        let expected_sustainable = net_biomass_delta(biomass_before, cap, &forage.ecology).max(0.0);
        let provisions = forage_take(&mut patch, 20, FollowPolicy::Sustain, &forage, 1.0, 1.0);
        let take = biomass_before - patch.biomass;

        // The Sustain take equals one turn's net regrowth (the sustainable rate) — no overdraw.
        assert!((take - expected_sustainable).abs() < 1e-3);
        let actual = provisions.to_f32();
        let sustainable = expected_sustainable * forage.provisions_per_biomass;
        assert!((actual - sustainable).abs() < 1e-3);
        assert!(actual <= sustainable + 1e-4, "sustain must not over-forage");

        // Over many take+regrowth turns the patch holds/recovers — Sustain never draws a Thriving
        // patch down (above cap/2 each cycle regrows at least what it gathered), so biomass stays
        // at/above where it sits going into the loop and stays Thriving.
        let loop_floor = patch.biomass - 1e-2;
        for _ in 0..40 {
            let _ = forage_take(&mut patch, 20, FollowPolicy::Sustain, &forage, 1.0, 1.0);
            regrow_patch(&mut patch, &forage.ecology);
            assert!(
                patch.biomass >= loop_floor,
                "sustain must not draw a Thriving patch down: {}",
                patch.biomass
            );
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn heavy_take_depletes_patch_and_drops_phase() {
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let mut patch = ForagePatch::new(UVec2::new(2, 3), cap);
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // A heavier-than-sustainable draw (non-Sustain ceiling = throughput only) with enough
        // workers to out-pace regrowth drives biomass DOWN turn over turn and drops the phase.
        let mut last = patch.biomass;
        let mut saw_stressed = false;
        for _ in 0..40 {
            let _ = forage_take(&mut patch, 3, FollowPolicy::Eradicate, &forage, 1.0, 1.0);
            regrow_patch(&mut patch, &forage.ecology);
            assert!(patch.biomass < last + 1e-3, "biomass must trend downward");
            last = patch.biomass;
            if patch.ecology_phase == EcologyPhase::Stressed {
                saw_stressed = true;
            }
        }
        assert!(
            saw_stressed,
            "phase should pass through Stressed while depleting"
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);
        assert!(patch.biomass < forage.ecology.collapse_fraction * cap);
    }

    /// The forage policy axis (§0-iii, parity with hunting): on an identical Thriving patch with
    /// ample workers (so the take is ceiling-bound, not throughput-bound), a heavier policy takes
    /// more — `Sustain ≤ Surplus < Market < Eradicate` — and the heavier policies deplete the patch
    /// faster (biomass drops more in a single turn).
    #[test]
    fn policy_ceilings_order_take_and_depletion() {
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let start = 0.8 * cap; // Thriving, clear positive net regrowth.
        let workers = 20; // worker_cap (20 × per_worker) far exceeds every policy ceiling.

        // One-turn take under each policy from the same starting biomass.
        let take_under = |policy: FollowPolicy| -> (f32, f32) {
            let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
            patch.biomass = start;
            let provisions = forage_take(&mut patch, workers, policy, &forage, 1.0, 1.0);
            let take = start - patch.biomass;
            (take, provisions.to_f32())
        };

        let (sustain_take, _) = take_under(FollowPolicy::Sustain);
        let (surplus_take, _) = take_under(FollowPolicy::Surplus);
        let (market_take, _) = take_under(FollowPolicy::Market);
        let (eradicate_take, _) = take_under(FollowPolicy::Eradicate);

        // Sustain is the regrowth skim; Surplus overdraws it; Market/Eradicate strip a share.
        assert!(sustain_take <= surplus_take + 1e-4, "Sustain ≤ Surplus");
        assert!(surplus_take < market_take, "Surplus < Market");
        assert!(market_take < eradicate_take, "Market < Eradicate");
        // Heavier policies deplete the patch faster (more biomass removed this turn).
        assert!(
            market_take > sustain_take,
            "Market depletes faster than Sustain"
        );
        assert!(
            eradicate_take > sustain_take,
            "Eradicate depletes faster than Sustain"
        );
        // Sustain leaves the patch at/above where it started net of regrowth (no overdraw): the
        // take equals the net regrowth ceiling exactly.
        let expected_sustain = net_biomass_delta(start, cap, &forage.ecology).max(0.0);
        assert!((sustain_take - expected_sustain).abs() < 1e-3);
    }

    #[test]
    fn below_cap_patch_regrows_toward_cap() {
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
        patch.biomass = 0.25 * cap;
        patch.refresh_ecology_phase(&forage.ecology);

        let mut prev = patch.biomass;
        for _ in 0..30 {
            regrow_patch(&mut patch, &forage.ecology);
            assert!(patch.biomass >= prev, "regrowth must be monotonic upward");
            prev = patch.biomass;
        }
        // Converges toward the cap.
        assert!(patch.biomass > 0.9 * cap);
        assert!(patch.biomass <= cap);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn crashed_patch_recovers_no_extinction() {
        // Pure-logistic regrowth: a patch driven far below the Allee threshold still recovers
        // (plants have no critical-depensation crash / extinction floor).
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let mut patch = ForagePatch::new(UVec2::new(4, 4), cap);
        patch.biomass = 0.02 * cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        for _ in 0..80 {
            regrow_patch(&mut patch, &forage.ecology);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn forage_state_roundtrip_is_identity() {
        // A ForageState with non-default ecology so biomass / cap / phase all round-trip.
        let original = ForageState {
            x: 7,
            y: 3,
            ecology: EcologyState {
                biomass: 42.5,
                carrying_capacity: 120.0,
                ecology_phase: "stressed".to_string(),
                progress: 0.0,
                owner: None,
            },
        };

        let registry = ForageRegistry::from_states(std::slice::from_ref(&original));
        let patch = registry
            .patch(UVec2::new(7, 3))
            .expect("one patch restored");
        let restored = crate::snapshot::forage_state(patch);
        assert_eq!(restored, original);
    }
}
