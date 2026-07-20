//! Climate bands — **the** answer to "how cold is this tile?".
//!
//! `docs/plan_climate_authority.md` §4: *temperature is the climate authority. A biome's climate
//! eligibility is a derived function of the tile's temperature — never of its latitude.* Latitude
//! remains an input to temperature (`climate_temperature`, `systems/worldgen.rs`), where it
//! belongs; it stops being a second, parallel answer that can disagree with the first.
//!
//! This module exposes exactly one decision function, [`climate_band_for_temperature`], and it is
//! the **single seam**: no call site may re-derive a band or compare a raw temperature against a
//! literal. Before this arc there were three separate arithmetic copies of a latitude rule
//! (`terrain.rs`'s `polar_latitude_cutoff`, `systems/mod.rs`'s `POLAR_LATITUDE_THRESHOLD`, and
//! `worldgen.rs`'s `climate_band_for_position`) which could and did disagree.
//!
//! The gate reads the **jittered** temperature deliberately (design §8.2). Band boundaries come out
//! ragged because the previous latitude gate drew clean horizontal edges that read as artificial on
//! a real map. If the result is ever too noisy the lever is `climate.element_jitter_scale`, **not**
//! re-gating on an un-jittered temperature.

use crate::resources::ClimateConfig;

/// The climate ladder a tile's temperature places it on (design §8.1).
///
/// A ladder of four bands rather than a single "polar" cut point: the measured incoherence was not
/// confined to the polar edge (`BorealTaiga` was 1,601 of the 4,397 warm-polar tiles — a
/// *boreal-band* problem specifically), and one threshold cannot express that.
///
/// Ordered coldest → warmest, so `Ord` is the temperature ordering and `band <= ClimateBand::Boreal`
/// reads as "at least as cold as boreal".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClimateBand {
    Polar,
    Boreal,
    Temperate,
    Tropical,
}

impl ClimateBand {
    /// The stable lowercase key used by preset `climate_band_weights` and by the snapshot.
    pub fn as_str(self) -> &'static str {
        match self {
            ClimateBand::Polar => "polar",
            ClimateBand::Boreal => "boreal",
            ClimateBand::Temperate => "temperate",
            ClimateBand::Tropical => "tropical",
        }
    }

    /// Whether this band admits the cold (POLAR-tagged) biome ladder.
    ///
    /// Polar and boreal are both cold enough for the cold ladder — `BorealTaiga`, `Tundra` and
    /// `PeatHeath` are boreal-to-polar biomes, not exclusively polar ones. This is the predicate
    /// that replaces every former `is_polar_lat` / `dist_from_equator >= polar_latitude_cutoff`
    /// test, and the one `BiomePalette::remap` is keyed on.
    pub fn admits_cold_biomes(self) -> bool {
        matches!(self, ClimateBand::Polar | ClimateBand::Boreal)
    }
}

/// The single seam: which climate band a tile's temperature places it in.
///
/// Cut points are config levers in the `climate` block of `simulation_config.json` and are
/// inclusive upper bounds — a tile at exactly `polar_max_temp` is polar. See [`ClimateConfig`] for
/// the shipped defaults and their justification.
pub fn climate_band_for_temperature(temperature: f32, climate: &ClimateConfig) -> ClimateBand {
    if temperature <= climate.polar_max_temp {
        ClimateBand::Polar
    } else if temperature <= climate.boreal_max_temp {
        ClimateBand::Boreal
    } else if temperature <= climate.temperate_max_temp {
        ClimateBand::Temperate
    } else {
        ClimateBand::Tropical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ClimateConfig {
        ClimateConfig {
            equator_temp: 30.0,
            polar_temp: -5.0,
            elevation_lapse_span: 12.0,
            element_jitter_scale: 0.25,
            polar_max_temp: 0.0,
            boreal_max_temp: 5.0,
            temperate_max_temp: 18.0,
        }
    }

    #[test]
    fn ladder_is_monotonic_in_temperature() {
        let cfg = cfg();
        let mut previous = ClimateBand::Polar;
        let mut temperature = -30.0f32;
        while temperature <= 40.0 {
            let band = climate_band_for_temperature(temperature, &cfg);
            assert!(
                band >= previous,
                "band went backwards at {temperature}: {previous:?} -> {band:?}"
            );
            previous = band;
            temperature += 0.1;
        }
    }

    #[test]
    fn cut_points_are_inclusive_upper_bounds() {
        let cfg = cfg();
        assert_eq!(
            climate_band_for_temperature(cfg.polar_max_temp, &cfg),
            ClimateBand::Polar
        );
        assert_eq!(
            climate_band_for_temperature(cfg.boreal_max_temp, &cfg),
            ClimateBand::Boreal
        );
        assert_eq!(
            climate_band_for_temperature(cfg.temperate_max_temp, &cfg),
            ClimateBand::Temperate
        );
    }

    #[test]
    fn every_band_is_reachable_and_named() {
        let cfg = cfg();
        let samples = [
            (-20.0, ClimateBand::Polar, "polar"),
            (2.5, ClimateBand::Boreal, "boreal"),
            (12.0, ClimateBand::Temperate, "temperate"),
            (28.0, ClimateBand::Tropical, "tropical"),
        ];
        for (temperature, expected, name) in samples {
            let band = climate_band_for_temperature(temperature, &cfg);
            assert_eq!(band, expected, "{temperature} should be {expected:?}");
            assert_eq!(band.as_str(), name);
        }
    }

    #[test]
    fn only_polar_and_boreal_admit_cold_biomes() {
        assert!(ClimateBand::Polar.admits_cold_biomes());
        assert!(ClimateBand::Boreal.admits_cold_biomes());
        assert!(!ClimateBand::Temperate.admits_cold_biomes());
        assert!(!ClimateBand::Tropical.admits_cold_biomes());
    }
}
