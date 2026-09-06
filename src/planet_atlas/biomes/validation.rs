//! Country record lookup and dense partition consistency.

use super::{
    BIOME_OCEAN, BIOME_SCHEMA_VERSION, BiomeModel, CountryRecord, HABITAT_AQUATIC_BRACKISH,
    HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT,
};
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{ATLAS_FACE_SIDE, AtlasError, AtlasGrid, BiomeCell, TerrainCell};

impl BiomeModel {
    pub fn country(&self, id: u16) -> Option<&CountryRecord> {
        id.checked_sub(1)
            .and_then(|index| self.countries.get(usize::from(index)))
            .filter(|country| country.id == id)
    }

    pub fn validate(
        &self,
        side: u16,
        terrain: &AtlasGrid<TerrainCell>,
        biomes: &AtlasGrid<BiomeCell>,
    ) -> Result<(), AtlasError> {
        if self.schema_version != BIOME_SCHEMA_VERSION || self.countries.len() > u16::MAX as usize {
            return Err(AtlasError::Corrupt(
                "biome model has an unsupported schema or country count".into(),
            ));
        }
        if side == ATLAS_FACE_SIDE && !(400..=600).contains(&self.countries.len()) {
            return Err(AtlasError::Corrupt(format!(
                "production geography has {} countries; expected 400..=600",
                self.countries.len()
            )));
        }
        let mut dense_counts = vec![0u32; self.countries.len()];
        let aquatic_mask = HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT;
        for (index, cell) in biomes.values().iter().enumerate() {
            let above_sea = terrain.values()[index].eroded_elevation > SEA_LEVEL as f32;
            let terrestrial = above_sea && cell.habitat_flags & aquatic_mask == 0;
            if (terrestrial && cell.country_id == 0)
                || (!above_sea && cell.country_id != 0)
                || cell.country_id != cell.heart_assignment
            {
                return Err(AtlasError::Corrupt(
                    "country partition does not cover terrestrial land exactly once".into(),
                ));
            }
            if cell.country_id != 0 {
                let Some(slot) = cell.country_id.checked_sub(1).map(usize::from) else {
                    return Err(AtlasError::Corrupt("country id underflow".into()));
                };
                let Some(count) = dense_counts.get_mut(slot) else {
                    return Err(AtlasError::Corrupt("dense country id has no record".into()));
                };
                *count += 1;
            }
            if !(1..=13).contains(&cell.baseline_biome) {
                return Err(AtlasError::Corrupt("unknown zonal biome identifier".into()));
            }
        }
        for (offset, country) in self.countries.iter().enumerate() {
            if country.id as usize != offset + 1
                || country.cell_count == 0
                || country.cell_count != dense_counts[offset]
                || country.dominant_biome == BIOME_OCEAN
                || biomes.values()[country.heart_site.index(side)].country_id != country.id
                || biomes.values()[country.heart_site.index(side)].habitat_flags & aquatic_mask != 0
                || terrain.values()[country.heart_site.index(side)].eroded_elevation
                    <= SEA_LEVEL as f32
            {
                return Err(AtlasError::Corrupt(
                    "country record, heart site, and dense partition disagree".into(),
                ));
            }
            for route in &country.routes {
                let Some(neighbor) = self.country(route.neighbor_id) else {
                    return Err(AtlasError::Corrupt(
                        "country route names an absent neighbor".into(),
                    ));
                };
                if !neighbor
                    .routes
                    .iter()
                    .any(|back| back.neighbor_id == country.id)
                {
                    return Err(AtlasError::Corrupt(
                        "country adjacency is not reciprocal".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
