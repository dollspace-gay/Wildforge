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
const REPORT_SCHEMA_VERSION: u32 = 1;
#[cfg(test)]
const COMPARISON_SCHEMA_VERSION: u32 = 1;
#[cfg(test)]
const CONVERSION_ID: &str = "python-stdlib-p6-rgb8-filter0-zlib9-v1";
#[cfg(test)]
const QUALIFICATION_SOURCES: &[&str] = &[
    "build.rs",
    "src/game/app.rs",
    "src/game/capture.rs",
    "src/game/content.rs",
    "src/game/frame.rs",
    "src/game/mod.rs",
    "src/lib.rs",
    "src/renderer/frame.rs",
    "src/renderer/mod.rs",
    "src/renderer/setup.rs",
    "src/shader.wgsl",
    "src/visual_capture.rs",
    "tools/verify_visual_polish.py",
];

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
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualManifest {
    schema_version: u32,
    status: String,
    evidence_commit: String,
    scene_id: String,
    qualification_source_sha256: String,
    conversion: String,
    conversion_tool: String,
    conversion_tool_sha256: String,
    comparison: String,
    capture: Vec<ManifestCapture>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCapture {
    id: String,
    sidecar: String,
    report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualReport {
    report_schema_version: u32,
    capture_schema_version: u32,
    capture_id: String,
    scene_id: String,
    identity_sha256: String,
    sidecar: String,
    sidecar_sha256: String,
    width: u32,
    height: u32,
    color_ppm_sha256: String,
    diagnostic_sha256: String,
    color_png: String,
    color_png_sha256: String,
    diagnostic_png: String,
    diagnostic_png_sha256: String,
    conversion: String,
    conversion_tool: String,
    conversion_tool_sha256: String,
    python: String,
    zlib: String,
    pixel_count: u64,
    sky_pixels: u64,
    overlay_pixels: u64,
    material_pixels: u64,
    sky_fraction: f64,
    overlay_fraction: f64,
    material_fraction: f64,
    luminance_mean: f64,
    luminance_stddev: f64,
    local_contrast_rms: f64,
    family: Vec<ReportFamily>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportFamily {
    diagnostic_id: u16,
    atlas_slot: u16,
    names: Vec<String>,
    pixels: u64,
    pixel_fraction: f64,
    mean_normalized_depth: f64,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualComparison {
    comparison_schema_version: u32,
    scene_id: String,
    identity_sha256: String,
    first_report: String,
    first_report_sha256: String,
    second_report: String,
    second_report_sha256: String,
    same_identity: bool,
    same_dimensions: bool,
    segmentation_agreement: f64,
    minimum_segmentation_agreement: f64,
    family_total_variation: f64,
    maximum_family_total_variation: f64,
    depth_rmse: f64,
    maximum_depth_rmse: f64,
    luminance_mean_delta: f64,
    maximum_luminance_mean_delta: f64,
    local_contrast_delta: f64,
    maximum_local_contrast_delta: f64,
    sky_fraction_delta: f64,
    maximum_sky_fraction_delta: f64,
    equivalent: bool,
}

#[cfg(test)]
fn valid_hex(value: &str, digits: usize) -> bool {
    value.len() == digits
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
fn safe_relative<'a>(value: &'a str, extension: &str) -> Result<&'a Path, String> {
    let path = Path::new(value);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        || path.extension().and_then(|value| value.to_str()) != Some(extension)
    {
        return Err(format!(
            "visual evidence path must be relative .{extension}: {value}"
        ));
    }
    Ok(path)
}

#[cfg(test)]
fn read_toml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<(Vec<u8>, T), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("read {label} {}: {error}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| format!("decode {label} {}: {error}", path.display()))?;
    let value = toml::from_str(text)
        .map_err(|error| format!("parse {label} {}: {error}", path.display()))?;
    Ok((bytes, value))
}

#[cfg(test)]
fn finite_fraction(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
pub(crate) fn qualification_source_sha256(root: &Path) -> Result<String, String> {
    let mut source = Vec::new();
    for relative in QUALIFICATION_SOURCES {
        let bytes = fs::read(root.join(relative))
            .map_err(|error| format!("read qualification source {relative}: {error}"))?;
        source.extend_from_slice(relative.as_bytes());
        source.push(0);
        source.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        source.extend_from_slice(&bytes);
    }
    Ok(sha256_hex(&source))
}

#[cfg(test)]
fn validate_visual_polish_manifest_at(root: &Path, manifest_path: &Path) -> Result<(), String> {
    let (_manifest_bytes, manifest): (Vec<u8>, VisualManifest) =
        read_toml(manifest_path, "visual-polish manifest")?;
    if manifest.schema_version != CAPTURE_SCHEMA_VERSION || manifest.status != "accepted" {
        return Err("visual-polish manifest is not accepted schema 1 evidence".into());
    }
    if !valid_hex(&manifest.evidence_commit, 40)
        || !valid_hex(&manifest.qualification_source_sha256, 64)
        || manifest.scene_id.is_empty()
    {
        return Err("visual-polish manifest identity is incomplete".into());
    }
    let current_source = qualification_source_sha256(root)?;
    if current_source != manifest.qualification_source_sha256 {
        return Err("visual-polish evidence is stale relative to qualification source".into());
    }
    if manifest.conversion != CONVERSION_ID
        || manifest.conversion_tool != "tools/verify_visual_polish.py"
        || !valid_hex(&manifest.conversion_tool_sha256, 64)
    {
        return Err("visual-polish conversion provenance is incomplete".into());
    }
    let tool_path = root.join(safe_relative(&manifest.conversion_tool, "py")?);
    let tool_hash = sha256_hex(
        &fs::read(&tool_path)
            .map_err(|error| format!("read conversion tool {}: {error}", tool_path.display()))?,
    );
    if tool_hash != manifest.conversion_tool_sha256 {
        return Err("visual-polish conversion tool hash is stale".into());
    }
    if manifest.capture.len() != 2 {
        return Err("shared evidence foundation requires exactly two repeat captures".into());
    }

    let mut captures = Vec::with_capacity(2);
    let mut reports = Vec::with_capacity(2);
    let mut report_bytes = Vec::with_capacity(2);
    let mut ids = BTreeSet::new();
    for declaration in &manifest.capture {
        if !ids.insert(declaration.id.clone()) {
            return Err("visual-polish capture ids are not unique".into());
        }
        let sidecar_relative = safe_relative(&declaration.sidecar, "toml")?;
        let report_relative = safe_relative(&declaration.report, "toml")?;
        if !declaration.sidecar.starts_with("screenshots/")
            || !declaration.sidecar.ends_with(".capture.toml")
            || !declaration.report.starts_with("screenshots/")
            || !declaration.report.ends_with(".report.toml")
        {
            return Err("visual-polish evidence files use the wrong role suffix".into());
        }
        let sidecar_path = root.join(sidecar_relative);
        let report_path = root.join(report_relative);
        let (sidecar_bytes, sidecar): (Vec<u8>, CaptureSidecar) =
            read_toml(&sidecar_path, "capture sidecar")?;
        let (this_report_bytes, report): (Vec<u8>, VisualReport) =
            read_toml(&report_path, "visual report")?;

        if sidecar.metadata.schema_version != CAPTURE_SCHEMA_VERSION
            || sidecar.metadata.capture_id != declaration.id
            || sidecar.metadata.scene_id != manifest.scene_id
            || sidecar.metadata.build.commit != manifest.evidence_commit
            || sidecar.metadata.build.dirty
            || sidecar.metadata.build.marker.is_empty()
            || sidecar.metadata.world.generator_version != crate::world::WORLD_GENERATOR_VERSION
            || sidecar.metadata.world.atlas_format_version
                != crate::planet_atlas::ATLAS_FORMAT_VERSION
            || sidecar.metadata.world.atlas_algorithm_version
                != crate::planet_atlas::ATLAS_ALGORITHM_VERSION
            || !sidecar.metadata.render.hardware
            || sidecar.metadata.render.adapter.is_empty()
            || sidecar.metadata.render.backend.is_empty()
            || !sidecar.metadata.telemetry.settled
            || sidecar.metadata.telemetry.settled_frames < crate::game::SHOT_SETTLE_FRAMES
            || sidecar.metadata.render.width < 320
            || sidecar.metadata.render.height < 200
            || sidecar.metadata.render.width > 4096
            || sidecar.metadata.render.height > 4096
            || sidecar.metadata.family.is_empty()
        {
            return Err(format!(
                "capture {} has incomplete or stale identity",
                declaration.id
            ));
        }
        if sidecar.metadata.render.width as u64 * sidecar.metadata.render.height as u64 > 16_777_216
            || !valid_hex(&sidecar.metadata.world.atlas_content_hash, 16)
            || !valid_hex(&sidecar.metadata.world.atlas_genesis_checksum, 16)
            || !valid_hex(&sidecar.artifacts.color_ppm_sha256, 64)
            || !valid_hex(&sidecar.artifacts.diagnostic_sha256, 64)
            || sidecar.artifacts.diagnostic_format != DIAGNOSTIC_FORMAT
        {
            return Err(format!(
                "capture {} has invalid bounds or hashes",
                declaration.id
            ));
        }
        let base = declaration
            .sidecar
            .strip_suffix(".capture.toml")
            .ok_or("capture sidecar suffix disappeared")?;
        if sidecar.artifacts.color_ppm != format!("{base}.ppm")
            || sidecar.artifacts.diagnostic != format!("{base}.wfd")
        {
            return Err(format!(
                "capture {} raw artifacts are not adjacent",
                declaration.id
            ));
        }

        if report.report_schema_version != REPORT_SCHEMA_VERSION
            || report.capture_schema_version != CAPTURE_SCHEMA_VERSION
            || report.capture_id != declaration.id
            || report.scene_id != manifest.scene_id
            || report.sidecar != declaration.sidecar
            || report.sidecar_sha256 != sha256_hex(&sidecar_bytes)
            || report.width != sidecar.metadata.render.width
            || report.height != sidecar.metadata.render.height
            || report.color_ppm_sha256 != sidecar.artifacts.color_ppm_sha256
            || report.diagnostic_sha256 != sidecar.artifacts.diagnostic_sha256
            || report.conversion != manifest.conversion
            || report.conversion_tool != manifest.conversion_tool
            || report.conversion_tool_sha256 != manifest.conversion_tool_sha256
            || !valid_hex(&report.identity_sha256, 64)
            || !valid_hex(&report.color_png_sha256, 64)
            || !valid_hex(&report.diagnostic_png_sha256, 64)
            || report.python.is_empty()
            || report.zlib.is_empty()
        {
            return Err(format!(
                "capture {} report is incomplete or stale",
                declaration.id
            ));
        }
        safe_relative(&report.color_png, "png")?;
        safe_relative(&report.diagnostic_png, "png")?;
        let expected_pixels = u64::from(report.width) * u64::from(report.height);
        if report.pixel_count != expected_pixels
            || report.sky_pixels + report.overlay_pixels + report.material_pixels != expected_pixels
            || report.material_pixels == 0
            || ![
                report.sky_fraction,
                report.overlay_fraction,
                report.material_fraction,
                report.luminance_mean,
                report.luminance_stddev,
                report.local_contrast_rms,
            ]
            .into_iter()
            .all(finite_fraction)
        {
            return Err(format!("capture {} metrics are invalid", declaration.id));
        }
        let sidecar_families: BTreeMap<u16, &DiagnosticFamily> = sidecar
            .metadata
            .family
            .iter()
            .map(|family| (family.diagnostic_id, family))
            .collect();
        let mut report_ids = BTreeSet::new();
        let mut family_pixels = 0u64;
        for family in &report.family {
            let Some(mapped) = sidecar_families.get(&family.diagnostic_id) else {
                return Err(format!(
                    "capture {} report names an unknown family",
                    declaration.id
                ));
            };
            if !report_ids.insert(family.diagnostic_id)
                || family.atlas_slot != mapped.atlas_slot
                || family.names != mapped.names
                || family.pixels == 0
                || !finite_fraction(family.pixel_fraction)
                || !finite_fraction(family.mean_normalized_depth)
            {
                return Err(format!(
                    "capture {} family report is invalid",
                    declaration.id
                ));
            }
            family_pixels = family_pixels.saturating_add(family.pixels);
        }
        if family_pixels != report.material_pixels {
            return Err(format!(
                "capture {} family totals do not reconcile",
                declaration.id
            ));
        }
        captures.push(sidecar.metadata);
        reports.push(report);
        report_bytes.push(this_report_bytes);
    }

    let first = &captures[0];
    let second = &captures[1];
    if first.capture_id == second.capture_id
        || first.scene_id != second.scene_id
        || first.build != second.build
        || first.world != second.world
        || first.camera != second.camera
        || first.environment != second.environment
        || first.render != second.render
        || first.family != second.family
        || reports[0].identity_sha256 != reports[1].identity_sha256
    {
        return Err("repeat captures do not have the same exact scene identity".into());
    }

    let comparison_relative = safe_relative(&manifest.comparison, "toml")?;
    if !manifest.comparison.starts_with("screenshots/")
        || !manifest.comparison.ends_with(".comparison.toml")
    {
        return Err("visual comparison uses the wrong role suffix".into());
    }
    let (_comparison_bytes, comparison): (Vec<u8>, VisualComparison) =
        read_toml(&root.join(comparison_relative), "visual comparison")?;
    if comparison.comparison_schema_version != COMPARISON_SCHEMA_VERSION
        || comparison.scene_id != manifest.scene_id
        || comparison.identity_sha256 != reports[0].identity_sha256
        || comparison.first_report != manifest.capture[0].report
        || comparison.second_report != manifest.capture[1].report
        || comparison.first_report_sha256 != sha256_hex(&report_bytes[0])
        || comparison.second_report_sha256 != sha256_hex(&report_bytes[1])
        || !comparison.same_identity
        || !comparison.same_dimensions
        || !comparison.equivalent
        || comparison.minimum_segmentation_agreement != 0.985
        || comparison.maximum_family_total_variation != 0.010
        || comparison.maximum_depth_rmse != 0.010
        || comparison.maximum_luminance_mean_delta != 0.015
        || comparison.maximum_local_contrast_delta != 0.015
        || comparison.maximum_sky_fraction_delta != 0.005
        || comparison.segmentation_agreement < comparison.minimum_segmentation_agreement
        || comparison.family_total_variation > comparison.maximum_family_total_variation
        || comparison.depth_rmse > comparison.maximum_depth_rmse
        || comparison.luminance_mean_delta > comparison.maximum_luminance_mean_delta
        || comparison.local_contrast_delta > comparison.maximum_local_contrast_delta
        || comparison.sky_fraction_delta > comparison.maximum_sky_fraction_delta
    {
        return Err("repeat comparison is incomplete, stale, or outside tolerance".into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_visual_polish_manifest(root: &Path) -> Result<(), String> {
    validate_visual_polish_manifest_at(root, &root.join("screenshots/visual-polish.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_paths_are_adjacent_and_fail_closed() {
        let paths = evidence_paths(Path::new("screenshots/foundation-a.ppm")).unwrap();
        assert_eq!(paths.diagnostic, Path::new("screenshots/foundation-a.wfd"));
        assert_eq!(
            paths.sidecar,
            Path::new("screenshots/foundation-a.capture.toml")
        );
        assert!(evidence_paths(Path::new("capture.png")).is_err());
    }

    #[test]
    fn wfd_encoder_removes_padding_and_has_a_versioned_header() {
        let mut gpu = vec![0u8; 256 * 2];
        let row0 = [1u16, 2, 3, 4, 5, 6, 7, 8];
        let row1 = [9u16, 10, 11, 12, 13, 14, 15, 16];
        for (row, values) in [row0, row1].iter().enumerate() {
            for (index, value) in values.iter().enumerate() {
                let at = row * 256 + index * 2;
                gpu[at..at + 2].copy_from_slice(&value.to_ne_bytes());
            }
        }
        let encoded = encode_wfd(2, 2, 256, &gpu).unwrap();
        assert_eq!(&encoded[..8], &WFD_MAGIC);
        assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(encoded[12..16].try_into().unwrap()), 2);
        assert_eq!(encoded.len(), WFD_HEADER_BYTES + 2 * 2 * 8);
        let decoded: Vec<u16> = encoded[WFD_HEADER_BYTES..]
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect();
        assert_eq!(decoded, row0.into_iter().chain(row1).collect::<Vec<_>>());
    }

    #[test]
    fn repository_visual_polish_evidence_is_complete() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(root).unwrap();
    }

    #[test]
    fn visual_polish_validator_rejects_incomplete_and_stale_manifests() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let original = fs::read_to_string(root.join("screenshots/visual-polish.toml")).unwrap();
        let scratch = std::env::temp_dir().join(format!(
            "wildforge-visual-manifest-negative-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch).unwrap();

        let incomplete = scratch.join("incomplete.toml");
        fs::write(
            &incomplete,
            original.replacen("[[capture]]", "[[omitted]]", 1),
        )
        .unwrap();
        assert!(validate_visual_polish_manifest_at(root, &incomplete).is_err());

        let stale = scratch.join("stale.toml");
        let manifest: VisualManifest = toml::from_str(&original).unwrap();
        fs::write(
            &stale,
            original.replacen(&manifest.qualification_source_sha256, &"0".repeat(64), 1),
        )
        .unwrap();
        assert!(validate_visual_polish_manifest_at(root, &stale).is_err());
        let _ = fs::remove_dir_all(&scratch);
    }
}
