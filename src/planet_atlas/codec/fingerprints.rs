//! Deterministic immutable-layer fingerprints in persisted field order.

use crate::planet_atlas::codec::primitives::{
    put_f32, put_i16, put_i32, put_u8, put_u16, put_u32, put_u64,
};
use crate::planet_atlas::identity::stable_hash;
use crate::planet_atlas::{
    AtlasGrid, BiomeCell, ClimateCell, GeometryCell, GroundCell, HydrologyCell, PlanetAtlas,
    ResourceCell, TectonicCell, TerrainCell,
};

impl PlanetAtlas {
    pub fn immutable_fingerprint(&self, layer: &str) -> Option<u64> {
        match layer {
            "geometry" => Some(fingerprint_geometry(&self.genesis.geometry)),
            "tectonics" => Some(fingerprint_tectonics(&self.genesis.tectonics)),
            "terrain" => Some(fingerprint_terrain(&self.genesis.terrain)),
            "climate" => Some(fingerprint_climate(&self.genesis.climate)),
            "hydrology" => Some(fingerprint_hydrology(&self.genesis.hydrology)),
            "ground" => Some(fingerprint_ground(&self.genesis.ground)),
            "biomes" => Some(fingerprint_biomes(&self.genesis.biomes)),
            "resources" => Some(fingerprint_resources(&self.genesis.resources)),
            _ => None,
        }
    }
}

