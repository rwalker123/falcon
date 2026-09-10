//! **Hex distance on the odd-r offset grid**, restated from `core_sim/src/grid_utils.rs`
//! (`hex_distance_wrapped`, `offset_to_axial`, `axial_distance`, `shortest_delta_x`) by the rule
//! every other restated constant follows: this crate cannot link the server, so the server's is the
//! authority and this is its twin, with the same convention (pointy-top, odd rows shoved right,
//! horizontal wrap when the header says so, the poles hard edges).
//!
//! Reach is judged in this metric by the sim's assignment loop (`BandReach` in
//! `core_sim/src/systems/labor.rs`), so a specialist that measures reach any other way proposes
//! rows the sim lapses.

/// A tile's offset coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
}

impl Tile {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// The grid a distance is measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub wrap_horizontal: bool,
}

impl Grid {
    /// True hex-step distance between two tiles, wrap-aware horizontally.
    pub fn distance(&self, a: Tile, b: Tile) -> u32 {
        let b_col = a.x as i32 + self.shortest_delta_x(a.x, b.x);
        let a_axial = offset_to_axial(a.x as i32, a.y as i32);
        let b_axial = offset_to_axial(b_col, b.y as i32);
        axial_distance(a_axial, b_axial)
    }

    /// The signed column delta from `x1` to `x2` by the shortest route.
    fn shortest_delta_x(&self, x1: u32, x2: u32) -> i32 {
        let direct = x2 as i32 - x1 as i32;
        if !self.wrap_horizontal {
            return direct;
        }
        let w = self.width as i32;
        if direct.abs() <= w / 2 {
            direct
        } else if direct > 0 {
            direct - w
        } else {
            direct + w
        }
    }

    /// The raster index of a tile, row-major — how every map-sized raster in the frame is laid out.
    pub fn index(&self, tile: Tile) -> Option<usize> {
        if tile.x >= self.width || tile.y >= self.height {
            return None;
        }
        Some((tile.y * self.width + tile.x) as usize)
    }

    /// Every tile within `radius` hex steps of `center`: the bounding box, exactly filtered.
    pub fn disk(&self, center: Tile, radius: u32) -> Vec<Tile> {
        let r = radius as i32;
        let mut tiles = Vec::new();
        for dy in -r..=r {
            let y = center.y as i32 + dy;
            if y < 0 || y >= self.height as i32 {
                continue;
            }
            for dx in -r..=r {
                let raw_x = center.x as i32 + dx;
                let x = if self.wrap_horizontal {
                    raw_x.rem_euclid(self.width as i32)
                } else if raw_x < 0 || raw_x >= self.width as i32 {
                    continue;
                } else {
                    raw_x
                };
                let tile = Tile::new(x as u32, y as u32);
                if self.distance(center, tile) <= radius && !tiles.contains(&tile) {
                    tiles.push(tile);
                }
            }
        }
        tiles
    }
}

/// Odd-r offset `(col, row)` → axial `(q, r)`.
fn offset_to_axial(col: i32, row: i32) -> (i32, i32) {
    (col - (row - (row & 1)) / 2, row)
}

/// Cube distance between two axial coordinates.
fn axial_distance(a: (i32, i32), b: (i32, i32)) -> u32 {
    let (a_s, b_s) = (-a.0 - a.1, -b.0 - b.1);
    ((a.0 - b.0).unsigned_abs() + (a.1 - b.1).unsigned_abs() + (a_s - b_s).unsigned_abs()) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid {
        width: 24,
        height: 16,
        wrap_horizontal: true,
    };

    #[test]
    fn neighbours_are_one_step_and_the_chebyshev_corner_is_three() {
        let center = Tile::new(5, 5);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1)] {
            let tile = Tile::new((5 + dx) as u32, (5 + dy) as u32);
            assert_eq!(GRID.distance(center, tile), 1, "{tile:?}");
        }
        // Odd row: the offset corner (-2, +2) is three hex steps, not two.
        assert_eq!(GRID.distance(center, Tile::new(3, 7)), 3);
    }

    #[test]
    fn the_seam_is_near_when_the_map_wraps() {
        assert_eq!(GRID.distance(Tile::new(23, 4), Tile::new(0, 4)), 1);
        let flat = Grid {
            wrap_horizontal: false,
            ..GRID
        };
        assert_eq!(flat.distance(Tile::new(23, 4), Tile::new(0, 4)), 23);
    }

    #[test]
    fn a_disk_grows_one_seven_nineteen_and_indexes_row_major() {
        let center = Tile::new(10, 8);
        assert_eq!(GRID.disk(center, 0).len(), 1);
        assert_eq!(GRID.disk(center, 1).len(), 7);
        assert_eq!(GRID.disk(center, 2).len(), 19);
        assert_eq!(GRID.index(Tile::new(3, 2)), Some(2 * 24 + 3));
        assert_eq!(GRID.index(Tile::new(24, 2)), None);
    }
}
