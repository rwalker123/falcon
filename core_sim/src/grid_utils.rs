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
    // Neighbor offsets for odd-r coordinates
    // Format: (dx_even, dx_odd, dy) for each direction
    const NEIGHBOR_OFFSETS: [(i32, i32, i32); 6] = [
        (1, 1, 0),   // E
        (0, 1, 1),   // SE
        (-1, 0, 1),  // SW
        (-1, -1, 0), // W
        (-1, 0, -1), // NW
        (0, 1, -1),  // NE
    ];

    let is_odd_row = (y % 2) == 1;
    let xi = x as i32;
    let yi = y as i32;
    let w = width as i32;
    let h = height as i32;

    NEIGHBOR_OFFSETS
        .into_iter()
        .filter_map(move |(dx_even, dx_odd, dy)| {
            let dx = if is_odd_row { dx_odd } else { dx_even };
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
