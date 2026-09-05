//! One ordered pass over dense layers and finite water reservoirs.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{geodesic_distance};
use crate::planet_atlas::{AtlasPos, BedrockFamily, BoundaryClass, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT, HABITAT_NAMES, HABITAT_OASIS, HABITAT_RIPARIAN, HABITAT_SPRING, HABITAT_WETLAND, HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_FLOODPLAIN, HYDRO_RIVER, HYDRO_WETLAND, PlanetAtlas};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct DenseCensus {
    pub(super) physical_area: f64,
    pub(super) land_area: f64,
    pub(super) ocean_area: f64,
    pub(super) elevation_histogram: Vec<u64>,
    pub(super) boundary_cells: BTreeMap<String, u64>,
    pub(super) detailed_boundary_cells: BTreeMap<String, u64>,
    pub(super) bedrock_areas: BTreeMap<String, f64>,
    pub(super) oceanic_age_histogram: Vec<u64>,
    pub(super) climate_zone_areas: BTreeMap<String, f64>,
    pub(super) temperature_area_sum: f64,
    pub(super) minimum_temperature: f32,
    pub(super) maximum_temperature: f32,
    pub(super) precipitation_area_sum: f64,
    pub(super) maximum_precipitation: f32,
    pub(super) aridity_area_sum: f64,
    pub(super) maximum_aridity: f32,
    pub(super) biome_areas: BTreeMap<u16, f64>,
    pub(super) biome_area_by_latitude_band: BTreeMap<String, f64>,
    pub(super) biome_area_by_continent: BTreeMap<String, f64>,
    pub(super) biome_area_by_elevation_band: BTreeMap<String, f64>,
    pub(super) biome_area_by_water_availability: BTreeMap<String, f64>,
    pub(super) habitat_areas: BTreeMap<String, f64>,
    pub(super) watersheds: BTreeSet<u32>,
    pub(super) lakes: BTreeSet<u32>,
    pub(super) provinces: BTreeSet<u32>,
    pub(super) hearts: BTreeSet<u16>,
    pub(super) drainage_edge_count: u64,
    pub(super) river_length_blocks: f64,
    pub(super) receiver_histogram: Vec<u64>,
    pub(super) aquifer_storage_units: u128,
    pub(super) freshwater_units: u128,
    pub(super) total_water_units: u128,
    pub(super) floodplain_cells: u64,
    pub(super) wetland_cells: u64,
    pub(super) delta_cells: u64,
    pub(super) estuary_cells: u64,
    pub(super) resource_sites: Vec<AtlasPos>,
}

