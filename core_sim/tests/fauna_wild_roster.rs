//! The four **hunt-only** roster additions — Wild Elk, Alpine Ibex, Desert Gazelle, Forest Grouse —
//! and the abundance row that makes the northern one reachable at all.
//!
//! Short-range game only appears where BOTH halves agree: the species lists the biome in
//! `host_biomes` (`FaunaConfig::game_species_for_biome`) **and** `abundance.per_biome` has a positive
//! weight for it (`spawn_short_range_game` rolls that weight per tile). Either half alone spawns
//! nothing, and neither half failing is loud — which is exactly the defect these tests pin:
//! `boreal_arctic` had no abundance row, so the north could only ever hold the two migratory herds
//! no matter what any species claimed as a host.

use core_sim::{FaunaConfig, HusbandryCeiling};

/// Each addition with the biome it is meant to make huntable.
const WILD_ADDITIONS: [(&str, &str); 4] = [
    ("wild_elk", "boreal_arctic"),
    ("alpine_ibex", "montane_highland"),
    ("gazelle", "semi_arid_scrub"),
    ("forest_grouse", "mixed_woodland"),
];

/// The shipped roster loads (validation runs inside `from_json_str`, so a bad row would panic in
/// `builtin()` rather than be silently swapped out), and each addition is a non-migratory,
/// `wild`-ceiling species that short-range spawning can actually pick for its host biome.
#[test]
fn wild_additions_are_huntable_in_their_host_biomes() {
    let config = FaunaConfig::builtin();

    for (key, biome) in WILD_ADDITIONS {
        let def = config
            .species
            .get(key)
            .unwrap_or_else(|| panic!("{key} should be on the shipped roster"));
        assert!(
            !def.migratory,
            "{key} is short-range game, not a herd route"
        );
        assert_eq!(
            def.husbandry_ceiling,
            HusbandryCeiling::Wild,
            "{key} is hunt-only"
        );
        // A `wild` ceiling never tames or herds, so the roster states no husbandry dials for it and
        // the inert defaults are read — the same ones the existing wild exemplar (`deer`) reads.
        let wild_exemplar = &config.species["deer"];
        assert_eq!(
            def.animals_per_herder, wild_exemplar.animals_per_herder,
            "{key} should omit animals_per_herder"
        );
        assert_eq!(
            def.taming_rate, wild_exemplar.taming_rate,
            "{key} should omit taming_rate"
        );

        let candidates = config.game_species_for_biome(biome);
        assert!(
            candidates.iter().any(|(name, _)| name.as_str() == key),
            "{key} should be a spawn candidate in {biome}"
        );
        assert!(
            config.abundance.probability_for(biome) > 0.0,
            "{biome} needs a positive abundance row or {key} can never spawn"
        );
    }
}

/// Regression guard for the empty-north defect. `boreal_arctic` was absent from `abundance.per_biome`,
/// which reads as `0.0` — no short-range game anywhere in the north. Removing this row again would
/// make Wild Elk unspawnable with nothing else failing.
#[test]
fn boreal_arctic_has_a_positive_abundance_row() {
    let config = FaunaConfig::builtin();
    assert!(
        config.abundance.probability_for("boreal_arctic") > 0.0,
        "boreal_arctic must carry a positive spawn weight"
    );
}
