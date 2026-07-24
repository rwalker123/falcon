//! **Per-tile realization** (Flora Roster §10, `docs/plan_flora_roster.md`).
//!
//! `FloraConfig::composition(terrain)` is the **affinity** roster — *what CAN grow here* — uniform
//! across a biome. `realized_composition(terrain, tile, map_seed)` is the per-tile **realization** —
//! *what IS growing here* — a seeded weighted subset, so two tiles of one biome carry different
//! baskets. These tests pin the invariants that make that safe: per-tile neutrality (realization
//! moves nothing at the wild rung), real per-tile variance, a map-wide mix that still tracks affinity,
//! and bit-exact determinism.

use bevy::math::UVec2;
use core_sim::{FloraConfig, LaborConfig, BUILTIN_LABOR_CONFIG};
use sim_runtime::TerrainType;

/// f32 slack on a sum of a handful of normalized shares.
const SHARE_EPSILON: f32 = 1e-5;
/// Relative slack for `Σ share × capacity` against the capacity itself.
const CAPACITY_RELATIVE_EPSILON: f32 = 1e-5;
/// A pinned seed for the sweeps.
const SEED: u64 = 0x_F10A_5EED_2EA1_0010;
/// Side of the per-biome tile grid a sweep samples.
const SIDE: u32 = 12;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

fn sweep_tiles() -> impl Iterator<Item = UVec2> {
    (0..SIDE).flat_map(|x| (0..SIDE).map(move |y| UVec2::new(x, y)))
}

/// **Realization moves nothing at the wild rung.** A tile's realized shares renormalize to sum to
/// `1`, so `Σ share × capacity == capacity` on every tile — the tile still yields its full biome
/// capacity gathered wild, just composed of different species. The economy-neutrality that F1 proved
/// for the uniform basket, now per tile.
#[test]
fn realized_shares_sum_to_one_on_every_tile() {
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;

    for terrain in TerrainType::VALUES {
        let capacity = forage.capacity_for(terrain);
        for coord in sweep_tiles() {
            let realized = flora.realized_composition(terrain, coord, SEED);
            if realized.is_empty() {
                // A biome no species hosts realizes to nothing — an *absent* reading, not a zero one.
                continue;
            }
            let total: f32 = realized.iter().map(|s| s.share).sum();
            assert!(
                (total - 1.0).abs() <= SHARE_EPSILON,
                "{terrain:?} @ {coord:?}: realized shares sum to {total}, not 1.0"
            );
            let decomposed: f32 = realized.iter().map(|s| s.share * capacity).sum();
            assert!(
                (decomposed - capacity).abs() <= CAPACITY_RELATIVE_EPSILON * capacity.max(1.0),
                "{terrain:?} @ {coord:?}: Σ share × capacity = {decomposed}, not {capacity}"
            );
        }
    }
}

/// **A navigable hex realizes too, and still names its fishery.** The underlying valley's basket is
/// realized per tile, the channel term (`river_fish`) is blended in un-realized, and the result still
/// sums to `1` — so the two-term decomposition stays total on a realized navigable tile.
#[test]
fn a_navigable_hex_realizes_and_still_sums_to_one() {
    let flora = FloraConfig::builtin();
    let forage = &labor().forage;

    for underlying in [
        TerrainType::AlluvialPlain,
        TerrainType::Floodplain,
        TerrainType::RollingHills,
    ] {
        let mut saw_channel = false;
        for coord in sweep_tiles() {
            let realized = flora.realized_navigable_composition(underlying, forage, coord, SEED);
            let total: f32 = realized.iter().map(|s| s.share).sum();
            assert!(
                (total - 1.0).abs() <= SHARE_EPSILON,
                "navigable/{underlying:?} @ {coord:?}: realized shares sum to {total}"
            );
            saw_channel |= realized.iter().any(|s| s.species == "river_fish");
        }
        assert!(
            saw_channel,
            "navigable/{underlying:?}: the fishery (river_fish) must survive realization on some tile"
        );
    }
}

/// **The whole point: two tiles of one biome carry DIFFERENT baskets.** On a biome that hosts several
/// crops, realization spreads them — some tiles wheat, some tobacco — instead of every tile carrying
/// the same diluted slice of all of them.
#[test]
fn two_tiles_of_one_biome_realize_different_baskets() {
    let flora = FloraConfig::builtin();

    // AlluvialPlain hosts six species (wheat/tubers/hay/cotton/tobacco/flax) — plenty to differ over.
    let terrain = TerrainType::AlluvialPlain;
    let baskets: Vec<Vec<String>> = sweep_tiles()
        .map(|coord| {
            let mut keys: Vec<String> = flora
                .realized_composition(terrain, coord, SEED)
                .into_iter()
                .map(|s| s.species)
                .collect();
            keys.sort();
            keys
        })
        .collect();
    let distinct: std::collections::BTreeSet<&Vec<String>> = baskets.iter().collect();
    assert!(
        distinct.len() > 1,
        "{terrain:?}: realization must produce more than one distinct basket across its tiles \
         (got {} tiles, all identical)",
        baskets.len()
    );
}

