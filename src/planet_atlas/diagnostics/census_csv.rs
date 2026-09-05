//! Stable CSV projection of the recorded atlas census.

use crate::planet_atlas::{AtlasError};
use crate::planet_atlas::diagnostics::{AtlasCensus};
use std::path::{Path};

pub(in crate::planet_atlas::diagnostics) fn write_census_csv(census: &AtlasCensus, path: &Path) -> Result<(), AtlasError> {
    let mut out = String::from("metric,key,value\n");
    out.push_str(&format!("planet,cells,{}\n", census.cells));
    out.push_str(&format!(
        "planet,physical_area,{:.3}\n",
        census.physical_area
    ));
    out.push_str(&format!(
        "planet,land_fraction,{:.8}\n",
        census.land_fraction
    ));
    out.push_str(&format!(
        "planet,ocean_fraction,{:.8}\n",
        census.ocean_fraction
    ));
    out.push_str(&format!("geology,plates,{}\n", census.plate_count));
    out.push_str(&format!("geology,cratons,{}\n", census.craton_count));
    out.push_str(&format!(
        "geology,major_continents,{}\n",
        census.major_continent_count
    ));
    out.push_str(&format!(
        "geology,largest_ocean_share,{:.8}\n",
        census.largest_ocean_share
    ));
    for (face, cells) in &census.cells_by_face {
        out.push_str(&format!("face,{face},{cells}\n"));
    }
    for (biome, area) in &census.biome_areas {
        out.push_str(&format!("biome,{biome},{area:.3}\n"));
    }
    for (zone, area) in &census.climate_zone_areas {
        out.push_str(&format!("climate,{zone},{area:.3}\n"));
    }
    out.push_str(&format!(
        "climate,area_mean_temperature_c,{:.4}\nclimate,minimum_temperature_c,{:.4}\nclimate,maximum_temperature_c,{:.4}\nclimate,area_mean_precipitation_mm,{:.4}\nclimate,maximum_precipitation_mm,{:.4}\nclimate,area_mean_aridity,{:.6}\nclimate,maximum_aridity,{:.6}\n",
        census.area_mean_temperature_c,
        census.minimum_temperature_c,
        census.maximum_temperature_c,
        census.area_mean_precipitation_mm,
        census.maximum_precipitation_mm,
        census.area_mean_aridity,
        census.maximum_aridity,
    ));
    out.push_str(&format!(
        "hydrology,ocean_basins,{}\nhydrology,named_rivers,{}\nhydrology,lakes,{}\nhydrology,watersheds,{}\nhydrology,maximum_discharge,{:.5}\nhydrology,maximum_channel_width_blocks,{:.3}\nhydrology,maximum_stream_order,{}\nhydrology,floodplain_cells,{}\nhydrology,wetland_cells,{}\nhydrology,delta_cells,{}\nhydrology,estuary_cells,{}\nhydrology,baseline_surface_water_units,{}\nhydrology,voxel_volume_residual,{}\n",
        census.ocean_basin_count,
        census.named_river_count,
        census.lake_count,
        census.watershed_count,
        census.maximum_discharge,
        census.maximum_channel_width_blocks,
        census.maximum_stream_order,
        census.floodplain_cells,
        census.wetland_cells,
        census.delta_cells,
        census.estuary_cells,
        census.baseline_surface_water_units,
        census.voxel_volume_residual,
    ));
    for (class, count) in &census.lake_classes {
        out.push_str(&format!("lake_class,{class},{count}\n"));
    }
    for (mineral, count) in &census.deposits_by_mineral {
        out.push_str(&format!("deposit_sites,{mineral},{count}\n"));
    }
    for (mineral, tonnage) in &census.deposit_tonnage_by_mineral {
        out.push_str(&format!("deposit_tonnage,{mineral},{tonnage}\n"));
    }
    for (mineral, distance) in &census.max_nearest_deposit_distance_blocks {
        out.push_str(&format!(
            "deposit_max_distance_blocks,{mineral},{distance:.3}\n"
        ));
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}
