//! Item entities (dropped blocks): mini-cubes that fall, bob, spin, and get
//! picked up. Also builds the crack-overlay mesh for the block being mined.

use glam::Vec3;

use crate::atlas::{ATLAS_TILES, CRACK_SLOT};
use crate::mesher::{CORNERS, FACE_SHADE, Vertex};
use crate::registry::{ItemId, Registry};
use crate::world::World;

pub struct ItemEntity {
    pub pos: Vec3, // center of the mini-cube
    pub vel: Vec3,
    pub item: ItemId,
    pub count: u32,
    pub age: f32,
    /// Non-zero: this drop carries specific durability (worn ruin tools).
    pub durability: u32,
}

const SIZE: f32 = 0.25;
const DESPAWN: f32 = 300.0;
/// Age before the dropping player can pick it back up.
pub const PICKUP_DELAY: f32 = 0.6;

impl ItemEntity {
    pub fn new(pos: Vec3, vel: Vec3, item: ItemId, count: u32) -> ItemEntity {
        ItemEntity { pos, vel, item, count, age: 0.0, durability: 0 }
    }

    /// Returns false when the entity should despawn.
    pub fn update(&mut self, world: &World, dt: f32) -> bool {
        self.age += dt;
        if self.age > DESPAWN {
            return false;
        }
        self.vel.y -= 16.0 * dt;
        self.vel.y = self.vel.y.max(-30.0);
        // Horizontal drag.
        let drag = (1.0 - 4.0 * dt).max(0.0);
        self.vel.x *= drag;
        self.vel.z *= drag;

        let mut next = self.pos + self.vel * dt;
        let half = SIZE / 2.0;
        // Floor collision at the bottom of the cube.
        let bx = next.x.floor() as i32;
        let bz = next.z.floor() as i32;
        let by = (next.y - half).floor() as i32;
        if world.reg.is_solid(world.get_block(bx, by, bz)) && self.vel.y < 0.0 {
            next.y = by as f32 + 1.0 + half;
            self.vel.y = 0.0;
        }
        // Simple side collision: don't move into solid blocks.
        let cx = next.x.floor() as i32;
        let cy = next.y.floor() as i32;
        let cz = next.z.floor() as i32;
        if world.reg.is_solid(world.get_block(cx, cy, cz)) {
            next = self.pos;
            self.vel.x = 0.0;
            self.vel.z = 0.0;
        }
        self.pos = next;
        true
    }

    /// Emit this entity as a spinning, bobbing mini-cube (blocks) or a
    /// crossed pair of upright sprite quads (tools, sticks).
    pub fn emit(&self, reg: &Registry, lum: (f32, f32), verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let block = match reg.item(self.item).places {
            Some(b) if !reg.block(b).cross => b,
            _ => {
                self.emit_sprite(reg, lum, verts, idx);
                return;
            }
        };
        let half = SIZE / 2.0;
        let bob = (self.age * 2.2).sin() * 0.05;
        let ang = self.age * 1.5;
        let (sin, cos) = ang.sin_cos();
        let center = self.pos + Vec3::new(0.0, bob, 0.0);

        for face in 0..6 {
            let slot = reg.block(block).tiles[face];
            let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
            let ts = 1.0 / ATLAS_TILES as f32;
            let inset = ts / 32.0;
            let base = verts.len() as u32;
            for c in CORNERS[face].iter() {
                // Cube corner in local space, spun around Y.
                let lx = (c[0] - 0.5) * SIZE;
                let ly = (c[1] - 0.5) * SIZE + half; // rest on the ground
                let lz = (c[2] - 0.5) * SIZE;
                let rx = lx * cos - lz * sin;
                let rz = lx * sin + lz * cos;
                let (u, v) = match face {
                    0 | 1 => (c[2], 1.0 - c[1]),
                    4 | 5 => (c[0], 1.0 - c[1]),
                    _ => (c[0], c[2]),
                };
                let shade = FACE_SHADE[face].max(0.7);
                verts.push(Vertex {
                    pos: [center.x + rx, center.y + ly - half, center.z + rz],
                    uv: [
                        tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                        ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                    ],
                    normal: [0.0, 0.0, 0.0],
                    light: [shade * lum.0; 3],
                    sky: shade * lum.1,
                });
            }
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

impl ItemEntity {
    fn emit_sprite(&self, reg: &Registry, lum: (f32, f32), verts: &mut Vec<Vertex>, idx: &mut Vec<u32>) {
        let slot = reg.item(self.item).icon;
        let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
        let ts = 1.0 / ATLAS_TILES as f32;
        let inset = ts / 32.0;
        let bob = (self.age * 2.2).sin() * 0.05;
        let ang = self.age * 1.5;
        let (sin, cos) = ang.sin_cos();
        let c = self.pos + Vec3::new(0.0, bob, 0.0);
        let h = 0.35; // sprite size

        // Two crossed upright quads, spun around Y (drawn double-sided).
        for (dx, dz) in [(cos, sin), (-sin, cos)] {
            for flip in [false, true] {
                let base = verts.len() as u32;
                let (u0, u1) = if flip {
                    ((tx + 1) as f32 * ts - inset, tx as f32 * ts + inset)
                } else {
                    (tx as f32 * ts + inset, (tx + 1) as f32 * ts - inset)
                };
                let s = if flip { -1.0 } else { 1.0 };
                let corners = [
                    (-0.5 * h * s, -0.5 * h, u0),
                    (0.5 * h * s, -0.5 * h, u1),
                    (0.5 * h * s, 0.5 * h, u1),
                    (-0.5 * h * s, 0.5 * h, u0),
                ];
                for (o, y, u) in corners {
                    let v = if y < 0.0 {
                        (ty + 1) as f32 * ts - inset
                    } else {
                        ty as f32 * ts + inset
                    };
                    verts.push(Vertex {
                        pos: [c.x + dx * o, c.y + y + 0.5 * h, c.z + dz * o],
                        uv: [u, v],
                        normal: [0.0, 0.0, 0.0],
                        light: [0.95 * lum.0; 3],
                        sky: 0.95 * lum.1,
                    });
                }
                idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
}

/// Crack overlay: a slightly inflated cube around the block being mined,
/// textured with the crack stage. Rendered alpha-blended.
pub fn emit_crack(
    block: (i32, i32, i32),
    progress: f32,
    verts: &mut Vec<Vertex>,
    idx: &mut Vec<u32>,
) {
    let stage = ((progress * 4.0) as u16).min(3);
    let slot = CRACK_SLOT + stage;
    let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
    let ts = 1.0 / ATLAS_TILES as f32;
    let inset = ts / 32.0;
    let e = 0.006; // inflate to avoid z-fighting
    let origin = Vec3::new(block.0 as f32 - e, block.1 as f32 - e, block.2 as f32 - e);
    let scale = 1.0 + 2.0 * e;

    for face in 0..6 {
        let base = verts.len() as u32;
        for c in CORNERS[face].iter() {
            let (u, v) = match face {
                0 | 1 => (c[2], 1.0 - c[1]),
                4 | 5 => (c[0], 1.0 - c[1]),
                _ => (c[0], c[2]),
            };
            verts.push(Vertex {
                pos: [
                    origin.x + c[0] * scale,
                    origin.y + c[1] * scale,
                    origin.z + c[2] * scale,
                ],
                uv: [
                    tx as f32 * ts + inset + u * (ts - 2.0 * inset),
                    ty as f32 * ts + inset + v * (ts - 2.0 * inset),
                ],
                normal: [0.0, 0.0, 0.0],
                light: [1.0; 3],
                sky: 1.0,
            });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
