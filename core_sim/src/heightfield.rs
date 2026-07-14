use std::{cmp::Ordering, collections::BinaryHeap, sync::Arc};

use bevy::prelude::*;

use crate::{
    map_preset::{ErosionConfig, MapPreset},
    resources::SimulationConfig,
};

/// Sea level (on the normalized 0..1 elevation scale) used when no map preset is
/// available. Mirrors the `unwrap_or` fallback in the worldgen/hydrology paths.
pub const DEFAULT_SEA_LEVEL: f32 = 0.6;

#[derive(Resource, Debug, Clone)]
pub struct ElevationField {
    pub width: u32,
    pub height: u32,
    /// The active map's sea level on this field's normalized 0..1 scale. Carried on
    /// the field so it can be emitted in the snapshot's `ElevationOverlay` for the
    /// client's relative-height readout. Defaults to [`DEFAULT_SEA_LEVEL`] until the
    /// preset value is attached via [`ElevationField::with_sea_level`].
    pub sea_level: f32,
    values: Arc<Vec<f32>>,
}

impl ElevationField {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Self {
        debug_assert_eq!(values.len(), (width * height) as usize);
        Self {
            width,
            height,
            sea_level: DEFAULT_SEA_LEVEL,
            values: Arc::new(values),
        }
    }

    /// Attaches the active map's sea level (normalized 0..1 scale) to this field.
    pub fn with_sea_level(mut self, sea_level: f32) -> Self {
        self.sea_level = sea_level;
        self
    }

    #[inline]
    pub fn sample(&self, x: u32, y: u32) -> f32 {
        debug_assert!(x < self.width && y < self.height);
        let idx = (y * self.width + x) as usize;
        self.values[idx]
    }

    /// Height above sea level remapped to `[0, 1]` (0 = at/below sea level, 1 = the field's max
    /// elevation) using the attached `sea_level`. Feeds the climate model's elevation lapse.
    #[inline]
    pub fn above_sea_normalized(&self, x: u32, y: u32) -> f32 {
        let headroom = 1.0 - self.sea_level;
        if headroom <= 0.0 {
            return 0.0;
        }
        ((self.sample(x, y) - self.sea_level) / headroom).clamp(0.0, 1.0)
    }
}

pub fn build_elevation_field(
    config: &SimulationConfig,
    preset: Option<&MapPreset>,
    seed: u64,
) -> ElevationField {
    let width = config.grid_size.x;
    let height = config.grid_size.y;

    let (continent_scale, mountain_scale) = if let Some(p) = preset {
        (p.continent_scale, p.mountain_scale)
    } else {
        (0.6, 0.6)
    };

    let mut values = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let nx = x as f32 / width.max(1) as f32;
            let ny = y as f32 / height.max(1) as f32;

            let continent_freq = 2.0 + continent_scale.clamp(0.1, 1.5) * 6.0;
            let mountain_freq = 6.0 + mountain_scale.clamp(0.2, 2.5) * 16.0;

            let continent_seed = mix_seed(0x9E37_0001, seed, 0);
            let ridge_seed = mix_seed(0xC0F3_0001, seed, 0x85EB);

            let continent = fbm_noise(
                nx * continent_freq,
                ny * continent_freq,
                4,
                2.0,
                0.5,
                continent_seed,
            );

            let ridge_source = fbm_noise(
                nx * mountain_freq,
                ny * mountain_freq,
                3,
                2.1,
                0.45,
                ridge_seed,
            );
            let ridged = (1.0 - (ridge_source - 0.5).abs() * 2.0)
                .clamp(0.0, 1.0)
                .powf(1.6);

            let mut height_value = continent * 0.75 + ridged * (0.2 + mountain_scale * 0.25);

            let dx = nx - 0.5;
            let dy = ny - 0.5;
            let radial = (dx * dx + dy * dy).sqrt();
            height_value -= radial.powf(1.8) * 0.25;

            let lat = (ny - 0.5).abs();
            height_value -= (lat.powf(1.3) * 0.1).clamp(0.0, 0.1);

            values.push(height_value.clamp(0.0, 1.0));
        }
    }

    let mut values = normalise_values(values);

    // Erosion runs on the NORMALISED field, and it runs *before* the caller sees it — and therefore
    // before `mapgen::generate_land_mask`, whose elevation ranking makes the coastline an
    // iso-contour of exactly this surface. With no preset there is no config, so the preset-less
    // fallback path keeps the raw fractal field.
    if let Some(p) = preset {
        let base_level = land_contour(&values, p.macro_land.target_land_pct);
        apply_fluvial_erosion(
            &mut values,
            width as usize,
            height as usize,
            base_level,
            &p.erosion,
        );
        if p.erosion.enabled && p.erosion.anchor_contour_to_sea_level {
            let eroded_contour = land_contour(&values, p.macro_land.target_land_pct);
            anchor_contour_to_sea_level(&mut values, eroded_contour, p.sea_level);
        }
    }

    ElevationField::new(width, height, values)
}

