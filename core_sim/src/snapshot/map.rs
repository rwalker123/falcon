use super::*;

pub(crate) fn terrain_overlay_from_tiles(
    tiles: &[TileState],
    grid_size: UVec2,
) -> TerrainOverlayState {
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }
    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![TerrainSample::default(); total];
    for tile in tiles {
        if tile.x >= width || tile.y >= height {
            continue;
        }
        let idx = (tile.y as usize) * (width as usize) + tile.x as usize;
        if idx < samples.len() {
            samples[idx] = TerrainSample {
                terrain: tile.terrain,
                tags: tile.terrain_tags,
                mountain_kind: tile.mountain_kind,
                relief_scale: tile.mountain_relief,
            };
        }
    }
    TerrainOverlayState {
        width,
        height,
        samples,
    }
}

pub(crate) fn elevation_overlay_from_field(
    field: &ElevationField,
    grid_size: UVec2,
) -> ElevationOverlayState {
    let width = grid_size.x.max(field.width).max(1);
    let height = grid_size.y.max(field.height).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![0u16; total];

    let mut min_value = f32::MAX;
    let mut max_value = f32::MIN;
    let max_y = field.height.min(height);
    let max_x = field.width.min(width);
    for y in 0..max_y {
        for x in 0..max_x {
            let value = field.sample(x, y);
            min_value = min_value.min(value);
            max_value = max_value.max(value);
        }
    }
    if min_value >= max_value {
        max_value = min_value + f32::EPSILON;
    }
    let range = (max_value - min_value).max(f32::EPSILON);

    for y in 0..max_y {
        for x in 0..max_x {
            let idx = (y as usize) * (width as usize) + x as usize;
            if idx >= samples.len() {
                continue;
            }
            let value = field.sample(x, y);
            let normalised = ((value - min_value) / range).clamp(0.0, 1.0);
            samples[idx] = (normalised * 65535.0).round().clamp(0.0, 65535.0) as u16;
        }
    }

    // Express sea level on the same [min_value, max_value] scale as the samples so the
    // client can compare it directly against decoded (0..1) samples for its
    // relative-height / LOS readout.
    let sea_level = ((field.sea_level - min_value) / range).clamp(0.0, 1.0);

    ElevationOverlayState {
        width,
        height,
        samples,
        min_value,
        max_value,
        sea_level,
    }
}

pub(crate) fn moisture_overlay_from_resource(
    moisture: Option<&MoistureRaster>,
    grid_size: UVec2,
) -> FloatRasterState {
    if let Some(raster) = moisture {
        if raster.width == grid_size.x && raster.height == grid_size.y {
            return raster.as_state();
        }
    }
    FloatRasterState::default()
}

pub(crate) fn tile_state(
    entity: Entity,
    tile: &Tile,
    morale_pressure_cfg: &MoralePressureConfig,
    graze: Option<&GrazePatch>,
    forage: &ForageLaborConfig,
) -> TileState {
    let (mountain_kind, mountain_relief) = match tile.mountain {
        Some(meta) => (map_mountain_kind(meta.kind), meta.relief),
        None => (MountainKind::None, 1.0),
    };
    // Band-independent tile harshness — the same `tile_morale_pressure` the sim applies to morale.
    let habitability = tile_morale_pressure(
        &terrain_definition(tile.terrain),
        tile.temperature,
        morale_pressure_cfg,
    )
    .total()
    .raw();
    TileState {
        entity: entity.to_bits(),
        x: tile.position.x,
        y: tile.position.y,
        element: u8::from(tile.element),
        mass: tile.mass.raw(),
        temperature: tile.temperature.raw(),
        terrain: tile.terrain,
        terrain_tags: tile.terrain_tags,
        culture_layer: 0,
        mountain_kind,
        mountain_relief,
        habitability,
        // The pasture readout (Phase 2a). A tile with no patch is a biome that carries no graze at
        // all, and reads a stated zero + `GRAZE_PHASE_NONE` — never a "healthy" default.
        graze_biomass: graze.map(|patch| patch.biomass).unwrap_or_default(),
        graze_capacity: graze
            .map(|patch| patch.carrying_capacity)
            .unwrap_or_default(),
        graze_ecology_phase: graze_phase_code(graze),
        // FORAGE POTENTIAL — the human-edible twin of `graze_capacity`. Read straight from the biome
        // table for EVERY tile, NOT from the sparse `ForageRegistry`, so the potential shows on the
        // ~95% of tiles that carry no patch (all the best cropland). On a food-module tile that DOES
        // hold a `ForagePatch`, that patch was seeded at this same value (the SHARED helper), so this
        // equals the patch's `carrying_capacity` — no drift between potential and realized. Non-zero
        // on fishery water (shelf/coral/inland sea); a NavigableRiver reads its underlying biome plus
        // the river fishing bonus; only a stated-zero biome reads 0.
        forage_capacity: crate::forage::tile_forage_capacity(forage, tile),
        // The tile's REAL ground for resource reads — its own `terrain` everywhere, the underlying
        // valley on a NavigableRiver. The client consults this only when `terrain == NavigableRiver`
        // (elsewhere it equals `terrain`), so it is always the meaningful "real ground" biome.
        underlying_terrain: tile.resource_terrain(),
        river_edges: tile.river_edges,
        river_inflow: tile.river_inflow,
        river_channel: tile.river_channel,
    }
}

pub(crate) fn map_mountain_kind(kind: MountainType) -> MountainKind {
    match kind {
        MountainType::Fold => MountainKind::Fold,
        MountainType::Fault => MountainKind::Fault,
        MountainType::Volcanic => MountainKind::Volcanic,
        MountainType::Dome => MountainKind::Dome,
    }
}

pub(crate) fn mountain_metadata_from_state(
    kind: MountainKind,
    relief: f32,
) -> Option<MountainMetadata> {
    match kind {
        MountainKind::None => None,
        MountainKind::Fold => Some(MountainMetadata {
            kind: MountainType::Fold,
            relief,
        }),
        MountainKind::Fault => Some(MountainMetadata {
            kind: MountainType::Fault,
            relief,
        }),
        MountainKind::Volcanic => Some(MountainMetadata {
            kind: MountainType::Volcanic,
            relief,
        }),
        MountainKind::Dome => Some(MountainMetadata {
            kind: MountainType::Dome,
            relief,
        }),
    }
}
