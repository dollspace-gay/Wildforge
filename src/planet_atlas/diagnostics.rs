//! Stable, headless atlas maps and qualification census.
use super::{AtlasError, PlanetAtlas};
use std::fs;
use std::path::Path;
use std::time::Instant;

mod records;
pub use records::{AtlasCensus, AtlasExportReport};
mod catalog;
mod census;
mod census_cells;
mod census_csv;
mod country_routes;
mod images;
mod maps;
mod memory;
mod sites;
mod transects;
mod values;
mod weather_examples;
use catalog::{layer_count, layers};
use census_csv::write_census_csv;
use country_routes::export_country_adjacency;
use images::export_globe_preview;
use maps::export_layer;
use memory::{estimated_loaded_bytes, peak_resident_bytes};
use sites::qualification_sites;
use transects::{export_climate_transects, export_river_profiles};
use weather_examples::export_weather_examples;

const FACE_LAYOUT: &str =
    "3x2: pos_x,neg_x,pos_y / neg_y,pos_z,neg_z; u left-to-right, v top-to-bottom";

pub fn export_diagnostics(
    atlas: &PlanetAtlas,
    output: &Path,
) -> Result<AtlasExportReport, AtlasError> {
    let started = Instant::now();
    atlas.validate()?;
    fs::create_dir_all(output)?;
    let map_dir = output.join("maps");
    fs::create_dir_all(&map_dir)?;
    let mut exported_maps = Vec::with_capacity(layer_count());
    for spec in layers() {
        export_layer(atlas, *spec, &map_dir)?;
        exported_maps.push(format!("maps/{}.png", spec.id));
    }
    export_climate_transects(atlas, &output.join("climate-transects.csv"))?;
    export_river_profiles(atlas, &output.join("river-profiles.csv"))?;
    export_weather_examples(atlas, &map_dir, &output.join("weather-tracks.csv"))?;
    export_globe_preview(atlas, &output.join("globe_preview.png"))?;

    let census = atlas.census()?;
    let census_toml = toml::to_string_pretty(&census)
        .map_err(|error| AtlasError::Corrupt(format!("census encoding failed: {error}")))?;
    crate::persist::atomic_write(&output.join("census.toml"), census_toml.as_bytes(), false)?;
    write_census_csv(&census, &output.join("census.csv"))?;
    let manifest = toml::to_string_pretty(&atlas.manifest)
        .map_err(|error| AtlasError::Corrupt(format!("manifest encoding failed: {error}")))?;
    crate::persist::atomic_write(&output.join("manifest.toml"), manifest.as_bytes(), false)?;
    let geology = toml::to_string_pretty(&atlas.geology)
        .map_err(|error| AtlasError::Corrupt(format!("geology export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("geology.toml"), geology.as_bytes(), false)?;
    let hydrology = toml::to_string_pretty(&atlas.hydrology)
        .map_err(|error| AtlasError::Corrupt(format!("hydrology export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("hydrology.toml"), hydrology.as_bytes(), false)?;
    let biomes = toml::to_string_pretty(&atlas.biomes)
        .map_err(|error| AtlasError::Corrupt(format!("biome export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("biomes.toml"), biomes.as_bytes(), false)?;
    export_country_adjacency(atlas, &output.join("country-adjacency.csv"))?;
    crate::persist::atomic_write(
        &output.join("water-audit.txt"),
        atlas.water_audit_text().as_bytes(),
        false,
    )?;
    let qualification_sites = qualification_sites(atlas);
    let qualification_toml = toml::to_string_pretty(&qualification_sites).map_err(|error| {
        AtlasError::Corrupt(format!("qualification-site encoding failed: {error}"))
    })?;
    crate::persist::atomic_write(
        &output.join("qualification-sites.toml"),
        qualification_toml.as_bytes(),
        false,
    )?;

    let estimated_loaded_bytes = estimated_loaded_bytes(atlas);
    let report = AtlasExportReport {
        validation: "passed".into(),
        layout: FACE_LAYOUT.into(),
        registered_layers: layers().map(|layer| layer.id.to_string()).collect(),
        exported_maps,
        cell_count: atlas.genesis.geometry.len(),
        stage_elapsed_micros: atlas
            .manifest
            .stages
            .iter()
            .map(|stage| (stage.id.clone(), stage.elapsed_micros))
            .collect(),
        immutable_bytes: atlas.manifest.genesis_bytes,
        dynamic_bytes: atlas.manifest.dynamic_bytes,
        water_cycle_bytes: atlas.manifest.water_cycle_bytes,
        geology_bytes: atlas.manifest.geology_bytes,
        hydrology_bytes: atlas.manifest.hydrology_bytes,
        biome_bytes: atlas.manifest.biome_bytes,
        estimated_loaded_bytes,
        peak_resident_bytes: peak_resident_bytes(),
        export_elapsed_micros: started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
    };
    let report_toml = toml::to_string_pretty(&report)
        .map_err(|error| AtlasError::Corrupt(format!("report encoding failed: {error}")))?;
    crate::persist::atomic_write(
        &output.join("validation-report.toml"),
        report_toml.as_bytes(),
        false,
    )?;
    Ok(report)
}
