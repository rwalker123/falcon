//! The two top-level snapshot assemblers: [`snapshot_dict`] builds the client
//! dictionary from already-decoded rasters and sections, and [`snapshot_to_dict`]
//! walks a `WorldSnapshot` FlatBuffer to feed it.

pub(crate) mod delta;
pub(crate) mod raster;

use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use std::collections::{BTreeSet, HashMap};

use crate::dict::campaign::{
    campaign_label_to_dict, campaign_profile_to_dict, command_events_to_array,
    pending_forks_to_array, stance_axes_to_array, victory_state_to_dict, voice_medium_to_array,
};
use crate::dict::culture::{
    axis_bias_to_dict, culture_layers_to_array, culture_tensions_to_array, influencers_to_array,
    sentiment_to_dict,
};
use crate::dict::economy::{faction_inventory_to_array, trade_links_to_array};
use crate::dict::fixed64_to_f32;
use crate::dict::governance::{
    corruption_to_dict, crisis_annotation_to_dict, crisis_overlay_to_dict,
    crisis_telemetry_to_dict, power_metrics_to_dict, power_nodes_to_array,
};
use crate::dict::knowledge::{
    discovered_sites_to_array, discovery_progress_to_array, great_discovery_definitions_to_array,
    great_discovery_progress_states_to_array, great_discovery_states_to_array,
    great_discovery_telemetry_to_dict,
};
use crate::dict::map::{terrain_label_from_id, tiles_to_array, TERRAIN_TAG_LABELS};
use crate::dict::population::{demographics_to_array, generations_to_array, populations_to_array};
use crate::dict::subsistence::{
    food_modules_to_array, forage_patches_to_array, herds_to_array,
    intensification_knowledge_to_array, sedentarization_to_array,
};
use crate::snapshot::delta::CrisisAnnotationRecord;
use crate::snapshot::raster::{
    insert_overlay_channel, normalize_overlay, packed_from_slice, GridSize, OverlayChannelParams,
    OverlaySlices, TerrainSlices,
};

