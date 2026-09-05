//! Climate-, ground-, and water-derived biomes and geographic countries.
//!
//! The dense layers contain only facts needed by ordinary local queries.  The
//! sparse model owns country composition, heart sites, and traversable
//! adjacency once per country instead of repeating them in every atlas cell.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{Direction4, SurfacePos, geodesic_distance, surface_to_unit};
use crate::planet_atlas::{ATLAS_FACE_SIDE, AtlasError, AtlasGrid, AtlasPos, BedrockFamily, BiomeCell, ClimateCell, GenerationMode, GeometryCell, GroundCell, HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_FLOODPLAIN, HYDRO_KARST_LOSS, HYDRO_LAKE, HYDRO_OCEAN, HYDRO_PERENNIAL, HYDRO_PLAYA, HYDRO_RIVER, HYDRO_TERMINAL, HYDRO_WETLAND, HydrologyCell, PlanetAtlas, TectonicCell, TerrainCell, WaterBodyKind};
use crate::planet_atlas::grid::{generate_grid};
use crate::planet_atlas::identity::{cell_hash};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};




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
pub struct CountrySoilComposition {
    pub mean_depth_decimeters: u8,
    pub mean_sand: u8,
    pub mean_silt: u8,
    pub mean_clay: u8,
    pub mean_organic: u8,
    pub mean_fertility: u8,
    pub mean_drainage: u8,
    pub mean_salinity: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountryRoute {
    pub neighbor_id: u16,
    pub pass: AtlasPos,
    /// Compact relative crossing cost; lower values are easier routes.
    pub barrier: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountryRecord {
    pub id: u16,
    pub name_seed: u64,
    pub seed_site: AtlasPos,
    pub heart_site: AtlasPos,
    pub dominant_biome: u8,
    pub heart_form: u8,
    pub principal_watershed: u32,
    pub principal_landmass: u16,
    pub cell_count: u32,
    pub physical_area: f64,
    pub habitat_cells: BTreeMap<String, u32>,
    pub biome_cells: BTreeMap<String, u32>,
    pub soil: CountrySoilComposition,
    pub routes: Vec<CountryRoute>,
}

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

#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    cost: f32,
    index: usize,
    country: u16,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost.to_bits() == other.cost.to_bits()
            && self.index == other.index
            && self.country == other.country
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.index.cmp(&self.index))
            .then_with(|| other.country.cmp(&self.country))
    }
}

fn slope_at(index: usize, side: u16, terrain: &AtlasGrid<TerrainCell>) -> f32 {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let elevation = terrain.values()[index].eroded_elevation;
    pos.neighbors4(side)
        .into_iter()
        .map(|neighbor| (terrain.values()[neighbor.index(side)].eroded_elevation - elevation).abs())
        .fold(0.0, f32::max)
}

fn classify_zonal(climate: ClimateCell, elevation: f32, slope: f32, tree_line: f32) -> u8 {
    if elevation <= SEA_LEVEL as f32 {
        return BIOME_OCEAN;
    }
    let warmest = climate
        .seasonal_temperature
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    if elevation > tree_line + 7.0 || (elevation > 112.0 && slope > 17.0) {
        BIOME_MOUNTAINS
    } else if warmest < 0.0 || (climate.mean_temperature < -12.0 && climate.snow_persistence > 0.65)
    {
        BIOME_ARCTIC
    } else if warmest < 8.0 || climate.mean_temperature < -5.0 {
        BIOME_TUNDRA
    } else if climate.mean_temperature < 6.5 {
        BIOME_TAIGA
    } else if climate.mean_temperature >= 21.0
        && climate.mean_precipitation >= 1_550.0
        && climate.aridity < 0.82
        && climate.precipitation_seasonality < 0.75
    {
        BIOME_JUNGLE
    } else if climate.mean_temperature >= 18.0
        && climate.mean_precipitation >= 560.0
        && climate.aridity < 1.65
    {
        BIOME_SAVANNA
    } else if climate.mean_temperature >= 13.0
        && (climate.aridity >= 1.55 || climate.mean_precipitation < 310.0)
    {
        BIOME_DESERT
    } else if climate.aridity >= 1.02 || climate.mean_precipitation < 620.0 {
        if slope > 7.0 || climate.aridity > 1.38 {
            BIOME_BADLANDS
        } else {
            BIOME_SCRUBLAND
        }
    } else if climate.mean_precipitation >= 820.0 && climate.aridity < 1.0 {
        BIOME_FOREST
    } else {
        BIOME_PLAINS
    }
}

