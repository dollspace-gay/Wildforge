//! Chunk meshing: visible faces only, per-vertex ambient occlusion,
//! Minecraft-style directional face shading. Produces separate opaque and
//! water (translucent) meshes.

use bytemuck::{Pod, Zeroable};

use crate::atlas::ATLAS_TILES;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, ChunkPos};
use crate::registry::{AIR, BlockId, Registry};
use crate::world::World;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// Geometric face normal (world space). Drives the sun N·L term and the
    /// Minecraft-style face shade, both recomputed in the shader.
    pub normal: [f32; 3],
    /// Block-light color (torches), rgb 0..1, premultiplied by AO only.
    pub light: [f32; 3],
    /// Sky-visibility channel, 0..1, premultiplied by AO. Gates direct sun and
    /// scales the sky-ambient fill; the daylight uniform dims it at night.
    pub sky: f32,
}

pub struct ChunkMesh {
    pub opaque_verts: Vec<Vertex>,
    pub opaque_idx: Vec<u32>,
    pub water_verts: Vec<Vertex>,
    pub water_idx: Vec<u32>,
}

/// face: 0=+X 1=-X 2=+Y 3=-Y 4=+Z 5=-Z
pub(crate) const NORMALS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Corner offsets per face, wound CCW viewed from outside.
pub(crate) const CORNERS: [[[f32; 3]; 4]; 6] = [
    [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]], // +X
    [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]], // -X
    [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]], // +Y
    [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]], // -Y
    [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]], // +Z
    [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]], // -Z
];

/// Minecraft-ish face brightness: top 1.0, bottom 0.5, Z sides 0.8, X sides 0.6.
pub(crate) const FACE_SHADE: [f32; 6] = [0.6, 0.6, 1.0, 0.5, 0.8, 0.8];

fn should_draw(reg: &Registry, b: BlockId, n: BlockId) -> bool {
    if reg.is_water(b) {
        return n == AIR;
    }
    if !reg.is_opaque(b) {
        // Leaf-like blocks: draw faces between different non-opaque blocks.
        return !reg.is_opaque(n) && n != b && !reg.is_water(n);
    }
    !reg.is_opaque(n)
}

