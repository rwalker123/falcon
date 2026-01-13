//! Visibility system implementations for the Fog of War.
//!
//! Four systems run in sequence during TurnStage::Visibility:
//! 1. `clear_active_visibility` - Reset Active tiles to Discovered
//! 2. `calculate_visibility` - Compute visibility from all sources
//! 3. `apply_trade_route_visibility` - Mark trade route tiles as Active
//! 4. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored

use bevy::prelude::*;

/// Height bonus added to viewer position to simulate eye level above ground.
/// Value is in normalized elevation units (0.0-1.0 range maps to ~0-1000m).
/// 0.02 ≈ 20m eye level, reasonable for observers on elevated terrain or watchtowers.
const VIEWER_EYE_LEVEL_BONUS: f32 = 0.02;

/// Elevation threshold for terrain to block line of sight.
/// If intermediate terrain is this much higher than the expected sight line, it blocks.
/// Value is in normalized elevation units. 0.03 ≈ 30m provides some tolerance for
/// minor elevation variations while still blocking significant obstacles.
const LOS_BLOCKING_THRESHOLD: f32 = 0.03;

use sim_runtime::TerrainTags;

use crate::{
    components::{
        LogisticsLink, PopulationCohort, Settlement, StartingUnit, Tile, TownCenter, TradeLink,
    },
    heightfield::ElevationField,
    orders::FactionId,
    resources::SimulationTick,
    visibility::{VisibilityLedger, VisibilityState},
    visibility_config::{TerrainModifierConfig, VisibilityConfigHandle},
};

/// Step 1: Clear all Active visibility states to Discovered at the start of the visibility phase.
pub fn clear_active_visibility(mut ledger: ResMut<VisibilityLedger>) {
    let faction_count = ledger.factions().count();
    tracing::info!(
        target: "shadow_scale::visibility",
        faction_count,
        "visibility.step1_clear_active START"
    );

    for faction in ledger.factions().collect::<Vec<_>>() {
        if let Some(map) = ledger.get_faction_mut(faction) {
            for (_, tile) in map.iter_tiles_mut() {
                if tile.state == VisibilityState::Active {
                    tile.state = VisibilityState::Discovered;
                }
            }
        }
    }

    tracing::info!(
        target: "shadow_scale::visibility",
        "visibility.step1_clear_active END"
    );
}

