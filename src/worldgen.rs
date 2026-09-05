//! Terrain v2: "Caves & Cliffs"-style generation (docs/terrain-v2-plan.md).
//!
//! Pipeline per chunk: 3D density shaping (lattice-sampled + trilinear
//! interpolation) -> cave carving (cheese + spaghetti) -> slope/altitude-aware
//! surface rules -> data-driven ores -> biome vegetation -> bedrock.

mod shape;
mod caves;
mod surface;
mod water;
mod minerals;
mod vegetation;
mod structures;
mod landmarks;
pub use landmarks::{heart_form, heart_height};

mod geography;
pub(crate) use geography::Geography;
mod queries;
mod hydrology;
mod prospecting;
mod density;
mod strata;
mod context;

use std::sync::Arc;

use noise::Perlin;

use crate::chunk::{CHUNK_X, CHUNK_Z, Chunk, ChunkPos};
use crate::planet::{Face, SurfacePos};
use crate::registry::{BlockId, Registry};

/// Atlas relief is already the authoritative large-scale surface. Keep the
/// noisy density skin from punching isolated shelves through its upper crust;
/// controlled cave carving runs afterward and may still open into cliffs.
const ATLAS_SURFACE_MANTLE: i32 = 24;
const ATLAS_CAVE_ROOF: f32 = 12.0;

/// Climate-derived biome identities. Production atlas worlds classify zonal
/// climate continuously and then apply local habitat overlays; the centroids
/// remain only for small atlas-free unit fixtures.
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
#[derive(Clone, Debug)]
/// What a prospecting strike reveals: each regional feature's rough
/// distance in blocks and the offset it was found at (for a compass
/// direction), or None past the pick's reach.
pub struct ProspectReading {
    pub province_name: Option<String>,
    pub bedrock: Option<crate::planet_atlas::BedrockFamily>,
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
    /// Whole-planet genesis shared by every authoritative chunk worker.
    /// Direct unit-test generators may omit it to exercise legacy fine-detail
    /// math without allocating a planet fixture.
    atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
    geography: Geography,
    base3d: [Perlin; 3],
    cheese: Perlin,
    spag1: Perlin,
    spag2: Perlin,
    seed: u32,
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
    halite: BlockId,
    kimberlite: BlockId,
    carbonatite: BlockId,
    /// Every interior rock (for cave carving and surface scans).
    rocks: [BlockId; 11],
    mud: BlockId,
    lava: BlockId,
    quartz_block: BlockId,
    amethyst_block: BlockId,
    granite3d: Perlin,
    rivernoise: Perlin,
    lakenoise: Perlin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GeodeBand {
    Shell,
    Lining,
    Heart,
}

pub(crate) fn geode_band_at(distance_squared: i32, radius: i32) -> Option<GeodeBand> {
    if distance_squared > radius * radius {
        None
    } else if distance_squared >= (radius - 1) * (radius - 1) {
        Some(GeodeBand::Shell)
    } else if distance_squared > (radius - 2) * (radius - 2) {
        Some(GeodeBand::Lining)
    } else {
        Some(GeodeBand::Heart)
    }
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
    /// The deterministic stage order is shared by every terrain worker.
    /// Each stage borrows this immutable generation context and owns its output.
    pub fn generate(&self, pos: ChunkPos, reg: &Registry) -> Chunk {
        // Dungeon runs stamp the Deep's rooms into pure void.
        if pos.face().is_deep() {
            return Chunk::new();
        }
        let shape::ShapedTerrain { chunk: mut c, columns } = self.shape(pos);
        self.carve(pos, &mut c, &columns.top);
        let surface = self.apply_surface(pos, &mut c, &columns);
        self.plant_pipe(&mut c, pos, &surface.heights);
        self.plant_geode(&mut c, pos);
        self.plant_ores(&mut c, pos, reg);
        self.plant_trees(&mut c, pos, &surface.heights, &surface.biomes);
        self.plant_province_structures(&mut c, pos, &surface.heights);
        self.finish_water(pos, &mut c, reg);
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                c.set(lx, 0, lz, self.bedrock);
            }
        }
        c.dirty = true;
        c.modified = false;
        c
    }

}
