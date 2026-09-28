//! `CoastField` -- the distance-to-coast field the terrain shader's coastal swell runs on.
//!
//! The swell's crests are CONTOURS of the distance from each water point to the nearest land, so the
//! field has to be smooth at sub-hex resolution: a per-hex distance would draw every crest as a
//! hexagon. The builder rasterises the hex land mask at `texels_per_radius` texels per hex radius,
//! runs an EXACT Euclidean distance transform (Felzenszwalb & Huttenlocher, two separable 1D passes of
//! the lower envelope of parabolas), and blurs the result lightly so a crest rounds a hex corner
//! instead of kinking on it.
//!
//! It lives here rather than in GDScript because of cost. The largest offered map (Huge, 128x80)
//! rasterises to ~1.8M texels at 8 per radius, and ONE chamfer pass over that many texels measured
//! 191 ms in GDScript -- a full build (rasterise, two passes, blur) would have been over a second of
//! hitch on every world load. Here the whole build is a few tens of milliseconds.
//!
//! The terrain is static within a world, so `TerrainRenderer` builds this once per world (and again
//! only when the land/water mask it feeds in actually changes).

use godot::prelude::*;
use std::time::Instant;

/// A cell code the caller passes per hex: land (distance 0), water the swell runs on, or water it
/// does not (a lake). Anything outside the grid, or off the edge of a non-wrapping map, is treated as
/// ineligible water: no land there, and no swell.
const CELL_LAND: u8 = 0;
const CELL_COASTAL_WATER: u8 = 1;

/// sqrt(3), the hex column pitch in hex radii (pointy-top odd-r, the layout `MapView._hex_center` and
/// the shader's `hex_center` share).
const SQRT3: f64 = 1.732_050_807_568_877_2;
/// Half a hex's height above/below its centre row, in radii.
const HEX_HALF_HEIGHT: f64 = 1.0;
/// Row pitch of pointy-top hexes, in radii.
const HEX_ROW_PITCH: f64 = 1.5;
/// "Infinitely far" for the distance transform's non-land samples.
const EDT_INF: f64 = 1.0e20;
/// Box-blur passes stacked to approximate a Gaussian.
const BLUR_PASSES: usize = 2;

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct CoastField;

#[godot_api]
impl CoastField {
    /// Build the field. `cells` is `grid_w * grid_h` cell codes (see `CELL_*`), row-major by hex row.
    /// Returns `{width, height, origin: Vector2 (hex radii), texels_per_radius, data: PackedFloat32Array
    /// (RG interleaved: R = distance to land in hex radii, capped at `cap_radii`; G = coastal
    /// eligibility 0..1, a water-only blur of the per-hex flag), build_ms}`.
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn build(
        cells: PackedByteArray,
        grid_w: i64,
        grid_h: i64,
        texels_per_radius: f64,
        pad_radii: f64,
        wrap: bool,
        blur_radii: f64,
        cap_radii: f64,
    ) -> VarDictionary {
        let started = Instant::now();
        let mut out = VarDictionary::new();
        let gw = grid_w.max(0) as usize;
        let gh = grid_h.max(0) as usize;
        let cells = cells.as_slice();
        if gw == 0 || gh == 0 || cells.len() < gw * gh || texels_per_radius <= 0.0 {
            return out;
        }
        let tpr = texels_per_radius;
        let x0 = -SQRT3 * 0.5 - pad_radii;
        let x1 = SQRT3 * (gw as f64 + 0.5) - SQRT3 * 0.5 + pad_radii;
        let y0 = -HEX_HALF_HEIGHT - pad_radii;
        let y1 = HEX_ROW_PITCH * (gh as f64 - 1.0) + HEX_HALF_HEIGHT + pad_radii;
        let w = ((x1 - x0) * tpr).ceil() as usize;
        let h = ((y1 - y0) * tpr).ceil() as usize;
        let n = w * h;

        // Rasterise: each texel centre → the hex containing it → its cell code.
        let mut land = vec![false; n];
        let mut water = vec![0.0f32; n];
        let mut coastal = vec![0.0f32; n];
        for ty in 0..h {
            let y = y0 + (ty as f64 + 0.5) / tpr;
            for tx in 0..w {
                let x = x0 + (tx as f64 + 0.5) / tpr;
                let code = cell_at(cells, gw, gh, wrap, x, y);
                let i = ty * w + tx;
                if code == CELL_LAND {
                    land[i] = true;
                } else {
                    water[i] = 1.0;
                    if code == CELL_COASTAL_WATER {
                        coastal[i] = 1.0;
                    }
                }
            }
        }

        // Exact squared EDT to the nearest land texel, in texels, then to hex radii.
        let mut grid: Vec<f64> = land
            .iter()
            .map(|&l| if l { 0.0 } else { EDT_INF })
            .collect();
        edt_2d(&mut grid, w, h);
        let cap = cap_radii as f32;
        let mut dist: Vec<f32> = grid
            .iter()
            .map(|&d2| ((d2.sqrt() / tpr) as f32).min(cap))
            .collect();

        // Light blur: the distance plainly, the eligibility as a WATER-ONLY mean (so a land texel's
        // absence does not drag a coast's eligibility down, and a lake's 0 does not leak far).
        let radius = (blur_radii * tpr).round().max(0.0) as usize;
        if radius > 0 {
            for _ in 0..BLUR_PASSES {
                box_blur(&mut dist, w, h, radius);
            }
            let mut weighted: Vec<f32> = coastal.iter().zip(&water).map(|(c, wt)| c * wt).collect();
            for _ in 0..BLUR_PASSES {
                box_blur(&mut weighted, w, h, radius);
                box_blur(&mut water, w, h, radius);
            }
            for i in 0..n {
                coastal[i] = if water[i] > 0.0 {
                    weighted[i] / water[i]
                } else {
                    0.0
                };
            }
        }

        let mut data = PackedFloat32Array::new();
        data.resize(n * 2);
        {
            let slice = data.as_mut_slice();
            for i in 0..n {
                slice[i * 2] = dist[i];
                slice[i * 2 + 1] = coastal[i];
            }
        }
        out.set("width", w as i64);
        out.set("height", h as i64);
        out.set("origin", Vector2::new(x0 as f32, y0 as f32));
        out.set("texels_per_radius", tpr);
        out.set("data", &data);
        out.set("build_ms", started.elapsed().as_secs_f64() * 1000.0);
        out
    }
}