/// Step 2: Calculate visibility from all visibility sources (units, settlements).
pub fn calculate_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    config: Res<VisibilityConfigHandle>,
    tick: Res<SimulationTick>,
    elevation: Option<Res<ElevationField>>,
    tiles: Query<&Tile>,
    // Population cohorts with StartingUnit marker for unit type
    cohorts: Query<(&PopulationCohort, &StartingUnit)>,
    // Settlements with TownCenter
    settlements: Query<(&Settlement, &TownCenter)>,
) {
    let cfg = config.0.as_ref();
    let current_turn = tick.0;

    // Get grid dimensions from elevation field or bail
    let (width, height) = match &elevation {
        Some(elev) => (elev.width, elev.height),
        None => {
            tracing::info!(
                target: "shadow_scale::visibility",
                "visibility.step2_calculate SKIPPED - no elevation field"
            );
            return;
        }
    };
    let elevation = elevation.unwrap();

    tracing::info!(
        target: "shadow_scale::visibility",
        width,
        height,
        turn = current_turn,
        "visibility.step2_calculate START"
    );

    let _span = tracing::debug_span!(
        target: "shadow_scale::visibility",
        "calculate_visibility",
        turn = current_turn,
        los_enabled = cfg.line_of_sight.enabled,
    )
    .entered();

    // Build terrain tags grid for terrain modifier lookups
    let terrain_tags = build_terrain_tags_grid(&tiles, width, height);

    // Parse blocking terrain tags from config (e.g., HIGHLAND, VOLCANIC)
    let blocking_tags = parse_blocking_tags(&cfg.line_of_sight.blocking_terrain_tags);

    // Collect all visibility sources: (faction, position, base_range, elev_factor)
    let mut sources: Vec<(FactionId, UVec2, u32, f32)> = Vec::new();

    // Units (population cohorts with StartingUnit marker)
    for (cohort, unit) in cohorts.iter() {
        let range_def = cfg.sight_range_for(&unit.kind);
        // Get position from home tile entity
        if let Ok(home_tile) = tiles.get(cohort.home) {
            sources.push((
                cohort.faction,
                home_tile.position,
                range_def.base_range,
                range_def.elevation_bonus_factor,
            ));
        }
    }

    // Settlements with TownCenter
    for (settlement, _town_center) in settlements.iter() {
        let range_def = cfg.sight_range_for("TownCenter");
        sources.push((
            settlement.faction,
            settlement.position,
            range_def.base_range,
            range_def.elevation_bonus_factor,
        ));
    }

    let cohort_count = cohorts.iter().count();
    let settlement_count = settlements.iter().count();
    tracing::info!(
        target: "shadow_scale::visibility",
        source_count = sources.len(),
        cohort_count,
        settlement_count,
        "visibility.step2_calculate sources_collected"
    );

    // Process each visibility source
    for (faction, pos, base_range, elev_factor) in sources.iter() {
        tracing::info!(
            target: "shadow_scale::visibility",
            faction = faction.0,
            pos_x = pos.x,
            pos_y = pos.y,
            base_range,
            "visibility.step2_calculate processing_source"
        );

        let map = ledger.ensure_faction(*faction, width, height);
        let source_elevation = elevation.sample(pos.x, pos.y);

        // Calculate effective range with elevation bonus
        let elev_bonus = if cfg.elevation.enabled {
            // Elevation is normalized 0-1, scale to approximate meters (0-1000m)
            let elevation_m = source_elevation * 1000.0;
            let bonus = (elevation_m / 100.0) as u32 * cfg.elevation.bonus_per_100m;
            ((bonus as f32) * elev_factor) as u32
        } else {
            0
        };
        // Cap elevation bonus before adding to base range
        let capped_bonus = elev_bonus.min(cfg.elevation.max_bonus);
        let effective_range = base_range + capped_bonus;

        // Reveal tiles in range
        reveal_tiles_in_range(
            map,
            *pos,
            effective_range,
            current_turn,
            &elevation,
            cfg.line_of_sight.enabled,
            &terrain_tags,
            &cfg.terrain_modifiers,
            blocking_tags,
        );
    }

    // Log visibility state after processing
    for faction in ledger.factions() {
        if let Some(map) = ledger.get_faction(faction) {
            let (unexplored, discovered, active) = map.count_by_state();
            tracing::info!(
                target: "shadow_scale::visibility",
                faction = faction.0,
                unexplored,
                discovered,
                active,
                "visibility.step2_calculate faction_state"
            );
        }
    }

    tracing::info!(
        target: "shadow_scale::visibility",
        "visibility.step2_calculate END"
    );
}

/// Build a grid of terrain tags from tile entities.
fn build_terrain_tags_grid(tiles: &Query<&Tile>, width: u32, height: u32) -> Vec<TerrainTags> {
    let mut grid = vec![TerrainTags::empty(); (width * height) as usize];
    for tile in tiles.iter() {
        let idx = (tile.position.y * width + tile.position.x) as usize;
        if idx < grid.len() {
            grid[idx] = tile.terrain_tags;
        }
    }
    grid
}

