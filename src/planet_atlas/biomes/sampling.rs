//! Read-only biome, country, and graft compatibility queries.

use crate::planet::{SurfacePos};
use crate::planet_atlas::{HYDRO_FLOODPLAIN, HYDRO_RIVER, PlanetAtlas};
use super::{BIOME_ARCTIC, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS, BIOME_SAVANNA, BIOME_SCRUBLAND, BIOME_TAIGA, BIOME_TUNDRA, CountryRecord, HABITAT_OASIS, HABITAT_RIPARIAN};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasBiomeSample {
    pub zonal_biome: u8,
    pub edaphic_flags: u16,
    pub habitat_flags: u32,
    pub vegetation_potential: u8,
    pub tree_line_y: u8,
    pub succession_potential: u8,
    pub country_id: u16,
    pub soil_depth_decimeters: u8,
    pub sand: u8,
    pub silt: u8,
    pub clay: u8,
    pub organic: u8,
    pub fertility: u8,
    pub drainage: u8,
    pub salinity: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraftCompatibility {
    Compatible,
    Marginal,
    Incompatible,
}

impl PlanetAtlas {
    pub fn biome_sample(&self, surface: SurfacePos) -> AtlasBiomeSample {
        let pos = self.atlas_pos(surface);
        let biome = self.genesis.biomes.get(pos).expect("validated biome query");
        let hydrology = self
            .genesis
            .hydrology
            .get(pos)
            .expect("validated hydrology query");
        let ground = self
            .genesis
            .ground
            .get(pos)
            .expect("validated ground query");
        let mut habitat_flags = biome.habitat_flags;
        // Atlas algorithm 6 admitted any drainage cell with more than four
        // discharge units as riparian. Most land meets that bar, including
        // cells with no channel at all. Filter that legacy derived flag at
        // the query boundary so existing planets and newly generated ones
        // expose the same physically backed habitat contract.
        if hydrology.flags & (HYDRO_RIVER | HYDRO_FLOODPLAIN) == 0 {
            habitat_flags &= !HABITAT_RIPARIAN;
        }
        if habitat_flags & HABITAT_OASIS != 0 {
            let current_head = self.water_cycle.cells.values()[pos.index(self.side())]
                .groundwater_head_milliblocks;
            let baseline_head = (ground.baseline_groundwater_head * 1000.0).round() as i32;
            if current_head < baseline_head - 1_500 {
                habitat_flags &= !HABITAT_OASIS;
            }
        }
        AtlasBiomeSample {
            zonal_biome: biome.baseline_biome,
            edaphic_flags: biome.edaphic_flags,
            habitat_flags,
            vegetation_potential: biome.vegetation_potential,
            tree_line_y: biome.tree_line_y,
            succession_potential: biome.succession_potential,
            country_id: biome.country_id,
            soil_depth_decimeters: ground.soil_depth_decimeters,
            sand: ground.sand,
            silt: ground.silt,
            clay: 255u8
                .saturating_sub(ground.sand)
                .saturating_sub(ground.silt),
            organic: ground.organic,
            fertility: ground.baseline_fertility,
            drainage: ground.drainage,
            salinity: ground.soil_salinity,
        }
    }

    pub fn country_at(&self, surface: SurfacePos) -> Option<&CountryRecord> {
        self.biomes.country(self.biome_sample(surface).country_id)
    }

    pub fn country(&self, id: u16) -> Option<&CountryRecord> {
        self.biomes.country(id)
    }

    pub fn graft_compatibility_at(
        &self,
        surface: SurfacePos,
        target_biome: u8,
    ) -> GraftCompatibility {
        let pos = self.atlas_pos(surface);
        let climate = self.genesis.climate.values()[pos.index(self.side())];
        let local = self.genesis.biomes.values()[pos.index(self.side())].baseline_biome;
        if target_biome == local {
            return GraftCompatibility::Compatible;
        }
        let tolerances = |biome| match biome {
            BIOME_JUNGLE => (18.0, 1_250.0),
            BIOME_FOREST => (5.0, 650.0),
            BIOME_TAIGA => (-8.0, 420.0),
            BIOME_TUNDRA => (-18.0, 180.0),
            BIOME_ARCTIC => (-35.0, 80.0),
            BIOME_DESERT | BIOME_BADLANDS => (10.0, 80.0),
            BIOME_SAVANNA | BIOME_SCRUBLAND => (8.0, 300.0),
            BIOME_MOUNTAINS => (-15.0, 180.0),
            _ => (0.0, 400.0),
        };
        let (minimum_temperature, minimum_precipitation) = tolerances(target_biome);
        let thermal_gap = (minimum_temperature - climate.mean_temperature).max(0.0);
        let water_gap = (minimum_precipitation - climate.mean_precipitation).max(0.0);
        let gross_opposite = matches!(target_biome, BIOME_JUNGLE | BIOME_FOREST | BIOME_TAIGA)
            && matches!(local, BIOME_DESERT | BIOME_ARCTIC)
            || matches!(target_biome, BIOME_JUNGLE) && matches!(local, BIOME_TUNDRA | BIOME_ARCTIC);
        if gross_opposite || thermal_gap > 13.0 || water_gap > 850.0 {
            GraftCompatibility::Incompatible
        } else if thermal_gap > 3.0 || water_gap > 180.0 {
            GraftCompatibility::Marginal
        } else {
            GraftCompatibility::Compatible
        }
    }
}
