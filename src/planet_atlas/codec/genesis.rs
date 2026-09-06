//! Explicit fixed-width immutable cell encoding and decoding.

use crate::planet_atlas::codec::primitives::{
    ByteReader, put_f32, put_i16, put_i32, put_u8, put_u16, put_u32, put_u64,
};
use crate::planet_atlas::codec::{FILE_HEADER_BYTES, GENESIS_RECORD_BYTES};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::{
    AtlasError, AtlasGrid, BasinKind, BiomeCell, BoundaryClass, ClimateCell, DetailedBoundary,
    GenesisLayers, GeometryCell, GroundCell, HydrologyCell, ResourceCell, TectonicCell,
    TerrainCell, WaterBodyKind,
};

pub(in crate::planet_atlas) fn decode_genesis(
    side: u16,
    payload: &[u8],
) -> Result<GenesisLayers, AtlasError> {
    let count = atlas_count(side)?;
    if payload.len() != count * GENESIS_RECORD_BYTES {
        return Err(AtlasError::Corrupt("genesis payload width mismatch".into()));
    }
    let mut reader = ByteReader::new(payload);
    let mut geometry = Vec::with_capacity(count);
    let mut tectonics = Vec::with_capacity(count);
    let mut terrain = Vec::with_capacity(count);
    let mut climate = Vec::with_capacity(count);
    let mut hydrology = Vec::with_capacity(count);
    let mut ground = Vec::with_capacity(count);
    let mut biomes = Vec::with_capacity(count);
    let mut resources = Vec::with_capacity(count);
    for _ in 0..count {
        geometry.push(GeometryCell {
            unit_direction: [reader.f32()?, reader.f32()?, reader.f32()?],
            latitude_radians: reader.f32()?,
            physical_area: reader.f32()?,
        });
        tectonics.push(TectonicCell {
            plate_id: reader.u16()?,
            boundary: BoundaryClass::from_u8(reader.u8()?)?,
            boundary_detail: DetailedBoundary::from_u8(reader.u8()?)?,
            neighbor_plate: reader.u16()?,
            boundary_strength: reader.f32()?,
            boundary_distance: reader.u16()?,
            boundary_strike: [reader.i16()?, reader.i16()?, reader.i16()?],
            continental_crust: reader.u16()?,
            craton_id: reader.u16()?,
            crust_age: reader.u16()?,
            oceanic_age: reader.u16()?,
            crust_thickness: reader.u16()?,
            bedrock_family: reader.u16()?,
            geological_province: reader.u16()?,
            stratigraphic_stack: reader.u16()?,
            metamorphic_grade: reader.u8()?,
            fault_intensity: reader.u16()?,
            volcanic_history: reader.u8()?,
            sediment_basin: BasinKind::from_u8(reader.u8()?)?,
        });
        terrain.push(TerrainCell {
            base_elevation: reader.f32()?,
            eroded_elevation: reader.f32()?,
            tectonic_contribution: reader.f32()?,
            volcanic_contribution: reader.f32()?,
            dynamic_topography: reader.f32()?,
            landmass_id: reader.u16()?,
        });
        climate.push(ClimateCell {
            mean_temperature: reader.f32()?,
            seasonality: reader.f32()?,
            ocean_temperature_anomaly: reader.f32()?,
            continentality: reader.f32()?,
            prevailing_wind: [reader.f32()?, reader.f32()?],
            ocean_current: [reader.f32()?, reader.f32()?],
            mean_atmospheric_moisture: reader.f32()?,
            mean_precipitation: reader.f32()?,
            precipitation_seasonality: reader.f32()?,
            potential_evapotranspiration: reader.f32()?,
            aridity: reader.f32()?,
            snow_persistence: reader.f32()?,
            seasonal_temperature: [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?],
            seasonal_precipitation: [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?],
            seasonal_wind: [
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
            ],
        });
        hydrology.push(HydrologyCell {
            drainage_receiver: reader.u32()?,
            watershed_id: reader.u32()?,
            ocean_basin_id: reader.u16()?,
            lake_basin_id: reader.u32()?,
            spill_elevation: reader.f32()?,
            filled_elevation: reader.f32()?,
            mean_runoff: reader.f32()?,
            seasonal_runoff_fraction: [reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?],
            mean_discharge: reader.f32()?,
            seasonal_discharge_fraction: [
                reader.u16()?,
                reader.u16()?,
                reader.u16()?,
                reader.u16()?,
            ],
            catchment_area: reader.f32()?,
            channel_bed_elevation: reader.f32()?,
            water_surface_elevation: reader.f32()?,
            channel_width_centiblocks: reader.u16()?,
            channel_depth_centiblocks: reader.u16()?,
            sediment_energy: reader.u16()?,
            baseline_water_units: reader.u64()?,
            voxel_volume_residual: reader.i32()?,
            river_id: reader.u32()?,
            erosion_centiblocks: reader.i16()?,
            deposition_centiblocks: reader.i16()?,
            seasonal_level_range_centiblocks: reader.u16()?,
            flags: reader.u16()?,
            stream_order: reader.u8()?,
            salinity: reader.u8()?,
            water_body: WaterBodyKind::from_u8(reader.u8()?)?,
        });
        ground.push(GroundCell {
            soil_parent_material: reader.u16()?,
            aquifer_capacity: reader.u32()?,
            aquifer_permeability: reader.u16()?,
            porosity: reader.u16()?,
            baseline_groundwater_head: reader.f32()?,
            soil_depth_decimeters: reader.u8()?,
            sand: reader.u8()?,
            silt: reader.u8()?,
            organic: reader.u8()?,
            baseline_fertility: reader.u8()?,
            drainage: reader.u8()?,
            soil_salinity: reader.u8()?,
            freeze_flags: reader.u8()?,
            erosion_susceptibility: reader.u8()?,
        });
        biomes.push(BiomeCell {
            baseline_biome: reader.u8()?,
            edaphic_flags: reader.u16()?,
            habitat_flags: reader.u32()?,
            vegetation_potential: reader.u8()?,
            tree_line_y: reader.u8()?,
            succession_potential: reader.u8()?,
            country_id: reader.u16()?,
            heart_assignment: reader.u16()?,
        });
        resources.push(ResourceCell {
            deposit_site_ref: reader.u32()?,
            deposit_site_count: reader.u16()?,
        });
    }
    Ok(GenesisLayers {
        geometry: AtlasGrid::from_values(side, geometry)?,
        tectonics: AtlasGrid::from_values(side, tectonics)?,
        terrain: AtlasGrid::from_values(side, terrain)?,
        climate: AtlasGrid::from_values(side, climate)?,
        hydrology: AtlasGrid::from_values(side, hydrology)?,
        ground: AtlasGrid::from_values(side, ground)?,
        biomes: AtlasGrid::from_values(side, biomes)?,
        resources: AtlasGrid::from_values(side, resources)?,
    })
}