/// **The realized species COUNT stays inside the config band**, clamped to what the biome hosts. A
/// tile can never realize zero species (nameless food) nor more than the dial's max.
#[test]
fn the_realized_count_respects_the_config_band() {
    let flora = FloraConfig::builtin();
    let lo = flora.realized_species_min;
    let hi = flora.realized_species_max;

    for terrain in TerrainType::VALUES {
        let hosted = flora.composition(terrain).len();
        if hosted == 0 {
            continue;
        }
        let expected_lo = lo.min(hosted).max(1);
        let expected_hi = hi.min(hosted);
        for coord in sweep_tiles() {
            let count = flora.realized_composition(terrain, coord, SEED).len();
            assert!(
                count >= expected_lo && count <= expected_hi,
                "{terrain:?} @ {coord:?}: realized {count} species, expected \
                 [{expected_lo}, {expected_hi}] (hosts {hosted})"
            );
        }
    }
}

/// **The map-wide mix still tracks affinity.** Weighted sampling is ~unbiased, so averaged over a
/// biome's tiles the *dominant* affinity species is still the most-present and the *least* affine is
/// the least-present — the biome-wide species mix is unmoved even though no single tile carries the
/// uniform basket. A measured/ordering test (the exact per-species mean drifts with renormalization),
/// not a bit-exact one.
#[test]
fn the_map_wide_aggregate_tracks_the_affinity_ordering() {
    let flora = FloraConfig::builtin();
    // A big single-biome sample so the aggregate is stable.
    let terrain = TerrainType::AlluvialPlain;
    let affinity = flora.composition(terrain);
    let side = 40u32;

    // Mean realized share per species over the sample.
    let mut sums: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut tiles = 0u32;
    for x in 0..side {
        for y in 0..side {
            for s in flora.realized_composition(terrain, UVec2::new(x, y), SEED) {
                *sums.entry(s.species).or_insert(0.0) += s.share as f64;
            }
            tiles += 1;
        }
    }
    let mean = |species: &str| sums.get(species).copied().unwrap_or(0.0) / tiles as f64;

    // The highest-affinity species must have the highest mean realized presence, and the lowest the
    // lowest — the aggregate preserves affinity's ordering.
    let top = &affinity[0].species; // affinity is sorted share DESC
    let bottom = &affinity[affinity.len() - 1].species;
    let top_mean = mean(top);
    let bottom_mean = mean(bottom);
    for entry in affinity {
        let m = mean(&entry.species);
        assert!(
            top_mean >= m - 1e-9,
            "{terrain:?}: `{top}` (top affinity) must be the most-present in the aggregate, but \
             `{}` beats it ({m} > {top_mean})",
            entry.species
        );
        assert!(
            bottom_mean <= m + 1e-9,
            "{terrain:?}: `{bottom}` (bottom affinity) must be the least-present, but `{}` is lower \
             ({m} < {bottom_mean})",
            entry.species
        );
    }
}

/// **Deterministic — bit-identical across two independent builds of the config.** Realization is a
/// pure function of `(map_seed, tile, terrain, affinities)`, so two `FloraConfig::builtin()` instances
/// realize the same tile identically. This is the realization analog of the share table's
/// `the_share_table_is_bit_identical_across_builds` — the snapshot-hash-stability guard §10 needs.
#[test]
fn realization_is_bit_identical_across_two_builds() {
    let a = FloraConfig::builtin();
    let b = FloraConfig::builtin();

    for terrain in TerrainType::VALUES {
        for coord in sweep_tiles() {
            let ra = a.realized_composition(terrain, coord, SEED);
            let rb = b.realized_composition(terrain, coord, SEED);
            assert_eq!(
                ra.len(),
                rb.len(),
                "{terrain:?} @ {coord:?}: differing realized species counts across builds"
            );
            for (x, y) in ra.iter().zip(rb.iter()) {
                assert_eq!(
                    x.species, y.species,
                    "{terrain:?} @ {coord:?}: species differ"
                );
                // Bit-exact: the shares must have identical bit patterns, not merely be close.
                assert_eq!(
                    x.share.to_bits(),
                    y.share.to_bits(),
                    "{terrain:?} @ {coord:?}: `{}` share differs bitwise across builds",
                    x.species
                );
            }
        }
    }
}

/// **A one-species biome realizes to itself**, byte-identical — there is nothing to subset, so the
/// realization is exactly the affinity basket (§10's `hosted <= 1` shortcut). `NavigableRiver` hosts
/// only `river_fish`.
#[test]
fn a_one_species_biome_realizes_to_itself() {
    let flora = FloraConfig::builtin();
    let affinity = flora.composition(TerrainType::NavigableRiver);
    assert_eq!(affinity.len(), 1, "NavigableRiver hosts river_fish alone");
    for coord in sweep_tiles() {
        let realized = flora.realized_composition(TerrainType::NavigableRiver, coord, SEED);
        assert_eq!(
            realized.as_slice(),
            affinity,
            "a one-species biome realizes to itself @ {coord:?}"
        );
    }
}
