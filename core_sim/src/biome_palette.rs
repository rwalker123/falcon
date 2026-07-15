//! Per-map biome palette (`docs/plan_biome_palette.md`).
//!
//! At world-gen time each map draws a curated, seed-driven, map-size-scaled subset of
//! the 37 biomes — few on a small map (legible), many on a large one (rich) — while the
//! full library is preserved for replay variety. The palette is a hard *allow-set* per
//! [`BiomeNiche`] plus a niche-nearest [`remap`](BiomePalette::remap) that pulls any
//! off-palette biome back to an allowed member of the same niche. It is built once in
//! `spawn_initial_world`, enforced at the `bias_terrain_for_preset` seam, and reconciled
//! with the tag-budget solver (force-included locked-tag fallbacks + a post-solver clamp).

use std::collections::HashSet;

use bevy::prelude::Resource;
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use sim_runtime::TerrainType;

use crate::{
    map_preset::MapPreset,
    terrain::{biome_must_have, biome_niche, terrain_definition, BiomeNiche},
};

/// XOR sub-seed salt for the palette selection RNG (`world_seed ^ PALETTE_SEED_SALT`),
/// following the repo's domain-subseed convention (cf. `HERD_MOVEMENT_SEED_SALT`,
/// `SITE_PLACEMENT_SEED_SALT`). A fixed, arbitrary non-zero constant so palette selection
/// is decorrelated from every other seeded worldgen domain yet reproducible per map.
pub const PALETTE_SEED_SALT: u64 = 0xB105_11E0_5A1E_77E5;

/// The chosen per-map biome palette — a hard allow-set of biomes. Membership per niche is
/// recovered on demand via [`biome_niche`]. Stored as a Bevy resource so the post-solver
/// clamp system can enforce it.
#[derive(Resource, Debug, Clone)]
pub struct BiomePalette {
    allowed: HashSet<TerrainType>,
}

impl BiomePalette {
    /// Build the palette for a preset at a given resolved world seed and map area.
    /// Deterministic from `world_seed ^ PALETTE_SEED_SALT` (§3.4 / §5).
    pub fn build(preset: &MapPreset, world_seed: u64, tile_count: u32) -> Self {
        let mut rng = SmallRng::seed_from_u64(world_seed ^ PALETTE_SEED_SALT);
        let mut allowed: HashSet<TerrainType> = HashSet::new();

        for niche in BiomeNiche::ALL {
            // Conservative spanned-niche handling (§3.4 step 1 / §10): treat every niche
            // as spanned and let the per-niche reachable count clamp K. `reachable_count`
            // is approximated by the niche's full membership — refine only if a niche is
            // ever wrongly starved.
            let members: Vec<TerrainType> = TerrainType::VALUES
                .into_iter()
                .filter(|&t| biome_niche(t) == niche)
                .collect();
            let must_haves: Vec<TerrainType> = members
                .iter()
                .copied()
                .filter(|&t| biome_must_have(t))
                .collect();

            let reachable = members.len() as u32;
            let must_count = must_haves.len() as u32;
            let k = compute_k(preset, niche, tile_count).clamp(must_count, reachable);

            let mut chosen: Vec<TerrainType> = must_haves.clone();
            // Seed-sample the remainder from the non-must-have candidates up to K.
            let mut sampleable: Vec<TerrainType> = members
                .iter()
                .copied()
                .filter(|t| !must_haves.contains(t))
                .collect();
            sampleable.shuffle(&mut rng);
            let extra = (k as usize).saturating_sub(chosen.len());
            chosen.extend(sampleable.into_iter().take(extra));

            for t in chosen {
                allowed.insert(t);
            }
        }

        // §6 #1 — force-include the solver's locked-tag fallback biomes so the tag-budget
        // solver can never reintroduce an off-palette biome by construction.
        for tag in &preset.locked_terrain_tags {
            for &fallback in solver_locked_tag_fallbacks(tag) {
                allowed.insert(fallback);
            }
        }

        Self { allowed }
    }

    /// Whether a biome is on the palette.
    pub fn contains(&self, biome: TerrainType) -> bool {
        self.allowed.contains(&biome)
    }