/// Rescale the field so the land-mask's coastline ([`land_contour`]) lands **exactly on
/// `sea_level`** — a strictly increasing, piecewise-linear map, pinned at both ends
/// (`φ(0) = 0`, `φ(contour) = sea_level`, `φ(1) = 1`).
///
/// Without this, carving valleys is pointless below sea level, and **a third of all land is below
/// sea level**: `mapgen::restamp_elevation`'s lowland branch computes
/// `above_sea = ((v − sea_level) / (1 − sea_level)).clamp(0, 1)`, so every land cell whose base
/// elevation sits under `sea_level` — 24–37% of cells are above it, but the mask claims 38% for
/// land — is **clamped flat to exactly `sea_level`**. That branch is only order-preserving *above*
/// sea level; below it, it is an order-destroying clamp that plates a third of every continent into
/// a dead-flat shelf where drainage is decided by fill epsilon and jitter. Aligning the contour with
/// sea level makes the whole pipeline's "land ⟺ above sea level" assumption true, and is what lets
/// the incised valleys survive into hydrology.
///
/// Being strictly monotone, it cannot reorder the field — the land mask's elevation ranking, and
/// therefore the land it selects, is preserved.
fn anchor_contour_to_sea_level(values: &mut [f32], contour: f32, sea_level: f32) {
    let sea_level = sea_level.clamp(0.0, 1.0);
    // A degenerate contour (all-water or all-land field) has no band to stretch.
    if contour <= 0.0 || contour >= 1.0 || sea_level <= 0.0 || sea_level >= 1.0 {
        return;
    }
    let below_scale = sea_level / contour;
    let above_scale = (1.0 - sea_level) / (1.0 - contour);
    for v in values.iter_mut() {
        *v = if *v <= contour {
            *v * below_scale
        } else {
            sea_level + (*v - contour) * above_scale
        };
    }
}

/// The elevation of the **land-mask's coastline** — the `(1 − target_land_pct)` quantile of the
/// field, which is where `mapgen::generate_land_mask`'s descending-elevation rank cut falls.
///
/// This, not `sea_level`, is base level for erosion, and the distinction is load-bearing: on the
/// earthlike preset only **24–37%** of cells sit above `sea_level = 0.62` while the mask claims
/// **38%** of them for land, so the coastline actually falls at elevation **0.55–0.61 — *below*
/// sea level**. An erosion pass that froze everything under `sea_level` would freeze the entire
/// coastal band it is supposed to reshape, and would measure as a no-op (it did).
fn land_contour(values: &[f32], target_land_pct: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_unstable_by(|a, b| a.total_cmp(b));
    let water_frac = 1.0 - target_land_pct.clamp(0.01, 0.99);
    let rank = ((water_frac * sorted.len() as f32) as usize).min(sorted.len() - 1);
    sorted[rank]
}

