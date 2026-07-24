//! Vision-section FlatBuffers serialization (the per-overlay rasters).

use crate::codec::{create_scalar_raster, FbBuilder};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_vision_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::VisionSection<'a>> {
    let fog_raster = create_scalar_raster(builder, &snapshot.fog_raster);
    let visibility_raster = create_scalar_raster(builder, &snapshot.visibility_raster);
    let military_raster = create_scalar_raster(builder, &snapshot.military_raster);
    fb::VisionSection::create(
        builder,
        &fb::VisionSectionArgs {
            fogRaster: Some(fog_raster),
            visibilityRaster: Some(visibility_raster),
            militaryRaster: Some(military_raster),
        },
    )
}

pub(crate) fn serialize_vision_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::VisionSection<'a>> {
    let fog_raster = delta
        .fog_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let visibility_raster = delta
        .visibility_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let military_raster = delta
        .military_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    fb::VisionSection::create(
        builder,
        &fb::VisionSectionArgs {
            fogRaster: fog_raster,
            visibilityRaster: visibility_raster,
            militaryRaster: military_raster,
        },
    )
}
