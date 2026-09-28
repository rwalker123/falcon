//! `CoastField` -- the fields the terrain shader's coastal swell runs on.
//!
//! The builder rasterises the per-hex mask at `texels_per_radius` texels per hex radius and derives:
//! - the DISTANCE TO LAND (an EXACT Euclidean distance transform -- Felzenszwalb & Huttenlocher, two
//!   separable 1D passes of the lower envelope of parabolas -- lightly blurred), which drives shoaling,
//!   the no-shelf fallback band and the shelter test;
//! - the coastal ELIGIBILITY (a water-only blur of the per-hex flag, so lakes carry none);
//! - the REGIONAL SWELL DIRECTION: a unit vector pointing from open water toward land, taken from the
//!   gradient of the distance to OPEN water (deep ocean) over the non-open WATER, and smoothed over several
//!   hexes so it varies only slowly. Water with no open water within the cap (an enclosed shelf) contributes
//!   its coast-normal instead. Where neither says anything nearby -- open water itself, an island alone in
//!   the ocean -- the map's mean direction fills in, so such an island takes the prevailing swell on one
//!   side rather than rows from every side. The sources are blended by how much each says, never switched;
//! - the SHELF ZONE, where rows run: the shelf flag (water-only blur) faded in from the shelf's seaward
//!   edge by the distance to open water, so a row appears at the shelf edge rather than at a hex line.
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
/// Shelf water: the swell's rows run here (`coastal_swell.swell_terrains`).
const CELL_SHELF_WATER: u8 = 1;
/// Open water the rows come FROM (`coastal_swell.open_water_terrains`): coastal, but the rows reach it
/// only as the narrow no-shelf band where it touches land. (2 is other water: a lake.)
const CELL_OPEN_WATER: u8 = 3;

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
/// A texel's distance gradient counts as a direction only when the distance rises at least this fast (per
/// hex radius); on a ridge between two sources it says nothing.
const DIR_MIN_SLOPE: f64 = 0.5;
/// A smoothed direction field of at least this length (a mean of unit vectors, 0..1) needs no fallback;
/// below it the next source fills in, in proportion.
const DIR_FULL_LENGTH: f32 = 0.25;
/// Below this length a vector is treated as no direction at all.
const DIR_EPSILON: f32 = 1.0e-6;
/// Channels of the interleaved `data` output: distance, eligibility, direction x, direction y.
const DATA_CHANNELS: usize = 4;

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct CoastField;