    /// The number of distinct biomes on the palette (diagnostics/tests).
    pub fn distinct_count(&self) -> usize {
        self.allowed.len()
    }

    /// Map an off-palette biome to the nearest allowed biome in its niche, falling back
    /// across niches when a niche is empty (§3.4 step 4 / §10). On-palette biomes pass
    /// through unchanged.
    ///
    /// `is_polar` keeps the remap **climate-safe**: for the latitude-sensitive lowland
    /// niches a polar tile is only remapped to a POLAR-tagged member (e.g. a polar
    /// wetland collapses to PeatHeath, never temperate FreshwaterMarsh), and if its niche
    /// has no allowed polar member it crosses to the polar-lowland anchor (Tundra) — so
    /// the palette can never stamp temperate plains/marshes at the poles (guarding
    /// `polar_latitudes_avoid_alluvial_plain_regression`). Ocean/Highland/Volcanic/Anomaly
    /// are climate-neutral and ignore `is_polar`.
    pub fn remap(&self, biome: TerrainType, is_polar: bool) -> TerrainType {
        if self.contains(biome) {
            return biome;
        }
        let niche = biome_niche(biome);
        if climate_sensitive(niche) {
            // Prefer a same-niche member whose polar-ness matches the tile.
            if let Some(t) = self.first_allowed_matching(niche, is_polar) {
                return t;
            }
            if is_polar {
                // No polar-compatible wetland/fertile/arid member on-palette: fall to the
                // polar-lowland anchor rather than a temperate member of this niche.
                if let Some(t) = self.first_allowed_matching(BiomeNiche::PolarLowland, true) {
                    return t;
                }
                if let Some(t) = self.first_allowed_in_niche(BiomeNiche::PolarLowland) {
                    return t;
                }
            }
        }
        if let Some(t) = self.first_allowed_in_niche(niche) {
            return t;
        }
        // Niche has no allowed members (only possible for niches with zero must-haves and
        // K == 0, e.g. Volcanic on a small map): hop to the nearest kindred niche.
        if let Some(t) = self.first_allowed_in_niche(cross_niche_fallback(niche)) {
            return t;
        }
        // Terminal anchor: FertileLowland always carries AlluvialPlain (a must-have).
        self.first_allowed_in_niche(BiomeNiche::FertileLowland)
            .unwrap_or(TerrainType::AlluvialPlain)
    }

    /// The first allowed biome in a niche following that niche's canonical remap
    /// priority ordering (most-generic anchor first).
    fn first_allowed_in_niche(&self, niche: BiomeNiche) -> Option<TerrainType> {
        niche_remap_priority(niche)
            .iter()
            .copied()
            .find(|&t| self.contains(t))
    }

    /// The first allowed niche member whose POLAR tag matches `is_polar`.
    fn first_allowed_matching(&self, niche: BiomeNiche, is_polar: bool) -> Option<TerrainType> {
        niche_remap_priority(niche)
            .iter()
            .copied()
            .find(|&t| self.contains(t) && biome_is_polar(t) == is_polar)
    }
}

/// Whether a niche's remap must respect the tile's polar-ness (the lowland niches whose
/// members span temperate↔polar climate). Ocean/Highland/Volcanic/Anomaly are treated as
/// climate-neutral so a polar ocean tile never remaps onto land, etc.
fn climate_sensitive(niche: BiomeNiche) -> bool {
    matches!(
        niche,
        BiomeNiche::CoastWetland
            | BiomeNiche::FertileLowland
            | BiomeNiche::AridLowland
            | BiomeNiche::PolarLowland
    )
}

fn biome_is_polar(biome: TerrainType) -> bool {
    terrain_definition(biome)
        .tags
        .contains(sim_runtime::TerrainTags::POLAR)
}

