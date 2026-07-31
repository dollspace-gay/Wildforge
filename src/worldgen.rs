//! Terrain v2: "Caves & Cliffs"-style generation (docs/terrain-v2-plan.md).
//!
//! Pipeline per chunk: 3D density shaping (lattice-sampled + trilinear
//! interpolation) -> cave carving (cheese + spaghetti) -> slope/altitude-aware
//! surface rules -> data-driven ores -> biome vegetation -> bedrock.

use std::collections::HashMap;

use noise::{NoiseFn, Perlin};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::planet::{
    Direction4, FACE_BLOCKS, Face, PLANET_RADIUS, SurfacePos, geodesic_distance, step4,
    surface_to_unit,
};
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
    /// Not a province culture: the label a submerged column wears. No
    /// centroid claims it and `classify` never returns it — the sea is
    /// a place you are, not a climate the land has.
    Ocean,
}

impl Biome {
    /// Round-trip for saves: 0 means "none", otherwise index + 1.
    pub fn from_index(i: u8) -> Option<Biome> {
        Some(match i {
            1 => Biome::Forest,
            2 => Biome::Plains,
            3 => Biome::Desert,
            4 => Biome::Jungle,
            5 => Biome::Scrubland,
            6 => Biome::Taiga,
            7 => Biome::Arctic,
            8 => Biome::Mountains,
            9 => Biome::Swamp,
            10 => Biome::Savanna,
            11 => Biome::Tundra,
            12 => Biome::Badlands,
            13 => Biome::Ocean,
            _ => return None,
        })
    }

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
            Biome::Ocean => "Ocean",
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
    pub pluton: Option<ProspectHit>,
    pub volcano: Option<ProspectHit>,
    pub pipe: Option<ProspectHit>,
    pub geode: Option<ProspectHit>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProspectHit {
    pub distance: i32,
    /// Radians clockwise from local north.
    pub bearing: Option<f64>,
}

#[cfg(test)]
pub struct Volcano {
    pub x: i32,
    pub z: i32,
    pub radius: f32,
    pub height: f32,
}

#[cfg(test)]
impl Volcano {
    pub fn dist(&self, wx: i32, wz: i32) -> f32 {
        (((wx - self.x).pow(2) + (wz - self.z).pow(2)) as f32).sqrt()
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

/// A country: one Voronoi cell of the province partition, its biome
/// decided once at its site. `edge` is the distance to the nearest
/// border; `neighbor` is what lies across it.
/// Stable finite country address. The grid lives on all six faces and its
/// offset operation canonicalizes through the planet's seam table.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProvinceKey {
    pub face: Face,
    pub u: u8,
    pub v: u8,
}

/// (label, site, temperature, humidity, erosion) — what a country is,
/// resolved once and cached.
type ProvinceLabel = (Biome, SurfacePos, f32, f32, f32);

#[derive(Clone, Copy, Debug)]
pub struct Province {
    /// Stable key — the heart of this country is keyed on it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub key: ProvinceKey,
    #[cfg_attr(not(test), allow(dead_code))]
    pub site: SurfacePos,
    /// The country's own label, read at its site: what to call it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub biome: Biome,
    pub neighbor: Biome,
    pub edge: f32,
    /// The zone the country imposes: temperature, humidity, and the
    /// worn-ness of its ground. Continentalness — how far from the
    /// sea a column sits — stays the column's own business, so a
    /// coast is still a coast inside a single country.
    pub t: f32,
    pub h: f32,
    pub e: f32,
    pub nt: f32,
    pub nh: f32,
    pub ne: f32,
}

pub struct Generator {
    /// Province label cache: classifying a country means sampling its
    /// site climate (tectonics included), and every column in it wants
    /// the same answer. Keyed by province, so the work happens once.
    province_cache: std::sync::RwLock<HashMap<ProvinceKey, ProvinceLabel>>,
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
    cattail: BlockId,
    kelp_frond: BlockId,
    water_lily: BlockId,
    lantern_fungus: BlockId,
    meadow_bloom: BlockId,
    ember_poppy: BlockId,
    /// Edifice materials per country, indexed by Biome. Resolved here
    /// for the same reason the heart blocks are: the generator has no
    /// registry to ask at generation time.
    edifice_mats: Vec<crate::edifice::Materials>,
    /// One heart block per country, indexed by Biome. Caching the
    /// three archetypes by name stopped working when every country
    /// grew its own: the lookup silently resolved to the placeholder
    /// and the generator laid down unknown blocks where the spirits
    /// should have stood, so nothing registered a heart at all.
    hearts: Vec<BlockId>,
    stone: BlockId,
    sand: BlockId,
    clay: BlockId,
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
    quartz_block: BlockId,
    amethyst_block: BlockId,
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
    #[inline]
    fn chunk_hash(&self, salt: u32, pos: ChunkPos) -> u32 {
        hash2(
            self.seed ^ salt ^ (pos.face() as u32).wrapping_mul(0x9e37_79b9),
            i32::from(pos.u()),
            i32::from(pos.v()),
        )
    }

    #[inline]
    fn surface_in_chunk(pos: ChunkPos, lx: i32, lz: i32) -> SurfacePos {
        SurfacePos::canonicalized(
            pos.face(),
            i32::from(pos.u()) * CHUNK_X as i32 + lx,
            i32::from(pos.v()) * CHUNK_Z as i32 + lz,
        )
        .expect("world-generation aprons cross at most one cube-face edge")
    }

    #[inline]
    fn noise_at(noise: &Perlin, pos: SurfacePos, scale: f64, offset: [f64; 3]) -> f32 {
        let p = surface_to_unit(pos.center()) * (PLANET_RADIUS / scale);
        noise.get([p.x + offset[0], p.y + offset[1], p.z + offset[2]]) as f32
    }

