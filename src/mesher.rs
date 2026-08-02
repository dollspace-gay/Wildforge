//! Chunk meshing: visible faces only, per-vertex ambient occlusion,
//! Minecraft-style directional face shading. Produces separate opaque and
//! water (translucent) meshes.

use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

use crate::atlas::ATLAS_TILES;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, ChunkMeshSnapshot, ChunkPos};
use crate::planet::{
    BlockPos, SurfacePos, block_to_render, canonicalize_surface_point, local_frame,
};
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

/// Immutable neighborhood needed to mesh one chunk off the render thread.
/// The center copies only render-relevant planes, while the eight neighboring
/// chunks contribute a one-cell halo. Copying whole neighbors made scheduling
/// a supposedly asynchronous mesh cost several milliseconds on the render
/// thread. A later edit marks the live chunk dirty again while this snapshot
/// finishes, causing a fresh job without blocking the frame.
pub struct ChunkMeshInput {
    pos: ChunkPos,
    reg: Arc<Registry>,
    center: ChunkMeshSnapshot,
    border: Box<[MeshBorderCell]>,
}

#[derive(Clone, Copy, Default)]
struct MeshBorderCell {
    block: u16,
    block_light: [u8; 3],
    sky_light: u8,
}

impl ChunkMeshInput {
    pub fn capture(world: &World, pos: ChunkPos) -> Option<Self> {
        let center = world.chunk(pos)?.mesh_snapshot();
        let mut border = vec![MeshBorderCell::default(); 68 * CHUNK_Y].into_boxed_slice();
        let origin = pos.block_origin();
        let mut capture_column = |lx: i32, lz: i32| {
            let Some(column) = Self::border_column(lx, lz) else {
                return;
            };
            let Ok(surface) = SurfacePos::canonicalized(
                pos.face(),
                i32::from(origin.u()) + lx,
                i32::from(origin.v()) + lz,
            ) else {
                return;
            };
            let Some(chunk) = world.chunk(ChunkPos::from_surface(surface)) else {
                return;
            };
            let x = usize::from(surface.u()) % CHUNK_X;
            let z = usize::from(surface.v()) % CHUNK_Z;
            for y in 0..CHUNK_Y {
                let (block_light, sky_light) = chunk.light(x, y, z);
                border[column * CHUNK_Y + y] = MeshBorderCell {
                    block: chunk.get(x, y, z).0,
                    block_light,
                    sky_light,
                };
            }
        };
        for lx in -1..=CHUNK_X as i32 {
            capture_column(lx, -1);
            capture_column(lx, CHUNK_Z as i32);
        }
        for lz in 0..CHUNK_Z as i32 {
            capture_column(-1, lz);
            capture_column(CHUNK_X as i32, lz);
        }
        Some(Self {
            pos,
            reg: Arc::clone(&world.reg),
            center,
            border,
        })
    }

    pub fn position(&self) -> ChunkPos {
        self.pos
    }

    fn border_column(lx: i32, lz: i32) -> Option<usize> {
        match (lx, lz) {
            (-1..=16, -1) => Some((lx + 1) as usize),
            (-1..=16, 16) => Some(18 + (lx + 1) as usize),
            (-1, 0..=15) => Some(36 + lz as usize),
            (16, 0..=15) => Some(52 + lz as usize),
            _ => None,
        }
    }

    fn block_at(&self, lx: i32, y: usize, lz: i32) -> BlockId {
        Self::border_column(lx, lz).map_or(AIR, |column| {
            BlockId(self.border[column * CHUNK_Y + y].block)
        })
    }

