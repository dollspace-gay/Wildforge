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
    /// Light-emitting blocks seen during the walk, for the point-light
    /// director (client presentation; the sim never reads this).
    pub emitters: Vec<crate::lights::Emitter>,
}

/// How hard an emitter's own faces are pushed past the [0,1] range so the
/// HDR/bloom pass makes them glow. The block-light channel already carries a
/// self-lit "torch" term in the shader; for emitter tiles we overwrite it with
/// the block's emission color at this gain (brightest channel ≈ light/15 × gain),
/// comfortably above 1.0 even after point-light suppression and face shading.
const EMISSIVE_GAIN: f32 = 3.5;

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

/// Octant bits a neighbor must have set on the side facing cube face f to fully
/// cover it (used to cull a merged full sand face against a full neighbor).
/// Order matches NORMALS: +X,-X,+Y,-Y,+Z,-Z. Bit o = (oy<<2)|(oz<<1)|ox.
const FACE_COVER_MASK: [u8; 6] = [0x55, 0xAA, 0x0F, 0xF0, 0x33, 0xCC];

fn should_draw(reg: &Registry, b: BlockId, n: BlockId) -> bool {
    // A sub-voxel neighbor only partially fills the shared face, so it must
    // never cull ours — otherwise the block beneath a sand octant (e.g. grass)
    // shows a see-through hole where the octant doesn't cover it.
    if reg.block(n).sub_voxel {
        return true;
    }
    if reg.is_fluid(b) {
        return n == AIR;
    }
    if !reg.is_opaque(b) {
        // Leaf-like blocks: draw faces between different non-opaque blocks.
        return !reg.is_opaque(n) && n != b && !reg.is_fluid(n);
    }
    !reg.is_opaque(n)
}

