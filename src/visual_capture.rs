//! Versioned capture evidence shared by visual qualification arcs.
//!
//! The GPU owns pixels; this module owns the small, reviewable identity beside
//! them. Nothing here is consulted unless an automated capture explicitly sets
//! `WILDFORGE_VISUAL_EVIDENCE=1`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::{fs, path::Component};

use serde::{Deserialize, Serialize};

pub const CAPTURE_SCHEMA_VERSION: u32 = 1;
pub const DIAGNOSTIC_FORMAT: &str = "wfd-rgba16uint-v1";
pub const WFD_MAGIC: [u8; 8] = *b"WFD1LE\0\0";
pub const WFD_HEADER_BYTES: usize = 24;
pub const DIAGNOSTIC_SKY_ID: u16 = 0;
pub const DIAGNOSTIC_OVERLAY_ID: u16 = u16::MAX;
#[cfg(test)]
const REPORT_SCHEMA_VERSION: u32 = 2;
#[cfg(test)]
const COMPARISON_SCHEMA_VERSION: u32 = 1;
#[cfg(test)]
const CONVERSION_ID: &str = "python-stdlib-p6-rgb8-filter0-zlib9-v1";
#[cfg(test)]
mod campaign;
#[cfg(test)]
mod native;

pub fn evidence_enabled() -> bool {
    std::env::var("WILDFORGE_VISUAL_EVIDENCE").as_deref() == Ok("1")
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BuildIdentity {
    pub commit: String,
    pub dirty: bool,
    pub marker: String,
}

impl BuildIdentity {
    pub fn current(marker: &str) -> Self {
        Self {
            commit: env!("WILDFORGE_BUILD_COMMIT").into(),
            dirty: env!("WILDFORGE_BUILD_DIRTY") == "true",
            marker: marker.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorldIdentity {
    pub name: String,
    pub seed: u32,
    pub generator_version: u32,
    pub atlas_format_version: u32,
    pub atlas_algorithm_version: u32,
    pub atlas_content_hash: String,
    pub atlas_genesis_checksum: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CameraIdentity {
    pub face: String,
    pub u: f32,
    pub y: f32,
    pub v: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_degrees: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EnvironmentIdentity {
    pub day: u32,
    pub time_of_day: f32,
    pub season: String,
    pub weather: String,
    pub precipitation: String,
    pub temperature_c: f32,
    pub pressure_anomaly: i16,
    pub vapor: u32,
    pub cloud_water: u32,
    pub storm_energy: u16,
    pub precipitation_units: u16,
    pub wind: [f32; 2],
    pub gloom: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RenderIdentity {
    pub width: u32,
    pub height: u32,
    pub pack: String,
    pub view_distance_chunks: i32,
    pub fog_distance_blocks: f32,
    pub adapter: String,
    pub backend: String,
    pub hardware: bool,
    pub lights: u8,
    pub point_grid: bool,
    pub stark: bool,
    pub bloom: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptureTelemetry {
    pub frame: u64,
    pub eligible_frame: u64,
    pub settled: bool,
    pub settled_frames: u64,
    pub fps: u32,
    pub simulation_ms: f32,
    pub draw_ms: f32,
    pub resident_chunks: usize,
    pub gpu_chunks: usize,
    pub opaque_chunks: usize,
    pub water_chunks: usize,
    pub empty_chunks: usize,
    pub dirty_chunks: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DiagnosticFamily {
    pub diagnostic_id: u16,
    pub atlas_slot: u16,
    pub names: Vec<String>,
}

/// Stable, reviewable names for every atlas slot that a submitted mesh can
/// reference. Variant slots inherit the base tile's block/item names because
/// they are appearance variants of the same finite world material.
pub fn diagnostic_families(
    registry: &crate::registry::Registry,
    variants: &crate::atlas::TileVariants,
) -> Vec<DiagnosticFamily> {
    let mut names: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
    for block in &registry.blocks {
        for slot in block
            .tiles
            .iter()
            .copied()
            .chain(block.fert_tiles.into_iter().flatten())
        {
            names.entry(slot).or_default().insert(block.name.clone());
        }
    }
    for item in &registry.items {
        names
            .entry(item.icon)
            .or_default()
            .insert(format!("item:{}", item.name));
    }
    for (name, slot) in &registry.tex_names {
        names
            .entry(*slot)
            .or_default()
            .insert(format!("texture:{name}"));
    }
    for (base, alternates) in variants.signature() {
        let inherited = names
            .get(&base)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([format!("atlas-slot:{base}")]));
        for slot in alternates {
            names.entry(slot).or_default().extend(inherited.clone());
        }
    }
    names
        .into_iter()
        .map(|(atlas_slot, names)| DiagnosticFamily {
            diagnostic_id: atlas_slot
                .checked_add(1)
                .filter(|id| *id != DIAGNOSTIC_SKY_ID && *id != DIAGNOSTIC_OVERLAY_ID)
                .expect("atlas slot is representable as a material diagnostic id"),
            atlas_slot,
            names: names.into_iter().collect(),
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptureMetadata {
    pub schema_version: u32,
    pub capture_id: String,
    pub scene_id: String,
    pub build: BuildIdentity,
    pub world: WorldIdentity,
    pub camera: CameraIdentity,
    pub environment: EnvironmentIdentity,
    pub render: RenderIdentity,
    pub telemetry: CaptureTelemetry,
    pub family: Vec<DiagnosticFamily>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RawArtifacts {
    pub color_ppm: String,
    pub color_ppm_sha256: String,
    pub diagnostic: String,
    pub diagnostic_sha256: String,
    pub diagnostic_format: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptureSidecar {
    #[serde(flatten)]
    pub metadata: CaptureMetadata,
    pub artifacts: RawArtifacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidencePaths {
    pub ppm: PathBuf,
    pub diagnostic: PathBuf,
    pub sidecar: PathBuf,
}

pub fn evidence_paths(ppm: &Path) -> Result<EvidencePaths, String> {
    if ppm.extension().and_then(|value| value.to_str()) != Some("ppm") {
        return Err("visual-evidence color output must have a .ppm suffix".into());
    }
    Ok(EvidencePaths {
        ppm: ppm.to_owned(),
        diagnostic: ppm.with_extension("wfd"),
        sidecar: ppm.with_extension("capture.toml"),
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    crate::identity::sha256(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Strip GPU row padding and write the little-endian WFD attachment. Each
/// pixel is RGBA16Uint: family id, normalized geodesic depth, class flags,
/// schema marker.
pub fn encode_wfd(
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    gpu_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let compact_row = width
        .checked_mul(8)
        .ok_or("diagnostic row byte count overflow")?;
    if padded_bytes_per_row < compact_row {
        return Err("diagnostic GPU row is shorter than its compact row".into());
    }
    let required = padded_bytes_per_row as usize * height as usize;
    if gpu_bytes.len() < required {
        return Err("diagnostic GPU readback is truncated".into());
    }
    let payload = compact_row as usize * height as usize;
    let mut out = Vec::with_capacity(WFD_HEADER_BYTES + payload);
    out.extend_from_slice(&WFD_MAGIC);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // RGBA16Uint little-endian
    out.extend_from_slice(&0u32.to_le_bytes());
    for y in 0..height as usize {
        let row = &gpu_bytes[y * padded_bytes_per_row as usize..][..compact_row as usize];
        for bytes in row.chunks_exact(2) {
            out.extend_from_slice(&u16::from_ne_bytes([bytes[0], bytes[1]]).to_le_bytes());
        }
    }
    Ok(out)
}

pub fn sidecar_text(
    metadata: CaptureMetadata,
    color_path: &Path,
    color_bytes: &[u8],
    diagnostic_path: &Path,
    diagnostic_bytes: &[u8],
) -> Result<String, String> {
    let sidecar = CaptureSidecar {
        metadata,
        artifacts: RawArtifacts {
            color_ppm: color_path.to_string_lossy().replace('\\', "/"),
            color_ppm_sha256: sha256_hex(color_bytes),
            diagnostic: diagnostic_path.to_string_lossy().replace('\\', "/"),
            diagnostic_sha256: sha256_hex(diagnostic_bytes),
            diagnostic_format: DIAGNOSTIC_FORMAT.into(),
        },
    };
    toml::to_string_pretty(&sidecar).map_err(|error| format!("serialize capture identity: {error}"))
}

#[cfg(test)]
mod models;
#[cfg(test)]
use models::*;
#[cfg(test)]
mod evidence;
#[cfg(test)]
pub(crate) use evidence::qualification_source_sha256;
#[cfg(test)]
use evidence::*;
#[cfg(test)]
mod manifest;
#[cfg(test)]
pub(crate) use manifest::validate_visual_polish_manifest;
#[cfg(test)]
use manifest::validate_visual_polish_manifest_at;
#[cfg(test)]
mod geode_manifest;
#[cfg(test)]
use geode_manifest::*;
#[cfg(test)]
mod geode_group;
#[cfg(test)]
use geode_group::*;
#[cfg(test)]
mod closeout;
#[cfg(test)]
use closeout::*;
#[cfg(test)]
mod tests;
