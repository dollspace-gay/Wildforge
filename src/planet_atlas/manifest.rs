//! Persisted atlas manifest and layer compatibility contract.

use crate::planet::{FACE_BLOCKS, PLANET_RADIUS};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::{
    ATLAS_ALGORITHM_VERSION, ATLAS_DYNAMIC_VERSION, ATLAS_FACE_SIDE, ATLAS_FORMAT_VERSION,
    ATLAS_HISTORY_VERSION, AXIAL_TILT_DEGREES, AtlasError, BIOME_SCHEMA_VERSION,
    CLIMATE_CONVERGENCE_TOLERANCE, CLIMATE_MAX_ITERATIONS, CLIMATE_SEASONS, GEOLOGY_SCHEMA_VERSION,
    HYDROLOGY_SCHEMA_VERSION, PRIME_MERIDIAN, ROTATION_AXIS, WATER_CYCLE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageRecord {
    pub id: String,
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub checksum: u64,
    pub elapsed_micros: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasManifest {
    pub format_version: u32,
    pub seed: u32,
    pub topology: String,
    pub face_blocks: u16,
    pub atlas_cell_blocks: u16,
    pub atlas_face_side: u16,
    pub atlas_cell_count: u32,
    pub topology_version: u32,
    pub atlas_algorithm_version: u32,
    pub dynamic_schema_version: u32,
    pub water_cycle_schema_version: u32,
    pub history_schema_version: u32,
    pub geology_schema_version: u32,
    pub hydrology_schema_version: u32,
    pub biome_schema_version: u32,
    pub planet_radius: f64,
    pub rotation_axis: [f64; 3],
    pub prime_meridian: [f64; 3],
    pub axial_tilt_degrees: f64,
    pub climate_convergence_iterations: [u16; CLIMATE_SEASONS],
    pub climate_max_residual: f32,
    pub climate_moisture_budget_error: f64,
    pub content_hash: u64,
    pub genesis_checksum: u64,
    pub dynamic_checksum: u64,
    pub water_cycle_checksum: u64,
    pub history_checksum: u64,
    pub geology_checksum: u64,
    pub hydrology_checksum: u64,
    pub biome_checksum: u64,
    #[serde(default)]
    pub arcane_schema_version: u32,
    #[serde(default)]
    pub arcane_algorithm_version: u32,
    #[serde(default)]
    pub arcane_unit_scale: u32,
    #[serde(default)]
    pub arcane_genesis_total: u64,
    #[serde(default)]
    pub arcane_last_clean_total: u64,
    /// Deep, Ambient, Bound, Active, Dross, Scar at the last ledger
    /// checkpoint. Fixed order keeps the qualified manifest compact.
    #[serde(default)]
    pub arcane_reservoir_totals: [u64; 6],
    #[serde(default)]
    pub arcane_registry_hash: u64,
    #[serde(default)]
    pub arcane_ledger_checksum: u64,
    #[serde(default)]
    pub arcane_delta_checksum: u64,
    #[serde(default)]
    pub arcane_geography_schema_version: u32,
    #[serde(default)]
    pub arcane_geography_algorithm_version: u32,
    #[serde(default)]
    pub arcane_geography_dynamic_version: u32,
    #[serde(default)]
    pub arcane_geography_genesis_total: u64,
    #[serde(default)]
    pub arcane_geography_immutable_checksum: u64,
    #[serde(default)]
    pub arcane_geography_dynamic_checksum: u64,
    #[serde(default)]
    pub arcane_geography_site_catalog_checksum: u64,
    #[serde(default)]
    pub arcane_geography_last_authoritative_time: u64,
    pub genesis_bytes: u64,
    pub dynamic_bytes: u64,
    pub water_cycle_bytes: u64,
    pub geology_bytes: u64,
    pub hydrology_bytes: u64,
    pub biome_bytes: u64,
    pub complete: bool,
    #[serde(default)]
    pub stages: Vec<StageRecord>,
    #[serde(default)]
    pub layer_versions: BTreeMap<String, u32>,
}

pub(in crate::planet_atlas) fn layer_versions() -> BTreeMap<String, u32> {
    let mut versions = [
        "geometry",
        "tectonics",
        "terrain",
        "climate",
        "hydrology",
        "ground",
        "biomes",
        "resources",
        "geology_manifest",
        "dynamic",
        "water_cycle",
        "history",
        "arcane_current_capacity",
        "arcane_deep_reserve_capacity",
        "arcane_surface_deep_exchange",
        "arcane_horizontal_conductivity",
        "arcane_dross_mobility_retention",
        "arcane_baseline_resonance",
        "arcane_stability",
        "arcane_recovery_potential",
        "arcane_site_references",
        "arcane_dynamic_state",
    ]
    .into_iter()
    .map(|name| (name.to_string(), 1))
    .collect::<BTreeMap<_, _>>();
    versions.insert("climate".to_string(), 2);
    versions.insert("arcane_horizontal_conductivity".to_string(), 2);
    versions.insert("hydrology".to_string(), 2);
    versions.insert("dynamic".to_string(), 3);
    versions.insert("water_cycle".to_string(), WATER_CYCLE_SCHEMA_VERSION);
    versions
}

pub(in crate::planet_atlas) fn validate_manifest(
    manifest: &AtlasManifest,
    production_only: bool,
) -> Result<(), AtlasError> {
    if manifest.format_version != ATLAS_FORMAT_VERSION {
        return Err(AtlasError::UnsupportedVersion(format!(
            "format {} (supported {})",
            manifest.format_version, ATLAS_FORMAT_VERSION
        )));
    }
    if manifest.topology != crate::planet::WORLD_TOPOLOGY
        || manifest.face_blocks != FACE_BLOCKS
        || manifest.topology_version != 1
        || (manifest.planet_radius - PLANET_RADIUS).abs() > 0.001
        || manifest.rotation_axis != ROTATION_AXIS
        || manifest.prime_meridian != PRIME_MERIDIAN
        || (manifest.axial_tilt_degrees - AXIAL_TILT_DEGREES).abs() > 1.0e-9
    {
        return Err(AtlasError::UnsupportedVersion(
            "topology parameters do not match this build".into(),
        ));
    }
    if manifest.atlas_algorithm_version > ATLAS_ALGORITHM_VERSION
        || manifest.dynamic_schema_version != ATLAS_DYNAMIC_VERSION
        || manifest.water_cycle_schema_version != WATER_CYCLE_SCHEMA_VERSION
        || manifest.history_schema_version != ATLAS_HISTORY_VERSION
        || manifest.geology_schema_version != GEOLOGY_SCHEMA_VERSION
        || manifest.hydrology_schema_version != HYDROLOGY_SCHEMA_VERSION
        || manifest.biome_schema_version != BIOME_SCHEMA_VERSION
        || manifest.layer_versions != layer_versions()
    {
        return Err(AtlasError::UnsupportedVersion(
            "one or more atlas layer schemas do not match this build".into(),
        ));
    }
    if manifest
        .climate_convergence_iterations
        .iter()
        .any(|iterations| *iterations == 0 || *iterations > CLIMATE_MAX_ITERATIONS)
        || !manifest.climate_max_residual.is_finite()
        || f64::from(manifest.climate_max_residual) > CLIMATE_CONVERGENCE_TOLERANCE
        || !manifest.climate_moisture_budget_error.is_finite()
        || manifest.climate_moisture_budget_error < 0.0
    {
        return Err(AtlasError::Corrupt(
            "climate convergence or moisture-budget evidence is invalid".into(),
        ));
    }
    let expected = atlas_count(manifest.atlas_face_side)?;
    if manifest.atlas_cell_count as usize != expected
        || manifest.atlas_cell_blocks != FACE_BLOCKS / manifest.atlas_face_side
    {
        return Err(AtlasError::Corrupt(
            "manifest dimensions are internally inconsistent".into(),
        ));
    }
    if production_only && manifest.atlas_face_side != ATLAS_FACE_SIDE {
        return Err(AtlasError::UnsupportedVersion(format!(
            "fixture atlas side {} cannot be opened as a production planet",
            manifest.atlas_face_side
        )));
    }
    if !manifest.complete {
        return Err(AtlasError::Incomplete(
            "manifest was not committed as complete".into(),
        ));
    }
    Ok(())
}