    #[inline]
    fn radial_noise_at(
        noise: &Perlin,
        pos: SurfacePos,
        y: f64,
        scale: f64,
        offset: [f64; 3],
    ) -> f32 {
        let p = surface_to_unit(pos.center()) * ((PLANET_RADIUS + y) / scale);
        noise.get([p.x + offset[0], p.y + offset[1], p.z + offset[2]]) as f32
    }

    fn hash_surface(&self, salt: u32, pos: SurfacePos) -> u32 {
        // Canonical face/cell identity means the same physical cell has one
        // roll even at seams. Adjacent cells remain decorrelated as intended.
        let a = ((pos.face() as u32) << 29) ^ (u32::from(pos.u()) << 13) ^ u32::from(pos.v());
        let mut h = self.seed ^ salt ^ a.wrapping_mul(0x9e37_79b9);
        h ^= h >> 16;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^ (h >> 13)
    }

    pub fn new(seed: u32, reg: &Registry) -> Generator {
        let b = |name: &str| reg.block_id(name).unwrap_or(AIR);
        let p = |k: u32| Perlin::new(seed.wrapping_add(k));
        Generator {
            province_cache: std::sync::RwLock::new(HashMap::new()),
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
            cattail: b("base:cattail"),
            kelp_frond: b("base:kelp_frond"),
            water_lily: b("base:water_lily"),
            lantern_fungus: b("base:lantern_fungus"),
            meadow_bloom: b("base:meadow_bloom"),
            ember_poppy: b("base:ember_poppy"),
            hearts: (1..=12)
                .filter_map(Biome::from_index)
                .map(|biome| b(crate::world::heart_form(biome)))
                .collect(),
            edifice_mats: (1..=12)
                .filter_map(Biome::from_index)
                .map(|biome| {
                    let e = crate::edifice::edifice_of(biome);
                    crate::edifice::Materials {
                        shell: b(e.shell),
                        crown: b(e.crown),
                    }
                })
                .collect(),
            stone: b("base:stone"),
            sand: b("base:sand"),
            clay: b("base:clay_block"),
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
            quartz_block: b("base:quartz_block"),
            amethyst_block: b("base:amethyst_block"),
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

    #[cfg(test)]
    const PLATE_SIZE: f64 = 1400.0;

    #[cfg(test)]
    fn plate_center(&self, px: i32, pz: i32) -> (f64, f64) {
        let h = hash2(self.seed ^ 0x91a7e, px, pz);
        (
            (px as f64 + 0.15 + ((h & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
            (pz as f64 + 0.15 + (((h >> 16) & 0xffff) as f64 / 65536.0) * 0.7) * Self::PLATE_SIZE,
        )
    }

    #[cfg(test)]
    fn plate_vel(&self, px: i32, pz: i32) -> (f32, f32) {
        let a = (hash2(self.seed ^ 0x7ec70, px, pz) % 6283) as f32 / 1000.0;
        (a.cos(), a.sin())
    }

    #[cfg(test)]
    fn plate_oceanic(&self, px: i32, pz: i32) -> bool {
        hash2(self.seed ^ 0x0c00, px, pz) % 10 < 4
    }

    /// The static plate map: jittered-grid Voronoi cells, each with a
    /// deterministic (conceptual) drift vector and crust kind. Nearest
    /// two centers give the boundary; the closing speed across it
    /// decides fold ranges, trenches, and rifts.
    #[cfg(test)]
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

    #[cfg(test)]
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
        // Slow fields: ~2500-block features. At the old 0.0026 the
        // climate turned over every ~385 blocks, which is what a
        // per-column classifier turned into confetti — and a province
        // needs to be small against its climate for its site to speak
        // for the whole country.
        let t = self.temperature.get([x * 0.0004, z * 0.0004]) as f32;
        let h = self.moisture.get([x * 0.0004 + 31.7, z * 0.0004 - 17.3]) as f32;
        Climate { t, h, c, e, r, tec }
    }

    /// Seam-safe planetary climate. Every field is sampled from the embedded
    /// unit direction; latitude supplies the broad temperature belt while
    /// low-frequency 3D noise breaks it into recognizable regions.
    pub fn climate_at(&self, pos: SurfacePos) -> Climate {
        let unit = surface_to_unit(pos.center());
        let c = Self::noise_at(&self.cont, pos, 1_650.0, [0.0, 0.0, 0.0]) * 1.15;
        let e = Self::noise_at(&self.ero, pos, 720.0, [13.5, -7.2, 4.1]);
        let ridge_raw = Self::noise_at(&self.ridge, pos, 410.0, [-3.3, 21.7, 8.9]);
        let r = 1.0 - (2.0 * ridge_raw.abs() - 1.0).abs();
        let lat_heat = 1.0 - 2.0 * unit.y.abs() as f32;
        let t = (lat_heat * 0.82
            + Self::noise_at(&self.temperature, pos, 2_300.0, [2.7, -4.9, 8.3]) * 0.34)
            .clamp(-1.0, 1.0);
        let h = (Self::noise_at(&self.moisture, pos, 1_900.0, [31.7, -17.3, 11.9])
            + Self::noise_at(&self.moisture, pos, 520.0, [-9.1, 6.4, 23.0]) * 0.28)
            .clamp(-1.0, 1.0);

        // A continuous stand-in for static plate readings. Zero crossings of
        // the folded ridge field are boundaries; a second vector field says
        // whether the two sides converge or part.
        let boundary_dist = ridge_raw.abs() * 760.0;
        let convergence = Self::noise_at(&self.detail, pos, 1_100.0, [47.0, -19.0, 5.0]) * 0.75;
        let along = Self::noise_at(&self.bandwarp, pos, 280.0, [3.0, 7.0, 13.0]) * 2_000.0;
        let own_c = Self::noise_at(&self.cont, pos, 1_650.0, [0.0, 0.0, 0.0]);
        let across = step4(pos, Direction4::East).pos;
        let neighbor_c = Self::noise_at(&self.cont, across, 1_650.0, [0.0, 0.0, 0.0]);
        let tec = Tectonics {
            boundary_dist,
            convergence,
            along,
            oceanic: own_c < -0.12,
            neighbor_oceanic: neighbor_c < -0.12,
        };
        Climate { t, h, c, e, r, tec }
    }

    /// Planetary biome classification used by generation and typed callers.
    pub fn biome_at(&self, pos: SurfacePos) -> Biome {
        let climate = self.climate_at(pos);
        if self.plate_relief(&climate) > 30.0 {
            Biome::Mountains
        } else if self.offset_base.at(climate.c) < SEA_LEVEL as f32 - 5.0 {
            Biome::Ocean
        } else {
            let province = self.province_at(pos);
            let (mut t, mut h, mut e) = (province.t, province.h, province.e);
            if province.neighbor != province.biome && province.edge < Self::PROVINCE_BLEND {
                let depth = (province.edge / Self::PROVINCE_BLEND).clamp(0.0, 1.0);
                let fringe = self.hash_surface(0x0051_f16e, pos) as f32 / u32::MAX as f32;
                if fringe > 0.5 + depth * 0.5 {
                    t = province.nt;
                    h = province.nh;
                    e = province.ne;
                }
            }
            self.classify(&Climate { t, h, e, ..climate })
        }
    }

    /// Provinces: the world's countries. A jittered-grid Voronoi
    /// partition (the plate trick at a smaller scale) whose climate is
    /// sampled ONCE at the site — so a province has one biome, not a
    /// per-column vote that flips a forest into a desert and back
    /// across a hundred blocks. This is the unit a place can be named
    /// by, and the territory a heart owns.
    pub const PROVINCE_CELLS: u8 = 9;
    /// Life fades across this fringe rather than ending at a line.
    const PROVINCE_BLEND: f32 = 70.0;

    fn province_key_at(pos: SurfacePos) -> ProvinceKey {
        let cells = u32::from(Self::PROVINCE_CELLS);
        ProvinceKey {
            face: pos.face(),
            u: ((u32::from(pos.u()) * cells) / u32::from(FACE_BLOCKS)).min(cells - 1) as u8,
            v: ((u32::from(pos.v()) * cells) / u32::from(FACE_BLOCKS)).min(cells - 1) as u8,
        }
    }

    fn province_nominal_center(face: Face, u: i32, v: i32) -> SurfacePos {
        let cells = i32::from(Self::PROVINCE_CELLS);
        let side = i32::from(FACE_BLOCKS);
        let center_u = ((u * 2 + 1) * side) / (cells * 2);
        let center_v = ((v * 2 + 1) * side) / (cells * 2);
        SurfacePos::canonicalized(face, center_u, center_v)
            .expect("a nearby province-grid center canonicalizes")
    }

    /// Walk the finite country grid through a face seam.
    pub fn province_offset(&self, key: ProvinceKey, du: i32, dv: i32) -> ProvinceKey {
        let pos =
            Self::province_nominal_center(key.face, i32::from(key.u) + du, i32::from(key.v) + dv);
        Self::province_key_at(pos)
    }

    fn province_site(&self, key: ProvinceKey) -> SurfacePos {
        let cells = u32::from(Self::PROVINCE_CELLS);
        let side = u32::from(FACE_BLOCKS);
        let lo_u = u32::from(key.u) * side / cells;
        let hi_u = (u32::from(key.u) + 1) * side / cells;
        let lo_v = u32::from(key.v) * side / cells;
        let hi_v = (u32::from(key.v) + 1) * side / cells;
        let packed = i32::from(key.u) | (i32::from(key.v) << 8);
        let h = hash2(
            self.seed ^ 0x9120_11ce ^ (key.face as u32).wrapping_mul(0x9e37_79b9),
            packed,
            i32::from(key.face as u8),
        );
        let jitter_u = 0.18 + (h & 0xffff) as f64 / 65536.0 * 0.64;
        let jitter_v = 0.18 + ((h >> 16) & 0xffff) as f64 / 65536.0 * 0.64;
        let u = f64::from(lo_u) + f64::from(hi_u - lo_u) * jitter_u;
        let v = f64::from(lo_v) + f64::from(hi_v - lo_v) * jitter_v;
        SurfacePos::new(
            key.face,
            u.floor().min(f64::from(FACE_BLOCKS - 1)) as u16,
            v.floor().min(f64::from(FACE_BLOCKS - 1)) as u16,
        )
        .expect("a jittered province site stays inside its canonical cell")
    }

    pub fn province_center_at(&self, key: ProvinceKey) -> SurfacePos {
        self.province_site(key)
    }

    /// The label and site of a province, computed once and kept.
    fn province_label(&self, key: ProvinceKey) -> ProvinceLabel {
        if let Some(hit) = self
            .province_cache
            .read()
            .ok()
            .and_then(|c| c.get(&key).copied())
        {
            return hit;
        }
        let site = self.province_site(key);
        let cl = self.climate_at(site);
        let biome = if self.plate_relief(&cl) > 30.0 {
            Biome::Mountains
        } else if self.offset_base.at(cl.c) < SEA_LEVEL as f32 - 5.0 {
            Biome::Ocean
        } else {
            self.classify(&cl)
        };
        let out = (biome, site, cl.t, cl.h, cl.e);
        if let Ok(mut c) = self.province_cache.write() {
            c.insert(key, out);
        }
        out
    }

    pub fn province_at(&self, pos: SurfacePos) -> Province {
        let home = Self::province_key_at(pos);
        let mut candidates = Vec::with_capacity(25);
        for du in -2..=2 {
            for dv in -2..=2 {
                let key = self.province_offset(home, du, dv);
                if !candidates.contains(&key) {
                    candidates.push(key);
                }
            }
        }
        let mut best = (f64::MAX, home);
        let mut second = (f64::MAX, home);
        for key in candidates {
            let site = self.province_site(key);
            let d = geodesic_distance(pos.center(), site.center());
            if d < best.0 {
                second = best;
                best = (d, key);
            } else if d < second.0 {
                second = (d, key);
            }
        }
        let edge = ((second.0 - best.0) * 0.5) as f32;
        let (biome, site, t, h, e) = self.province_label(best.1);
        // The neighbor only matters inside the border fringe; deep in
        // a country nobody asks who lives next door.
        let (neighbor, nt, nh, ne) = if edge < Self::PROVINCE_BLEND {
            let n = self.province_label(second.1);
            (n.0, n.2, n.3, n.4)
        } else {
            (biome, t, h, e)
        };
        Province {
            key: best.1,
            site,
            biome,
            neighbor,
            edge,
            t,
            h,
            e,
            nt,
            nh,
            ne,
        }
    }

    /// How close this column is to its country's heart, 0..1. Squared
    /// off so the thickening reads as a gradient you can walk up
    /// rather than a hard edge you cross.
    pub fn heart_nearness_at(&self, pos: SurfacePos) -> f32 {
        let p = self.province_at(pos);
        let site = self.province_center_at(p.key);
        let d = geodesic_distance(pos.center(), site.center()) as f32;
        // Readable from about a third of the way across a province,
        // which is roughly where you would give up and grid-search.
        const REACH: f32 = 300.0;
        (1.0 - (d / REACH).min(1.0)).powi(2)
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn province(&self, wx: i32, wz: i32) -> Province {
        let pos = SurfacePos::from_centered(Face::PosZ, wx, wz)
            .expect("test province query is inside the positive-Z face");
        self.province_at(pos)
    }

    /// The stone and trim a country builds with.
    fn edifice_materials(&self, biome: Biome) -> crate::edifice::Materials {
        self.edifice_mats
            .get(biome as usize)
            .copied()
            .unwrap_or(crate::edifice::Materials {
                shell: self.stone,
                crown: self.stone,
            })
    }

    /// The living heart block a country raises.
    pub(crate) fn heart_block(&self, biome: Biome) -> BlockId {
        self.hearts
            .get(biome as usize)
            .copied()
            .unwrap_or(self.stone)
    }

    #[cfg(test)]
    pub fn biome(&self, wx: i32, wz: i32) -> Biome {
        self.biome_from_at(wx, wz, &self.climate(wx, wz))
    }

    /// The biome a column reads as: its province's label, dithered
    /// with the neighbor's through the border fringe (so a forest
    /// thins into plains instead of ending at a line), with terrain
    /// keeping its local veto.
    #[cfg(test)]
    pub fn biome_from_at(&self, wx: i32, wz: i32, cl: &Climate) -> Biome {
        // A young fold range is Mountains whatever the country says.
        if self.plate_relief(cl) > 30.0 {
            return Biome::Mountains;
        }
        let p = self.province(wx, wz);
        // Culture from the country, terrain from the column: the
        // province fixes temperature and humidity across its whole
        // extent (that is what stops the confetti), while sea level
        // and relief stay local — so a coast is still a coast and a
        // basin is still a basin inside a single country.
        let (mut zt, mut zh, mut ze) = (p.t, p.h, p.e);
        if p.neighbor != p.biome && p.edge < Self::PROVINCE_BLEND {
            // Interleave the two zones across the fringe: near the
            // border it is a coin the noise flips, deep in it never is.
            let f = (p.edge / Self::PROVINCE_BLEND).clamp(0.0, 1.0);
            // Coarse enough that the fringe reads as fingers of one
            // country reaching into the other, not as static.
            let n = self.detail.get([wx as f64 / 55.0, wz as f64 / 55.0]) as f32;
            if n * 0.5 + 0.5 > 0.5 + f * 0.5 {
                zt = p.nt;
                zh = p.nh;
                ze = p.ne;
            }
        }
        self.classify(&Climate {
            t: zt,
            h: zh,
            e: ze,
            ..*cl
        })
    }

    /// Nearest-centroid classification of one climate sample. Provinces
    /// are labelled with this at their site; nothing else should call
    /// it per-column (that was the patchwork).
    fn classify(&self, cl: &Climate) -> Biome {
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
    pub(crate) fn plate_relief(&self, cl: &Climate) -> f32 {
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

    #[cfg(test)]
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

    fn base_offset_at(&self, pos: SurfacePos, cl: &Climate) -> f32 {
        let base = self.offset_base.at(cl.c);
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let mtn = self.mountain_amp.at(cl.e) * (0.35 + 0.65 * cl.r) * land * 0.45;
        let mut off = base + mtn + self.plate_relief(cl);
        if Self::is_badlands(cl) {
            let mesa = Self::noise_at(&self.detail, pos, 140.0, [0.0, 0.0, 0.0]);
            off += ((mesa * 3.0).floor().clamp(0.0, 2.0)) * 11.0;
        }
        off
    }

    /// Raw waterline math for one column, before sealing: the carve,
    /// the candidate water level, and whether the column sits close
    /// enough to a channel or lake basin that sealing must look at it
    /// (gates the neighbor probes — the margins cover the one-block
    /// noise gradient to the true water zones).
    #[cfg(test)]
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

    fn hydro_raw_at(&self, pos: SurfacePos, cl: &Climate, pre: f32) -> (f32, Option<i32>, bool) {
        let mut carve = 0.0f32;
        let mut level = None;
        let mut near = false;
        if pre > SEA_LEVEL as f32 - 2.0 && cl.c > -0.05 {
            let riv = Self::noise_at(&self.rivernoise, pos, 620.0, [0.0, 0.0, 0.0]);
            let width = 0.012 + 0.010 * (0.6 - cl.c).clamp(0.0, 1.0);
            let shoulder = width * 3.2;
            if riv.abs() < shoulder {
                near = true;
                let t = 1.0 - riv.abs() / shoulder;
                carve += t * t * 8.0;
                if riv.abs() < width {
                    carve += 3.0;
                    let floor = (pre - carve) as i32;
                    let fill = floor + 3;
                    let fill = fill - fill.rem_euclid(4);
                    if fill > floor {
                        level = Some(fill);
                    }
                }
            }
            let lake = Self::noise_at(&self.lakenoise, pos, 300.0, [0.0, 0.0, 0.0]);
            if lake > 0.56 {
                near = true;
            }
            if lake > 0.58 && pre > SEA_LEVEL as f32 + 2.0 && pre < 120.0 {
                let t = ((lake - 0.58) / 0.42).min(1.0);
                carve += t * 10.0;
                let fill = (pre - 2.0) as i32;
                let fill = fill - fill.rem_euclid(4);
                level = Some(level.map_or(fill, |old: i32| old.max(fill)));
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
    #[cfg(test)]
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

    fn hydrology_at(
        &self,
        pos: SurfacePos,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, Option<i32>) {
        let (carve, level, near) = self.hydro_raw_at(pos, cl, pre);
        if !near {
            return (carve, None, None);
        }
        let mut step_down = false;
        let mut tallest = None;
        for direction in [
            Direction4::East,
            Direction4::North,
            Direction4::West,
            Direction4::South,
        ] {
            let neighbor = step4(pos, direction).pos;
            let ncl = self.climate_at(neighbor);
            let npre = self.base_offset_at(neighbor, &ncl);
            let (_, nlevel, _) = self.hydro_raw_at(neighbor, &ncl, npre);
            match (nlevel, level) {
                (Some(nf), Some(f)) if nf < f => step_down = true,
                (Some(nf), None) => tallest = Some(tallest.map_or(nf, |t: i32| t.max(nf))),
                _ => {}
            }
        }
        match level {
            Some(fill) if step_down => (carve, None, Some(fill)),
            Some(fill) => (carve, Some(fill), None),
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

    #[cfg(test)]
    pub fn water_features_at(&self, pos: SurfacePos) -> Option<i32> {
        let climate = self.climate_at(pos);
        let pre = self.base_offset_at(pos, &climate);
        self.hydrology_at(pos, &climate, pre).1
    }

    /// Does a granite pluton intrude this column at mineable depth?
    /// Mirrors sample_lattice's threshold math. The census measures
    /// with it; the prospecting pick reads with it.
    pub fn pluton_at_surface(&self, pos: SurfacePos) -> bool {
        let prov = Self::radial_noise_at(&self.granite3d, pos, 77.7, 1_400.0, [0.0; 3]);
        let prov_pen = (0.44 - prov).max(0.0) * 1.8;
        for y in [16.0f64, 32.0, 48.0, 64.0] {
            let g = Self::radial_noise_at(&self.granite3d, pos, y, 230.0, [0.0; 3]);
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
    pub fn prospect_at(&self, pos: SurfacePos) -> ProspectReading {
        let reading = |target: SurfacePos| ProspectHit {
            distance: geodesic_distance(pos.center(), target.center()).round() as i32,
            bearing: crate::planet::great_circle_bearing(pos.center(), target.center()),
        };
        let ring = |step: i32, cap: i32, hit: &dyn Fn(SurfacePos) -> bool| {
            if hit(pos) {
                return Some(reading(pos));
            }
            let mut r = step;
            while r <= cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        if let Ok(target) = SurfacePos::canonicalized(
                            pos.face(),
                            i32::from(pos.u()) + dx,
                            i32::from(pos.v()) + dz,
                        ) && hit(target)
                        {
                            return Some(reading(target));
                        }
                    }
                    i += step;
                }
                r += step;
            }
            None
        };
        let cp = ChunkPos::from_surface(pos);
        let chunk_ring = |cap: i32, hit: &dyn Fn(ChunkPos) -> bool| {
            if hit(cp) {
                return Some(reading(pos));
            }
            for r in 1..=cap {
                let mut i = -r;
                while i <= r {
                    for (dx, dz) in [(i, -r), (i, r), (-r, i), (r, i)] {
                        let target_chunk = cp.offset(dx, dz);
                        if hit(target_chunk) {
                            let target = SurfacePos::new(
                                target_chunk.face(),
                                target_chunk.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
                                target_chunk.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
                            )
                            .expect("chunk center is canonical");
                            return Some(reading(target));
                        }
                    }
                    i += 1;
                }
            }
            None
        };
        ProspectReading {
            pluton: ring(64, 1216, &|surface| self.pluton_at_surface(surface)),
            // Goal-1's temporary planetary generator does not stamp the old
            // planar volcano regions.
            volcano: None,
            pipe: chunk_ring(24, &|p| self.pipe_at(p).is_some()),
            geode: chunk_ring(12, &|p| self.geode_at(p).is_some()),
        }
    }

    #[cfg(test)]
    pub fn pluton_at(&self, wx: i32, wz: i32) -> bool {
        SurfacePos::from_centered(Face::PosZ, wx, wz).is_ok_and(|pos| self.pluton_at_surface(pos))
    }

    /// The armor level sealing a column, if any (tests and tooling).
    #[cfg(test)]
    pub fn armor_at(&self, wx: i32, wz: i32) -> Option<i32> {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        self.hydrology(wx, wz, &cl, pre).2
    }

    #[cfg(test)]
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

    fn column_params_at(&self, pos: SurfacePos) -> (f32, f32) {
        let cl = self.climate_at(pos);
        let pre = self.base_offset_at(pos, &cl);
        let (carve, _, _) = self.hydro_raw_at(pos, &cl, pre);
        (
            (pre - carve).clamp(6.0, CHUNK_Y as f32 - 22.0),
            self.factor_spline.at(cl.e),
        )
    }

    /// The volcano whose reach covers a column, if any: deterministic
    /// per region cell, so every chunk agrees without communication.
    /// Land and coastal shelves only — volcanic islands are welcome,
    /// the deep ocean floor is not.
    #[cfg(test)]
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
    #[cfg(test)]
    pub fn surface_estimate(&self, wx: i32, wz: i32) -> i32 {
        self.column_params(wx, wz).0 as i32
    }

    /// Seam-safe cheap surface estimate for planetary spawn search and
    /// diagnostics.
    pub fn surface_estimate_at(&self, pos: SurfacePos) -> i32 {
        self.column_params_at(pos).0 as i32
    }

    fn density_at_planet(&self, pos: SurfacePos, y: f64, offset: f32, factor: f32) -> f32 {
        let mut noise = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        for octave in &self.base3d {
            noise += f64::from(Self::radial_noise_at(
                octave,
                pos,
                y,
                171.0 / frequency,
                [0.0, 0.0, 0.0],
            )) * amplitude;
            frequency *= 2.0;
            amplitude *= 0.5;
        }
        let noise = (noise / 1.75) as f32;
        let dy = offset - y as f32;
        let slope = if dy < 0.0 {
            factor * 0.011
        } else {
            factor.max(3.0) * 0.026
        };
        noise * 0.62 + dy * slope
    }

    /// Sample density on a 4x8x4 lattice covering the chunk plus a 4-block
    /// apron, so border columns interpolate identically to their neighbors.
    /// The second channel is the granite intrusion margin: distance past
    /// the (depth-loosening) pluton threshold, baked in so interpolation
    /// carries the widening-with-depth shape for free.
    fn sample_lattice(&self, pos: ChunkPos) -> (Vec<f32>, Vec<f32>) {
        const NX: usize = 7; // x/z: -4, 0, 4, 8, 12, 16, 20
        const NY: usize = CHUNK_Y / 8 + 1;
        let mut lat = vec![0f32; NX * NX * NY];
        let mut lat_g = vec![0f32; NX * NX * NY];
        for ix in 0..NX {
            for iz in 0..NX {
                let surface = Self::surface_in_chunk(pos, ix as i32 * 4 - 4, iz as i32 * 4 - 4);
                let (offset, factor) = self.column_params_at(surface);
                // Batholith provinces: a coarse gate over the pluton
                // noise. Inside a province intrusions abound; outside,
                // the threshold climbs out of reach — granite country
                // is a REGION you travel to (economy plan, leg 1),
                // not a backyard given.
                let prov = Self::radial_noise_at(&self.granite3d, surface, 77.7, 1_400.0, [0.0; 3]);
                let prov_pen = (0.44 - prov).max(0.0) * 1.8;
                for iy in 0..NY {
                    let y = (iy * 8) as f64;
                    let i = (ix * NX + iz) * NY + iy;
                    lat[i] = self.density_at_planet(surface, y, offset, factor);
                    let g = Self::radial_noise_at(&self.granite3d, surface, y, 230.0, [0.0; 3]);
                    // Plutons widen downward: the threshold tightens
                    // with altitude, so intrusions taper as they rise.
                    let thr = 0.55 + prov_pen + y as f32 * 0.0012;
                    lat_g[i] = g - thr;
                }
            }
        }
        (lat, lat_g)
    }

    fn strata_bands_at(&self, pos: SurfacePos, cl: &Climate) -> [i32; 5] {
        let w1 = Self::noise_at(&self.bandwarp, pos, 260.0, [0.0, 0.0, 0.0]);
        let w2 = Self::noise_at(&self.bandwarp, pos, 170.0, [7.3, -2.1, 4.7]);
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
            (50.0 + w2 * 5.0 + cl.h * 5.0 + fold) as i32,
            (68.0 + w1 * 6.0 + fold) as i32,
            (92.0 + w2 * 9.0 - cl.h * 6.0 + fold + mesa) as i32,
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
                let surface = Self::surface_in_chunk(pos, lx, lz);
                // One climate read serves bands, hydrology, and rock.
                let cl = self.climate_at(surface);
                let bands = self.strata_bands_at(surface, &cl);
                // The goal-1 generator keeps the complete rock stack but
                // leaves landmark placement to the later spherical feature
                // pass; a planar volcano region may never bleed across a face.
                let vol = 0.0;
                let dike = false;
                // Rivers and lakes flood their carve as a local sea.
                let pre = self.base_offset_at(surface, &cl);
                let (_, fill, armor) = self.hydrology_at(surface, &cl, pre);
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
                let surface = Self::surface_in_chunk(pos, lx, lz);
                let top = shape_top[(lx + 1) as usize][(lz + 1) as usize];
                for y in 5..top.min(CHUNK_Y as i32 - 1) {
                    if !self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        continue;
                    }
                    let depth = (top - y).max(0) as f32;
                    let yf = y as f64;
                    // Cheese: big voids, more common deeper down.
                    let ch = Self::radial_noise_at(&self.cheese, surface, yf, 120.0, [0.0; 3]);
                    let cheese_thr = 0.74 - (SEA_LEVEL as f32 - y as f32).clamp(0.0, 50.0) * 0.004;
                    // Spaghetti: two noises near zero = a winding tunnel.
                    // Width tapers near the surface so entrances are rare.
                    let taper = (depth / 12.0).min(1.0);
                    let w = (0.055 + depth * 0.0003) * taper;
                    let s1 = Self::radial_noise_at(&self.spag1, surface, yf, 70.0, [0.0; 3]);
                    let s2 =
                        Self::radial_noise_at(&self.spag2, surface, yf, 70.0, [41.0, 0.0, -13.0]);
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
                let surface = Self::surface_in_chunk(pos, lx as i32, lz as i32);
                let biome = self.biome_at(surface);
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
                // Standing water needs somewhere to stand: a column
                // whose neighbors all sit at or above it. On a
                // shoulder a "pool" is just a spring, and it pours
                // downhill forever — which is exactly what a whole
                // swamp province turned into before this rule.
                let basin = [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)]
                    .iter()
                    .all(|&(dx, dz)| {
                        shape_top[(lx as i32 + 1 + dx) as usize][(lz as i32 + 1 + dz) as usize]
                            >= h0
                    });
                let underwater = top < fills[lx][lz].max(SEA_LEVEL) - 1;
                let snowcap = top >= 170
                    || (biome == Biome::Mountains
                        && top >= 150
                        && Self::noise_at(&self.detail, surface, 9.0, [0.0; 3]) > -0.2);

                let scrub_sandy = Self::noise_at(&self.detail, surface, 33.0, [0.0; 3]) > 0.15;
                // None = leave the natural rock exposed (bare mountains,
                // steep faces — the strata read in the cliffs).
                let (top_b, under_b): (Option<BlockId>, Option<BlockId>) = if underwater {
                    if top < SEA_LEVEL - 14 {
                        (Some(self.gravel), Some(self.gravel))
                    } else if Self::noise_at(&self.detail, surface, 23.0, [0.0; 3]) > 0.34 {
                        // Clay beds: patches where still shallows let
                        // the fine sediment settle (wild arc, stage 5
                        // — the crock starts here).
                        (Some(self.clay), Some(self.clay))
                    } else {
                        (Some(self.sand), Some(self.sand))
                    }
                } else if snowcap {
                    (Some(self.snow), None)
                } else if biome == Biome::Mountains || steep {
                    // Bare rock: mountains, cliffs, volcano flanks.
                    (None, None)
                } else {
                    let beach = top <= SEA_LEVEL + 1;
                    let patch = Self::noise_at(&self.detail, surface, 9.0, [0.0; 3]);
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
                        Biome::Swamp if patch > 0.34 && !beach && basin => {
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
                    && self.hash_surface(0x59a1, surface).is_multiple_of(211)
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

        self.plant_pipe(&mut c, pos, &heights);
        self.plant_geode(&mut c, pos);
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

    /// The kimberlite pipe rolled for a chunk, if any: (local cx, cz,
    /// breaches_surface). Roughly one chunk in four hundred; the pipe
    /// fits inside its chunk's footprint by construction.
    pub fn pipe_at(&self, pos: ChunkPos) -> Option<(usize, usize, bool)> {
        let h = self.chunk_hash(0x8d1a, pos);
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
        let h = self.chunk_hash(0x8d1a, pos);
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
                        && hash2(
                            self.seed ^ 0xb1e ^ (pos.face() as u32).wrapping_mul(0x9e37_79b9),
                            pos.u() as i32 * 16 + lx,
                            pos.v() as i32 * 16 + lz,
                        )
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
        let h = self.chunk_hash(0x6e0d, pos);
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
        let h = self.chunk_hash(0x6e0d, pos);
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
            let mut rng = self.chunk_hash((fi as u32).wrapping_mul(0x9e37), pos);
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

    fn plant_trees(
        &self,
        c: &mut Chunk,
        pos: ChunkPos,
        heights: &[[i32; CHUNK_Z]; CHUNK_X],
        biomes: &[[Biome; CHUNK_Z]; CHUNK_X],
    ) {
        for lx in 2..CHUNK_X - 2 {
            for lz in 2..CHUNK_Z - 2 {
                let surface = Self::surface_in_chunk(pos, lx as i32, lz as i32);
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
                    Biome::Arctic
                    | Biome::Mountains
                    | Biome::Tundra
                    | Biome::Badlands
                    | Biome::Ocean => 0,
                };
                // Jungle floor: undergrowth independent of trees —
                // lush, but no longer an endless buffet.
                if biome == Biome::Jungle {
                    let ur = self.hash_surface(0x0f01, surface);
                    // This bush bears real food. Keep the jungle visually
                    // tree-dense without turning its floor into a free
                    // orchard that makes farming irrelevant.
                    if ur.is_multiple_of(384) {
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
                let food_roll = self.hash_surface(0x5eed, surface);
                let patch = self.chunk_hash(0xf00d, pos).is_multiple_of(8);
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
                // The deep grows its own light: lantern fungus takes
                // root in cave pockets, the underground's first
                // native lamp.
                {
                    let cr = self.hash_surface(0xca9e, surface);
                    if cr.is_multiple_of(20) {
                        let start = (8 + (cr >> 8) % 30) as i32;
                        if let Some(fy) = (start..(start + 12).min(44)).find(|&fy| {
                            c.get(lx, fy as usize, lz) == AIR
                                && c.get(lx, (fy + 1) as usize, lz) == AIR
                                && {
                                    let floor = c.get(lx, (fy - 1) as usize, lz);
                                    floor != AIR && floor != self.water
                                }
                        }) {
                            c.set(lx, fy as usize, lz, self.lantern_fungus);
                        }
                    }
                }
                // Meadow flowers: scattered, useless, and worth it —
                // and they thicken toward a country's heart, which is
                // the only navigation the game offers between one
                // province and the next. No compass, no marker: the
                // ground gets busier and you follow it. Every biome
                // shows it, not just the flowery ones, because it is a
                // signal before it is decoration.
                {
                    let fr = self.hash_surface(0xf10e, surface);
                    let flowery = matches!(biome, Biome::Plains | Biome::Forest | Biome::Savanna);
                    let rate = match self.heart_nearness_at(surface) {
                        n if n > 0.82 => 6, // you are all but on it
                        n if n > 0.60 => 22,
                        n if n > 0.35 => 90,
                        n if n > 0.12 => 220,
                        _ if flowery => 340, // ordinary meadow
                        _ => 0,              // ordinary elsewhere: none
                    };
                    if rate > 0 && fr.is_multiple_of(rate) {
                        let h2 = self.height_hint(heights, lx, lz);
                        if h2 > SEA_LEVEL + 1
                            && h2 + 2 < CHUNK_Y as i32
                            && c.get(lx, h2 as usize, lz) == self.grass
                            && c.get(lx, (h2 + 1) as usize, lz) == AIR
                        {
                            let f = if fr.is_multiple_of(2) {
                                self.meadow_bloom
                            } else {
                                self.ember_poppy
                            };
                            c.set(lx, (h2 + 1) as usize, lz, f);
                        }
                    }
                }
                // The waterline flora: cattails stand where the land
                // meets the water table, lilies float on swamp glass,
                // kelp sways in the deeper cold.
                {
                    let h2 = self.height_hint(heights, lx, lz);
                    let wr = self.hash_surface(0x77a7, surface);
                    let shore = (SEA_LEVEL - 1..=SEA_LEVEL + 1).contains(&h2);
                    let reed_odds = if biome == Biome::Swamp { 5 } else { 14 };
                    if shore
                        && wr.is_multiple_of(reed_odds)
                        && c.get(lx, h2 as usize, lz) == self.grass
                        && c.get(lx, (h2 + 1) as usize, lz) == AIR
                    {
                        c.set(lx, (h2 + 1) as usize, lz, self.cattail);
                    }
                    if h2 < SEA_LEVEL - 2 {
                        // Underwater ground with real depth above it.
                        if biome == Biome::Swamp
                            && wr.is_multiple_of(10)
                            && (SEA_LEVEL + 1) < CHUNK_Y as i32
                            && c.get(lx, (SEA_LEVEL + 1) as usize, lz) == AIR
                        {
                            c.set(lx, (SEA_LEVEL + 1) as usize, lz, self.water_lily);
                        } else if wr.is_multiple_of(9) {
                            let fronds = 1 + (wr >> 8) % 3;
                            for dy in 1..=fronds as i32 {
                                let y = h2 + dy;
                                if y < SEA_LEVEL && c.get(lx, y as usize, lz) == self.water {
                                    c.set(lx, y as usize, lz, self.kelp_frond);
                                }
                            }
                        }
                    }
                }
                if density == 0 || !self.hash_surface(0, surface).is_multiple_of(density) {
                    continue;
                }
                let h = heights[lx][lz];
                if h <= SEA_LEVEL + 1 || h + 11 >= CHUNK_Y as i32 {
                    continue;
                }
                let surface_block = c.get(lx, h as usize, lz);
                let rnd = self.hash_surface(0xabcd, surface);

                if biome == Biome::Desert {
                    if surface_block == self.sand {
                        let ch = 2 + (rnd % 2) as i32;
                        for y in 1..=ch {
                            c.set(lx, (h + y) as usize, lz, self.cactus);
                        }
                    }
                    continue;
                }
                if surface_block != self.grass {
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

        // The heart of a country stands at its province's center. A
        // site sits inside its own province by construction, so at
        // most a few keys can land in any one chunk.
        {
            let center = Self::surface_in_chunk(pos, CHUNK_X as i32 / 2, CHUNK_Z as i32 / 2);
            let home = self.province_at(center).key;
            let mut seen = Vec::new();
            for du in -2..=2 {
                for dv in -2..=2 {
                    let key = self.province_offset(home, du, dv);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    let site = self.province_center_at(key);
                    if ChunkPos::from_surface(site) != pos {
                        continue;
                    }
                    let lx = usize::from(site.u() % CHUNK_X as u16);
                    let lz = usize::from(site.v() % CHUNK_Z as u16);
                    let ground = heights[lx][lz];
                    // A site wants dry, standable ground; a country
                    // whose center drowns keeps its heart unbuilt, and
                    // the world reads such country as living.
                    if ground <= SEA_LEVEL || ground + 8 >= CHUNK_Y as i32 {
                        continue;
                    }
                    let biome = self.province_at(site).biome;
                    if biome == Biome::Ocean {
                        continue;
                    }
                    let form = crate::world::heart_form(biome);
                    let block = self.heart_block(biome);
                    let tall = crate::world::heart_height(form);
                    for dy in 1..=tall {
                        c.set(lx, (ground + dy) as usize, lz, block);
                    }
                }
            }
        }

        // And the edifice over it. A monument spans several chunks and
        // a chunk cannot write into its neighbours, so this is not one
        // chunk stamping a shape: every chunk asks what belongs in its
        // OWN columns and they agree at the seams by construction.
        // The site's ground therefore has to come from a function of
        // position alone, not from this chunk's carved heightmap.
        {
            let center = Self::surface_in_chunk(pos, CHUNK_X as i32 / 2, CHUNK_Z as i32 / 2);
            let home = self.province_at(center).key;
            let mut seen = Vec::new();
            for du in -2..=2 {
                for dv in -2..=2 {
                    let key = self.province_offset(home, du, dv);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    let site = self.province_center_at(key);
                    let biome = self.province_at(site).biome;
                    if biome == Biome::Ocean {
                        continue;
                    }
                    let ed = crate::edifice::edifice_of(biome);
                    if geodesic_distance(center.center(), site.center())
                        > f64::from(ed.reach) + 24.0
                    {
                        continue;
                    }
                    let base = self.surface_estimate_at(site);
                    if base <= SEA_LEVEL || base + ed.rise + 4 >= CHUNK_Y as i32 {
                        continue;
                    }
                    let mats = self.edifice_materials(biome);
                    let site_entity = crate::planet::EntityPos::new(
                        site.face(),
                        f32::from(site.u()) + 0.5,
                        0.0,
                        f32::from(site.v()) + 0.5,
                    )
                    .expect("province site is canonical");
                    for lx in 0..CHUNK_X as i32 {
                        for lz in 0..CHUNK_Z as i32 {
                            let column = Self::surface_in_chunk(pos, lx, lz);
                            let column_entity = crate::planet::EntityPos::new(
                                column.face(),
                                f32::from(column.u()) + 0.5,
                                0.0,
                                f32::from(column.v()) + 0.5,
                            )
                            .expect("chunk column is canonical");
                            let delta = site_entity.local_delta_to(column_entity);
                            let (dx, dz) = (delta.x.round() as i32, delta.z.round() as i32);
                            if dx.abs() > ed.reach || dz.abs() > ed.reach {
                                continue;
                            }
                            // Footings run a little below the estimate so
                            // the mass never floats over carved ground.
                            for dy in -4..=ed.rise {
                                let y = base + dy;
                                if !(1..CHUNK_Y as i32).contains(&y) {
                                    continue;
                                }
                                if let Some(b) = crate::edifice::block_at(&ed, &mats, dx, dy, dz) {
                                    c.set(lx as usize, y as usize, lz as usize, b);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
