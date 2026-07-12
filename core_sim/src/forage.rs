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
//! crash) and **never despawns** — plants reseed, so a depleted (feral) patch always recovers. A
//! small **reseed floor** (`forage.reseed_floor_fraction × carrying_capacity`) lifts a fully-depleted
//! patch back to a seed stock before regrowth each turn, so even a patch driven to exactly `0`
//! (Eradicate / f32 underflow / a restored `biomass = 0`) recovers rather than sticking at `0`. The
//! Allee branch of `net_biomass_delta` (via `sustainable_yield`) still sizes the **Sustain** gather
//! ceiling (so a collapsed patch yields no sustainable surplus). Foraging honors the full policy axis
//! (Sustain/Surplus/Market/Eradicate — §0-iii, parity with hunting): the `LaborTarget::Forage`
//! policy flows through `advance_labor_allocation` into `forage_take`, and a Market gather sells its
//! take as trade goods. **Cultivation** (Phase 1a) transposes husbandry onto patches: a patch carries
//! `cultivation_progress`/`owner`, a Sustain forage on a Thriving patch accrues it, `advance_cultivation`
//! pays a cultivated patch's owner a steady yield (without drawing biomass down), and the `cultivate`
//! command claims a patch early — the plant mirror of `fauna.rs`'s domestication.

use std::collections::HashMap;

use bevy::prelude::*;
use sim_schema::ForageState;

use crate::{
    components::{FollowPolicy, PopulationCohort, Tile, FOOD},
    fauna::{classify_ecology_phase, logistic_regrowth, sustainable_yield, EcologyPhase},
    fauna_config::EcologyConfig,
    food::FoodModuleTag,
    labor_config::{ForageLaborConfig, LaborConfigHandle},
    orders::FactionId,
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    systems::output_multiplier,
    wellbeing_config::WellbeingConfigHandle,
};

/// A live depletable forage patch on a `FoodModuleTag` tile. Mirrors the herd biomass model's
/// ecology subset, including cultivation (`cultivation_progress`/`owner`) — the plant analog of a
/// herd's domestication (Phase 1a).
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
    /// Cultivation progress in `[0.0, 1.0]`; `1.0` = cultivated. Accrues while a band
    /// Sustain-forages this (Thriving) patch and decays otherwise (see `advance_cultivation`).
    /// The plant mirror of `Herd::domestication_progress`.
    pub cultivation_progress: f32,
    /// Faction tending/owning this patch (`Some` iff `cultivation_progress > 0`).
    pub owner: Option<FactionId>,
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
            cultivation_progress: 0.0,
            owner: None,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the forage ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }

    /// A fully-cultivated (tended crop) patch: yields steady provisions to its owner each turn and
    /// is not gather-drawn. The plant mirror of `Herd::is_domesticated`.
    pub fn is_cultivated(&self) -> bool {
        self.cultivation_progress >= 1.0
    }

    /// Accrue cultivation progress for `faction` (the tending band). Sets ownership on the first
    /// accrual; only the owner makes progress. Clamped to 1.0 (auto-cultivation). No-op once the
    /// patch is cultivated. Mirrors `Herd::accrue_domestication`.
    pub(crate) fn accrue_cultivation(&mut self, faction: FactionId, amount: f32) {
        if self.is_cultivated() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.cultivation_progress = (self.cultivation_progress + amount).min(1.0);
        }
    }

    /// Decay cultivation progress toward zero when the patch isn't being actively tended; ownership
    /// lapses once progress reaches zero. A cultivated patch is left alone. Mirrors
    /// `Herd::decay_domestication`.
    pub(crate) fn decay_cultivation(&mut self, amount: f32) {
        if self.is_cultivated() {
            return;
        }
        self.cultivation_progress = (self.cultivation_progress - amount).max(0.0);
        // Reconcile the `owner is Some ⟺ progress > 0` invariant unconditionally, so a patch that
        // reaches zero progress never keeps a stale owner (which would block another faction from
        // ever tending it).
        if self.cultivation_progress <= 0.0 {
            self.owner = None;
        }
    }

    /// Finalize cultivation for `faction` (the `cultivate` command's early claim): set ownership
    /// and snap progress to 1.0 so `is_cultivated()` latches. Mirrors `Herd::claim_domestication`.
    pub fn claim_cultivation(&mut self, faction: FactionId) {
        self.owner = Some(faction);
        self.cultivation_progress = 1.0;
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

    /// Number of cultivated patches owned by `faction`. Folded (with domesticated herds) into the
    /// sedentarization "domestication" signal — plant + animal domestication share one driver.
    /// The plant mirror of `HerdRegistry::domesticated_count`.
    pub fn cultivated_count(&self, faction: FactionId) -> usize {
        self.patches
            .values()
            .filter(|patch| patch.is_cultivated() && patch.owner == Some(faction))
            .count()
    }
}

