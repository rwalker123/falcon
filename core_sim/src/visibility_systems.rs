//! Visibility system implementations for the Fog of War.
//!
//! Systems run in sequence during TurnStage::Visibility:
//! 1. `clear_active_visibility` - Reset Active tiles to Discovered
//! 2. `prune_sweep_tracker` - Forget sweep positions of despawned cohorts
//! 3. `calculate_visibility` - Compute visibility from all sources
//! 4. `apply_trade_route_visibility` - Mark trade route tiles as Active
//! 5. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored

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

/// Meters of real relief spanned by the normalized (0.0–1.0) elevation field.
/// Used to convert normalized elevation to meters for the sight bonus.
const ELEVATION_RANGE_METERS: f32 = 1000.0;

/// Meters of elevation per step of the `elevation.bonus_per_100m` sight bonus.
/// Named to keep this coupled, and consistent, with that config field.
const METERS_PER_ELEVATION_BONUS_STEP: f32 = 100.0;

/// A unit always retains at least this much sight range after terrain penalties,
/// so it can never be blinded to its own tile/ring.
const MIN_EFFECTIVE_SIGHT_RANGE: i32 = 1;

/// Squared offset-space distance at or below which no tile can lie *between* the
/// viewer and the target, so the line-of-sight ray-cast is skipped. `dist² ≤ 2`
/// covers the eight immediate neighbours (orthogonal `dist²=1`, diagonal `dist²=2`).
const ADJACENT_LOS_SKIP_DIST_SQ: i32 = 2;

use sim_runtime::TerrainTags;

