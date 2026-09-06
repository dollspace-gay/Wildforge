//! Climate-, ground-, and water-derived biomes and geographic countries.
//!
//! The dense layers contain only facts needed by ordinary local queries.  The
//! sparse model owns country composition, heart sites, and traversable
//! adjacency once per country instead of repeating them in every atlas cell.

use super::grid::generate_grid;
use super::identity::cell_hash;
use super::{
    AtlasError, AtlasGrid, AtlasPos, BiomeCell, ClimateCell, GenerationMode, GeometryCell,
    GroundCell, HydrologyCell, TectonicCell, TerrainCell,
};
use crate::planet::Direction4;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod records;
pub use records::{CountryRecord, CountryRoute, CountrySoilComposition};
mod classification;
mod ground;
pub(super) use ground::generate_ground_layer;
mod country_partition;
mod country_seeds;
mod habitats;
mod hearts;
mod sampling;
mod validation;
use classification::biome_name;
use country_partition::{crossing_cost, partition_countries};
use country_seeds::choose_country_seeds;
use habitats::derive_biome_cell;
use hearts::heart_score;
pub use sampling::{AtlasBiomeSample, GraftCompatibility};

pub const BIOME_SCHEMA_VERSION: u32 = 1;

pub const HABITAT_RIPARIAN: u32 = 1 << 0;
pub const HABITAT_FLOODPLAIN: u32 = 1 << 1;
pub const HABITAT_WETLAND: u32 = 1 << 2;
pub const HABITAT_OASIS: u32 = 1 << 3;
pub const HABITAT_SPRING: u32 = 1 << 4;
pub const HABITAT_LAKESHORE: u32 = 1 << 5;
pub const HABITAT_ESTUARY_DELTA: u32 = 1 << 6;
pub const HABITAT_SALT_MARSH: u32 = 1 << 7;
pub const HABITAT_BEACH_DUNE: u32 = 1 << 8;
pub const HABITAT_AQUATIC_FRESH: u32 = 1 << 9;
pub const HABITAT_AQUATIC_BRACKISH: u32 = 1 << 10;
pub const HABITAT_AQUATIC_SALT: u32 = 1 << 11;
pub const HABITAT_CAVE_OUTLET: u32 = 1 << 12;
pub const HABITAT_ALPINE: u32 = 1 << 13;
pub const HABITAT_VOLCANIC_SOIL: u32 = 1 << 14;
pub const HABITAT_PERMAFROST: u32 = 1 << 15;

pub const EDAPHIC_SHALLOW_ROCK: u16 = 1 << 0;
pub const EDAPHIC_LIMESTONE: u16 = 1 << 1;
pub const EDAPHIC_DUNE: u16 = 1 << 2;
pub const EDAPHIC_VOLCANIC: u16 = 1 << 3;
pub const EDAPHIC_ALLUVIAL: u16 = 1 << 4;
pub const EDAPHIC_STEEP: u16 = 1 << 5;
pub const EDAPHIC_FIRE_FAVORED: u16 = 1 << 6;
pub const EDAPHIC_PERMAFROST: u16 = 1 << 7;

pub const FREEZE_SEASONAL: u8 = 1 << 0;
pub const FREEZE_PERMAFROST: u8 = 1 << 1;

/// Stable dense biome identifiers match `worldgen::Biome::from_index`.
pub const BIOME_FOREST: u8 = 1;
pub const BIOME_PLAINS: u8 = 2;
pub const BIOME_DESERT: u8 = 3;
pub const BIOME_JUNGLE: u8 = 4;
pub const BIOME_SCRUBLAND: u8 = 5;
pub const BIOME_TAIGA: u8 = 6;
pub const BIOME_ARCTIC: u8 = 7;
pub const BIOME_MOUNTAINS: u8 = 8;
pub const BIOME_SWAMP: u8 = 9;
pub const BIOME_SAVANNA: u8 = 10;
pub const BIOME_TUNDRA: u8 = 11;
pub const BIOME_BADLANDS: u8 = 12;
pub const BIOME_OCEAN: u8 = 13;

