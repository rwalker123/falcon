use std::{cmp::Ordering, collections::BinaryHeap, sync::Arc};

use bevy::prelude::*;

use crate::{
    map_preset::{ErosionConfig, MacroLandConfig, MapPreset},
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

    /// Mutable access to the raw samples, for the **only** legitimate way to move a coastline:
    /// edit the field and re-derive the mask (`mapgen::generate_land_mask`). No stage may write to
    /// `land`/`is_ocean` directly — elevation is the sole authority, so a stage that wants land
    /// where there is water raises the ground, and one that wants water lowers it.
    ///
    /// `Arc::make_mut` clones the buffer only if the field is shared, so the common case (a private
    /// working copy inside `build_bands`) is in-place.
    #[inline]
    pub fn values_mut(&mut self) -> &mut [f32] {
        Arc::make_mut(&mut self.values).as_mut_slice()
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

    // Continental structure + coastline raggedness go into the field BEFORE erosion and before the
    // contour is anchored to sea level, because the land mask is a pure threshold of this field:
    // `land = elevation > sea_level`. Anything that wants to move a coastline edits the field here.
    if let Some(p) = preset {
        apply_continental_bias(
            &mut values,
            width as usize,
            height as usize,
            &p.macro_land,
            seed,
            config.map_topology.wrap_horizontal,
        );
        apply_coastline_roughness(
            &mut values,
            width as usize,
            height as usize,
            &p.macro_land,
            seed,
        );
        values = normalise_values(values);
    }

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

    // Attach the preset's sea level HERE, at the field's origin, rather than leaving every
    // downstream consumer to remember `with_sea_level`. `ElevationField::new` resets to
    // `DEFAULT_SEA_LEVEL`, so a field that travels through any constructor without it silently
    // reverts — which is exactly how the snapshot came to ship 0.6 while `earthlike` specifies 0.62.
    let sea_level = preset.map_or(DEFAULT_SEA_LEVEL, |p| p.sea_level);
    ElevationField::new(width, height, values).with_sea_level(sea_level)
}

/// Hash salt for the continent-centre sampler, so the centres are a deterministic function of the
/// world seed alone and share no stream with the fractal/ridge/roughness noise.
const CONTINENT_CENTER_SEED_SALT: u32 = 0x5EED_C047;
/// Hash salt for the coastline-roughness noise field, likewise disjoint from every other stream.
const COASTLINE_ROUGHNESS_SEED_SALT: u32 = 0x0C0A_5751;
/// How many candidate centres the Poisson-ish sampler may reject before it accepts one anyway. The
/// spacing rule is a *preference*, not a hard constraint — with many continents on a small grid the
/// rejection region can cover most of the map, and a sampler that could fail to place a centre would
/// silently deliver fewer continents than the preset asked for.
const CONTINENT_CENTER_MAX_ATTEMPTS: u32 = 64;
/// The minimum centre separation, as a fraction of `sqrt(grid_area / continents)` — the same
/// "one continent's worth of area per continent" spacing rule the retired BFS seed placement used
/// (`mapgen::generate_land_mask`), expressed in continuous coordinates. Below 1.0 so a crowded map
/// can still seat every centre without exhausting the attempt budget.
const CONTINENT_MIN_SEPARATION_FRACTION: f32 = 0.75;
/// Hash salt for the **domain-warp** noise field — the low-frequency displacement applied to the
/// sample coordinates before the continental envelope is evaluated. Disjoint from every other stream.
const CONTINENT_WARP_SEED_SALT: u32 = 0x7A1D_0FE5;
/// Hash salt for the **ridged-spine** noise field, likewise disjoint.
const CONTINENT_SPINE_SEED_SALT: u32 = 0x5B1D_9E77;
/// Hash salt for the per-centre **tilt direction**. Each centre draws one angle from this stream, so
/// the tilt is a pure function of `(world_seed, centre_index)` — no RNG, no iteration order.
const CONTINENT_TILT_SEED_SALT: u32 = 0x71B7_0A21;
/// Octaves / lacunarity / gain of the domain-warp fbm. A short stack: the warp exists to make a
/// continent *lobed*, which is large-scale shape, not texture — high octaves would fray the coastline
/// (that is [`apply_coastline_roughness`]'s job) instead of moving the landmass.
const CONTINENT_WARP_OCTAVES: u32 = 3;
const CONTINENT_WARP_LACUNARITY: f32 = 2.0;
const CONTINENT_WARP_GAIN: f32 = 0.5;
/// Shape of the window `1 - t^exponent` that confines the per-continent tilt to that continent's own
/// footprint. **Deliberately high**: the window must be near-flat across the interior — where the
/// drainage direction is decided — and collapse only at the rim. A gentle window (exponent 1) merely
/// shifts the dome's summit a little off-centre and the drainage stays radial (measured: navigable
/// rivers fell back to 0–1 per map), while *no* window lets the tilt plane climb without bound past
/// the radius and ramp every continent into one landmass (measured: landmasses ≥ `min_area` collapsed
/// to 1).
const CONTINENT_TILT_WINDOW_EXPONENT: f32 = 4.0;
/// How hard the tilt term lifts the ground **away from** a continent's drainage axis, as a fraction
/// of `continental_tilt_strength`. This is what turns a tilted plane (parallel flow, forty short
/// rivers) into a tilted trough (convergent flow, one trunk). Tied to the tilt lever rather than
/// broken out on its own, because a trough with no down-axis gradient has nowhere to drain **to** and
/// a gradient with no trough never gathers — they are two halves of one structure, not two dials.
const CONTINENT_TROUGH_GAIN: f32 = 0.5;
/// Octaves / lacunarity / gain of the ridged-spine fbm. One octave more than the warp, because a
/// divide reads as a *range* — a trunk ridge with subsidiary spurs — rather than a single smooth arch.
const CONTINENT_SPINE_OCTAVES: u32 = 4;
const CONTINENT_SPINE_LACUNARITY: f32 = 2.0;
const CONTINENT_SPINE_GAIN: f32 = 0.5;
/// Sharpness of the ridged transform `(1 - |2n - 1|)^exponent`. Above 1 the ridge crest narrows and
/// its flanks fall away faster, which is what makes it read as a **divide** (two drainage sides) and
/// not as a broad dome with a bump on it.
const CONTINENT_SPINE_RIDGE_EXPONENT: f32 = 2.0;
/// Cycles of coastline-roughness noise across the map's smaller dimension. High enough that it
/// perturbs the coastline tile-to-tile rather than moving whole landmasses (that is
/// [`apply_continental_bias`]'s job), low enough that the fbm's octaves stay above the raster's
/// Nyquist limit.
const COASTLINE_ROUGHNESS_FREQUENCY: f32 = 24.0;
/// Octaves / lacunarity / gain of the coastline-roughness fbm — a short, fast-decaying stack, since
/// this term exists to add fine detail, not structure.
const COASTLINE_ROUGHNESS_OCTAVES: u32 = 3;
const COASTLINE_ROUGHNESS_LACUNARITY: f32 = 2.0;
const COASTLINE_ROUGHNESS_GAIN: f32 = 0.5;

/// The low-frequency **continental bias**: `elevation += continental_weight * bias(x, y)`, where
/// `bias = max_i(envelope_i + tilt_i)` over `macro_land.continents` seed-derived centres, plus a
/// ridged **spine** gated to the continent interiors.
///
/// Properties that are load-bearing:
/// - **`max`, not sum.** Summing lets two nearby centres add into a land bridge, fusing exactly the
///   continents the lever exists to separate; the maximum keeps each continent's profile its own.
/// - **The envelope spans `[-1, 1]`, not `[0, 1]`.** A bias that only ever *adds* height leaves the
///   inter-continental gaps merely lower than the continents, which after renormalisation and the
///   contour anchor can still land above sea level. Reaching `-1` actively sinks them.
/// - **The envelope alone is a DOME, and a dome sheds water radially.** Every direction off a radial
///   falloff is downhill, so a continent decomposes into many short independent basins instead of a
///   few long trunk drainages — measured: the largest basin was ~5% of its landmass and max drainage
///   accumulation topped out at 10–20 against a navigable threshold of 25, so 80×52 maps grew **no
///   rivers at all**, and with no rivers there were no `RiverDelta`/`Floodplain` tiles and therefore
///   **zero** ground the `plant:field` rung could take seed on. The three terms below exist to give a
///   continent **internal divides and directional drainage**, which the dome cannot have:
///   1. **Domain warp** — the sample coordinates are displaced by low-frequency noise *before* the
///      envelope is evaluated, so continents are irregular and lobed rather than circular.
///   2. **Per-continent tilt** — a directional gradient across each centre, its direction drawn
///      deterministically from the world seed. **A tilted surface drains ONE way**, which is what
///      turns radial spokes into a long trunk basin. **Both shipped presets set
///      `continental_tilt_strength` to `0.0`, so this term is inert on the maps a player gets**: it
///      buys drainage but bridges continents into a supercontinent, which starves the fold-belt
///      plate network (see [`MacroLandConfig::continental_tilt_strength`]). The code stays because
///      the lever stays live — nothing below is conditional on it being zero.
///   3. **Ridged spine** — a ridged-noise term gated to the continent interior, so a continent
///      carries an internal divide (two drainage sides) rather than a single summit. With the tilt
///      off, this and the warp are what shape divides on the shipped maps.
///
/// Everything here is pure arithmetic over seed-derived hashes — no RNG, no map iteration — as
/// `integration_tests::determinism` requires.
fn apply_continental_bias(
    values: &mut [f32],
    width: usize,
    height: usize,
    cfg: &MacroLandConfig,
    seed: u64,
    wrap_horizontal: bool,
) {
    let weight = cfg.continental_weight;
    let continents = cfg.continents.max(1) as usize;
    if weight <= 0.0 || width == 0 || height == 0 {
        return;
    }
    let min_dim = width.min(height) as f32;
    let radius = cfg.continental_radius.max(f32::EPSILON) * min_dim;
    let exponent = cfg.continental_falloff_exponent.max(f32::EPSILON);
    let centers = continent_centers(width, height, continents, seed, wrap_horizontal);
    // One unit tilt direction per centre, hashed from `(world_seed, index)` — so which way a
    // continent drains is a property of the map, reproducibly, and two centres never share a heading
    // by accident of iteration order.
    let tilt_dirs: Vec<(f32, f32)> = (0..centers.len())
        .map(|index| {
            let hash_seed = mix_seed(CONTINENT_TILT_SEED_SALT, seed, index as u32);
            let angle = hash2(index as i32, index as i32, hash_seed) * std::f32::consts::TAU;
            (angle.cos(), angle.sin())
        })
        .collect();

    let warp_amplitude = cfg.continental_warp_amplitude.max(0.0) * min_dim;
    let warp_frequency = cfg.continental_warp_frequency.max(0.0);
    let warp_seed_x = mix_seed(CONTINENT_WARP_SEED_SALT, seed, 0);
    let warp_seed_y = mix_seed(CONTINENT_WARP_SEED_SALT, seed, 1);
    let spine_amplitude = cfg.continental_spine_amplitude.max(0.0);
    let spine_frequency = cfg.continental_spine_frequency.max(0.0);
    let spine_seed = mix_seed(CONTINENT_SPINE_SEED_SALT, seed, 0);
    let basin_amplitude = cfg.continental_basin_amplitude.max(0.0);
    // One line per worldgen recording the lake lever the code actually saw. Doubles as a build probe:
    // if this line is ABSENT from a server's log, that binary predates the interior-sink term; if it
    // reads `basin_amplitude=0`, the config never reached worldgen. See `map_preset.rs`
    // `continental_basin_amplitude` and CLAUDE.md → "Lakes are emergent".
    tracing::info!(
        target: "shadow_scale::mapgen",
        basin_amplitude,
        "mapgen.macro_land.interior_sink"
    );
    let aspect = width as f32 / height.max(1) as f32;

    for y in 0..height {
        for x in 0..width {
            // (1) Domain warp. `fbm_noise` is 0..1, so centring it makes the displacement symmetric —
            // the warp reshapes a continent without translating the whole field.
            let nx = x as f32 / width as f32;
            let ny = y as f32 / height as f32;
            let (warp_x, warp_y) = if warp_amplitude > 0.0 && warp_frequency > 0.0 {
                let sx = nx * warp_frequency * aspect;
                let sy = ny * warp_frequency;
                let dx = fbm_noise(
                    sx,
                    sy,
                    CONTINENT_WARP_OCTAVES,
                    CONTINENT_WARP_LACUNARITY,
                    CONTINENT_WARP_GAIN,
                    warp_seed_x,
                ) - 0.5;
                let dy = fbm_noise(
                    sx,
                    sy,
                    CONTINENT_WARP_OCTAVES,
                    CONTINENT_WARP_LACUNARITY,
                    CONTINENT_WARP_GAIN,
                    warp_seed_y,
                ) - 0.5;
                (2.0 * warp_amplitude * dx, 2.0 * warp_amplitude * dy)
            } else {
                (0.0, 0.0)
            };
            let sample_x = x as f32 + warp_x;
            let sample_y = y as f32 + warp_y;

            let mut bias = -1.0f32;
            for (center, &(ux, uy)) in centers.iter().zip(tilt_dirs.iter()) {
                let &(cx, cy) = center;
                // The signed offset is what the tilt needs; `torus_delta` yields only a magnitude, so
                // the sign is recovered from the unwrapped delta and re-applied.
                let raw_dx = sample_x - cx;
                let dx = torus_delta(sample_x, cx, width as f32, wrap_horizontal)
                    * if raw_dx < 0.0 { -1.0 } else { 1.0 };
                let dy = sample_y - cy;
                let t = ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
                // (2) Per-continent tilt: a plane through the centre, normalised by the radius so the
                // strength lever is in the same units as the envelope's own `[-1, 1]` span — and
                // **windowed by `(1 - t)` so it vanishes at the continent's rim**.
                //
                // The window is not cosmetic. `t` saturates at 1 beyond the radius but a raw plane does
                // not: an unwindowed tilt keeps climbing with distance, so it lifts the far field into
                // a ramp that bridges every centre. Measured, that collapsed landmasses ≥ `min_area`
                // from 3–5 to **1** on every seed while the tilt was strong enough to make rivers. The
                // window keeps the tilt where drainage is decided — the interior — and leaves the
                // coastline the envelope's business.
                let along = ((dx * ux + dy * uy) / radius).clamp(-1.0, 1.0);
                // The cross-axis coordinate: `u` rotated a quarter turn.
                let across = ((dx * -uy + dy * ux) / radius).clamp(-1.0, 1.0);
                let window = 1.0 - t.powf(CONTINENT_TILT_WINDOW_EXPONENT);
                // A bare tilt is not enough, and this is the correction the measurements forced.
                // A tilted *plane* drains one way but drains it in **parallel**: every column runs
                // straight down its own strip to the coast, so a 40-tile continent yields forty
                // 20-long rivers rather than one trunk. Convergence needs the cross-section to be
                // **concave** — high flanks, low axis — which is what real trunk basins sit in
                // (Mississippi, Amazon, Congo all occupy a structural low between highland rims).
                //
                // So the term is a *tilted trough*, not a tilted plane: `tilt_strength` sets the
                // down-axis gradient that decides which way it drains, and `trough_gain` (a fraction
                // of it, so one lever still governs the whole structure) raises the ground away from
                // the axis so flow gathers onto it before running down.
                let tilt = cfg.continental_tilt_strength
                    * (along + CONTINENT_TROUGH_GAIN * across.abs())
                    * window;
                bias = bias.max(1.0 - 2.0 * t.powf(exponent) + tilt);
            }

            // (3) Ridged spine, gated by the continent envelope so it raises divides *inside* a
            // landmass instead of scattering seamounts across the ocean. `clamp(bias, 0, 1)` is that
            // gate: it is 0 everywhere the bias is already sinking the gaps.
            if spine_amplitude > 0.0 && spine_frequency > 0.0 {
                let raw = fbm_noise(
                    nx * spine_frequency * aspect,
                    ny * spine_frequency,
                    CONTINENT_SPINE_OCTAVES,
                    CONTINENT_SPINE_LACUNARITY,
                    CONTINENT_SPINE_GAIN,
                    spine_seed,
                );
                let ridged = (1.0 - (raw - 0.5).abs() * 2.0)
                    .clamp(0.0, 1.0)
                    .powf(CONTINENT_SPINE_RIDGE_EXPONENT);
                bias += spine_amplitude * ridged * bias.clamp(0.0, 1.0);
            }

            // (4) The interior sink — the endorheic-lake term. It planes the continent INTERIOR down
            // toward the contour, `bias -= basin_amplitude * bias.clamp(0, 1)`: the top of the dome
            // becomes a broad near-sea-level plateau while the coastal rim (`bias ≤ 0`, gate 0) is
            // left exactly as it was.
            //
            // Why this, and not discrete carved bowls (the shape this term started as, and the reason
            // the config name says "basin"): a bowl gouged into high interior mostly drains to the sea
            // and reads as a coastal inlet — measured, it barely moved the lake count at any depth.
            // What actually makes lakes is a large area sitting *just above* the contour, where the
            // field's own fine-scale fbm dips below it in many small enclosed pools; lowering the
            // interior plateau is what creates that zone. It is the same mechanism as reducing
            // `continental_weight`, with one decisive difference: the `bias.clamp(0, 1)` gate pins the
            // COAST (the dome is dented, not shrunk), so the cold-ocean coastline the seal habitat
            // sits on is untouched — which is the whole reason this exists as its own term rather than
            // as a smaller `continental_weight`.
            if basin_amplitude > 0.0 {
                bias -= basin_amplitude * bias.clamp(0.0, 1.0);
            }

            values[y * width + x] += weight * bias;
        }
    }
}

/// Poisson-ish continent centres, sampled deterministically from the world seed in **continuous**
/// grid coordinates: reject a candidate that lands closer than the spacing rule to an accepted one,
/// and accept unconditionally once the attempt budget is spent so the count is always honored.
fn continent_centers(
    width: usize,
    height: usize,
    continents: usize,
    seed: u64,
    wrap_horizontal: bool,
) -> Vec<(f32, f32)> {
    let total = (width * height) as f32;
    let min_separation = (total / continents as f32).sqrt() * CONTINENT_MIN_SEPARATION_FRACTION;
    let min_separation_sq = min_separation * min_separation;
    let mut centers: Vec<(f32, f32)> = Vec::with_capacity(continents);

    for index in 0..continents {
        for attempt in 0..CONTINENT_CENTER_MAX_ATTEMPTS {
            let salt = CONTINENT_CENTER_SEED_SALT
                .wrapping_add((index as u32).wrapping_mul(CONTINENT_CENTER_MAX_ATTEMPTS))
                .wrapping_add(attempt);
            let hash_seed = mix_seed(CONTINENT_CENTER_SEED_SALT, seed, salt);
            let cx = hash2(index as i32, attempt as i32, hash_seed) * width as f32;
            let cy = hash2(attempt as i32, index as i32, hash_seed) * height as f32;
            let spaced = centers.iter().all(|&(ex, ey)| {
                let dx = torus_delta(cx, ex, width as f32, wrap_horizontal);
                let dy = cy - ey;
                dx * dx + dy * dy >= min_separation_sq
            });
            if spaced || attempt + 1 == CONTINENT_CENTER_MAX_ATTEMPTS {
                centers.push((cx, cy));
                break;
            }
        }
    }
    centers
}

/// Shortest **absolute** x-separation, taking the short way around when the map wraps horizontally.
/// The result is a magnitude, never a signed offset — callers only ever square it.
#[inline]
fn torus_delta(a: f32, b: f32, span: f32, wrap: bool) -> f32 {
    let d = (a - b).abs();
    if wrap {
        d.min(span - d)
    } else {
        d
    }
}

/// The high-frequency term that gives the coastline its raggedness, applied **before**
/// [`land_contour`] so the anchor runs on the field that is actually thresholded. This is where the
/// retired land-mask `jitter` belongs: perturbing the field is a coastline detail, perturbing the
/// mask's *ranking* was a decoupling.
fn apply_coastline_roughness(
    values: &mut [f32],
    width: usize,
    height: usize,
    cfg: &MacroLandConfig,
    seed: u64,
) {
    let amplitude = cfg.coastline_roughness;
    if amplitude <= 0.0 || width == 0 || height == 0 {
        return;
    }
    let noise_seed = mix_seed(COASTLINE_ROUGHNESS_SEED_SALT, seed, 0);
    let aspect = width as f32 / height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let nx = x as f32 / width as f32 * COASTLINE_ROUGHNESS_FREQUENCY * aspect;
            let ny = y as f32 / height as f32 * COASTLINE_ROUGHNESS_FREQUENCY;
            let noise = fbm_noise(
                nx,
                ny,
                COASTLINE_ROUGHNESS_OCTAVES,
                COASTLINE_ROUGHNESS_LACUNARITY,
                COASTLINE_ROUGHNESS_GAIN,
                noise_seed,
            );
            // fbm is 0..1; centre it so roughness perturbs the coastline symmetrically instead of
            // adding a net uplift that would shift the land fraction.
            values[y * width + x] += amplitude * (noise - 0.5) * 2.0;
        }
    }
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
            // only ever erodes further down from there. INVARIANT: incision never RAISES a cell — the
            // floor is clamped to at most `elev` on BOTH terms, so `values[idx] <= elev` always.
            // Inside a filled depression the downstream neighbour can sit above `elev`
            // (the `.max(cfg.min_slope)` above exists for exactly that negative-slope case);
            // `snapshot[d].min(elev)` stops that from lifting the cell onto its neighbour. On the dry
            // side (`snapshot[d] < elev`) the `.min(elev)` is a no-op, so the "never below downstream"
            // behaviour is unchanged.
            let floor = incision_floor.min(elev).max(snapshot[d].min(elev));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_preset::MapPresets;

    /// `anchor_contour_to_sea_level` is the safety property the whole erosion placement rests on:
    /// it reshapes the field *before* `generate_land_mask`, and it is only safe because it is a
    /// strictly monotone rescale, so it cannot reorder the field and therefore cannot change which
    /// tiles the mask ranks as land. If it ever stopped being monotone (e.g. a plateau at the
    /// contour breakpoint), the land mask would silently shift. Nothing else pins this.
    #[test]
    fn anchor_contour_to_sea_level_is_strictly_monotone_and_pinned() {
        // A field spanning [0, 1] with the two pin points (0.0, the contour, 1.0) explicitly present,
        // plus dense samples straddling the breakpoint where a discontinuity would hide.
        let contour = 0.55f32;
        let sea_level = 0.6f32;
        let mut values: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();
        // Deterministic non-monotone input order, so the test checks value-ordering, not slot-order.
        values.rotate_left(337);
        let before = values.clone();

        anchor_contour_to_sea_level(&mut values, contour, sea_level);

        // (a) Order-preserving: for every pair, the mapped values keep the same strict ordering as
        // the inputs — the exact property that keeps the land-mask rank cut unchanged. A single
        // reordering (or a plateau collapsing two distinct inputs to equal outputs) fails this.
        for i in 0..before.len() {
            for j in (i + 1)..before.len() {
                let ord_in = before[i].total_cmp(&before[j]);
                let ord_out = values[i].total_cmp(&values[j]);
                assert_eq!(
                    ord_in, ord_out,
                    "reordered {}->{} vs {}->{}",
                    before[i], values[i], before[j], values[j]
                );
            }
        }

        // (b) Pinned at all three anchor points.
        let map = |v: f32| {
            let mut a = [v];
            anchor_contour_to_sea_level(&mut a, contour, sea_level);
            a[0]
        };
        assert!((map(0.0) - 0.0).abs() < 1e-6, "phi(0) != 0");
        assert!(
            (map(contour) - sea_level).abs() < 1e-6,
            "phi(contour) != sea_level"
        );
        assert!((map(1.0) - 1.0).abs() < 1e-6, "phi(1) != 1");
    }

    /// The erosion kill switch is also the A/B control the census leans on: `enabled = false` must
    /// leave the field completely untouched, *regardless of the other erosion knobs*. This pins that
    /// the disabled path short-circuits before reading any other lever or mutating a single cell —
    /// and, so the test can't pass vacuously, that `enabled = true` genuinely changes the field.
    #[test]
    fn erosion_disabled_is_inert_to_every_other_lever() {
        let presets = MapPresets::builtin();
        let preset = presets.get("earthlike").expect("earthlike preset");
        let seed = 0xC0FF_EE01u64; // any fixed seed; erosion is deterministic
        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);

        let build = |erosion: ErosionConfig| {
            let mut p = preset.clone();
            p.erosion = erosion;
            build_elevation_field(&config, Some(&p), seed)
                .values
                .to_vec()
        };

        // Two disabled configs whose *other* levers are wildly different.
        let off_tame = build(ErosionConfig {
            enabled: false,
            iterations: 1,
            erodibility: 0.0,
            diffusivity: 0.0,
            ..preset.erosion.clone()
        });
        let off_extreme = build(ErosionConfig {
            enabled: false,
            iterations: 250,
            erodibility: 5.0,
            diffusivity: 5.0,
            ..preset.erosion.clone()
        });
        assert_eq!(
            off_tame, off_extreme,
            "enabled=false is not inert: other erosion levers perturbed the field"
        );

        // And the switch is not a global no-op: enabling erosion actually moves the field.
        let on = build(ErosionConfig {
            enabled: true,
            ..preset.erosion.clone()
        });
        assert_ne!(
            off_tame, on,
            "enabled=true left the field identical to disabled — erosion did nothing"
        );
    }

    /// Incision is NON-INCREASING per cell: `apply_fluvial_erosion` may lower or hold a cell, but it
    /// must never RAISE one. This pins the fix for the "erosion lifts terrain in a pit" artifact —
    /// inside a filled depression the steepest-descent downstream neighbour can sit *above* the
    /// current cell (the `.max(min_slope)` in the incision step exists for exactly that negative-slope
    /// case), and an unclamped downstream floor would set `values[idx] = snapshot[d] > elev`, lifting
    /// the pit bottom onto its neighbour. Diffusion legitimately raises valley cells (∇²z averaging),
    /// so it is disabled here to isolate the incision term the invariant applies to.
    #[test]
    fn incision_never_raises_a_cell() {
        // A field with a hard enclosed basin: a low pit ringed by a tall wall, draining out one gap
        // to a border outlet. The fill floods the pit and the wall-interior flats, so the incision
        // step routes several cells toward a *higher* filled neighbour — the exact case the bug hit.
        let (width, height) = (9usize, 9usize);
        let (cx, cy) = (4i32, 4i32);
        let mut values = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = (x as i32 - cx, y as i32 - cy);
                let cheby = dx.abs().max(dy.abs());
                let elev = match cheby {
                    0 => 0.10, // pit bottom
                    1 => 0.20, // basin floor
                    2 => 0.90, // ring wall
                    _ => 0.50, // outer slope draining to the border
                };
                values[y * width + x] = elev;
            }
        }
        // A gap in the wall so the basin has a genuine spill point (otherwise the fill still resolves,
        // but this keeps the drainage realistic).
        values[(cy as usize) * width + (cx as usize + 2)] = 0.30;

        let before = values.clone();
        let cfg = ErosionConfig {
            enabled: true,
            iterations: 20,
            erodibility: 0.5, // strong incision, so the term genuinely bites
            area_exponent: 0.5,
            slope_exponent: 1.0,
            timestep: 0.5,
            min_slope: 1e-4,
            fill_epsilon: 1e-6,
            diffusivity: 0.0, // isolate incision — diffusion may legitimately raise valley cells
            incision_floor: 0.0,
            anchor_contour_to_sea_level: false,
        };
        // base_level below every cell: nothing is frozen sea, so every non-border cell incises.
        apply_fluvial_erosion(&mut values, width, height, -1.0, &cfg);

        for (i, (&after, &orig)) in values.iter().zip(before.iter()).enumerate() {
            assert!(
                after <= orig + 1e-6,
                "cell {i} was RAISED by incision: {orig} -> {after}"
            );
        }
        // Non-vacuous: with strong incision on a real basin, at least one cell must actually drop.
        assert!(
            values
                .iter()
                .zip(before.iter())
                .any(|(&a, &o)| a < o - 1e-6),
            "erosion left every cell unchanged — the test isn't exercising incision"
        );
    }
}