/// The cell code of the hex containing point (x, y), in hex radii relative to hex (0, 0)'s centre.
fn cell_at(cells: &[u8], gw: usize, gh: usize, wrap: bool, x: f64, y: f64) -> u8 {
    // Pointy-top pixel → fractional axial, then cube rounding.
    let q = SQRT3 / 3.0 * x - y / 3.0;
    let r = 2.0 / 3.0 * y;
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    let row = rr as i64;
    if row < 0 || row >= gh as i64 {
        return u8::MAX;
    }
    // odd-r: col = q + (r - (r & 1)) / 2
    let mut col = rq as i64 + (row - (row & 1)) / 2;
    if wrap {
        col = col.rem_euclid(gw as i64);
    } else if col < 0 || col >= gw as i64 {
        return u8::MAX;
    }
    cells[row as usize * gw + col as usize]
}

/// In-place exact squared Euclidean distance transform of a `w x h` grid (0 at sources, `EDT_INF`
/// elsewhere): columns, then rows.
fn edt_2d(grid: &mut [f64], w: usize, h: usize) {
    let longest = w.max(h);
    let mut f = vec![0.0f64; longest];
    let mut d = vec![0.0f64; longest];
    let mut v = vec![0usize; longest];
    let mut z = vec![0.0f64; longest + 1];
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..y * w + w]);
        edt_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..y * w + w].copy_from_slice(&d[..w]);
    }
}

/// The 1D squared distance transform (lower envelope of parabolas).
fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        // Pop every parabola the new one hides; z[0] is -inf, so this always stops at k = 0.
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            break;
        }
        if s <= z[k] {
            // k == 0 and the new parabola hides the first one too: it replaces it.
            v[0] = q;
            z[1] = f64::INFINITY;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *out = dq * dq + f[p];
    }
}

/// One separable box blur of `radius` texels, clamped at the edges.
fn box_blur(values: &mut [f32], w: usize, h: usize, radius: usize) {
    let mut tmp = vec![0.0f32; values.len()];
    let r = radius as i64;
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let mut sum = 0.0f32;
            let mut count = 0.0f32;
            for dx in -r..=r {
                let sx = x as i64 + dx;
                if sx >= 0 && sx < w as i64 {
                    sum += values[row + sx as usize];
                    count += 1.0;
                }
            }
            tmp[row + x] = sum / count;
        }
    }
    for x in 0..w {
        for y in 0..h {
            let mut sum = 0.0f32;
            let mut count = 0.0f32;
            for dy in -r..=r {
                let sy = y as i64 + dy;
                if sy >= 0 && sy < h as i64 {
                    sum += tmp[sy as usize * w + x];
                    count += 1.0;
                }
            }
            values[y * w + x] = sum / count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edt_1d_matches_brute_force() {
        let f = [EDT_INF, 0.0, EDT_INF, EDT_INF, EDT_INF, 0.0, EDT_INF];
        let mut d = [0.0f64; 7];
        let mut v = [0usize; 7];
        let mut z = [0.0f64; 8];
        edt_1d(&f, &mut d, &mut v, &mut z);
        assert_eq!(d, [1.0, 0.0, 1.0, 4.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn cell_at_finds_hex_centres() {
        // 3x2 grid, codes 0..6 by index.
        let cells: Vec<u8> = (0..6).collect();
        for row in 0..2usize {
            for col in 0..3usize {
                let x = SQRT3 * (col as f64 + 0.5 * (row & 1) as f64);
                let y = HEX_ROW_PITCH * row as f64;
                assert_eq!(cell_at(&cells, 3, 2, false, x, y), (row * 3 + col) as u8);
            }
        }
    }
}
