//! Map-section FlatBuffers serialization.

use crate::codec::{
    create_float_raster, decode_float_raster, decode_rows, decode_scalars, unknown_enum,
    DecodeError, FbBuilder,
};
use crate::state::map::{
    ClimateBandsState, ElevationOverlayState, MountainKind, TemperatureSurvivabilityState,
    TerrainOverlayState, TerrainSample, TerrainTags, TerrainType, TileState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_map_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::MapSection<'a>> {
    let tiles = create_tiles(builder, &snapshot.tiles);
    let terrain_overlay = create_terrain_overlay(builder, &snapshot.terrain);
    let elevation_overlay = create_elevation_overlay(builder, &snapshot.elevation_overlay);
    let moisture_raster = create_float_raster(builder, &snapshot.moisture_raster);
    let climate_bands = create_climate_bands(builder, &snapshot.climate_bands);
    let temperature_survivability =
        create_temperature_survivability(builder, &snapshot.temperature_survivability);
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: Some(terrain_overlay),
            elevationOverlay: Some(elevation_overlay),
            moistureRaster: Some(moisture_raster),
            removedTiles: None,
            climateBands: Some(climate_bands),
            temperatureSurvivability: Some(temperature_survivability),
        },
    )
}

pub(crate) fn serialize_map_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::MapSection<'a>> {
    let tiles = create_tiles(builder, &delta.tiles);
    let removed_tiles = builder.create_vector(&delta.removed_tiles);
    let terrain_overlay = delta
        .terrain
        .as_ref()
        .map(|overlay| create_terrain_overlay(builder, overlay));
    let elevation_overlay = delta
        .elevation_overlay
        .as_ref()
        .map(|overlay| create_elevation_overlay(builder, overlay));
    let moisture_raster = delta
        .moisture_raster
        .as_ref()
        .map(|raster| create_float_raster(builder, raster));
    let climate_bands = delta
        .climate_bands
        .as_ref()
        .map(|bands| create_climate_bands(builder, bands));
    let temperature_survivability = delta
        .temperature_survivability
        .as_ref()
        .map(|model| create_temperature_survivability(builder, model));
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: terrain_overlay,
            elevationOverlay: elevation_overlay,
            moistureRaster: moisture_raster,
            removedTiles: Some(removed_tiles),
            climateBands: climate_bands,
            temperatureSurvivability: temperature_survivability,
        },
    )
}

fn create_elevation_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &ElevationOverlayState,
) -> WIPOffset<fb::ElevationOverlay<'a>> {
    let samples_vec = builder.create_vector(&overlay.samples);
    fb::ElevationOverlay::create(
        builder,
        &fb::ElevationOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            minValue: overlay.min_value,
            maxValue: overlay.max_value,
            samples: Some(samples_vec),
            seaLevel: overlay.sea_level,
        },
    )
}

fn create_climate_bands<'a>(
    builder: &mut FbBuilder<'a>,
    bands: &ClimateBandsState,
) -> WIPOffset<fb::ClimateBands<'a>> {
    fb::ClimateBands::create(
        builder,
        &fb::ClimateBandsArgs {
            polarMaxTemp: bands.polar_max_temp,
            borealMaxTemp: bands.boreal_max_temp,
            temperateMaxTemp: bands.temperate_max_temp,
        },
    )
}

fn create_temperature_survivability<'a>(
    builder: &mut FbBuilder<'a>,
    model: &TemperatureSurvivabilityState,
) -> WIPOffset<fb::TemperatureSurvivability<'a>> {
    fb::TemperatureSurvivability::create(
        builder,
        &fb::TemperatureSurvivabilityArgs {
            coldOnsetTemp: model.cold_onset_temp,
            coldMortalityScale: model.cold_mortality_scale,
            coldMaxMortality: model.cold_max_mortality,
            heatOnsetTemp: model.heat_onset_temp,
            heatMortalityScale: model.heat_mortality_scale,
            heatMaxMortality: model.heat_max_mortality,
        },
    )
}

