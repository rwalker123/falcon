//! Vision-section FlatBuffers serialization (the per-overlay rasters).

use crate::codec::{create_scalar_raster, FbBuilder};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_vision_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::VisionSection<'a>> {
    let visibility_raster = create_scalar_raster(builder, &snapshot.visibility_raster);
    let military_raster = create_scalar_raster(builder, &snapshot.military_raster);
    fb::VisionSection::create(
        builder,
        &fb::VisionSectionArgs {
            visibilityRaster: Some(visibility_raster),
            militaryRaster: Some(military_raster),
            fogEnabled: snapshot.fog_enabled,
        },
    )
}

pub(crate) fn serialize_vision_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::VisionSection<'a>> {
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
            visibilityRaster: visibility_raster,
            militaryRaster: military_raster,
            fogEnabled: delta.fog_enabled,
        },
    )
}