#[godot_api]
impl CoastField {
    /// Build the field. `cells` is `grid_w * grid_h` cell codes (see `CELL_*`), row-major by hex row.
    /// Returns `{width, height, origin: Vector2 (hex radii), texels_per_radius, data: PackedFloat32Array
    /// (RGBA interleaved: R = distance to land in hex radii, capped at `cap_radii`; G = coastal
    /// eligibility 0..1, a water-only blur of the per-hex flag; BA = the regional swell direction, unit
    /// or zero), shelf: PackedFloat32Array (one per texel: the shelf zone 0..1), build_ms}`.
    /// `direction_smoothing_radii` is the direction field's blur radius; `shelf_fade_radii` how far in
    /// from the shelf's seaward edge the zone takes to reach full.
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
        direction_smoothing_radii: f64,
        shelf_fade_radii: f64,
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
        let mut open = vec![false; n];
        let mut water = vec![0.0f32; n];
        let mut coastal = vec![0.0f32; n];
        let mut shelf = vec![0.0f32; n];
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
                    if code == CELL_SHELF_WATER {
                        coastal[i] = 1.0;
                        shelf[i] = 1.0;
                    } else if code == CELL_OPEN_WATER {
                        coastal[i] = 1.0;
                        open[i] = true;
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
        // The same transform from OPEN water, for the direction and the shelf zone.
        let mut open_grid: Vec<f64> = open
            .iter()
            .map(|&o| if o { 0.0 } else { EDT_INF })
            .collect();
        edt_2d(&mut open_grid, w, h);
        let open_dist: Vec<f32> = open_grid
            .iter()
            .map(|&d2| ((d2.sqrt() / tpr) as f32).min(cap))
            .collect();
        let direction = regional_direction(
            &open_dist,
            &dist,
            w,
            h,
            cap,
            tpr,
            (direction_smoothing_radii * tpr).round().max(0.0) as usize,
        );

        // Light blur: the distance plainly, the eligibility as a WATER-ONLY mean (so a land texel's
        // absence does not drag a coast's eligibility down, and a lake's 0 does not leak far).
        let radius = (blur_radii * tpr).round().max(0.0) as usize;
        if radius > 0 {
            for _ in 0..BLUR_PASSES {
                box_blur(&mut dist, w, h, radius);
            }
            let mut weighted: Vec<f32> = coastal.iter().zip(&water).map(|(c, wt)| c * wt).collect();
            let mut shelf_weighted: Vec<f32> =
                shelf.iter().zip(&water).map(|(c, wt)| c * wt).collect();
            for _ in 0..BLUR_PASSES {
                box_blur(&mut weighted, w, h, radius);
                box_blur(&mut shelf_weighted, w, h, radius);
                box_blur(&mut water, w, h, radius);
            }
            for i in 0..n {
                let (c, s) = if water[i] > 0.0 {
                    (weighted[i] / water[i], shelf_weighted[i] / water[i])
                } else {
                    (0.0, 0.0)
                };
                coastal[i] = c;
                shelf[i] = s;
            }
        }
        // The shelf zone: the shelf, faded in from its seaward edge (where the distance to open water is 0).
        let fade = shelf_fade_radii.max(f64::EPSILON) as f32;
        let zone: Vec<f32> = (0..n)
            .map(|i| shelf[i] * smoothstep(0.0, fade, open_dist[i]))
            .collect();

        let mut data = PackedFloat32Array::new();
        data.resize(n * DATA_CHANNELS);
        {
            let slice = data.as_mut_slice();
            for i in 0..n {
                let o = i * DATA_CHANNELS;
                slice[o] = dist[i];
                slice[o + 1] = coastal[i];
                slice[o + 2] = direction[i * 2];
                slice[o + 3] = direction[i * 2 + 1];
            }
        }
        let mut shelf_out = PackedFloat32Array::new();
        shelf_out.resize(n);
        shelf_out.as_mut_slice().copy_from_slice(&zone);
        out.set("width", w as i64);
        out.set("height", h as i64);
        out.set("origin", Vector2::new(x0 as f32, y0 as f32));
        out.set("texels_per_radius", tpr);
        out.set("data", &data);
        out.set("shelf", &shelf_out);
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

/// One separable box blur of `radius` texels: each output is the mean of the in-bounds texels within
/// `radius`, so the edges are not darkened. A running sum, so the cost does not grow with the radius (the
/// direction field blurs over several hexes).
fn box_blur(values: &mut [f32], w: usize, h: usize, radius: usize) {
    let mut tmp = vec![0.0f32; values.len()];
    let mut line = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    for y in 0..h {
        line[..w].copy_from_slice(&values[y * w..y * w + w]);
        blur_line(&line[..w], &mut out[..w], radius);
        tmp[y * w..y * w + w].copy_from_slice(&out[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            line[y] = tmp[y * w + x];
        }
        blur_line(&line[..h], &mut out[..h], radius);
        for y in 0..h {
            values[y * w + x] = out[y];
        }
    }
}

/// The mean of `src` over each `[i - radius, i + radius]` window clipped to the line, by a running sum.
fn blur_line(src: &[f32], dst: &mut [f32], radius: usize) {
    let n = src.len();
    let mut sum = 0.0f64;
    let mut lo = 0usize;
    let mut hi = 0usize; // the window is src[lo..hi]
    for (i, out) in dst.iter_mut().enumerate().take(n) {
        let want_hi = (i + radius + 1).min(n);
        let want_lo = i.saturating_sub(radius);
        while hi < want_hi {
            sum += src[hi] as f64;
            hi += 1;
        }
        while lo < want_lo {
            sum -= src[lo] as f64;
            lo += 1;
        }
        *out = (sum / (hi - lo) as f64) as f32;
    }
}

/// Hermite smoothstep, as the shader's.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The unit central-difference gradient of `field` at texel (x, y), or `None` where it rises slower than
/// `min_slope` per texel.
fn unit_gradient(
    field: &[f32],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    min_slope: f64,
) -> Option<(f32, f32)> {
    let at = |xx: usize, yy: usize| field[yy * w + xx] as f64;
    let (xl, xr) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let (yu, yd) = (y.saturating_sub(1), (y + 1).min(h - 1));
    if xr == xl || yd == yu {
        return None;
    }
    let gx = (at(xr, y) - at(xl, y)) / (xr - xl) as f64;
    let gy = (at(x, yd) - at(x, yu)) / (yd - yu) as f64;
    let len = (gx * gx + gy * gy).sqrt();
    if len < min_slope {
        return None;
    }
    Some(((gx / len) as f32, (gy / len) as f32))
}

/// THE REGIONAL SWELL DIRECTION (interleaved x, y per texel; unit or zero): see the module docs. Three
/// sources, the first two fields of unit vectors box-blurred over `smoothing` texels, blended by how much
/// each says -- a blurred field of unit vectors is short where its inputs disagree or are absent. Only
/// WATER texels that are not open water contribute (land would add an island's own radial vectors):
/// 1. away from OPEN water (+grad of `open_dist`), where open water lies within the cap and the gradient
///    is steep enough to mean something;
/// 2. the coast-normal (-grad of `land_dist`), toward the nearest land, where no open water lies within
///    the cap (an enclosed shelf);
/// 3. the map's mean of source 1 (or of source 2 where there is no open water at all).
fn regional_direction(
    open_dist: &[f32],
    land_dist: &[f32],
    w: usize,
    h: usize,
    cap: f32,
    tpr: f64,
    smoothing: usize,
) -> Vec<f32> {
    let n = w * h;
    let min_slope = DIR_MIN_SLOPE / tpr;
    let mut ox = vec![0.0f32; n];
    let mut oy = vec![0.0f32; n];
    let mut cx = vec![0.0f32; n];
    let mut cy = vec![0.0f32; n];
    let (mut mean_ox, mut mean_oy, mut mean_cx, mut mean_cy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if land_dist[i] <= 0.0 || open_dist[i] <= 0.0 {
                continue; // land, or open water: neither says which way the rows run
            }
            if open_dist[i] < cap {
                if let Some((gx, gy)) = unit_gradient(open_dist, w, h, x, y, min_slope) {
                    ox[i] = gx;
                    oy[i] = gy;
                    mean_ox += gx as f64;
                    mean_oy += gy as f64;
                }
            } else if land_dist[i] < cap {
                if let Some((gx, gy)) = unit_gradient(land_dist, w, h, x, y, min_slope) {
                    cx[i] = -gx;
                    cy[i] = -gy;
                    mean_cx -= gx as f64;
                    mean_cy -= gy as f64;
                }
            }
        }
    }
    if smoothing > 0 {
        for _ in 0..BLUR_PASSES {
            box_blur(&mut ox, w, h, smoothing);
            box_blur(&mut oy, w, h, smoothing);
            box_blur(&mut cx, w, h, smoothing);
            box_blur(&mut cy, w, h, smoothing);
        }
    }
    let (mut gx, mut gy) = (mean_ox as f32, mean_oy as f32);
    if (gx * gx + gy * gy).sqrt() < DIR_EPSILON {
        gx = mean_cx as f32;
        gy = mean_cy as f32;
    }
    let glen = (gx * gx + gy * gy).sqrt();
    let (gx, gy) = if glen < DIR_EPSILON {
        (0.0, 0.0)
    } else {
        (gx / glen, gy / glen)
    };
    let mut out = vec![0.0f32; n * 2];
    for i in 0..n {
        let (mut vx, mut vy) = (ox[i], oy[i]);
        let short = 1.0 - smoothstep(0.0, DIR_FULL_LENGTH, (vx * vx + vy * vy).sqrt());
        vx += short * cx[i];
        vy += short * cy[i];
        let short = 1.0 - smoothstep(0.0, DIR_FULL_LENGTH, (vx * vx + vy * vy).sqrt());
        vx += short * gx;
        vy += short * gy;
        let len = (vx * vx + vy * vy).sqrt();
        if len >= DIR_EPSILON {
            out[i * 2] = vx / len;
            out[i * 2 + 1] = vy / len;
        }
    }
    out
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
    fn blur_line_is_a_clipped_window_mean() {
        let src = [0.0f32, 3.0, 6.0, 9.0];
        let mut dst = [0.0f32; 4];
        blur_line(&src, &mut dst, 1);
        assert_eq!(dst, [1.5, 3.0, 6.0, 7.5]);
    }

    #[test]
    fn direction_points_from_open_water_toward_land() {
        // Open water up to x = 3, land from x = 10: in between, the rows run +x.
        let w = 12usize;
        let h = 3usize;
        let mut open_dist = vec![0.0f32; w * h];
        let mut land_dist = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                open_dist[y * w + x] = (x as f32 - 3.0).max(0.0);
                land_dist[y * w + x] = (10.0 - x as f32).max(0.0);
            }
        }
        let dir = regional_direction(&open_dist, &land_dist, w, h, 100.0, 1.0, 0);
        let i = w + 6;
        assert!(
            dir[i * 2] > 0.99,
            "expected +x, got {:?}",
            &dir[i * 2..i * 2 + 2]
        );
        // An open-water texel says nothing itself, and takes the map's mean direction.
        let i = w + 1;
        assert!(
            dir[i * 2] > 0.99,
            "expected the mean +x, got {:?}",
            &dir[i * 2..i * 2 + 2]
        );
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