fn biome_name(id: u8) -> &'static str {
    match id {
        BIOME_FOREST => "forest",
        BIOME_PLAINS => "plains",
        BIOME_DESERT => "desert",
        BIOME_JUNGLE => "jungle",
        BIOME_SCRUBLAND => "scrubland",
        BIOME_TAIGA => "taiga",
        BIOME_ARCTIC => "arctic",
        BIOME_MOUNTAINS => "mountains",
        BIOME_SWAMP => "swamp",
        BIOME_SAVANNA => "savanna",
        BIOME_TUNDRA => "tundra",
        BIOME_BADLANDS => "badlands",
        BIOME_OCEAN => "ocean",
        _ => "unknown",
    }
}

fn parent_texture(parent: BedrockFamily) -> (u8, u8) {
    match parent {
        BedrockFamily::Sandstone => (188, 48),
        BedrockFamily::Limestone => (92, 96),
        BedrockFamily::Shale => (42, 92),
        BedrockFamily::Granite | BedrockFamily::Quartzite => (126, 72),
        BedrockFamily::Basalt | BedrockFamily::Ultramafic => (72, 92),
        BedrockFamily::Evaporite => (202, 35),
        BedrockFamily::Marble | BedrockFamily::Slate => (76, 94),
        BedrockFamily::MixedBasement => (108, 82),
    }
}