pub fn mesh_chunk(world: &World, pos: ChunkPos) -> ChunkMesh {
    let bx = pos.x * CHUNK_X as i32;
    let bz = pos.z * CHUNK_Z as i32;
    let reg = &world.reg;
    let chunk = world.chunk(pos).expect("meshing missing chunk");

    let mut m = ChunkMesh {
        opaque_verts: Vec::new(),
        opaque_idx: Vec::new(),
        water_verts: Vec::new(),
        water_idx: Vec::new(),
        emitters: Vec::new(),
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
    // Octant mask of a neighbor cell, for sub-voxel face culling across borders.
    let get_meta = |lx: i32, y: i32, lz: i32| -> u8 {
        if y < 0 || y >= CHUNK_Y as i32 {
            return 0;
        }
        if lx >= 0 && lx < CHUNK_X as i32 && lz >= 0 && lz < CHUNK_Z as i32 {
            chunk.meta(lx as usize, y as usize, lz as usize)
        } else {
            world.get_meta(bx + lx, y, bz + lz)
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
    // Light for a sub-voxel face looking into cell (lx,y,lz). A sub-voxel block
    // is opaque, so its cell stores no light even where it is hollow; climb up
    // out of the sand into the open air above (these pockets are top-lit) so
    // carved interiors and risers beside partial neighbors aren't left black.
    let face_light = |lx: i32, mut y: i32, lz: i32| -> ([f32; 3], f32) {
        for _ in 0..4 {
            let b = get(lx, y, lz);
            if b != AIR && reg.block(b).sub_voxel {
                y += 1;
            } else {
                break;
            }
        }
        light(lx, y, lz)
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
                // An emitter's own faces glow: overwrite their light with the
                // emission color at an overbright gain, so the HDR bloom catches
                // them. `None` for ordinary blocks leaves lighting untouched.
                let emissive: Option<[f32; 3]> = if def.light_emit > 0 {
                    m.emitters.push(crate::lights::Emitter {
                        pos: (bx + lx, y, bz + lz),
                        rgb: def.light_rgb,
                        emit: def.light_emit,
                    });
                    Some([
                        def.light_rgb[0] as f32 / 15.0 * EMISSIVE_GAIN,
                        def.light_rgb[1] as f32 / 15.0 * EMISSIVE_GAIN,
                        def.light_rgb[2] as f32 / 15.0 * EMISSIVE_GAIN,
                    ])
                } else {
                    None
                };
                if def.cross {
                    // Plant: two crossed quads, both sides, alpha-tested.
                    let (cl, cs) = light(lx, y, lz);
                    let slot = def.tiles[0];
                    let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                    let (wx, wy, wz) = ((bx + lx) as f32, y as f32, (bz + lz) as f32);
                    for (x0, z0, x1, z1) in [(0.15, 0.15, 0.85, 0.85), (0.15, 0.85, 0.85, 0.15)] {
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
                                    light: emissive.unwrap_or([
                                        0.95 * cl[0],
                                        0.95 * cl[1],
                                        0.95 * cl[2],
                                    ]),
                                    sky: 0.95 * cs,
                                });
                            }
                            m.opaque_idx.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        }
                    }
                    continue;
                }
                if def.sub_voxel {
                    // Octant occupancy: emit up to 8 half-cubes. A face is
                    // skipped when the adjacent half-cell is filled — a sibling
                    // octant in this block, or the facing octant of a sub-voxel
                    // neighbor — or when an opaque neighbor cube covers it.
                    let mask = chunk.meta(lx as usize, y as usize, lz as usize);
                    // Fast path: a fully-filled cell is just a cube — 6 merged
                    // faces instead of 24 octant quarters (the common case for
                    // placed/interior sand; keeps meshes ~4x lighter).
                    if mask == 0xFF {
                        for face in 0..6 {
                            let nrm = NORMALS[face];
                            let nb = get(lx + nrm[0], y + nrm[1], lz + nrm[2]);
                            let covered = if nb == AIR {
                                false
                            } else if reg.block(nb).sub_voxel {
                                let nm = get_meta(lx + nrm[0], y + nrm[1], lz + nrm[2]);
                                nm & FACE_COVER_MASK[face] == FACE_COVER_MASK[face]
                            } else {
                                reg.is_opaque(nb)
                            };
                            if covered {
                                continue;
                            }
                            let (fl, fs) = face_light(lx + nrm[0], y + nrm[1], lz + nrm[2]);
                            let slot = def.tiles[face];
                            let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                            let nf = [nrm[0] as f32, nrm[1] as f32, nrm[2] as f32];
                            let base = m.opaque_verts.len() as u32;
                            for c in CORNERS[face].iter() {
                                let (u, v) = match face {
                                    0 | 1 => (c[2], 1.0 - c[1]),
                                    4 | 5 => (c[0], 1.0 - c[1]),
                                    _ => (c[0], c[2]),
                                };
                                m.opaque_verts.push(Vertex {
                                    pos: [
                                        (bx + lx) as f32 + c[0],
                                        y as f32 + c[1],
                                        (bz + lz) as f32 + c[2],
                                    ],
                                    uv: tile_uv(tx, ty, u, v),
                                    normal: nf,
                                    light: emissive.unwrap_or(fl),
                                    sky: fs,
                                });
                            }
                            m.opaque_idx.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        }
                        continue;
                    }
                    for o in 0..8u32 {
                        if mask & (1 << o) == 0 {
                            continue;
                        }
                        let ox = (o & 1) as i32;
                        let oz = ((o >> 1) & 1) as i32;
                        let oy = ((o >> 2) & 1) as i32;
                        for face in 0..6 {
                            let nrm = NORMALS[face];
                            let (nox, noy, noz) = (ox + nrm[0], oy + nrm[1], oz + nrm[2]);
                            let crossing = !(0..=1).contains(&nox)
                                || !(0..=1).contains(&noy)
                                || !(0..=1).contains(&noz);
                            let (fl, fs) = if crossing {
                                // Boundary face: cull against the neighbor block.
                                let nb = get(lx + nrm[0], y + nrm[1], lz + nrm[2]);
                                if nb != AIR {
                                    if reg.block(nb).sub_voxel {
                                        let nmask = get_meta(lx + nrm[0], y + nrm[1], lz + nrm[2]);
                                        let fo = ((noy.rem_euclid(2) as u32) << 2)
                                            | ((noz.rem_euclid(2) as u32) << 1)
                                            | (nox.rem_euclid(2) as u32);
                                        if nmask & (1 << fo) != 0 {
                                            continue;
                                        }
                                    } else if reg.is_opaque(nb) {
                                        continue;
                                    }
                                }
                                face_light(lx + nrm[0], y + nrm[1], lz + nrm[2])
                            } else {
                                // Internal face: hidden if the sibling is filled.
                                let so = ((noy as u32) << 2) | ((noz as u32) << 1) | (nox as u32);
                                if mask & (1 << so) != 0 {
                                    continue;
                                }
                                // Exposed interior pocket: lit from the open air
                                // above, not this solid cell's own (dark) light.
                                face_light(lx, y, lz)
                            };
                            let slot = def.tiles[face];
                            let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                            let nf = [nrm[0] as f32, nrm[1] as f32, nrm[2] as f32];
                            let base = m.opaque_verts.len() as u32;
                            for c in CORNERS[face].iter() {
                                let ib = [
                                    c[0] * 0.5 + ox as f32 * 0.5,
                                    c[1] * 0.5 + oy as f32 * 0.5,
                                    c[2] * 0.5 + oz as f32 * 0.5,
                                ];
                                let (u, v) = match face {
                                    0 | 1 => (ib[2], 1.0 - ib[1]),
                                    4 | 5 => (ib[0], 1.0 - ib[1]),
                                    _ => (ib[0], ib[2]),
                                };
                                m.opaque_verts.push(Vertex {
                                    pos: [
                                        (bx + lx) as f32 + ib[0],
                                        y as f32 + ib[1],
                                        (bz + lz) as f32 + ib[2],
                                    ],
                                    uv: tile_uv(tx, ty, u, v),
                                    normal: nf,
                                    light: emissive.unwrap_or(fl),
                                    sky: fs,
                                });
                            }
                            m.opaque_idx.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        }
                    }
                    continue;
                }
                let water = reg.is_fluid(b);
                // Glass rides the blended pipeline: translucent tint,
                // and panes catch the water shader's sun glint.
                let blended = water || reg.block(b).glass;
                // Water surface height falls with flow level (unless the
                // block above is also water — then it's a full column).
                let top_drop = if water && !reg.is_fluid(get(lx, y + 1, lz)) {
                    1.0 - reg.water_height(b)
                } else if let Some(h) = reg.block(b).height {
                    // Thin slabs (snow layers): the top face and every
                    // side's upper corners drop to the declared height.
                    1.0 - h
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
                    // `face_light` (not `light`) so a face looking into an opaque
                    // sub-voxel cell (e.g. grass under a sand octant) is lit by the
                    // open air above it, not that cell's dark stored light.
                    let (fl, fs) = face_light(lx + n[0], y + n[1], lz + n[2]);

                    let mut ao = [3u8; 4];
                    if !water {
                        ao = corner_ao(reg, &get, [lx, y, lz], face);
                    }

                    let (verts, idx) = if blended {
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
                            // Emitter faces glow at full strength (no AO dimming);
                            // ordinary faces keep their occluded block light.
                            light: emissive.unwrap_or([ao_f * fl[0], ao_f * fl[1], ao_f * fl[2]]),
                            sky: ao_f * fs,
                        });
                    }
                    // Flip the quad diagonal when AO is anisotropic.
                    if ao[0] as u16 + ao[2] as u16 >= ao[1] as u16 + ao[3] as u16 {
                        idx.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    } else {
                        idx.extend_from_slice(&[
                            base + 1,
                            base + 2,
                            base + 3,
                            base + 1,
                            base + 3,
                            base,
                        ]);
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
        // A sub-voxel neighbor only partially covers the cell, so it must not
        // ambient-occlude a full face (else an octant of sand darkens the whole
        // grass block below it).
        let bb = get(p[0] + n[0] + dx, p[1] + n[1] + dy, p[2] + n[2] + dz);
        reg.is_opaque(bb) && !reg.block(bb).sub_voxel
    };
    let mut out = [3u8; 4];
    for (ci, c) in CORNERS[face].iter().enumerate() {
        // Sign of this corner along each tangent axis.
        let s1 = if c[0] * t1[0] as f32 + c[1] * t1[1] as f32 + c[2] * t1[2] as f32 > 0.5 {
            1
        } else {
            -1
        };
        let s2 = if c[0] * t2[0] as f32 + c[1] * t2[1] as f32 + c[2] * t2[2] as f32 > 0.5 {
            1
        } else {
            -1
        };
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
