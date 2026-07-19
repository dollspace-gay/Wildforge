//! The juice layer's one engine piece: a tiny client-side particle
//! pool. Presentation only — nothing here may touch the simulation.
//! Debris quads sample a random sub-region of their source tile, so
//! dirt crumbles brown and cobalt sprays blue with zero per-block art.

use glam::Vec3;

use crate::atlas::ATLAS_TILES;
use crate::mesher::Vertex;

pub const CAP: usize = 512;

#[derive(Clone, Copy)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub tile: u16,
    /// Sub-region of the tile to sample: offset (0..1) and scale.
    pub uv_off: (f32, f32),
    pub uv_scale: f32,
    pub size: f32,
    pub age: f32,
    pub ttl: f32,
    pub gravity: f32,
    pub lum: f32,
}

#[derive(Default)]
pub struct Pool {
    pub v: Vec<Particle>,
}

impl Pool {
    /// Add a particle; the oldest dies first when the pool is full.
    pub fn spawn(&mut self, p: Particle) {
        if self.v.len() >= CAP {
            let oldest = self
                .v
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.age.total_cmp(&b.1.age))
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.v.swap_remove(oldest);
        }
        self.v.push(p);
    }

    /// A radial burst of debris cut from `tile` (block breaks, hits).
    pub fn burst(&mut self, at: Vec3, tile: u16, n: usize, speed: f32, rng: &mut u32) {
        let mut r01 = || {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            (*rng >> 8) as f32 / (1 << 24) as f32
        };
        for _ in 0..n {
            let a = r01() * std::f32::consts::TAU;
            let up = 1.5 + r01() * 2.5;
            self.spawn(Particle {
                pos: at + Vec3::new((r01() - 0.5) * 0.4, r01() * 0.4, (r01() - 0.5) * 0.4),
                vel: Vec3::new(
                    a.cos() * speed * (0.4 + r01()),
                    up,
                    a.sin() * speed * (0.4 + r01()),
                ),
                tile,
                uv_off: (r01() * 0.75, r01() * 0.75),
                uv_scale: 0.25,
                size: 0.06 + r01() * 0.07,
                age: 0.0,
                ttl: 0.35 + r01() * 0.35,
                gravity: 14.0,
                lum: 0.85,
            });
        }
    }

    /// A soft ground puff (landings, quern dust): slow, airy, brief.
    pub fn puff(&mut self, at: Vec3, tile: u16, n: usize, rng: &mut u32) {
        let mut r01 = || {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            (*rng >> 8) as f32 / (1 << 24) as f32
        };
        for _ in 0..n {
            let a = r01() * std::f32::consts::TAU;
            self.spawn(Particle {
                pos: at + Vec3::new((r01() - 0.5) * 0.5, 0.05, (r01() - 0.5) * 0.5),
                vel: Vec3::new(a.cos() * 0.8, 0.6 + r01() * 0.6, a.sin() * 0.8),
                tile,
                uv_off: (r01() * 0.75, r01() * 0.75),
                uv_scale: 0.25,
                size: 0.08 + r01() * 0.06,
                age: 0.0,
                ttl: 0.3 + r01() * 0.25,
                gravity: 2.0,
                lum: 0.9,
            });
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.v.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.ttl {
                return false;
            }
            p.vel.y -= p.gravity * dt;
            p.pos += p.vel * dt;
            true
        });
    }

    /// Emit as crossed billboard quads into the entity batch.
    pub fn emit(&self, verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let ts = 1.0 / ATLAS_TILES as f32;
        for p in &self.v {
            let (tx, ty) = (p.tile as u32 % ATLAS_TILES, p.tile as u32 / ATLAS_TILES);
            // Shrink out over the last third of life.
            let fade = ((p.ttl - p.age) / (p.ttl * 0.33)).clamp(0.0, 1.0);
            let s = p.size * (0.5 + 0.5 * fade);
            let u0 = tx as f32 * ts + p.uv_off.0 * ts;
            let v0 = ty as f32 * ts + p.uv_off.1 * ts;
            let span = ts * p.uv_scale;
            for (dx, dz) in [(1.0f32, 0.0f32), (0.0, 1.0)] {
                for flip in [false, true] {
                    let base = verts.len() as u32;
                    let sgn = if flip { -1.0 } else { 1.0 };
                    for (o, dy, uu, vv) in [
                        (-s * sgn, -s, u0, v0 + span),
                        (s * sgn, -s, u0 + span, v0 + span),
                        (s * sgn, s, u0 + span, v0),
                        (-s * sgn, s, u0, v0),
                    ] {
                        verts.push(Vertex {
                            pos: [p.pos.x + dx * o, p.pos.y + dy, p.pos.z + dz * o],
                            uv: [uu, vv],
                            normal: [0.0, 0.0, 0.0],
                            light: [p.lum; 3],
                            sky: p.lum,
                        });
                    }
                    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }
}