/// Reveal all tiles within range of a viewer position.
/// Applies terrain modifiers: forest tiles are harder to see (penalty), water tiles are easier (bonus).
///
/// # Performance Notes
///
/// Line-of-sight checks use Bresenham ray-casting which runs O(range) per tile.
/// For range 6, this means ~113 tiles × ~6 steps = ~680 ray steps per source.
///
/// If profiling shows this as a bottleneck with many units, consider:
/// - Shadow-casting algorithm (single pass from source, O(range²) total)
/// - Spatial caching of blocking tiles
///
/// Current optimizations:
/// - Adjacent tiles (dist ≤ 1) skip LoS check (no intermediate blocker possible)
/// - Range check before LoS check (avoids unnecessary ray-casts)
#[allow(clippy::too_many_arguments)]
fn reveal_tiles_in_range(
    map: &mut crate::visibility::FactionVisibilityMap,
    center: UVec2,
    base_range: u32,
    current_turn: u64,
    elevation: &ElevationField,
    los_enabled: bool,
    terrain_tags: &[TerrainTags],
    terrain_modifiers: &TerrainModifierConfig,
    blocking_tags: TerrainTags,
) {
    // Use max possible range for bounding box (base + max bonus)
    let max_range = base_range + terrain_modifiers.water_bonus.max(0) as u32;

    // Bounding box
    let min_x = center.x.saturating_sub(max_range);
    let max_x = (center.x + max_range).min(elevation.width.saturating_sub(1));
    let min_y = center.y.saturating_sub(max_range);
    let max_y = (center.y + max_range).min(elevation.height.saturating_sub(1));

    let center_elevation = elevation.sample(center.x, center.y);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as i32 - center.x as i32;
            let dy = y as i32 - center.y as i32;
            let dist_sq = dx * dx + dy * dy;

            // Calculate terrain modifier for target tile
            let idx = (y * elevation.width + x) as usize;
            let tags = terrain_tags
                .get(idx)
                .copied()
                .unwrap_or(TerrainTags::empty());
            let terrain_modifier = get_terrain_modifier(tags, terrain_modifiers);

            // Calculate effective range for this target tile
            let effective_range = (base_range as i32 + terrain_modifier).max(1) as u32;
            let range_sq = (effective_range * effective_range) as i32;

            // Skip tiles outside circular range (accounting for terrain modifier)
            if dist_sq > range_sq {
                continue;
            }

            // Line of sight check if enabled (skip for adjacent tiles - no intermediate blocker)
            if los_enabled
                && dist_sq > 2
                && !has_line_of_sight(
                    center,
                    UVec2::new(x, y),
                    center_elevation,
                    elevation,
                    terrain_tags,
                    blocking_tags,
                )
            {
                continue;
            }

            // Mark tile as visible
            map.mark_active(x, y, current_turn);
        }
    }
}

/// Convert string terrain tag names to a combined TerrainTags bitfield.
fn parse_blocking_tags(tag_names: &[String]) -> TerrainTags {
    let mut result = TerrainTags::empty();
    for name in tag_names {
        let tag = match name.as_str() {
            "HIGHLAND" => TerrainTags::HIGHLAND,
            "VOLCANIC" => TerrainTags::VOLCANIC,
            "WATER" => TerrainTags::WATER,
            "WETLAND" => TerrainTags::WETLAND,
            "FERTILE" => TerrainTags::FERTILE,
            "COASTAL" => TerrainTags::COASTAL,
            "POLAR" => TerrainTags::POLAR,
            "ARID" => TerrainTags::ARID,
            "HAZARDOUS" => TerrainTags::HAZARDOUS,
            _ => {
                tracing::warn!(
                    target: "shadow_scale::visibility",
                    tag = name,
                    "Unknown blocking terrain tag in config"
                );
                TerrainTags::empty()
            }
        };
        result |= tag;
    }
    result
}

/// Get terrain modifier for a tile based on its terrain tags.
/// - Forest/wetland tiles apply a penalty (harder to see into)
/// - Water tiles apply a bonus (easier to see across)
fn get_terrain_modifier(tags: TerrainTags, cfg: &TerrainModifierConfig) -> i32 {
    // Water bonus takes precedence (open water is easy to see across)
    if tags.contains(TerrainTags::WATER) {
        return cfg.water_bonus;
    }

    // Wetland/forest-like terrain applies penalty (foliage obscures vision)
    if tags.contains(TerrainTags::WETLAND) {
        return cfg.forest_penalty;
    }

    // No modifier for other terrain
    0
}

