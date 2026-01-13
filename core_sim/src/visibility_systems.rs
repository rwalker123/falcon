//! Visibility system implementations for the Fog of War.
//!
//! Three systems run in sequence during TurnStage::Visibility:
//! 1. `clear_active_visibility` - Reset Active tiles to Discovered
//! 2. `calculate_visibility` - Compute visibility from all sources
//! 3. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored

use bevy::prelude::*;

use sim_runtime::TerrainTags;

use crate::{
    components::{PopulationCohort, Settlement, StartingUnit, Tile, TownCenter},
    heightfield::ElevationField,
    orders::FactionId,
    resources::SimulationTick,
    visibility::{VisibilityLedger, VisibilityState},
    visibility_config::{TerrainModifierConfig, VisibilityConfigHandle},
};

/// Step 1: Clear all Active visibility states to Discovered at the start of the visibility phase.
pub fn clear_active_visibility(mut ledger: ResMut<VisibilityLedger>) {
    for faction in ledger.factions().collect::<Vec<_>>() {
        if let Some(map) = ledger.get_faction_mut(faction) {
            for (_, tile) in map.iter_tiles_mut() {
                if tile.state == VisibilityState::Active {
                    tile.state = VisibilityState::Discovered;
                }
            }
        }
    }
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
        None => return,
    };
    let elevation = elevation.unwrap();

    let _span = tracing::debug_span!(
        target: "shadow_scale::visibility",
        "calculate_visibility",
        turn = current_turn,
        los_enabled = cfg.line_of_sight.enabled,
    )
    .entered();

    // Build terrain tags grid for terrain modifier lookups
    let terrain_tags = build_terrain_tags_grid(&tiles, width, height);

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

    tracing::trace!(
        target: "shadow_scale::visibility",
        source_count = sources.len(),
        "visibility.sources_collected"
    );

    // Process each visibility source
    for (faction, pos, base_range, elev_factor) in sources {
        let map = ledger.ensure_faction(faction, width, height);
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
            pos,
            effective_range,
            current_turn,
            &elevation,
            cfg.line_of_sight.enabled,
            &terrain_tags,
            &cfg.terrain_modifiers,
        );
    }
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
                && !has_line_of_sight(center, UVec2::new(x, y), center_elevation, elevation)
            {
                continue;
            }

            // Mark tile as visible
            map.mark_active(x, y, current_turn);
        }
    }
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
fn has_line_of_sight(
    from: UVec2,
    to: UVec2,
    source_elevation: f32,
    elevation: &ElevationField,
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
    let viewer_height = source_elevation + 0.02;

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

        let intermediate_elevation = elevation.sample(current_x, current_y);

        // Calculate expected elevation at this point on the sight line
        let dist_from_source =
            (((x - from.x as i32).pow(2) + (y - from.y as i32).pow(2)) as f32).sqrt();
        let progress = dist_from_source / total_dist;
        let expected_elevation = viewer_height + (target_elevation - viewer_height) * progress;

        // If intermediate terrain is significantly higher than the sight line, it blocks
        if intermediate_elevation > expected_elevation + 0.03 {
            return false;
        }
    }

    true
}

/// Step 3: Apply visibility decay - tiles not seen for too long revert to unexplored.
pub fn apply_visibility_decay(
    mut ledger: ResMut<VisibilityLedger>,
    config: Res<VisibilityConfigHandle>,
    tick: Res<SimulationTick>,
) {
    let cfg = config.0.as_ref();
    if !cfg.decay.enabled {
        return;
    }

    let current_turn = tick.0;
    let decay_threshold = cfg.decay.threshold_turns;

    for faction in ledger.factions().collect::<Vec<_>>() {
        if let Some(map) = ledger.get_faction_mut(faction) {
            for (_, tile) in map.iter_tiles_mut() {
                if tile.state == VisibilityState::Discovered {
                    let turns_since_seen = current_turn.saturating_sub(tile.last_seen_turn);
                    if turns_since_seen >= decay_threshold {
                        tile.state = VisibilityState::Unexplored;
                        tile.last_seen_turn = 0;
                    }
                }
            }
        }
    }
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
        assert!(has_line_of_sight(
            UVec2::new(5, 5),
            UVec2::new(5, 5),
            0.5,
            &elevation
        ));
    }

    #[test]
    fn line_of_sight_flat_terrain() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        assert!(has_line_of_sight(
            UVec2::new(0, 0),
            UVec2::new(9, 9),
            0.5,
            &elevation
        ));
    }

    #[test]
    fn line_of_sight_blocked_by_mountain() {
        let mut values = vec![0.3; 100];
        // Create a high point in the middle
        values[55] = 0.9; // (5, 5)
        let elevation = ElevationField::new(10, 10, values);

        // Looking from (0, 5) to (9, 5) should be blocked by the mountain at (5, 5)
        assert!(!has_line_of_sight(
            UVec2::new(0, 5),
            UVec2::new(9, 5),
            0.3,
            &elevation
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