#[allow(clippy::too_many_arguments)]
fn snapshot_dict(
    tick: u64,
    grid_size: GridSize,
    overlays: OverlaySlices<'_>,
    terrain: TerrainSlices<'_>,
    crisis_annotations: &[CrisisAnnotationRecord],
    campaign_label: Option<VarDictionary>,
    campaign_profiles: Option<VarArray>,
    victory_state: Option<VarDictionary>,
    faction_inventory: Option<VarArray>,
    command_events: Option<VarArray>,
    herds: Option<VarArray>,
    food_modules: Option<VarArray>,
) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("turn", tick as i64);

    let mut grid_dict = VarDictionary::new();
    let _ = grid_dict.insert("width", grid_size.width as i64);
    let _ = grid_dict.insert("height", grid_size.height as i64);
    let _ = grid_dict.insert("wrap_horizontal", grid_size.wrap_horizontal);
    let _ = dict.insert("grid", &grid_dict);

    let size = (grid_size.width as usize)
        .saturating_mul(grid_size.height as usize)
        .max(1);

    let copy_into = |source: &[f32]| -> Vec<f32> {
        let mut dest = vec![0.0f32; size];
        let count = source.len().min(size);
        if count > 0 {
            dest[..count].copy_from_slice(&source[..count]);
        }
        dest
    };

    let logistics_base = copy_into(overlays.logistics);
    let sentiment_base = copy_into(overlays.sentiment);
    let corruption_base = copy_into(overlays.corruption);
    let fog_base = copy_into(overlays.fog);
    let visibility_base = copy_into(overlays.visibility);
    let culture_base = copy_into(overlays.culture);
    let military_base = copy_into(overlays.military);
    let crisis_base = copy_into(overlays.crisis);
    let elevation_base = copy_into(overlays.elevation);
    let elevation_sea_level = overlays.elevation_sea_level;
    let climate_bands = overlays.climate_bands;
    let moisture_base = copy_into(overlays.moisture);
    let pasture_base = copy_into(overlays.pasture_capacity);
    let forage_base = copy_into(overlays.forage_capacity);

    let mut logistics_normalized = logistics_base.clone();
    normalize_overlay(&mut logistics_normalized);
    let mut sentiment_normalized = sentiment_base.clone();
    normalize_overlay(&mut sentiment_normalized);
    let mut corruption_normalized = corruption_base.clone();
    normalize_overlay(&mut corruption_normalized);
    let mut fog_normalized = fog_base.clone();
    normalize_overlay(&mut fog_normalized);
    let mut visibility_normalized = visibility_base.clone();
    normalize_overlay(&mut visibility_normalized);
    let mut culture_normalized = culture_base.clone();
    normalize_overlay(&mut culture_normalized);
    let mut military_normalized = military_base.clone();
    normalize_overlay(&mut military_normalized);
    let mut crisis_normalized = crisis_base.clone();
    normalize_overlay(&mut crisis_normalized);
    let mut elevation_normalized = elevation_base.clone();
    normalize_overlay(&mut elevation_normalized);
    let mut moisture_normalized = moisture_base.clone();
    normalize_overlay(&mut moisture_normalized);
    // Pasture is normalized against the map's RICHEST pasture, NOT min-max stretched like the
    // other channels. Zero is a REAL, meaningful reading here ("no pasture at all"), not merely
    // the low end of a range: a min-max stretch would rebase the ramp onto the worst *land* value
    // and make a marginal desert look like a dead glacier — or vice versa — depending on the map.
    let pasture_max_capacity = pasture_base
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0f32, f32::max);
    let pasture_normalized: Vec<f32> = if pasture_max_capacity > 0.0 {
        pasture_base
            .iter()
            .map(|v| (v / pasture_max_capacity).clamp(0.0, 1.0))
            .collect()
    } else {
        vec![0.0f32; pasture_base.len()]
    };

    // Forage normalizes against the map's RICHEST forage patch (mirrors pasture, NOT min-max):
    // 1.0 is the best gathering site on this map, and 0.0 is the abundant "no patch here" state,
    // which the client paints neutrally rather than as barren.
    let forage_max_capacity = forage_base
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0f32, f32::max);
    let forage_normalized: Vec<f32> = if forage_max_capacity > 0.0 {
        forage_base
            .iter()
            .map(|v| (v / forage_max_capacity).clamp(0.0, 1.0))
            .collect()
    } else {
        vec![0.0f32; forage_base.len()]
    };

    // Predators Phase 0 — TWO derived-danger channels, both per-ENTITY properties projected onto
    // tiles CLIENT-SIDE (there is no per-tile danger wire field). For each herd we stamp its value
    // onto its own tile index and take the MAX where herds overlap. `herds` is the already-decoded
    // VarArray of herd dicts (each carrying `x` / `y` / `attack` / `ferocity` / `aggression`); it is
    // read by reference so the later `insert("herds", …)` still moves it. Each channel normalizes
    // against its OWN map-max, so the ramp's top is "the worst of THIS kind on THIS map":
    //   HUNT danger = attack × ferocity (cost to hunt it — a mammoth reads high).
    //   THREAT      = attack × aggression (menace unprovoked — near-zero in Phase 0; predators later).
    let mut hunt_danger_base = vec![0.0f32; size];
    let mut threat_base = vec![0.0f32; size];
    if let Some(herd_arr) = herds.as_ref() {
        let herd_f32 = |dict: &VarDictionary, key: &str| -> f32 {
            dict.get(key)
                .and_then(|v| v.try_to::<f64>().ok())
                .unwrap_or(0.0) as f32
        };
        for item in herd_arr.iter_shared() {
            let Ok(herd_dict) = item.try_to::<VarDictionary>() else {
                continue;
            };
            let attack = herd_f32(&herd_dict, "attack");
            let hunt_danger = attack * herd_f32(&herd_dict, "ferocity");
            let threat = attack * herd_f32(&herd_dict, "aggression");
            if hunt_danger <= 0.0 && threat <= 0.0 {
                continue;
            }
            let x = herd_dict
                .get("x")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(-1);
            let y = herd_dict
                .get("y")
                .and_then(|v| v.try_to::<i64>().ok())
                .unwrap_or(-1);
            // In-bounds on BOTH axes before computing idx: flooring at 0 is not enough — a herd
            // off the right/bottom edge (x >= width / y >= height) would wrap into the wrong tile's
            // slot (idx = y*width + x row-wraps), and the `idx < len` guard only catches full OOB,
            // never a row-wrap. Skip the herd entirely if either axis is out of range.
            if x < 0 || y < 0 || x >= grid_size.width as i64 || y >= grid_size.height as i64 {
                continue;
            }
            let idx = (y as usize)
                .saturating_mul(grid_size.width as usize)
                .saturating_add(x as usize);
            if idx < hunt_danger_base.len() {
                hunt_danger_base[idx] = hunt_danger_base[idx].max(hunt_danger);
                threat_base[idx] = threat_base[idx].max(threat);
            }
        }
    }
    let channel_max = |base: &[f32]| -> f32 {
        base.iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(0.0f32, f32::max)
    };
    let normalize_channel = |base: &[f32], max: f32| -> Vec<f32> {
        if max > 0.0 {
            base.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
        } else {
            vec![0.0f32; base.len()]
        }
    };
    let hunt_danger_max = channel_max(&hunt_danger_base);
    let threat_max = channel_max(&threat_base);
    let hunt_danger_normalized = normalize_channel(&hunt_danger_base, hunt_danger_max);
    let threat_normalized = normalize_channel(&threat_base, threat_max);

    let mut logistics_contrast_vec = logistics_normalized.clone();
    for value in logistics_contrast_vec.iter_mut() {
        let v = *value;
        *value = v * (1.0 - v);
    }

    let mut sentiment_contrast_vec = sentiment_normalized.clone();
    for value in sentiment_contrast_vec.iter_mut() {
        *value = ((*value - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    }

    let corruption_contrast_vec = corruption_normalized.clone();
    let fog_contrast_vec = fog_normalized.clone();
    let visibility_contrast_vec = visibility_normalized.clone();
    let culture_contrast_vec = culture_normalized.clone();
    let mut military_contrast_vec = military_normalized.clone();
    for value in military_contrast_vec.iter_mut() {
        *value = ((*value - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    }
    let mut crisis_contrast_vec = crisis_normalized.clone();
    for value in crisis_contrast_vec.iter_mut() {
        let v = *value;
        *value = v * (1.0 - v);
    }
    let elevation_contrast_vec = elevation_normalized.clone();
    let moisture_contrast_vec = moisture_normalized.clone();

    let corruption_placeholder = overlays.corruption.is_empty();
    let fog_placeholder = overlays.fog.is_empty();
    let visibility_placeholder = overlays.visibility.is_empty();
    let culture_placeholder = overlays.culture.is_empty();
    let military_placeholder = overlays.military.is_empty();
    let crisis_placeholder = overlays.crisis.is_empty();

    let logistics_array = packed_from_slice(&logistics_normalized);
    let logistics_raw_array = packed_from_slice(&logistics_base);
    let logistics_contrast_array = packed_from_slice(&logistics_contrast_vec);
    let sentiment_array = packed_from_slice(&sentiment_normalized);
    let sentiment_raw_array = packed_from_slice(&sentiment_base);
    let sentiment_contrast_array = packed_from_slice(&sentiment_contrast_vec);
    let corruption_array = packed_from_slice(&corruption_normalized);
    let corruption_raw_array = packed_from_slice(&corruption_base);
    let corruption_contrast_array = packed_from_slice(&corruption_contrast_vec);
    let fog_array = packed_from_slice(&fog_normalized);
    let fog_raw_array = packed_from_slice(&fog_base);
    let fog_contrast_array = packed_from_slice(&fog_contrast_vec);
    let visibility_array = packed_from_slice(&visibility_normalized);
    let visibility_raw_array = packed_from_slice(&visibility_base);
    let visibility_contrast_array = packed_from_slice(&visibility_contrast_vec);
    let culture_array = packed_from_slice(&culture_normalized);
    let culture_raw_array = packed_from_slice(&culture_base);
    let culture_contrast_array = packed_from_slice(&culture_contrast_vec);
    let military_array = packed_from_slice(&military_normalized);
    let military_raw_array = packed_from_slice(&military_base);
    let military_contrast_array = packed_from_slice(&military_contrast_vec);
    let crisis_array = packed_from_slice(&crisis_normalized);
    let crisis_raw_array = packed_from_slice(&crisis_base);
    let crisis_contrast_array = packed_from_slice(&crisis_contrast_vec);
    let elevation_array = packed_from_slice(&elevation_normalized);
    let elevation_raw_array = packed_from_slice(&elevation_base);
    let elevation_contrast_array = packed_from_slice(&elevation_contrast_vec);
    let moisture_array = packed_from_slice(&moisture_normalized);
    let moisture_raw_array = packed_from_slice(&moisture_base);
    let moisture_contrast_array = packed_from_slice(&moisture_contrast_vec);
    let pasture_array = packed_from_slice(&pasture_normalized);
    let pasture_raw_array = packed_from_slice(&pasture_base);
    let forage_array = packed_from_slice(&forage_normalized);
    let forage_raw_array = packed_from_slice(&forage_base);
    let hunt_danger_array = packed_from_slice(&hunt_danger_normalized);
    let hunt_danger_raw_array = packed_from_slice(&hunt_danger_base);
    let threat_array = packed_from_slice(&threat_normalized);
    let threat_raw_array = packed_from_slice(&threat_base);

    let elevation_placeholder = elevation_array.is_empty();
    let moisture_placeholder = overlays.moisture.is_empty();

    let mut overlays = VarDictionary::new();
    let mut channels = VarDictionary::new();
    let mut channel_order = PackedStringArray::new();

    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "logistics",
            label: "Logistics Throughput",
            description: Some(
                "Sum of supply flow touching the tile after current corruption multipliers.",
            ),
            normalized: &logistics_array,
            raw: &logistics_raw_array,
            contrast: &logistics_contrast_array,
            placeholder: false,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "crisis",
            label: "Crisis Stress",
            description: Some(
                "Normalized crisis pressure per tile derived from local grid stability and incidents.",
            ),
            normalized: &crisis_array,
            raw: &crisis_raw_array,
            contrast: &crisis_contrast_array,
            placeholder: crisis_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "sentiment",
            label: "Sentiment Morale",
            description: Some(
                "Average morale of population cohorts anchored to the tile (fixed-point scale).",
            ),
            normalized: &sentiment_array,
            raw: &sentiment_raw_array,
            contrast: &sentiment_contrast_array,
            placeholder: false,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "corruption",
            label: "Corruption Pressure",
            description: Some(
                "Composite pressure mixing active incidents with logistics, trade, military, and governance risk at each tile.",
            ),
            normalized: &corruption_array,
            raw: &corruption_raw_array,
            contrast: &corruption_contrast_array,
            placeholder: corruption_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "fog",
            label: "Fog of Knowledge",
            description: Some(
                "Knowledge gap for the controlling faction and local cohorts (1.0 = unknown, 0.0 = fully scouted).",
            ),
            normalized: &fog_array,
            raw: &fog_raw_array,
            contrast: &fog_contrast_array,
            placeholder: fog_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "visibility",
            label: "Fog of War",
            description: Some(
                "Line-of-sight visibility from units and settlements (0 = unexplored, 0.5 = discovered, 1 = active).",
            ),
            normalized: &visibility_array,
            raw: &visibility_raw_array,
            contrast: &visibility_contrast_array,
            placeholder: visibility_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "culture",
            label: "Culture Divergence",
            description: Some(
                "Local layer divergence relative to schism thresholds (1.0 = schism risk).",
            ),
            normalized: &culture_array,
            raw: &culture_raw_array,
            contrast: &culture_contrast_array,
            placeholder: culture_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "military",
            label: "Force Readiness",
            description: Some("Composite of garrison morale, manpower, and local supply margin."),
            normalized: &military_array,
            raw: &military_raw_array,
            contrast: &military_contrast_array,
            placeholder: military_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "moisture",
            label: "Moisture & Rain Shadows",
            description: Some(
                "Humidity field after windward lift and leeward drying (0 = arid, 1 = saturated).",
            ),
            normalized: &moisture_array,
            raw: &moisture_raw_array,
            contrast: &moisture_contrast_array,
            placeholder: moisture_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "elevation",
            label: "Elevation Heatmap",
            description: Some(
                "Relative elevation above sea level after tectonic restamp (0 = coast, 1 = peaks).",
            ),
            normalized: &elevation_array,
            raw: &elevation_raw_array,
            contrast: &elevation_contrast_array,
            placeholder: elevation_base.is_empty(),
        },
    );
    // Pasture (Grazing Phase 2a). Published ONLY when the snapshot actually carries graze: a
    // map-wide field of zeros is not "no data", it is the false claim that the world has no
    // pasture anywhere. A delta (which carries no per-tile graze) therefore simply omits it.
    if pasture_max_capacity > 0.0 {
        insert_overlay_channel(
            &mut channels,
            &mut channel_order,
            OverlayChannelParams {
                key: "pasture",
                label: "Pasture (Graze Capacity)",
                description: Some(
                    "How much GRASS AND BROWSE this tile's biome can carry — the animal-edible stock (humans cannot digest it). Prairie is the reference pasture; a closed forest canopy shades the ground cover out; water, glacier and lava carry NO pasture at all and are drawn as barren, not as poor.",
                ),
                normalized: &pasture_array,
                raw: &pasture_raw_array,
                // No separate contrast curve: the capacity ramp IS the signal being read.
                contrast: &pasture_array,
                placeholder: false,
            },
        );
    }
    // Forage (human food) — the twin of the pasture channel, sourced per-tile from the biome's
    // human-food potential. Published ONLY when the snapshot actually carries forage capacity.
    // A `0` here means genuinely no human food (deep ocean, glacier, lava); coastal shelves carry
    // fishing potential and sit ON the ramp, so the forage map DIVERGES from pasture (where all
    // water is barren) — that divergence is the whole point of the two-web split.
    if forage_max_capacity > 0.0 {
        insert_overlay_channel(
            &mut channels,
            &mut channel_order,
            OverlayChannelParams {
                key: "forage",
                label: "Forage (Human Food Capacity)",
                description: Some(
                    "The HUMAN-edible potential of this land — seeds, nuts, tubers, fruit, and fish. Every tile carries a value from its biome; forest and river valleys read rich where prairie reads poor, coastal shelves light up as fishing grounds, and only deep ocean, glacier and lava carry none.",
                ),
                normalized: &forage_array,
                raw: &forage_raw_array,
                // No separate contrast curve: the capacity ramp IS the signal being read.
                contrast: &forage_array,
                placeholder: false,
            },
        );
    }
    // Predators Phase 0 — the two derived-danger channels (see the base builds above), each
    // projected client-side from herd positions. Published ONLY when the map actually carries a
    // qualifying animal (its own max > 0), so a placid map omits the channel entirely and the
    // data-driven overlay selector shows no dead entry. In Phase 0 nothing is aggressive yet, so
    // `threat` is typically absent — that is correct. The generic scalar legend handles both.
    if hunt_danger_max > 0.0 {
        insert_overlay_channel(
            &mut channels,
            &mut channel_order,
            OverlayChannelParams {
                key: "hunt_danger",
                label: "Hunt danger",
                description: Some(
                    "How costly the wildlife on this tile is to HUNT — attack × ferocity, the deadliest fighter standing here. A placid grazer reads low even if huge; an aurochs or mammoth reads high. Empty ground reads as none.",
                ),
                normalized: &hunt_danger_array,
                raw: &hunt_danger_raw_array,
                // No separate contrast curve: the threat ramp IS the signal being read.
                contrast: &hunt_danger_array,
                placeholder: false,
            },
        );
    }
    if threat_max > 0.0 {
        insert_overlay_channel(
            &mut channels,
            &mut channel_order,
            OverlayChannelParams {
                key: "threat",
                label: "Threat",
                description: Some(
                    "How much the wildlife on this tile menaces you UNPROVOKED — attack × aggression. A grazer (aggression 0) reads none however dangerous it is to hunt; a predator reads high. Empty ground reads as none.",
                ),
                normalized: &threat_array,
                raw: &threat_raw_array,
                // No separate contrast curve: the threat ramp IS the signal being read.
                contrast: &threat_array,
                placeholder: false,
            },
        );
    }

    let _ = overlays.insert("channels", &channels);
    let _ = overlays.insert("channel_order", &channel_order.clone());
    let _ = overlays.insert("default_channel", "logistics");

    if corruption_placeholder
        || fog_placeholder
        || culture_placeholder
        || military_placeholder
        || crisis_placeholder
        || elevation_placeholder
        || moisture_placeholder
    {
        let mut placeholder_keys = PackedStringArray::new();
        if corruption_placeholder {
            placeholder_keys.push(&GString::from("corruption"));
        }
        if fog_placeholder {
            placeholder_keys.push(&GString::from("fog"));
        }
        if culture_placeholder {
            placeholder_keys.push(&GString::from("culture"));
        }
        if military_placeholder {
            placeholder_keys.push(&GString::from("military"));
        }
        if crisis_placeholder {
            placeholder_keys.push(&GString::from("crisis"));
        }
        if elevation_placeholder {
            placeholder_keys.push(&GString::from("elevation"));
        }
        if moisture_placeholder {
            placeholder_keys.push(&GString::from("moisture"));
        }
        let _ = overlays.insert("placeholder_channels", &placeholder_keys);
    }

    let _ = overlays.insert("logistics", &logistics_array.clone());
    let _ = overlays.insert("logistics_raw", &logistics_raw_array.clone());
    let _ = overlays.insert("logistics_contrast", &logistics_contrast_array.clone());
    let _ = overlays.insert("contrast", &logistics_contrast_array.clone());
    let _ = overlays.insert("sentiment", &sentiment_array.clone());
    let _ = overlays.insert("sentiment_raw", &sentiment_raw_array.clone());
    let _ = overlays.insert("sentiment_contrast", &sentiment_contrast_array.clone());
    let _ = overlays.insert("corruption", &corruption_array.clone());
    let _ = overlays.insert("corruption_raw", &corruption_raw_array.clone());
    let _ = overlays.insert("corruption_contrast", &corruption_contrast_array.clone());
    let _ = overlays.insert("fog", &fog_array.clone());
    let _ = overlays.insert("fog_raw", &fog_raw_array.clone());
    let _ = overlays.insert("fog_contrast", &fog_contrast_array.clone());
    let _ = overlays.insert("culture", &culture_array.clone());
    let _ = overlays.insert("culture_raw", &culture_raw_array.clone());
    let _ = overlays.insert("culture_contrast", &culture_contrast_array.clone());
    let _ = overlays.insert("military", &military_array.clone());
    let _ = overlays.insert("military_raw", &military_raw_array.clone());
    let _ = overlays.insert("military_contrast", &military_contrast_array.clone());
    let _ = overlays.insert("crisis", &crisis_array.clone());
    let _ = overlays.insert("crisis_raw", &crisis_raw_array.clone());
    let _ = overlays.insert("crisis_contrast", &crisis_contrast_array.clone());
    let _ = overlays.insert("elevation", &elevation_array.clone());
    let _ = overlays.insert("elevation_raw", &elevation_raw_array.clone());
    let _ = overlays.insert("elevation_contrast", &elevation_contrast_array.clone());
    let _ = overlays.insert("elevation_sea_level", elevation_sea_level);
    // Per-map climate-band cut points, published only when the snapshot carries them (a
    // missing table leaves the keys absent so the client renders the Climate line blank
    // rather than inheriting a stale or fabricated cut point — see OverlaySlices).
    if let Some([polar_max, boreal_max, temperate_max]) = climate_bands {
        let _ = overlays.insert("climate_polar_max_temp", polar_max);
        let _ = overlays.insert("climate_boreal_max_temp", boreal_max);
        let _ = overlays.insert("climate_temperate_max_temp", temperate_max);
    }
    let _ = overlays.insert("moisture", &moisture_array.clone());
    let _ = overlays.insert("moisture_raw", &moisture_raw_array.clone());
    let _ = overlays.insert("moisture_contrast", &moisture_contrast_array.clone());
    let mut crisis_annotation_array = VarArray::new();
    for record in crisis_annotations {
        let dict = crisis_annotation_to_dict(record);
        crisis_annotation_array.push(&dict.to_variant());
    }
    let _ = overlays.insert("crisis_annotations", &crisis_annotation_array);

    if let Some(terrain_data) = terrain.terrain {
        let mut terrain_array = PackedInt32Array::new();
        terrain_array.resize(size);
        if size > 0 {
            let slice = terrain_array.as_mut_slice();
            let count = terrain_data.len().min(slice.len());
            for i in 0..count {
                slice[i] = terrain_data[i] as i32;
            }
        }
        let _ = overlays.insert("terrain", &terrain_array);

        if let Some(tag_data) = terrain.tags {
            let mut tag_array = PackedInt32Array::new();
            tag_array.resize(size);
            if size > 0 {
                let slice = tag_array.as_mut_slice();
                let count = tag_data.len().min(slice.len());
                for i in 0..count {
                    slice[i] = tag_data[i] as i32;
                }
            }
            let _ = overlays.insert("terrain_tags", &tag_array);
        }

        let mut palette = VarDictionary::new();
        let mut seen = BTreeSet::new();
        for &value in terrain_data {
            if seen.insert(value) {
                let _ = palette.insert(value as i64, terrain_label_from_id(value));
            }
        }
        let _ = overlays.insert("terrain_palette", &palette);

        let mut tag_labels = VarDictionary::new();
        for (mask, label) in TERRAIN_TAG_LABELS.iter() {
            let _ = tag_labels.insert(*mask as i64, *label);
        }
        let _ = overlays.insert("terrain_tag_labels", &tag_labels);
    }

    let _ = dict.insert("overlays", &overlays);

    let _ = dict.insert("units", &VarArray::new());
    let _ = dict.insert("orders", &VarArray::new());

    if let Some(label) = campaign_label {
        let _ = dict.insert("campaign_label", &label);
    }
    if let Some(profiles) = campaign_profiles {
        let _ = dict.insert("campaign_profiles", &profiles);
    }
    if let Some(victory) = victory_state {
        let _ = dict.insert("victory", &victory);
    }
    if let Some(inventory) = faction_inventory {
        if !inventory.is_empty() {
            let _ = dict.insert("faction_inventory", &inventory);
        }
    }
    if let Some(events) = command_events {
        if !events.is_empty() {
            let _ = dict.insert("command_events", &events);
        }
    }
    if let Some(herd_array) = herds {
        if !herd_array.is_empty() {
            let _ = dict.insert("herds", &herd_array);
        }
    }
    if let Some(food_array) = food_modules {
        if !food_array.is_empty() {
            let _ = dict.insert("food_modules", &food_array);
        }
    }

    dict
}
pub(crate) fn snapshot_to_dict(snapshot: fb::WorldSnapshot<'_>) -> VarDictionary {
    let header = snapshot.header().unwrap();

    let mut logistics_grid: Vec<f32> = Vec::new();
    let mut logistics_dims = (0u32, 0u32);
    let mut corruption_grid: Vec<f32> = Vec::new();
    let mut corruption_dims = (0u32, 0u32);
    let mut fog_grid: Vec<f32> = Vec::new();
    let mut fog_dims = (0u32, 0u32);
    let mut visibility_grid: Vec<f32> = Vec::new();
    let mut visibility_dims = (0u32, 0u32);
    let mut culture_grid: Vec<f32> = Vec::new();
    let mut culture_dims = (0u32, 0u32);
    let mut military_grid: Vec<f32> = Vec::new();
    let mut military_dims = (0u32, 0u32);
    let mut crisis_grid: Vec<f32> = Vec::new();
    let mut crisis_dims = (0u32, 0u32);
    let mut elevation_grid: Vec<f32> = Vec::new();
    let mut elevation_dims = (0u32, 0u32);
    let mut elevation_sea_level: f32 = 0.0;
    let mut moisture_grid: Vec<f32> = Vec::new();
    let mut moisture_dims = (0u32, 0u32);
    let mut crisis_annotations: Vec<CrisisAnnotationRecord> = Vec::new();
    if let Some(raster) = snapshot.economy().and_then(|s| s.logisticsRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            logistics_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    logistics_grid[idx] = fixed64_to_f32(value);
                }
            }
            logistics_dims = (width, height);
        }
    }

    if logistics_grid.is_empty() {
        let mut width = 0u32;
        let mut height = 0u32;
        let mut fallback: HashMap<(u32, u32), f32> = HashMap::new();
        if let Some(tiles) = snapshot.map().and_then(|s| s.tiles()) {
            for tile in tiles {
                let x = tile.x();
                let y = tile.y();
                width = width.max(x + 1);
                height = height.max(y + 1);
                fallback.insert((x, y), fixed64_to_f32(tile.temperature()));
            }
        }
        let width = width.max(1);
        let height = height.max(1);
        let total = (width as usize).saturating_mul(height as usize);
        logistics_grid = vec![0.0f32; total];
        for ((x, y), value) in fallback.into_iter() {
            if x >= width || y >= height {
                continue;
            }
            let idx = (y as usize) * (width as usize) + x as usize;
            logistics_grid[idx] = value;
        }
        logistics_dims = (width, height);
    }

    let mut terrain_width = 0u32;
    let mut terrain_height = 0u32;
    let mut terrain_samples: Vec<(u16, u16)> = Vec::new();
    if let Some(layer) = snapshot.map().and_then(|s| s.terrainOverlay()) {
        terrain_width = layer.width();
        terrain_height = layer.height();
        if let Some(samples) = layer.samples() {
            terrain_samples.reserve(samples.len());
            for sample in samples {
                terrain_samples.push((sample.terrain().0, sample.tags()));
            }
        }
    }

    if let Some(raster) = snapshot.governance().and_then(|s| s.corruptionRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            corruption_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    corruption_grid[idx] = fixed64_to_f32(value);
                }
            }
            corruption_dims = (width, height);
        }
    }

    if corruption_grid.is_empty() {
        let fallback_width = logistics_dims.0.max(terrain_width).max(1);
        let fallback_height = logistics_dims.1.max(terrain_height).max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        corruption_grid = vec![0.0f32; total];
        corruption_dims = (fallback_width, fallback_height);
    }

    let mut sentiment_grid: Vec<f32> = Vec::new();
    let mut sentiment_dims = (0u32, 0u32);
    if let Some(raster) = snapshot.culture().and_then(|s| s.sentimentRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            sentiment_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    sentiment_grid[idx] = fixed64_to_f32(value);
                }
            }
            sentiment_dims = (width, height);
        }
    }

    if sentiment_grid.is_empty() {
        let fallback_width = if logistics_dims.0 > 0 {
            logistics_dims.0
        } else if terrain_width > 0 {
            terrain_width
        } else {
            1
        };
        let fallback_height = if logistics_dims.1 > 0 {
            logistics_dims.1
        } else if terrain_height > 0 {
            terrain_height
        } else {
            1
        };
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        sentiment_grid = vec![0.0f32; total];
        sentiment_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.vision().and_then(|s| s.fogRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            fog_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    fog_grid[idx] = fixed64_to_f32(value);
                }
            }
            fog_dims = (width, height);
        }
    }

    if fog_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(corruption_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(corruption_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        fog_grid = vec![0.0f32; total];
        fog_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.vision().and_then(|s| s.visibilityRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            visibility_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    visibility_grid[idx] = fixed64_to_f32(value);
                }
            }
            visibility_dims = (width, height);
        }
    }

    if visibility_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(corruption_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(corruption_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        visibility_grid = vec![0.0f32; total];
        visibility_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.culture().and_then(|s| s.cultureRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            culture_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    culture_grid[idx] = fixed64_to_f32(value);
                }
            }
            culture_dims = (width, height);
        }
    }

    if culture_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(terrain_width)
            .max(corruption_dims.0)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(terrain_height)
            .max(corruption_dims.1)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        culture_grid = vec![0.0f32; total];
        culture_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.vision().and_then(|s| s.militaryRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            military_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    military_grid[idx] = fixed64_to_f32(value);
                }
            }
            military_dims = (width, height);
        }
    }

    if let Some(overlay) = snapshot.governance().and_then(|s| s.crisisOverlay()) {
        if let Some(raster) = overlay.heatmap() {
            let width = raster.width();
            let height = raster.height();
            if width > 0 && height > 0 {
                let total = (width as usize).saturating_mul(height as usize);
                crisis_grid = vec![0.0f32; total];
                if let Some(samples) = raster.samples() {
                    for (idx, value) in samples.iter().enumerate() {
                        if idx >= total {
                            break;
                        }
                        crisis_grid[idx] = fixed64_to_f32(value);
                    }
                }
                crisis_dims = (width, height);
            }
        }
        if let Some(entries) = overlay.annotations() {
            for annotation in entries {
                let mut record = CrisisAnnotationRecord {
                    label: annotation.label().map(|value| value.to_string()),
                    severity: annotation.severity(),
                    path: Vec::new(),
                };
                if let Some(path) = annotation.path() {
                    record.path.reserve(path.len());
                    for value in path {
                        record.path.push(value as i32);
                    }
                }
                crisis_annotations.push(record);
            }
        }
    }

    if let Some(raster) = snapshot.map().and_then(|s| s.moistureRaster()) {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            moisture_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    moisture_grid[idx] = value;
                }
            }
            moisture_dims = (width, height);
        }
    }

    let climate_bands = snapshot.map().and_then(|s| s.climateBands()).map(|bands| {
        [
            bands.polarMaxTemp(),
            bands.borealMaxTemp(),
            bands.temperateMaxTemp(),
        ]
    });

    if let Some(overlay) = snapshot.map().and_then(|s| s.elevationOverlay()) {
        let width = overlay.width();
        let height = overlay.height();
        elevation_sea_level = overlay.seaLevel();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            elevation_grid = vec![0.0f32; total];
            if let Some(samples) = overlay.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    elevation_grid[idx] = (value as f32) / 65535.0;
                }
            }
            elevation_dims = (width, height);
        }
    }

    if military_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(culture_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(culture_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        military_grid = vec![0.0f32; total];
        military_dims = (fallback_width, fallback_height);
    }

    if crisis_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(military_dims.0)
            .max(culture_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(military_dims.1)
            .max(culture_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        crisis_grid = vec![0.0f32; total];
        crisis_dims = (fallback_width, fallback_height);
    }

    if elevation_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(sentiment_dims.0)
            .max(corruption_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(sentiment_dims.1)
            .max(corruption_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        elevation_grid = vec![0.0f32; total];
        elevation_dims = (fallback_width, fallback_height);
    }

    let final_width = logistics_dims
        .0
        .max(sentiment_dims.0)
        .max(terrain_width)
        .max(corruption_dims.0)
        .max(fog_dims.0)
        .max(culture_dims.0)
        .max(military_dims.0)
        .max(crisis_dims.0)
        .max(elevation_dims.0)
        .max(moisture_dims.0)
        .max(1);
    let final_height = logistics_dims
        .1
        .max(sentiment_dims.1)
        .max(terrain_height)
        .max(corruption_dims.1)
        .max(fog_dims.1)
        .max(culture_dims.1)
        .max(military_dims.1)
        .max(crisis_dims.1)
        .max(elevation_dims.1)
        .max(moisture_dims.1)
        .max(1);
    let total = (final_width as usize)
        .saturating_mul(final_height as usize)
        .max(1);

    let mut logistics_resized = vec![0.0f32; total];
    if logistics_dims.0 > 0 && logistics_dims.1 > 0 {
        for y in 0..logistics_dims.1 {
            for x in 0..logistics_dims.0 {
                let src_idx = (y as usize) * (logistics_dims.0 as usize) + x as usize;
                if src_idx >= logistics_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                logistics_resized[dst_idx] = logistics_grid[src_idx];
            }
        }
    }

    let mut sentiment_resized = vec![0.0f32; total];
    if sentiment_dims.0 > 0 && sentiment_dims.1 > 0 {
        for y in 0..sentiment_dims.1 {
            for x in 0..sentiment_dims.0 {
                let src_idx = (y as usize) * (sentiment_dims.0 as usize) + x as usize;
                if src_idx >= sentiment_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                sentiment_resized[dst_idx] = sentiment_grid[src_idx];
            }
        }
    }

    let mut corruption_resized = vec![0.0f32; total];
    if corruption_dims.0 > 0 && corruption_dims.1 > 0 {
        for y in 0..corruption_dims.1 {
            for x in 0..corruption_dims.0 {
                let src_idx = (y as usize) * (corruption_dims.0 as usize) + x as usize;
                if src_idx >= corruption_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                corruption_resized[dst_idx] = corruption_grid[src_idx];
            }
        }
    }

    let mut fog_resized = vec![0.0f32; total];
    if fog_dims.0 > 0 && fog_dims.1 > 0 {
        for y in 0..fog_dims.1 {
            for x in 0..fog_dims.0 {
                let src_idx = (y as usize) * (fog_dims.0 as usize) + x as usize;
                if src_idx >= fog_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                fog_resized[dst_idx] = fog_grid[src_idx];
            }
        }
    }

    let mut visibility_resized = vec![0.0f32; total];
    if visibility_dims.0 > 0 && visibility_dims.1 > 0 {
        for y in 0..visibility_dims.1 {
            for x in 0..visibility_dims.0 {
                let src_idx = (y as usize) * (visibility_dims.0 as usize) + x as usize;
                if src_idx >= visibility_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                visibility_resized[dst_idx] = visibility_grid[src_idx];
            }
        }
    }

    let mut culture_resized = vec![0.0f32; total];
    if culture_dims.0 > 0 && culture_dims.1 > 0 {
        for y in 0..culture_dims.1 {
            for x in 0..culture_dims.0 {
                let src_idx = (y as usize) * (culture_dims.0 as usize) + x as usize;
                if src_idx >= culture_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                culture_resized[dst_idx] = culture_grid[src_idx];
            }
        }
    }

    let mut military_resized = vec![0.0f32; total];
    if military_dims.0 > 0 && military_dims.1 > 0 {
        for y in 0..military_dims.1 {
            for x in 0..military_dims.0 {
                let src_idx = (y as usize) * (military_dims.0 as usize) + x as usize;
                if src_idx >= military_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                military_resized[dst_idx] = military_grid[src_idx];
            }
        }
    }

    let mut crisis_resized = vec![0.0f32; total];
    if crisis_dims.0 > 0 && crisis_dims.1 > 0 {
        for y in 0..crisis_dims.1 {
            for x in 0..crisis_dims.0 {
                let src_idx = (y as usize) * (crisis_dims.0 as usize) + x as usize;
                if src_idx >= crisis_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                crisis_resized[dst_idx] = crisis_grid[src_idx];
            }
        }
    }

    let mut elevation_resized = vec![0.0f32; total];
    if elevation_dims.0 > 0 && elevation_dims.1 > 0 {
        for y in 0..elevation_dims.1 {
            for x in 0..elevation_dims.0 {
                let src_idx = (y as usize) * (elevation_dims.0 as usize) + x as usize;
                if src_idx >= elevation_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                elevation_resized[dst_idx] = elevation_grid[src_idx];
            }
        }
    }
    let mut moisture_resized = vec![0.0f32; total];
    if moisture_dims.0 > 0 && moisture_dims.1 > 0 {
        for y in 0..moisture_dims.1 {
            for x in 0..moisture_dims.0 {
                let src_idx = (y as usize) * (moisture_dims.0 as usize) + x as usize;
                if src_idx >= moisture_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                moisture_resized[dst_idx] = moisture_grid[src_idx];
            }
        }
    }

    let mut terrain_vec: Vec<u16> = Vec::new();
    let mut tag_vec: Vec<u16> = Vec::new();
    if terrain_width > 0 && terrain_height > 0 && !terrain_samples.is_empty() {
        terrain_vec = vec![0u16; total];
        tag_vec = vec![0u16; total];
        for y in 0..terrain_height {
            for x in 0..terrain_width {
                let src_idx = (y as usize) * (terrain_width as usize) + x as usize;
                if src_idx >= terrain_samples.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                let (terrain, tags) = terrain_samples[src_idx];
                terrain_vec[dst_idx] = terrain;
                tag_vec[dst_idx] = tags;
            }
        }
    }

    let terrain_slice = if terrain_vec.is_empty() {
        None
    } else {
        Some(terrain_vec.as_slice())
    };
    let terrain_tag_slice = if tag_vec.is_empty() {
        None
    } else {
        Some(tag_vec.as_slice())
    };

    // The PASTURE field, assembled from the tiles (graze rides `TileState`, not a raster — it is
    // per-entity diffed, so an ungrazed turn costs zero delta bytes). A tile that carries no patch
    // reports capacity 0, which is exactly the reading we want: "this ground holds no pasture".
    let mut pasture_capacity_vec: Vec<f32> = vec![0.0f32; total];
    if let Some(tiles) = snapshot.map().and_then(|s| s.tiles()) {
        for tile in tiles {
            let x = tile.x();
            let y = tile.y();
            if x >= final_width || y >= final_height {
                continue;
            }
            let idx = (y as usize) * (final_width as usize) + x as usize;
            pasture_capacity_vec[idx] = tile.grazeCapacity();
        }
    }

    // The FORAGE field, assembled from the tiles' human-food POTENTIAL (`TileState.forageCapacity`),
    // the exact twin of pasture above — every tile carries a value from its biome. `0` = genuinely
    // no human food (deep ocean, glacier, lava); coastal shelves carry a positive value and sit ON
    // the ramp (fishing), which is where the forage map diverges from pasture.
    let mut forage_capacity_vec: Vec<f32> = vec![0.0f32; total];
    if let Some(tiles) = snapshot.map().and_then(|s| s.tiles()) {
        for tile in tiles {
            let x = tile.x();
            let y = tile.y();
            if x >= final_width || y >= final_height {
                continue;
            }
            let idx = (y as usize) * (final_width as usize) + x as usize;
            forage_capacity_vec[idx] = tile.forageCapacity();
        }
    }

    // NOTE: the old HydrologyOverlay polyline (RiverSegment/HydrologyPoint) was DELETED from the
    // schema. Rivers now ride the tiles: Minor/Major as the per-tile `riverEdges` bitmask (see
    // tile_to_dict) and Navigable as the `NavigableRiver` terrain — the tiles fully determine the
    // render, so a parallel overlay copy of that state no longer exists.

    let campaign_label_dict = header.campaignLabel().map(campaign_label_to_dict);
    let mut campaign_profiles_array: Option<VarArray> = None;
    if let Some(profiles) = snapshot.campaign().and_then(|s| s.campaignProfiles()) {
        let mut arr = VarArray::new();
        for profile in profiles {
            let dict = campaign_profile_to_dict(profile);
            arr.push(&dict.to_variant());
        }
        if !arr.is_empty() {
            campaign_profiles_array = Some(arr);
        }
    }
    let victory_dict = snapshot
        .campaign()
        .and_then(|s| s.victory())
        .map(victory_state_to_dict);
    let faction_inventory_array = snapshot
        .economy()
        .and_then(|s| s.factionInventory())
        .map(faction_inventory_to_array);
    let herds_array = snapshot
        .subsistence()
        .and_then(|s| s.herds())
        .map(herds_to_array);

    let mut dict = snapshot_dict(
        header.tick(),
        GridSize {
            width: final_width,
            height: final_height,
            wrap_horizontal: header.wrapHorizontal(),
        },
        OverlaySlices {
            logistics: &logistics_resized,
            sentiment: &sentiment_resized,
            corruption: &corruption_resized,
            fog: &fog_resized,
            culture: &culture_resized,
            military: &military_resized,
            crisis: &crisis_resized,
            elevation: &elevation_resized,
            elevation_sea_level,
            climate_bands,
            moisture: &moisture_resized,
            visibility: &visibility_resized,
            pasture_capacity: &pasture_capacity_vec,
            forage_capacity: &forage_capacity_vec,
        },
        TerrainSlices {
            terrain: terrain_slice,
            tags: terrain_tag_slice,
        },
        &crisis_annotations,
        campaign_label_dict,
        campaign_profiles_array,
        victory_dict,
        faction_inventory_array,
        snapshot
            .campaign()
            .and_then(|s| s.commandEvents())
            .map(command_events_to_array),
        herds_array,
        snapshot
            .subsistence()
            .and_then(|s| s.foodModules())
            .map(food_modules_to_array),
    );

    if let Some(pending_forks) = snapshot.campaign().and_then(|s| s.pendingForks()) {
        let _ = dict.insert("pending_forks", &pending_forks_to_array(pending_forks));
    }

    if let Some(stance_axes) = snapshot.campaign().and_then(|s| s.stanceAxes()) {
        let _ = dict.insert("stance_axes", &stance_axes_to_array(stance_axes));
    }

    if let Some(voice_medium) = snapshot.campaign().and_then(|s| s.voiceMedium()) {
        let _ = dict.insert("voice_medium", &voice_medium_to_array(voice_medium));
    }

    if let Some(server_build) = header.serverBuild() {
        let _ = dict.insert("server_build", server_build);
    }

    // Monotonic world-generation counter (see snapshot.fbs `worldEpoch`): 0 for the idle boot
    // app, 1 for the first real world, +1 on every rebuild (`new_game`/`ResetMap`). The client's
    // loading gate reveals the map only once a full snapshot arrives whose epoch exceeds the
    // last-revealed baseline, so a reconnecting client ignores the replayed pre-rebuild frame.
    let _ = dict.insert("world_epoch", header.worldEpoch() as i64);

    if let Some(sedentarization) = snapshot.subsistence().and_then(|s| s.sedentarization()) {
        let _ = dict.insert(
            "sedentarization",
            &sedentarization_to_array(sedentarization),
        );
    }

    if let Some(forage_patches) = snapshot.subsistence().and_then(|s| s.foragePatches()) {
        let _ = dict.insert("forage_patches", &forage_patches_to_array(forage_patches));
    }

    if let Some(intensification) = snapshot
        .subsistence()
        .and_then(|s| s.intensificationKnowledge())
    {
        let _ = dict.insert(
            "intensification_knowledge",
            &intensification_knowledge_to_array(intensification),
        );
    }

    if let Some(demographics) = snapshot.population().and_then(|s| s.demographics()) {
        let _ = dict.insert("demographics", &demographics_to_array(demographics));
    }

    if let Some(discovered_sites) = snapshot.knowledge().and_then(|s| s.discoveredSites()) {
        let _ = dict.insert(
            "discovered_sites",
            &discovered_sites_to_array(discovered_sites),
        );
    }

    if let Some(axis_bias) = snapshot.culture().and_then(|s| s.axisBias()) {
        let _ = dict.insert("axis_bias", &axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = snapshot.culture().and_then(|s| s.sentiment()) {
        let _ = dict.insert("sentiment", &sentiment_to_dict(sentiment));
    }

    if let Some(influencers) = snapshot.culture().and_then(|s| s.influencers()) {
        let _ = dict.insert("influencers", &influencers_to_array(influencers));
    }

    if let Some(ledger) = snapshot.governance().and_then(|s| s.corruption()) {
        let _ = dict.insert("corruption", &corruption_to_dict(ledger));
    }

    if let Some(populations) = snapshot.population().and_then(|s| s.populations()) {
        let _ = dict.insert("populations", &populations_to_array(populations));
    }

    if let Some(power_nodes) = snapshot.governance().and_then(|s| s.power()) {
        let _ = dict.insert("power_nodes", &power_nodes_to_array(power_nodes));
    }

    if let Some(power_metrics) = snapshot.governance().and_then(|s| s.powerMetrics()) {
        let _ = dict.insert("power_metrics", &power_metrics_to_dict(power_metrics));
    }

    if let Some(crisis) = snapshot.governance().and_then(|s| s.crisisTelemetry()) {
        let _ = dict.insert("crisis_telemetry", &crisis_telemetry_to_dict(crisis));
    }

    if let Some(crisis_overlay) = snapshot.governance().and_then(|s| s.crisisOverlay()) {
        let _ = dict.insert("crisis_overlay", &crisis_overlay_to_dict(crisis_overlay));
    }

    if let Some(trade_links) = snapshot.economy().and_then(|s| s.tradeLinks()) {
        let _ = dict.insert("trade_links", &trade_links_to_array(trade_links));
    }

    if let Some(definitions) = snapshot
        .knowledge()
        .and_then(|s| s.greatDiscoveryDefinitions())
    {
        let _ = dict.insert(
            "great_discovery_definitions",
            &great_discovery_definitions_to_array(definitions),
        );
    }

    if let Some(great_discoveries) = snapshot.knowledge().and_then(|s| s.greatDiscoveries()) {
        let _ = dict.insert(
            "great_discoveries",
            &great_discovery_states_to_array(great_discoveries),
        );
    }

    if let Some(great_progress) = snapshot
        .knowledge()
        .and_then(|s| s.greatDiscoveryProgress())
    {
        let _ = dict.insert(
            "great_discovery_progress",
            &great_discovery_progress_states_to_array(great_progress),
        );
    }

    if let Some(gd_telemetry) = snapshot
        .knowledge()
        .and_then(|s| s.greatDiscoveryTelemetry())
    {
        let _ = dict.insert(
            "great_discovery_telemetry",
            &great_discovery_telemetry_to_dict(gd_telemetry),
        );
    }

    if let Some(tiles_fb) = snapshot.map().and_then(|s| s.tiles()) {
        let _ = dict.insert("tiles", &tiles_to_array(tiles_fb));
    }

    if let Some(generations) = snapshot.population().and_then(|s| s.generations()) {
        let _ = dict.insert("generations", &generations_to_array(generations));
    }

    if let Some(layers) = snapshot.culture().and_then(|s| s.cultureLayers()) {
        let _ = dict.insert("culture_layers", &culture_layers_to_array(layers));
    }

    if let Some(tensions) = snapshot.culture().and_then(|s| s.cultureTensions()) {
        let _ = dict.insert("culture_tensions", &culture_tensions_to_array(tensions));
    }

    if let Some(progress) = snapshot.knowledge().and_then(|s| s.discoveryProgress()) {
        let _ = dict.insert("discovery_progress", &discovery_progress_to_array(progress));
    }

    dict
}
