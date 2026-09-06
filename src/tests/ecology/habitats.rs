//! Habitats scenarios.

use super::*;

#[test]
fn atlas_animals_require_their_compound_habitats() {
    use crate::planet_atlas::{
        BIOME_ARCTIC, BIOME_MOUNTAINS, BIOME_TAIGA, BIOME_TUNDRA, HABITAT_AQUATIC_FRESH,
        HABITAT_AQUATIC_SALT, HABITAT_WETLAND,
    };

    let reg = base_reg();
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_704, 64).unwrap());
    let center = |pos: crate::planet_atlas::AtlasPos| {
        let point = pos.center(atlas.side());
        crate::planet::SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16)
            .unwrap()
    };
    let frog_sites = atlas
        .genesis
        .biomes
        .iter()
        .filter(|(pos, biome)| {
            biome.habitat_flags & HABITAT_WETLAND != 0
                && !matches!(
                    biome.baseline_biome,
                    BIOME_ARCTIC | BIOME_TUNDRA | BIOME_MOUNTAINS
                )
                && (4.0..=30.0).contains(
                    &atlas
                        .genesis
                        .climate
                        .get(*pos)
                        .unwrap()
                        .seasonal_temperature[0],
                )
        })
        .map(|(pos, _)| pos)
        .take(24)
        .collect::<Vec<_>>();
    let trout_sites = atlas
        .genesis
        .biomes
        .iter()
        .filter(|(_, biome)| {
            biome.habitat_flags & HABITAT_AQUATIC_FRESH != 0
                && matches!(
                    biome.baseline_biome,
                    BIOME_TAIGA | BIOME_ARCTIC | BIOME_MOUNTAINS | BIOME_TUNDRA
                )
        })
        .map(|(pos, _)| pos)
        .take(24)
        .collect::<Vec<_>>();
    let seal_sites = atlas
        .genesis
        .biomes
        .iter()
        .filter(|(pos, biome)| {
            biome.habitat_flags & HABITAT_AQUATIC_SALT != 0
                && atlas.genesis.climate.get(*pos).unwrap().mean_temperature <= 7.0
                && (-8.0..=12.0).contains(
                    &atlas
                        .genesis
                        .climate
                        .get(*pos)
                        .unwrap()
                        .seasonal_temperature[0],
                )
        })
        .map(|(pos, _)| pos)
        .take(24)
        .collect::<Vec<_>>();
    assert!(!frog_sites.is_empty() && !trout_sites.is_empty() && !seal_sites.is_empty());

    let mut world = World::new_with_atlas(
        8_704,
        tmp_dir("atlas-animal-habitats"),
        reg.clone(),
        atlas.clone(),
    );
    {
        let mut supports = |species: &str, sites: &[crate::planet_atlas::AtlasPos], wet: bool| {
            let species = reg.animal_id(species).unwrap();
            sites.iter().copied().any(|site| {
                let surface = center(site);
                world.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
                world.animal_habitat_suitable_at(species, surface, wet)
            })
        };
        assert!(supports("base:frog", &frog_sites, false));
        assert!(supports("base:trout", &trout_sites, true));
        assert!(supports("base:seal", &seal_sites, true));
    }

    let connected = |species: &str, sites: &[crate::planet_atlas::AtlasPos]| {
        let species = reg.animal_id(species).unwrap();
        sites
            .iter()
            .copied()
            .any(|site| world.animal_habitat_network_connected_at(species, center(site)))
    };
    assert!(
        connected("base:frog", &frog_sites),
        "wetland recovery has a neighboring wetland rather than a chunk-local respawn roll"
    );
    assert!(
        connected("base:trout", &trout_sites),
        "freshwater recovery follows a connected cold-water network"
    );
    assert!(
        connected("base:seal", &seal_sites),
        "seal recovery follows connected cold coast or ocean habitat"
    );
}
