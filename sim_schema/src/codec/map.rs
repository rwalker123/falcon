//! Map-section FlatBuffers serialization.

use crate::codec::{create_float_raster, FbBuilder};
use crate::state::map::{
    ClimateBandsState, ElevationOverlayState, MountainKind, TerrainOverlayState, TerrainType,
    TileState,
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
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: Some(terrain_overlay),
            elevationOverlay: Some(elevation_overlay),
            moistureRaster: Some(moisture_raster),
            removedTiles: None,
            climateBands: Some(climate_bands),
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
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: terrain_overlay,
            elevationOverlay: elevation_overlay,
            moistureRaster: moisture_raster,
            removedTiles: Some(removed_tiles),
            climateBands: climate_bands,
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
                    mass: tile.mass,
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