    fn light_at(&self, lx: i32, y: usize, lz: i32) -> ([u8; 3], u8) {
        Self::border_column(lx, lz).map_or(([0; 3], 0), |column| {
            let cell = self.border[column * CHUNK_Y + y];
            (cell.block_light, cell.sky_light)
        })
    }
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

/// Corner offsets per face, wound CCW viewed from outside in chart-local
/// coordinates. A planet chart is intentionally `(east, radial-up, north)`, a
/// left-handed frame, so triangle indices are emitted in reverse order after
/// `curved` maps these corners into render space.
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
fn should_draw(reg: &Registry, b: BlockId, n: BlockId) -> bool {
    if reg.is_fluid(b) {
        return n == AIR;
    }
    if !reg.is_opaque(b) {
        // Leaf-like blocks: draw faces between different non-opaque blocks.
        return !reg.is_opaque(n) && n != b && !reg.is_fluid(n);
    }
    !reg.is_opaque(n)
}

/// `variants` supplies the active pack's alternate tiles. Variant choice is
/// baked into the uvs here rather than resolved in the shader, so switching
/// packs has to remesh — see `apply_pack`.
pub fn mesh_chunk(
    world: &World,
    pos: ChunkPos,
    variants: &crate::atlas::TileVariants,
) -> ChunkMesh {
    let input = ChunkMeshInput::capture(world, pos).expect("meshing missing chunk");
    mesh_chunk_input(&input, variants)
}

pub fn mesh_chunk_input(
    input: &ChunkMeshInput,
    variants: &crate::atlas::TileVariants,
) -> ChunkMesh {
    let pos = input.position();
    let bx = i32::from(pos.u()) * CHUNK_X as i32;
    let bz = i32::from(pos.v()) * CHUNK_Z as i32;
    let reg = &input.reg;
    let chunk = &input.center;

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
            input.block_at(lx, y as usize, lz)
        }
    };
    // Octant mask of a neighbor cell, for sub-voxel face culling across borders.
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
            input.light_at(lx, y as usize, lz)
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
    let face_light = |lx: i32, y: i32, lz: i32| -> ([f32; 3], f32) { light(lx, y, lz) };

    let curved = |u: f32, y: f32, v: f32, normal: [f32; 3]| {
        let canonical = canonicalize_surface_point(pos.face(), f64::from(u), f64::from(v))
            .expect("mesh vertices cross at most one face edge");
        let local_normal = canonical.rotation.rotate_vec3(glam::Vec3::from(normal));
        let frame = local_frame(canonical.point);
        let world_normal = (frame.east * f64::from(local_normal.x)
            + frame.up * f64::from(local_normal.y)
            + frame.north * f64::from(local_normal.z))
        .normalize();
        let world_pos = block_to_render(canonical.point, f64::from(y));
        (
            [world_pos.x as f32, world_pos.y as f32, world_pos.z as f32],
            [
                world_normal.x as f32,
                world_normal.y as f32,
                world_normal.z as f32,
            ],
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
                // An emitter's own faces glow: overwrite their light with the
                // emission color at an overbright gain, so the HDR bloom catches
                // them. `None` for ordinary blocks leaves lighting untouched.
                let emissive: Option<[f32; 3]> = if def.light_emit > 0 {
                    m.emitters.push(crate::lights::Emitter {
                        pos: BlockPos::new(pos.face(), (bx + lx) as u16, y as u8, (bz + lz) as u16)
                            .expect("meshed block is inside its chunk face"),
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
                                let (curved_pos, curved_normal) =
                                    curved(wx + qx, wy + ys[i], wz + qz, [0.0, 1.0, 0.0]);
                                m.opaque_verts.push(Vertex {
                                    pos: curved_pos,
                                    uv: tile_uv(tx, ty, u, 1.0 - ys[i]),
                                    // Cross-quads have no single face; treat as
                                    // upward-lit vegetation.
                                    normal: curved_normal,
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
                                base + 2,
                                base + 1,
                                base,
                                base + 3,
                                base + 2,
                            ]);
                        }
                    }
                    continue;
                }
                if let Some(shape) = def.shape.as_deref() {
                    // Custom silhouettes: boxes emitted by hand, lit
                    // from the cell (these blocks are non-opaque).
                    let (cl, cs) = light(lx, y, lz);
                    let slot = def.tiles[0];
                    let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
                    let (wx, wy, wz) = ((bx + lx) as f32, y as f32, (bz + lz) as f32);
                    let lit = emissive.unwrap_or([0.85 * cl[0], 0.85 * cl[1], 0.85 * cl[2]]);
                    let sky_l = 0.85 * cs;
                    // One textured box: min/max corners in block space.
                    let mut boxed = |x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32| {
                        for face in 0..6 {
                            let nrm = NORMALS[face];
                            let nf = [nrm[0] as f32, nrm[1] as f32, nrm[2] as f32];
                            let base = m.opaque_verts.len() as u32;
                            for c in CORNERS[face].iter() {
                                let px = wx + x0 + c[0] * (x1 - x0);
                                let py = wy + y0 + c[1] * (y1 - y0);
                                let pz = wz + z0 + c[2] * (z1 - z0);
                                let (u, v) = match face {
                                    0 | 1 => (z0 + c[2] * (z1 - z0), 1.0 - (y0 + c[1] * (y1 - y0))),
                                    4 | 5 => (x0 + c[0] * (x1 - x0), 1.0 - (y0 + c[1] * (y1 - y0))),
                                    _ => (x0 + c[0] * (x1 - x0), z0 + c[2] * (z1 - z0)),
                                };
                                let (curved_pos, curved_normal) = curved(px, py, pz, nf);
                                m.opaque_verts.push(Vertex {
                                    pos: curved_pos,
                                    uv: tile_uv(tx, ty, u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)),
                                    normal: curved_normal,
                                    light: lit,
                                    sky: sky_l,
                                });
                            }
                            m.opaque_idx.extend_from_slice(&[
                                base,
                                base + 2,
                                base + 1,
                                base,
                                base + 3,
                                base + 2,
                            ]);
                        }
                    };
                    match shape {
                        "obelisk" => {
                            // A standing stone half again a block
                            // tall: plinth, tapering shaft, and a
                            // pyramidion reaching into the air above
                            // (visual only; collision stays the cell).
                            boxed(0.20, 0.0, 0.20, 0.80, 0.12, 0.80);
                            boxed(0.32, 0.12, 0.32, 0.68, 1.05, 0.68);
                            boxed(0.36, 1.05, 0.36, 0.64, 1.45, 0.64);
                            boxed(0.42, 1.45, 0.42, 0.58, 1.62, 0.58);
                        }
                        "rack" => {
                            // Four legs under a slatted tray: smoke
                            // needs somewhere to rise through.
                            for (lx0, lz0) in
                                [(0.12, 0.12), (0.76, 0.12), (0.12, 0.76), (0.76, 0.76)]
                            {
                                boxed(lx0, 0.0, lz0, lx0 + 0.12, 0.72, lz0 + 0.12);
                            }
                            boxed(0.04, 0.72, 0.04, 0.96, 0.86, 0.32);
                            boxed(0.04, 0.72, 0.40, 0.96, 0.86, 0.60);
                            boxed(0.04, 0.72, 0.68, 0.96, 0.86, 0.96);
                        }
                        "signboard" => {
                            // A post with a broad reading board.
                            boxed(0.44, 0.0, 0.44, 0.56, 0.5, 0.56);
                            boxed(0.06, 0.5, 0.42, 0.94, 0.96, 0.58);
                        }
                        "axle" | "wheel" | "sails" | "gearbox" | "millstone" | "sawbench"
                        | "helve" | "helve_up" | "lathe" | "lathe_iron" | "vice" | "boring"
                        | "pump" | "boiler" | "engine" | "generator" => {
                            // Millwork reads its orientation from its
                            // neighbors: shafts run toward machines,
                            // wheels set their plane from their axle,
                            // hammers face their anvil.
                            let powerish = |b: crate::registry::BlockId| {
                                matches!(
                                    reg.block(b).shape.as_deref(),
                                    Some(
                                        "axle"
                                            | "gearbox"
                                            | "wheel"
                                            | "sails"
                                            | "millstone"
                                            | "sawbench"
                                            | "helve"
                                            | "helve_up"
                                            | "lathe"
                                            | "lathe_iron"
                                            | "boring"
                                            | "engine"
                                            | "generator"
                                    )
                                )
                            };
                            let px = powerish(get(lx + 1, y, lz)) || powerish(get(lx - 1, y, lz));
                            let pz = powerish(get(lx, y, lz + 1)) || powerish(get(lx, y, lz - 1));
                            let py = powerish(get(lx, y + 1, lz)) || powerish(get(lx, y - 1, lz));
                            match shape {
                                "axle" => {
                                    // Journals at the ends, the shaft
                                    // between; vertical when the run
                                    // climbs, along its line otherwise.
                                    if py && !px && !pz {
                                        boxed(0.38, 0.0, 0.38, 0.62, 1.0, 0.62);
                                        boxed(0.30, 0.0, 0.30, 0.70, 0.10, 0.70);
                                        boxed(0.30, 0.90, 0.30, 0.70, 1.0, 0.70);
                                    } else if pz && !px {
                                        boxed(0.38, 0.38, 0.0, 0.62, 0.62, 1.0);
                                        boxed(0.30, 0.30, 0.0, 0.70, 0.70, 0.10);
                                        boxed(0.30, 0.30, 0.90, 0.70, 0.70, 1.0);
                                    } else {
                                        boxed(0.0, 0.38, 0.38, 1.0, 0.62, 0.62);
                                        boxed(0.0, 0.30, 0.30, 0.10, 0.70, 0.70);
                                        boxed(0.90, 0.30, 0.30, 1.0, 0.70, 0.70);
                                    }
                                }
                                "gearbox" => {
                                    // A framed crown gear: rim ring,
                                    // four teeth, and the meshing pin.
                                    boxed(0.30, 0.30, 0.30, 0.70, 0.70, 0.70);
                                    boxed(0.18, 0.82, 0.38, 0.82, 0.96, 0.62);
                                    boxed(0.18, 0.04, 0.38, 0.82, 0.18, 0.62);
                                    boxed(0.04, 0.18, 0.38, 0.18, 0.82, 0.62);
                                    boxed(0.82, 0.18, 0.38, 0.96, 0.82, 0.62);
                                    boxed(0.70, 0.70, 0.40, 0.88, 0.88, 0.60);
                                    boxed(0.12, 0.70, 0.40, 0.30, 0.88, 0.60);
                                    boxed(0.70, 0.12, 0.40, 0.88, 0.30, 0.60);
                                    boxed(0.12, 0.12, 0.40, 0.30, 0.30, 0.60);
                                }
                                "wheel" | "sails" => {
                                    // A wheel's plane CONTAINS its
                                    // stream: read the race first
                                    // (water beside or under-beside),
                                    // the axle shaft as the fallback.
                                    let water_on = |dx: i32, dz: i32| {
                                        reg.water_volume(get(lx + dx, y, lz + dz)).is_some()
                                            || reg
                                                .water_volume(get(lx + dx, y - 1, lz + dz))
                                                .is_some()
                                    };
                                    let wx = water_on(1, 0) || water_on(-1, 0);
                                    let wz = water_on(0, 1) || water_on(0, -1);
                                    let flat = if shape == "wheel" && (wx ^ wz) {
                                        wz
                                    } else {
                                        px && !pz
                                    };
                                    // Box in wheel-plane coords (u =
                                    // across, v = up, w = thickness).
                                    let mut disc =
                                        |u0: f32, v0: f32, w0: f32, u1: f32, v1: f32, w1: f32| {
                                            if flat {
                                                boxed(w0, v0, u0, w1, v1, u1);
                                            } else {
                                                boxed(u0, v0, w0, u1, v1, w1);
                                            }
                                        };
                                    if shape == "wheel" {
                                        // A real mill wheel: hub,
                                        // crossed spokes, a broad
                                        // octagon rim, paddle boards
                                        // — two and a half blocks of
                                        // it (render-only overflow,
                                        // the obelisk precedent).
                                        disc(0.36, 0.36, 0.32, 0.64, 0.64, 0.68);
                                        disc(-0.45, 0.42, 0.42, 1.45, 0.58, 0.58);
                                        disc(0.42, -0.45, 0.42, 0.58, 1.45, 0.58);
                                        disc(0.02, 1.30, 0.38, 0.98, 1.50, 0.62);
                                        disc(0.02, -0.50, 0.38, 0.98, -0.30, 0.62);
                                        disc(-0.50, 0.02, 0.38, -0.30, 0.98, 0.62);
                                        disc(1.30, 0.02, 0.38, 1.50, 0.98, 0.62);
                                        disc(0.94, 0.94, 0.38, 1.36, 1.36, 0.62);
                                        disc(-0.36, 0.94, 0.38, 0.06, 1.36, 0.62);
                                        disc(0.94, -0.36, 0.38, 1.36, 0.06, 0.62);
                                        disc(-0.36, -0.36, 0.38, 0.06, 0.06, 0.62);
                                        disc(0.26, 1.50, 0.34, 0.74, 1.72, 0.66);
                                        disc(0.26, -0.72, 0.34, 0.74, -0.50, 0.66);
                                        disc(-0.72, 0.26, 0.34, -0.50, 0.74, 0.66);
                                        disc(1.50, 0.26, 0.34, 1.72, 0.74, 0.66);
                                    } else {
                                        // Hub, crossed arms, and four
                                        // pinwheeled cloth panels.
                                        disc(0.40, 0.40, 0.42, 0.60, 0.60, 0.58);
                                        disc(-0.45, 0.46, 0.46, 1.45, 0.54, 0.54);
                                        disc(0.46, -0.45, 0.46, 0.54, 1.45, 0.54);
                                        disc(0.60, 0.54, 0.47, 1.40, 0.86, 0.53);
                                        disc(-0.40, 0.14, 0.47, 0.40, 0.46, 0.53);
                                        disc(0.14, 0.60, 0.47, 0.46, 1.40, 0.53);
                                        disc(0.54, -0.40, 0.47, 0.86, 0.40, 0.53);
                                    }
                                }
                                "millstone" => {
                                    // Bedstone, runner, feed hopper,
                                    // and the drive stub on top.
                                    boxed(0.06, 0.0, 0.06, 0.94, 0.30, 0.94);
                                    boxed(0.14, 0.30, 0.14, 0.86, 0.52, 0.86);
                                    boxed(0.34, 0.52, 0.34, 0.66, 0.66, 0.66);
                                    boxed(0.26, 0.62, 0.26, 0.74, 0.86, 0.74);
                                    boxed(0.44, 0.80, 0.44, 0.56, 1.0, 0.56);
                                }
                                "sawbench" => {
                                    // A stout bench, the blade proud
                                    // of the table, a fence to guide
                                    // the cut.
                                    for (lx0, lz0) in
                                        [(0.06, 0.06), (0.82, 0.06), (0.06, 0.82), (0.82, 0.82)]
                                    {
                                        boxed(lx0, 0.0, lz0, lx0 + 0.12, 0.55, lz0 + 0.12);
                                    }
                                    boxed(0.0, 0.55, 0.0, 1.0, 0.68, 1.0);
                                    boxed(0.34, 0.62, 0.47, 0.66, 0.98, 0.53);
                                    boxed(0.0, 0.68, 0.12, 1.0, 0.76, 0.24);
                                }
                                "lathe" | "lathe_iron" => {
                                    // A long bed with ways: pedestal
                                    // legs, rails, headstock and
                                    // tailstock, work between
                                    // centers, a tool rest — and on
                                    // the iron lathe, the leadscrew
                                    // rail that made it possible.
                                    let along_z = pz && !px;
                                    let mut bed =
                                        |a0: f32, y0: f32, c0: f32, a1: f32, y1: f32, c1: f32| {
                                            if along_z {
                                                boxed(c0, y0, a0, c1, y1, a1);
                                            } else {
                                                boxed(a0, y0, c0, a1, y1, c1);
                                            }
                                        };
                                    bed(0.06, 0.0, 0.30, 0.22, 0.35, 0.70);
                                    bed(0.78, 0.0, 0.30, 0.94, 0.35, 0.70);
                                    bed(0.0, 0.35, 0.34, 1.0, 0.50, 0.66);
                                    let iron = shape == "lathe_iron";
                                    let head_top = if iron { 1.0 } else { 0.90 };
                                    bed(0.0, 0.50, 0.26, 0.28, head_top, 0.74);
                                    bed(0.78, 0.50, 0.32, 0.96, 0.78, 0.68);
                                    bed(0.28, 0.62, 0.44, 0.78, 0.72, 0.56);
                                    bed(0.40, 0.50, 0.20, 0.68, 0.60, 0.34);
                                    if iron {
                                        bed(0.0, 0.42, 0.72, 1.0, 0.50, 0.80);
                                    }
                                }
                                "generator" => {
                                    // Bedplate, a drum body between
                                    // end caps, terminal posts on
                                    // top, the drive stub reaching
                                    // for its shaft.
                                    boxed(0.04, 0.0, 0.10, 0.96, 0.16, 0.90);
                                    boxed(0.14, 0.16, 0.22, 0.86, 0.72, 0.78);
                                    boxed(0.06, 0.16, 0.30, 0.14, 0.66, 0.70);
                                    boxed(0.86, 0.16, 0.30, 0.94, 0.66, 0.70);
                                    boxed(0.30, 0.72, 0.40, 0.40, 0.92, 0.52);
                                    boxed(0.58, 0.72, 0.40, 0.68, 0.92, 0.52);
                                    boxed(-0.10, 0.34, 0.42, 0.06, 0.52, 0.58);
                                }
                                "boiler" => {
                                    // A riveted drum with chamfered
                                    // shoulders and a steam dome.
                                    boxed(0.0, 0.18, 0.14, 1.0, 0.82, 0.86);
                                    boxed(0.0, 0.08, 0.26, 1.0, 0.18, 0.74);
                                    boxed(0.0, 0.82, 0.26, 1.0, 0.94, 0.74);
                                    boxed(0.35, 0.94, 0.32, 0.65, 1.10, 0.68);
                                }
                                "engine" => {
                                    // Bedplate, A-post, the rocking
                                    // beam overhead, cylinder at one
                                    // end and flywheel at the other.
                                    boxed(0.0, 0.0, 0.0, 1.0, 0.18, 1.0);
                                    boxed(0.40, 0.18, 0.30, 0.60, 1.10, 0.70);
                                    boxed(-0.15, 1.10, 0.40, 1.15, 1.28, 0.60);
                                    boxed(0.02, 0.18, 0.36, 0.32, 0.62, 0.64);
                                    boxed(0.10, 0.62, 0.46, 0.24, 1.10, 0.54);
                                    boxed(0.96, 0.25, 0.25, 1.14, 0.95, 0.75);
                                    boxed(1.00, 0.45, 0.45, 1.10, 0.75, 0.55);
                                }
                                "boring" => {
                                    // A heavy frame over a clamped
                                    // blank: base, posts, crossbeam,
                                    // the spindle plunging down.
                                    boxed(0.04, 0.0, 0.04, 0.96, 0.20, 0.96);
                                    boxed(0.10, 0.20, 0.30, 0.30, 1.05, 0.70);
                                    boxed(0.70, 0.20, 0.30, 0.90, 1.05, 0.70);
                                    boxed(0.0, 1.05, 0.34, 1.0, 1.25, 0.66);
                                    boxed(0.42, 0.45, 0.42, 0.58, 1.05, 0.58);
                                    boxed(0.30, 0.20, 0.30, 0.70, 0.45, 0.70);
                                }
                                "pump" => {
                                    // Standpipe, pump head, spout.
                                    boxed(0.36, 0.0, 0.36, 0.64, 0.85, 0.64);
                                    boxed(0.26, 0.85, 0.26, 0.74, 1.10, 0.74);
                                    boxed(0.64, 0.55, 0.42, 1.06, 0.72, 0.58);
                                }
                                "vice" => {
                                    // Pedestal, bench cap, two jaws,
                                    // the screw spindle and handle.
                                    boxed(0.38, 0.0, 0.38, 0.62, 0.45, 0.62);
                                    boxed(0.28, 0.45, 0.28, 0.72, 0.60, 0.72);
                                    boxed(0.30, 0.60, 0.40, 0.48, 0.85, 0.60);
                                    boxed(0.54, 0.60, 0.40, 0.72, 0.85, 0.60);
                                    boxed(0.24, 0.66, 0.46, 0.80, 0.74, 0.54);
                                    boxed(0.76, 0.52, 0.47, 0.82, 0.90, 0.53);
                                }
                                "helve" | "helve_up" => {
                                    // Pivot post, beam arm, hammer
                                    // head — facing its anvil, raised
                                    // while the shaft has it working.
                                    let toward = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                                        .into_iter()
                                        .find(|&(dx, dz)| {
                                            reg.block(get(lx + dx, y, lz + dz))
                                                .interaction
                                                .as_deref()
                                                == Some("anvil")
                                        })
                                        .unwrap_or((1, 0));
                                    let mut part =
                                        |a0: f32, y0: f32, c0: f32, a1: f32, y1: f32, c1: f32| {
                                            let (m0, m1) = match toward {
                                                (1, 0) => ((a0, c0), (a1, c1)),
                                                (-1, 0) => ((1.0 - a1, c0), (1.0 - a0, c1)),
                                                (0, 1) => ((c0, a0), (c1, a1)),
                                                _ => ((c0, 1.0 - a1), (c1, 1.0 - a0)),
                                            };
                                            boxed(m0.0, y0, m0.1, m1.0, y1, m1.1);
                                        };
                                    let up = shape == "helve_up";
                                    part(0.04, 0.0, 0.34, 0.24, 0.78, 0.66);
                                    part(0.00, 0.78, 0.30, 0.28, 0.92, 0.70);
                                    if up {
                                        part(0.10, 0.62, 0.42, 0.96, 0.78, 0.58);
                                        part(0.74, 0.34, 0.34, 0.98, 0.72, 0.66);
                                    } else {
                                        part(0.10, 0.42, 0.42, 0.96, 0.58, 0.58);
                                        part(0.74, 0.10, 0.34, 0.98, 0.48, 0.66);
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                let water = reg.is_fluid(b);
                // Glass rides the blended pipeline: translucent tint,
                // and panes catch the water shader's sun glint.
                let blended = water || reg.block(b).glass;
                // A fluid surface takes each top corner from the tallest
                // fluid among the four cells sharing it (full height if
                // any of them continues upward), so neighboring cells at
                // different volumes meet exactly instead of rendering as
                // disconnected tiles with gaps between their rims.
                let same = |n: BlockId| reg.is_fluid(n) && reg.is_lava(n) == reg.is_lava(b);
                let surf: Option<[f32; 4]> = if water && !same(get(lx, y + 1, lz)) {
                    let mut hs = [0f32; 4];
                    for cx in 0..2i32 {
                        for cz in 0..2i32 {
                            let mut h = reg.water_height(b);
                            for (dx, dz) in [(cx - 1, cz - 1), (cx - 1, cz), (cx, cz - 1), (cx, cz)]
                            {
                                let nb = get(lx + dx, y, lz + dz);
                                if !same(nb) {
                                    continue;
                                }
                                if same(get(lx + dx, y + 1, lz + dz)) {
                                    h = 1.0;
                                    break;
                                }
                                h = h.max(reg.water_height(nb));
                            }
                            hs[(cx * 2 + cz) as usize] = h;
                        }
                    }
                    Some(hs)
                } else {
                    None
                };
                // Thin slabs (snow layers): the top face and every
                // side's upper corners drop to the declared height.
                let top_drop = match reg.block(b).height {
                    Some(h) if !water => 1.0 - h,
                    _ => 0.0,
                };

                for face in 0..6 {
                    let n = NORMALS[face];
                    let nb = get(lx + n[0], y + n[1], lz + n[2]);
                    if !should_draw(reg, b, nb) {
                        continue;
                    }
                    let slot = match (face, reg.block(b).fert_tiles) {
                        // Soil wears its fertility on its top face:
                        // the meta quartile picks dust through loam.
                        (2, Some(ft)) => {
                            ft[((chunk.meta(lx as usize, y as usize, lz as usize) & 63) >> 4)
                                as usize]
                        }
                        _ => reg.block(b).tiles[face],
                    };
                    // Break the repeat: one of this tile's alternate looks,
                    // chosen per block face from the world position.
                    let slot = variants.pick(slot, bx + lx, y, bz + lz, face);
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
                            match surf {
                                Some(hs) => py = y as f32 + hs[c[0] as usize * 2 + c[2] as usize],
                                None => py -= top_drop,
                            }
                        }
                        let (u, v) = match face {
                            0 | 1 => (c[2], 1.0 - c[1]),
                            4 | 5 => (c[0], 1.0 - c[1]),
                            _ => (c[0], c[2]),
                        };
                        let ao_f = 0.4 + 0.2 * ao[ci] as f32;
                        let (curved_pos, curved_normal) = curved(px, py, pz, nrm);
                        verts.push(Vertex {
                            pos: curved_pos,
                            uv: tile_uv(tx, ty, u, v),
                            normal: curved_normal,
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
                            base + 2,
                            base + 1,
                            base,
                            base + 3,
                            base + 2,
                        ]);
                    } else {
                        idx.extend_from_slice(&[
                            base + 1,
                            base + 3,
                            base + 2,
                            base + 1,
                            base,
                            base + 3,
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
        let bb = get(p[0] + n[0] + dx, p[1] + n[1] + dy, p[2] + n[2] + dz);
        reg.is_opaque(bb)
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