/// One cell in the priority flood. Ordered as a **min-heap** (lowest fill level first), with an
/// explicit index tie-break so the fill is bit-for-bit deterministic.
#[derive(PartialEq)]
struct FloodCell {
    level: f32,
    idx: usize,
}

impl Eq for FloodCell {}

impl Ord for FloodCell {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed on both keys: `BinaryHeap` is a max-heap, we want the lowest level (then the
        // lowest index) popped first.
        other
            .level
            .total_cmp(&self.level)
            .then_with(|| other.idx.cmp(&self.idx))
    }
}

impl PartialOrd for FloodCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The 8 D8 neighbour offsets on the square raster, with the step length each implies (1 for the
/// orthogonals, √2 for the diagonals) so slope is a real gradient rather than a raw drop.
const D8_OFFSETS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Fluvial erosion, in place, on the normalised heightfield — the classic landscape-evolution
/// model minus its uplift term: `∂z/∂t = D∇²z − K·A^m·S^n`.
///
/// Per iteration: priority-flood the depressions, route D8 steepest descent on the filled surface,
/// accumulate uniform unit drainage downstream, incise `dz = K·A^m·S^n·Δt`, then diffuse the
/// hillslopes. Cells at or below `base_level` — the coastline the land mask is about to cut, see
/// [`land_contour`] — are frozen sea: never eroded, never filled. So erosion can never dig new
/// ocean, and the mask can still hit its `target_land_pct`.
///
/// **The two terms do two different jobs, and both are needed** (measured — see
/// `hydrology_earthlike::drainage_census`): stream power carves the trunk valleys that give a
/// continent *capture*, but it does nothing to the high-frequency noise on the coastline contour,
/// so it cannot de-sponge; diffusion planes that noise off and de-sponges, but carves nothing.
///
/// Accumulation is **uniform (1.0 per cell)** on purpose: this is landscape evolution, not the
/// hydrology discharge model, which weights by precipitation on the hex/corner graph downstream of
/// here. Everything is pure arithmetic with explicit index tie-breaks — no RNG, no hash iteration —
/// as `integration_tests::determinism` requires.
fn apply_fluvial_erosion(
    values: &mut [f32],
    width: usize,
    height: usize,
    base_level: f32,
    cfg: &ErosionConfig,
) {
    let total = width * height;
    if !cfg.enabled || cfg.iterations == 0 || total == 0 {
        return;
    }

    let is_border = |idx: usize| -> bool {
        let (x, y) = (idx % width, idx / width);
        x == 0 || y == 0 || x + 1 == width || y + 1 == height
    };
    // Base level: the sea drains everything. Map-border cells are outlets too — a land cell on the
    // edge has off-map neighbours, so without this the flood would wall the whole map in and fill
    // it to the brim. Border LAND still erodes (it is a fill seed, not a frozen sea cell).
    let is_sea = |elev: f32| elev <= base_level;
    // Erosion cannot dig new ocean, and it must not dig a valley so deep that the land mask's
    // elevation ranking drowns it (see `ErosionConfig::incision_floor`). Expressed as a fraction of
    // the land band above the coastline.
    let incision_floor =
        base_level + cfg.incision_floor.clamp(0.0, 1.0) * (1.0 - base_level).max(0.0);

    let mut filled = vec![0.0f32; total];
    let mut visited = vec![false; total];
    let mut downstream = vec![usize::MAX; total];
    let mut accumulation = vec![0.0f32; total];
    let mut order: Vec<usize> = Vec::with_capacity(total);
    let mut snapshot = vec![0.0f32; total];
    let mut heap: BinaryHeap<FloodCell> = BinaryHeap::with_capacity(total);

    for _ in 0..cfg.iterations {
        // (1) Priority-flood + epsilon: every cell gets a filled height that drains monotonically
        // to an outlet, with a tiny gradient laid across the flats the fill creates.
        visited.iter_mut().for_each(|v| *v = false);
        heap.clear();
        for (idx, &elev) in values.iter().enumerate() {
            if is_sea(elev) || is_border(idx) {
                filled[idx] = elev;
                visited[idx] = true;
                heap.push(FloodCell { level: elev, idx });
            }
        }
        while let Some(cell) = heap.pop() {
            for &(dx, dy) in &D8_OFFSETS {
                let Some(n) = neighbor_index(cell.idx, dx, dy, width, height) else {
                    continue;
                };
                if visited[n] {
                    continue;
                }
                visited[n] = true;
                filled[n] = values[n].max(cell.level + cfg.fill_epsilon);
                heap.push(FloodCell {
                    level: filled[n],
                    idx: n,
                });
            }
        }

        // (2) D8 steepest descent on the FILLED surface, so a filled basin still routes out.
        // Steepest = biggest drop per unit distance; ties break on the lowest neighbour index.
        for idx in 0..total {
            let mut best: Option<(f32, usize)> = None;
            for &(dx, dy) in &D8_OFFSETS {
                let Some(n) = neighbor_index(idx, dx, dy, width, height) else {
                    continue;
                };
                if filled[n] >= filled[idx] {
                    continue;
                }
                let gradient = (filled[idx] - filled[n]) / step_length(dx, dy);
                let better = match best {
                    None => true,
                    Some((best_gradient, best_idx)) => match gradient.total_cmp(&best_gradient) {
                        Ordering::Greater => true,
                        Ordering::Equal => n < best_idx,
                        Ordering::Less => false,
                    },
                };
                if better {
                    best = Some((gradient, n));
                }
            }
            downstream[idx] = best.map_or(usize::MAX, |(_, n)| n);
        }

        // (3) Uniform flow accumulation, summed downstream in descending-filled order (a valid
        // topological order: `downstream` is strictly lower on the filled surface).
        accumulation.iter_mut().for_each(|a| *a = 1.0);
        order.clear();
        order.extend(0..total);
        order.sort_unstable_by(|&a, &b| filled[b].total_cmp(&filled[a]).then_with(|| a.cmp(&b)));
        for &idx in &order {
            let d = downstream[idx];
            if d != usize::MAX {
                accumulation[d] += accumulation[idx];
            }
        }

        // (4) Incise. Slope is read on the CURRENT (unfilled) surface, so a filled flat erodes at
        // `min_slope` rather than at the fill's cosmetic gradient. The whole pass reads from a
        // snapshot and writes back, so it is simultaneous — no cell's erosion depends on where it
        // sits in the loop.
        snapshot.copy_from_slice(values);
        for idx in 0..total {
            let elev = snapshot[idx];
            if is_sea(elev) {
                continue; // frozen outlet
            }
            let d = downstream[idx];
            if d == usize::MAX {
                continue; // no outlet on-map: nothing to incise toward
            }
            let (dx, dy) = offset_between(idx, d, width);
            let slope = ((elev - snapshot[d]) / step_length(dx, dy)).max(cfg.min_slope);
            let dz = cfg.erodibility
                * accumulation[idx].powf(cfg.area_exponent)
                * slope.powf(cfg.slope_exponent)
                * cfg.timestep;
            // Never below the downstream neighbour (that would just dig a new pit for the next fill
            // to undo) and never below the incision floor (a valley cut to the coastline contour is
            // *drowned* by the land mask's elevation ranking — it becomes a sea inlet and takes its
            // basin with it). A cell that already sits below the floor simply cannot incise, rather
            // than being lifted onto it. Clamping against the SNAPSHOT is safe: the downstream cell
            // only ever erodes further down from there.
            let floor = incision_floor.min(elev).max(snapshot[d]);
            values[idx] = (elev - dz).max(floor);
        }

        // (5) Hillslope diffusion — the D∇²z half of the model, and the ONLY term that touches the
        // coastline contour: incision is concentrated where A is large, which is never the noisy
        // headwater coast that makes a continent a sponge. The stencil is LAND-ONLY: an ocean
        // neighbour sits far below the coastline (the field's deep water runs to 0), so averaging
        // it in would not smooth the coast, it would suck the whole coastal band under the contour
        // and re-crenellate it. Diffusion here is a *relief* smoother, not a coastal blur.
        if cfg.diffusivity > 0.0 {
            snapshot.copy_from_slice(values);
            for idx in 0..total {
                let elev = snapshot[idx];
                if is_sea(elev) {
                    continue; // frozen base level
                }
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for &(dx, dy) in &D8_OFFSETS {
                    let Some(n) = neighbor_index(idx, dx, dy, width, height) else {
                        continue;
                    };
                    if is_sea(snapshot[n]) {
                        continue;
                    }
                    sum += snapshot[n];
                    count += 1.0;
                }
                if count == 0.0 {
                    continue;
                }
                let laplacian = sum / count - elev;
                values[idx] = (elev + cfg.diffusivity * cfg.timestep * laplacian).max(base_level);
            }
        }
    }
}

/// D8 step length in cells: 1 orthogonally, √2 diagonally.
#[inline]
fn step_length(dx: i32, dy: i32) -> f32 {
    if dx != 0 && dy != 0 {
        std::f32::consts::SQRT_2
    } else {
        1.0
    }
}

#[inline]
fn neighbor_index(idx: usize, dx: i32, dy: i32, width: usize, height: usize) -> Option<usize> {
    let x = (idx % width) as i32 + dx;
    let y = (idx / width) as i32 + dy;
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        return None;
    }
    Some(y as usize * width + x as usize)
}