pub fn mesh_chunk(world: &World, pos: ChunkPos) -> ChunkMesh {
    let bx = pos.x * CHUNK_X as i32;
    let bz = pos.z * CHUNK_Z as i32;
    let reg = &world.reg;
    let chunk = world.chunks.get(&pos).expect("meshing missing chunk");

    let mut m = ChunkMesh {
        opaque_verts: Vec::new(),
        opaque_idx: Vec::new(),
        water_verts: Vec::new(),
        water_idx: Vec::new(),
    };

    // Neighbor lookup crossing chunk borders (world lookup only on the border).
    let get = |lx: i32, y: i32, lz: i32| -> BlockId {
        if y < 0 || y >= CHUNK_Y as i32 {
            return AIR;
        }
        if lx >= 0 && lx < CHUNK_X as i32 && lz >= 0 && lz < CHUNK_Z as i32 {
            chunk.get(lx as usize, y as usize, lz as usize)
        } else {
            world.get_block(bx + lx, y, bz + lz)
        }
    };
    // Light of the cell a face looks into: block rgb + sky, each 0..1.
    let light = |lx: i32, y: i32, lz: i32| -> ([f32; 3], f32) {
        if y < 0 {
            return ([0.0; 3], 0.0);
        }
        if y >= CHUNK_Y as i32 {
            return ([0.0; 3], 1.0);
        }
        let (b, sk) = if lx >= 0 && lx < CHUNK_X as i32 && lz >= 0 && lz < CHUNK_Z as i32 {
            chunk.light(lx as usize, y as usize, lz as usize)
        } else {
            world.light_rgb_at(bx + lx, y, bz + lz)
        };
        (
            [b[0] as f32 / 15.0, b[1] as f32 / 15.0, b[2] as f32 / 15.0],
            sk as f32 / 15.0,
        )
    };

    let tile_uv = |tx: u32, ty: u32, u: f32, v: f32| -> [f32; 2] {
        let ts = 1.0 / ATLAS_TILES as f32;
        let inset = ts / 32.0; // half-texel to avoid bleeding
        [
            tx as f32 * ts + inset + u * (ts - 2.0 * inset),
            ty as f32 * ts + inset + v * (ts - 2.0 * inset),
        ]
    };

    for lx in 0..CHUNK_X as i32 {
        for lz in 0..CHUNK_Z as i32 {
            for y in 0..CHUNK_Y as i32 {
                let b = get(lx, y, lz);
                if b == AIR {
                    continue;
                }
                let def = reg.block(b);
                if def.cross {
                    // Plant: two crossed quads, both sides, alpha-tested.
                    let (cl, cs) = light(lx, y, lz);
                    let slot = def.tiles[0];
                    let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                    let (wx, wy, wz) = ((bx + lx) as f32, y as f32, (bz + lz) as f32);
                    for (x0, z0, x1, z1) in
                        [(0.15, 0.15, 0.85, 0.85), (0.15, 0.85, 0.85, 0.15)]
                    {
                        for flip in [false, true] {
                            let base = m.opaque_verts.len() as u32;
                            let quad = if flip {
                                [(x1, z1, 1.0), (x0, z0, 0.0), (x0, z0, 0.0), (x1, z1, 1.0)]
                            } else {
                                [(x0, z0, 0.0), (x1, z1, 1.0), (x1, z1, 1.0), (x0, z0, 0.0)]
                            };
                            let ys = [0.0, 0.0, 1.0, 1.0];
                            for i in 0..4 {
                                let (qx, qz, u) = quad[i];
                                m.opaque_verts.push(Vertex {
                                    pos: [wx + qx, wy + ys[i], wz + qz],
                                    uv: tile_uv(tx, ty, u, 1.0 - ys[i]),
                                    // Cross-quads have no single face; treat as
                                    // upward-lit vegetation.
                                    normal: [0.0, 1.0, 0.0],
                                    light: [0.95 * cl[0], 0.95 * cl[1], 0.95 * cl[2]],
                                    sky: 0.95 * cs,
                                });
                            }
                            m.opaque_idx.extend_from_slice(&[
                                base, base + 1, base + 2, base, base + 2, base + 3,
                            ]);
                        }
                    }
                    continue;
                }
                let water = reg.is_water(b);
                // Water surface height falls with flow level (unless the
                // block above is also water — then it's a full column).
                let top_drop = if water && !reg.is_water(get(lx, y + 1, lz)) {
                    1.0 - reg.water_height(b)
                } else {
                    0.0
                };

                for face in 0..6 {
                    let n = NORMALS[face];
                    let nb = get(lx + n[0], y + n[1], lz + n[2]);
                    if !should_draw(reg, b, nb) {
                        continue;
                    }
                    let slot = reg.block(b).tiles[face];
                    let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                    let nrm = [n[0] as f32, n[1] as f32, n[2] as f32];
                    let (fl, fs) = light(lx + n[0], y + n[1], lz + n[2]);

                    let mut ao = [3u8; 4];
                    if !water {
                        ao = corner_ao(reg, &get, [lx, y, lz], face);
                    }

                    let (verts, idx) = if water {
                        (&mut m.water_verts, &mut m.water_idx)
                    } else {
                        (&mut m.opaque_verts, &mut m.opaque_idx)
                    };
                    let base = verts.len() as u32;

                    for (ci, c) in CORNERS[face].iter().enumerate() {
                        let px = (bx + lx) as f32 + c[0];
                        let mut py = y as f32 + c[1];
                        let pz = (bz + lz) as f32 + c[2];
                        if c[1] > 0.5 {
                            py -= top_drop;
                        }
                        let (u, v) = match face {
                            0 | 1 => (c[2], 1.0 - c[1]),
                            4 | 5 => (c[0], 1.0 - c[1]),
                            _ => (c[0], c[2]),
                        };
                        let ao_f = 0.4 + 0.2 * ao[ci] as f32;
                        verts.push(Vertex {
                            pos: [px, py, pz],
                            uv: tile_uv(tx, ty, u, v),
                            normal: nrm,
                            light: [ao_f * fl[0], ao_f * fl[1], ao_f * fl[2]],
                            sky: ao_f * fs,
                        });
                    }
                    // Flip the quad diagonal when AO is anisotropic.
                    if ao[0] as u16 + ao[2] as u16 >= ao[1] as u16 + ao[3] as u16 {
                        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                    } else {
                        idx.extend_from_slice(&[base + 1, base + 2, base + 3, base + 1, base + 3, base]);
                    }
                }
            }
        }
    }
    m
}

/// Vertex AO (0=darkest .. 3=fully open) for the 4 corners of a face.
fn corner_ao(
    reg: &Registry,
    get: &dyn Fn(i32, i32, i32) -> BlockId,
    p: [i32; 3],
    face: usize,
) -> [u8; 4] {
    let n = NORMALS[face];
    // Tangent axes = the two non-normal axes.
    let (t1, t2): ([i32; 3], [i32; 3]) = match face {
        0 | 1 => ([0, 1, 0], [0, 0, 1]),
        2 | 3 => ([1, 0, 0], [0, 0, 1]),
        _ => ([1, 0, 0], [0, 1, 0]),
    };
    let occl = |dx: i32, dy: i32, dz: i32| -> bool {
        reg.is_opaque(get(p[0] + n[0] + dx, p[1] + n[1] + dy, p[2] + n[2] + dz))
    };
    let mut out = [3u8; 4];
    for (ci, c) in CORNERS[face].iter().enumerate() {
        // Sign of this corner along each tangent axis.
        let s1 = if c[0] * t1[0] as f32 + c[1] * t1[1] as f32 + c[2] * t1[2] as f32 > 0.5 { 1 } else { -1 };
        let s2 = if c[0] * t2[0] as f32 + c[1] * t2[1] as f32 + c[2] * t2[2] as f32 > 0.5 { 1 } else { -1 };
        let side1 = occl(t1[0] * s1, t1[1] * s1, t1[2] * s1);
        let side2 = occl(t2[0] * s2, t2[1] * s2, t2[2] * s2);
        let corner = occl(
            t1[0] * s1 + t2[0] * s2,
            t1[1] * s1 + t2[1] * s2,
            t1[2] * s1 + t2[2] * s2,
        );
        out[ci] = if side1 && side2 {
            0
        } else {
            3 - (side1 as u8 + side2 as u8 + corner as u8)
        };
    }
    out
}
