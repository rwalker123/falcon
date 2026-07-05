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

/// Upper bound on how many tiles a single-turn move may sweep for visibility.
/// Normal moves cover ~3 tiles/turn; a span larger than this is treated as
/// spurious (e.g. a wrap-seam artifact or entity-id reuse) and only the endpoint
/// is revealed, guarding against pathological corridor blowups.
const MAX_SWEEP_TILES: i32 = 8;

use sim_runtime::TerrainTags;

use crate::{
    components::{
        LogisticsLink, PopulationCohort, Settlement, StartingUnit, Tile, TownCenter, TradeLink,
    },
    grid_utils::{shortest_delta_x, wrap_x, wrapped_distance_x},
    heightfield::ElevationField,
    orders::FactionId,
    resources::{SimulationConfig, SimulationTick},
    visibility::{VisibilityLedger, VisibilityState, VisibilitySweepTracker},
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
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn calculate_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    mut sweep: ResMut<VisibilitySweepTracker>,
    config: Res<VisibilityConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    elevation: Option<Res<ElevationField>>,
    tiles: Query<&Tile>,
    // Population cohorts with StartingUnit marker for unit type
    cohorts: Query<(Entity, &PopulationCohort, &StartingUnit)>,
    // Settlements with TownCenter
    settlements: Query<(&Settlement, &TownCenter)>,
) {
    let cfg = config.0.as_ref();
    let current_turn = tick.0;
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;

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
    let mut cohort_count = 0u32;
    let mut settlement_count = 0u32;

    // Units (population cohorts with StartingUnit marker)
    for (entity, cohort, unit) in cohorts.iter() {
        cohort_count += 1;
        let range_def = cfg.sight_range_for(&unit.kind);
        // Get position from current tile (tracks travel position)
        if let Ok(current_tile) = tiles.get(cohort.current_tile) {
            let current_pos = current_tile.position;
            // A unit can move several tiles in one turn, so reveal every tile along
            // the corridor it swept from its previous position to the current one —
            // not just the endpoint — otherwise passed-over tiles stay Unexplored.
            let path = match sweep.previous(entity) {
                Some(prev) if prev != current_pos => {
                    corridor_tiles(prev, current_pos, width, height, wrap_horizontal)
                }
                _ => vec![current_pos],
            };
            for pos in path {
                sources.push((
                    cohort.faction,
                    pos,
                    range_def.base_range,
                    range_def.elevation_bonus_factor,
                ));
            }
            sweep.record(entity, current_pos);
        }
    }

    // Settlements with TownCenter
    for (settlement, _town_center) in settlements.iter() {
        settlement_count += 1;
        let range_def = cfg.sight_range_for("TownCenter");
        sources.push((
            settlement.faction,
            settlement.position,
            range_def.base_range,
            range_def.elevation_bonus_factor,
        ));
    }

    tracing::info!(
        target: "shadow_scale::visibility",
        source_count = sources.len(),
        cohort_count,
        settlement_count,
        "visibility.step2_calculate sources_collected"
    );

    // Process each visibility source
    for (faction, pos, base_range, elev_factor) in sources.iter() {
        tracing::debug!(
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
            wrap_horizontal,
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

/// Tiles crossed by the straight offset-space segment from `from` to `to`,
/// inclusive of both ends and de-duplicated. Used to reveal the corridor a unit
/// sweeps when it moves several tiles in one turn. Honors horizontal wrap via the
/// shortest x-delta; returns just `[to]` for a degenerate or over-long span (see
/// `MAX_SWEEP_TILES`).
fn corridor_tiles(
    from: UVec2,
    to: UVec2,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> Vec<UVec2> {
    let dx = shortest_delta_x(from.x, to.x, width, wrap_horizontal);
    let dy = to.y as i32 - from.y as i32;
    let steps = dx.abs().max(dy.abs());
    if steps == 0 || steps > MAX_SWEEP_TILES {
        return vec![to];
    }
    let max_y = height.saturating_sub(1) as i32;
    let mut tiles = Vec::with_capacity(steps as usize + 1);
    let mut last: Option<UVec2> = None;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = wrap_x(
            (from.x as f32 + dx as f32 * t).round() as i32,
            width,
            wrap_horizontal,
        );
        let y = ((from.y as f32 + dy as f32 * t).round() as i32).clamp(0, max_y) as u32;
        let tile = UVec2::new(x, y);
        if last != Some(tile) {
            tiles.push(tile);
            last = Some(tile);
        }
    }
    tiles
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
/// Supports horizontal wrapping for cylindrical world topology.
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
    wrap_horizontal: bool,
) {
    let width = elevation.width;
    let height = elevation.height;

    // Use max possible range for bounding box (base + max bonus)
    let max_range = base_range + terrain_modifiers.water_bonus.max(0) as u32;

    // Y bounds (no vertical wrap)
    let min_y = center.y.saturating_sub(max_range);
    let max_y = (center.y + max_range).min(height.saturating_sub(1));

    // X iteration range (may extend beyond grid bounds when wrapping)
    let (x_start, x_end) = if wrap_horizontal {
        // Iterate using signed offsets to handle wrap correctly
        let range_i = max_range as i32;
        (-(range_i), range_i)
    } else {
        // Clamp to grid bounds
        let min_x = center.x.saturating_sub(max_range) as i32;
        let max_x = (center.x + max_range).min(width.saturating_sub(1)) as i32;
        (min_x - center.x as i32, max_x - center.x as i32)
    };

    let center_elevation = elevation.sample(center.x, center.y);

    for y in min_y..=max_y {
        for dx in x_start..=x_end {
            // Calculate actual x coordinate, wrapping if needed
            let x = if wrap_horizontal {
                wrap_x(center.x as i32 + dx, width, true)
            } else {
                let raw_x = center.x as i32 + dx;
                if raw_x < 0 || raw_x >= width as i32 {
                    continue;
                }
                raw_x as u32
            };

            // Calculate distance using wrapped distance for X
            let actual_dx = wrapped_distance_x(center.x, x, width, wrap_horizontal) as i32;
            let dy = y as i32 - center.y as i32;
            let dist_sq = actual_dx * actual_dx + dy * dy;

            // Calculate terrain modifier for target tile
            let idx = (y * width + x) as usize;
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
                && !has_line_of_sight_wrapped(
                    center,
                    UVec2::new(x, y),
                    center_elevation,
                    &LineOfSightCtx {
                        elevation,
                        terrain_tags,
                        blocking_tags,
                        wrap_horizontal,
                    },
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

/// Terrain data and blocking rules that stay constant across every ray cast in a
/// visibility pass. Bundled into one parameter so the LoS query stays under Clippy's
/// argument limit without suppressing the lint.
struct LineOfSightCtx<'a> {
    elevation: &'a ElevationField,
    terrain_tags: &'a [TerrainTags],
    blocking_tags: TerrainTags,
    wrap_horizontal: bool,
}

/// Check if there is clear line of sight between two points using Bresenham's algorithm.
/// Checks both elevation (terrain height) and blocking terrain tags (e.g., HIGHLAND, VOLCANIC).
/// Supports horizontal wrapping for cylindrical world topology.
fn has_line_of_sight_wrapped(
    from: UVec2,
    to: UVec2,
    source_elevation: f32,
    ctx: &LineOfSightCtx,
) -> bool {
    // Skip if same tile
    if from == to {
        return true;
    }

    let width = ctx.elevation.width;

    // Calculate deltas using shortest path (may go through wrap boundary)
    let delta_x = if ctx.wrap_horizontal {
        shortest_delta_x(from.x, to.x, width, true)
    } else {
        to.x as i32 - from.x as i32
    };
    let delta_y = to.y as i32 - from.y as i32;

    let dx = delta_x.abs();
    let dy = delta_y.abs();
    let sx = if delta_x > 0 { 1i32 } else { -1i32 };
    let sy = if delta_y > 0 { 1i32 } else { -1i32 };
    let mut err = dx - dy;

    // Use logical coordinates for Bresenham (may go negative or beyond width when wrapping)
    let mut logical_x = from.x as i32;
    let mut y = from.y as i32;
    let target_logical_x = from.x as i32 + delta_x;
    let target_y = to.y as i32;

    let total_dist = ((dx * dx + dy * dy) as f32).sqrt().max(1.0);
    let target_elevation = ctx.elevation.sample(to.x, to.y);

    // Add slight height bonus to viewer (simulating eye level above ground)
    let viewer_height = source_elevation + VIEWER_EYE_LEVEL_BONUS;

    while logical_x != target_logical_x || y != target_y {
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            logical_x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        // Skip the target tile itself
        if logical_x == target_logical_x && y == target_y {
            break;
        }

        // Y bounds check (no vertical wrap)
        if y < 0 || y >= ctx.elevation.height as i32 {
            continue;
        }

        // Wrap X coordinate to get actual grid position
        let current_x = wrap_x(logical_x, width, ctx.wrap_horizontal);
        let current_y = y as u32;

        // Check for blocking terrain tags (e.g., mountains, volcanoes)
        let idx = (current_y * width + current_x) as usize;
        if let Some(&tags) = ctx.terrain_tags.get(idx) {
            if (tags & ctx.blocking_tags).bits() != 0 {
                return false;
            }
        }

        let intermediate_elevation = ctx.elevation.sample(current_x, current_y);

        // Calculate expected elevation at this point on the sight line
        // Use logical distance from source for progress calculation
        let logical_dx = logical_x - from.x as i32;
        let logical_dy = y - from.y as i32;
        let dist_from_source = ((logical_dx * logical_dx + logical_dy * logical_dy) as f32).sqrt();
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
    fn corridor_tiles_covers_intermediate_tiles() {
        // A 3-tile horizontal move must reveal the two tiles it passed over, not
        // just the endpoint — this is the fog-of-war "teleport gap" fix.
        let path = corridor_tiles(UVec2::new(10, 5), UVec2::new(13, 5), 80, 40, false);
        assert_eq!(
            path,
            vec![
                UVec2::new(10, 5),
                UVec2::new(11, 5),
                UVec2::new(12, 5),
                UVec2::new(13, 5),
            ]
        );
    }

    #[test]
    fn corridor_tiles_diagonal_and_dedup() {
        // Diagonal move: one tile per step, endpoints included, no duplicates.
        let path = corridor_tiles(UVec2::new(4, 4), UVec2::new(6, 6), 80, 40, false);
        assert_eq!(path.first(), Some(&UVec2::new(4, 4)));
        assert_eq!(path.last(), Some(&UVec2::new(6, 6)));
        assert_eq!(path.len(), 3);
        for w in path.windows(2) {
            assert_ne!(w[0], w[1]);
        }
    }

    #[test]
    fn corridor_tiles_over_long_span_reveals_endpoint_only() {
        // A span beyond MAX_SWEEP_TILES (spurious/wrap-seam) collapses to the endpoint.
        let path = corridor_tiles(UVec2::new(0, 0), UVec2::new(40, 0), 80, 40, false);
        assert_eq!(path, vec![UVec2::new(40, 0)]);
    }

    #[test]
    fn corridor_tiles_wraps_horizontally() {
        // Moving from x=79 to x=1 on an 80-wide wrapped map goes the short way
        // across the seam (79 -> 0 -> 1), not the long way back across the map.
        let path = corridor_tiles(UVec2::new(79, 5), UVec2::new(1, 5), 80, 40, true);
        assert_eq!(
            path,
            vec![UVec2::new(79, 5), UVec2::new(0, 5), UVec2::new(1, 5)]
        );
    }

    #[test]
    fn line_of_sight_same_tile() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        let terrain_tags = vec![TerrainTags::empty(); 100];
        assert!(has_line_of_sight_wrapped(
            UVec2::new(5, 5),
            UVec2::new(5, 5),
            0.5,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags: TerrainTags::empty(),
                wrap_horizontal: false,
            },
        ));
    }

    #[test]
    fn line_of_sight_flat_terrain() {
        let elevation = ElevationField::new(10, 10, vec![0.5; 100]);
        let terrain_tags = vec![TerrainTags::empty(); 100];
        assert!(has_line_of_sight_wrapped(
            UVec2::new(0, 0),
            UVec2::new(9, 9),
            0.5,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags: TerrainTags::empty(),
                wrap_horizontal: false,
            },
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
        assert!(!has_line_of_sight_wrapped(
            UVec2::new(0, 5),
            UVec2::new(9, 5),
            0.3,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags: TerrainTags::empty(),
                wrap_horizontal: false,
            },
        ));
    }

    #[test]
    fn line_of_sight_blocked_by_terrain_tag() {
        const WIDTH: u32 = 10;
        const HEIGHT: u32 = 10;
        let tile_count = (WIDTH * HEIGHT) as usize;
        let idx = |x: u32, y: u32| (y * WIDTH + x) as usize;

        let elevation = ElevationField::new(WIDTH, HEIGHT, vec![0.5; tile_count]);
        let mut terrain_tags = vec![TerrainTags::empty(); tile_count];
        // Place HIGHLAND terrain at (5, 5).
        terrain_tags[idx(5, 5)] = TerrainTags::HIGHLAND;

        // Configure HIGHLAND as blocking
        let blocking_tags = TerrainTags::HIGHLAND;

        // Looking from (0, 5) to (9, 5) should be blocked by HIGHLAND at (5, 5)
        assert!(!has_line_of_sight_wrapped(
            UVec2::new(0, 5),
            UVec2::new(9, 5),
            0.5,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags,
                wrap_horizontal: false,
            },
        ));

        // Looking at the HIGHLAND tile itself should still work (target not blocked)
        assert!(has_line_of_sight_wrapped(
            UVec2::new(0, 5),
            UVec2::new(5, 5),
            0.5,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags,
                wrap_horizontal: false,
            },
        ));
    }

    #[test]
    fn line_of_sight_across_wrap_boundary() {
        const WIDTH: u32 = 20;
        const HEIGHT: u32 = 10;
        let tile_count = (WIDTH * HEIGHT) as usize;

        // 20-wide grid, looking from x=2 to x=18 (via wrap: 2->1->0->19->18 = distance 4)
        let elevation = ElevationField::new(WIDTH, HEIGHT, vec![0.5; tile_count]);
        let terrain_tags = vec![TerrainTags::empty(); tile_count];

        // With wrapping, should have clear LoS across the boundary
        assert!(has_line_of_sight_wrapped(
            UVec2::new(2, 5),
            UVec2::new(18, 5),
            0.5,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags: TerrainTags::empty(),
                wrap_horizontal: true,
            },
        ));
    }

    #[test]
    fn line_of_sight_blocked_across_wrap() {
        const WIDTH: u32 = 20;
        const HEIGHT: u32 = 10;
        let tile_count = (WIDTH * HEIGHT) as usize;
        let idx = |x: u32, y: u32| (y * WIDTH + x) as usize;

        // Place a mountain at (0, 5), on the wrap boundary path.
        let mut values = vec![0.3; tile_count];
        values[idx(0, 5)] = 0.9;
        let elevation = ElevationField::new(WIDTH, HEIGHT, values);
        let terrain_tags = vec![TerrainTags::empty(); tile_count];

        // Looking from x=2 to x=18 via wrap should be blocked by mountain at x=0
        assert!(!has_line_of_sight_wrapped(
            UVec2::new(2, 5),
            UVec2::new(18, 5),
            0.3,
            &LineOfSightCtx {
                elevation: &elevation,
                terrain_tags: &terrain_tags,
                blocking_tags: TerrainTags::empty(),
                wrap_horizontal: true,
            },
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