/// Check if there is clear line of sight between two points using Bresenham's algorithm.
/// Checks both elevation (terrain height) and blocking terrain tags (e.g., HIGHLAND, VOLCANIC).
fn has_line_of_sight(
    from: UVec2,
    to: UVec2,
    source_elevation: f32,
    elevation: &ElevationField,
    terrain_tags: &[TerrainTags],
    blocking_tags: TerrainTags,
) -> bool {
    // Skip if same tile
    if from == to {
        return true;
    }

    let dx = (to.x as i32 - from.x as i32).abs();
    let dy = (to.y as i32 - from.y as i32).abs();
    let sx = if from.x < to.x { 1i32 } else { -1i32 };
    let sy = if from.y < to.y { 1i32 } else { -1i32 };
    let mut err = dx - dy;

    let mut x = from.x as i32;
    let mut y = from.y as i32;
    let target_x = to.x as i32;
    let target_y = to.y as i32;

    let total_dist = ((dx * dx + dy * dy) as f32).sqrt().max(1.0);
    let target_elevation = elevation.sample(to.x, to.y);

    // Add slight height bonus to viewer (simulating eye level above ground)
    let viewer_height = source_elevation + VIEWER_EYE_LEVEL_BONUS;

    while x != target_x || y != target_y {
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        // Skip the target tile itself
        if x == target_x && y == target_y {
            break;
        }

        // Bounds check
        if x < 0 || y < 0 || x >= elevation.width as i32 || y >= elevation.height as i32 {
            continue;
        }

        let current_x = x as u32;
        let current_y = y as u32;

        // Check for blocking terrain tags (e.g., mountains, volcanoes)
        let idx = (current_y * elevation.width + current_x) as usize;
        if let Some(&tags) = terrain_tags.get(idx) {
            if (tags & blocking_tags).bits() != 0 {
                return false;
            }
        }

        let intermediate_elevation = elevation.sample(current_x, current_y);

        // Calculate expected elevation at this point on the sight line
        let dist_from_source =
            (((x - from.x as i32).pow(2) + (y - from.y as i32).pow(2)) as f32).sqrt();
        let progress = dist_from_source / total_dist;
        let expected_elevation = viewer_height + (target_elevation - viewer_height) * progress;

        // If intermediate terrain is significantly higher than the sight line, it blocks
        if intermediate_elevation > expected_elevation + LOS_BLOCKING_THRESHOLD {
            return false;
        }
    }

    true
}

/// Step 3: Mark tiles along trade routes as Active for visibility.
///
/// Trade routes provide visibility because merchants travel the path, reporting
/// what they see. Both endpoint factions of a trade link gain visibility of
/// all tiles along the route.
///
/// TradeLink components are only attached to LogisticsLinks that are part of
/// an active trade route, so this query only matches actual trade routes.
pub fn apply_trade_route_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    tick: Res<SimulationTick>,
    trade_links: Query<(&LogisticsLink, &TradeLink)>,
    tiles: Query<&Tile>,
) {
    let current_turn = tick.0;

    for (logistics, trade) in trade_links.iter() {
        // Get tile positions from logistics link endpoints
        let from_pos = tiles.get(logistics.from).ok().map(|t| t.position);
        let to_pos = tiles.get(logistics.to).ok().map(|t| t.position);

        // Mark tiles visible for the "from" faction
        if let Some(map) = ledger.get_faction_mut(trade.from_faction) {
            if let Some(pos) = from_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
            if let Some(pos) = to_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
        }

        // Mark tiles visible for the "to" faction (they see the route too)
        if trade.to_faction != trade.from_faction {
            if let Some(map) = ledger.get_faction_mut(trade.to_faction) {
                if let Some(pos) = from_pos {
                    map.mark_active(pos.x, pos.y, current_turn);
                }
                if let Some(pos) = to_pos {
                    map.mark_active(pos.x, pos.y, current_turn);
                }
            }
        }
    }
}

/// Step 4: Apply visibility decay - tiles not seen for too long revert to unexplored.
pub fn apply_visibility_decay(
    mut ledger: ResMut<VisibilityLedger>,
    config: Res<VisibilityConfigHandle>,
    tick: Res<SimulationTick>,
) {
    let cfg = config.0.as_ref();
    if !cfg.decay.enabled {
        tracing::info!(
            target: "shadow_scale::visibility",
            "visibility.step4_decay SKIPPED - decay disabled"
        );
        return;
    }

    let current_turn = tick.0;
    let decay_threshold = cfg.decay.threshold_turns;

    tracing::info!(
        target: "shadow_scale::visibility",
        current_turn,
        decay_threshold,
        "visibility.step4_decay START"
    );

    let mut decayed_count = 0u32;
    for faction in ledger.factions().collect::<Vec<_>>() {
        if let Some(map) = ledger.get_faction_mut(faction) {
            for (_, tile) in map.iter_tiles_mut() {
                if tile.state == VisibilityState::Discovered {
                    let turns_since_seen = current_turn.saturating_sub(tile.last_seen_turn);
                    if turns_since_seen >= decay_threshold {
                        tile.state = VisibilityState::Unexplored;
                        tile.last_seen_turn = 0;
                        decayed_count += 1;
                    }
                }
            }
        }
    }

    tracing::info!(
        target: "shadow_scale::visibility",
        decayed_count,
        "visibility.step4_decay END"
    );
}

