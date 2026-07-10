//! Grid utility functions for coordinate wrapping and distance calculations.
//!
//! Supports cylindrical world topology where east/west edges connect seamlessly
//! while north/south poles remain hard boundaries.
//!
//! # Coordinate System
//!
//! Uses odd-r offset coordinates (pointy-top hexes, odd rows shifted right).
//! Position (x, y) maps to array index y * width + x.

use bevy::prelude::UVec2;

/// Wrap an x-coordinate into the range [0, width).
///
/// When wrapping is disabled, the coordinate is clamped to valid bounds.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(wrap_x(-1, 80, true), 79);   // wraps left edge to right
/// assert_eq!(wrap_x(80, 80, true), 0);    // wraps right edge to left
/// assert_eq!(wrap_x(40, 80, true), 40);   // no change for valid coord
/// assert_eq!(wrap_x(-1, 80, false), 0);   // clamps to 0 when not wrapping
/// ```
#[inline]
pub fn wrap_x(x: i32, width: u32, wrap: bool) -> u32 {
    let w = width as i32;
    if wrap {
        // Rust's % can return negative for negative x, so use rem_euclid
        x.rem_euclid(w) as u32
    } else {
        x.clamp(0, w.saturating_sub(1)) as u32
    }
}

/// Compute the shortest distance between two x-coordinates, considering wrap.
///
/// When wrapping is enabled, returns the minimum of the direct distance
/// and the distance going through the wrap boundary.
///
/// # Examples
///
/// ```ignore
/// // Direct distance
/// assert_eq!(wrapped_distance_x(10, 20, 80, true), 10);
/// // Wrapped distance is shorter (79 -> 1 via wrap = 2, not 78 direct)
/// assert_eq!(wrapped_distance_x(79, 1, 80, true), 2);
/// ```
#[inline]
pub fn wrapped_distance_x(x1: u32, x2: u32, width: u32, wrap: bool) -> u32 {
    let direct = (x1 as i32 - x2 as i32).unsigned_abs();
    if wrap {
        let wrapped = width - direct;
        direct.min(wrapped)
    } else {
        direct
    }
}

/// Compute the signed delta to move from x1 to x2 via the shortest path.
///
/// Returns a value in the range (-width/2, width/2] when wrapping is enabled.
/// Positive values mean move right (increasing x), negative means move left.
///
/// This is useful for Bresenham line-drawing and pathfinding across the wrap boundary.
#[inline]
pub fn shortest_delta_x(x1: u32, x2: u32, width: u32, wrap: bool) -> i32 {
    let direct = x2 as i32 - x1 as i32;
    if !wrap {
        return direct;
    }

    let w = width as i32;
    // If direct path is within half the width, use it
    if direct.abs() <= w / 2 {
        direct
    } else if direct > 0 {
        // Target is to the right, but wrapping left is shorter
        direct - w
    } else {
        // Target is to the left, but wrapping right is shorter
        direct + w
    }
}

/// Compute squared Euclidean distance between two points, considering horizontal wrap.
///
/// Y-axis does not wrap (poles are hard boundaries).
///
/// Returns i32 to match existing codebase patterns where distance squared
/// is compared against range squared.
#[inline]
pub fn wrapped_distance_sq(from: UVec2, to: UVec2, width: u32, wrap_horizontal: bool) -> i32 {
    let dx = wrapped_distance_x(from.x, to.x, width, wrap_horizontal) as i32;
    let dy = (from.y as i32 - to.y as i32).abs();
    dx * dx + dy * dy
}

/// Convert odd-r offset coordinates `(col, row)` to axial `(q, r)`.
///
/// Matches the **odd-r** convention `hex_neighbor` steps in (pointy-top, odd rows shoved
/// right — see `HEX_NEIGHBOR_OFFSETS`), so a hex distance computed from these agrees exactly
/// with the six single-step neighbors. `col` may be negative (a wrapped column brought into a
/// reference tile's local frame); `(row - (row & 1))` is even and non-negative, so the `/ 2`
/// is exact regardless of `col`'s sign.
#[inline]
fn offset_to_axial(col: i32, row: i32) -> (i32, i32) {
    let q = col - (row - (row & 1)) / 2;
    (q, row)
}

