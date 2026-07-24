//! `map` section -- terrain, tiles, and the pasture phase that rides them.

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use sim_runtime::TerrainType;

use crate::dict::fixed64_to_f64;

pub(crate) const TERRAIN_TAG_LABELS: &[(u16, &str)] = &[
    (1 << 0, "Water"),
    (1 << 1, "Freshwater"),
    (1 << 2, "Coastal"),
    (1 << 3, "Wetland"),
    (1 << 4, "Fertile"),
    (1 << 5, "Arid"),
    (1 << 6, "Polar"),
    (1 << 7, "Highland"),
    (1 << 8, "Volcanic"),
    (1 << 9, "Hazardous"),
    (1 << 10, "Subsurface"),
    (1 << 11, "Hydrothermal"),
];

pub(crate) fn terrain_label_from_id(id: u16) -> &'static str {
    // Resolve the id through the enum rather than matching the raw number, so the label table
    // below can be exhaustive. "Unknown" now means only one thing: an id this build has no
    // `TerrainType` for (a server newer than the client).
    match TerrainType::VALUES
        .iter()
        .copied()
        .find(|t| *t as u16 == id)
    {
        Some(terrain) => terrain_label(terrain),
        None => "Unknown",
    }
}

/// Display label for every terrain, used to build the snapshot's `terrain_palette` (which the
/// Terrain Types legend and the Inspector's Terrain tab both prefer over their local config).
///
/// **Exhaustive on purpose — do NOT add a `_` wildcard.** This table previously matched the raw
/// `u16` with a `_ => "Unknown"` catch-all, so adding `NavigableRiver` (id 37) silently rendered
/// it as "Unknown" in the legend instead of failing the build. Matching the enum with no wildcard
/// makes a new terrain a *compile error* here rather than a runtime typo in the UI.
fn terrain_label(terrain: TerrainType) -> &'static str {
    match terrain {
        TerrainType::DeepOcean => "Deep Ocean",
        TerrainType::ContinentalShelf => "Continental Shelf",
        TerrainType::InlandSea => "Lake",
        TerrainType::CoralShelf => "Coral Shelf",
        TerrainType::HydrothermalVentField => "Hydrothermal Vent Field",
        TerrainType::TidalFlat => "Tidal Flat",
        TerrainType::RiverDelta => "River Delta",
        TerrainType::MangroveSwamp => "Mangrove Swamp",
        TerrainType::FreshwaterMarsh => "Freshwater Marsh",
        TerrainType::Floodplain => "Floodplain",
        TerrainType::AlluvialPlain => "Alluvial Plain",
        TerrainType::PrairieSteppe => "Prairie Steppe",
        TerrainType::MixedWoodland => "Mixed Woodland",
        TerrainType::BorealTaiga => "Boreal Taiga",
        TerrainType::PeatHeath => "Peatland/Heath",
        TerrainType::HotDesertErg => "Hot Desert Erg",
        TerrainType::RockyReg => "Rocky Reg Desert",
        TerrainType::SemiAridScrub => "Semi-Arid Scrub",
        TerrainType::SaltFlat => "Salt Flat",
        TerrainType::OasisBasin => "Oasis Basin",
        TerrainType::Tundra => "Tundra",
        TerrainType::PeriglacialSteppe => "Periglacial Steppe",
        TerrainType::Glacier => "Glacier",
        TerrainType::SeasonalSnowfield => "Seasonal Snowfield",
        TerrainType::RollingHills => "Rolling Hills",
        TerrainType::HighPlateau => "High Plateau",
        TerrainType::AlpineMountain => "Alpine Mountain",
        TerrainType::KarstHighland => "Karst Highland",
        TerrainType::CanyonBadlands => "Canyon Badlands",
        TerrainType::ActiveVolcanoSlope => "Active Volcano Slope",
        TerrainType::BasalticLavaField => "Basaltic Lava Field",
        TerrainType::AshPlain => "Ash Plain",
        TerrainType::FumaroleBasin => "Fumarole Basin",
        TerrainType::ImpactCraterField => "Impact Crater Field",
        TerrainType::KarstCavernMouth => "Karst Cavern Mouth",
        TerrainType::SinkholeField => "Sinkhole Field",
        TerrainType::AquiferCeiling => "Aquifer Ceiling",
        TerrainType::NavigableRiver => "Navigable River",
    }
}