pub(super) fn generate_ground_layer(
    seed: u32,
    side: u16,
    mode: GenerationMode,
    tectonics: &AtlasGrid<TectonicCell>,
    terrain: &AtlasGrid<TerrainCell>,
    climate: &AtlasGrid<ClimateCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
) -> Result<AtlasGrid<GroundCell>, AtlasError> {
    let climate_grid = climate;
    generate_grid(side, mode, |pos| {
        let index = pos.index(side);
        let climate = climate.values()[index];
        let terrain_cell = terrain.values()[index];
        let hydro = hydrology.values()[index];
        let tectonic = tectonics.values()[index];
        let parent = BedrockFamily::from_id(tectonic.bedrock_family);
        let slope = slope_at(index, side, terrain);
        let geological_permeability: f32 = match parent {
            BedrockFamily::Limestone => 0.78,
            BedrockFamily::Sandstone => 0.58,
            BedrockFamily::Shale => 0.16,
            BedrockFamily::Granite | BedrockFamily::Quartzite => 0.20,
            BedrockFamily::Basalt | BedrockFamily::Ultramafic => 0.42,
            _ => 0.34,
        };
        let variation = (cell_hash(seed, pos, 0x5045_524d) & 0xffff) as f32 / 65_535.0;
        let permeability = ((geological_permeability * 0.82 + variation * 0.18) * 65_535.0) as u16;
        let alluvial = hydro.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0;
        let (mut sand, mut silt) = parent_texture(parent);
        if alluvial {
            sand = sand.saturating_sub(24);
            silt = silt.saturating_add(30).min(220);
        }
        let sum = u16::from(sand) + u16::from(silt);
        if sum > 245 {
            silt = (245 - u16::from(sand).min(245)) as u8;
        }
        let wetness = (climate.mean_precipitation
            / climate.potential_evapotranspiration.max(180.0))
        .clamp(0.0, 2.5);
        let deposition = f32::from(hydro.deposition_centiblocks) / 100.0;
        let depth_dm =
            (4.0 + wetness * 10.0 + deposition * 2.5 - slope * 0.42).clamp(1.0, 80.0) as u8;
        let baseline_groundwater_head = if hydro.flags & (HYDRO_OCEAN | HYDRO_LAKE) != 0 {
            hydro.water_surface_elevation - 0.5
        } else if hydro.flags & (HYDRO_WETLAND | HYDRO_FLOODPLAIN) != 0 {
            terrain_cell.eroded_elevation - 1.0
        } else {
            // A local recharge table sits several blocks down. Lateral
            // hydraulic pressure from a wetter/higher neighboring cell can
            // lift it against a valley wall or footslope, creating a finite
            // spring outlet instead of sprinkling decorative springs by hash.
            let local_head = terrain_cell.eroded_elevation - 4.0 - climate.aridity;
            let lateral_head = pos
                .neighbors4(side)
                .into_iter()
                .filter_map(|neighbor| {
                    let next = neighbor.index(side);
                    let neighbor_terrain = terrain.values()[next];
                    let neighbor_climate = climate_grid.values()[next];
                    if neighbor_terrain.eroded_elevation <= SEA_LEVEL as f32
                        || neighbor_climate.mean_precipitation < 140.0
                    {
                        return None;
                    }
                    let recharge = (neighbor_climate.mean_precipitation
                        / neighbor_climate.potential_evapotranspiration.max(220.0))
                    .clamp(0.0, 1.0);
                    let transmission_loss =
                        1.0 + (1.0 - geological_permeability) * 3.0 + (1.0 - recharge) * 2.0;
                    Some(
                        neighbor_terrain.eroded_elevation
                            - 4.0
                            - neighbor_climate.aridity
                            - transmission_loss,
                    )
                })
                .fold(f32::NEG_INFINITY, f32::max);
            local_head
                .max(lateral_head)
                .min(terrain_cell.eroded_elevation - 0.5)
        };
        let water_table_depth =
            (terrain_cell.eroded_elevation - baseline_groundwater_head).max(0.0);
        let drainage = ((geological_permeability * 120.0
            + f32::from(sand) * 0.52
            + slope * 4.0
            + water_table_depth * 5.0)
            .clamp(0.0, 255.0)) as u8;
        let organic = ((wetness * 72.0 + (18.0 - climate.mean_temperature).max(0.0) * 1.8
            - climate.aridity * 22.0
            - slope * 1.6)
            .clamp(0.0, 255.0)) as u8;
        let salinity = if hydro.salinity > 0 {
            hydro.salinity
        } else if hydro.flags & (HYDRO_TERMINAL | HYDRO_PLAYA) != 0 {
            (climate.aridity * 62.0).clamp(0.0, 190.0) as u8
        } else {
            ((climate.aridity - 1.0).max(0.0) * 22.0).clamp(0.0, 63.0) as u8
        };
        let volcanic = tectonic.volcanic_history != 0 || terrain_cell.volcanic_contribution > 1.5;
        let fertility = (38.0
            + f32::from(organic) * 0.56
            + if volcanic { 45.0 } else { 0.0 }
            + if alluvial { 48.0 } else { 0.0 }
            + if parent == BedrockFamily::Limestone {
                15.0
            } else {
                0.0
            }
            - f32::from(salinity) * 0.52
            - f32::from(sand) * 0.10)
            .clamp(0.0, 255.0) as u8;
        let permafrost = climate.mean_temperature < -5.0 && climate.snow_persistence > 0.45;
        let seasonal_freeze = permafrost
            || climate
                .seasonal_temperature
                .into_iter()
                .any(|temperature| temperature < 0.0);
        let freeze_flags = (u8::from(seasonal_freeze) * FREEZE_SEASONAL)
            | (u8::from(permafrost) * FREEZE_PERMAFROST);
        let erosion = ((slope * 7.0
            + hydro.mean_runoff.sqrt() * 2.2
            + if depth_dm < 5 { 34.0 } else { 0.0 }
            + if matches!(parent, BedrockFamily::Shale | BedrockFamily::Sandstone) {
                22.0
            } else {
                0.0
            })
        .clamp(0.0, 255.0)) as u8;
        GroundCell {
            soil_parent_material: tectonic.bedrock_family,
            aquifer_capacity: ((climate.mean_precipitation + hydro.mean_runoff * 0.65)
                * (24.0 + f32::from(permeability) / 4096.0)) as u32,
            aquifer_permeability: permeability,
            porosity: ((65_535u32 - u32::from(permeability)) / 2 + u32::from(permeability) / 3)
                as u16,
            baseline_groundwater_head,
            soil_depth_decimeters: depth_dm,
            sand,
            silt,
            organic,
            baseline_fertility: fertility,
            drainage,
            soil_salinity: salinity,
            freeze_flags,
            erosion_susceptibility: erosion,
        }
    })
}