fn create_tiles<'a>(
    builder: &mut FbBuilder<'a>,
    tiles: &[TileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TileState<'a>>>> {
    let offsets: Vec<_> = tiles
        .iter()
        .map(|tile| {
            fb::TileState::create(
                builder,
                &fb::TileStateArgs {
                    entity: tile.entity,
                    x: tile.x,
                    y: tile.y,
                    element: tile.element,
                    temperature: tile.temperature,
                    terrain: to_fb_terrain_type(tile.terrain),
                    terrainTags: tile.terrain_tags.bits(),
                    cultureLayer: tile.culture_layer,
                    mountainKind: to_fb_mountain_kind(tile.mountain_kind),
                    mountainRelief: tile.mountain_relief,
                    habitability: tile.habitability,
                    grazeBiomass: tile.graze_biomass,
                    grazeCapacity: tile.graze_capacity,
                    grazeEcologyPhase: tile.graze_ecology_phase,
                    forageCapacity: tile.forage_capacity,
                    underlyingTerrain: to_fb_terrain_type(tile.underlying_terrain),
                    riverEdges: tile.river_edges,
                    riverInflow: tile.river_inflow,
                    riverChannel: tile.river_channel,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_terrain_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &TerrainOverlayState,
) -> WIPOffset<fb::TerrainOverlay<'a>> {
    let sample_offsets: Vec<_> = overlay
        .samples
        .iter()
        .map(|sample| {
            fb::TerrainSample::create(
                builder,
                &fb::TerrainSampleArgs {
                    terrain: to_fb_terrain_type(sample.terrain),
                    tags: sample.tags.bits(),
                    mountainKind: to_fb_mountain_kind(sample.mountain_kind),
                    reliefScale: sample.relief_scale,
                },
            )
        })
        .collect();
    let samples = builder.create_vector(&sample_offsets);
    fb::TerrainOverlay::create(
        builder,
        &fb::TerrainOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            samples: Some(samples),
        },
    )
}

fn to_fb_terrain_type(terrain: TerrainType) -> fb::TerrainType {
    match terrain {
        TerrainType::DeepOcean => fb::TerrainType::DeepOcean,
        TerrainType::ContinentalShelf => fb::TerrainType::ContinentalShelf,
        TerrainType::InlandSea => fb::TerrainType::InlandSea,
        TerrainType::CoralShelf => fb::TerrainType::CoralShelf,
        TerrainType::HydrothermalVentField => fb::TerrainType::HydrothermalVentField,
        TerrainType::TidalFlat => fb::TerrainType::TidalFlat,
        TerrainType::RiverDelta => fb::TerrainType::RiverDelta,
        TerrainType::MangroveSwamp => fb::TerrainType::MangroveSwamp,
        TerrainType::FreshwaterMarsh => fb::TerrainType::FreshwaterMarsh,
        TerrainType::Floodplain => fb::TerrainType::Floodplain,
        TerrainType::AlluvialPlain => fb::TerrainType::AlluvialPlain,
        TerrainType::PrairieSteppe => fb::TerrainType::PrairieSteppe,
        TerrainType::MixedWoodland => fb::TerrainType::MixedWoodland,
        TerrainType::BorealTaiga => fb::TerrainType::BorealTaiga,
        TerrainType::PeatHeath => fb::TerrainType::PeatHeath,
        TerrainType::HotDesertErg => fb::TerrainType::HotDesertErg,
        TerrainType::RockyReg => fb::TerrainType::RockyReg,
        TerrainType::SemiAridScrub => fb::TerrainType::SemiAridScrub,
        TerrainType::SaltFlat => fb::TerrainType::SaltFlat,
        TerrainType::OasisBasin => fb::TerrainType::OasisBasin,
        TerrainType::Tundra => fb::TerrainType::Tundra,
        TerrainType::PeriglacialSteppe => fb::TerrainType::PeriglacialSteppe,
        TerrainType::Glacier => fb::TerrainType::Glacier,
        TerrainType::SeasonalSnowfield => fb::TerrainType::SeasonalSnowfield,
        TerrainType::RollingHills => fb::TerrainType::RollingHills,
        TerrainType::HighPlateau => fb::TerrainType::HighPlateau,
        TerrainType::AlpineMountain => fb::TerrainType::AlpineMountain,
        TerrainType::KarstHighland => fb::TerrainType::KarstHighland,
        TerrainType::CanyonBadlands => fb::TerrainType::CanyonBadlands,
        TerrainType::ActiveVolcanoSlope => fb::TerrainType::ActiveVolcanoSlope,
        TerrainType::BasalticLavaField => fb::TerrainType::BasalticLavaField,
        TerrainType::AshPlain => fb::TerrainType::AshPlain,
        TerrainType::FumaroleBasin => fb::TerrainType::FumaroleBasin,
        TerrainType::ImpactCraterField => fb::TerrainType::ImpactCraterField,
        TerrainType::KarstCavernMouth => fb::TerrainType::KarstCavernMouth,
        TerrainType::SinkholeField => fb::TerrainType::SinkholeField,
        TerrainType::AquiferCeiling => fb::TerrainType::AquiferCeiling,
        TerrainType::NavigableRiver => fb::TerrainType::NavigableRiver,
    }
}

fn to_fb_mountain_kind(kind: MountainKind) -> fb::MountainKind {
    match kind {
        MountainKind::None => fb::MountainKind::None,
        MountainKind::Fold => fb::MountainKind::Fold,
        MountainKind::Fault => fb::MountainKind::Fault,
        MountainKind::Volcanic => fb::MountainKind::Volcanic,
        MountainKind::Dome => fb::MountainKind::Dome,
    }
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` / `to_fb_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_map_section(
    section: fb::MapSection<'_>,
    snapshot: &mut WorldSnapshot,
) -> Result<(), DecodeError> {
    snapshot.tiles = decode_rows(section.tiles(), decode_tile)?;
    snapshot.terrain = section
        .terrainOverlay()
        .map(decode_terrain_overlay)
        .transpose()?
        .unwrap_or_default();
    snapshot.elevation_overlay = section
        .elevationOverlay()
        .map(decode_elevation_overlay)
        .unwrap_or_default();
    snapshot.moisture_raster = section
        .moistureRaster()
        .map(decode_float_raster)
        .unwrap_or_default();
    snapshot.climate_bands = section
        .climateBands()
        .map(decode_climate_bands)
        .unwrap_or_default();
    snapshot.temperature_survivability = section
        .temperatureSurvivability()
        .map(decode_temperature_survivability)
        .unwrap_or_default();
    Ok(())
}

pub(crate) fn decode_map_section_delta(
    section: fb::MapSection<'_>,
    delta: &mut WorldDelta,
) -> Result<(), DecodeError> {
    delta.tiles = decode_rows(section.tiles(), decode_tile)?;
    delta.removed_tiles = decode_scalars(section.removedTiles());
    delta.terrain = section
        .terrainOverlay()
        .map(decode_terrain_overlay)
        .transpose()?;
    delta.elevation_overlay = section.elevationOverlay().map(decode_elevation_overlay);
    delta.moisture_raster = section.moistureRaster().map(decode_float_raster);
    delta.climate_bands = section.climateBands().map(decode_climate_bands);
    delta.temperature_survivability = section
        .temperatureSurvivability()
        .map(decode_temperature_survivability);
    Ok(())
}

fn decode_elevation_overlay(overlay: fb::ElevationOverlay<'_>) -> ElevationOverlayState {
    ElevationOverlayState {
        width: overlay.width(),
        height: overlay.height(),
        min_value: overlay.minValue(),
        max_value: overlay.maxValue(),
        samples: decode_scalars(overlay.samples()),
        sea_level: overlay.seaLevel(),
    }
}

fn decode_climate_bands(bands: fb::ClimateBands<'_>) -> ClimateBandsState {
    ClimateBandsState {
        polar_max_temp: bands.polarMaxTemp(),
        boreal_max_temp: bands.borealMaxTemp(),
        temperate_max_temp: bands.temperateMaxTemp(),
    }
}

fn decode_temperature_survivability(
    model: fb::TemperatureSurvivability<'_>,
) -> TemperatureSurvivabilityState {
    TemperatureSurvivabilityState {
        cold_onset_temp: model.coldOnsetTemp(),
        cold_mortality_scale: model.coldMortalityScale(),
        cold_max_mortality: model.coldMaxMortality(),
        heat_onset_temp: model.heatOnsetTemp(),
        heat_mortality_scale: model.heatMortalityScale(),
        heat_max_mortality: model.heatMaxMortality(),
    }
}

fn decode_tile(tile: fb::TileState<'_>) -> Result<TileState, DecodeError> {
    Ok(TileState {
        entity: tile.entity(),
        x: tile.x(),
        y: tile.y(),
        element: tile.element(),
        temperature: tile.temperature(),
        terrain: to_state_terrain_type(tile.terrain())?,
        terrain_tags: TerrainTags::new(tile.terrainTags()),
        culture_layer: tile.cultureLayer(),
        mountain_kind: to_state_mountain_kind(tile.mountainKind())?,
        mountain_relief: tile.mountainRelief(),
        habitability: tile.habitability(),
        river_edges: tile.riverEdges(),
        river_inflow: tile.riverInflow(),
        river_channel: tile.riverChannel(),
        graze_biomass: tile.grazeBiomass(),
        graze_capacity: tile.grazeCapacity(),
        graze_ecology_phase: tile.grazeEcologyPhase(),
        forage_capacity: tile.forageCapacity(),
        underlying_terrain: to_state_terrain_type(tile.underlyingTerrain())?,
    })
}

fn decode_terrain_overlay(
    overlay: fb::TerrainOverlay<'_>,
) -> Result<TerrainOverlayState, DecodeError> {
    Ok(TerrainOverlayState {
        width: overlay.width(),
        height: overlay.height(),
        samples: decode_rows(overlay.samples(), |sample| {
            Ok(TerrainSample {
                terrain: to_state_terrain_type(sample.terrain())?,
                tags: TerrainTags::new(sample.tags()),
                mountain_kind: to_state_mountain_kind(sample.mountainKind())?,
                relief_scale: sample.reliefScale(),
            })
        })?,
    })
}

fn to_state_terrain_type(terrain: fb::TerrainType) -> Result<TerrainType, DecodeError> {
    Ok(match terrain {
        fb::TerrainType::DeepOcean => TerrainType::DeepOcean,
        fb::TerrainType::ContinentalShelf => TerrainType::ContinentalShelf,
        fb::TerrainType::InlandSea => TerrainType::InlandSea,
        fb::TerrainType::CoralShelf => TerrainType::CoralShelf,
        fb::TerrainType::HydrothermalVentField => TerrainType::HydrothermalVentField,
        fb::TerrainType::TidalFlat => TerrainType::TidalFlat,
        fb::TerrainType::RiverDelta => TerrainType::RiverDelta,
        fb::TerrainType::MangroveSwamp => TerrainType::MangroveSwamp,
        fb::TerrainType::FreshwaterMarsh => TerrainType::FreshwaterMarsh,
        fb::TerrainType::Floodplain => TerrainType::Floodplain,
        fb::TerrainType::AlluvialPlain => TerrainType::AlluvialPlain,
        fb::TerrainType::PrairieSteppe => TerrainType::PrairieSteppe,
        fb::TerrainType::MixedWoodland => TerrainType::MixedWoodland,
        fb::TerrainType::BorealTaiga => TerrainType::BorealTaiga,
        fb::TerrainType::PeatHeath => TerrainType::PeatHeath,
        fb::TerrainType::HotDesertErg => TerrainType::HotDesertErg,
        fb::TerrainType::RockyReg => TerrainType::RockyReg,
        fb::TerrainType::SemiAridScrub => TerrainType::SemiAridScrub,
        fb::TerrainType::SaltFlat => TerrainType::SaltFlat,
        fb::TerrainType::OasisBasin => TerrainType::OasisBasin,
        fb::TerrainType::Tundra => TerrainType::Tundra,
        fb::TerrainType::PeriglacialSteppe => TerrainType::PeriglacialSteppe,
        fb::TerrainType::Glacier => TerrainType::Glacier,
        fb::TerrainType::SeasonalSnowfield => TerrainType::SeasonalSnowfield,
        fb::TerrainType::RollingHills => TerrainType::RollingHills,
        fb::TerrainType::HighPlateau => TerrainType::HighPlateau,
        fb::TerrainType::AlpineMountain => TerrainType::AlpineMountain,
        fb::TerrainType::KarstHighland => TerrainType::KarstHighland,
        fb::TerrainType::CanyonBadlands => TerrainType::CanyonBadlands,
        fb::TerrainType::ActiveVolcanoSlope => TerrainType::ActiveVolcanoSlope,
        fb::TerrainType::BasalticLavaField => TerrainType::BasalticLavaField,
        fb::TerrainType::AshPlain => TerrainType::AshPlain,
        fb::TerrainType::FumaroleBasin => TerrainType::FumaroleBasin,
        fb::TerrainType::ImpactCraterField => TerrainType::ImpactCraterField,
        fb::TerrainType::KarstCavernMouth => TerrainType::KarstCavernMouth,
        fb::TerrainType::SinkholeField => TerrainType::SinkholeField,
        fb::TerrainType::AquiferCeiling => TerrainType::AquiferCeiling,
        fb::TerrainType::NavigableRiver => TerrainType::NavigableRiver,
        other => return Err(unknown_enum("TerrainType", other.0)),
    })
}

fn to_state_mountain_kind(kind: fb::MountainKind) -> Result<MountainKind, DecodeError> {
    Ok(match kind {
        fb::MountainKind::None => MountainKind::None,
        fb::MountainKind::Fold => MountainKind::Fold,
        fb::MountainKind::Fault => MountainKind::Fault,
        fb::MountainKind::Volcanic => MountainKind::Volcanic,
        fb::MountainKind::Dome => MountainKind::Dome,
        other => return Err(unknown_enum("MountainKind", other.0)),
    })
}

#[cfg(test)]
mod enum_round_trip_tests {
    use super::*;

    /// Every variant the encoder can write, the decoder reads back as the same variant — an
    /// enum's arms are the one thing the saturated fixture cannot cover past the default.
    #[test]
    fn every_terrain_type_and_mountain_kind_round_trips() {
        for terrain in TerrainType::VALUES {
            assert_eq!(
                to_state_terrain_type(to_fb_terrain_type(terrain)).expect("a known variant"),
                terrain
            );
        }
        for kind in [
            MountainKind::None,
            MountainKind::Fold,
            MountainKind::Fault,
            MountainKind::Volcanic,
            MountainKind::Dome,
        ] {
            assert_eq!(
                to_state_mountain_kind(to_fb_mountain_kind(kind)).expect("a known variant"),
                kind
            );
        }
    }

    /// …and a discriminant no build has ever written is an ERROR naming the field, never a
    /// silent default.
    #[test]
    fn an_unknown_discriminant_is_an_error_naming_the_field() {
        const UNASSIGNED_TERRAIN: u16 = u16::MAX;
        match to_state_terrain_type(fb::TerrainType(UNASSIGNED_TERRAIN)) {
            Err(DecodeError::UnknownEnum { field, value }) => {
                assert_eq!(field, "TerrainType");
                assert_eq!(value, i64::from(UNASSIGNED_TERRAIN));
            }
            other => panic!("expected UnknownEnum, got {other:?}"),
        }
    }
}