fn tile_to_dict(tile: fb::TileState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("entity", tile.entity() as i64);
    let _ = dict.insert("x", tile.x() as i64);
    let _ = dict.insert("y", tile.y() as i64);
    let _ = dict.insert("element", tile.element() as i64);
    let _ = dict.insert("mass", fixed64_to_f64(tile.mass()));
    let _ = dict.insert("temperature", fixed64_to_f64(tile.temperature()));
    // Band-independent per-turn morale drain of living on this tile's terrain +
    // temperature (>=0; bigger = harsher). Bucketed into a Habitability rating in Hud.
    let _ = dict.insert("habitability", fixed64_to_f64(tile.habitability()));
    let _ = dict.insert("terrain", tile.terrain().0 as i64);
    // The "real ground" biome: equals `terrain` on ordinary tiles, and the preserved VALLEY biome on a
    // navigable hex (which stamps NavigableRiver over the ground the river cut). MapView packs it into the
    // shader's navigable_underlying_map so a navigable hex renders its valley as the base with only a slim
    // bank skirt hugging the channel, instead of a whole-hex bank hiding the land.
    let _ = dict.insert("underlying_terrain", tile.underlyingTerrain().0 as i64);
    let _ = dict.insert("terrain_tags", tile.terrainTags() as i64);
    // Minor/Major rivers ride the tile's EDGES, not its center: a 12-bit mask, 2 bits per odd-r
    // direction (class = (river_edges >> (2*dir)) & 0b11; 0 none / 1 Minor / 2 Major, 3 reserved).
    // Both hexes flanking an edge carry it, so a hex can answer "river on my side d?" locally.
    // MapView packs this into the shader's RG8 river-map splatmap (see _rebuild_terrain_shader_maps).
    // Navigable rivers are NOT here — they are an ordinary water terrain (TerrainType::NavigableRiver).
    let _ = dict.insert("river_edges", tile.riverEdges() as i64);
    // Where an edge river HANDS OVER to a navigable trunk. Same 12-bit packing, but keyed by hex CORNER
    // (class = (river_inflow >> (2*corner)) & 0b11), corner i being the vertex at angle 60*i + 30 with +y
    // down — exactly MapView._hex_points order. An edge river runs ALONG a side and so ends at a VERTEX,
    // never mid-edge; river_edges (which records sides) cannot say which vertex, so the sim states it here.
    // Nonzero ONLY on the first hex of a navigable chain (0 everywhere else, and 0 for a river that was
    // navigable from its first step — no tributary). MapView packs it into the river-map splatmap's B/A
    // channels, and the shader draws the channel's INFLOW SPUR from the hex centre out to that vertex, at
    // the tributary's own Minor/Major width.
    let _ = dict.insert("river_inflow", tile.riverInflow() as i64);
    // Which SIDES a navigable hex's channel actually flows out through: 1 bit per odd-r direction
    // (exits(dir) = (river_channel >> dir) & 1), same direction order as river_edges. A navigable river
    // is a chain of water hexes, and a chain is a PATH — a hex links only to its upstream and downstream
    // neighbours. Terrain cannot say which those are, so a renderer that armed every navigable/water
    // neighbour cross-linked adjacent chains into a WEB of triangles. The sim states the path here; the
    // shader arms ONLY the set sides. Symmetric across a shared side EXCEPT at the mouth, where the last
    // hex also exits into the sea / delta it drains into (open water carries no channel, so that bit is
    // not mirrored back). MapView packs it into its own R8 river-channel splatmap.
    let _ = dict.insert("river_channel", tile.riverChannel() as i64);
    let _ = dict.insert("culture_layer", tile.cultureLayer() as i64);
    let _ = dict.insert("mountain_kind", i64::from(tile.mountainKind().0));
    let _ = dict.insert("mountain_relief", tile.mountainRelief());
    // The GRAZE (pasture) layer — the ANIMAL-edible vegetal stock, on nearly every land tile
    // (Grazing Phase 2a). Deliberately NOT the same thing as the human-edible ForagePatch biomass:
    // grass/browse is cellulose humans cannot digest. Plain floats on the wire (not fixed-point).
    // `graze_capacity == 0` means this biome carries NO pasture at all (water/glacier/lava) — an
    // absent reading, never a zero-but-healthy one.
    let _ = dict.insert("graze_biomass", tile.grazeBiomass() as f64);
    let _ = dict.insert("graze_capacity", tile.grazeCapacity() as f64);
    // The FORAGE (human food) layer — the human-edible POTENTIAL of this tile's biome, twin of
    // graze_capacity. Cached client-side in `tile_forage` for the Forage overlay legend's
    // Poorest/Average/Richest figures (the overlay slice itself is built from the same field above).
    // `0` = genuinely no human food (deep ocean, glacier, lava); coastal shelves are positive.
    let _ = dict.insert("forage_capacity", tile.forageCapacity() as f64);
    // The phase rides the wire as a compact code; it is resolved HERE into the same phase
    // vocabulary the herd/forage-patch payloads already carry as strings, so the client has ONE
    // ecology vocabulary and the tile card can reuse the shared Ecology label/tint path verbatim.
    let phase = graze_phase_str(tile.grazeEcologyPhase());
    if !phase.is_empty() {
        let _ = dict.insert("graze_ecology_phase", phase);
    }
    dict
}

/// `TileState.grazeEcologyPhase` codes. Mirrors `sim_schema::GRAZE_PHASE_*` — the native crate
/// depends only on the generated FlatBuffers crate, so the codes are restated (not re-invented)
/// here. `NONE` is the schema default, so "this biome has no pasture" can never be misread as
/// "this pasture is healthy".
const GRAZE_PHASE_NONE: u8 = 0;
const GRAZE_PHASE_THRIVING: u8 = 1;
const GRAZE_PHASE_STRESSED: u8 = 2;
const GRAZE_PHASE_COLLAPSING: u8 = 3;

/// The phase code → the client's shared ecology phase vocabulary (`EcologyPhase::as_str`, the same
/// strings `HerdTelemetryState.ecologyPhase` / `ForagePatchState.ecologyPhase` carry). `NONE` maps
/// to the empty string: no pasture here, so there is no phase to report at all.
fn graze_phase_str(code: u8) -> &'static str {
    match code {
        GRAZE_PHASE_THRIVING => "thriving",
        GRAZE_PHASE_STRESSED => "stressed",
        GRAZE_PHASE_COLLAPSING => "collapsing",
        GRAZE_PHASE_NONE => "",
        _ => "",
    }
}

pub(crate) fn tiles_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TileState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for tile in list {
        let dict = tile_to_dict(tile);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}
