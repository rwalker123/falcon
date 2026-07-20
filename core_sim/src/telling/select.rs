//! Weighted wardrobe selection: `weight = fit × novelty × stance_affinity`
//! (`docs/plan_the_telling.md` §2c).
//!
//! **Determinism.** The RNG is seeded **per decision** from
//! `world_seed ^ tick ^ TELLING_SEED_SALT ^ FnvHasher(beat.id)` — never a rolling stream — so a
//! selection is reproducible *and* independent of beat evaluation order. Adding a beat to the
//! catalog cannot perturb an unrelated beat's roll (`fauna.rs`'s per-herd recipe).

use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
};

use rand::{rngs::SmallRng, Rng, SeedableRng};
use sim_schema::TerrainType;

use crate::hashing::FnvHasher;

use super::{
    catalog::{BeatDefinition, WardrobeEntry},
    config::SelectionConfig,
    nouns::{terrain_has_biome_tag, Noun},
    stance,
};

/// Salt distinguishing narrative selection from every other seeded stream in the sim.
pub const TELLING_SEED_SALT: u64 = 0x7E11_1146_BEA7_5A17;

/// A wardrobe entry that survived fit + novelty, with the weight it draws at.
#[derive(Debug, Clone)]
pub struct WeightedEntry<'a> {
    pub entry: &'a WardrobeEntry,
    pub weight: f32,
}

/// How well an entry fits the world right now.
///
/// `0.0` excludes it: a `requires_noun` slot is unresolved, or the entry is biome-gated and the
/// band's ground carries none of its tags. Otherwise the base fit is `1.0`, plus
/// `fit_soft_tag_weight` per matched biome tag — a line written *for* this ground beats a
/// generic one on it.
pub fn fit_weight(
    entry: &WardrobeEntry,
    resolved: &BTreeMap<String, Noun>,
    terrain: Option<TerrainType>,
    cfg: &SelectionConfig,
) -> f32 {
    for slot in &entry.fit.requires_noun {
        if !resolved.contains_key(slot) {
            return 0.0;
        }
    }
    if entry.fit.biome.is_empty() {
        return 1.0;
    }
    let matched = entry
        .fit
        .biome
        .iter()
        .filter(|tag| terrain_has_biome_tag(terrain, tag))
        .count();
    if matched == 0 {
        return 0.0;
    }
    1.0 + cfg.fit_soft_tag_weight * matched as f32
}

/// Novelty: `1.0` if never used, else ramping linearly from `novelty_floor` back to `1.0` over
/// `novelty_window_turns` since the entry was last used.
pub fn novelty_weight(last_used_tick: Option<u64>, tick: u64, cfg: &SelectionConfig) -> f32 {
    let Some(last_used) = last_used_tick else {
        return 1.0;
    };
    let elapsed = tick.saturating_sub(last_used) as f32;
    // `novelty_window_turns > 0` is a validated config invariant, so this cannot divide by zero.
    let progress = (elapsed / cfg.novelty_window_turns as f32).clamp(0.0, 1.0);
    cfg.novelty_floor + (1.0 - cfg.novelty_floor) * progress
}

/// Weigh every wardrobe entry of `beat`, dropping those below `min_selection_weight`.
pub fn weigh_wardrobe<'a>(
    beat: &'a BeatDefinition,
    resolved: &BTreeMap<String, Noun>,
    terrain: Option<TerrainType>,
    wardrobe_usage: &BTreeMap<String, u64>,
    tick: u64,
    effective_stance: &BTreeMap<String, f32>,
    cfg: &SelectionConfig,
) -> Vec<WeightedEntry<'a>> {
    beat.wardrobe
        .iter()
        .filter_map(|entry| {
            let fit = fit_weight(entry, resolved, terrain, cfg);
            if fit <= 0.0 {
                return None;
            }
            let novelty = novelty_weight(wardrobe_usage.get(&entry.id).copied(), tick, cfg);
            let weight = fit * novelty * stance::affinity_term(entry, effective_stance, cfg);
            (weight >= cfg.min_selection_weight).then_some(WeightedEntry { entry, weight })
        })
        .collect()
}

/// The per-decision RNG seed. Hashed on the **beat id** so adding a beat to the catalog cannot
/// perturb another beat's roll.
pub fn decision_seed(world_seed: u64, tick: u64, beat_id: &str) -> u64 {
    let mut hasher = FnvHasher::new();
    beat_id.hash(&mut hasher);
    world_seed ^ tick ^ TELLING_SEED_SALT ^ hasher.finish()
}