pub const HABITAT_NAMES: &[(u32, &str)] = &[
    (HABITAT_RIPARIAN, "riparian"),
    (HABITAT_FLOODPLAIN, "floodplain"),
    (HABITAT_WETLAND, "wetland"),
    (HABITAT_OASIS, "oasis"),
    (HABITAT_SPRING, "spring"),
    (HABITAT_LAKESHORE, "lakeshore"),
    (HABITAT_ESTUARY_DELTA, "estuary_delta"),
    (HABITAT_SALT_MARSH, "salt_marsh"),
    (HABITAT_BEACH_DUNE, "beach_dune"),
    (HABITAT_AQUATIC_FRESH, "freshwater"),
    (HABITAT_AQUATIC_BRACKISH, "brackish"),
    (HABITAT_AQUATIC_SALT, "marine"),
    (HABITAT_CAVE_OUTLET, "cave_aquifer_outlet"),
    (HABITAT_ALPINE, "alpine"),
    (HABITAT_VOLCANIC_SOIL, "volcanic_soil"),
    (HABITAT_PERMAFROST, "permafrost"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BiomeModel {
    pub schema_version: u32,
    pub countries: Vec<CountryRecord>,
}

impl Default for BiomeModel {
    fn default() -> Self {
        Self {
            schema_version: BIOME_SCHEMA_VERSION,
            countries: Vec::new(),
        }
    }
}

#[derive(Default)]
struct CountryAccum {
    area: f64,
    cells: u32,
    biome: [u32; 14],
    habitat: [u32; 16],
    soil: [u64; 8],
    watersheds: BTreeMap<u32, u32>,
    landmasses: BTreeMap<u16, u32>,
}

fn dominant_index(counts: &[u32]) -> usize {
    counts
        .iter()
        .enumerate()
        .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
        .map_or(0, |(index, _)| index)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn generate_biomes_and_countries(
    seed: u32,
    side: u16,
    mode: GenerationMode,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    terrain: &AtlasGrid<TerrainCell>,
    climate: &AtlasGrid<ClimateCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
) -> Result<(AtlasGrid<BiomeCell>, BiomeModel), AtlasError> {
    let mut biomes = generate_grid(side, mode, |pos| {
        derive_biome_cell(
            pos.index(side),
            side,
            terrain,
            tectonics,
            climate,
            hydrology,
            ground,
        )
    })?;
    let seeds = choose_country_seeds(seed, side, geometry, terrain, hydrology, &biomes);
    let (owner, _) = partition_countries(side, &seeds, terrain, hydrology, ground);
    let mut accum = (0..seeds.len())
        .map(|_| CountryAccum::default())
        .collect::<Vec<_>>();
    for (index, country) in owner.iter().copied().enumerate() {
        if country == 0 {
            continue;
        }
        let cell = &mut biomes.values_mut()[index];
        cell.country_id = country;
        cell.heart_assignment = country;
        let stats = &mut accum[usize::from(country - 1)];
        stats.cells += 1;
        stats.area += f64::from(geometry.values()[index].physical_area);
        stats.biome[usize::from(cell.baseline_biome).min(13)] += 1;
        for (bit, slot) in (0..16).enumerate() {
            if cell.habitat_flags & (1 << bit) != 0 {
                stats.habitat[slot] += 1;
            }
        }
        let soil = ground.values()[index];
        let clay = 255u8.saturating_sub(soil.sand).saturating_sub(soil.silt);
        for (slot, value) in [
            soil.soil_depth_decimeters,
            soil.sand,
            soil.silt,
            clay,
            soil.organic,
            soil.baseline_fertility,
            soil.drainage,
            soil.soil_salinity,
        ]
        .into_iter()
        .enumerate()
        {
            stats.soil[slot] += u64::from(value);
        }
        *stats
            .watersheds
            .entry(hydrology.values()[index].watershed_id)
            .or_default() += 1;
        *stats
            .landmasses
            .entry(terrain.values()[index].landmass_id)
            .or_default() += 1;
    }

    let mut heart_sites = seeds.clone();
    for (offset, stats) in accum.iter().enumerate() {
        let country = (offset + 1) as u16;
        let dominant = dominant_index(&stats.biome) as u8;
        let wetland_country = stats.habitat[2] > stats.cells / 3;
        let mut best = (f32::NEG_INFINITY, seeds[offset]);
        for (index, assigned) in owner.iter().copied().enumerate() {
            if assigned != country {
                continue;
            }
            let score = heart_score(
                index,
                dominant,
                wetland_country,
                side,
                seeds[offset],
                geometry,
                terrain,
                hydrology,
                ground,
                &biomes,
            );
            if score > best.0 {
                best = (score, index);
            }
        }
        heart_sites[offset] = best.1;
    }

    let mut adjacency = BTreeMap::<(u16, u16), (f32, usize)>::new();
    for index in 0..owner.len() {
        let a = owner[index];
        if a == 0 {
            continue;
        }
        let pos = AtlasPos::from_index(index, side).expect("atlas index");
        for direction in [Direction4::East, Direction4::North] {
            let next = pos.step(direction, side).pos.index(side);
            let b = owner[next];
            if b == 0 || a == b {
                continue;
            }
            let pair = if a < b { (a, b) } else { (b, a) };
            let barrier = crossing_cost(index, next, side, terrain, hydrology, ground);
            let candidate = adjacency.entry(pair).or_insert((barrier, index));
            if barrier < candidate.0 {
                *candidate = (barrier, index);
            }
        }
    }

    let mut routes = vec![Vec::<CountryRoute>::new(); seeds.len()];
    for ((a, b), (barrier, index)) in adjacency {
        let pass = AtlasPos::from_index(index, side).expect("pass index");
        let barrier = (barrier * 8.0).clamp(0.0, f32::from(u16::MAX)) as u16;
        routes[usize::from(a - 1)].push(CountryRoute {
            neighbor_id: b,
            pass,
            barrier,
        });
        routes[usize::from(b - 1)].push(CountryRoute {
            neighbor_id: a,
            pass,
            barrier,
        });
    }

    let countries = accum
        .into_iter()
        .enumerate()
        .map(|(offset, stats)| {
            let id = (offset + 1) as u16;
            let divisor = u64::from(stats.cells.max(1));
            let dominant = dominant_index(&stats.biome) as u8;
            let mut habitat_cells = BTreeMap::new();
            for (slot, (bit, name)) in HABITAT_NAMES.iter().enumerate() {
                let count = stats.habitat[slot];
                if count > 0 && *bit != 0 {
                    habitat_cells.insert((*name).to_string(), count);
                }
            }
            let mut biome_cells = BTreeMap::new();
            for biome in 1..=13u8 {
                let count = stats.biome[usize::from(biome)];
                if count > 0 {
                    biome_cells.insert(biome_name(biome).to_string(), count);
                }
            }
            let principal_watershed = stats
                .watersheds
                .into_iter()
                .max_by_key(|(id, count)| (*count, std::cmp::Reverse(*id)))
                .map_or(0, |(id, _)| id);
            let principal_landmass = stats
                .landmasses
                .into_iter()
                .max_by_key(|(id, count)| (*count, std::cmp::Reverse(*id)))
                .map_or(0, |(id, _)| id);
            let avg = |slot: usize| (stats.soil[slot] / divisor).min(255) as u8;
            let seed_site = AtlasPos::from_index(seeds[offset], side).expect("seed index");
            let heart_site = AtlasPos::from_index(heart_sites[offset], side).expect("heart index");
            let heart_form = if stats.habitat[2] > stats.cells / 3 {
                // The dry hummock is the accessible centre of a wetland
                // heart site, but the country raises the swamp form around
                // it rather than pretending its zonal forest/plains label
                // explains the saturated landscape.
                BIOME_SWAMP
            } else {
                dominant
            };
            CountryRecord {
                id,
                name_seed: cell_hash(seed, seed_site, 0x4e41_4d45),
                seed_site,
                heart_site,
                dominant_biome: dominant,
                heart_form,
                principal_watershed,
                principal_landmass,
                cell_count: stats.cells,
                physical_area: stats.area,
                habitat_cells,
                biome_cells,
                soil: CountrySoilComposition {
                    mean_depth_decimeters: avg(0),
                    mean_sand: avg(1),
                    mean_silt: avg(2),
                    mean_clay: avg(3),
                    mean_organic: avg(4),
                    mean_fertility: avg(5),
                    mean_drainage: avg(6),
                    mean_salinity: avg(7),
                },
                routes: std::mem::take(&mut routes[offset]),
            }
        })
        .collect();
    let model = BiomeModel {
        schema_version: BIOME_SCHEMA_VERSION,
        countries,
    };
    model.validate(side, terrain, &biomes)?;
    Ok((biomes, model))
}
