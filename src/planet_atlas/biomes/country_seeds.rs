//! Deterministic island, watershed, and distance-weighted country nuclei.

use super::{
    BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS, BIOME_PLAINS, BIOME_SAVANNA,
    BIOME_TAIGA, BIOME_TUNDRA, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH,
    HABITAT_AQUATIC_SALT,
};
use crate::chunk::SEA_LEVEL;
use crate::planet::geodesic_distance;
use crate::planet_atlas::identity::cell_hash;
use crate::planet_atlas::{
    ATLAS_FACE_SIDE, AtlasGrid, AtlasPos, BiomeCell, GeometryCell, HydrologyCell, TerrainCell,
};
use std::collections::BTreeMap;

fn seed_weight(index: usize, biomes: &[BiomeCell], terrain: &[TerrainCell]) -> f32 {
    let biome = biomes[index].baseline_biome;
    let fragmented = if terrain[index].landmass_id != 0 && terrain[index].landmass_id > 8 {
        1.28
    } else {
        1.0
    };
    fragmented
        * match biome {
            BIOME_MOUNTAINS => 1.42,
            BIOME_TUNDRA | BIOME_TAIGA | BIOME_FOREST | BIOME_JUNGLE => 1.08,
            BIOME_PLAINS | BIOME_SAVANNA | BIOME_DESERT => 0.78,
            _ => 0.94,
        }
}

pub(super) fn choose_country_seeds(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    biomes: &AtlasGrid<BiomeCell>,
) -> Vec<usize> {
    let aquatic_mask = HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT;
    let land = terrain
        .values()
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| {
            (cell.eroded_elevation > SEA_LEVEL as f32
                && biomes.values()[index].habitat_flags & aquatic_mask == 0)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if land.is_empty() {
        return Vec::new();
    }
    let mut target = if side == ATLAS_FACE_SIDE {
        500usize
    } else {
        land.len().div_ceil(280).clamp(1, 500)
    }
    .min(land.len());

    // Every disconnected island begins with a seed.  The deterministic hash
    // breaks equal-area candidates without introducing face preference.
    let mut by_landmass = BTreeMap::<u16, (f64, usize)>::new();
    for &index in &land {
        let cell = terrain.values()[index];
        let pos = AtlasPos::from_index(index, side).expect("atlas index");
        let score = f64::from(geometry.values()[index].physical_area)
            * (1.0 + (cell_hash(seed, pos, 0x434f_554e) & 0xffff) as f64 / 65_535.0 * 0.01);
        let entry = by_landmass
            .entry(cell.landmass_id)
            .or_insert((score, index));
        if score > entry.0 {
            *entry = (score, index);
        }
    }
    target = target.max(by_landmass.len()).min(land.len());
    let mut seeds = by_landmass
        .into_values()
        .map(|(_, index)| index)
        .take(target)
        .collect::<Vec<_>>();

    // Pure farthest-point seeding looks evenly spaced on a globe but often
    // puts every seed on one side of a major divide. Dijkstra then has no
    // competing country on the other side and is forced to cross the very
    // barrier intended to define the border. Give the largest drainage
    // basins their own nuclei first; remaining seeds still use geographic
    // spacing, so a giant open basin can contain several countries while
    // tiny gullies do not each become one.
    let mut watersheds = BTreeMap::<u32, (u32, f32, usize)>::new();
    for &index in &land {
        let watershed = hydrology.values()[index].watershed_id;
        if watershed == 0 {
            continue;
        }
        let pos = AtlasPos::from_index(index, side).expect("watershed seed candidate");
        let jitter = (cell_hash(seed, pos, 0x5741_5445) & 0xffff) as f32 / 65_535.0;
        let suitability = seed_weight(index, biomes.values(), terrain.values()) + jitter * 0.01;
        let entry = watersheds
            .entry(watershed)
            .or_insert((0, suitability, index));
        entry.0 += 1;
        if suitability > entry.1 {
            entry.1 = suitability;
            entry.2 = index;
        }
    }
    let mut watersheds = watersheds.into_values().collect::<Vec<_>>();
    watersheds.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    let basin_seed_target = (target * 3 / 4).max(seeds.len()).min(target);
    for (_, _, index) in watersheds {
        if seeds.len() >= basin_seed_target {
            break;
        }
        if !seeds.contains(&index) {
            seeds.push(index);
        }
    }
    let mut nearest = vec![f32::INFINITY; terrain.len()];
    for &site in &seeds {
        let center = AtlasPos::from_index(site, side)
            .expect("seed index")
            .center(side);
        for &index in &land {
            let point = AtlasPos::from_index(index, side)
                .expect("land index")
                .center(side);
            nearest[index] = nearest[index].min(geodesic_distance(center, point) as f32);
        }
    }
    while seeds.len() < target {
        let mut best = None::<(f32, usize)>;
        for &index in &land {
            if seeds.contains(&index) {
                continue;
            }
            let pos = AtlasPos::from_index(index, side).expect("land index");
            let jitter =
                0.985 + (cell_hash(seed, pos, 0x5345_4544) & 0xffff) as f32 / 65_535.0 * 0.03;
            let score =
                nearest[index] * seed_weight(index, biomes.values(), terrain.values()) * jitter;
            if best.is_none_or(|candidate| score > candidate.0) {
                best = Some((score, index));
            }
        }
        let Some((_, site)) = best else { break };
        seeds.push(site);
        let center = AtlasPos::from_index(site, side)
            .expect("seed index")
            .center(side);
        for &index in &land {
            let point = AtlasPos::from_index(index, side)
                .expect("land index")
                .center(side);
            nearest[index] = nearest[index].min(geodesic_distance(center, point) as f32);
        }
    }
    seeds
}
