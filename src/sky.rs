//! Sky-driven ambient light.
//!
//! The visible sky (a gradient dome that changes with the sun) is also the
//! world's fill light: instead of a flat ambient color, every surface receives
//! the sky's colored light *from the direction it faces* — warm toward the
//! sunset, blue toward the zenith, dark toward the ground. Because the sky is
//! low-frequency, its diffuse irradiance is captured to ~1% by nine spherical-
//! harmonic coefficients (Ramamoorthi & Hanrahan 2001), so we project it here,
//! CPU-side, once per frame and hand the shader nine `vec3`s. The shader then
//! evaluates `SH(normal)` — a few dot products — for the ambient term.
//!
//! The radiance model below MIRRORS the low-frequency part of `sky_radiance` in
//! `shader.wgsl` (the day/night gradient, twilight band, and overcast flatten).
//! It deliberately omits the sharp sun core and horizon limn: those are tiny
//! solid angles that contribute nothing to a hemisphere integral, and leaving
//! them out means the visible sky can gain such details without touching this.

use glam::Vec3;

/// Overall strength of the sky fill. Tuned so a clear-noon dome lands near the
/// old flat ambient's brightness — same level, now directional and colored.
const GAIN: f32 = 0.5;

/// How much light the (unlit) ground bounces back up into downward-facing
/// normals. Keeps undersides from going pure black without faking a full GI
/// bounce.
const GROUND: f32 = 0.18;

/// Everything the sky-radiance model needs for one frame. Built in the frame
/// loop from the same values that drive the visible sky.
pub struct SkyParams {
    /// True direction toward the sun (world space), dipping below the horizon
    /// at night — the same vector the sky pass uses.
    pub sun_dir: Vec3,
    /// Weather gloom 0..1 (flattens the dome toward `overcast`).
    pub gloom: f32,
    /// The flat overcast/fog color the dome flattens toward under cloud.
    pub overcast: Vec3,
    /// Cold moon fill folded uniformly into the ambient (a full moon lifts a
    /// night's shadows off the floor; a new moon leaves them near-black).
    pub moon_fill: Vec3,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Low-frequency sky radiance along `dir` (normalized). Mirrors `shader.wgsl`.
fn radiance(dir: Vec3, p: &SkyParams) -> Vec3 {
    let up = dir.y.clamp(0.0, 1.0);
    let se = p.sun_dir.y; // sun elevation

    let day_zenith = Vec3::new(0.18, 0.40, 0.78);
    let day_horizon = Vec3::new(0.66, 0.79, 0.94);
    let day_sky = day_horizon.lerp(day_zenith, up.powf(0.55));

    let night_zenith = Vec3::new(0.001, 0.0016, 0.005);
    let night_horizon = Vec3::new(0.0025, 0.0045, 0.012);
    let night_sky = night_horizon.lerp(night_zenith, up.powf(0.7));

    let day = smoothstep(-0.12, 0.18, se);
    let mut col = night_sky.lerp(day_sky, day);

    // Twilight: dim/warm the dome, then a molten band biased toward the sun.
    let twilight = smoothstep(0.35, 0.0, se) * smoothstep(-0.32, 0.03, se);
    let hb = 1.0 - up;
    let sun_h = Vec3::new(p.sun_dir.x, 0.0, p.sun_dir.z).normalize_or_zero();
    let dir_h = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    let sun_side = dir_h.dot(sun_h).max(0.0);
    col *= 1.0 - 0.5 * twilight;
    let band_w = (hb.powf(3.0) * (0.30 + 0.70 * sun_side) * twilight).clamp(0.0, 0.90);
    col = col.lerp(Vec3::new(1.0, 0.32, 0.06), band_w);

    // Overcast flattens the dome toward the gray fog color.
    col = col.lerp(p.overcast, smoothstep(0.0, 0.85, p.gloom));

    // Below the horizon is ground, not sky: darken it so undersides stay dim.
    if dir.y < 0.0 {
        col *= 1.0 - (1.0 - GROUND) * (-dir.y).clamp(0.0, 1.0);
    }
    col
}

/// Real SH basis (l ≤ 2), same order and constants as the shader's evaluator.
fn sh_basis(d: Vec3) -> [f32; 9] {
    [
        0.282095,
        0.488603 * d.y,
        0.488603 * d.z,
        0.488603 * d.x,
        1.092548 * d.x * d.y,
        1.092548 * d.y * d.z,
        0.315392 * (3.0 * d.z * d.z - 1.0),
        1.092548 * d.x * d.z,
        0.546274 * (d.x * d.x - d.y * d.y),
    ]
}

/// Project the sky into nine RGB SH coefficients, cosine-convolved to diffuse
/// irradiance (and divided by π so the result is a light multiplier applied
/// before surface albedo). The moon fill is folded into the DC term.
pub fn project(p: &SkyParams) -> [Vec3; 9] {
    const N: usize = 256;
    let mut sh = [Vec3::ZERO; 9];
    let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
    for i in 0..N {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / N as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let theta = golden * i as f32;
        let dir = Vec3::new(theta.cos() * r, y, theta.sin() * r);
        let rad = radiance(dir, p);
        let basis = sh_basis(dir);
        for j in 0..9 {
            sh[j] += rad * basis[j];
        }
    }
    // Monte-Carlo weight (uniform sphere), cosine-convolution A_l/π (band 0 = 1,
    // band 1 = 2/3, band 2 = 1/4), and the overall gain.
    let w = 4.0 * std::f32::consts::PI / N as f32;
    let band = [
        1.0,
        2.0 / 3.0,
        2.0 / 3.0,
        2.0 / 3.0,
        0.25,
        0.25,
        0.25,
        0.25,
        0.25,
    ];
    for j in 0..9 {
        sh[j] *= w * band[j] * GAIN;
    }
    // Fold the flat moon fill into the DC term: adding C to sh[0]*Y00 for every
    // normal means adding C / Y00 to sh[0].
    sh[0] += p.moon_fill / 0.282095;
    sh
}