impl DenseCensus {
    pub(super) fn collect(atlas: &PlanetAtlas) -> Self {
        let side = atlas.side();
        let mut physical_area = 0.0f64;
        let mut land_area = 0.0f64;
        let mut ocean_area = 0.0f64;
        let mut elevation_histogram = vec![0u64; 16];
        let mut boundary_cells = BTreeMap::from([
            ("interior".to_string(), 0),
            ("convergent".to_string(), 0),
            ("divergent".to_string(), 0),
            ("transform".to_string(), 0),
        ]);
        let mut detailed_boundary_cells = BTreeMap::<String, u64>::new();
        let mut bedrock_areas = BTreeMap::<String, f64>::new();
        let mut oceanic_age_histogram = vec![0u64; 12];
        let mut climate_zone_areas = BTreeMap::<String, f64>::new();
        let mut temperature_area_sum = 0.0f64;
        let mut minimum_temperature = f32::INFINITY;
        let mut maximum_temperature = f32::NEG_INFINITY;
        let mut precipitation_area_sum = 0.0f64;
        let mut maximum_precipitation = 0.0f32;
        let mut aridity_area_sum = 0.0f64;
        let mut maximum_aridity = 0.0f32;
        let mut biome_areas = BTreeMap::<u16, f64>::new();
        let mut biome_area_by_latitude_band = BTreeMap::<String, f64>::new();
        let mut biome_area_by_continent = BTreeMap::<String, f64>::new();
        let mut biome_area_by_elevation_band = BTreeMap::<String, f64>::new();
        let mut biome_area_by_water_availability = BTreeMap::<String, f64>::new();
        let mut habitat_areas = BTreeMap::<String, f64>::new();
        let mut watersheds = BTreeSet::new();
        let mut lakes = BTreeSet::new();
        let mut provinces = BTreeSet::new();
        let mut hearts = BTreeSet::new();
        let mut drainage_edge_count = 0u64;
        let mut river_length_blocks = 0.0f64;
        let mut receiver_histogram = vec![0u64; 8];
        let mut aquifer_storage_units = 0u128;
        let mut freshwater_units = 0u128;
        let mut total_water_units = 0u128;
        let mut floodplain_cells = 0u64;
        let mut wetland_cells = 0u64;
        let mut delta_cells = 0u64;
        let mut estuary_cells = 0u64;
        let mut resource_sites = Vec::new();

        for (pos, geometry) in atlas.genesis.geometry.iter() {
            let area = f64::from(geometry.physical_area);
            physical_area += area;
            let terrain = atlas.genesis.terrain.get(pos).expect("matching atlas grids");
            let tectonics = atlas
                .genesis
                .tectonics
                .get(pos)
                .expect("matching atlas grids");
            let climate = atlas.genesis.climate.get(pos).expect("matching atlas grids");
            let hydrology = atlas
                .genesis
                .hydrology
                .get(pos)
                .expect("matching atlas grids");
            let biome = atlas.genesis.biomes.get(pos).expect("matching atlas grids");
            let resource = atlas
                .genesis
                .resources
                .get(pos)
                .expect("matching atlas grids");
            let dynamic = atlas.dynamic.cells.get(pos).expect("matching atlas grids");
            let water = atlas
                .water_cycle
                .cells
                .get(pos)
                .expect("matching water grid");
            let land = terrain.eroded_elevation > SEA_LEVEL as f32;
            if land {
                land_area += area;
            } else {
                ocean_area += area;
            }
            let elevation_bin = (((terrain.eroded_elevation - 0.0) / 256.0) * 16.0)
                .floor()
                .clamp(0.0, 15.0) as usize;
            elevation_histogram[elevation_bin] += 1;
            let boundary = match tectonics.boundary {
                BoundaryClass::Interior => "interior",
                BoundaryClass::Convergent => "convergent",
                BoundaryClass::Divergent => "divergent",
                BoundaryClass::Transform => "transform",
            };
            *boundary_cells.get_mut(boundary).expect("known boundary") += 1;
            *detailed_boundary_cells
                .entry(format!("{:?}", tectonics.boundary_detail).to_lowercase())
                .or_default() += 1;
            *bedrock_areas
                .entry(
                    BedrockFamily::from_id(tectonics.bedrock_family)
                        .label()
                        .to_string(),
                )
                .or_default() += area;
            if tectonics.continental_crust < 32_768 {
                let bin = (usize::from(tectonics.oceanic_age) / 20).min(11);
                oceanic_age_histogram[bin] += 1;
            }
            let climate_zone = if climate.aridity > 1.2 {
                "dry"
            } else if climate.mean_temperature < 0.0 {
                "polar"
            } else if climate.mean_temperature < 18.0 {
                "temperate"
            } else {
                "tropical"
            };
            *climate_zone_areas.entry(climate_zone.into()).or_default() += area;
            temperature_area_sum += f64::from(climate.mean_temperature) * area;
            minimum_temperature = minimum_temperature.min(climate.mean_temperature);
            maximum_temperature = maximum_temperature.max(climate.mean_temperature);
            precipitation_area_sum += f64::from(climate.mean_precipitation) * area;
            maximum_precipitation = maximum_precipitation.max(climate.mean_precipitation);
            aridity_area_sum += f64::from(climate.aridity) * area;
            maximum_aridity = maximum_aridity.max(climate.aridity);
            *biome_areas
                .entry(u16::from(biome.baseline_biome))
                .or_default() += area;
            let latitude_band = ((geometry.latitude_radians.to_degrees() + 90.0) / 15.0)
                .floor()
                .clamp(0.0, 11.0) as i32;
            *biome_area_by_latitude_band
                .entry(format!(
                    "biome_{}_band_{latitude_band}",
                    biome.baseline_biome
                ))
                .or_default() += area;
            if land {
                *biome_area_by_continent
                    .entry(format!(
                        "biome_{}_continent_{}",
                        biome.baseline_biome, terrain.landmass_id
                    ))
                    .or_default() += area;
            }
            let elevation_band = ((terrain.eroded_elevation - SEA_LEVEL as f32) / 32.0)
                .floor()
                .clamp(-4.0, 7.0) as i32;
            *biome_area_by_elevation_band
                .entry(format!(
                    "biome_{}_elevation_{elevation_band}",
                    biome.baseline_biome
                ))
                .or_default() += area;
            let water_class = if biome.habitat_flags
                & (HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT)
                != 0
            {
                "aquatic"
            } else if biome.habitat_flags
                & (HABITAT_RIPARIAN | HABITAT_WETLAND | HABITAT_OASIS | HABITAT_SPRING)
                != 0
            {
                "water_connected"
            } else if climate.aridity >= 1.02 {
                "water_limited"
            } else {
                "mesic"
            };
            *biome_area_by_water_availability
                .entry(format!("biome_{}_{water_class}", biome.baseline_biome))
                .or_default() += area;
            for (flag, name) in HABITAT_NAMES {
                if biome.habitat_flags & *flag != 0 {
                    *habitat_areas.entry((*name).to_string()).or_default() += area;
                }
            }
            if hydrology.watershed_id != 0 {
                watersheds.insert(hydrology.watershed_id);
            }
            if hydrology.lake_basin_id != 0 {
                lakes.insert(hydrology.lake_basin_id);
            }
            if hydrology.drainage_receiver != u32::MAX {
                drainage_edge_count += 1;
                if hydrology.flags & HYDRO_RIVER != 0
                    && let Some(receiver) =
                        AtlasPos::from_index(hydrology.drainage_receiver as usize, side)
                {
                    let distance = geodesic_distance(pos.center(side), receiver.center(side));
                    river_length_blocks += distance;
                    let bin = (distance / f64::from(atlas.cell_blocks()))
                        .floor()
                        .clamp(0.0, 7.0) as usize;
                    receiver_histogram[bin] += 1;
                }
            }
            floodplain_cells += u64::from(u8::from(hydrology.flags & HYDRO_FLOODPLAIN != 0));
            wetland_cells += u64::from(u8::from(hydrology.flags & HYDRO_WETLAND != 0));
            delta_cells += u64::from(u8::from(hydrology.flags & HYDRO_DELTA != 0));
            estuary_cells += u64::from(u8::from(hydrology.flags & HYDRO_ESTUARY != 0));
            if biome.country_id != 0 {
                provinces.insert(u32::from(biome.country_id));
            }
            if biome.heart_assignment != 0 {
                hearts.insert(biome.heart_assignment);
            }
            if resource.deposit_site_ref != 0 {
                resource_sites.push(pos);
            }
            aquifer_storage_units += u128::from(water.groundwater.water_hu);
            freshwater_units += u128::from(dynamic.cloud_water)
                + u128::from(water.soil.water_hu)
                + u128::from(water.snow.water_hu)
                + u128::from(water.groundwater.water_hu)
                + u128::from(water.runoff.water_hu);
            total_water_units += u128::from(dynamic.atmospheric_vapor)
                + u128::from(dynamic.cloud_water)
                + u128::from(water.soil.water_hu)
                + u128::from(water.snow.water_hu)
                + u128::from(water.groundwater.water_hu)
                + u128::from(water.runoff.water_hu);
        }
        for reservoir in &atlas.water_cycle.reservoirs {
            total_water_units +=
                u128::from(reservoir.coarse.water_hu) + u128::from(reservoir.committed.water_hu);
            if reservoir.coarse.salinity() < 64 {
                freshwater_units += u128::from(reservoir.coarse.water_hu);
            }
        }
        for aquifer in &atlas.water_cycle.aquifers {
            aquifer_storage_units += u128::from(aquifer.mass.water_hu);
            total_water_units += u128::from(aquifer.mass.water_hu);
        }
        Self {
            physical_area,
            land_area,
            ocean_area,
            elevation_histogram,
            boundary_cells,
            detailed_boundary_cells,
            bedrock_areas,
            oceanic_age_histogram,
            climate_zone_areas,
            temperature_area_sum,
            minimum_temperature,
            maximum_temperature,
            precipitation_area_sum,
            maximum_precipitation,
            aridity_area_sum,
            maximum_aridity,
            biome_areas,
            biome_area_by_latitude_band,
            biome_area_by_continent,
            biome_area_by_elevation_band,
            biome_area_by_water_availability,
            habitat_areas,
            watersheds,
            lakes,
            provinces,
            hearts,
            drainage_edge_count,
            river_length_blocks,
            receiver_histogram,
            aquifer_storage_units,
            freshwater_units,
            total_water_units,
            floodplain_cells,
            wetland_cells,
            delta_cells,
            estuary_cells,
            resource_sites,
        }
    }
}
