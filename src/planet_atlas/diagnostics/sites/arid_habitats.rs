//! Qualification site selection for arid habitats.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{
    AtlasPos, BIOME_BADLANDS, BIOME_DESERT, BIOME_SCRUBLAND, EDAPHIC_STEEP, FREEZE_SEASONAL,
    HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT, HABITAT_OASIS,
    HABITAT_RIPARIAN, HABITAT_SPRING, HABITAT_WETLAND, PlanetAtlas,
};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    if let Some((contrast, discharge, river_pos, overlook, river_biome, overlook_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter_map(|(river_pos, river_biome)| {
            let river_terrain = atlas.genesis.terrain.get(river_pos).expect("grid");
            let river_climate = atlas.genesis.climate.get(river_pos).expect("grid");
            let river_ground = atlas.genesis.ground.get(river_pos).expect("grid");
            if !matches!(
                river_biome.baseline_biome,
                BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS
            ) || river_biome.habitat_flags & HABITAT_RIPARIAN == 0
                || river_biome.edaphic_flags & EDAPHIC_STEEP != 0
                || river_terrain.eroded_elevation > SEA_LEVEL as f32 + 24.0
                || river_climate.mean_temperature < 16.0
                || river_climate.snow_persistence > 0.02
                || river_ground.freeze_flags & FREEZE_SEASONAL != 0
            {
                return None;
            }
            let overlook = river_pos
                .neighbors4(atlas.side())
                .into_iter()
                .filter(|neighbor| {
                    let terrain = atlas.genesis.terrain.get(*neighbor).expect("grid");
                    let biome = atlas.genesis.biomes.get(*neighbor).expect("grid");
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && terrain.eroded_elevation - river_terrain.eroded_elevation < 12.0
                        && biome.baseline_biome == river_biome.baseline_biome
                        && biome.edaphic_flags & EDAPHIC_STEEP == 0
                        && biome.habitat_flags
                            & (HABITAT_RIPARIAN
                                | HABITAT_WETLAND
                                | HABITAT_OASIS
                                | HABITAT_AQUATIC_FRESH
                                | HABITAT_AQUATIC_BRACKISH
                                | HABITAT_AQUATIC_SALT)
                            == 0
                })
                .min_by_key(|neighbor| {
                    atlas
                        .genesis
                        .biomes
                        .get(*neighbor)
                        .expect("grid")
                        .vegetation_potential
                })?;
            let overlook_biome = atlas.genesis.biomes.get(overlook).expect("grid");
            let contrast = river_biome
                .vegetation_potential
                .saturating_sub(overlook_biome.vegetation_potential);
            let discharge = atlas
                .genesis
                .hydrology
                .get(river_pos)
                .expect("matching grid")
                .mean_discharge;
            Some((
                contrast,
                discharge,
                river_pos,
                overlook,
                river_biome,
                overlook_biome,
            ))
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| b.2.cmp(&a.2))
        })
    {
        insert(
            "desert_green_river",
            overlook,
            format!(
                "dry overlook vegetation {} toward river at {},{},{}; arid zonal biome {}; riparian vegetation {}; contrast {}; discharge {:.1}",
                overlook_biome.vegetation_potential,
                river_pos.face.name(),
                river_pos.u,
                river_pos.v,
                river_biome.baseline_biome,
                river_biome.vegetation_potential,
                contrast,
                discharge
            ),
        );
    }
    if let Some((_, _, _, contrast, spring_pos, overlook, spring_biome, overlook_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter_map(|(spring_pos, spring_biome)| {
            let spring_terrain = atlas.genesis.terrain.get(spring_pos).expect("grid");
            let spring_climate = atlas.genesis.climate.get(spring_pos).expect("grid");
            let spring_ground = atlas.genesis.ground.get(spring_pos).expect("grid");
            if spring_biome.habitat_flags & (HABITAT_OASIS | HABITAT_SPRING)
                != (HABITAT_OASIS | HABITAT_SPRING)
                || spring_climate.mean_temperature < 16.0
                || spring_climate.snow_persistence > 0.02
                || spring_ground.freeze_flags & FREEZE_SEASONAL != 0
            {
                return None;
            }
            let overlook = spring_pos
                .neighbors4(atlas.side())
                .into_iter()
                .filter(|neighbor| {
                    let terrain = atlas.genesis.terrain.get(*neighbor).expect("grid");
                    let biome = atlas.genesis.biomes.get(*neighbor).expect("grid");
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && matches!(
                            biome.baseline_biome,
                            BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS
                        )
                        && biome.habitat_flags
                            & (HABITAT_OASIS
                                | HABITAT_SPRING
                                | HABITAT_RIPARIAN
                                | HABITAT_WETLAND
                                | HABITAT_AQUATIC_FRESH)
                            == 0
                })
                .min_by_key(|neighbor| {
                    atlas
                        .genesis
                        .biomes
                        .get(*neighbor)
                        .expect("grid")
                        .vegetation_potential
                })?;
            let overlook_biome = atlas.genesis.biomes.get(overlook).expect("grid");
            let overlook_terrain = atlas.genesis.terrain.get(overlook).expect("grid");
            let contrast = spring_biome
                .vegetation_potential
                .saturating_sub(overlook_biome.vegetation_potential);
            Some((
                u8::from(spring_biome.habitat_flags & HABITAT_RIPARIAN == 0),
                u8::from(
                    spring_terrain.eroded_elevation <= SEA_LEVEL as f32 + 32.0
                        && overlook_terrain.eroded_elevation <= SEA_LEVEL as f32 + 32.0,
                ),
                u8::from(
                    spring_biome.edaphic_flags & EDAPHIC_STEEP == 0
                        && overlook_biome.edaphic_flags & EDAPHIC_STEEP == 0,
                ),
                contrast,
                spring_pos,
                overlook,
                spring_biome,
                overlook_biome,
            ))
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
                .then_with(|| b.4.cmp(&a.4))
        })
    {
        let ground = atlas.genesis.ground.get(spring_pos).expect("matching grid");
        insert(
            "natural_oasis_spring",
            overlook,
            format!(
                "dry overlook vegetation {} toward spring at {},{},{}; fresh shallow groundwater head {:.1}; oasis vegetation {}; contrast {}; permeability {}",
                overlook_biome.vegetation_potential,
                spring_pos.face.name(),
                spring_pos.u,
                spring_pos.v,
                ground.baseline_groundwater_head,
                spring_biome.vegetation_potential,
                contrast,
                ground.aquifer_permeability
            ),
        );
    }
}