/// Cube distance between two axial coordinates: `(|dq| + |dr| + |ds|) / 2` with `s = -q - r`.
#[inline]
fn axial_distance(a: (i32, i32), b: (i32, i32)) -> u32 {
    let (aq, ar) = a;
    let (bq, br) = b;
    let (a_s, b_s) = (-aq - ar, -bq - br);
    let sum = (aq - bq).unsigned_abs() + (ar - br).unsigned_abs() + (a_s - b_s).unsigned_abs();
    sum / 2
}

/// True hex-grid distance (in hex steps) between two tiles, honoring horizontal wrap.
///
/// Uses the **odd-r** offset↔axial convention `hex_neighbor` steps in, so this metric agrees
/// with the neighbor stepping: the 6 immediate neighbors are distance 1, the next ring is
/// distance 2, and — unlike Chebyshev on offset coords, which measures a *square* whose corners
/// are actually 3 hex-steps away — the corner of a Chebyshev range-2 box is correctly distance 3.
///
/// Wrap: the shortest wrapped column delta (`shortest_delta_x`) brings `b` into `a`'s local
/// column frame first, so a tile just across the seam is near (not `width`-far). Y never wraps.
#[inline]
pub fn hex_distance_wrapped(a: UVec2, b: UVec2, width: u32, wrap: bool) -> u32 {
    let b_col = a.x as i32 + shortest_delta_x(a.x, b.x, width, wrap);
    let a_axial = offset_to_axial(a.x as i32, a.y as i32);
    let b_axial = offset_to_axial(b_col, b.y as i32);
    axial_distance(a_axial, b_axial)
}

