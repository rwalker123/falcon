//! The raster (grid-shaped) half of a snapshot: the per-overlay sample planes, the
//! grid dimensions they share, and the packing/normalization they all go through.

use godot::prelude::*;

pub(crate) fn packed_from_slice(values: &[f32]) -> PackedFloat32Array {
    if values.is_empty() {
        return PackedFloat32Array::new();
    }
    let mut array = PackedFloat32Array::new();
    array.resize(values.len());
    array.as_mut_slice().copy_from_slice(values);
    array
}

pub(crate) struct OverlayChannelParams<'a> {
    pub(crate) key: &'a str,
    pub(crate) label: &'a str,
    pub(crate) description: Option<&'a str>,
    pub(crate) normalized: &'a PackedFloat32Array,
    pub(crate) raw: &'a PackedFloat32Array,
    pub(crate) contrast: &'a PackedFloat32Array,
    pub(crate) placeholder: bool,
}

pub(crate) fn insert_overlay_channel(
    channels: &mut VarDictionary,
    order: &mut PackedStringArray,
    params: OverlayChannelParams<'_>,
) {
    let mut channel = VarDictionary::new();
    let _ = channel.insert("label", params.label);
    if let Some(description) = params.description {
        let _ = channel.insert("description", description);
    }
    let _ = channel.insert("normalized", &params.normalized.clone());
    let _ = channel.insert("raw", &params.raw.clone());
    let _ = channel.insert("contrast", &params.contrast.clone());
    if params.placeholder {
        let _ = channel.insert("placeholder", true);
    }
    let _ = channels.insert(params.key, &channel);
    let key_str = GString::from(params.key);
    order.push(&key_str);
}

/// What `fog_enabled` reads as when a payload carries no `VisionSection` at all. Mirrors the schema
/// default (`fogEnabled:bool = true`): fog of war is ON unless the sim says otherwise, so a missing
/// section can never be read as "the server disabled fog".
pub(crate) const FOG_ENABLED_WHEN_ABSENT: bool = true;

pub(crate) struct GridSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) wrap_horizontal: bool,
}

pub(crate) struct OverlaySlices<'a> {
    pub(crate) sentiment: &'a [f32],
    pub(crate) corruption: &'a [f32],
    pub(crate) culture: &'a [f32],
    pub(crate) military: &'a [f32],
    pub(crate) crisis: &'a [f32],
    pub(crate) elevation: &'a [f32],
    /// Sea level on the same normalized 0..1 scale as `elevation`, surfaced to the
    /// client as the `elevation_sea_level` scalar for its relative-height readout.
    pub(crate) elevation_sea_level: f32,
    /// The per-map climate-band cut points `[polarMaxTemp, borealMaxTemp, temperateMaxTemp]`
    /// (°C), published by the sim so the client renders the band it is TOLD rather than
    /// deciding one (the same reason `elevation_sea_level` rides the elevation overlay).
    /// `None` when the snapshot carries no `ClimateBands` table — the keys are then omitted
    /// entirely rather than published as a fabricated threshold, so the client can render the
    /// Climate line blank instead of inventing a cut point that could disagree with the sim.
    pub(crate) climate_bands: Option<[f32; 3]>,
    /// The temperature-mortality model's constants, in wire order: `[coldOnsetTemp,
    /// coldMortalityScale, coldMaxMortality, heatOnsetTemp, heatMortalityScale, heatMaxMortality]`
    /// (`MapSection.temperatureSurvivability`), published so the client can state the range the SIM
    /// kills outside of rather than inferring one from the climate bands — a DIFFERENT set of
    /// thresholds that happens to sit beside these on the wire (issue #614).
    ///
    /// **TWO INDEPENDENT TAILS**, not a tolerance around an ambient: each side has its own onset,
    /// slope and ceiling, because cold and heat are not symmetric phenomena. `None` when the
    /// snapshot carries no `TemperatureSurvivability` table, in which case the six keys are omitted
    /// together and the client renders no survivability readout at all, exactly as `climate_bands`
    /// leaves the Climate line blank.
    pub(crate) temperature_survivability: Option<[f32; 6]>,
    pub(crate) moisture: &'a [f32],
    pub(crate) visibility: &'a [f32],
    /// The server-owned fog-of-war switch (`VisionSection.fogEnabled`), surfaced as the top-level
    /// `fog_enabled` scalar. The client renders the state it is TOLD rather than holding its own
    /// flag — with fog off the sim stops dropping unseen herds from the payload, which is something
    /// no client-side toggle could reproduce. Same reasoning as `elevation_sea_level` riding here.
    pub(crate) fog_enabled: bool,
    /// The GRAZE (pasture) layer's per-tile CAPACITY — how much pasture this tile's biome can
    /// carry, in graze-biomass units (`0` = this biome carries no pasture at all). Unlike every
    /// other slice here it is not a wire raster: graze rides `TileState`, so this is assembled
    /// from the tiles rather than read off a `ScalarRaster`. Empty
    /// when the snapshot carries no graze — the channel is then omitted rather than published as
    /// a map-wide field of zeros, which would read as "nowhere has any pasture".
    pub(crate) pasture_capacity: &'a [f32],
    /// The FORAGE (human food) layer's per-tile CAPACITY — the human-edible POTENTIAL of this
    /// tile's biome (`TileState.forageCapacity`), the exact twin of `pasture_capacity`: every tile
    /// carries a value from its biome table, assembled from the tiles. `0` = genuinely no human
    /// food (deep ocean, glacier, lava). UNLIKE pasture, WATER is not uniformly barren — coastal
    /// shelves carry real fishing potential and sit ON the capacity ramp; only genuinely-zero tiles
    /// are the off-ramp barren fill (see MapView `_forage_color`).
    pub(crate) forage_capacity: &'a [f32],
}

pub(crate) struct TerrainSlices<'a> {
    pub(crate) terrain: Option<&'a [u16]>,
    pub(crate) tags: Option<&'a [u16]>,
}

pub(crate) fn normalize_overlay(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &v in values.iter() {
        if !v.is_finite() {
            continue;
        }
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if !min.is_finite() || !max.is_finite() || (max - min).abs() < f32::EPSILON {
        values.fill(0.0);
        return;
    }
    let range = max - min;
    for v in values.iter_mut() {
        if v.is_finite() {
            *v = ((*v - min) / range).clamp(0.0, 1.0);
        } else {
            *v = 0.0;
        }
    }
}