/// Log visibility statistics for debugging.
#[allow(dead_code)]
pub fn log_visibility_stats(ledger: Res<VisibilityLedger>) {
    for faction in ledger.factions() {
        if let Some(map) = ledger.get_faction(faction) {
            let (unexplored, discovered, active) = map.count_by_state();
            tracing::debug!(
                target: "shadow_scale::visibility",
                faction = faction.0,
                unexplored = unexplored,
                discovered = discovered,
                active = active,
                "visibility.stats"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_of_sight_same_tile() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        let terrain_tags = vec![TerrainTags::empty(); 100];
        assert!(has_line_of_sight(
            UVec2::new(5, 5),
            UVec2::new(5, 5),
            0.5,
            &elevation,
            &terrain_tags,
            TerrainTags::empty(),
        ));
    }

    #[test]
    fn line_of_sight_flat_terrain() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        let terrain_tags = vec![TerrainTags::empty(); 100];
        assert!(has_line_of_sight(
            UVec2::new(0, 0),
            UVec2::new(9, 9),
            0.5,
            &elevation,
            &terrain_tags,
            TerrainTags::empty(),
        ));
    }

    #[test]
    fn line_of_sight_blocked_by_mountain() {
        let mut values = vec![0.3; 100];
        // Create a high point in the middle
        values[55] = 0.9; // (5, 5)
        let elevation = ElevationField::new(10, 10, values);
        let terrain_tags = vec![TerrainTags::empty(); 100];

        // Looking from (0, 5) to (9, 5) should be blocked by the mountain at (5, 5)
        assert!(!has_line_of_sight(
            UVec2::new(0, 5),
            UVec2::new(9, 5),
            0.3,
            &elevation,
            &terrain_tags,
            TerrainTags::empty(),
        ));
    }

    #[test]
    fn line_of_sight_blocked_by_terrain_tag() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        let mut terrain_tags = vec![TerrainTags::empty(); 100];
        // Place HIGHLAND terrain at (5, 5) - index 55
        terrain_tags[55] = TerrainTags::HIGHLAND;

        // Configure HIGHLAND as blocking
        let blocking_tags = TerrainTags::HIGHLAND;

        // Looking from (0, 5) to (9, 5) should be blocked by HIGHLAND at (5, 5)
        assert!(!has_line_of_sight(
            UVec2::new(0, 5),
            UVec2::new(9, 5),
            0.5,
            &elevation,
            &terrain_tags,
            blocking_tags,
        ));

        // Looking at the HIGHLAND tile itself should still work (target not blocked)
        assert!(has_line_of_sight(
            UVec2::new(0, 5),
            UVec2::new(5, 5),
            0.5,
            &elevation,
            &terrain_tags,
            blocking_tags,
        ));
    }

    #[test]
    fn terrain_modifier_water_bonus() {
        let cfg = TerrainModifierConfig {
            forest_penalty: -2,
            water_bonus: 1,
        };

        // Water tiles get bonus
        let water_tags = TerrainTags::WATER;
        assert_eq!(get_terrain_modifier(water_tags, &cfg), 1);

        // Wetland tiles get penalty
        let wetland_tags = TerrainTags::WETLAND;
        assert_eq!(get_terrain_modifier(wetland_tags, &cfg), -2);

        // Plain tiles get no modifier
        let plain_tags = TerrainTags::empty();
        assert_eq!(get_terrain_modifier(plain_tags, &cfg), 0);

        // Water takes precedence over wetland (coastal wetland)
        let coastal_wetland = TerrainTags::WATER | TerrainTags::WETLAND;
        assert_eq!(get_terrain_modifier(coastal_wetland, &cfg), 1);
    }
}