/// Get the six hex neighbors in odd-r offset coordinates, with optional horizontal wrap.
///
/// Returns up to 6 valid neighbor coordinates. Neighbors that would be outside
/// the grid (y < 0 or y >= height) are excluded. When wrap_horizontal is false,
/// neighbors with x < 0 or x >= width are also excluded.
///
/// # Coordinate System
///
/// Odd-r offset coordinates (pointy-top hexes):
/// - Even rows (y % 2 == 0): NE/SE neighbors at same x, NW/SW at x-1
/// - Odd rows (y % 2 == 1): NE/SE neighbors at x+1, NW/SW at same x
///
/// Neighbor directions (clockwise from E):
/// - 0: E  (east)
/// - 1: SE (southeast)
/// - 2: SW (southwest)
/// - 3: W  (west)
/// - 4: NW (northwest)
/// - 5: NE (northeast)
pub fn hex_neighbors_wrapped(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> impl Iterator<Item = (u32, u32)> {
    // Reuse the single-direction step for each of the 6 directions, dropping any that
    // fall off the map — keeping one authoritative offset table (`HEX_NEIGHBOR_OFFSETS`).
    (0..HEX_DIRECTION_COUNT)
        .filter_map(move |dir| hex_neighbor(x, y, dir, width, height, wrap_horizontal))
}

/// Number of hex directions (odd-r), i.e. the six neighbors around any tile.
pub const HEX_DIRECTION_COUNT: usize = 6;

/// Odd-r neighbor offsets, one row per hex direction (clockwise from E).
/// Format: `(dx_even, dx_odd, dy)` — the x-shift depends on the source row's parity.
/// Single source of truth for both `hex_neighbor` and `hex_neighbors_wrapped`.
const HEX_NEIGHBOR_OFFSETS: [(i32, i32, i32); HEX_DIRECTION_COUNT] = [
    (1, 1, 0),   // 0: E
    (0, 1, 1),   // 1: SE
    (-1, 0, 1),  // 2: SW
    (-1, -1, 0), // 3: W
    (-1, 0, -1), // 4: NW
    (0, 1, -1),  // 5: NE
];

/// Step one tile from `(x, y)` in hex direction `dir` (`0..HEX_DIRECTION_COUNT`,
/// clockwise from E — see `hex_neighbors_wrapped`), honoring horizontal wrap.
///
/// Returns `None` when the step leaves the map: off the top/bottom edge (no vertical
/// wrap), or — when `wrap_horizontal` is false — past the left/right edge. This is the
/// single-direction primitive used to walk a straight hex line outward (e.g. posting a
/// scout vantage), where each successive step must re-read the *current* tile's row
/// parity rather than assuming a fixed dx.
#[inline]
pub fn hex_neighbor(
    x: u32,
    y: u32,
    dir: usize,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> Option<(u32, u32)> {
    // Return `None` for an out-of-range direction rather than panicking — this is a
    // `pub` primitive, and callers already treat `None` as "step left the map".
    let &(dx_even, dx_odd, dy) = HEX_NEIGHBOR_OFFSETS.get(dir)?;
    let dx = if (y % 2) == 1 { dx_odd } else { dx_even };
    let nx = x as i32 + dx;
    let ny = y as i32 + dy;

    // Y must be in bounds (no vertical wrap).
    if ny < 0 || ny >= height as i32 {
        return None;
    }

    let final_x = if wrap_horizontal {
        wrap_x(nx, width, true)
    } else {
        if nx < 0 || nx >= width as i32 {
            return None;
        }
        nx as u32
    };

    Some((final_x, ny as u32))
}

/// Get 4-connected (cardinal) neighbors with optional horizontal wrap.
///
/// This is for raster grid operations (non-hex), returning N/S/E/W neighbors.
pub fn neighbors4_wrapped(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> impl Iterator<Item = (u32, u32)> {
    const OFFSETS: [(i32, i32); 4] = [
        (1, 0),  // E
        (-1, 0), // W
        (0, 1),  // S
        (0, -1), // N
    ];

    let xi = x as i32;
    let yi = y as i32;
    let w = width as i32;
    let h = height as i32;

    OFFSETS.into_iter().filter_map(move |(dx, dy)| {
        let nx = xi + dx;
        let ny = yi + dy;

        // Y must be in bounds (no vertical wrap)
        if ny < 0 || ny >= h {
            return None;
        }

        // Handle X coordinate
        let final_x = if wrap_horizontal {
            wrap_x(nx, width, true)
        } else {
            if nx < 0 || nx >= w {
                return None;
            }
            nx as u32
        };

        Some((final_x, ny as u32))
    })
}

/// Get 8-connected neighbors with optional horizontal wrap.
///
/// This is for raster grid operations (non-hex), returning all 8 surrounding cells.
pub fn neighbors8_wrapped(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    wrap_horizontal: bool,
) -> impl Iterator<Item = (u32, u32)> {
    const OFFSETS: [(i32, i32); 8] = [
        (1, 0),   // E
        (1, 1),   // SE
        (0, 1),   // S
        (-1, 1),  // SW
        (-1, 0),  // W
        (-1, -1), // NW
        (0, -1),  // N
        (1, -1),  // NE
    ];

    let xi = x as i32;
    let yi = y as i32;
    let w = width as i32;
    let h = height as i32;

    OFFSETS.into_iter().filter_map(move |(dx, dy)| {
        let nx = xi + dx;
        let ny = yi + dy;

        // Y must be in bounds (no vertical wrap)
        if ny < 0 || ny >= h {
            return None;
        }

        // Handle X coordinate
        let final_x = if wrap_horizontal {
            wrap_x(nx, width, true)
        } else {
            if nx < 0 || nx >= w {
                return None;
            }
            nx as u32
        };

        Some((final_x, ny as u32))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_x_basic() {
        // Normal coordinates unchanged
        assert_eq!(wrap_x(0, 80, true), 0);
        assert_eq!(wrap_x(40, 80, true), 40);
        assert_eq!(wrap_x(79, 80, true), 79);

        // Wrap at boundaries
        assert_eq!(wrap_x(80, 80, true), 0);
        assert_eq!(wrap_x(81, 80, true), 1);
        assert_eq!(wrap_x(-1, 80, true), 79);
        assert_eq!(wrap_x(-2, 80, true), 78);

        // Multiple wraps
        assert_eq!(wrap_x(160, 80, true), 0);
        assert_eq!(wrap_x(-80, 80, true), 0);
    }

    #[test]
    fn test_wrap_x_no_wrap() {
        // Clamps to bounds
        assert_eq!(wrap_x(-1, 80, false), 0);
        assert_eq!(wrap_x(-10, 80, false), 0);
        assert_eq!(wrap_x(80, 80, false), 79);
        assert_eq!(wrap_x(100, 80, false), 79);

        // Valid coordinates unchanged
        assert_eq!(wrap_x(0, 80, false), 0);
        assert_eq!(wrap_x(40, 80, false), 40);
        assert_eq!(wrap_x(79, 80, false), 79);
    }

    #[test]
    fn test_wrapped_distance_x() {
        // Direct distance when shorter
        assert_eq!(wrapped_distance_x(10, 20, 80, true), 10);
        assert_eq!(wrapped_distance_x(20, 10, 80, true), 10);
        assert_eq!(wrapped_distance_x(0, 39, 80, true), 39);

        // Wrapped distance when shorter
        assert_eq!(wrapped_distance_x(79, 1, 80, true), 2); // 79->80->0->1
        assert_eq!(wrapped_distance_x(1, 79, 80, true), 2);
        assert_eq!(wrapped_distance_x(0, 79, 80, true), 1); // adjacent via wrap

        // Edge case: halfway point
        assert_eq!(wrapped_distance_x(0, 40, 80, true), 40);
        assert_eq!(wrapped_distance_x(0, 41, 80, true), 39); // wrap is shorter

        // Without wrap, always direct
        assert_eq!(wrapped_distance_x(79, 1, 80, false), 78);
        assert_eq!(wrapped_distance_x(0, 79, 80, false), 79);
    }

    #[test]
    fn test_shortest_delta_x() {
        // Direct path when shorter
        assert_eq!(shortest_delta_x(10, 20, 80, true), 10);
        assert_eq!(shortest_delta_x(20, 10, 80, true), -10);

        // Wrapped path when shorter
        assert_eq!(shortest_delta_x(79, 1, 80, true), 2); // go right, wrap
        assert_eq!(shortest_delta_x(1, 79, 80, true), -2); // go left, wrap

        // Adjacent via wrap
        assert_eq!(shortest_delta_x(0, 79, 80, true), -1);
        assert_eq!(shortest_delta_x(79, 0, 80, true), 1);

        // Without wrap
        assert_eq!(shortest_delta_x(79, 1, 80, false), -78);
        assert_eq!(shortest_delta_x(0, 79, 80, false), 79);
    }

    #[test]
    fn test_wrapped_distance_sq() {
        // Simple case, no wrap needed
        let dist_sq = wrapped_distance_sq(UVec2::new(10, 10), UVec2::new(13, 14), 80, true);
        assert_eq!(dist_sq, 3 * 3 + 4 * 4); // 9 + 16 = 25

        // Wrap case
        let dist_sq = wrapped_distance_sq(UVec2::new(79, 10), UVec2::new(1, 10), 80, true);
        assert_eq!(dist_sq, 2 * 2); // dx=2 via wrap, dy=0

        // Y distance unaffected by wrap
        let dist_sq = wrapped_distance_sq(UVec2::new(0, 0), UVec2::new(0, 10), 80, true);
        assert_eq!(dist_sq, 10 * 10); // dx=0, dy=10
    }

    #[test]
    fn test_hex_neighbors_center() {
        // Test neighbors of a tile in the center (no boundary issues)
        let neighbors: Vec<_> = hex_neighbors_wrapped(40, 25, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 6);

        // Even row (y=24): verify specific neighbors
        let neighbors_even: Vec<_> = hex_neighbors_wrapped(40, 24, 80, 52, true).collect();
        assert_eq!(neighbors_even.len(), 6);
        assert!(neighbors_even.contains(&(41, 24))); // E
        assert!(neighbors_even.contains(&(40, 25))); // SE
        assert!(neighbors_even.contains(&(39, 25))); // SW
        assert!(neighbors_even.contains(&(39, 24))); // W
        assert!(neighbors_even.contains(&(39, 23))); // NW
        assert!(neighbors_even.contains(&(40, 23))); // NE

        // Odd row (y=25): verify specific neighbors
        let neighbors_odd: Vec<_> = hex_neighbors_wrapped(40, 25, 80, 52, true).collect();
        assert_eq!(neighbors_odd.len(), 6);
        assert!(neighbors_odd.contains(&(41, 25))); // E
        assert!(neighbors_odd.contains(&(41, 26))); // SE
        assert!(neighbors_odd.contains(&(40, 26))); // SW
        assert!(neighbors_odd.contains(&(39, 25))); // W
        assert!(neighbors_odd.contains(&(40, 24))); // NW
        assert!(neighbors_odd.contains(&(41, 24))); // NE
    }

    #[test]
    fn test_hex_neighbors_wrap_boundary() {
        // At left edge (x=0), even row
        let neighbors: Vec<_> = hex_neighbors_wrapped(0, 24, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 6);
        assert!(neighbors.contains(&(79, 24))); // W wraps to right edge
        assert!(neighbors.contains(&(79, 23))); // NW wraps
        assert!(neighbors.contains(&(79, 25))); // SW wraps

        // At right edge (x=79), odd row
        let neighbors: Vec<_> = hex_neighbors_wrapped(79, 25, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 6);
        assert!(neighbors.contains(&(0, 25))); // E wraps to left edge
        assert!(neighbors.contains(&(0, 24))); // NE wraps
        assert!(neighbors.contains(&(0, 26))); // SE wraps
    }

    #[test]
    fn test_hex_neighbors_no_wrap() {
        // At left edge without wrap - loses 3 neighbors
        let neighbors: Vec<_> = hex_neighbors_wrapped(0, 24, 80, 52, false).collect();
        assert_eq!(neighbors.len(), 3); // Only E, SE, NE remain

        // At right edge without wrap (odd row) - loses 3 neighbors
        let neighbors: Vec<_> = hex_neighbors_wrapped(79, 25, 80, 52, false).collect();
        assert_eq!(neighbors.len(), 3); // Only W, SW, NW remain
    }

    #[test]
    fn test_hex_neighbors_top_bottom_edge() {
        // Top edge (y=0) - loses 2-3 neighbors regardless of wrap
        let neighbors: Vec<_> = hex_neighbors_wrapped(40, 0, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 4); // Loses NE and NW

        // Bottom edge (y=51) - loses 2-3 neighbors
        let neighbors: Vec<_> = hex_neighbors_wrapped(40, 51, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 4); // Loses SE and SW
    }

    #[test]
    fn test_neighbors4_wrapped() {
        // Center tile
        let neighbors: Vec<_> = neighbors4_wrapped(40, 25, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 4);

        // Left edge with wrap
        let neighbors: Vec<_> = neighbors4_wrapped(0, 25, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 4);
        assert!(neighbors.contains(&(79, 25))); // W wraps

        // Left edge without wrap
        let neighbors: Vec<_> = neighbors4_wrapped(0, 25, 80, 52, false).collect();
        assert_eq!(neighbors.len(), 3); // Loses W

        // Top edge
        let neighbors: Vec<_> = neighbors4_wrapped(40, 0, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 3); // Loses N
    }

    #[test]
    fn hex_distance_matches_neighbor_stepping() {
        // The 6 tiles at hex distance 1 are exactly the `hex_neighbor` set — the metric
        // agrees with the odd-r stepping. Checked on both row parities.
        const W: u32 = 80;
        const H: u32 = 52;
        for &center in &[UVec2::new(40, 24), UVec2::new(40, 25)] {
            let neighbors: Vec<UVec2> = (0..HEX_DIRECTION_COUNT)
                .filter_map(|dir| hex_neighbor(center.x, center.y, dir, W, H, true))
                .map(|(x, y)| UVec2::new(x, y))
                .collect();
            assert_eq!(neighbors.len(), 6);
            for n in &neighbors {
                assert_eq!(
                    hex_distance_wrapped(center, *n, W, true),
                    1,
                    "neighbor {n:?} of {center:?} must be hex distance 1"
                );
            }

            // Exactly 6 tiles in the surrounding area are distance 1, and 12 are distance 2.
            let mut ring1 = 0u32;
            let mut ring2 = 0u32;
            for dy in -3i32..=3 {
                for dx in -3i32..=3 {
                    let (x, y) = (center.x as i32 + dx, center.y as i32 + dy);
                    if y < 0 || y >= H as i32 {
                        continue;
                    }
                    let t = UVec2::new(x as u32, y as u32);
                    match hex_distance_wrapped(center, t, W, true) {
                        1 => ring1 += 1,
                        2 => ring2 += 1,
                        _ => {}
                    }
                }
            }
            assert_eq!(ring1, 6, "hex ring 1 has 6 tiles around {center:?}");
            assert_eq!(ring2, 12, "hex ring 2 has 12 tiles around {center:?}");
        }
    }

    #[test]
    fn hex_distance_chebyshev_corner_is_three_not_two() {
        // The bug: the corner of the old Chebyshev range-2 (5×5) square reads Chebyshev-2 but
        // is really 3 hex-steps away. All four diagonal corners are distance 3, so a range-2
        // work check correctly excludes them (playtest: "corners are 3 hexes away").
        const W: u32 = 80;
        let center = UVec2::new(40, 24);
        for &corner in &[
            UVec2::new(42, 26),
            UVec2::new(38, 26),
            UVec2::new(42, 22),
            UVec2::new(38, 22),
        ] {
            // Chebyshev would call these distance 2.
            assert_eq!(
                corner.x.abs_diff(center.x).max(corner.y.abs_diff(center.y)),
                2
            );
            // True hex distance is 3 — out of range at work_range 2.
            assert_eq!(
                hex_distance_wrapped(center, corner, W, true),
                3,
                "Chebyshev-corner {corner:?} is 3 hex-steps from {center:?}"
            );
        }
    }

    #[test]
    fn hex_distance_wraps_across_seam() {
        // A tile just across the horizontal seam is near, not `width`-far.
        const W: u32 = 80;
        let a = UVec2::new(79, 24);
        let b = UVec2::new(0, 24); // one column across the wrap
        assert_eq!(hex_distance_wrapped(a, b, W, true), 1);
        // Without wrap the same pair is far (79 columns apart).
        assert_eq!(hex_distance_wrapped(a, b, W, false), 79);
    }

    #[test]
    fn hex_distance_is_symmetric_and_zero_on_self() {
        const W: u32 = 80;
        let a = UVec2::new(10, 7);
        let b = UVec2::new(14, 11);
        assert_eq!(hex_distance_wrapped(a, a, W, true), 0);
        assert_eq!(
            hex_distance_wrapped(a, b, W, true),
            hex_distance_wrapped(b, a, W, true)
        );
    }

    #[test]
    fn test_neighbors8_wrapped() {
        // Center tile
        let neighbors: Vec<_> = neighbors8_wrapped(40, 25, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 8);

        // Corner with wrap
        let neighbors: Vec<_> = neighbors8_wrapped(0, 0, 80, 52, true).collect();
        assert_eq!(neighbors.len(), 5); // Loses N, NE, NW
        assert!(neighbors.contains(&(79, 0))); // W wraps
        assert!(neighbors.contains(&(79, 1))); // SW wraps
    }
}