/// Weighted-choose one entry with a per-decision seeded RNG. `None` when nothing survived
/// weighing — the beat then silently does not emit and **must not be marked fired**.
pub fn select_wardrobe<'a>(
    candidates: &[WeightedEntry<'a>],
    world_seed: u64,
    tick: u64,
    beat_id: &str,
) -> Option<&'a WardrobeEntry> {
    let total: f32 = candidates.iter().map(|c| c.weight).sum();
    if candidates.is_empty() || total <= 0.0 || !total.is_finite() {
        return None;
    }
    let mut rng = SmallRng::seed_from_u64(decision_seed(world_seed, tick, beat_id));
    let mut roll = rng.gen::<f32>() * total;
    for candidate in candidates {
        roll -= candidate.weight;
        if roll <= 0.0 {
            return Some(candidate.entry);
        }
    }
    // Float drift on the final subtraction: fall back to the last candidate, never `None`.
    candidates.last().map(|c| c.entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telling::catalog::{BeatCatalog, BeatTier, Fit};
    use crate::telling::config::BeatConfig;

    fn entry(id: &str) -> WardrobeEntry {
        WardrobeEntry {
            id: id.to_string(),
            fit: Fit::default(),
            voice: BTreeMap::from([("mythic".to_string(), format!("line {id}"))]),
            stance_affinity: None,
        }
    }

    fn weighted(entries: &[WardrobeEntry]) -> Vec<WeightedEntry<'_>> {
        entries
            .iter()
            .map(|entry| WeightedEntry { entry, weight: 1.0 })
            .collect()
    }

    #[test]
    fn selection_is_deterministic_for_a_fixed_seed() {
        let entries = vec![entry("a"), entry("b"), entry("c"), entry("d")];
        let candidates = weighted(&entries);
        for tick in 0..25u64 {
            let first = select_wardrobe(&candidates, 4242, tick, "some.beat").map(|e| e.id.clone());
            let second =
                select_wardrobe(&candidates, 4242, tick, "some.beat").map(|e| e.id.clone());
            assert_eq!(first, second, "same seed must reproduce the same draw");
        }
    }

    /// The whole point of per-decision seeding: a beat's roll must not depend on what else is in
    /// the catalog. Adding an unrelated beat must leave every other beat's selection untouched.
    #[test]
    fn selection_is_stable_when_an_unrelated_beat_is_added_to_the_catalog() {
        let config = BeatConfig::builtin();
        let baseline = BeatCatalog::builtin();

        // Same catalog plus one extra beat appended (a rolling RNG stream would shift every
        // subsequent beat's draw; per-decision seeding must not).
        let mut json: serde_json::Value =
            serde_json::from_str(crate::telling::catalog::BUILTIN_BEAT_DEFINITIONS).unwrap();
        let mut extra = json[0].clone();
        extra["id"] = "test.unrelated_beat".into();
        json.as_array_mut().unwrap().insert(0, extra);
        let widened = BeatCatalog::from_json_str(&json.to_string(), &config).unwrap();

        for tick in 0..40u64 {
            for beat in baseline.beats() {
                let entries: Vec<WeightedEntry<'_>> = beat
                    .wardrobe
                    .iter()
                    .map(|entry| WeightedEntry { entry, weight: 1.0 })
                    .collect();
                let before = select_wardrobe(&entries, 99, tick, &beat.id).map(|e| e.id.clone());

                let widened_beat = widened.find(&beat.id).expect("beat still present");
                let widened_entries: Vec<WeightedEntry<'_>> = widened_beat
                    .wardrobe
                    .iter()
                    .map(|entry| WeightedEntry { entry, weight: 1.0 })
                    .collect();
                let after = select_wardrobe(&widened_entries, 99, tick, &widened_beat.id)
                    .map(|e| e.id.clone());

                assert_eq!(
                    before, after,
                    "beat {} drew differently after an unrelated beat was added",
                    beat.id
                );
            }
        }
        // Sanity: the widened catalog really is different.
        assert_eq!(widened.beats().len(), baseline.beats().len() + 1);
        assert!(matches!(baseline.beats()[0].tier, BeatTier::Beat));
    }

    #[test]
    fn novelty_decays_on_use_and_recovers_over_the_window() {
        let cfg = SelectionConfig::default();
        // Never used: full novelty.
        assert_eq!(novelty_weight(None, 10, &cfg), 1.0);
        // Just used: at the floor.
        assert!((novelty_weight(Some(10), 10, &cfg) - cfg.novelty_floor).abs() < 1e-6);
        // Halfway through the window: halfway back.
        let half = novelty_weight(Some(0), (cfg.novelty_window_turns / 2) as u64, &cfg);
        assert!(half > cfg.novelty_floor && half < 1.0, "{half}");
        // Past the window: fully recovered.
        let full = novelty_weight(Some(0), cfg.novelty_window_turns as u64 + 5, &cfg);
        assert!((full - 1.0).abs() < 1e-6, "{full}");
        // Monotone in between.
        let mut previous = 0.0;
        for elapsed in 0..=cfg.novelty_window_turns as u64 {
            let value = novelty_weight(Some(0), elapsed, &cfg);
            assert!(value >= previous, "novelty must not dip while recovering");
            previous = value;
        }
    }

    #[test]
    fn fit_excludes_entries_with_unresolved_required_nouns() {
        let cfg = SelectionConfig::default();
        let mut gated = entry("gated");
        gated.fit.requires_noun = vec!["beast".to_string()];
        let empty = BTreeMap::new();
        assert_eq!(fit_weight(&gated, &empty, None, &cfg), 0.0);

        let resolved = BTreeMap::from([("beast".to_string(), Noun::Scalar(1.0))]);
        assert_eq!(fit_weight(&gated, &resolved, None, &cfg), 1.0);
    }

    #[test]
    fn fit_hard_gates_on_biome_and_rewards_matches() {
        let cfg = SelectionConfig::default();
        let mut gated = entry("biome_gated");
        gated.fit.biome = vec!["alluvial".to_string(), "grassland".to_string()];
        let empty = BTreeMap::new();

        assert_eq!(
            fit_weight(&gated, &empty, Some(TerrainType::AlpineMountain), &cfg),
            0.0,
            "a mismatched biome excludes the entry"
        );
        assert_eq!(fit_weight(&gated, &empty, None, &cfg), 0.0);
        let matched = fit_weight(&gated, &empty, Some(TerrainType::AlluvialPlain), &cfg);
        assert!(
            matched > 1.0,
            "a matched biome tag outweighs a generic entry"
        );
    }

    #[test]
    fn weighing_drops_everything_when_a_required_noun_is_missing() {
        let config = BeatConfig::builtin();
        let catalog = BeatCatalog::builtin();
        // `discovery.site_found` requires the `place` slot on every wardrobe entry.
        let beat = catalog.find("discovery.site_found").expect("beat present");
        let candidates = weigh_wardrobe(
            beat,
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            0,
            &BTreeMap::new(),
            &config.selection,
        );
        assert!(
            candidates.is_empty(),
            "every entry requires an unresolved noun, so nothing may be selectable"
        );
        assert!(select_wardrobe(&candidates, 1, 0, &beat.id).is_none());
    }

    /// **The design claim of concept §6, made explicit**: the *same* beat, on the *same* trigger,
    /// reads with opposite valence depending on who the player has become. A roam-leaning stance
    /// must weigh "the chase thins" above "less reason to follow", and a settle-leaning one must
    /// weigh them the other way round.
    #[test]
    fn a_collapsing_herd_is_re_coloured_by_the_players_stance() {
        let config = BeatConfig::builtin();
        let catalog = BeatCatalog::builtin();
        let beat = catalog
            .find("ecology.herd_collapsing")
            .expect("beat present");
        let resolved = BTreeMap::from([(
            "beast".to_string(),
            Noun::named("Red Deer", "Red Deer", "deer"),
        )]);

        let weight_of = |stance_value: f32, entry_id: &str| {
            let stance = BTreeMap::from([("roam_settle".to_string(), stance_value)]);
            weigh_wardrobe(
                beat,
                &resolved,
                None,
                &BTreeMap::new(),
                0,
                &stance,
                &config.selection,
            )
            .into_iter()
            .find(|candidate| candidate.entry.id == entry_id)
            .map(|candidate| candidate.weight)
            .unwrap_or_else(|| panic!("{entry_id} should survive weighing"))
        };

        // Roam-leaning: the herd was the road, and the road is going quiet.
        assert!(
            weight_of(-1.0, "collapse.the_chase_thins")
                > weight_of(-1.0, "collapse.less_reason_to_follow")
        );
        // Settle-leaning: the same collapse is barely worth looking up for.
        assert!(
            weight_of(1.0, "collapse.less_reason_to_follow")
                > weight_of(1.0, "collapse.the_chase_thins")
        );
        // Uncoloured dressings of the same beat are untouched by stance.
        assert_eq!(
            weight_of(-1.0, "collapse.thin_season"),
            weight_of(1.0, "collapse.thin_season")
        );
    }

    #[test]
    fn weights_bias_the_draw() {
        let entries = [entry("heavy"), entry("light")];
        let candidates = vec![
            WeightedEntry {
                entry: &entries[0],
                weight: 100.0,
            },
            WeightedEntry {
                entry: &entries[1],
                weight: 0.01,
            },
        ];
        let heavy = (0..200u64)
            .filter(|tick| {
                select_wardrobe(&candidates, 7, *tick, "b").map(|e| e.id.as_str()) == Some("heavy")
            })
            .count();
        assert!(
            heavy > 180,
            "the heavy entry should dominate, got {heavy}/200"
        );
    }
}
