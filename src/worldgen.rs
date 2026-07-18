//! Procedural terrain generation: fBm heightmap, beaches, caves, trees,
//! and data-driven ore features from the registry.

use noise::{NoiseFn, Perlin};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::{AIR, BlockId, Registry};

pub struct Generator {
    height_lo: Perlin,
    height_hi: Perlin,
    detail: Perlin,
    cave: Perlin,
    seed: u32,
    // Resolved base block ids.
    grass: BlockId,
    dirt: BlockId,
    stone: BlockId,
    sand: BlockId,
    water: BlockId,
    log: BlockId,
    leaves: BlockId,
    bedrock: BlockId,
}

fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    let mut h = seed ^ 0x9e37_79b9;
    h = h.wrapping_add(x as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_add(z as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

impl Generator {
    pub fn new(seed: u32, reg: &Registry) -> Generator {
        let b = |name: &str| reg.block_id(name).unwrap_or(AIR);
        Generator {
            height_lo: Perlin::new(seed),
            height_hi: Perlin::new(seed.wrapping_add(1)),
            detail: Perlin::new(seed.wrapping_add(2)),
            cave: Perlin::new(seed.wrapping_add(3)),
            seed,
            grass: b("base:grass"),
            dirt: b("base:dirt"),
            stone: b("base:stone"),
            sand: b("base:sand"),
            water: b("base:water"),
            log: b("base:log"),
            leaves: b("base:leaves"),
            bedrock: b("base:bedrock"),
        }
    }

    pub fn height(&self, wx: i32, wz: i32) -> i32 {
        let x = wx as f64;
        let z = wz as f64;
        let lo = self.height_lo.get([x * 0.004, z * 0.004]);
        let hi = self.height_hi.get([x * 0.02, z * 0.02]);
        let d = self.detail.get([x * 0.08, z * 0.08]);
        let h = SEA_LEVEL as f64 + 2.0 + lo * 22.0 + hi * 7.0 + d * 2.0;
        (h as i32).clamp(4, CHUNK_Y as i32 - 20)
    }

    fn is_cave(&self, wx: i32, wy: i32, wz: i32) -> bool {
        if wy <= 4 {
            return false;
        }
        let v = self.cave.get([wx as f64 * 0.055, wy as f64 * 0.075, wz as f64 * 0.055]);
        v > 0.58
    }

    pub fn generate(&self, pos: ChunkPos, reg: &Registry) -> Chunk {
        let mut c = Chunk::new();
        let bx = pos.x * CHUNK_X as i32;
        let bz = pos.z * CHUNK_Z as i32;

        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                let wx = bx + lx as i32;
                let wz = bz + lz as i32;
                let h = self.height(wx, wz);
                let beach = h <= SEA_LEVEL + 1;

                for y in 0..CHUNK_Y as i32 {
                    let b = if y == 0 {
                        self.bedrock
                    } else if y > h {
                        if y <= SEA_LEVEL { self.water } else { AIR }
                    } else if self.is_cave(wx, y, wz) && y > SEA_LEVEL - 8 {
                        AIR
                    } else if y == h {
                        if beach { self.sand } else { self.grass }
                    } else if y >= h - 3 {
                        if beach { self.sand } else { self.dirt }
                    } else {
                        self.stone
                    };
                    c.set(lx, y as usize, lz, b);
                }
            }
        }

        self.plant_ores(&mut c, pos, reg);
        self.plant_trees(&mut c, pos);
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

    fn plant_trees(&self, c: &mut Chunk, pos: ChunkPos) {
        let bx = pos.x * CHUNK_X as i32;
        let bz = pos.z * CHUNK_Z as i32;
        for lx in 2..CHUNK_X - 2 {
            for lz in 2..CHUNK_Z - 2 {
                let wx = bx + lx as i32;
                let wz = bz + lz as i32;
                if hash2(self.seed, wx, wz) % 97 != 0 {
                    continue;
                }
                let h = self.height(wx, wz);
                if h <= SEA_LEVEL + 1 || h + 8 >= CHUNK_Y as i32 {
                    continue;
                }
                if c.get(lx, h as usize, lz) != self.grass {
                    continue;
                }
                let trunk_h = 4 + (hash2(self.seed ^ 0xabcd, wx, wz) % 3) as i32;
                let base = h + 1;
                for y in 0..trunk_h {
                    c.set(lx, (base + y) as usize, lz, self.log);
                }
                c.set(lx, h as usize, lz, self.dirt);
                let top = base + trunk_h;
                for (dy, r) in [(-2i32, 2i32), (-1, 2), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            if dx.abs() == r && dz.abs() == r && (r == 2 || dy == 1) {
                                continue;
                            }
                            let (x, y, z) = (lx as i32 + dx, top + dy, lz as i32 + dz);
                            if y < 0 || y >= CHUNK_Y as i32 {
                                continue;
                            }
                            let (xu, yu, zu) = (x as usize, y as usize, z as usize);
                            if c.get(xu, yu, zu) == AIR {
                                c.set(xu, yu, zu, self.leaves);
                            }
                        }
                    }
                }
            }
        }
    }
}