/// The D8 offset that steps `from` → `to` (they are adjacent by construction).
#[inline]
fn offset_between(from: usize, to: usize, width: usize) -> (i32, i32) {
    let dx = (to % width) as i32 - (from % width) as i32;
    let dy = (to / width) as i32 - (from / width) as i32;
    (dx, dy)
}

fn normalise_values(values: Vec<f32>) -> Vec<f32> {
    let mut min_v = f32::MAX;
    let mut max_v = f32::MIN;
    for &v in &values {
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    let scale = if (max_v - min_v).abs() < f32::EPSILON {
        1.0
    } else {
        1.0 / (max_v - min_v)
    };
    let mut normalised = values;
    for v in &mut normalised {
        *v = (*v - min_v) * scale;
    }
    normalised
}

fn fbm_noise(x: f32, y: f32, octaves: u32, lacunarity: f32, gain: f32, seed: u32) -> f32 {
    let mut frequency = 1.0;
    let mut amplitude = 1.0;
    let mut sum = 0.0;
    let mut normaliser = 0.0;
    for i in 0..octaves {
        let s = seed.wrapping_add(i);
        sum += value_noise(x * frequency, y * frequency, s) * amplitude;
        normaliser += amplitude;
        frequency *= lacunarity;
        amplitude *= gain;
    }
    (sum / normaliser).clamp(0.0, 1.0)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let xf = x - x0 as f32;
    let yf = y - y0 as f32;

    let v00 = hash2(x0, y0, seed);
    let v10 = hash2(x0 + 1, y0, seed);
    let v01 = hash2(x0, y0 + 1, seed);
    let v11 = hash2(x0 + 1, y0 + 1, seed);

    let i1 = lerp(v00, v10, smooth_step(xf));
    let i2 = lerp(v01, v11, smooth_step(xf));
    lerp(i1, i2, smooth_step(yf))
}

fn smooth_step(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = x as u32;
    n = n.wrapping_mul(0x6C8E_9CF5) ^ (y as u32).wrapping_mul(0xB529_7A4D) ^ seed;
    n ^= n >> 13;
    n = n.wrapping_mul(0x1B56_C4E9);
    n ^= n >> 11;
    ((n >> 8) & 0xFFFF) as f32 / 65535.0
}

fn mix_seed(base: u32, seed: u64, salt: u32) -> u32 {
    let seed_low = seed as u32;
    let seed_high = (seed >> 32) as u32;
    base ^ seed_low.rotate_left(7) ^ seed_high.rotate_left(11) ^ salt
}
