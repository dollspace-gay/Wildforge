//! Layer dimensions, physical values, and complete snapshot consistency.

use crate::planet_atlas::codec::FILE_HEADER_BYTES;
use crate::planet_atlas::codec::dynamic::encode_dynamic;
use crate::planet_atlas::codec::genesis::encode_genesis;
use crate::planet_atlas::codec::models::{
    encode_biomes, encode_geology, encode_history, encode_hydrology,
};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::identity::stable_hash;
use crate::planet_atlas::manifest::validate_manifest;
use crate::planet_atlas::{
    ATLAS_HISTORY_VERSION, AtlasError, AtlasGrid, BIOME_OCEAN, BiomeCell, ClimateCell,
    DynamicLayers, GenesisLayers, GeometryCell, GroundCell, HydrologyCell, PlanetAtlas,
    ReservoirMass, TectonicCell, TerrainCell, dynamic_water_total, encode_water_cycle,
};
use glam::Vec3;

impl PlanetAtlas {
    pub fn validate(&self) -> Result<(), AtlasError> {
        validate_manifest(&self.manifest, false)?;
        validate_layers(self.side(), &self.genesis, &self.dynamic)?;
        if self.history.version != ATLAS_HISTORY_VERSION {
            return Err(AtlasError::UnsupportedVersion(format!(
                "history schema {} (supported {})",
                self.history.version, ATLAS_HISTORY_VERSION
            )));
        }
        let genesis = encode_genesis(&self.genesis)?;
        if self.manifest.genesis_bytes != (FILE_HEADER_BYTES + genesis.len()) as u64 {
            return Err(AtlasError::Corrupt(
                "manifest immutable byte count is inconsistent".into(),
            ));
        }
        if stable_hash(&genesis) != self.manifest.genesis_checksum {
            return Err(AtlasError::Corrupt(
                "immutable layer checksum does not match manifest".into(),
            ));
        }
        drop(genesis);
        let dynamic = encode_dynamic(&self.dynamic)?;
        if self.manifest.dynamic_bytes != (FILE_HEADER_BYTES + dynamic.len()) as u64 {
            return Err(AtlasError::Corrupt(
                "manifest dynamic byte count is inconsistent".into(),
            ));
        }
        if stable_hash(&dynamic) != self.manifest.dynamic_checksum {
            return Err(AtlasError::Corrupt(
                "dynamic layer checksum does not match manifest".into(),
            ));
        }
        drop(dynamic);
        let water_cycle = encode_water_cycle(&self.water_cycle)?;
        if self.manifest.water_cycle_bytes != (FILE_HEADER_BYTES + water_cycle.len()) as u64
            || stable_hash(&water_cycle) != self.manifest.water_cycle_checksum
        {
            return Err(AtlasError::Corrupt(
                "water-cycle state does not match the committed manifest".into(),
            ));
        }
        drop(water_cycle);
        self.water_cycle.validate(
            self.side(),
            ReservoirMass::fresh(dynamic_water_total(&self.dynamic) as u64),
        )?;
        if self.water_cycle.completed_surface_hours != self.dynamic.completed_climate_hours {
            return Err(AtlasError::Corrupt(format!(
                "atmosphere hour {} and water-cycle hour {} are not one atomic snapshot",
                self.dynamic.completed_climate_hours, self.water_cycle.completed_surface_hours,
            )));
        }
        let history = encode_history(&self.history)?;
        if stable_hash(history.as_bytes()) != self.manifest.history_checksum {
            return Err(AtlasError::Corrupt(
                "history checksum does not match manifest".into(),
            ));
        }
        self.geology.validate(
            self.side(),
            self.genesis.tectonics.values(),
            self.genesis.terrain.values(),
            self.genesis.resources.values(),
        )?;
        let geology = encode_geology(&self.geology)?;
        if geology.len() as u64 != self.manifest.geology_bytes
            || stable_hash(geology.as_bytes()) != self.manifest.geology_checksum
        {
            return Err(AtlasError::Corrupt(
                "geology manifest does not match the committed atlas manifest".into(),
            ));
        }
        self.hydrology
            .validate(self.side(), &self.genesis.terrain, &self.genesis.hydrology)?;
        let hydrology = encode_hydrology(&self.hydrology)?;
        if hydrology.len() as u64 != self.manifest.hydrology_bytes
            || stable_hash(hydrology.as_bytes()) != self.manifest.hydrology_checksum
        {
            return Err(AtlasError::Corrupt(
                "hydrology manifest does not match the committed atlas manifest".into(),
            ));
        }
        self.biomes
            .validate(self.side(), &self.genesis.terrain, &self.genesis.biomes)?;
        let biomes = encode_biomes(&self.biomes)?;
        if biomes.len() as u64 != self.manifest.biome_bytes
            || stable_hash(biomes.as_bytes()) != self.manifest.biome_checksum
        {
            return Err(AtlasError::Corrupt(
                "biome/country manifest does not match the committed atlas manifest".into(),
            ));
        }
        Ok(())
    }
}