fn water_neighbor_flags(
    index: usize,
    side: u16,
    hydrology: &AtlasGrid<HydrologyCell>,
) -> (bool, bool) {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let mut ocean = false;
    let mut lake = false;
    for neighbor in pos.neighbors4(side) {
        let cell = hydrology.values()[neighbor.index(side)];
        ocean |= cell.flags & HYDRO_OCEAN != 0;
        lake |= cell.flags & HYDRO_LAKE != 0;
    }
    (ocean, lake)
}

fn derive_biome_cell(
    index: usize,
    side: u16,
    terrain_grid: &AtlasGrid<TerrainCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate_grid: &AtlasGrid<ClimateCell>,
    hydrology_grid: &AtlasGrid<HydrologyCell>,
    ground_grid: &AtlasGrid<GroundCell>,
) -> BiomeCell {
    let terrain = terrain_grid.values()[index];
    let tectonic = tectonics.values()[index];
    let climate = climate_grid.values()[index];
    let hydro = hydrology_grid.values()[index];
    let ground = ground_grid.values()[index];
    let latitude = AtlasPos::from_index(index, side)
        .expect("atlas index")
        .center(side);
    let latitude = surface_to_unit(latitude).y.asin().abs() as f32;
    let slope = slope_at(index, side, terrain_grid);
    let warmest = climate
        .seasonal_temperature
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    let tree_line = (178.0 - latitude.to_degrees() * 0.78 + warmest * 0.72).clamp(72.0, 218.0);
    let zonal = classify_zonal(climate, terrain.eroded_elevation, slope, tree_line);
    let (ocean_neighbor, lake_neighbor) = water_neighbor_flags(index, side, hydrology_grid);
    let aquatic =
        hydro.flags & (HYDRO_OCEAN | HYDRO_LAKE) != 0 || hydro.water_body != WaterBodyKind::Land;
    let fresh = hydro.salinity < 64;
    let brackish = (64..192).contains(&hydro.salinity);
    let near_fresh_groundwater = ground.baseline_groundwater_head >= terrain.eroded_elevation - 2.0
        && ground.soil_salinity < 64;
    let arid = climate.aridity >= 1.02 || climate.mean_precipitation < 620.0;
    let spring = !aquatic
        && near_fresh_groundwater
        && (ground.aquifer_permeability > 12_000
            || slope > 1.5
            || hydro.flags & HYDRO_KARST_LOSS != 0
            || hydro.flags & HYDRO_PERENNIAL != 0);

    let mut habitat = 0u32;
    // Discharge exists in every drainage cell, including dry slopes with no
    // channel. Treating a tiny accumulated value as a river painted whole
    // desert watersheds riparian even though hydrology had assigned them no
    // river, water body, width, or depth.
    if hydro.flags & HYDRO_RIVER != 0 {
        habitat |= HABITAT_RIPARIAN;
    }
    if hydro.flags & HYDRO_FLOODPLAIN != 0 {
        habitat |= HABITAT_FLOODPLAIN | HABITAT_RIPARIAN;
    }
    if hydro.flags & HYDRO_WETLAND != 0
        || (!aquatic && near_fresh_groundwater && ground.drainage < 92)
    {
        habitat |= HABITAT_WETLAND;
    }
    if spring {
        habitat |= HABITAT_SPRING;
        if arid {
            habitat |= HABITAT_OASIS;
        }
    }
    if !aquatic && lake_neighbor {
        habitat |= HABITAT_LAKESHORE;
    }
    if hydro.flags & (HYDRO_ESTUARY | HYDRO_DELTA) != 0 {
        habitat |= HABITAT_ESTUARY_DELTA;
    }
    if !aquatic && ocean_neighbor {
        habitat |= HABITAT_BEACH_DUNE;
        if ground.drainage < 105 && ground.soil_salinity >= 64 {
            habitat |= HABITAT_SALT_MARSH;
        }
    }
    if aquatic {
        habitat |= if fresh {
            HABITAT_AQUATIC_FRESH
        } else if brackish {
            HABITAT_AQUATIC_BRACKISH
        } else {
            HABITAT_AQUATIC_SALT
        };
    }
    if hydro.flags & HYDRO_KARST_LOSS != 0
        || (BedrockFamily::from_id(tectonic.bedrock_family) == BedrockFamily::Limestone && spring)
    {
        habitat |= HABITAT_CAVE_OUTLET;
    }
    if zonal == BIOME_MOUNTAINS || terrain.eroded_elevation > tree_line {
        habitat |= HABITAT_ALPINE;
    }
    if tectonic.volcanic_history != 0 || terrain.volcanic_contribution > 1.5 {
        habitat |= HABITAT_VOLCANIC_SOIL;
    }
    if ground.freeze_flags & FREEZE_PERMAFROST != 0 {
        habitat |= HABITAT_PERMAFROST;
    }

    let mut edaphic = 0u16;
    if ground.soil_depth_decimeters < 5 {
        edaphic |= EDAPHIC_SHALLOW_ROCK;
    }
    if BedrockFamily::from_id(tectonic.bedrock_family) == BedrockFamily::Limestone {
        edaphic |= EDAPHIC_LIMESTONE;
    }
    if habitat & HABITAT_BEACH_DUNE != 0 || (ground.sand > 180 && arid) {
        edaphic |= EDAPHIC_DUNE;
    }
    if habitat & HABITAT_VOLCANIC_SOIL != 0 {
        edaphic |= EDAPHIC_VOLCANIC;
    }
    if hydro.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0 || hydro.deposition_centiblocks > 50 {
        edaphic |= EDAPHIC_ALLUVIAL;
    }
    if slope > 11.0 {
        edaphic |= EDAPHIC_STEEP;
    }
    if arid && warmest > 12.0 && climate.precipitation_seasonality > 0.42 {
        edaphic |= EDAPHIC_FIRE_FAVORED;
    }
    if habitat & HABITAT_PERMAFROST != 0 {
        edaphic |= EDAPHIC_PERMAFROST;
    }

    let water_factor = (climate.mean_precipitation
        / climate.potential_evapotranspiration.max(220.0))
    .clamp(0.0, 1.6);
    let energy = ((warmest + 8.0) / 30.0).clamp(0.0, 1.0);
    let fertility = f32::from(ground.baseline_fertility) / 255.0;
    let salinity_penalty = 1.0 - f32::from(ground.soil_salinity) / 300.0;
    let habitat_bonus = if habitat & (HABITAT_RIPARIAN | HABITAT_OASIS | HABITAT_WETLAND) != 0 {
        0.28
    } else {
        0.0
    };
    let vegetation =
        ((water_factor.min(1.0) * 0.44 + energy * 0.30 + fertility * 0.26 + habitat_bonus)
            * salinity_penalty.clamp(0.0, 1.0)
            * 255.0)
            .clamp(0.0, 255.0) as u8;
    let succession = (f32::from(vegetation) * 0.65
        + f32::from(ground.organic) * 0.22
        + if edaphic & EDAPHIC_FIRE_FAVORED != 0 {
            -28.0
        } else {
            18.0
        })
    .clamp(0.0, 255.0) as u8;

    BiomeCell {
        baseline_biome: zonal,
        edaphic_flags: edaphic,
        habitat_flags: habitat,
        vegetation_potential: vegetation,
        tree_line_y: tree_line as u8,
        succession_potential: succession,
        country_id: 0,
        heart_assignment: 0,
    }
}

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