/// Reconstruct a live `ForagePatch` from its snapshot mirror (the rollback restore side of
/// `snapshot::forage_state`). The `progress`/`owner` `EcologyState` fields carry cultivation
/// (Phase 1a), mirroring `herd_from_state`.
fn forage_patch_from_state(state: &ForageState) -> ForagePatch {
    ForagePatch {
        tile: UVec2::new(state.x, state.y),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
        cultivation_progress: state.ecology.progress,
        owner: state.ecology.owner.map(FactionId),
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
    let forage = &labor.forage;
    for patch in registry.patches.values_mut() {
        regrow_patch(patch, &forage.ecology, forage.reseed_floor_fraction);
    }
}

/// Per-turn cultivation upkeep (`TurnStage::Logistics`, alongside `advance_forage_regrowth`): pay
/// each cultivated patch's owner a steady provisions yield — proportional to biomass and **without**
/// depleting the patch (sustainable tended harvest) — and decay cultivation progress on any
/// not-yet-cultivated patch. Runs before the same turn's accrual in `advance_labor_allocation`
/// (`Population`), so a Sustain-foraged patch nets `progress_per_turn - decay_per_turn` while an
/// untended one only decays. The plant mirror of `fauna::advance_husbandry`.
pub fn advance_cultivation(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    mut cohorts: Query<&mut PopulationCohort>,
) {
    let labor = labor_config.get();
    let wellbeing = wellbeing_config.get();
    let cultivation = &labor.forage.cultivation;
    // Accumulate each owner's tended-crop yield, then split it evenly across that faction's bands'
    // larders — the plant counterpart of husbandry income. An even split is a v1 (place-local yield
    // is the later improvement-engine work). FOOD income is fully fractional: accumulate as `Scalar`
    // so a small/near-cap patch whose per-turn yield is < 1 provision still credits the larder.
    let mut yields: HashMap<FactionId, Scalar> = HashMap::new();
    for patch in registry.patches.values_mut() {
        if patch.is_cultivated() {
            let Some(owner) = patch.owner else {
                continue;
            };
            let provisions = scalar_from_f32(patch.biomass * cultivation.provisions_per_biomass);
            if provisions > scalar_zero() {
                *yields.entry(owner).or_insert_with(scalar_zero) += provisions;
            }
        } else {
            patch.decay_cultivation(cultivation.decay_per_turn);
        }
    }
    if yields.is_empty() {
        return;
    }
    let mut band_counts: HashMap<FactionId, u32> = HashMap::new();
    for cohort in cohorts.iter() {
        if yields.contains_key(&cohort.faction) {
            *band_counts.entry(cohort.faction).or_insert(0) += 1;
        }
    }
    for mut cohort in cohorts.iter_mut() {
        if let (Some(&total), Some(&count)) = (
            yields.get(&cohort.faction),
            band_counts.get(&cohort.faction),
        ) {
            if count > 0 {
                // Productivity modifier stack (wellbeing): a discontented band tends the patch less
                // effectively — scale its even share by its output multiplier at PAYOUT.
                let share = total / Scalar::from_u32(count);
                let mult = output_multiplier(&cohort, &wellbeing);
                cohort.stores.add(FOOD, share * mult);
            }
        }
    }
}

/// Apply one turn of **pure logistic** regrowth toward the patch's carrying capacity and refresh its
/// ecology phase. Unlike a wild herd (`fauna::regrow_biomass`, which crashes below the Allee
/// threshold and despawns), a patch has no critical-depensation crash — a depleted (feral) patch
/// always recovers, and patches never despawn.
///
/// **Reseed floor.** `logistic_regrowth` returns `0` at `biomass == 0`, so a patch driven to exactly
/// `0` (repeated Eradicate + f32 underflow, `take_fraction = 1.0`, or a restored snapshot carrying
/// `biomass = 0`) would otherwise be stuck at `0` forever — contradicting the "always recovers"
/// invariant. To model plants reseeding from surrounding vegetation, a depleted patch is first lifted
/// to a small standing crop (`reseed_floor_fraction × carrying_capacity`) before regrowth, so it
/// recovers from that floor via the normal logistic curve. The lift only touches patches below the
/// floor — a healthy patch is untouched — and the floor is small (below `collapse_fraction`), so
/// Eradicate still crashes a patch hard into the Collapsing band; it just can't hold it at `0`.
fn regrow_patch(patch: &mut ForagePatch, ecology: &EcologyConfig, reseed_floor_fraction: f32) {
    let cap = patch.carrying_capacity;
    // Reseed a depleted patch up to the floor (no-op for a healthy patch) so it has a seed stock to
    // regrow from — plants reseed, so a crashed patch is never permanently stuck at 0.
    let reseed_floor = reseed_floor_fraction * cap;
    patch.biomass = patch.biomass.max(reseed_floor);
    let delta = logistic_regrowth(patch.biomass, cap, ecology.regrowth_rate);
    patch.biomass = (patch.biomass + delta).clamp(0.0, cap);
    patch.refresh_ecology_phase(ecology);
}

/// The forage counterpart of `fauna::hunt_take`: resolve the per-policy ecology ceiling, cap it by
/// the gathering crew's throughput (`workers × per_worker_biomass_capacity × seasonal`), clamp to
/// the patch's remaining biomass, **subtract it from the patch**, and convert the take to provisions
/// (× the caller's productivity `output_multiplier`). Returns the provisions gathered.
///
/// Policy ceilings mirror `hunt_take` (§0-iii — forage parity with hunting): **Sustain** = the
/// Maximum Sustainable Yield (`sustainable_yield`: regrowth at the most-productive biomass K/2, so a
/// patch at carrying capacity still yields a positive skim and a collapsed patch yields nothing);
/// **Surplus** = that × `surplus_multiplier` (overdraws a healthy
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
    // Per-policy ecology ceiling: Sustain = Maximum Sustainable Yield (regrowth at K/2, so a full
    // patch still yields), Surplus = that × multiplier, Market = a commercial share, Eradicate = an
    // aggressive strip. Market/Eradicate deplete a healthy patch; Sustain draws it toward K/2.
    let policy_ceiling = match policy {
        FollowPolicy::Sustain => sustainable_yield(patch.biomass, patch.carrying_capacity, ecology),
        FollowPolicy::Surplus => {
            sustainable_yield(patch.biomass, patch.carrying_capacity, ecology)
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
    fn sustain_on_full_patch_yields_msy_and_draws_to_half_cap() {
        // Regression (Phase 0 bug): a patch AT carrying capacity used to yield 0 under Sustain
        // (logistic regrowth is 0 at K), so a full patch stayed stuck at 0 forever. The MSY-based
        // `sustainable_yield` ceiling skims regrowth at the most-productive biomass (K/2), so a
        // full patch yields a positive harvest and Sustain draws it DOWN toward K/2 and holds.
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let half_cap = cap * 0.5;
        let msy = sustainable_yield(cap, cap, &forage.ecology);
        assert!(
            msy > 0.0,
            "a full patch must be sustainably harvestable: {msy}"
        );

        // Seed FULL, exactly as real forage patches spawn.
        let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
        patch.biomass = cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // First Sustain gather off the full patch: take equals MSY (positive), no longer 0.
        let biomass_before = patch.biomass;
        let expected_sustainable = sustainable_yield(biomass_before, cap, &forage.ecology);
        let provisions = forage_take(&mut patch, 20, FollowPolicy::Sustain, &forage, 1.0, 1.0);
        let take = biomass_before - patch.biomass;
        assert!(
            take > 0.0,
            "a full patch under Sustain must yield > 0: {take}"
        );
        assert!((take - expected_sustainable).abs() < 1e-3);
        let actual = provisions.to_f32();
        let sustainable = expected_sustainable * forage.provisions_per_biomass;
        assert!(
            (actual - sustainable).abs() < 1e-3,
            "actual ≈ sustainable (no overdraw): {actual} vs {sustainable}"
        );

        // Over many take+regrowth turns Sustain draws the patch DOWN from full and then HOLDS: the
        // post-take biomass settles at the MSY point (K/2), so the stored biomass stabilizes just
        // above K/2 and the per-turn yield stays ≈ MSY (never falling back to 0).
        let mut prev = patch.biomass;
        let mut last_take = take;
        for turn in 0..200 {
            let before = patch.biomass;
            let _ = forage_take(&mut patch, 20, FollowPolicy::Sustain, &forage, 1.0, 1.0);
            last_take = before - patch.biomass;
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
            if turn >= 190 {
                assert!(
                    (patch.biomass - prev).abs() < 1.0,
                    "late turns: biomass has stabilized: {} vs {}",
                    patch.biomass,
                    prev
                );
            }
            prev = patch.biomass;
        }
        assert!(
            patch.biomass < cap,
            "Sustain drew the full patch down: {}",
            patch.biomass
        );
        assert!(
            patch.biomass > half_cap,
            "Sustain holds at/above the MSY point K/2: {} vs {}",
            patch.biomass,
            half_cap
        );
        assert!(
            (last_take - msy).abs() < 1e-3 && last_take > 0.0,
            "steady-state yield stays ≈ MSY: {last_take} vs {msy}"
        );
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
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
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
        let expected_sustain = sustainable_yield(start, cap, &forage.ecology);
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
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
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
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn zero_biomass_patch_reseeds_and_recovers() {
        // Regression: a patch driven to *exactly* 0 (repeated Eradicate + f32 underflow,
        // `take_fraction = 1.0`, or a snapshot restore carrying biomass = 0) used to be stuck at 0
        // forever, because `logistic_regrowth(0, ..) == 0`. The reseed floor lifts a depleted patch
        // to a small standing crop each turn, so it recovers via normal regrowth — the "a feral
        // patch always recovers" invariant is now backed by code, not just the docstring.
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let floor = forage.reseed_floor_fraction * cap;
        assert!(floor > 0.0, "reseed floor must be a positive standing crop");

        let mut patch = ForagePatch::new(UVec2::new(5, 5), cap);
        patch.biomass = 0.0;
        patch.refresh_ecology_phase(&forage.ecology);

        // One turn off dead-zero: reseeded to the floor and already regrowing above it (> 0).
        regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
        assert!(
            patch.biomass > 0.0,
            "a 0-biomass patch must escape 0 via the reseed floor: {}",
            patch.biomass
        );
        assert!(patch.biomass >= floor);

        // Over subsequent turns it recovers toward a healthy level (Thriving), just like a patch
        // seeded a hair above 0 — no permanent stall at 0.
        for _ in 0..80 {
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn continuous_eradicate_bottoms_at_floor_then_recovers() {
        // The floor is small enough that Eradicate still crashes the patch hard (into Collapsing),
        // but it can't drive it *permanently* to 0: the patch bottoms out at ~the reseed floor and
        // recovers once Eradicate stops.
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let floor = forage.reseed_floor_fraction * cap;
        let mut patch = ForagePatch::new(UVec2::new(6, 6), cap);
        patch.refresh_ecology_phase(&forage.ecology);

        // Hammer with Eradicate + regrowth: biomass crashes but never sits at 0 — it floats at/above
        // the reseed floor while still reading Collapsing (a hard crash, not extinction).
        for _ in 0..60 {
            let _ = forage_take(&mut patch, 50, FollowPolicy::Eradicate, &forage, 1.0, 1.0);
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
            assert!(
                patch.biomass > 0.0,
                "Eradicate must not permanently zero a patch"
            );
        }
        assert!(
            patch.biomass < cap * forage.ecology.collapse_fraction,
            "Eradicate still crashes the patch hard: {} vs {}",
            patch.biomass,
            cap * forage.ecology.collapse_fraction
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        // Stop hunting: from the crashed floor the patch recovers all the way back to Thriving.
        for _ in 0..120 {
            regrow_patch(&mut patch, &forage.ecology, forage.reseed_floor_fraction);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass >= floor);
    }

    #[test]
    fn reseed_floor_leaves_healthy_patch_regrowth_unchanged() {
        // A patch above the floor must regrow identically with or without the reseed lift (the floor
        // only reseeds depleted patches — a healthy patch is untouched).
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let start = 0.5 * cap; // comfortably above reseed_floor_fraction × cap.

        let mut with_floor = ForagePatch::new(UVec2::new(7, 7), cap);
        with_floor.biomass = start;
        let mut without_floor = ForagePatch::new(UVec2::new(8, 8), cap);
        without_floor.biomass = start;

        for _ in 0..30 {
            regrow_patch(
                &mut with_floor,
                &forage.ecology,
                forage.reseed_floor_fraction,
            );
            // A zero floor is the "no reseed" baseline.
            regrow_patch(&mut without_floor, &forage.ecology, 0.0);
        }
        assert!(
            (with_floor.biomass - without_floor.biomass).abs() < 1e-6,
            "reseed floor must not perturb a healthy patch's regrowth: {} vs {}",
            with_floor.biomass,
            without_floor.biomass
        );
    }

    #[test]
    fn sustainable_yield_is_zero_below_allee() {
        // A collapsing (sub-Allee) patch is not sustainably harvestable.
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let below_allee = forage.ecology.collapse_fraction * cap * 0.5;
        assert_eq!(
            sustainable_yield(below_allee, cap, &forage.ecology),
            0.0,
            "a collapsing patch has no sustainable yield"
        );
    }

    #[test]
    fn sustainable_yield_plateaus_at_msy_above_half_cap() {
        // For any healthy biomass (>= K/2) the MSY ceiling is flat at the K/2 peak.
        let forage = test_forage_config();
        let cap = forage.carrying_capacity;
        let msy = sustainable_yield(cap * 0.5, cap, &forage.ecology);
        assert!(msy > 0.0);
        for frac in [0.5_f32, 0.6, 0.75, 0.9, 1.0] {
            assert!(
                (sustainable_yield(cap * frac, cap, &forage.ecology) - msy).abs() < 1e-6,
                "flat MSY plateau at biomass = {frac}·K"
            );
        }
    }

    #[test]
    fn forage_state_roundtrip_is_identity() {
        // A ForageState with non-default ecology AND cultivation so biomass / cap / phase /
        // progress / owner all round-trip (Phase 1a: cultivation now rides the snapshot).
        let original = ForageState {
            x: 7,
            y: 3,
            ecology: EcologyState {
                biomass: 42.5,
                carrying_capacity: 120.0,
                ecology_phase: "stressed".to_string(),
                progress: 0.6,
                owner: Some(3),
            },
        };

        let registry = ForageRegistry::from_states(std::slice::from_ref(&original));
        let patch = registry
            .patch(UVec2::new(7, 3))
            .expect("one patch restored");
        assert_eq!(patch.cultivation_progress, 0.6);
        assert_eq!(patch.owner, Some(FactionId(3)));
        let restored = crate::snapshot::forage_state(patch);
        assert_eq!(restored, original);
    }

    #[test]
    fn cultivation_accrual_is_owner_locked_and_clamped() {
        let mut patch = ForagePatch::new(UVec2::new(1, 1), 120.0);
        // First accrual claims ownership for the acting faction.
        patch.accrue_cultivation(FactionId(0), 0.3);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 0.3).abs() < 1e-6);
        // A different faction cannot accrue on an already-owned patch.
        patch.accrue_cultivation(FactionId(1), 0.5);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 0.3).abs() < 1e-6);
        // Owner accrues; progress clamps at 1.0 and latches cultivated.
        patch.accrue_cultivation(FactionId(0), 0.9);
        assert!(patch.is_cultivated());
        assert_eq!(patch.cultivation_progress, 1.0);
        // A cultivated patch is a no-op for further accrual.
        patch.accrue_cultivation(FactionId(0), 0.5);
        assert_eq!(patch.cultivation_progress, 1.0);
    }

    #[test]
    fn cultivation_decay_clears_owner_at_zero_and_spares_cultivated() {
        let mut patch = ForagePatch::new(UVec2::new(2, 2), 120.0);
        patch.accrue_cultivation(FactionId(0), 0.05);
        patch.decay_cultivation(0.02);
        assert!((patch.cultivation_progress - 0.03).abs() < 1e-6);
        assert_eq!(patch.owner, Some(FactionId(0)), "owner held above zero");
        // Decaying to zero clears ownership so another faction can later tend it.
        patch.decay_cultivation(1.0);
        assert_eq!(patch.cultivation_progress, 0.0);
        assert_eq!(patch.owner, None);
        // A cultivated patch never decays.
        patch.claim_cultivation(FactionId(1));
        patch.decay_cultivation(0.5);
        assert!(patch.is_cultivated());
    }

    #[test]
    fn cultivated_count_filters_by_owner() {
        let mut registry = ForageRegistry::default();
        let mut a = ForagePatch::new(UVec2::new(0, 0), 120.0);
        a.claim_cultivation(FactionId(0));
        let mut b = ForagePatch::new(UVec2::new(1, 0), 120.0);
        b.claim_cultivation(FactionId(1));
        let uncultivated = ForagePatch::new(UVec2::new(2, 0), 120.0);
        registry.patches.insert(a.tile, a);
        registry.patches.insert(b.tile, b);
        registry.patches.insert(uncultivated.tile, uncultivated);
        assert_eq!(registry.cultivated_count(FactionId(0)), 1);
        assert_eq!(registry.cultivated_count(FactionId(1)), 1);
        assert_eq!(registry.cultivated_count(FactionId(2)), 0);
    }
}