use crate::{
    components::{
        Expedition, LaborAllocation, LaborTarget, LogisticsLink, PopulationCohort, Settlement,
        StartingUnit, Tile, TownCenter, TradeLink,
    },
    fauna::HerdRegistry,
    grid_utils::{hex_neighbor, shortest_delta_x, wrap_x, wrapped_distance_x, HEX_DIRECTION_COUNT},
    heightfield::ElevationField,
    labor_config::LaborConfigHandle,
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

/// Forget sweep-tracker positions for cohorts that despawned since last turn.
///
/// Drains `RemovedComponents<StartingUnit>` (fired when a visibility-source cohort
/// is despawned — e.g. Founders consumed on settlement founding — or loses its
/// marker), removing exactly those entities from `VisibilitySweepTracker`. This is
/// cheaper than rebuilding a live-set each turn: it touches only the entities that
/// actually went away, keeping the tracker from accumulating stale entries over a
/// long-running sim. Runs before `calculate_visibility`, though ordering is not
/// load-bearing — the sweep only reads `previous()` for live cohorts, so a stale
/// entry is never read, merely wastes memory until drained.
pub fn prune_sweep_tracker(
    mut sweep: ResMut<VisibilitySweepTracker>,
    mut removed_sources: RemovedComponents<StartingUnit>,
) {
    for entity in removed_sources.read() {
        sweep.forget(entity);
    }
}

/// Step 2: Calculate visibility from all visibility sources (units, settlements).
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn calculate_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    mut sweep: ResMut<VisibilitySweepTracker>,
    config: Res<VisibilityConfigHandle>,
    labor_config: Res<LaborConfigHandle>,
    sim_config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    elevation: Option<Res<ElevationField>>,
    // Resolves a Hunt assignment's herd id to the herd's current tile (a worked visibility source).
    herds: Res<HerdRegistry>,
    tiles: Query<&Tile>,
    // Population cohorts with StartingUnit marker for unit type. The optional LaborAllocation
    // supplies the band's Scout head-count, which posts forward-observer vantages (see below).
    // `Without<Expedition>`: a detached expedition keeps `StartingUnit` (for move_band retargeting +
    // selection) but is deliberately NOT a live faction vision source — comm-range gating means it
    // must not light up the faction map from wherever it stands. It observes into its own
    // pending-reveal buffer and `advance_expeditions` flushes that on a comm-range delay.
    cohorts: Query<
        (
            Entity,
            &PopulationCohort,
            &StartingUnit,
            Option<&LaborAllocation>,
        ),
        Without<Expedition>,
    >,
    // Settlements with TownCenter
    settlements: Query<(&Settlement, &TownCenter)>,
) {
    let cfg = config.0.as_ref();
    let labor = labor_config.get();
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
    for (entity, cohort, unit, allocation) in cohorts.iter() {
        cohort_count += 1;
        let range_def = cfg.sight_range_for(&unit.kind);
        // The band's own base-range LOS from its center is unchanged; scouts are additive
        // forward observers layered on top (posted below).
        let base_range = range_def.base_range;
        // Get position from current tile (tracks travel position)
        if let Ok(current_tile) = tiles.get(cohort.current_tile) {
            let current_pos = current_tile.position;
            // A unit can move several tiles in one turn, so reveal every tile along
            // the corridor it swept from its previous position to the current one —
            // not just the endpoint — otherwise passed-over tiles stay Unexplored.
            let path = match sweep.previous(entity) {
                Some(prev) if prev != current_pos => corridor_tiles(
                    prev,
                    current_pos,
                    width,
                    height,
                    wrap_horizontal,
                    cfg.movement.max_sweep_tiles,
                ),
                _ => vec![current_pos],
            };
            for pos in path {
                sources.push((
                    cohort.faction,
                    pos,
                    base_range,
                    range_def.elevation_bonus_factor,
                ));
            }
            // Local scout (forward observers): with scouts staffed, post vantage tiles out
            // from the band in every hex direction and reveal from each with `vantage_range`,
            // so scouts see *around* obstacles (ridges/forest) rather than merely farther. The
            // vantages ride the SAME per-source LOS reveal below (elevation/terrain modifiers
            // included) — no separate raycast.
            let scout_workers = allocation
                .map(|alloc| alloc.workers_on(&LaborTarget::Scout))
                .unwrap_or(0);
            let vantage_distance = labor.scout.vantage_distance(scout_workers);
            if vantage_distance > 0 {
                for vantage in scout_vantage_tiles(
                    current_pos,
                    vantage_distance,
                    &terrain_tags,
                    width,
                    height,
                    wrap_horizontal,
                ) {
                    sources.push((
                        cohort.faction,
                        vantage,
                        labor.scout.vantage_range,
                        range_def.elevation_bonus_factor,
                    ));
                }
            }
            // Worked sources: a band's foragers stand on the forage tile and its hunters are
            // out at the herd, so those spots see fog too — additive to the band-center reveal
            // and scout vantages, riding the SAME per-source LOS path below at
            // `worked_source_sight_range`. All assignments carry ≥1 worker, so no head-count
            // gate is needed. Scout/Warrior are band-wide roles, not tile sources — skipped.
            if let Some(alloc) = allocation {
                for assignment in &alloc.assignments {
                    let worked_tile = match &assignment.target {
                        LaborTarget::Forage { tile, .. } => Some(*tile),
                        // Resolve the herd's live tile; a despawned/extinct herd yields no source.
                        LaborTarget::Hunt { fauna_id, .. } => {
                            herds.find(fauna_id).map(|herd| herd.position())
                        }
                        LaborTarget::Scout | LaborTarget::Warrior => None,
                    };
                    // A Forage assignment carries raw command-supplied coords (see
                    // `handle_assign_labor`), so guard the bounds before pushing — an OOB tile
                    // would panic in `reveal_tiles_in_range`'s `elevation.sample`. Herd tiles are
                    // always on-map, but this check is cheap and covers every worked source.
                    if let Some(tile) = worked_tile {
                        if tile.x < width && tile.y < height {
                            sources.push((
                                cohort.faction,
                                tile,
                                labor.worked_source_sight_range,
                                range_def.elevation_bonus_factor,
                            ));
                        }
                    }
                }
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
            let elevation_m = source_elevation * ELEVATION_RANGE_METERS;
            let bonus = (elevation_m / METERS_PER_ELEVATION_BONUS_STEP) as u32
                * cfg.elevation.bonus_per_100m;
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
/// inclusive of both ends. Used to reveal the corridor a unit sweeps when it moves
/// several tiles in one turn. Honors horizontal wrap via the shortest x-delta;
/// returns just `[to]` for a degenerate span, or one longer than `max_sweep_tiles`
/// — an implausible span from a wrap-seam artifact or a coordinate glitch, which
/// should not blow up into a huge corridor.
///
/// Uses integer supercover rasterization (all cells the segment passes through, not
/// just one per major-axis step) rather than float interpolation, so the tile set
/// is deterministic — no `f32` rounding — and never skips a crossed tile.
fn corridor_tiles(
    from: UVec2,
    to: UVec2,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
    max_sweep_tiles: u32,
) -> Vec<UVec2> {
    let dx = shortest_delta_x(from.x, to.x, width, wrap_horizontal);
    let dy = to.y as i32 - from.y as i32;
    let span = dx.abs().max(dy.abs());
    if span == 0 || span > max_sweep_tiles as i32 {
        return vec![to];
    }

    let max_y = height.saturating_sub(1) as i32;
    // 4-connected integer supercover from (from.x, from.y) to (from.x + dx, ...):
    // step exactly one axis per iteration, so consecutive cells are orthogonally
    // adjacent and no crossed cell is skipped. `x` stays "logical" (may run past the
    // grid edge) and is wrapped back on emit. Deterministic — pure integer math.
    let n_x = dx.abs();
    let n_y = dy.abs();
    let step_x = dx.signum();
    let step_y = dy.signum();
    let mut logical_x = from.x as i32;
    let mut y = from.y as i32;
    // Doubled error accumulator; ties (err == 0, exact diagonal crossings) step y.
    let mut err = n_x - n_y;
    let (n_x2, n_y2) = (n_x * 2, n_y * 2);

    let mut tiles = Vec::with_capacity((n_x + n_y) as usize + 1);
    let push = |lx: i32, gy: i32, out: &mut Vec<UVec2>| {
        let tile = UVec2::new(
            wrap_x(lx, width, wrap_horizontal),
            gy.clamp(0, max_y) as u32,
        );
        if out.last() != Some(&tile) {
            out.push(tile);
        }
    };

    for _ in 0..(n_x + n_y) {
        push(logical_x, y, &mut tiles);
        if err > 0 {
            logical_x += step_x;
            err -= n_y2;
        } else {
            y += step_y;
            err += n_x2;
        }
    }
    push(logical_x, y, &mut tiles);
    tiles
}

/// Post scout vantage tiles: step `vantage_distance` tiles out from `center` along each of
/// the six hex directions, pulling each vantage back to the last on-map, passable
/// (non-`WATER`) tile so foot scouts stop at ocean/edge instead of standing in the sea or
/// off the map. Returns the distinct vantages that lie off the band's own tile — a
/// direction the band can't step out at all (boxed in by water/edge) collapses to `center`
/// and is dropped as a no-op, and two directions pulling back to the same tile dedupe.
///
/// Reuses `hex_neighbor` for the single-direction walk (each step re-reads the current
/// tile's row parity), so the vantage line stays a proper odd-r hex ray — no bespoke
/// stepping. Bounded work: ≤ 6 directions × `vantage_distance` steps.
fn scout_vantage_tiles(
    center: UVec2,
    vantage_distance: u32,
    terrain_tags: &[TerrainTags],
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> Vec<UVec2> {
    let passable = |x: u32, y: u32| -> bool {
        let idx = (y * width + x) as usize;
        terrain_tags
            .get(idx)
            .map(|tags| !tags.contains(TerrainTags::WATER))
            .unwrap_or(false)
    };

    let mut vantages: Vec<UVec2> = Vec::with_capacity(HEX_DIRECTION_COUNT);
    for dir in 0..HEX_DIRECTION_COUNT {
        let mut cur = center;
        for _ in 0..vantage_distance {
            match hex_neighbor(cur.x, cur.y, dir, width, height, wrap_horizontal) {
                // Advance only onto an on-map, passable tile.
                Some((nx, ny)) if passable(nx, ny) => cur = UVec2::new(nx, ny),
                // Off-map (edge) or impassable (ocean): stop at the last good tile.
                _ => break,
            }
        }
        if cur != center && !vantages.contains(&cur) {
            vantages.push(cur);
        }
    }
    vantages
}

/// Build a grid of terrain tags from tile entities.
pub(crate) fn build_terrain_tags_grid(
    tiles: &Query<&Tile>,
    width: u32,
    height: u32,
) -> Vec<TerrainTags> {
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
    // Mark each visible tile Active **inline** — this runs for every vision source every turn, so
    // it must not allocate a Vec on the reveal hot path (the shared geometry hands us tiles via the
    // closure instead of collecting them).
    for_each_visible_tile_in_range(
        center,
        base_range,
        elevation,
        los_enabled,
        terrain_tags,
        terrain_modifiers,
        blocking_tags,
        wrap_horizontal,
        |pos| map.mark_active(pos.x, pos.y, current_turn),
    );
}

/// The tiles a source at `center` with `base_range` can see (elevation/terrain/LOS applied) — the
/// pure geometry behind [`reveal_tiles_in_range`], collected into a `Vec` so a caller can observe
/// **without** mutating a faction map. `advance_expeditions` uses this to accumulate a
/// comm-range-gated pending-reveal buffer (an expedition observes but does not live-reveal fog).
/// The allocation here is fine — this path is the (rare) expedition observation, not the per-source
/// reveal, which streams through [`for_each_visible_tile_in_range`] allocation-free.
// Args are inherent: it threads the LOS/elevation/terrain config through the shared reveal geometry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn visible_tiles_in_range(
    center: UVec2,
    base_range: u32,
    elevation: &ElevationField,
    los_enabled: bool,
    terrain_tags: &[TerrainTags],
    terrain_modifiers: &TerrainModifierConfig,
    blocking_tags: TerrainTags,
    wrap_horizontal: bool,
) -> Vec<UVec2> {
    let mut visible = Vec::new();
    for_each_visible_tile_in_range(
        center,
        base_range,
        elevation,
        los_enabled,
        terrain_tags,
        terrain_modifiers,
        blocking_tags,
        wrap_horizontal,
        |pos| visible.push(pos),
    );
    visible
}

/// Shared per-tile visibility geometry (elevation bounding box + circular range + terrain modifier +
/// LOS ray-cast), invoking `f(pos)` for each visible tile. Holds the geometry once so both the
/// allocation-free reveal path ([`reveal_tiles_in_range`], marks inline) and the collecting
/// observation path ([`visible_tiles_in_range`], pushes to a `Vec`) stay DRY.
// Args are inherent: it threads the LOS/elevation/terrain config through the shared reveal geometry.
#[allow(clippy::too_many_arguments)]
fn for_each_visible_tile_in_range(
    center: UVec2,
    base_range: u32,
    elevation: &ElevationField,
    los_enabled: bool,
    terrain_tags: &[TerrainTags],
    terrain_modifiers: &TerrainModifierConfig,
    blocking_tags: TerrainTags,
    wrap_horizontal: bool,
    mut f: impl FnMut(UVec2),
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
            let effective_range =
                (base_range as i32 + terrain_modifier).max(MIN_EFFECTIVE_SIGHT_RANGE) as u32;
            let range_sq = (effective_range * effective_range) as i32;

            // Skip tiles outside circular range (accounting for terrain modifier)
            if dist_sq > range_sq {
                continue;
            }

            // Line of sight check if enabled (skip for adjacent tiles - no intermediate blocker)
            if los_enabled
                && dist_sq > ADJACENT_LOS_SKIP_DIST_SQ
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

            // Tile is visible from this source.
            f(UVec2::new(x, y));
        }
    }
}

/// Convert string terrain tag names to a combined TerrainTags bitfield.
pub(crate) fn parse_blocking_tags(tag_names: &[String]) -> TerrainTags {
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
    fn prune_sweep_tracker_forgets_despawned_source() {
        use crate::components::StartingUnit;

        let mut world = World::new();
        let mut sweep = VisibilitySweepTracker::default();

        let live = world
            .spawn(StartingUnit::new("BandScout".to_string(), vec![]))
            .id();
        let despawned = world
            .spawn(StartingUnit::new("BandScout".to_string(), vec![]))
            .id();
        sweep.record(live, UVec2::new(1, 1));
        sweep.record(despawned, UVec2::new(2, 2));
        world.insert_resource(sweep);

        // Despawning removes StartingUnit, which the drain reads via RemovedComponents.
        world.entity_mut(despawned).despawn();

        let mut schedule = Schedule::default();
        schedule.add_systems(prune_sweep_tracker);
        schedule.run(&mut world);

        let sweep = world.resource::<VisibilitySweepTracker>();
        assert_eq!(sweep.previous(live), Some(UVec2::new(1, 1)));
        assert_eq!(sweep.previous(despawned), None);
    }

    #[test]
    fn scouts_reveal_around_obstacle() {
        use std::sync::Arc;

        use crate::components::{
            LaborAllocation, LaborTarget, LocalStore, MoraleCause, MoraleContributions,
            PopulationCohort,
        };
        use crate::labor_config::LaborConfigHandle;
        use crate::scalar::{scalar_from_f32, scalar_zero};
        use crate::visibility::VisibilityLedger;
        use crate::visibility_config::VisibilityConfig;

        const WIDTH: u32 = 40;
        const HEIGHT: u32 = 21;
        const BAND: UVec2 = UVec2::new(10, 10);
        // A blocking HIGHLAND ridge tile directly east of the band, on the sight line to
        // the target. Passable to foot scouts (not water) — a vantage steps *past* it.
        const RIDGE: UVec2 = UVec2::new(13, 10);
        // Target 5 tiles east: inside BandScout base range (6) but behind the ridge, so the
        // band's own center LOS can't see it. A scout vantage posted past the ridge can.
        const TARGET: UVec2 = UVec2::new(15, 10);

        // Returns the target tile's visibility state for a band staffing `scouts` workers.
        fn probe_state(scouts: u32) -> VisibilityState {
            let mut world = World::new();

            // Flat elevation (so only the terrain-tag ridge blocks) but LINE OF SIGHT ON, so
            // the ridge blocks the band's center view and only a forward vantage sees around it.
            let mut vis = VisibilityConfig::default();
            vis.elevation.enabled = false;
            vis.line_of_sight.enabled = true;
            world.insert_resource(VisibilityConfigHandle::new(Arc::new(vis)));
            world.insert_resource(LaborConfigHandle::default());

            let mut sim = SimulationConfig::builtin();
            sim.map_topology.wrap_horizontal = false;
            world.insert_resource(sim);
            world.insert_resource(SimulationTick(1));
            world.insert_resource(VisibilityLedger::default());
            world.insert_resource(VisibilitySweepTracker::default());
            world.insert_resource(HerdRegistry::default());
            world.insert_resource(ElevationField::new(
                WIDTH,
                HEIGHT,
                vec![0.5; (WIDTH * HEIGHT) as usize],
            ));

            let tile = world
                .spawn(Tile {
                    position: BAND,
                    ..Default::default()
                })
                .id();
            // The ridge: a HIGHLAND (default blocking) tile between band and target.
            world.spawn(Tile {
                position: RIDGE,
                terrain_tags: TerrainTags::HIGHLAND,
                ..Default::default()
            });

            let mut allocation = LaborAllocation::default();
            if scouts > 0 {
                allocation.set_assignment(LaborTarget::Scout, scouts, scouts);
            }

            world.spawn((
                PopulationCohort {
                    home: tile,
                    current_tile: tile,
                    size: 0,
                    children: scalar_zero(),
                    working: scalar_from_f32(scouts.max(1) as f32),
                    elders: scalar_zero(),
                    stores: LocalStore::new(),
                    morale: scalar_zero(),
                    last_morale_delta: scalar_zero(),
                    last_morale_cause: MoraleCause::None,
                    last_morale_contributions: MoraleContributions::default(),
                    discontent_fraction: scalar_zero(),
                    grievance: scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 10,
                    generation: 0,
                    faction: FactionId(0),
                    knowledge: Vec::new(),
                    migration: None,
                },
                StartingUnit::new("BandScout".to_string(), vec![]),
                allocation,
            ));

            let mut schedule = Schedule::default();
            schedule.add_systems(calculate_visibility);
            schedule.run(&mut world);

            world
                .resource::<VisibilityLedger>()
                .visibility_state(FactionId(0), TARGET.x, TARGET.y)
        }

        // 0 scouts: the ridge blocks the band's center LOS, so the target stays Unexplored —
        // extra *radius* alone could never see past it.
        assert_eq!(probe_state(0), VisibilityState::Unexplored);
        // 4 scouts: vantage_distance caps at 6, so the east vantage steps past the ridge and
        // reveals the target as Active — scouts seeing *around* the obstacle.
        assert_eq!(probe_state(4), VisibilityState::Active);
    }

    /// Runs `calculate_visibility` for a single BandCrafter band (base_range 2, no scouts) at
    /// `BAND`, given a `LaborAllocation` and any herds to seed into the registry, and returns
    /// the resulting ledger. A worked source reveals at `worked_source_sight_range` (2), while
    /// the band center's own reach (2) can't see the far worked tiles — so any reveal there is
    /// attributable to the worked source alone.
    #[cfg(test)]
    fn run_worked_visibility(
        allocation: LaborAllocation,
        herds: Vec<crate::fauna::Herd>,
    ) -> VisibilityLedger {
        use std::sync::Arc;

        use crate::components::{LocalStore, MoraleCause, MoraleContributions, PopulationCohort};
        use crate::labor_config::LaborConfigHandle;
        use crate::scalar::{scalar_from_f32, scalar_zero};
        use crate::visibility_config::VisibilityConfig;

        const WIDTH: u32 = 40;
        const HEIGHT: u32 = 21;
        const BAND: UVec2 = UVec2::new(20, 10);

        let mut world = World::new();

        // Flat elevation, LOS on but no blockers → clean reveal; isolates the worked source.
        let mut vis = VisibilityConfig::default();
        vis.elevation.enabled = false;
        vis.line_of_sight.enabled = true;
        world.insert_resource(VisibilityConfigHandle::new(Arc::new(vis)));
        world.insert_resource(LaborConfigHandle::default());

        let mut sim = SimulationConfig::builtin();
        sim.map_topology.wrap_horizontal = false;
        world.insert_resource(sim);
        world.insert_resource(SimulationTick(1));
        world.insert_resource(VisibilityLedger::default());
        world.insert_resource(VisibilitySweepTracker::default());
        world.insert_resource(HerdRegistry { herds });
        world.insert_resource(ElevationField::new(
            WIDTH,
            HEIGHT,
            vec![0.5; (WIDTH * HEIGHT) as usize],
        ));

        let tile = world
            .spawn(Tile {
                position: BAND,
                ..Default::default()
            })
            .id();

        world.spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 0,
                children: scalar_zero(),
                working: scalar_from_f32(4.0),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_zero(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: MoraleContributions::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 10,
                generation: 0,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            // BandCrafter: base_range 2, so the band center can't reveal the far worked tiles.
            StartingUnit::new("BandCrafter".to_string(), vec![]),
            allocation,
        ));

        let mut schedule = Schedule::default();
        schedule.add_systems(calculate_visibility);
        schedule.run(&mut world);
        world.remove_resource::<VisibilityLedger>().unwrap()
    }

    #[test]
    fn forage_worked_tile_provides_visibility() {
        use crate::components::{FollowPolicy, LaborAllocation, LaborTarget};

        // A forage tile 7 tiles north of the band — far beyond the band center's base_range (2),
        // so only the forager standing on it can reveal it.
        const FORAGE: UVec2 = UVec2::new(20, 3);

        // No assignment: the far forage tile is never revealed by the band center alone.
        let idle = run_worked_visibility(LaborAllocation::default(), Vec::new());
        assert_eq!(
            idle.visibility_state(FactionId(0), FORAGE.x, FORAGE.y),
            VisibilityState::Unexplored,
            "an unworked far tile is not a visibility source"
        );

        // Staff a forager on it: the worked tile and its immediate neighbors go Active.
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Forage {
                tile: FORAGE,
                policy: FollowPolicy::Sustain,
            },
            2,
            4,
        );
        let worked = run_worked_visibility(allocation, Vec::new());
        assert_eq!(
            worked.visibility_state(FactionId(0), FORAGE.x, FORAGE.y),
            VisibilityState::Active,
            "the worked forage tile reveals itself"
        );
        // A neighbor within worked_source_sight_range (2) is revealed too.
        assert_eq!(
            worked.visibility_state(FactionId(0), FORAGE.x, FORAGE.y + 1),
            VisibilityState::Active,
            "worked_source_sight_range reveals around the forage tile"
        );
    }

    #[test]
    fn hunt_worked_herd_tile_provides_visibility() {
        use crate::components::{FollowPolicy, LaborAllocation, LaborTarget};
        use crate::fauna::Herd;
        use crate::fauna_config::SizeClass;

        // A herd parked 7 tiles south of the band — beyond the band center's base_range (2).
        const HERD_TILE: UVec2 = UVec2::new(20, 17);
        let herd = Herd::new(
            "herd-1".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![HERD_TILE],
            100.0,
            100.0,
            0.0,
            0.05,
        );

        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Hunt {
                fauna_id: "herd-1".to_string(),
                policy: FollowPolicy::Sustain,
            },
            2,
            4,
        );
        let worked = run_worked_visibility(allocation, vec![herd]);
        assert_eq!(
            worked.visibility_state(FactionId(0), HERD_TILE.x, HERD_TILE.y),
            VisibilityState::Active,
            "the hunted herd's current tile is a visibility source"
        );
    }

    #[test]
    fn hunt_unresolved_herd_adds_no_source_and_does_not_panic() {
        use crate::components::{FollowPolicy, LaborAllocation, LaborTarget};

        // A Hunt on a herd id absent from the registry (despawned/extinct) must be skipped
        // silently — no source, no panic.
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Hunt {
                fauna_id: "ghost".to_string(),
                policy: FollowPolicy::Sustain,
            },
            2,
            4,
        );
        let ledger = run_worked_visibility(allocation, Vec::new());
        // The band center still reveals its own tile — the pass ran to completion.
        assert_eq!(
            ledger.visibility_state(FactionId(0), 20, 10),
            VisibilityState::Active,
        );
    }

    #[test]
    fn scout_vantage_tiles_pulls_back_from_water_and_edge() {
        // Band near a coast: a seaward/edge vantage must pull back to the last passable
        // on-map tile — never posted in the ocean or off the grid (no panic, no OOB).
        const WIDTH: u32 = 10;
        const HEIGHT: u32 = 10;
        let idx = |x: u32, y: u32| (y * WIDTH + x) as usize;
        let mut terrain_tags = vec![TerrainTags::empty(); (WIDTH * HEIGHT) as usize];
        // Flood the two westmost columns with ocean.
        for y in 0..HEIGHT {
            terrain_tags[idx(0, y)] = TerrainTags::WATER;
            terrain_tags[idx(1, y)] = TerrainTags::WATER;
        }

        let band = UVec2::new(3, 5);
        // A generous reach so vantages would run into the water / off the east edge if unclamped.
        let vantages = scout_vantage_tiles(band, 6, &terrain_tags, WIDTH, HEIGHT, false);

        assert!(
            !vantages.is_empty(),
            "band is not boxed in — some vantage posts"
        );
        for v in &vantages {
            assert!(v.x < WIDTH && v.y < HEIGHT, "vantage {v:?} is off-map");
            assert!(
                !terrain_tags[idx(v.x, v.y)].contains(TerrainTags::WATER),
                "vantage {v:?} was posted in the ocean"
            );
            assert_ne!(*v, band, "the band tile itself is not a vantage");
        }
    }

    #[test]
    fn scout_vantage_tiles_scale_with_distance() {
        // On open terrain the ring posts a vantage in every hex direction, and a larger
        // vantage_distance pushes them farther out from the band.
        const WIDTH: u32 = 40;
        const HEIGHT: u32 = 40;
        let terrain_tags = vec![TerrainTags::empty(); (WIDTH * HEIGHT) as usize];
        let band = UVec2::new(20, 20);

        let near = scout_vantage_tiles(band, 2, &terrain_tags, WIDTH, HEIGHT, false);
        let far = scout_vantage_tiles(band, 6, &terrain_tags, WIDTH, HEIGHT, false);

        assert_eq!(
            near.len(),
            HEX_DIRECTION_COUNT,
            "one vantage per hex direction"
        );
        assert_eq!(far.len(), HEX_DIRECTION_COUNT);

        let max_reach = |ring: &[UVec2]| -> u32 {
            ring.iter()
                .map(|v| v.x.abs_diff(band.x).max(v.y.abs_diff(band.y)))
                .max()
                .unwrap_or(0)
        };
        assert!(
            max_reach(&far) > max_reach(&near),
            "more scouts push the vantage ring farther out"
        );
    }

    #[test]
    fn corridor_tiles_covers_intermediate_tiles() {
        // A 3-tile horizontal move must reveal the two tiles it passed over, not
        // just the endpoint — this is the fog-of-war "teleport gap" fix.
        let path = corridor_tiles(UVec2::new(10, 5), UVec2::new(13, 5), 80, 40, false, 8);
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
    fn corridor_tiles_diagonal_is_4_connected() {
        // Diagonal move: supercover steps one axis at a time, so the corridor is a
        // contiguous staircase with no diagonal skips (each step is a neighbour).
        let path = corridor_tiles(UVec2::new(4, 4), UVec2::new(6, 6), 80, 40, false, 8);
        assert_eq!(
            path,
            vec![
                UVec2::new(4, 4),
                UVec2::new(4, 5),
                UVec2::new(5, 5),
                UVec2::new(5, 6),
                UVec2::new(6, 6),
            ]
        );
        for w in path.windows(2) {
            let manhattan = w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y);
            assert_eq!(manhattan, 1, "steps must be orthogonally adjacent");
        }
    }

    #[test]
    fn corridor_tiles_shallow_slope_covers_all_crossed_cells() {
        // Regression for the float-rounding gap: a shallow (0,0)->(2,1) segment must
        // include the corner cells it crosses, deterministically, with no skips.
        let path = corridor_tiles(UVec2::new(0, 0), UVec2::new(2, 1), 80, 40, false, 8);
        assert_eq!(
            path,
            vec![
                UVec2::new(0, 0),
                UVec2::new(1, 0),
                UVec2::new(1, 1),
                UVec2::new(2, 1),
            ]
        );
    }

    #[test]
    fn corridor_tiles_over_long_span_reveals_endpoint_only() {
        // A span beyond max_sweep_tiles (spurious/wrap-seam) collapses to the endpoint.
        let path = corridor_tiles(UVec2::new(0, 0), UVec2::new(40, 0), 80, 40, false, 8);
        assert_eq!(path, vec![UVec2::new(40, 0)]);
    }

    #[test]
    fn corridor_tiles_respects_config_cap() {
        // The cap is config-driven: a lower max_sweep_tiles collapses a span that
        // the default (8) would sweep, confirming the value is honored, not hardcoded.
        let path = corridor_tiles(UVec2::new(10, 5), UVec2::new(13, 5), 80, 40, false, 2);
        assert_eq!(path, vec![UVec2::new(13, 5)]);
    }

    #[test]
    fn corridor_tiles_wraps_horizontally() {
        // Moving from x=79 to x=1 on an 80-wide wrapped map goes the short way
        // across the seam (79 -> 0 -> 1), not the long way back across the map.
        let path = corridor_tiles(UVec2::new(79, 5), UVec2::new(1, 5), 80, 40, true, 8);
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
