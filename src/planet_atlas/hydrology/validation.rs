//! Hydrology manifest, dense-cell, drainage, and reservoir consistency.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasError, AtlasGrid, HydrologyCell, TerrainCell};
use super::{HydrologyModel, StoragePoint, WaterBodyKind, HYDROLOGY_SCHEMA_VERSION, MAX_EROSION_ITERATIONS, HYDRO_RIVER};
use super::flood::neighbors8_indices;
use super::flow::topological_order;

impl HydrologyModel {
    pub fn validate(
        &self,
        side: u16,
        terrain: &AtlasGrid<TerrainCell>,
        cells: &AtlasGrid<HydrologyCell>,
    ) -> Result<(), AtlasError> {
        if self.schema_version != HYDROLOGY_SCHEMA_VERSION
            || self.sea_level != SEA_LEVEL as f32
            || self.erosion_iterations == 0
            || self.erosion_iterations > MAX_EROSION_ITERATIONS
            || !self.erosion_max_residual.is_finite()
        {
            return Err(AtlasError::Corrupt(
                "hydrology model has invalid schema or erosion evidence".into(),
            ));
        }
        if !self
            .oceans
            .iter()
            .any(|ocean| ocean.id == self.dominant_ocean_id)
        {
            return Err(AtlasError::Corrupt(
                "hydrology model has no dominant ocean".into(),
            ));
        }
        for ocean in &self.oceans {
            validate_curve(&ocean.volume_elevation_curve)?;
            let dense_cells = cells
                .values()
                .iter()
                .filter(|cell| cell.ocean_basin_id == ocean.id);
            let dense_count = dense_cells.clone().count() as u32;
            let dense_volume = dense_cells
                .map(|cell| cell.baseline_water_units)
                .fold(0u64, u64::saturating_add);
            if ocean.cell_count == 0
                || ocean.cell_count != dense_count
                || ocean.baseline_volume_units != dense_volume
                || ocean.salinity < 192
            {
                return Err(AtlasError::Corrupt(
                    "ocean record disagrees with dense cells or has freshwater salinity".into(),
                ));
            }
        }
        for lake in &self.lakes {
            validate_curve(&lake.volume_elevation_curve)?;
            let dense_cells = cells
                .values()
                .iter()
                .filter(|cell| cell.lake_basin_id == lake.id);
            let dense_count = dense_cells.clone().count() as u32;
            let dense_volume = dense_cells
                .clone()
                .map(|cell| cell.baseline_water_units)
                .fold(0u64, u64::saturating_add);
            let dense_residual = dense_cells
                .map(|cell| i64::from(cell.voxel_volume_residual))
                .sum::<i64>();
            if lake.cell_count == 0
                || lake.cell_count != dense_count
                || lake.baseline_volume_units != dense_volume
                || lake.voxel_volume_residual != dense_residual
                || lake.surface_elevation > lake.spill_elevation + 0.001
                || (lake.baseline_inflow - lake.baseline_evaporation - lake.baseline_outflow).abs()
                    > lake.baseline_inflow.max(1.0) * 1.0e-8
            {
                return Err(AtlasError::Corrupt(
                    "lake record has invalid geometry or an open water budget".into(),
                ));
            }
        }
        if self.baseline_surface_water_units
            != cells
                .values()
                .iter()
                .map(|cell| u128::from(cell.baseline_water_units))
                .sum::<u128>()
        {
            return Err(AtlasError::Corrupt(
                "hydrology baseline volume disagrees with dense cells".into(),
            ));
        }
        let order = topological_order(
            &cells
                .values()
                .iter()
                .map(|cell| cell.drainage_receiver)
                .collect::<Vec<_>>(),
        )?;
        if order.len() != cells.len() || terrain.side() != side || cells.side() != side {
            return Err(AtlasError::Corrupt(
                "hydrology grids have invalid dimensions".into(),
            ));
        }
        for (index, cell) in cells.values().iter().enumerate() {
            if cell.drainage_receiver != u32::MAX {
                let receiver = cell.drainage_receiver as usize;
                if !neighbors8_indices(index, side).contains(&receiver) {
                    return Err(AtlasError::Corrupt(
                        "drainage receiver is not a seam-aware neighbor".into(),
                    ));
                }
                if cell.flags & HYDRO_RIVER != 0
                    && cells.values()[receiver].flags & HYDRO_RIVER != 0
                    && cell.channel_bed_elevation + 0.001
                        < cells.values()[receiver].channel_bed_elevation
                {
                    return Err(AtlasError::Corrupt(
                        "river bed climbs in the downstream direction".into(),
                    ));
                }
            }
            if cell.water_body == WaterBodyKind::Ocean
                && terrain.values()[index].eroded_elevation > SEA_LEVEL as f32
            {
                return Err(AtlasError::Corrupt(
                    "ocean classification lies above marine datum".into(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_curve(curve: &[StoragePoint]) -> Result<(), AtlasError> {
    if curve.is_empty()
        || curve.iter().any(|point| !point.elevation.is_finite())
        || curve.windows(2).any(|pair| {
            pair[1].elevation < pair[0].elevation || pair[1].volume_units < pair[0].volume_units
        })
    {
        return Err(AtlasError::Corrupt(
            "reservoir volume/elevation curve is not monotonic".into(),
        ));
    }
    Ok(())
}