/// The per-niche `K` from the size-interpolated distinct-biome budget (§3.3). Config
/// driven; returns the *unclamped* K (the caller clamps to `[must_have, reachable]`).
fn compute_k(preset: &MapPreset, niche: BiomeNiche, tile_count: u32) -> u32 {
    let cfg = &preset.biome_palette;
    let nk = cfg.niches.get(niche.as_str()).copied().unwrap_or_default();
    let small = cfg.small_map_tiles as f32;
    let large = cfg.large_map_tiles as f32;
    let span = (large - small).max(1.0);
    let area_t = ((tile_count as f32 - small) / span).clamp(0.0, 1.0);
    let t = smoothstep(area_t);
    let k = lerp(nk.k_small as f32, nk.k_large as f32, t);
    k.round().max(0.0) as u32
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The kindred niche a niche remaps into when it has no allowed members. Every chain
/// terminates at `FertileLowland`, which always carries the `AlluvialPlain` must-have.
fn cross_niche_fallback(niche: BiomeNiche) -> BiomeNiche {
    match niche {
        // Volcanic slopes read as rugged highland when volcanism is off-palette.
        BiomeNiche::Volcanic => BiomeNiche::Highland,
        // Everything else collapses onto the fertile lowland spine.
        _ => BiomeNiche::FertileLowland,
    }
}

/// Canonical remap-target priority for a niche (§10). Most-generic anchor first.
/// **`RiverDelta` and `NavigableRiver` are deliberately excluded** as remap targets: both are
/// stamped only by the hydrology pass (river mouths / the navigable tail of a big river), so
/// remapping arbitrary wetland or ocean tiles onto them would scatter deltas and inland waterways
/// with no river attached. Both are `must_have`, so the *real* ones are always on-palette and pass
/// through `remap` unchanged.
fn niche_remap_priority(niche: BiomeNiche) -> &'static [TerrainType] {
    use TerrainType::*;
    match niche {
        BiomeNiche::Ocean => &[
            DeepOcean,
            ContinentalShelf,
            InlandSea,
            CoralShelf,
            HydrothermalVentField,
        ],
        BiomeNiche::CoastWetland => &[FreshwaterMarsh, TidalFlat, MangroveSwamp, PeatHeath],
        BiomeNiche::FertileLowland => &[AlluvialPlain, PrairieSteppe, Floodplain, MixedWoodland],
        BiomeNiche::AridLowland => &[
            SemiAridScrub,
            RockyReg,
            HotDesertErg,
            OasisBasin,
            SaltFlat,
            AshPlain,
        ],
        BiomeNiche::PolarLowland => &[
            Tundra,
            PeriglacialSteppe,
            BorealTaiga,
            SeasonalSnowfield,
            Glacier,
        ],
        BiomeNiche::Highland => &[
            RollingHills,
            HighPlateau,
            AlpineMountain,
            KarstHighland,
            CanyonBadlands,
        ],
        BiomeNiche::Volcanic => &[ActiveVolcanoSlope, BasalticLavaField, FumaroleBasin],
        BiomeNiche::Anomaly => &[
            KarstCavernMouth,
            ImpactCraterField,
            SinkholeField,
            AquiferCeiling,
        ],
    }
}

