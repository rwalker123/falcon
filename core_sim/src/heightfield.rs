use std::sync::Arc;

use bevy::prelude::*;

use crate::{map_preset::MapPreset, resources::SimulationConfig};

#[derive(Resource, Debug, Clone)]
pub struct ElevationField {
    pub width: u32,
    pub height: u32,
    values: Arc<Vec<f32>>,
}

impl ElevationField {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Self {
        debug_assert_eq!(values.len(), (width * height) as usize);
        Self {
            width,
            height,
            values: Arc::new(values),
        }
    }

    #[inline]
    pub fn sample(&self, x: u32, y: u32) -> f32 {
        debug_assert!(x < self.width && y < self.height);
        let idx = (y * self.width + x) as usize;
        self.values[idx]
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

    normalise_field(values, width, height)
}

fn normalise_field(values: Vec<f32>, width: u32, height: u32) -> ElevationField {
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
    ElevationField::new(width, height, normalised)
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
