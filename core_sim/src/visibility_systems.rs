//! Visibility system implementations for the Fog of War.
//!
//! Three systems run in sequence during TurnStage::Visibility:
//! 1. `clear_active_visibility` - Reset Active tiles to Discovered
//! 2. `calculate_visibility` - Compute visibility from all sources
//! 3. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored

use bevy::prelude::*;

use crate::{
    components::{PopulationCohort, Settlement, StartingUnit, Tile, TownCenter},
    heightfield::ElevationField,
    orders::FactionId,
    resources::SimulationTick,
    visibility::{VisibilityLedger, VisibilityState},
    visibility_config::VisibilityConfigHandle,
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
        let effective_range = (base_range + elev_bonus).min(base_range + cfg.elevation.max_bonus);

        // Reveal tiles in range
        reveal_tiles_in_range(
            map,
            pos,
            effective_range,
            current_turn,
            &elevation,
            cfg.line_of_sight.enabled,
        );
    }
}

/// Reveal all tiles within range of a viewer position.
fn reveal_tiles_in_range(
    map: &mut crate::visibility::FactionVisibilityMap,
    center: UVec2,
    range: u32,
    current_turn: u64,
    elevation: &ElevationField,
    los_enabled: bool,
) {
    let range_sq = (range * range) as i32;

    // Bounding box
    let min_x = center.x.saturating_sub(range);
    let max_x = (center.x + range).min(elevation.width.saturating_sub(1));
    let min_y = center.y.saturating_sub(range);
    let max_y = (center.y + range).min(elevation.height.saturating_sub(1));

    let center_elevation = elevation.sample(center.x, center.y);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as i32 - center.x as i32;
            let dy = y as i32 - center.y as i32;
            let dist_sq = dx * dx + dy * dy;

            // Skip tiles outside circular range
            if dist_sq > range_sq {
                continue;
            }

            // Line of sight check if enabled
            if los_enabled
                && !has_line_of_sight(center, UVec2::new(x, y), center_elevation, elevation)
            {
                continue;
            }

            // Mark tile as visible
            map.mark_active(x, y, current_turn);
        }
    }
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
}