/// The biome(s) `apply_tag_budget_solver` may *stamp* to satisfy a locked tag family
/// (§6 #1). Force-including these at palette-build guarantees the solver's coverage
/// passes only ever produce on-palette biomes. Mirrors the solver's per-tag replacement
/// vocabulary in `systems.rs`.
fn solver_locked_tag_fallbacks(tag: &str) -> &'static [TerrainType] {
    use TerrainType::*;
    match tag {
        "Water" => &[DeepOcean],
        "Wetland" => &[FreshwaterMarsh, PeatHeath],
        "Fertile" => &[Floodplain, AlluvialPlain],
        "Coastal" => &[TidalFlat],
        "Highland" => &[RollingHills],
        "Polar" => &[SeasonalSnowfield, Tundra],
        "Arid" => &[HotDesertErg, SemiAridScrub, RockyReg],
        "Volcanic" => &[ActiveVolcanoSlope],
        "Hazardous" => &[ImpactCraterField],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_preset::MapPresets;

    fn earthlike() -> MapPreset {
        MapPresets::builtin()
            .get("earthlike")
            .expect("earthlike preset")
            .clone()
    }

    // The selectable map presets (client `MapPanel.gd`), used as the representative sizes
    // in palette tests: Tiny 56x36, Standard 80x52, Huge 128x80.
    const TINY_TILES: u32 = 56 * 36; // 2016 — anchors k_small
    const STANDARD_TILES: u32 = 80 * 52; // 4160 — partway up the smoothstep curve
    const HUGE_TILES: u32 = 128 * 80; // 10240 — anchors k_large

    #[test]
    fn small_map_has_fewer_biomes_than_large_map() {
        let preset = earthlike();
        let small = BiomePalette::build(&preset, 42, TINY_TILES);
        let large = BiomePalette::build(&preset, 42, HUGE_TILES);
        assert!(
            small.distinct_count() < large.distinct_count(),
            "small={} large={}",
            small.distinct_count(),
            large.distinct_count()
        );
    }

    #[test]
    fn distinct_count_is_monotonic_across_selectable_sizes() {
        // Regression guard for the size-curve calibration: the palette's distinct-biome
        // count must not shrink as the map grows across the real selectable presets
        // Tiny(2016) <= Standard(4160) <= Huge(10240) at a fixed seed.
        let preset = earthlike();
        let seed = 42;
        let tiny = BiomePalette::build(&preset, seed, TINY_TILES).distinct_count();
        let standard = BiomePalette::build(&preset, seed, STANDARD_TILES).distinct_count();
        let huge = BiomePalette::build(&preset, seed, HUGE_TILES).distinct_count();
        assert!(
            tiny <= standard && standard <= huge,
            "expected Tiny({tiny}) <= Standard({standard}) <= Huge({huge})"
        );
        assert!(
            tiny < huge,
            "Tiny({tiny}) and Huge({huge}) should differ across the calibrated span"
        );
    }

    #[test]
    fn must_haves_and_locked_fallbacks_always_present() {
        let preset = earthlike();
        let palette = BiomePalette::build(&preset, 7, TINY_TILES);
        // must-have anchors
        for t in [
            TerrainType::DeepOcean,
            TerrainType::ContinentalShelf,
            TerrainType::InlandSea,
            TerrainType::AlluvialPlain,
            TerrainType::PrairieSteppe,
            TerrainType::Tundra,
            TerrainType::RiverDelta,
            TerrainType::Glacier,
            TerrainType::NavigableRiver,
        ] {
            assert!(palette.contains(t), "missing must-have {t:?}");
        }
        // earthlike locks Water/Fertile/Wetland → their solver fallbacks are forced in.
        for t in [
            TerrainType::DeepOcean,
            TerrainType::Floodplain,
            TerrainType::AlluvialPlain,
            TerrainType::FreshwaterMarsh,
            TerrainType::PeatHeath,
        ] {
            assert!(palette.contains(t), "missing locked-tag fallback {t:?}");
        }
    }

    #[test]
    fn inland_sea_is_always_in_palette_and_survives_remap() {
        // InlandSea is must-have (Ocean niche) so lakes/inland seas never remap to
        // DeepOcean at the bias seam. Check across seeds and both climate zones.
        let preset = earthlike();
        for seed in 0..32u64 {
            let palette = BiomePalette::build(&preset, seed, 20000);
            assert!(
                palette.contains(TerrainType::InlandSea),
                "seed {seed}: InlandSea missing from palette"
            );
            for is_polar in [false, true] {
                assert_eq!(
                    palette.remap(TerrainType::InlandSea, is_polar),
                    TerrainType::InlandSea,
                    "seed {seed}: InlandSea remapped away (polar={is_polar})"
                );
            }
        }
    }

    #[test]
    fn glacier_is_always_in_palette_and_survives_remap() {
        // Glacier is must-have (the extreme-relief member of PolarLowland), so a tall polar
        // peak stays ice instead of remapping down to flat Tundra. Check across seeds, map
        // sizes, and both climate zones.
        let preset = earthlike();
        for &tiles in &[TINY_TILES, STANDARD_TILES, HUGE_TILES] {
            for seed in 0..16u64 {
                let palette = BiomePalette::build(&preset, seed, tiles);
                assert!(
                    palette.contains(TerrainType::Glacier),
                    "seed {seed}, tiles {tiles}: Glacier missing from palette"
                );
                for is_polar in [false, true] {
                    assert_eq!(
                        palette.remap(TerrainType::Glacier, is_polar),
                        TerrainType::Glacier,
                        "seed {seed}, tiles {tiles}: Glacier remapped away (polar={is_polar})"
                    );
                }
            }
        }
    }

    #[test]
    fn highland_and_volcanic_niches_are_never_thinned() {
        // Highland + Volcanic are physically relief/elevation/mask-gated: every member is
        // kept always-available via full palette K (not `must_have`), so no palette swap can
        // stamp the wrong biome on a physically-specific tile. Assert every member of both
        // niches is on-palette across seeds and both small- and large-map sizes.
        let preset = earthlike();
        let highland_and_volcanic: Vec<TerrainType> = TerrainType::VALUES
            .into_iter()
            .filter(|&t| matches!(biome_niche(t), BiomeNiche::Highland | BiomeNiche::Volcanic))
            .collect();
        for &tiles in &[TINY_TILES, HUGE_TILES] {
            for seed in 0..16u64 {
                let palette = BiomePalette::build(&preset, seed, tiles);
                for member in &highland_and_volcanic {
                    assert!(
                        palette.contains(*member),
                        "seed {seed}, tiles {tiles}: physically-gated {member:?} thinned out"
                    );
                }
            }
        }
    }

    #[test]
    fn seeds_produce_different_biome_sets() {
        let preset = earthlike();
        let a = BiomePalette::build(&preset, 1, 20000);
        let b = BiomePalette::build(&preset, 2, 20000);
        assert_ne!(a.allowed, b.allowed);
    }

    #[test]
    fn remap_keeps_biomes_in_niche_and_never_targets_delta() {
        let preset = earthlike();
        let palette = BiomePalette::build(&preset, 99, TINY_TILES);
        for is_polar in [false, true] {
            for terrain in TerrainType::VALUES {
                let mapped = palette.remap(terrain, is_polar);
                assert!(palette.contains(mapped), "{terrain:?} remapped off-palette");
                // A non-delta biome must never be remapped onto RiverDelta.
                if terrain != TerrainType::RiverDelta {
                    assert_ne!(
                        mapped,
                        TerrainType::RiverDelta,
                        "{terrain:?} scattered onto a delta"
                    );
                }
            }
        }
    }

    #[test]
    fn polar_wetland_never_remaps_to_temperate_marsh() {
        // A polar tile whose wetland biome is off-palette must collapse to a POLAR-tagged
        // biome (PeatHeath / Tundra), never temperate FreshwaterMarsh or AlluvialPlain.
        let preset = earthlike();
        let palette = BiomePalette::build(&preset, 99, TINY_TILES);
        for probe in [
            TerrainType::TidalFlat,
            TerrainType::MangroveSwamp,
            TerrainType::CoralShelf,
        ] {
            if palette.contains(probe) {
                continue;
            }
            let mapped = palette.remap(probe, true);
            assert!(
                biome_is_polar(mapped) || biome_niche(mapped) == BiomeNiche::Ocean,
                "polar {probe:?} remapped to non-polar {mapped:?}"
            );
        }
    }

    #[test]
    fn large_map_spans_the_full_library_across_seeds() {
        // Across a spread of seeds a large map's palettes should, unioned, reach every
        // one of the 37 biomes (incl. the 3 revived) — nothing is structurally excluded.
        let preset = earthlike();
        let mut union: HashSet<TerrainType> = HashSet::new();
        for seed in 0..64u64 {
            let palette = BiomePalette::build(&preset, seed, HUGE_TILES);
            union.extend(palette.allowed.iter().copied());
        }
        assert_eq!(
            union.len(),
            TerrainType::VALUES.len(),
            "unreached biomes: {:?}",
            TerrainType::VALUES
                .into_iter()
                .filter(|t| !union.contains(t))
                .collect::<Vec<_>>()
        );
    }
}
