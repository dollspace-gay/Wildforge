//! Qualification site selection for countries.

use crate::planet_atlas::{AtlasPos, BIOME_FOREST, BIOME_MOUNTAINS, BIOME_PLAINS, BIOME_TAIGA, HABITAT_ALPINE, HABITAT_BEACH_DUNE, PlanetAtlas};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    if let Some((coast_pos, coast_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter(|(pos, biome)| {
            let climate = atlas.genesis.climate.get(*pos).expect("grid");
            matches!(
                biome.baseline_biome,
                BIOME_FOREST | BIOME_PLAINS | BIOME_TAIGA
            ) && biome.habitat_flags & HABITAT_BEACH_DUNE != 0
                && climate.mean_temperature >= 8.0
                && climate.snow_persistence < 0.08
        })
        .max_by_key(|(_, biome)| biome.vegetation_potential)
    {
        insert(
            "temperate_coast",
            coast_pos,
            format!(
                "temperate biome {}; coastal salinity {}; vegetation {}",
                coast_biome.baseline_biome,
                atlas
                    .genesis
                    .ground
                    .get(coast_pos)
                    .expect("grid")
                    .soil_salinity,
                coast_biome.vegetation_potential
            ),
        );
        if let Some((interior_pos, climate)) = atlas
            .genesis
            .climate
            .iter()
            .filter(|(pos, _)| {
                let biome = atlas.genesis.biomes.get(*pos).expect("grid");
                let climate = atlas.genesis.climate.get(*pos).expect("grid");
                biome.baseline_biome == coast_biome.baseline_biome
                    && biome.habitat_flags & HABITAT_BEACH_DUNE == 0
                    && climate.snow_persistence < 0.08
            })
            .max_by(|(_, a), (_, b)| a.continentality.total_cmp(&b.continentality))
        {
            insert(
                "temperate_continental_interior",
                interior_pos,
                format!(
                    "same zonal biome as temperate_coast; continentality {:.3}; annual range {:.1}",
                    climate.continentality, climate.seasonality
                ),
            );
        }
    }
    if let Some((forest_pos, alpine_pos)) = atlas.genesis.biomes.iter().find_map(|(pos, biome)| {
        matches!(biome.baseline_biome, BIOME_FOREST | BIOME_TAIGA)
            .then(|| {
                pos.neighbors4(atlas.side()).into_iter().find(|neighbor| {
                    atlas.genesis.biomes.get(*neighbor).is_some_and(|other| {
                        other.baseline_biome == BIOME_MOUNTAINS
                            || other.habitat_flags & HABITAT_ALPINE != 0
                    })
                })
            })
            .flatten()
            .map(|alpine| (pos, alpine))
    }) {
        insert(
            "treeline_forest",
            forest_pos,
            format!(
                "forest/taiga below local tree line {}",
                atlas
                    .genesis
                    .biomes
                    .get(forest_pos)
                    .expect("grid")
                    .tree_line_y
            ),
        );
        insert(
            "treeline_alpine",
            alpine_pos,
            format!(
                "adjacent alpine cell; elevation {:.1}; tree line {}",
                atlas
                    .genesis
                    .terrain
                    .get(alpine_pos)
                    .expect("grid")
                    .eroded_elevation,
                atlas
                    .genesis
                    .biomes
                    .get(alpine_pos)
                    .expect("grid")
                    .tree_line_y
            ),
        );
    }
    if let Some(country) = atlas.biomes.countries.iter().max_by_key(|country| {
        let climate = atlas
            .genesis
            .climate
            .get(country.heart_site)
            .expect("country heart is inside atlas");
        (
            u8::from(climate.mean_temperature >= 10.0 && climate.snow_persistence < 0.08),
            country.habitat_cells.len(),
            country.biome_cells.len(),
            country.cell_count,
        )
    }) {
        insert(
            "multi_habitat_country",
            country.heart_site,
            format!(
                "country {}; {} habitat overlays and {} zonal biomes across {} cells",
                country.id,
                country.habitat_cells.len(),
                country.biome_cells.len(),
                country.cell_count
            ),
        );
    }

}