pub(in crate::planet_atlas) fn validate_geometry(
    grid: &AtlasGrid<GeometryCell>,
) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.latitude_radians.is_finite()
            || !cell.physical_area.is_finite()
            || cell.physical_area <= 0.0
            || cell.unit_direction.iter().any(|value| !value.is_finite())
            || (Vec3::from_array(cell.unit_direction).length() - 1.0).abs() > 0.001
    }) {
        return Err(AtlasError::Corrupt(
            "geometry contains an invalid direction, latitude, or area".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_tectonics(
    grid: &AtlasGrid<TectonicCell>,
) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| cell.plate_id >= 24) {
        return Err(AtlasError::Corrupt(
            "tectonic layer contains an invalid plate identifier".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_terrain(
    grid: &AtlasGrid<TerrainCell>,
) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.base_elevation.is_finite()
            || !cell.eroded_elevation.is_finite()
            || !(0.0..crate::chunk::CHUNK_Y as f32).contains(&cell.base_elevation)
            || !(0.0..crate::chunk::CHUNK_Y as f32).contains(&cell.eroded_elevation)
    }) {
        return Err(AtlasError::Corrupt(
            "terrain layer contains an invalid elevation".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_climate(
    grid: &AtlasGrid<ClimateCell>,
) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.mean_temperature.is_finite()
            || !cell.seasonality.is_finite()
            || !cell.ocean_temperature_anomaly.is_finite()
            || !cell.continentality.is_finite()
            || !(0.0..=1.0).contains(&cell.continentality)
            || !cell.mean_atmospheric_moisture.is_finite()
            || !cell.mean_precipitation.is_finite()
            || cell.mean_precipitation < 0.0
            || !cell.precipitation_seasonality.is_finite()
            || !cell.potential_evapotranspiration.is_finite()
            || !cell.aridity.is_finite()
            || !cell.snow_persistence.is_finite()
            || !(0.0..=1.0).contains(&cell.snow_persistence)
            || cell.prevailing_wind.iter().any(|value| !value.is_finite())
            || cell.ocean_current.iter().any(|value| !value.is_finite())
            || cell
                .seasonal_temperature
                .iter()
                .chain(&cell.seasonal_precipitation)
                .any(|value| !value.is_finite())
            || cell
                .seasonal_wind
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
    }) {
        return Err(AtlasError::Corrupt(
            "climate layer contains a non-finite or negative value".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_hydrology(
    grid: &AtlasGrid<HydrologyCell>,
) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        (cell.drainage_receiver != u32::MAX && cell.drainage_receiver as usize >= grid.len())
            || !cell.spill_elevation.is_finite()
            || !cell.filled_elevation.is_finite()
            || !cell.mean_runoff.is_finite()
            || cell.mean_runoff < 0.0
            || !cell.mean_discharge.is_finite()
            || cell.mean_discharge < 0.0
            || !cell.catchment_area.is_finite()
            || cell.catchment_area < 0.0
            || !cell.channel_bed_elevation.is_finite()
            || !cell.water_surface_elevation.is_finite()
            || cell
                .seasonal_runoff_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>()
                != 65_535
            || cell
                .seasonal_discharge_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>()
                != 65_535
    }) {
        return Err(AtlasError::Corrupt(
            "hydrology layer contains an invalid receiver, budget, or elevation".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_ground(
    grid: &AtlasGrid<GroundCell>,
) -> Result<(), AtlasError> {
    if grid
        .values()
        .iter()
        .any(|cell| !cell.baseline_groundwater_head.is_finite())
    {
        return Err(AtlasError::Corrupt(
            "ground layer contains a non-finite groundwater head".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_biomes(
    grid: &AtlasGrid<BiomeCell>,
) -> Result<(), AtlasError> {
    if grid
        .values()
        .iter()
        .any(|cell| !(1..=BIOME_OCEAN).contains(&cell.baseline_biome))
    {
        return Err(AtlasError::Corrupt(
            "biome layer contains an unknown zonal biome".into(),
        ));
    }
    Ok(())
}

pub(in crate::planet_atlas) fn validate_layers(
    side: u16,
    genesis: &GenesisLayers,
    dynamic: &DynamicLayers,
) -> Result<(), AtlasError> {
    let expected = atlas_count(side)?;
    for (name, grid_side, len) in [
        ("geometry", genesis.geometry.side(), genesis.geometry.len()),
        (
            "tectonics",
            genesis.tectonics.side(),
            genesis.tectonics.len(),
        ),
        ("terrain", genesis.terrain.side(), genesis.terrain.len()),
        ("climate", genesis.climate.side(), genesis.climate.len()),
        (
            "hydrology",
            genesis.hydrology.side(),
            genesis.hydrology.len(),
        ),
        ("ground", genesis.ground.side(), genesis.ground.len()),
        ("biomes", genesis.biomes.side(), genesis.biomes.len()),
        (
            "resources",
            genesis.resources.side(),
            genesis.resources.len(),
        ),
        ("dynamic", dynamic.cells.side(), dynamic.cells.len()),
    ] {
        if grid_side != side || len != expected {
            return Err(AtlasError::Corrupt(format!(
                "{name} layer has side {grid_side} and {len} cells; expected side {side} and {expected}"
            )));
        }
    }
    validate_geometry(&genesis.geometry)?;
    validate_tectonics(&genesis.tectonics)?;
    validate_terrain(&genesis.terrain)?;
    validate_climate(&genesis.climate)?;
    validate_hydrology(&genesis.hydrology)?;
    validate_ground(&genesis.ground)?;
    validate_biomes(&genesis.biomes)?;
    Ok(())
}
