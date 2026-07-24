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
    Swamp,
    Savanna,
    Tundra,
    Badlands,
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
            Biome::Swamp => "Swamp",
            Biome::Savanna => "Savanna",
            Biome::Tundra => "Tundra",
            Biome::Badlands => "Badlands",
        }
    }
}

/// (biome, temperature, humidity, continentalness, erosion) centroids.
const CENTROIDS: [(Biome, f32, f32, f32, f32); 12] = [
    (Biome::Swamp, 0.45, 0.75, 0.12, 0.75),
    (Biome::Savanna, 0.75, -0.25, 0.3, 0.55),
    (Biome::Tundra, -0.62, -0.35, 0.3, 0.45),
    (Biome::Badlands, 0.85, -0.55, 0.35, -0.1),
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

/// A deterministic volcano: center, reach, and rise. The cone stamps
/// the terrain spline; the crater dips it back; generate() pools the
/// crater with lava and dresses rim and flanks.
#[derive(Clone, Copy)]
/// What a prospecting strike reveals: each regional feature's rough
/// distance in blocks and the offset it was found at (for a compass
/// direction), or None past the pick's reach.
pub struct ProspectReading {
    pub pluton: Option<(i32, (i32, i32))>,
    pub volcano: Option<(i32, (i32, i32))>,
    pub pipe: Option<(i32, (i32, i32))>,
    pub geode: Option<(i32, (i32, i32))>,
}

pub struct Volcano {
    pub x: i32,
    pub z: i32,
    pub radius: f32,
    pub height: f32,
}

impl Volcano {
    pub fn dist(&self, wx: i32, wz: i32) -> f32 {
        (((wx - self.x).pow(2) + (wz - self.z).pow(2)) as f32).sqrt()
    }

    /// Cone strength 0..1 at a column (1 = the crater's heart).
    pub fn strength(&self, wx: i32, wz: i32) -> f32 {
        (1.0 - self.dist(wx, wz) / self.radius).max(0.0)
    }

    pub fn crater_r(&self) -> f32 {
        7.0 + self.radius * 0.07
    }

    /// Height added to the terrain offset at a column.
    fn cone(&self, wx: i32, wz: i32) -> f32 {
        let d = self.dist(wx, wz);
        let t = (1.0 - d / self.radius).max(0.0);
        let mut cone = self.height * t.powf(1.6);
        let cr = self.crater_r();
        if d < cr {
            cone -= (1.0 - d / cr) * self.height * 0.30;
        }
        cone
    }
}

/// A static tectonic reading for a column: nothing moves and nothing
/// quakes, but the land remembers the pressure — which plate it sits
/// on, how far the nearest boundary lies, and how hard the two sides
/// press together there.
#[derive(Clone, Copy)]
pub struct Tectonics {
    /// Approximate distance to the nearest plate boundary, in blocks.
    pub boundary_dist: f32,
    /// Closing speed across that boundary: positive plates collide
    /// (fold mountains), negative plates part (rifts).
    pub convergence: f32,
    /// Coordinate along the boundary (phase for fold trains).
    pub along: f32,
    /// Crust kinds on each side: oceanic plates ride low.
    pub oceanic: bool,
    pub neighbor_oceanic: bool,
}

#[derive(Clone, Copy)]
pub struct Climate {
    pub t: f32,
    pub h: f32,
    pub c: f32,
    pub e: f32,
    /// Folded ridges 0..1 (1 = ridge crest).
    pub r: f32,
    pub tec: Tectonics,
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
    wild_wheat: BlockId,
    wild_carrot: BlockId,
    wild_potato: BlockId,
    berry_bush: BlockId,
    jungle_bush: BlockId,
    mushroom: BlockId,
    stone: BlockId,
    sand: BlockId,
    clay: BlockId,
    surface_sand: BlockId,
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
    // Strata (minerals & geology): the rock families.
    sandstone: BlockId,
    limestone: BlockId,
    shale: BlockId,
    granite: BlockId,
    marble: BlockId,
    slate: BlockId,
    quartzite: BlockId,
    basalt: BlockId,
    kimberlite: BlockId,
    carbonatite: BlockId,
    /// Every interior rock (for cave carving and surface scans).
    rocks: [BlockId; 11],
    mud: BlockId,
    lava: BlockId,
    obsidian: BlockId,
    quartz_block: BlockId,
    amethyst_block: BlockId,
    magma_vent: BlockId,
    sulfur_ore: BlockId,
    bandwarp: Perlin,
    granite3d: Perlin,
    rivernoise: Perlin,
    lakenoise: Perlin,
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
            wild_wheat: b("base:wheat_seeds/stage2"),
            wild_carrot: b("base:carrot_crop/stage1"),
            wild_potato: b("base:potato_crop/stage1"),
            berry_bush: b("base:berry_bush/stage1"),
            jungle_bush: b("base:jungle_bush/stage1"),
            mushroom: b("base:wild_mushroom"),
            stone: b("base:stone"),
            sand: b("base:sand"),
            clay: b("base:clay_block"),
            surface_sand: b("base:surface_sand"),
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
            sandstone: b("base:sandstone"),
            limestone: b("base:limestone"),
            shale: b("base:shale"),
            granite: b("base:granite"),
            marble: b("base:marble"),
            slate: b("base:slate"),
            quartzite: b("base:quartzite"),
            basalt: b("base:basalt"),
            kimberlite: b("base:kimberlite"),
            carbonatite: b("base:carbonatite"),
            rocks: [
                b("base:stone"),
                b("base:sandstone"),
                b("base:limestone"),
                b("base:shale"),
                b("base:granite"),
                b("base:marble"),
                b("base:slate"),
                b("base:quartzite"),
                b("base:basalt"),
                b("base:kimberlite"),
                b("base:carbonatite"),
            ],
            mud: b("base:mud"),
            lava: b("base:lava"),
            obsidian: b("base:obsidian"),
            quartz_block: b("base:quartz_block"),
            amethyst_block: b("base:amethyst_block"),
            magma_vent: b("base:magma_vent"),
            sulfur_ore: b("base:sulfur_ore"),
            bandwarp: p(40),
            granite3d: p(42),
            rivernoise: p(50),
            lakenoise: p(51),
        }
    }

    /// Is this block one of the interior rock family? (What caves may
    /// carve and surface scans read through.)
    fn is_rock(&self, b: BlockId) -> bool {
        b != AIR && self.rocks.contains(&b)
    }

    const PLATE_SIZE: f64 = 1400.0;

    fn plate_center(&self, px: i32, pz: i32) -> (f64, f64) {
        let h = hash2(self.seed ^ 0x91a7e, px, pz);
        (
            (px as f64 + 0.15 + ((h & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
            (pz as f64 + 0.15 + (((h >> 16) & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
        )
    }

    fn plate_vel(&self, px: i32, pz: i32) -> (f32, f32) {
        let a = (hash2(self.seed ^ 0x7ec70, px, pz) % 6283) as f32 / 1000.0;
        (a.cos(), a.sin())
    }

    fn plate_oceanic(&self, px: i32, pz: i32) -> bool {
        hash2(self.seed ^ 0x0c00, px, pz) % 10 < 4
    }

    /// The static plate map: jittered-grid Voronoi cells, each with a
    /// deterministic (conceptual) drift vector and crust kind. Nearest
    /// two centers give the boundary; the closing speed across it
    /// decides fold ranges, trenches, and rifts.
    pub fn tectonics(&self, wx: i32, wz: i32) -> Tectonics {
        let gx = (wx as f64 / Self::PLATE_SIZE).floor() as i32;
        let gz = (wz as f64 / Self::PLATE_SIZE).floor() as i32;
        let mut best = (f64::MAX, 0i32, 0i32);
        let mut second = (f64::MAX, 0i32, 0i32);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let (px, pz) = (gx + dx, gz + dz);
                let (cx, cz) = self.plate_center(px, pz);
                let d = (cx - wx as f64).hypot(cz - wz as f64);
                if d < best.0 {
                    second = best;
                    best = (d, px, pz);
                } else if d < second.0 {
                    second = (d, px, pz);
                }
            }
        }
        let (ax, az) = self.plate_center(best.1, best.2);
        let (bx, bz) = self.plate_center(second.1, second.2);
        let (mut nx, mut nz) = ((bx - ax) as f32, (bz - az) as f32);
        let nl = (nx * nx + nz * nz).sqrt().max(1e-3);
        nx /= nl;
        nz /= nl;
        let (vax, vaz) = self.plate_vel(best.1, best.2);
        let (vbx, vbz) = self.plate_vel(second.1, second.2);
        Tectonics {
            boundary_dist: ((second.0 - best.0) * 0.5) as f32,
            convergence: ((vax - vbx) * nx + (vaz - vbz) * nz) * 0.5,
            along: wx as f32 * -nz + wz as f32 * nx,
            oceanic: self.plate_oceanic(best.1, best.2),
            neighbor_oceanic: self.plate_oceanic(second.1, second.2),
        }
    }

    pub fn climate(&self, wx: i32, wz: i32) -> Climate {
        let x = wx as f64;
        let z = wz as f64;
        let tec = self.tectonics(wx, wz);
        // Continents are plate-shaped now: crust kind sets the base
        // level, blended across boundaries, with the old perlin as
        // coastline wiggle and inland variety.
        let crust = |oceanic: bool| if oceanic { -0.62 } else { 0.28 };
        let own = crust(tec.oceanic);
        let other = crust(tec.neighbor_oceanic);
        let blend = (tec.boundary_dist / 260.0).clamp(0.0, 1.0);
        let base_c = own * blend + (own + other) * 0.5 * (1.0 - blend);
        let c = base_c + self.cont.get([x / 900.0, z / 900.0]) as f32 * 0.45;
        let e = self.ero.get([x / 700.0 + 13.5, z / 700.0 - 7.2]) as f32;
        let r_raw = self.ridge.get([x / 400.0 - 3.3, z / 400.0 + 21.7]) as f32;
        let r = 1.0 - (2.0 * r_raw.abs() - 1.0).abs(); // folded, 0..1
        let t = self.temperature.get([x * 0.0026, z * 0.0026]) as f32;
        let h = self.moisture.get([x * 0.0026 + 31.7, z * 0.0026 - 17.3]) as f32;
        Climate { t, h, c, e, r, tec }
    }

    pub fn biome(&self, wx: i32, wz: i32) -> Biome {
        self.biome_from(&self.climate(wx, wz))
    }

    /// Biome from an already-computed climate (the generator's inner
    /// loops read climate once per column and reuse it everywhere).
    pub fn biome_from(&self, cl: &Climate) -> Biome {
        // A young fold range is Mountains whatever the climate says.
        if self.plate_relief(cl) > 30.0 {
            return Biome::Mountains;
        }
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
    /// Plate-driven relief for a column: fold ranges where continents
    /// collide, coastal ranges and offshore trenches at subduction
    /// zones, sunken valleys where plates part. Positive adds height,
    /// negative digs.
    fn plate_relief(&self, cl: &Climate) -> f32 {
        let tec = &cl.tec;
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let belt = (-(tec.boundary_dist / 80.0).powi(2)).exp();
        if tec.convergence > 0.12 {
            if !tec.oceanic && !tec.neighbor_oceanic {
                // Continent meets continent: the big fold ranges,
                // crests rippling along the boundary.
                let ripple = 0.8 + 0.2 * (tec.along / 90.0).sin();
                tec.convergence * 115.0 * belt * ripple * land
            } else if tec.oceanic {
                // The diving side dips into a trench offshore.
                -14.0 * belt * tec.convergence
            } else {
                // Subduction throws a coastal range on the overriding
                // plate (its volcano arc is weighted separately).
                tec.convergence * 70.0 * belt * land
            }
        } else if tec.convergence < -0.12 {
            // Rift valley: the land sags where plates part.
            tec.convergence * 16.0 * belt
        } else {
            0.0
        }
    }

    /// Terrain offset before hydrology: continents, worn highlands,
    /// and plate relief.
    /// Hot, dry, rugged inland climate: mesa country.
    fn is_badlands(cl: &Climate) -> bool {
        cl.t > 0.7 && cl.h < -0.4 && cl.c > 0.1
    }

    fn base_offset(&self, wx: i32, wz: i32, cl: &Climate) -> f32 {
        let base = self.offset_base.at(cl.c);
        // Old erosion mountains stay as worn highlands; the young
        // dramatic ranges belong to the plate boundaries now.
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let mtn = self.mountain_amp.at(cl.e) * (0.35 + 0.65 * cl.r) * land * 0.45;
        let mut off = base + mtn + self.plate_relief(cl);
        if Self::is_badlands(cl) {
            // Stepped mesas: quantized plateaus whose bare walls show
            // the sandstone banding.
            let m = self.detail.get([wx as f64 / 140.0, wz as f64 / 140.0]) as f32;
            off += ((m * 3.0).floor().clamp(0.0, 2.0)) * 11.0;
        }
        off
    }

    /// Raw waterline math for one column, before sealing: the carve,
    /// the candidate water level, and whether the column sits close
    /// enough to a channel or lake basin that sealing must look at it
    /// (gates the neighbor probes — the margins cover the one-block
    /// noise gradient to the true water zones).
    fn hydro_raw(&self, wx: i32, wz: i32, cl: &Climate, pre: f32) -> (f32, Option<i32>, bool) {
        let mut carve = 0.0f32;
        let mut level: Option<i32> = None;
        let mut near = false;
        if pre > SEA_LEVEL as f32 - 2.0 && cl.c > -0.05 {
            let riv = self.rivernoise.get([wx as f64 / 620.0, wz as f64 / 620.0]) as f32;
            let w = 0.012 + 0.010 * (0.6 - cl.c).clamp(0.0, 1.0);
            let shoulder = w * 3.2;
            if riv.abs() < shoulder {
                near = true;
                let t = 1.0 - riv.abs() / shoulder;
                carve += t * t * 8.0;
                if riv.abs() < w {
                    carve += 3.0;
                    // Terraced reaches: quantize the fill so each
                    // stretch of river is dead level, dropping in
                    // discrete falls; on steep runs the level lands
                    // at or under the channel floor and the stretch
                    // stays a dry wash between step pools.
                    let floor = (pre - carve) as i32;
                    let f = floor + 3;
                    let f = f - f.rem_euclid(4);
                    if f > floor {
                        level = Some(f);
                    }
                }
            }
            let lk = self.lakenoise.get([wx as f64 / 300.0, wz as f64 / 300.0]) as f32;
            if lk > 0.56 {
                near = true;
            }
            if lk > 0.58 && pre > SEA_LEVEL as f32 + 2.0 && pre < 120.0 {
                let t = ((lk - 0.58) / 0.42).min(1.0);
                carve += t * 10.0;
                let f2 = (pre - 2.0) as i32;
                let f2 = f2 - f2.rem_euclid(4);
                level = Some(level.map_or(f2, |f| f.max(f2)));
            }
        }
        (carve, level, near)
    }

    /// Rivers and lakes for a column: how deep the water has cut the
    /// terrain, the fill level (a river or lake acts as a local sea
    /// level in the shape pass), and the armor level. Every pool is
    /// sealed by construction: a column whose raw water level drops on
    /// any side becomes a rock weir instead of water, and a dry column
    /// beside water is armored — its non-solid cells below the tallest
    /// adjacent pool become native rock, so 3D-noise wobble and the
    /// shoulder carve can never leave a bank below the waterline. A
    /// woken pool has nowhere to shed: no thin films creeping over the
    /// sand, no floating shelves meeting edge-on. All decisions read
    /// only raw per-column math, so chunks agree without communication.
    pub fn hydrology(
        &self,
        wx: i32,
        wz: i32,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, Option<i32>) {
        let (carve, level, near) = self.hydro_raw(wx, wz, cl, pre);
        if !near {
            return (carve, None, None);
        }
        let mut step_down = false;
        let mut tallest: Option<i32> = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (wx + dx, wz + dz);
            let ncl = self.climate(nx, nz);
            let npre = self.base_offset(nx, nz, &ncl);
            let (_, nlevel, _) = self.hydro_raw(nx, nz, &ncl, npre);
            match (nlevel, level) {
                (Some(nf), Some(f)) if nf < f => step_down = true,
                (Some(nf), None) => tallest = Some(tallest.map_or(nf, |t: i32| t.max(nf))),
                _ => {}
            }
        }
        match level {
            Some(f) if step_down => (carve, None, Some(f)),
            Some(f) => (carve, Some(f), None),
            None => (carve, None, tallest),
        }
    }

    /// The local water level a river or lake gives a column, if any
    /// (tests and tooling; generate() computes the same inline).
    #[cfg(test)]
    pub fn water_features(&self, wx: i32, wz: i32) -> Option<i32> {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        self.hydrology(wx, wz, &cl, pre).1
    }

    /// Does a granite pluton intrude this column at mineable depth?
    /// Mirrors sample_lattice's threshold math. The census measures
    /// with it; the prospecting pick reads with it.
    pub fn pluton_at(&self, wx: i32, wz: i32) -> bool {
        let prov = self
            .granite3d
            .get([wx as f64 / 1400.0, 77.7, wz as f64 / 1400.0]) as f32;
        let prov_pen = (0.44 - prov).max(0.0) * 1.8;
        for y in [16.0f64, 32.0, 48.0, 64.0] {
            let g = self
                .granite3d
                .get([wx as f64 / 230.0, y / 150.0, wz as f64 / 230.0]) as f32;
            if g > 0.55 + prov_pen + y as f32 * 0.0012 {
                return true;
            }
        }
        false
    }

    /// One prospecting reading: what regional geology lies near this
    /// spot, and roughly which way. Everything here is a pure function
    /// of seed and position — the pick reveals, it never rolls.
    /// Detection reaches are deliberately shorter than the rarity
    /// bands: mapping a region takes a SWEEP of readings (surveying is
    /// work, which is what makes a finished survey worth trading).
    pub fn prospect(&self, wx: i32, wz: i32) -> ProspectReading {
        let ring = |step: i32, cap: i32, hit: &dyn Fn(i32, i32) -> bool| {
            if hit(wx, wz) {
                return Some((0, (0, 0)));
            }
            let mut r = step;
            while r <= cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        if hit(wx + dx, wz + dz) {
                            return Some((r, (dx, dz)));
                        }
                    }
                    i += step;
                }
                r += step;
            }
            None
        };
        let cp = ChunkPos::of_world(wx, wz);
        let chunk_ring = |cap: i32, hit: &dyn Fn(ChunkPos) -> bool| {
            if hit(cp) {
                return Some((0, (0, 0)));
            }
            for r in 1..=cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        if hit(ChunkPos {
                            x: cp.x + dx,
                            z: cp.z + dz,
                        }) {
                            return Some((r * 16, (dx * 16, dz * 16)));
                        }
                    }
                    i += 1;
                }
            }
            None
        };
        ProspectReading {
            pluton: ring(64, 1216, &|x, z| self.pluton_at(x, z)),
            volcano: ring(64, 1216, &|x, z| self.volcano_near(x, z).is_some()),
            pipe: chunk_ring(24, &|p| self.pipe_at(p).is_some()),
            geode: chunk_ring(12, &|p| self.geode_at(p).is_some()),
        }
    }

    /// The armor level sealing a column, if any (tests and tooling).
    #[cfg(test)]
    pub fn armor_at(&self, wx: i32, wz: i32) -> Option<i32> {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        self.hydrology(wx, wz, &cl, pre).2
    }

    fn column_params(&self, wx: i32, wz: i32) -> (f32, f32) {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        let (carve, _, _) = self.hydro_raw(wx, wz, &cl, pre);
        let mut offset = (pre - carve).clamp(6.0, CHUNK_Y as f32 - 22.0);
        // A volcano stamps its cone onto the spline terrain, crater
        // bowl and all.
        if let Some(v) = self.volcano_near(wx, wz) {
            offset = (offset + v.cone(wx, wz)).min(CHUNK_Y as f32 - 18.0);
        }
        (offset, self.factor_spline.at(cl.e))
    }

    /// The volcano whose reach covers a column, if any: deterministic
    /// per region cell, so every chunk agrees without communication.
    /// Land and coastal shelves only — volcanic islands are welcome,
    /// the deep ocean floor is not.
    pub fn volcano_near(&self, wx: i32, wz: i32) -> Option<Volcano> {
        const REGION: i32 = 384;
        let rx = wx.div_euclid(REGION);
        let rz = wz.div_euclid(REGION);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let (cx, cz) = (rx + dx, rz + dz);
                let h = hash2(self.seed ^ 0x70_1ca0, cx, cz);
                let margin = 90;
                let ox = (h >> 8) % (REGION - 2 * margin) as u32 + margin as u32;
                let oz = (h >> 17) % (REGION - 2 * margin) as u32 + margin as u32;
                let center_x = cx * REGION + ox as i32;
                let center_z = cz * REGION + oz as i32;
                let v = Volcano {
                    x: center_x,
                    z: center_z,
                    radius: 44.0 + (h % 28) as f32,
                    height: 52.0 + ((h >> 4) % 32) as f32,
                };
                // Distance first: this runs for every column of every
                // chunk, and almost every candidate is out of reach —
                // nothing heavier than hashes may run before this line.
                // (An earlier version computed full climate per
                // candidate and singlehandedly tanked worldgen.)
                let d = v.dist(wx, wz);
                if d >= v.radius + 12.0 {
                    continue;
                }
                // Volcanoes follow the plate map: subduction arcs run
                // thick with them, rifts leak a few, plate interiors
                // almost none. Tectonics is hash-and-math (no perlin);
                // the deep-ocean gate rides the crust kind, which is
                // what continentalness mostly is anyway.
                let tec = self.tectonics(center_x, center_z);
                if tec.oceanic && tec.boundary_dist > 260.0 {
                    continue; // abyssal plate interior: no hotspots
                }
                let subduction = tec.boundary_dist < 260.0
                    && tec.convergence > 0.1
                    && (tec.oceanic || tec.neighbor_oceanic);
                let rift = tec.boundary_dist < 220.0 && tec.convergence < -0.1;
                // Regional-band odds (economy plan): volcanic arcs
                // stay volcanic, plate interiors go quiet — volcanic
                // goods (carbonatite, obsidian, sulfur) are what arc
                // country trades away.
                let odds = if subduction {
                    2
                } else if rift {
                    8
                } else {
                    48
                };
                if !h.is_multiple_of(odds) {
                    continue;
                }
                return Some(v);
            }
        }
        None
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
        let s = if dy < 0.0 {
            factor * 0.011
        } else {
            factor.max(3.0) * 0.026
        };
        n * 0.62 + dy * s
    }

    /// Sample density on a 4x8x4 lattice covering the chunk plus a 4-block
    /// apron, so border columns interpolate identically to their neighbors.
    /// The second channel is the granite intrusion margin: distance past
    /// the (depth-loosening) pluton threshold, baked in so interpolation
    /// carries the widening-with-depth shape for free.
    fn sample_lattice(&self, pos: ChunkPos) -> (Vec<f32>, Vec<f32>) {
        const NX: usize = 7; // x/z: -4, 0, 4, 8, 12, 16, 20
        const NY: usize = CHUNK_Y / 8 + 1;
        let bx = pos.x * CHUNK_X as i32 - 4;
        let bz = pos.z * CHUNK_Z as i32 - 4;
        let mut lat = vec![0f32; NX * NX * NY];
        let mut lat_g = vec![0f32; NX * NX * NY];
        for ix in 0..NX {
            for iz in 0..NX {
                let wx = bx + ix as i32 * 4;
                let wz = bz + iz as i32 * 4;
                let (offset, factor) = self.column_params(wx, wz);
                // Batholith provinces: a coarse gate over the pluton
                // noise. Inside a province intrusions abound; outside,
                // the threshold climbs out of reach — granite country
                // is a REGION you travel to (economy plan, leg 1),
                // not a backyard given.
                let prov = self
                    .granite3d
                    .get([wx as f64 / 1400.0, 77.7, wz as f64 / 1400.0])
                    as f32;
                let prov_pen = (0.44 - prov).max(0.0) * 1.8;
                for iy in 0..NY {
                    let y = (iy * 8) as f64;
                    let i = (ix * NX + iz) * NY + iy;
                    lat[i] = self.density_at(wx as f64, y, wz as f64, offset, factor);
                    let g = self
                        .granite3d
                        .get([wx as f64 / 230.0, y / 150.0, wz as f64 / 230.0])
                        as f32;
                    // Plutons widen downward: the threshold tightens
                    // with altitude, so intrusions taper as they rise.
                    let thr = 0.55 + prov_pen + y as f32 * 0.0012;
                    lat_g[i] = g - thr;
                }
            }
        }
        (lat, lat_g)
    }

    /// Per-column stratigraphy: the top of each layer, gently warped
    /// so bedding drifts instead of ruling straight lines. Returns
    /// (basalt_top, basement_top, shale_top, limestone_top,
    /// sandstone_top); above the last it's basement again — mountain
    /// cores read as uplifted stone.
    fn strata_bands(&self, wx: i32, wz: i32, cl: &Climate) -> [i32; 5] {
        let x = wx as f64;
        let z = wz as f64;
        let w1 = self.bandwarp.get([x / 260.0, z / 260.0]) as f32;
        let w2 = self.bandwarp.get([x / 170.0 + 7.3, z / 170.0 - 2.1]) as f32;
        let wet = cl.h;
        // Two plates smushed together: near a convergent boundary the
        // bedding buckles into fold trains — anticlines and synclines
        // marching along the range, so cliff faces show bent strata.
        let tec = &cl.tec;
        let fold = if tec.convergence > 0.12 && !tec.oceanic && !tec.neighbor_oceanic {
            let belt = (-(tec.boundary_dist / 110.0).powi(2)).exp();
            tec.convergence * belt * 26.0 * (tec.along / 24.0 + w1).sin()
        } else {
            0.0
        };
        let mesa = if Self::is_badlands(cl) { 42.0 } else { 0.0 };
        [
            (8.0 + w1 * 3.0) as i32,
            (34.0 + w1 * 7.0 + fold * 0.5) as i32,
            (50.0 + w2 * 5.0 + wet * 5.0 + fold) as i32,
            (68.0 + w1 * 6.0 + fold) as i32,
            (92.0 + w2 * 9.0 - wet * 6.0 + fold + mesa) as i32,
        ]
    }

    /// The rock for a solid cell: volcanoes build in basalt (with
    /// carbonatite dikes threading their plumbing), granite
    /// intrusions override the stack, their contact halo cooks the
    /// sediment it touches, and the bands decide the rest.
    fn rock_at(&self, y: i32, bands: &[i32; 5], gm: f32, vol: f32, dike: bool) -> BlockId {
        if vol > 0.24 && y > bands[1] {
            return if dike { self.carbonatite } else { self.basalt };
        }
        if gm > 0.0 {
            return self.granite;
        }
        let sediment = if y < bands[0] {
            return self.basalt;
        } else if y < bands[1] {
            return self.stone;
        } else if y < bands[2] {
            self.shale
        } else if y < bands[3] {
            self.limestone
        } else if y < bands[4] {
            self.sandstone
        } else {
            return self.stone;
        };
        // Contact metamorphism: close enough to a pluton to bake.
        if gm > -0.08 {
            if sediment == self.shale {
                return self.slate;
            }
            if sediment == self.limestone {
                return self.marble;
            }
            return self.quartzite;
        }
        sediment
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
        let (ix1, iy1, iz1) = (
            (ix + 1).min(NX - 1),
            (iy + 1).min(NY - 1),
            (iz + 1).min(NX - 1),
        );
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
        let (lat, lat_g) = self.sample_lattice(pos);

        // Stage 1: shape. Track pre-carve solid tops for the 18x18 ring.
        let mut shape_top = [[0i32; RING]; RING];
        let mut fills = [[0i32; CHUNK_Z]; CHUNK_X];
        let mut armors = [[0i32; CHUNK_Z]; CHUNK_X];
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
                let (wx, wz) = (bx + lx, bz + lz);
                // One climate read serves bands, hydrology, and rock.
                let cl = self.climate(wx, wz);
                let bands = self.strata_bands(wx, wz, &cl);
                let vol = self
                    .volcano_near(wx, wz)
                    .map(|v| v.strength(wx, wz))
                    .unwrap_or(0.0);
                let dike =
                    vol > 0.05 && self.detail.get([wx as f64 / 13.0, wz as f64 / 13.0]) > 0.58;
                // Rivers and lakes flood their carve as a local sea.
                let pre = self.base_offset(wx, wz, &cl);
                let (_, fill, armor) = self.hydrology(wx, wz, &cl, pre);
                let fill_y = fill.unwrap_or(0).max(SEA_LEVEL);
                let armor_y = armor.unwrap_or(0);
                fills[lx as usize][lz as usize] = fill_y;
                armors[lx as usize][lz as usize] = armor_y;
                for y in 1..CHUNK_Y as i32 {
                    let solid = Self::lat_density(&lat, lx, y, lz) > 0.0 || y <= armor_y;
                    let b = if solid {
                        self.rock_at(y, &bands, Self::lat_density(&lat_g, lx, y, lz), vol, dike)
                    } else if y <= fill_y {
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
                    if !self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        continue;
                    }
                    let depth = (top - y).max(0) as f32;
                    let yf = y as f64;
                    // Cheese: big voids, more common deeper down.
                    let ch = self.cheese.get([wx / 120.0, yf / 85.0, wz / 120.0]) as f32;
                    let cheese_thr = 0.74 - (SEA_LEVEL as f32 - y as f32).clamp(0.0, 50.0) * 0.004;
                    // Spaghetti: two noises near zero = a winding tunnel.
                    // Width tapers near the surface so entrances are rare.
                    let taper = (depth / 12.0).min(1.0);
                    let w = (0.055 + depth * 0.0003) * taper;
                    let s1 = self.spag1.get([wx / 70.0, yf / 55.0, wz / 70.0]) as f32;
                    let s2 = self
                        .spag2
                        .get([wx / 70.0 + 41.0, yf / 55.0, wz / 70.0 - 13.0])
                        as f32;
                    if y < 11 && ch > 0.32 {
                        // Deep magma pockets: where the cheese noise
                        // merely swells, the rock holds lava instead
                        // of opening — sealed chambers you mine into.
                        // Settled full cells, never queued, until
                        // something breaks the crust.
                        c.set(lx as usize, y as usize, lz as usize, self.lava);
                    } else if ch > cheese_thr || (s1.abs() < w && s2.abs() < w) {
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
                let cl = self.climate(wx, wz);
                let biome = self.biome_from(&cl);
                biomes[lx][lz] = biome;

                // Post-carve top solid (an armored bank can stand
                // above the density top — start the scan at its crest).
                let mut top = shape_top[lx + 1][lz + 1].max(armors[lx][lz]);
                while top > 0 && !self.is_rock(c.get(lx, top as usize, lz)) {
                    top -= 1;
                }
                heights[lx][lz] = top;

                // Steepness from the pre-carve heightmap ring (consistent
                // across chunk borders by construction).
                let h0 = shape_top[lx + 1][lz + 1];
                let mut slope = 0;
                for (dx, dz) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                    let n = shape_top[(lx as i32 + 1 + dx) as usize][(lz as i32 + 1 + dz) as usize];
                    slope = slope.max((h0 - n).abs());
                }
                let steep = slope >= 3;
                let underwater = top < fills[lx][lz].max(SEA_LEVEL) - 1;
                let snowcap = top >= 170
                    || (biome == Biome::Mountains
                        && top >= 150
                        && self.detail.get([wx as f64 * 0.11, wz as f64 * 0.11]) > -0.2);

                let scrub_sandy = self.detail.get([wx as f64 * 0.03, wz as f64 * 0.03]) > 0.15;
                // None = leave the natural rock exposed (bare mountains,
                // steep faces — the strata read in the cliffs).
                let (top_b, under_b): (Option<BlockId>, Option<BlockId>) = if underwater {
                    if top < SEA_LEVEL - 14 {
                        (Some(self.gravel), Some(self.gravel))
                    } else if self.detail.get([wx as f64 / 23.0, wz as f64 / 23.0]) > 0.34 {
                        // Clay beds: patches where still shallows let
                        // the fine sediment settle (wild arc, stage 5
                        // — the crock starts here).
                        (Some(self.clay), Some(self.clay))
                    } else {
                        (Some(self.sand), Some(self.sand))
                    }
                } else if snowcap {
                    (Some(self.snow), None)
                } else if biome == Biome::Mountains
                    || steep
                    || self
                        .volcano_near(wx, wz)
                        .is_some_and(|v| v.strength(wx, wz) > 0.28)
                {
                    // Bare rock: mountains, cliffs, volcano flanks.
                    (None, None)
                } else {
                    let beach = top <= SEA_LEVEL + 1;
                    let patch = self.detail.get([wx as f64 / 9.0, wz as f64 / 9.0]) as f32;
                    match biome {
                        Biome::Desert => (Some(self.sand), Some(self.sand)),
                        Biome::Scrubland if scrub_sandy => (Some(self.sand), Some(self.sand)),
                        Biome::Arctic => (Some(self.snow), Some(self.dirt)),
                        // Mesa country bares its sandstone bones.
                        Biome::Badlands => (None, None),
                        // Frozen barrens: snow, dirt, and gravel patches.
                        Biome::Tundra if patch > 0.22 => (Some(self.snow), Some(self.dirt)),
                        Biome::Tundra if patch < -0.3 => (Some(self.gravel), Some(self.gravel)),
                        Biome::Tundra => (Some(self.dirt), Some(self.dirt)),
                        // Wetlands: standing pools and mud between grass.
                        Biome::Swamp if patch > 0.34 && !beach => {
                            (Some(self.water), Some(self.mud))
                        }
                        Biome::Swamp if patch < -0.22 => (Some(self.mud), Some(self.mud)),
                        _ if beach => (Some(self.sand), Some(self.sand)),
                        _ => (Some(self.grass), Some(self.dirt)),
                    }
                };

                // Apply to the consecutive solid run from the top.
                if top > 0
                    && let Some(tb) = top_b
                {
                    c.set(lx, top as usize, lz, tb);
                    if let Some(ub) = under_b {
                        for d in 1..=3i32 {
                            let y = top - d;
                            if y <= 0 || !self.is_rock(c.get(lx, y as usize, lz)) {
                                break;
                            }
                            c.set(lx, y as usize, lz, ub);
                        }
                    }
                }

                // Mountain springs: rare seeps on high steep ground,
                // a still pool the size of a footprint.
                if top > 110
                    && steep
                    && top + 1 < CHUNK_Y as i32 - 1
                    && hash2(self.seed ^ 0x59a1, wx, wz).is_multiple_of(211)
                    && c.get(lx, (top + 1) as usize, lz) == AIR
                {
                    c.set(lx, top as usize, lz, self.water);
                }

                // Frozen ocean surface.
                if biome == Biome::Arctic && c.get(lx, SEA_LEVEL as usize, lz) == self.water {
                    c.set(lx, SEA_LEVEL as usize, lz, self.ice);
                }
            }
        }

        // Volcano dressing: the crater pools with lava behind an
        // obsidian rim; magma vents stud the inner wall; sulfur
        // crusts the flanks. All deterministic from the volcano's
        // region hash, so chunk borders agree.
        let probe = [(8, 8), (0, 0), (15, 0), (0, 15), (15, 15)]
            .iter()
            .find_map(|&(qx, qz)| self.volcano_near(bx + qx, bz + qz));
        if let Some(v) = probe {
            let pool_y = self.column_params(v.x, v.z).0 as i32 + 4;
            let cr = v.crater_r();
            for (lx, hrow) in heights.iter().enumerate() {
                for (lz, &top) in hrow.iter().enumerate() {
                    let (wx, wz) = (bx + lx as i32, bz + lz as i32);
                    let d = v.dist(wx, wz);
                    if d >= v.radius {
                        continue;
                    }
                    // The magma chamber beneath: an ellipsoid half-full
                    // of lava, feeding the throat above.
                    let ch_r = (v.radius * 0.33).min(20.0);
                    if d < ch_r {
                        for y in 12..=28i32 {
                            let dy = (y - 20) as f32 / 8.0;
                            let dh = d / ch_r;
                            if dh * dh + dy * dy < 1.0 {
                                let b = if y < 20 { self.lava } else { AIR };
                                c.set(lx, y as usize, lz, b);
                            }
                        }
                    }
                    if d < cr - 0.5 {
                        for y in (top + 1).max(2)..=pool_y.min(CHUNK_Y as i32 - 2) {
                            if c.get(lx, y as usize, lz) == AIR {
                                c.set(lx, y as usize, lz, self.lava);
                            }
                        }
                    } else if d < cr + 1.8 {
                        if top > 0 {
                            c.set(lx, top as usize, lz, self.obsidian);
                        }
                    } else if top > 0 {
                        let h = hash2(self.seed ^ 0xbe27, wx, wz);
                        if d < cr + 4.0 && h.is_multiple_of(11) {
                            c.set(lx, top as usize, lz, self.magma_vent);
                        } else if d > cr + 4.0 && d < v.radius * 0.72 && h.is_multiple_of(29) {
                            c.set(lx, top as usize, lz, self.sulfur_ore);
                        }
                    }
                }
            }
        }

        self.plant_pipe(&mut c, pos, &heights);
        self.plant_geode(&mut c, pos);
        self.plant_ores(&mut c, pos, reg);
        self.plant_trees(&mut c, pos, &heights, &biomes);
        self.dust_sand(&mut c, pos, &heights);

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

    /// The kimberlite pipe rolled for a chunk, if any: (local cx, cz,
    /// breaches_surface). Roughly one chunk in four hundred; the pipe
    /// fits inside its chunk's footprint by construction.
    pub fn pipe_at(&self, pos: ChunkPos) -> Option<(usize, usize, bool)> {
        let h = hash2(self.seed ^ 0x8d1a, pos.x, pos.z);
        // Treasure-band rarity (economy plan): a pipe is a multi-km
        // expedition and a famous site, not a backyard curiosity —
        // median nearest ~2.3 km (was 1/397, ~150 blocks).
        if !h.is_multiple_of(90_000) {
            return None;
        }
        let cx = 6 + ((h >> 8) % 5) as usize;
        let cz = 6 + ((h >> 16) % 5) as usize;
        Some((cx, cz, (h >> 24) % 10 < 3))
    }

    /// A kimberlite pipe: a carrot of deep rock punched up through
    /// every stratum — wide near the top, a thread at depth. Most are
    /// blind (topped below the surface, found by mining); the ones
    /// that breach weather into a blue-ground stain, the prospector's
    /// tell. Diamonds only ever live inside these (the ore feature
    /// replaces kimberlite and nothing else).
    fn plant_pipe(&self, c: &mut Chunk, pos: ChunkPos, heights: &[[i32; CHUNK_Z]; CHUNK_X]) {
        let Some((cx, cz, breach)) = self.pipe_at(pos) else {
            return;
        };
        let h = hash2(self.seed ^ 0x8d1a, pos.x, pos.z);
        let surf = heights[cx][cz];
        let top_y = if breach {
            surf
        } else {
            (surf - 6 - ((h >> 26) % 12) as i32).max(20)
        };
        for y in 2..=top_y {
            let t = y as f32 / top_y as f32;
            let r = 1.2 + t * t * 3.6;
            let ri = r.ceil() as i32;
            for dx in -ri..=ri {
                for dz in -ri..=ri {
                    if ((dx * dx + dz * dz) as f32) > r * r {
                        continue;
                    }
                    let (lx, lz) = (cx as i32 + dx, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                        continue;
                    }
                    if self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        c.set(lx as usize, y as usize, lz as usize, self.kimberlite);
                    }
                }
            }
        }
        if breach {
            // Blue ground: the weathered pipe stains the topsoil.
            for dx in -5i32..=5 {
                for dz in -5i32..=5 {
                    let (lx, lz) = (cx as i32 + dx, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                        continue;
                    }
                    if dx * dx + dz * dz <= 20
                        && hash2(self.seed ^ 0xb1e, pos.x * 16 + lx, pos.z * 16 + lz)
                            .is_multiple_of(2)
                    {
                        let top = heights[lx as usize][lz as usize];
                        if top > 0 {
                            c.set(lx as usize, top as usize, lz as usize, self.kimberlite);
                        }
                    }
                }
            }
        }
    }

    /// The geode rolled for a chunk, if any: (local cx, cz, cy, r).
    pub fn geode_at(&self, pos: ChunkPos) -> Option<(usize, usize, i32, i32)> {
        let h = hash2(self.seed ^ 0x6e0d, pos.x, pos.z);
        // Uncommon local luxury (economy plan): median nearest ~200
        // blocks (was 1/89, ~70).
        if !h.is_multiple_of(700) {
            return None;
        }
        let cx = 5 + ((h >> 8) % 7) as usize;
        let cz = 5 + ((h >> 16) % 7) as usize;
        let cy = 46 + ((h >> 24) % 26) as i32;
        let r = 3 + ((h >> 5) % 3) as i32;
        Some((cx, cz, cy, r))
    }

    /// A limestone geode: a rough quartz shell around an amethyst
    /// lining around a void — crack one open with a torch in hand.
    fn plant_geode(&self, c: &mut Chunk, pos: ChunkPos) {
        let Some((cx, cz, cy, r)) = self.geode_at(pos) else {
            return;
        };
        // Only real limestone country hosts them.
        let heart = c.get(cx, cy as usize, cz);
        if heart != self.limestone && heart != self.marble {
            return;
        }
        let h = hash2(self.seed ^ 0x6e0d, pos.x, pos.z);
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 > r * r {
                        continue;
                    }
                    let (lx, y, lz) = (cx as i32 + dx, cy + dy, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx)
                        || !(0..CHUNK_Z as i32).contains(&lz)
                        || y < 2
                        || y >= CHUNK_Y as i32 - 1
                    {
                        continue;
                    }
                    if !self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        continue;
                    }
                    let b = if d2 > (r - 1) * (r - 1) {
                        self.quartz_block
                    } else if d2 > (r - 2) * (r - 2) {
                        if hash2(h, dx * 31 + dy, dz * 17 + dy).is_multiple_of(3) {
                            self.quartz_block
                        } else {
                            self.amethyst_block
                        }
                    } else {
                        AIR
                    };
                    c.set(lx as usize, y as usize, lz as usize, b);
                }
            }
        }
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
                if ore.chance < 1.0 && (next() as f32 / (1 << 24) as f32) >= ore.chance {
                    continue;
                }
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
                    match ore.shape {
                        // Round pockets: drift any direction.
                        crate::registry::VeinShape::Walk => match next() % 6 {
                            0 => x += 1,
                            1 => x -= 1,
                            2 => y += 1,
                            3 => y -= 1,
                            4 => z += 1,
                            _ => z -= 1,
                        },
                        // Flat lenses: spread wide, climb grudgingly.
                        crate::registry::VeinShape::Seam => match next() % 9 {
                            0 | 1 => x += 1,
                            2 | 3 => x -= 1,
                            4 | 5 => z += 1,
                            6 | 7 => z -= 1,
                            _ => y += if next() % 2 == 0 { 1 } else { -1 },
                        },
                        // Near-vertical streaks: climb hard, wander little.
                        crate::registry::VeinShape::Streak => match next() % 6 {
                            0..=2 => y += 1,
                            3 => y -= 1,
                            4 => x += if next() % 2 == 0 { 1 } else { -1 },
                            _ => z += if next() % 2 == 0 { 1 } else { -1 },
                        },
                    }
                }
            }
        }
    }

    fn height_hint(&self, heights: &[[i32; CHUNK_Z]; CHUNK_X], lx: usize, lz: usize) -> i32 {
        heights[lx][lz]
    }

    /// A subtle sub-voxel "dusting" of `surface_sand` over dry sand surfaces
    /// (deserts, beaches, sandy scrub). It reads as thin drifts that pool
    /// between ridges and thin out on high ground — a coherent low-frequency
    /// field sampled per half-cell sub-column (so edges fade over half-blocks
    /// rather than snapping per cell), never a full layer. Chunk-local.
    fn dust_sand(&self, c: &mut Chunk, pos: ChunkPos, heights: &[[i32; CHUNK_Z]; CHUNK_X]) {
        // Tunables (subtle by design): higher THRESHOLD = barer; the field is
        // ~[-0.7,0.7], so most of the biome stays clean.
        const FREQ: f64 = 0.05;
        const THRESHOLD: f64 = 0.28;
        const SECOND_LAYER: f64 = 0.42; // extra depth for the middle of a drift
        let bx = pos.x * CHUNK_X as i32;
        let bz = pos.z * CHUNK_Z as i32;
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                let h = heights[lx][lz];
                if h < 1 || (h + 1) as usize >= CHUNK_Y {
                    continue;
                }
                // Only dry sand surfaces, and only where nothing already sits on
                // top (skip water, plants, trunks placed by earlier passes).
                if c.get(lx, h as usize, lz) != self.sand || c.get(lx, (h + 1) as usize, lz) != AIR
                {
                    continue;
                }
                // Local hollowness (3x3 within the chunk): sand pools where the
                // column sits below its neighbours, thins on ridges.
                let mut sum = 0i32;
                let mut cnt = 0i32;
                for dx in -1..=1i32 {
                    for dz in -1..=1i32 {
                        let (nx, nz) = (lx as i32 + dx, lz as i32 + dz);
                        if (0..CHUNK_X as i32).contains(&nx) && (0..CHUNK_Z as i32).contains(&nz) {
                            sum += heights[nx as usize][nz as usize];
                            cnt += 1;
                        }
                    }
                }
                let hollow = (sum as f64 / cnt as f64 - h as f64).clamp(-2.0, 3.0) * 0.12;
                // Sample the drift field per half-cell sub-column for a soft edge.
                let mut mask = 0u8;
                for qz in 0..2u32 {
                    for qx in 0..2u32 {
                        let sx = (bx + lx as i32) as f64 + 0.5 * qx as f64 + 0.25;
                        let sz = (bz + lz as i32) as f64 + 0.5 * qz as f64 + 0.25;
                        let cover = self.detail.get([sx * FREQ, sz * FREQ]) + hollow - THRESHOLD;
                        if cover > 0.0 {
                            let col = (qz << 1) | qx;
                            mask |= 1 << col; // bottom octant
                            if cover > SECOND_LAYER {
                                mask |= 1 << (4 | col); // second layer in deep drifts
                            }
                        }
                    }
                }
                if mask != 0 {
                    c.set(lx, (h + 1) as usize, lz, self.surface_sand);
                    c.set_meta(lx, (h + 1) as usize, lz, mask);
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
                    Biome::Jungle => 22,
                    Biome::Taiga => 70,
                    Biome::Forest => 97,
                    Biome::Scrubland => 240,
                    Biome::Savanna => 150,
                    Biome::Swamp => 130,
                    Biome::Plains => 550,
                    Biome::Desert => 190, // cacti
                    Biome::Arctic | Biome::Mountains | Biome::Tundra | Biome::Badlands => 0,
                };
                // Jungle floor: undergrowth independent of trees —
                // lush, but no longer an endless buffet.
                if biome == Biome::Jungle {
                    let ur = hash2(self.seed ^ 0x0f01, wx, wz);
                    if ur.is_multiple_of(16) {
                        let h2 = self.height_hint(heights, lx, lz);
                        if h2 > SEA_LEVEL + 1
                            && h2 + 2 < CHUNK_Y as i32
                            && c.get(lx, h2 as usize, lz) == self.grass
                            && c.get(lx, (h2 + 1) as usize, lz) == AIR
                        {
                            let cover = if ur.is_multiple_of(48) {
                                self.mushroom
                            } else {
                                self.jungle_bush
                            };
                            c.set(lx, (h2 + 1) as usize, lz, cover);
                        }
                    }
                }
                // Wild food plants grow in scattered forage patches: a
                // coarse cell rolls for a patch, and only inside one do
                // columns roll for a plant (~1/384 overall, arriving as
                // clusters of a handful). Finding a berry patch or a
                // stand of wild wheat is a real find — and a seed
                // source — instead of groceries every few steps.
                let food_roll = hash2(self.seed ^ 0x5eed, wx, wz);
                let patch = hash2(self.seed ^ 0xf00d, wx >> 4, wz >> 4).is_multiple_of(8);
                if patch && food_roll.is_multiple_of(48) && biome != Biome::Desert {
                    let h2 = self.height_hint(heights, lx, lz);
                    let plant = match biome {
                        Biome::Plains | Biome::Savanna => self.wild_wheat,
                        Biome::Swamp => self.mushroom,
                        Biome::Forest => {
                            if food_roll.is_multiple_of(2) {
                                self.wild_carrot
                            } else {
                                self.berry_bush
                            }
                        }
                        Biome::Taiga => {
                            if food_roll.is_multiple_of(2) {
                                self.wild_potato
                            } else {
                                self.mushroom
                            }
                        }
                        Biome::Jungle => self.jungle_bush,
                        _ => AIR,
                    };
                    if plant != AIR
                        && h2 > SEA_LEVEL + 1
                        && h2 + 2 < CHUNK_Y as i32
                        && c.get(lx, h2 as usize, lz) == self.grass
                        && c.get(lx, (h2 + 1) as usize, lz) == AIR
                    {
                        c.set(lx, (h2 + 1) as usize, lz, plant);
                        continue;
                    }
                }
                if density == 0 || !hash2(self.seed, wx, wz).is_multiple_of(density) {
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
                    Biome::Scrubland | Biome::Savanna => (self.acacia_log, self.acacia_leaves),
                    Biome::Forest if rnd % 10 < 3 => (self.birch_log, self.birch_leaves),
                    _ => (self.log, self.leaves),
                };

                match biome {
                    Biome::Scrubland | Biome::Savanna => {
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
                        // Jungle canopy rides high — and one tree in
                        // seven is an emergent towering over the rest.
                        let trunk_h = if jungle {
                            if rnd.is_multiple_of(7) {
                                13 + (rnd % 4) as i32
                            } else {
                                8 + (rnd % 4) as i32
                            }
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
