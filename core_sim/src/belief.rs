//! **BELIEF ON A PLACE** — a per-tile stock, not a faction's (`docs/plan_civilization_steps.md`
//! §"Belief is a property of a place", §"The first pulls are not productive").
//!
//! A tile accrues belief from the things that happen *there*: deaths while a band stands on it (the
//! cemetery — evidence of returning), gatherings held on it, and a monument held on it. Every source
//! writes through the one seam, [`BeliefRegistry::add`]; the monument's is
//! [`advance_improvement_belief`], which credits whatever `belief_config.json` ->
//! `improvement_belief` names, so the engine names no rung.
//!
//! # Belief is monotone
//!
//! **Nothing ever subtracts from it or decays it.** An abandoned place keeps its dead: a band that
//! walks away leaves the belief where it accrued, and the stock reads the same a hundred turns
//! later. There is no decay lever in `belief_config.json` on purpose, and there is no `remove` /
//! `set` on the registry — a new source adds, it never rewrites.
//!
//! # The unit
//!
//! Dead-equivalents: at the shipped `belief_per_death` of `1.0`, a place reading `12` holds twelve
//! people's worth of ancestors. Later sources are priced in that unit.

use bevy::prelude::{Query, Res, ResMut, Resource, UVec2, With};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::belief_config::{BeliefConfig, BeliefConfigHandle};
use crate::components::{PopulationCohort, ResidentBand};
use crate::intensification::{LadderConfigHandle, LadderKnowledge};
use crate::orders::FactionId;
use crate::resources::DiscoveryProgressLedger;
use crate::scalar::scalar_from_f32;
use crate::tile_improvement::TileImprovementRegistry;
use crate::wellbeing_config::WellbeingConfigHandle;

/// The ritual lesson's name in `intensification_ladder.json` (`knowledge.lesson_costs`,
/// `standalone_knowledges`) and in `start_profile_knowledge_tags.json`.
pub const RITUAL_KNOWLEDGE: &str = "ritual";

/// The discovery-ledger id of the ritual lesson. Taught by gatherings rather than by a rung, so it is
/// coded beside the system that teaches it ([`credit_ritual_lesson`]); 2023 is the last id the tag
/// table had given out.
pub const RITUAL_DISCOVERY_ID: u32 = 2024;

/// What a tile with no entry holds — no belief has ever accrued there.
pub const NO_BELIEF: f32 = 0.0;

/// **EVERY PLACE THAT HOLDS BELIEF, ONE VALUE PER TILE** — `routes::RoadRegistry`'s shape.
///
/// **Sparse**: only a tile whose belief is above [`NO_BELIEF`] has an entry, so an untouched map
/// checkpoints an empty map. **`BTreeMap`, not `HashMap`** — the iteration order is observed by the
/// checkpoint, so it has to be an order and not an accident. Keyed `(y, x)` so that order is
/// row-major, like every other tile sweep in the engine.
#[derive(Resource, Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefRegistry {
    tiles: BTreeMap<(u32, u32), f32>,
}

impl BeliefRegistry {
    /// **THE SEAM EVERY BELIEF SOURCE WRITES THROUGH** — deaths
    /// ([`BeliefRegistry::credit_deaths`]) and gatherings ([`BeliefRegistry::credit_gathering`]); the
    /// monument (#692) later. Adds
    /// `amount` to the tile at `position`. An `amount` at or below [`NO_BELIEF`] is ignored, which
    /// is what keeps belief monotone and the registry sparse: no caller can draw a place down, and
    /// a zero accrual never mints an entry.
    pub fn add(&mut self, position: UVec2, amount: f32) {
        if amount <= NO_BELIEF {
            return;
        }
        *self
            .tiles
            .entry((position.y, position.x))
            .or_insert(NO_BELIEF) += amount;
    }

    /// The belief standing on the tile at `position`; [`NO_BELIEF`] where none has ever accrued.
    pub fn get(&self, position: UVec2) -> f32 {
        self.tiles
            .get(&(position.y, position.x))
            .copied()
            .unwrap_or(NO_BELIEF)
    }

    /// **The deaths source.** `deaths` people died while their band stood on `position` — credit
    /// the place at `belief_per_death` each. Fractional deaths accrue exactly: belief is a
    /// continuous stock, so a third of a person three turns running is one death's worth.
    ///
    /// The caller decides *where*: the tile the band **stands on**, never its home, and only for
    /// people who died there. A detached expedition's casualties and a far work party's (a row
    /// posted past `band_work_range`) are not where the band stands, and credit nothing.
    pub fn credit_deaths(&mut self, position: UVec2, deaths: f32, config: &BeliefConfig) {
        self.add(position, deaths * config.belief_per_death);
    }

