//! Terrain v2: "Caves & Cliffs"-style generation (docs/terrain-v2-plan.md).
//!
//! Pipeline per chunk: 3D density shaping (lattice-sampled + trilinear
//! interpolation) -> cave carving (cheese + spaghetti) -> slope/altitude-aware
//! surface rules -> data-driven ores -> biome vegetation -> bedrock.

use noise::{NoiseFn, Perlin};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::{AIR, BlockId, Registry};

/// Climate-derived biomes, chosen by nearest centroid in climate space.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Forest,
    Plains,
    Desert,
    Jungle,
    Scrubland,
    Taiga,
    Arctic,
    Mountains,
}

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Biome::Forest => "Forest",
            Biome::Plains => "Plains",
            Biome::Desert => "Desert",
            Biome::Jungle => "Jungle",
            Biome::Scrubland => "Scrubland",
            Biome::Taiga => "Taiga",
            Biome::Arctic => "Arctic",
            Biome::Mountains => "Mountains",
        }
    }
}

/// (biome, temperature, humidity, continentalness, erosion) centroids.
const CENTROIDS: [(Biome, f32, f32, f32, f32); 8] = [
    (Biome::Plains, 0.1, -0.2, 0.3, 0.6),
    (Biome::Forest, 0.1, 0.4, 0.3, 0.2),
    (Biome::Jungle, 0.7, 0.7, 0.3, 0.3),
    (Biome::Desert, 0.8, -0.7, 0.3, 0.4),
    (Biome::Scrubland, 0.5, -0.3, 0.2, 0.5),
    (Biome::Taiga, -0.5, 0.2, 0.3, 0.3),
    (Biome::Arctic, -0.8, 0.0, 0.3, 0.4),
    (Biome::Mountains, -0.1, 0.0, 0.5, -0.7),
];

/// Piecewise-linear spline over sorted control points, clamped at the ends.
pub struct Spline(Vec<(f32, f32)>);

impl Spline {
    pub fn new(pts: &[(f32, f32)]) -> Spline {
        Spline(pts.to_vec())
    }

    pub fn at(&self, x: f32) -> f32 {
        let p = &self.0;
        if x <= p[0].0 {
            return p[0].1;
        }
        for w in p.windows(2) {
            if x <= w[1].0 {
                let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                return w[0].1 + (w[1].1 - w[0].1) * t;
            }
        }
        p[p.len() - 1].1
    }
}

#[derive(Clone, Copy)]
pub struct Climate {
    pub t: f32,
    pub h: f32,
    pub c: f32,
    pub e: f32,
    /// Folded ridges 0..1 (1 = ridge crest).
    pub r: f32,
}

pub struct Generator {
    base3d: [Perlin; 3],
    cont: Perlin,
    ero: Perlin,
    ridge: Perlin,
    temperature: Perlin,
    moisture: Perlin,
    cheese: Perlin,
    spag1: Perlin,
    spag2: Perlin,
    detail: Perlin,
    seed: u32,
    offset_base: Spline,
    mountain_amp: Spline,
    factor_spline: Spline,
    // Resolved block ids.
    grass: BlockId,
    dirt: BlockId,
    stone: BlockId,
    sand: BlockId,
    gravel: BlockId,
    water: BlockId,
    log: BlockId,
    leaves: BlockId,
    birch_log: BlockId,
    birch_leaves: BlockId,
    spruce_log: BlockId,
    spruce_leaves: BlockId,
    jungle_log: BlockId,
    jungle_leaves: BlockId,
    acacia_log: BlockId,
    acacia_leaves: BlockId,
    bedrock: BlockId,
    snow: BlockId,
    ice: BlockId,
    cactus: BlockId,
}

fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9;
    h = h.wrapping_add(x as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_add(z as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

const RING: usize = CHUNK_X + 2; // heightmap with a 1-block border

impl Generator {
    pub fn new(seed: u32, reg: &Registry) -> Generator {
        let b = |name: &str| reg.block_id(name).unwrap_or(AIR);
        let p = |k: u32| Perlin::new(seed.wrapping_add(k));
        Generator {
            base3d: [p(10), p(11), p(12)],
            cont: p(20),
            ero: p(21),
            ridge: p(22),
            temperature: p(4),
            moisture: p(5),
            cheese: p(30),
            spag1: p(31),
            spag2: p(32),
            detail: p(2),
            seed,
            // Continental base height: ocean floor -> coast -> inland.
            offset_base: Spline::new(&[
                (-1.0, 38.0),
                (-0.45, 52.0),
                (-0.18, 62.0),
                (-0.05, 66.0),
                (0.2, 72.0),
                (0.6, 84.0),
                (1.0, 92.0),
            ]),
            // Mountain amplitude by erosion (low erosion = young peaks).
            mountain_amp: Spline::new(&[
                (-1.0, 130.0),
                (-0.6, 85.0),
                (-0.3, 38.0),
                (0.0, 14.0),
                (0.5, 5.0),
                (1.0, 0.0),
            ]),
            // Vertical squish by erosion (high erosion = flat).
            factor_spline: Spline::new(&[
                (-1.0, 1.7),
                (-0.5, 2.6),
                (0.0, 4.2),
                (0.5, 6.5),
                (1.0, 8.5),
            ]),
            grass: b("base:grass"),
            dirt: b("base:dirt"),
            stone: b("base:stone"),
            sand: b("base:sand"),
            gravel: b("base:gravel"),
            water: b("base:water"),
            log: b("base:log"),
            leaves: b("base:leaves"),
            birch_log: b("base:birch_log"),
            birch_leaves: b("base:birch_leaves"),
            spruce_log: b("base:spruce_log"),
            spruce_leaves: b("base:spruce_leaves"),
            jungle_log: b("base:jungle_log"),
            jungle_leaves: b("base:jungle_leaves"),
            acacia_log: b("base:acacia_log"),
            acacia_leaves: b("base:acacia_leaves"),
            bedrock: b("base:bedrock"),
            snow: b("base:snow"),
            ice: b("base:ice"),
            cactus: b("base:cactus"),
        }
    }

    pub fn climate(&self, wx: i32, wz: i32) -> Climate {
        let x = wx as f64;
        let z = wz as f64;
        let c = self.cont.get([x / 900.0, z / 900.0]) as f32;
        let e = self.ero.get([x / 700.0 + 13.5, z / 700.0 - 7.2]) as f32;
        let r_raw = self.ridge.get([x / 400.0 - 3.3, z / 400.0 + 21.7]) as f32;
        let r = 1.0 - (2.0 * r_raw.abs() - 1.0).abs(); // folded, 0..1
        let t = self.temperature.get([x * 0.0026, z * 0.0026]) as f32;
        let h = self.moisture.get([x * 0.0026 + 31.7, z * 0.0026 - 17.3]) as f32;
        Climate { t, h, c, e, r }
    }

    pub fn biome(&self, wx: i32, wz: i32) -> Biome {
        let cl = self.climate(wx, wz);
        let mut best = Biome::Plains;
        let mut best_d = f32::MAX;
        for (biome, t, h, c, e) in CENTROIDS {
            // Mountains only exist meaningfully inland — the same land mask
            // that gates their height gates the biome label.
            if biome == Biome::Mountains && cl.c < 0.05 {
                continue;
            }
            let d = (cl.t - t).powi(2)
                + (cl.h - h).powi(2)
                + (cl.c - c).powi(2) * 1.5
                + (cl.e - e).powi(2);
            if d < best_d {
                best_d = d;
                best = biome;
            }
        }
        best
    }

    /// Spline-driven terrain parameters for a column: (offset, factor).
    fn column_params(&self, wx: i32, wz: i32) -> (f32, f32) {
        let cl = self.climate(wx, wz);
        let base = self.offset_base.at(cl.c);
        // Mountains rise inland only; ridges concentrate them into ranges.
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let mtn = self.mountain_amp.at(cl.e) * (0.35 + 0.65 * cl.r) * land;
        let offset = (base + mtn).min(CHUNK_Y as f32 - 26.0);
        (offset, self.factor_spline.at(cl.e))
    }

    /// Cheap surface estimate (spline offset) for spawn search and tooling.
    pub fn surface_estimate(&self, wx: i32, wz: i32) -> i32 {
        self.column_params(wx, wz).0 as i32
    }

    fn density_at(&self, wx: f64, y: f64, wz: f64, offset: f32, factor: f32) -> f32 {
        let mut n = 0.0f64;
        let mut amp = 1.0;
        let mut freq = 1.0;
        for p in &self.base3d {
            n += p.get([wx / 171.0 * freq, y / 128.0 * freq, wz / 171.0 * freq]) * amp;
            freq *= 2.0;
            amp *= 0.5;
        }
        let n = (n / 1.75) as f32; // ~[-1, 1]
        let dy = offset - y as f32;
        let s = if dy < 0.0 { factor * 0.011 } else { factor.max(3.0) * 0.026 };
        n * 0.62 + dy * s
    }

    /// Sample density on a 4x8x4 lattice covering the chunk plus a 4-block
    /// apron, so border columns interpolate identically to their neighbors.
    fn sample_lattice(&self, pos: ChunkPos) -> Vec<f32> {
        const NX: usize = 7; // x/z: -4, 0, 4, 8, 12, 16, 20
        const NY: usize = CHUNK_Y / 8 + 1;
        let bx = pos.x * CHUNK_X as i32 - 4;
        let bz = pos.z * CHUNK_Z as i32 - 4;
        let mut lat = vec![0f32; NX * NX * NY];
        for ix in 0..NX {
            for iz in 0..NX {
                let wx = bx + ix as i32 * 4;
                let wz = bz + iz as i32 * 4;
                let (offset, factor) = self.column_params(wx, wz);
                for iy in 0..NY {
                    let y = (iy * 8) as f64;
                    lat[(ix * NX + iz) * NY + iy] =
                        self.density_at(wx as f64, y, wz as f64, offset, factor);
                }
            }
        }
        lat
    }

    /// Trilinear interpolation of the lattice at block coords relative to the
    /// chunk origin (lx/lz may be -1..=16 for the apron ring).
    fn lat_density(lat: &[f32], lx: i32, y: i32, lz: i32) -> f32 {
        const NX: usize = 7;
        const NY: usize = CHUNK_Y / 8 + 1;
        let fx = (lx + 4) as f32 / 4.0;
        let fz = (lz + 4) as f32 / 4.0;
        let fy = y as f32 / 8.0;
        let (ix, iy, iz) = (fx as usize, fy as usize, fz as usize);
        let (ix1, iy1, iz1) = ((ix + 1).min(NX - 1), (iy + 1).min(NY - 1), (iz + 1).min(NX - 1));
        let (tx, ty, tz) = (fx - ix as f32, fy - iy as f32, fz - iz as f32);
        let g = |x: usize, z: usize, y: usize| lat[(x * NX + z) * NY + y];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c00 = lerp(g(ix, iz, iy), g(ix1, iz, iy), tx);
        let c01 = lerp(g(ix, iz, iy1), g(ix1, iz, iy1), tx);
        let c10 = lerp(g(ix, iz1, iy), g(ix1, iz1, iy), tx);
        let c11 = lerp(g(ix, iz1, iy1), g(ix1, iz1, iy1), tx);
        lerp(lerp(c00, c10, tz), lerp(c01, c11, tz), ty)
    }

    pub fn generate(&self, pos: ChunkPos, reg: &Registry) -> Chunk {
        let mut c = Chunk::new();
        let bx = pos.x * CHUNK_X as i32;
        let bz = pos.z * CHUNK_Z as i32;
        let lat = self.sample_lattice(pos);

        // Stage 1: shape. Track pre-carve solid tops for the 18x18 ring.
        let mut shape_top = [[0i32; RING]; RING];
        for rx in 0..RING as i32 {
            for rz in 0..RING as i32 {
                let (lx, lz) = (rx - 1, rz - 1);
                let mut top = 0;
                for y in (1..CHUNK_Y as i32).rev() {
                    if Self::lat_density(&lat, lx, y, lz) > 0.0 {
                        top = y;
                        break;
                    }
                }
                shape_top[rx as usize][rz as usize] = top;
                if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                    continue;
                }
                for y in 1..CHUNK_Y as i32 {
                    let solid = Self::lat_density(&lat, lx, y, lz) > 0.0;
                    let b = if solid {
                        self.stone
                    } else if y <= SEA_LEVEL {
                        self.water
                    } else {
                        AIR
                    };
                    c.set(lx as usize, y as usize, lz as usize, b);
                }
            }
        }

        // Stage 2: carve caves (stone only, never the bedrock rows).
        for lx in 0..CHUNK_X as i32 {
            for lz in 0..CHUNK_Z as i32 {
                let wx = (bx + lx) as f64;
                let wz = (bz + lz) as f64;
                let top = shape_top[(lx + 1) as usize][(lz + 1) as usize];
                for y in 5..top.min(CHUNK_Y as i32 - 1) {
                    if c.get(lx as usize, y as usize, lz as usize) != self.stone {
                        continue;
                    }
                    let depth = (top - y).max(0) as f32;
                    let yf = y as f64;
                    // Cheese: big voids, more common deeper down.
                    let ch = self.cheese.get([wx / 120.0, yf / 85.0, wz / 120.0]) as f32;
                    let cheese_thr =
                        0.74 - (SEA_LEVEL as f32 - y as f32).clamp(0.0, 50.0) * 0.004;
                    // Spaghetti: two noises near zero = a winding tunnel.
                    // Width tapers near the surface so entrances are rare.
                    let taper = (depth / 12.0).min(1.0);
                    let w = (0.055 + depth * 0.0003) * taper;
                    let s1 = self.spag1.get([wx / 70.0, yf / 55.0, wz / 70.0]) as f32;
                    let s2 =
                        self.spag2.get([wx / 70.0 + 41.0, yf / 55.0, wz / 70.0 - 13.0]) as f32;
                    if ch > cheese_thr || (s1.abs() < w && s2.abs() < w) {
                        c.set(lx as usize, y as usize, lz as usize, AIR);
                    }
                }
            }
        }

        // Stage 3: surface rules.
        let mut heights = [[0i32; CHUNK_Z]; CHUNK_X];
        let mut biomes = [[Biome::Plains; CHUNK_Z]; CHUNK_X];
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                let wx = bx + lx as i32;
                let wz = bz + lz as i32;
                let biome = self.biome(wx, wz);
                biomes[lx][lz] = biome;

                // Post-carve top solid.
                let mut top = shape_top[lx + 1][lz + 1];
                while top > 0 && c.get(lx, top as usize, lz) != self.stone {
                    top -= 1;
                }
                heights[lx][lz] = top;

                // Steepness from the pre-carve heightmap ring (consistent
                // across chunk borders by construction).
                let h0 = shape_top[lx + 1][lz + 1];
                let mut slope = 0;
                for (dx, dz) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                    let n =
                        shape_top[(lx as i32 + 1 + dx) as usize][(lz as i32 + 1 + dz) as usize];
                    slope = slope.max((h0 - n).abs());
                }
                let steep = slope >= 3;
                let underwater = top < SEA_LEVEL - 1;
                let snowcap = top >= 170
                    || (biome == Biome::Mountains
                        && top >= 150
                        && self.detail.get([wx as f64 * 0.11, wz as f64 * 0.11]) > -0.2);

                let scrub_sandy = self.detail.get([wx as f64 * 0.03, wz as f64 * 0.03]) > 0.15;
                let (top_b, under_b) = if underwater {
                    if top < SEA_LEVEL - 14 {
                        (self.gravel, self.gravel)
                    } else {
                        (self.sand, self.sand)
                    }
                } else if snowcap {
                    (self.snow, self.stone)
                } else if biome == Biome::Mountains || steep {
                    (self.stone, self.stone)
                } else {
                    let beach = top <= SEA_LEVEL + 1;
                    match biome {
                        Biome::Desert => (self.sand, self.sand),
                        Biome::Scrubland if scrub_sandy => (self.sand, self.sand),
                        Biome::Arctic => (self.snow, self.dirt),
                        _ if beach => (self.sand, self.sand),
                        _ => (self.grass, self.dirt),
                    }
                };

                // Apply to the consecutive solid run from the top.
                if top > 0 && top_b != self.stone {
                    c.set(lx, top as usize, lz, top_b);
                    for d in 1..=3i32 {
                        let y = top - d;
                        if y <= 0 || c.get(lx, y as usize, lz) != self.stone {
                            break;
                        }
                        c.set(lx, y as usize, lz, under_b);
                    }
                }

                // Frozen ocean surface.
                if biome == Biome::Arctic && c.get(lx, SEA_LEVEL as usize, lz) == self.water {
                    c.set(lx, SEA_LEVEL as usize, lz, self.ice);
                }
            }
        }

        self.plant_ores(&mut c, pos, reg);
        self.plant_trees(&mut c, pos, &heights, &biomes);

        // Bedrock floor.
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                c.set(lx, 0, lz, self.bedrock);
            }
        }
        c.dirty = true;
        c.modified = false;
        c
    }

    /// Data-driven ore veins from mod features, deterministic per chunk.
    fn plant_ores(&self, c: &mut Chunk, pos: ChunkPos, reg: &Registry) {
        for (fi, ore) in reg.ores.iter().enumerate() {
            let mut rng = hash2(self.seed ^ (fi as u32).wrapping_mul(0x9e37), pos.x, pos.z);
            let mut next = || {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                rng >> 8
            };
            for _ in 0..ore.per_chunk {
                let (mut x, mut y, mut z) = (
                    (next() % CHUNK_X as u32) as i32,
                    ore.y_min + (next() % (ore.y_max - ore.y_min).max(1) as u32) as i32,
                    (next() % CHUNK_Z as u32) as i32,
                );
                for _ in 0..ore.vein_size {
                    if x >= 0
                        && x < CHUNK_X as i32
                        && y > 0
                        && y < CHUNK_Y as i32
                        && z >= 0
                        && z < CHUNK_Z as i32
                        && c.get(x as usize, y as usize, z as usize) == ore.replaces
                    {
                        c.set(x as usize, y as usize, z as usize, ore.block);
                    }
                    match next() % 6 {
                        0 => x += 1,
                        1 => x -= 1,
                        2 => y += 1,
                        3 => y -= 1,
                        4 => z += 1,
                        _ => z -= 1,
                    }
                }
            }
        }
    }

    fn plant_trees(
        &self,
        c: &mut Chunk,
        pos: ChunkPos,
        heights: &[[i32; CHUNK_Z]; CHUNK_X],
        biomes: &[[Biome; CHUNK_Z]; CHUNK_X],
    ) {
        let bx = pos.x * CHUNK_X as i32;
        let bz = pos.z * CHUNK_Z as i32;
        for lx in 2..CHUNK_X - 2 {
            for lz in 2..CHUNK_Z - 2 {
                let wx = bx + lx as i32;
                let wz = bz + lz as i32;
                let biome = biomes[lx][lz];
                let density = match biome {
                    Biome::Jungle => 34,
                    Biome::Taiga => 70,
                    Biome::Forest => 97,
                    Biome::Scrubland => 240,
                    Biome::Plains => 550,
                    Biome::Desert => 190, // cacti
                    Biome::Arctic | Biome::Mountains => 0,
                };
                if density == 0 || hash2(self.seed, wx, wz) % density != 0 {
                    continue;
                }
                let h = heights[lx][lz];
                if h <= SEA_LEVEL + 1 || h + 11 >= CHUNK_Y as i32 {
                    continue;
                }
                let surface = c.get(lx, h as usize, lz);
                let rnd = hash2(self.seed ^ 0xabcd, wx, wz);

                if biome == Biome::Desert {
                    if surface == self.sand {
                        let ch = 2 + (rnd % 2) as i32;
                        for y in 1..=ch {
                            c.set(lx, (h + y) as usize, lz, self.cactus);
                        }
                    }
                    continue;
                }
                if surface != self.grass {
                    continue;
                }
                c.set(lx, h as usize, lz, self.dirt);
                let base = h + 1;

                // Wood family per biome; forests mix oak with birch.
                let (wood, leaf) = match biome {
                    Biome::Taiga => (self.spruce_log, self.spruce_leaves),
                    Biome::Jungle => (self.jungle_log, self.jungle_leaves),
                    Biome::Scrubland => (self.acacia_log, self.acacia_leaves),
                    Biome::Forest if rnd % 10 < 3 => (self.birch_log, self.birch_leaves),
                    _ => (self.log, self.leaves),
                };

                match biome {
                    Biome::Scrubland => {
                        c.set(lx, base as usize, lz, wood);
                        for dx in -1i32..=1 {
                            for dz in -1i32..=1 {
                                let (x, y, z) = (lx as i32 + dx, base + 1, lz as i32 + dz);
                                if c.get(x as usize, y as usize, z as usize) == AIR {
                                    c.set(x as usize, y as usize, z as usize, leaf);
                                }
                            }
                        }
                    }
                    Biome::Taiga => {
                        let trunk_h = 5 + (rnd % 3) as i32;
                        for y in 0..trunk_h {
                            c.set(lx, (base + y) as usize, lz, wood);
                        }
                        let top = base + trunk_h;
                        for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                            let r = if dy == -3 || dy == -1 { r } else { 1 };
                            for dx in -r..=r {
                                for dz in -r..=r {
                                    if dx.abs() == r && dz.abs() == r && r > 1 {
                                        continue;
                                    }
                                    if dx == 0 && dz == 0 && dy < 0 {
                                        continue;
                                    }
                                    let (x, y, z) = (lx as i32 + dx, top + dy, lz as i32 + dz);
                                    if y < 0 || y >= CHUNK_Y as i32 {
                                        continue;
                                    }
                                    if c.get(x as usize, y as usize, z as usize) == AIR {
                                        c.set(x as usize, y as usize, z as usize, leaf);
                                    }
                                }
                            }
                        }
                        c.set(lx, (top + 2).min(CHUNK_Y as i32 - 1) as usize, lz, leaf);
                    }
                    _ => {
                        let jungle = biome == Biome::Jungle;
                        let trunk_h = if jungle {
                            6 + (rnd % 3) as i32
                        } else {
                            4 + (rnd % 3) as i32
                        };
                        for y in 0..trunk_h {
                            c.set(lx, (base + y) as usize, lz, wood);
                        }
                        let top = base + trunk_h;
                        let big: i32 = if jungle { 3 } else { 2 };
                        for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                            for dx in -r..=r {
                                for dz in -r..=r {
                                    if dx == 0 && dz == 0 && dy < 0 {
                                        continue;
                                    }
                                    if dx.abs() == r && dz.abs() == r && (r >= 2 || dy == 1) {
                                        continue;
                                    }
                                    let (x, y, z) = (lx as i32 + dx, top + dy, lz as i32 + dz);
                                    if x < 0
                                        || x >= CHUNK_X as i32
                                        || z < 0
                                        || z >= CHUNK_Z as i32
                                        || y < 0
                                        || y >= CHUNK_Y as i32
                                    {
                                        continue;
                                    }
                                    if c.get(x as usize, y as usize, z as usize) == AIR {
                                        c.set(x as usize, y as usize, z as usize, leaf);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