fn choose_country_seeds(
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

fn crossing_cost(
    from: usize,
    to: usize,
    side: u16,
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
) -> f32 {
    let a = terrain.values()[from];
    let b = terrain.values()[to];
    let ah = hydrology.values()[from];
    let bh = hydrology.values()[to];
    let physical = geodesic_distance(
        AtlasPos::from_index(from, side)
            .expect("from index")
            .center(side),
        AtlasPos::from_index(to, side)
            .expect("to index")
            .center(side),
    ) as f32;
    let watershed =
        if ah.watershed_id != 0 && bh.watershed_id != 0 && ah.watershed_id != bh.watershed_id {
            92.0
        } else {
            0.0
        };
    let crest = (a.eroded_elevation - b.eroded_elevation).abs() * 0.9
        + (f32::from(ground.values()[from].erosion_susceptibility)
            + f32::from(ground.values()[to].erosion_susceptibility))
            * 0.045;
    let river = if (ah.flags | bh.flags) & HYDRO_RIVER != 0
        && (ah.stream_order.max(bh.stream_order) >= 2
            || ah.mean_discharge.max(bh.mean_discharge) > 45.0)
    {
        36.0
    } else {
        0.0
    };
    physical + watershed + crest + river
}

fn partition_countries(
    side: u16,
    seeds: &[usize],
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
) -> (Vec<u16>, Vec<f32>) {
    let mut owner = vec![0u16; terrain.len()];
    let mut distance = vec![f32::INFINITY; terrain.len()];
    let mut queue = BinaryHeap::new();
    let mut watershed_nuclei = BTreeMap::<u32, BTreeSet<u16>>::new();
    for (offset, &index) in seeds.iter().enumerate() {
        let country = (offset + 1) as u16;
        let watershed = hydrology.values()[index].watershed_id;
        if watershed != 0 {
            watershed_nuclei
                .entry(watershed)
                .or_default()
                .insert(country);
        }
        owner[index] = country;
        distance[index] = 0.0;
        queue.push(QueueEntry {
            cost: 0.0,
            index,
            country,
        });
    }
    while let Some(entry) = queue.pop() {
        if entry.cost > distance[entry.index] + 0.001 || owner[entry.index] != entry.country {
            continue;
        }
        let pos = AtlasPos::from_index(entry.index, side).expect("atlas index");
        for neighbor in pos.neighbors4(side) {
            let next = neighbor.index(side);
            if terrain.values()[next].eroded_elevation <= SEA_LEVEL as f32
                || terrain.values()[next].landmass_id != terrain.values()[entry.index].landmass_id
            {
                continue;
            }
            let next_watershed = hydrology.values()[next].watershed_id;
            if next_watershed != 0
                && watershed_nuclei
                    .get(&next_watershed)
                    .is_some_and(|countries| !countries.contains(&entry.country))
            {
                // A drainage basin with its own country nucleus cannot be
                // swallowed from across its divide. Multiple nuclei inside
                // a very large basin may still partition it normally.
                continue;
            }
            let candidate =
                entry.cost + crossing_cost(entry.index, next, side, terrain, hydrology, ground);
            if candidate + 0.001 < distance[next]
                || ((candidate - distance[next]).abs() <= 0.001 && entry.country < owner[next])
            {
                distance[next] = candidate;
                owner[next] = entry.country;
                queue.push(QueueEntry {
                    cost: candidate,
                    index: next,
                    country: entry.country,
                });
            }
        }
    }
    // Flow routing may join a watershed diagonally at a cube vertex while
    // countries deliberately use four-neighbor travel. Such a one-cell
    // fragment has no path to its own protected nucleus. Fill only cells the
    // protected pass could not reach, preserving every established border
    // and exact land coverage.
    if owner.iter().copied().enumerate().any(|(index, country)| {
        country == 0 && terrain.values()[index].eroded_elevation > SEA_LEVEL as f32
    }) {
        let mut fallback = std::collections::VecDeque::new();
        for (index, country) in owner.iter().copied().enumerate() {
            if country != 0 {
                fallback.push_back(index);
            }
        }
        while let Some(index) = fallback.pop_front() {
            let pos = AtlasPos::from_index(index, side).expect("atlas fallback index");
            for neighbor in pos.neighbors4(side) {
                let next = neighbor.index(side);
                if owner[next] != 0
                    || terrain.values()[next].eroded_elevation <= SEA_LEVEL as f32
                    || terrain.values()[next].landmass_id != terrain.values()[index].landmass_id
                {
                    continue;
                }
                owner[next] = owner[index];
                distance[next] =
                    distance[index] + crossing_cost(index, next, side, terrain, hydrology, ground);
                fallback.push_back(next);
            }
        }
    }
    (owner, distance)
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
fn heart_score(
    index: usize,
    dominant: u8,
    wetland_country: bool,
    side: u16,
    seed_index: usize,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &AtlasGrid<TerrainCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    ground: &AtlasGrid<GroundCell>,
    biomes: &AtlasGrid<BiomeCell>,
) -> f32 {
    let cell = biomes.values()[index];
    let habitat = cell.habitat_flags;
    if habitat & (HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT) != 0 {
        return f32::NEG_INFINITY;
    }
    let distance = geodesic_distance(
        AtlasPos::from_index(index, side)
            .expect("candidate")
            .center(side),
        AtlasPos::from_index(seed_index, side)
            .expect("seed")
            .center(side),
    ) as f32;
    let mut score = 180.0 - distance * 0.12 + f32::from(cell.vegetation_potential) * 0.28;
    match dominant {
        BIOME_FOREST | BIOME_JUNGLE | BIOME_TAIGA => {
            score += f32::from(ground.values()[index].organic) * 0.42;
            score += if habitat & HABITAT_RIPARIAN != 0 {
                32.0
            } else {
                0.0
            };
        }
        BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS => {
            score += if habitat & HABITAT_OASIS != 0 {
                260.0
            } else {
                0.0
            };
            score += if habitat & HABITAT_SPRING != 0 {
                130.0
            } else {
                0.0
            };
            score += if habitat & HABITAT_RIPARIAN != 0 {
                74.0
            } else {
                0.0
            };
            score += f32::from(ground.values()[index].soil_depth_decimeters) * -0.8;
        }
        BIOME_PLAINS | BIOME_SAVANNA => {
            score += if habitat & (HABITAT_RIPARIAN | HABITAT_FLOODPLAIN) != 0 {
                66.0
            } else {
                0.0
            };
        }
        BIOME_MOUNTAINS => {
            score += (terrain.values()[index].eroded_elevation - SEA_LEVEL as f32) * 0.7;
            score +=
                if hydrology.values()[index].stream_order <= 1 && habitat & HABITAT_RIPARIAN != 0 {
                    72.0
                } else {
                    0.0
                };
        }
        BIOME_TUNDRA | BIOME_ARCTIC => {
            score += (terrain.values()[index].eroded_elevation - SEA_LEVEL as f32) * 0.25;
            score += if habitat & HABITAT_PERMAFROST != 0 {
                28.0
            } else {
                0.0
            };
        }
        _ => {}
    }
    if wetland_country {
        score += if habitat & HABITAT_WETLAND == 0 {
            72.0
        } else {
            24.0
        };
        score += if habitat & HABITAT_RIPARIAN != 0 {
            24.0
        } else {
            0.0
        };
    }
    score + geometry.values()[index].latitude_radians.cos().abs() * 0.001
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
