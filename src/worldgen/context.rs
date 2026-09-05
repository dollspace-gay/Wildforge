//! Seeded immutable noise, content bindings, and coordinate sampling.

use super::{Biome, Generator, Spline};
use std::sync::Arc;
use noise::Perlin;
use crate::chunk::{CHUNK_X, CHUNK_Z, ChunkPos};
use crate::planet::SurfacePos;
use crate::registry::{AIR, BlockId, Registry};

impl Generator {
    #[inline]
    pub(super) fn chunk_hash(&self, salt: u32, pos: ChunkPos) -> u32 {
        self.geography.chunk_hash(salt, pos)
    }

    #[inline]
    pub(super) fn surface_in_chunk(pos: ChunkPos, lx: i32, lz: i32) -> SurfacePos {
        SurfacePos::canonicalized(
            pos.face(),
            i32::from(pos.u()) * CHUNK_X as i32 + lx,
            i32::from(pos.v()) * CHUNK_Z as i32 + lz,
        )
        .expect("world-generation aprons cross at most one cube-face edge")
    }

    #[inline]
    pub(super) fn noise_at(noise: &Perlin, pos: SurfacePos, scale: f64, offset: [f64; 3]) -> f32 {
        crate::climate::surface_noise(noise, pos, scale, offset)
    }

    #[inline]
    pub(super) fn radial_noise_at(
        noise: &Perlin,
        pos: SurfacePos,
        y: f64,
        scale: f64,
        offset: [f64; 3],
    ) -> f32 {
        crate::climate::radial_noise(noise, pos, y, scale, offset)
    }

    pub(super) fn hash_surface(&self, salt: u32, pos: SurfacePos) -> u32 {
        crate::climate::surface_hash(self.seed, salt, pos)
    }

    pub fn new(seed: u32, reg: &Registry) -> Generator {
        let b = |name: &str| reg.block_id(name).unwrap_or(AIR);
        let p = |k: u32| Perlin::new(seed.wrapping_add(k));
        Generator {
            atlas: None,
            geography: super::Geography::new(seed, None),
            base3d: [p(10), p(11), p(12)],
            cheese: p(30),
            spag1: p(31),
            spag2: p(32),
            seed,
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
                .map(|biome| b(super::heart_form(biome)))
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
            halite: b("base:halite"),
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
            rivernoise: p(50),
            lakenoise: p(51),
        }
    }

    pub fn with_atlas(
        seed: u32,
        reg: &Registry,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> Generator {
        let mut generator = Self::new(seed, reg);
        generator.geography = super::Geography::new(seed, Some(Arc::clone(&atlas)));
        generator.atlas = Some(atlas);
        generator
    }

    pub fn has_planet_atlas(&self) -> bool {
        self.atlas.is_some()
    }

    pub fn graft_compatibility_at(
        &self,
        pos: SurfacePos,
        target: Biome,
    ) -> crate::planet_atlas::GraftCompatibility {
        self.atlas.as_ref().map_or(
            crate::planet_atlas::GraftCompatibility::Compatible,
            |atlas| atlas.graft_compatibility_at(pos, target as u8 + 1),
        )
    }

    /// Is this block one of the interior rock family? (What caves may
    /// carve and surface scans read through.)
    pub(super) fn is_rock(&self, b: BlockId) -> bool {
        b != AIR && self.rocks.contains(&b)
    }

}