    /// **The gatherings source.** One gathering was held on `position` — credit the place at
    /// `belief_per_gathering`. The caller decides *where*: the standing tile of the larger of the
    /// two bands that met, never a home.
    pub fn credit_gathering(&mut self, position: UVec2, config: &BeliefConfig) {
        self.add(position, config.belief_per_gathering);
    }

    /// **The improvement source.** A held improvement of a rung `improvement_belief` names credits its
    /// own tile `per_turn`, through the same [`Self::add`] seam. See [`advance_improvement_belief`].
    pub fn credit_improvement(&mut self, position: UVec2, per_turn: f32) {
        self.add(position, per_turn);
    }

    /// Every place holding belief, in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (UVec2, f32)> + '_ {
        self.tiles
            .iter()
            .map(|(&(y, x), &belief)| (UVec2::new(x, y), belief))
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

/// **A HELD MONUMENT ADDS BELIEF** (#692) - every tile improvement whose current HELD rung
/// `belief_config.json` -> `improvement_belief` lists credits `per_turn` to its own tile, through
/// [`BeliefRegistry::add`].
///
/// Run right after `advance_tile_improvements` has settled the turn (decay included), so a rung that
/// has just slipped back to its root pays nothing and one that has just been completed pays this very
/// turn. **Only the held rung pays**: a half-built monument holds the ground beneath it
/// (`partial_credit: on_completion`) and credits nothing. Belief is monotone, so a decayed monument
/// leaves what it already added. The registry iterates in `(y, x, branch)` order, so the credits
/// land in a fixed order.
pub fn advance_improvement_belief(
    improvements: Res<TileImprovementRegistry>,
    config: Res<BeliefConfigHandle>,
    mut belief: ResMut<BeliefRegistry>,
) {
    let config = config.get();
    if config.improvement_belief.is_empty() {
        return;
    }
    for improvement in improvements.iter() {
        let held = improvement.held_rung();
        if let Some(source) = config
            .improvement_belief
            .iter()
            .find(|source| source.rung == held)
        {
            belief.credit_improvement(improvement.tile, source.per_turn);
        }
    }
}

/// **A GATHERING TEACHES RITUAL** (#692) - one gathering is `ritual_lesson_per_gathering` practice
/// toward the [`RITUAL_KNOWLEDGE`] lesson, credited to `faction` through the ladder's one
/// practice-to-ledger seam ([`LadderKnowledge::ledger_credit`]). The caller pays each PEOPLE in the
/// pair once.
pub fn credit_ritual_lesson(
    knowledge: &LadderKnowledge,
    faction: FactionId,
    ledger: &mut DiscoveryProgressLedger,
) {
    credit_ritual_practice(
        knowledge,
        faction,
        knowledge.ritual_lesson_per_gathering,
        ledger,
    );
}

/// **ANY PRACTICE TOWARD RITUAL, through the one seam** - `practice` units converted by
/// [`LadderKnowledge::ledger_credit`] and added to `faction`'s ledger. A gathering's
/// ([`credit_ritual_lesson`]) and the dead's trickle ([`advance_ritual_trickle`]) both land here.
/// A practice at or below zero credits nothing.
pub fn credit_ritual_practice(
    knowledge: &LadderKnowledge,
    faction: FactionId,
    practice: f32,
    ledger: &mut DiscoveryProgressLedger,
) {
    if practice <= 0.0 {
        return;
    }
    let Some(amount) = knowledge.ledger_credit(RITUAL_KNOWLEDGE, practice) else {
        return;
    };
    ledger.add_progress(faction, RITUAL_DISCOVERY_ID, scalar_from_f32(amount));
}

/// **LIVING NEAR YOUR DEAD TEACHES RITUAL, SLOWLY** - each turn, every people is credited
/// `knowledge.ritual_lesson_per_turn_at_full_tie` x the strongest ancestor tie among its RESIDENT
/// bands. The tie is the culture code's own (`CultureConfig::ancestor_tie`: the anchor's saturating
/// weight x the kin-relay strength decoded from `last_belief_relay_hops`), read the way
/// [`crate::culture::band_ancestor_pulls`] reads it.
///
/// **MAX, never the sum**: parking more bands by one grave must not multiply the lesson. A people
/// whose strongest tie is zero (no anchor, or nothing reaches its bands) gets nothing, and a lever
/// of `0` is a no-op. The hop count is the one `simulate_population` stored this turn, so the
/// system runs right after it. A people that already knows ritual is credited as usual - the ledger
/// clamps, exactly as a further gathering's credit does. `BTreeMap` by faction keeps the credit
/// order fixed.
pub fn advance_ritual_trickle(
    bands: Query<&PopulationCohort, With<ResidentBand>>,
    belief: Res<BeliefRegistry>,
    wellbeing: Res<WellbeingConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
) {
    let knowledge = &ladder.get().knowledge;
    let per_turn = knowledge.ritual_lesson_per_turn_at_full_tie;
    if per_turn <= 0.0 {
        return;
    }
    let culture = &wellbeing.get().culture;
    let mut strongest: BTreeMap<FactionId, f32> = BTreeMap::new();
    for cohort in bands.iter() {
        let hops = crate::belief_relay::hops_from_wire(
            cohort.belief_anchor,
            cohort.last_belief_relay_hops,
        );
        let tie = culture.ancestor_tie(cohort.belief_anchor.map(|anchor| belief.get(anchor)), hops);
        let entry = strongest.entry(cohort.faction).or_insert(tie);
        *entry = entry.max(tie);
    }
    for (faction, tie) in strongest {
        credit_ritual_practice(knowledge, faction, per_turn * tie, &mut discovery);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE: UVec2 = UVec2::new(3, 7);
    const ELSEWHERE: UVec2 = UVec2::new(7, 3);
    /// A whole death's belief at the shipped `belief_per_death`.
    const ONE_DEATH: f32 = 1.0;
    /// A third of a person — the fractional flow the demographic model produces.
    const A_THIRD_OF_A_DEATH: f32 = 1.0 / 3.0;
    const TOLERANCE: f32 = 1e-5;

    #[test]
    fn an_untouched_place_holds_no_belief_and_no_entry() {
        let registry = BeliefRegistry::default();
        assert_eq!(registry.get(PLACE), NO_BELIEF);
        assert!(registry.is_empty());
    }

    #[test]
    fn add_accrues_on_the_named_tile_only() {
        let mut registry = BeliefRegistry::default();
        registry.add(PLACE, ONE_DEATH);
        registry.add(PLACE, ONE_DEATH);
        assert_eq!(registry.get(PLACE), ONE_DEATH + ONE_DEATH);
        assert_eq!(registry.get(ELSEWHERE), NO_BELIEF);
    }

    /// The seam cannot draw a place down, and a zero accrual mints no entry.
    #[test]
    fn a_non_positive_amount_is_ignored() {
        let mut registry = BeliefRegistry::default();
        registry.add(PLACE, NO_BELIEF);
        assert!(registry.is_empty(), "a zero accrual must not mint an entry");
        registry.add(PLACE, ONE_DEATH);
        registry.add(PLACE, -ONE_DEATH);
        assert_eq!(registry.get(PLACE), ONE_DEATH);
    }

    #[test]
    fn fractional_deaths_accrue_exactly() {
        let mut registry = BeliefRegistry::default();
        let config = BeliefConfig::default();
        for _ in 0..3 {
            registry.credit_deaths(PLACE, A_THIRD_OF_A_DEATH, &config);
        }
        assert!((registry.get(PLACE) - ONE_DEATH).abs() < TOLERANCE);
    }

    #[test]
    fn belief_per_death_scales_the_deaths_source() {
        /// A retuned lever, distinct from the shipped `1.0`.
        const TRIPLED: f32 = 3.0;
        let mut registry = BeliefRegistry::default();
        let config = BeliefConfig {
            belief_per_death: TRIPLED,
            ..BeliefConfig::default()
        };
        registry.credit_deaths(PLACE, ONE_DEATH, &config);
        assert_eq!(registry.get(PLACE), TRIPLED);
    }

    #[test]
    fn iteration_is_row_major() {
        let mut registry = BeliefRegistry::default();
        registry.add(UVec2::new(1, 2), ONE_DEATH);
        registry.add(UVec2::new(5, 0), ONE_DEATH);
        registry.add(UVec2::new(0, 2), ONE_DEATH);
        let order: Vec<UVec2> = registry.iter().map(|(tile, _)| tile).collect();
        assert_eq!(
            order,
            vec![UVec2::new(5, 0), UVec2::new(0, 2), UVec2::new(1, 2)]
        );
    }
}