pub(in crate::planet_atlas) fn encode_genesis(
    genesis: &GenesisLayers,
) -> Result<Vec<u8>, AtlasError> {
    let count = genesis.geometry.len();
    let mut out = Vec::with_capacity(count * GENESIS_RECORD_BYTES + FILE_HEADER_BYTES);
    for index in 0..count {
        let geometry = genesis.geometry.values()[index];
        let tectonics = genesis.tectonics.values()[index];
        let terrain = genesis.terrain.values()[index];
        let climate = genesis.climate.values()[index];
        let hydrology = genesis.hydrology.values()[index];
        let ground = genesis.ground.values()[index];
        let biome = genesis.biomes.values()[index];
        let resources = genesis.resources.values()[index];
        for value in geometry.unit_direction {
            put_f32(&mut out, value);
        }
        put_f32(&mut out, geometry.latitude_radians);
        put_f32(&mut out, geometry.physical_area);
        put_u16(&mut out, tectonics.plate_id);
        put_u8(&mut out, tectonics.boundary as u8);
        put_u8(&mut out, tectonics.boundary_detail as u8);
        put_u16(&mut out, tectonics.neighbor_plate);
        put_f32(&mut out, tectonics.boundary_strength);
        put_u16(&mut out, tectonics.boundary_distance);
        for value in tectonics.boundary_strike {
            put_i16(&mut out, value);
        }
        put_u16(&mut out, tectonics.continental_crust);
        put_u16(&mut out, tectonics.craton_id);
        put_u16(&mut out, tectonics.crust_age);
        put_u16(&mut out, tectonics.oceanic_age);
        put_u16(&mut out, tectonics.crust_thickness);
        put_u16(&mut out, tectonics.bedrock_family);
        put_u16(&mut out, tectonics.geological_province);
        put_u16(&mut out, tectonics.stratigraphic_stack);
        put_u8(&mut out, tectonics.metamorphic_grade);
        put_u16(&mut out, tectonics.fault_intensity);
        put_u8(&mut out, tectonics.volcanic_history);
        put_u8(&mut out, tectonics.sediment_basin as u8);
        put_f32(&mut out, terrain.base_elevation);
        put_f32(&mut out, terrain.eroded_elevation);
        put_f32(&mut out, terrain.tectonic_contribution);
        put_f32(&mut out, terrain.volcanic_contribution);
        put_f32(&mut out, terrain.dynamic_topography);
        put_u16(&mut out, terrain.landmass_id);
        put_f32(&mut out, climate.mean_temperature);
        put_f32(&mut out, climate.seasonality);
        put_f32(&mut out, climate.ocean_temperature_anomaly);
        put_f32(&mut out, climate.continentality);
        put_f32(&mut out, climate.prevailing_wind[0]);
        put_f32(&mut out, climate.prevailing_wind[1]);
        put_f32(&mut out, climate.ocean_current[0]);
        put_f32(&mut out, climate.ocean_current[1]);
        put_f32(&mut out, climate.mean_atmospheric_moisture);
        put_f32(&mut out, climate.mean_precipitation);
        put_f32(&mut out, climate.precipitation_seasonality);
        put_f32(&mut out, climate.potential_evapotranspiration);
        put_f32(&mut out, climate.aridity);
        put_f32(&mut out, climate.snow_persistence);
        for value in climate.seasonal_temperature {
            put_f32(&mut out, value);
        }
        for value in climate.seasonal_precipitation {
            put_f32(&mut out, value);
        }
        for wind in climate.seasonal_wind {
            put_f32(&mut out, wind[0]);
            put_f32(&mut out, wind[1]);
        }
        put_u32(&mut out, hydrology.drainage_receiver);
        put_u32(&mut out, hydrology.watershed_id);
        put_u16(&mut out, hydrology.ocean_basin_id);
        put_u32(&mut out, hydrology.lake_basin_id);
        put_f32(&mut out, hydrology.spill_elevation);
        put_f32(&mut out, hydrology.filled_elevation);
        put_f32(&mut out, hydrology.mean_runoff);
        for value in hydrology.seasonal_runoff_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, hydrology.mean_discharge);
        for value in hydrology.seasonal_discharge_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, hydrology.catchment_area);
        put_f32(&mut out, hydrology.channel_bed_elevation);
        put_f32(&mut out, hydrology.water_surface_elevation);
        put_u16(&mut out, hydrology.channel_width_centiblocks);
        put_u16(&mut out, hydrology.channel_depth_centiblocks);
        put_u16(&mut out, hydrology.sediment_energy);
        put_u64(&mut out, hydrology.baseline_water_units);
        put_i32(&mut out, hydrology.voxel_volume_residual);
        put_u32(&mut out, hydrology.river_id);
        put_i16(&mut out, hydrology.erosion_centiblocks);
        put_i16(&mut out, hydrology.deposition_centiblocks);
        put_u16(&mut out, hydrology.seasonal_level_range_centiblocks);
        put_u16(&mut out, hydrology.flags);
        put_u8(&mut out, hydrology.stream_order);
        put_u8(&mut out, hydrology.salinity);
        put_u8(&mut out, hydrology.water_body as u8);
        put_u16(&mut out, ground.soil_parent_material);
        put_u32(&mut out, ground.aquifer_capacity);
        put_u16(&mut out, ground.aquifer_permeability);
        put_u16(&mut out, ground.porosity);
        put_f32(&mut out, ground.baseline_groundwater_head);
        put_u8(&mut out, ground.soil_depth_decimeters);
        put_u8(&mut out, ground.sand);
        put_u8(&mut out, ground.silt);
        put_u8(&mut out, ground.organic);
        put_u8(&mut out, ground.baseline_fertility);
        put_u8(&mut out, ground.drainage);
        put_u8(&mut out, ground.soil_salinity);
        put_u8(&mut out, ground.freeze_flags);
        put_u8(&mut out, ground.erosion_susceptibility);
        put_u8(&mut out, biome.baseline_biome);
        put_u16(&mut out, biome.edaphic_flags);
        put_u32(&mut out, biome.habitat_flags);
        put_u8(&mut out, biome.vegetation_potential);
        put_u8(&mut out, biome.tree_line_y);
        put_u8(&mut out, biome.succession_potential);
        put_u16(&mut out, biome.country_id);
        put_u16(&mut out, biome.heart_assignment);
        put_u32(&mut out, resources.deposit_site_ref);
        put_u16(&mut out, resources.deposit_site_count);
    }
    if out.len() != count * GENESIS_RECORD_BYTES {
        return Err(AtlasError::Corrupt(
            "internal genesis record width mismatch".into(),
        ));
    }
    Ok(out)
}