pub(in crate::planet_atlas) fn fingerprint_geometry(grid: &AtlasGrid<GeometryCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 20);
    for cell in grid.values() {
        for value in cell.unit_direction {
            put_f32(&mut out, value);
        }
        put_f32(&mut out, cell.latitude_radians);
        put_f32(&mut out, cell.physical_area);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_tectonics(grid: &AtlasGrid<TectonicCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 39);
    for cell in grid.values() {
        put_u16(&mut out, cell.plate_id);
        put_u8(&mut out, cell.boundary as u8);
        put_u8(&mut out, cell.boundary_detail as u8);
        put_u16(&mut out, cell.neighbor_plate);
        put_f32(&mut out, cell.boundary_strength);
        put_u16(&mut out, cell.boundary_distance);
        for value in cell.boundary_strike {
            put_i16(&mut out, value);
        }
        put_u16(&mut out, cell.continental_crust);
        put_u16(&mut out, cell.craton_id);
        put_u16(&mut out, cell.crust_age);
        put_u16(&mut out, cell.oceanic_age);
        put_u16(&mut out, cell.crust_thickness);
        put_u16(&mut out, cell.bedrock_family);
        put_u16(&mut out, cell.geological_province);
        put_u16(&mut out, cell.stratigraphic_stack);
        put_u8(&mut out, cell.metamorphic_grade);
        put_u16(&mut out, cell.fault_intensity);
        put_u8(&mut out, cell.volcanic_history);
        put_u8(&mut out, cell.sediment_basin as u8);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_terrain(grid: &AtlasGrid<TerrainCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 22);
    for cell in grid.values() {
        put_f32(&mut out, cell.base_elevation);
        put_f32(&mut out, cell.eroded_elevation);
        put_f32(&mut out, cell.tectonic_contribution);
        put_f32(&mut out, cell.volcanic_contribution);
        put_f32(&mut out, cell.dynamic_topography);
        put_u16(&mut out, cell.landmass_id);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_climate(grid: &AtlasGrid<ClimateCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 120);
    for cell in grid.values() {
        put_f32(&mut out, cell.mean_temperature);
        put_f32(&mut out, cell.seasonality);
        put_f32(&mut out, cell.ocean_temperature_anomaly);
        put_f32(&mut out, cell.continentality);
        put_f32(&mut out, cell.prevailing_wind[0]);
        put_f32(&mut out, cell.prevailing_wind[1]);
        put_f32(&mut out, cell.ocean_current[0]);
        put_f32(&mut out, cell.ocean_current[1]);
        put_f32(&mut out, cell.mean_atmospheric_moisture);
        put_f32(&mut out, cell.mean_precipitation);
        put_f32(&mut out, cell.precipitation_seasonality);
        put_f32(&mut out, cell.potential_evapotranspiration);
        put_f32(&mut out, cell.aridity);
        put_f32(&mut out, cell.snow_persistence);
        for value in cell.seasonal_temperature {
            put_f32(&mut out, value);
        }
        for value in cell.seasonal_precipitation {
            put_f32(&mut out, value);
        }
        for wind in cell.seasonal_wind {
            put_f32(&mut out, wind[0]);
            put_f32(&mut out, wind[1]);
        }
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_hydrology(grid: &AtlasGrid<HydrologyCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 91);
    for cell in grid.values() {
        put_u32(&mut out, cell.drainage_receiver);
        put_u32(&mut out, cell.watershed_id);
        put_u16(&mut out, cell.ocean_basin_id);
        put_u32(&mut out, cell.lake_basin_id);
        put_f32(&mut out, cell.spill_elevation);
        put_f32(&mut out, cell.filled_elevation);
        put_f32(&mut out, cell.mean_runoff);
        for value in cell.seasonal_runoff_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, cell.mean_discharge);
        for value in cell.seasonal_discharge_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, cell.catchment_area);
        put_f32(&mut out, cell.channel_bed_elevation);
        put_f32(&mut out, cell.water_surface_elevation);
        put_u16(&mut out, cell.channel_width_centiblocks);
        put_u16(&mut out, cell.channel_depth_centiblocks);
        put_u16(&mut out, cell.sediment_energy);
        put_u64(&mut out, cell.baseline_water_units);
        put_i32(&mut out, cell.voxel_volume_residual);
        put_u32(&mut out, cell.river_id);
        put_i16(&mut out, cell.erosion_centiblocks);
        put_i16(&mut out, cell.deposition_centiblocks);
        put_u16(&mut out, cell.seasonal_level_range_centiblocks);
        put_u16(&mut out, cell.flags);
        put_u8(&mut out, cell.stream_order);
        put_u8(&mut out, cell.salinity);
        put_u8(&mut out, cell.water_body as u8);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_ground(grid: &AtlasGrid<GroundCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 23);
    for cell in grid.values() {
        put_u16(&mut out, cell.soil_parent_material);
        put_u32(&mut out, cell.aquifer_capacity);
        put_u16(&mut out, cell.aquifer_permeability);
        put_u16(&mut out, cell.porosity);
        put_f32(&mut out, cell.baseline_groundwater_head);
        put_u8(&mut out, cell.soil_depth_decimeters);
        put_u8(&mut out, cell.sand);
        put_u8(&mut out, cell.silt);
        put_u8(&mut out, cell.organic);
        put_u8(&mut out, cell.baseline_fertility);
        put_u8(&mut out, cell.drainage);
        put_u8(&mut out, cell.soil_salinity);
        put_u8(&mut out, cell.freeze_flags);
        put_u8(&mut out, cell.erosion_susceptibility);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_biomes(grid: &AtlasGrid<BiomeCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 14);
    for cell in grid.values() {
        put_u8(&mut out, cell.baseline_biome);
        put_u16(&mut out, cell.edaphic_flags);
        put_u32(&mut out, cell.habitat_flags);
        put_u8(&mut out, cell.vegetation_potential);
        put_u8(&mut out, cell.tree_line_y);
        put_u8(&mut out, cell.succession_potential);
        put_u16(&mut out, cell.country_id);
        put_u16(&mut out, cell.heart_assignment);
    }
    stable_hash(&out)
}

pub(in crate::planet_atlas) fn fingerprint_resources(grid: &AtlasGrid<ResourceCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 6);
    for cell in grid.values() {
        put_u32(&mut out, cell.deposit_site_ref);
        put_u16(&mut out, cell.deposit_site_count);
    }
    stable_hash(&out)
}
