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
    date: String,
    status: String,
    evidence_commit: String,
    baseline_commit: String,
    after_commit: String,
    scene_id: String,
    qualification_source_sha256: String,
    conversion: String,
    conversion_tool: String,
    conversion_tool_sha256: String,
    comparison: String,
    readability_report: String,
    performance_report: String,
    geode_evidence_commit: String,
    geode_qualification_source_sha256: String,
    geode_verifier: String,
    geode_verifier_sha256: String,
    geode_site_record: String,
    geode_preparation_report: String,
    geode_composition_report: String,
    geode_performance_report: String,
    capture: Vec<ManifestCapture>,
    site: Vec<ManifestSite>,
    case: Vec<ManifestCase>,
    performance: Vec<ManifestPerformance>,
    geode_capture: Vec<ManifestGeodeCapture>,
    geode_performance_capture: Vec<ManifestPerformance>,
    closeout: ManifestCloseout,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCloseout {
    commit: String,
    date: String,
    verifier: String,
    verifier_sha256: String,
    readability_report: String,
    strata_performance_report: String,
    geode_composition_report: String,
    geode_performance_report: String,
    motion_report: String,
    case: Vec<ManifestCase>,
    performance: Vec<ManifestPerformance>,
    geode_capture: Vec<ManifestGeodeCapture>,
    geode_performance_capture: Vec<ManifestPerformance>,
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
struct ManifestSite {
    rock: String,
    face: String,
    u: i32,
    y: i32,
    v: i32,
    exposure: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCase {
    id: String,
    phase: String,
    rock: String,
    role: String,
    light: String,
    weather: String,
    pack: String,
    view_distance_chunks: i32,
    sidecar: String,
    report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestPerformance {
    id: String,
    phase: String,
    sidecar: String,
    report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestGeodeCapture {
    id: String,
    purpose: String,
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
    #[serde(default)]
    stratum: Vec<ReportStratum>,
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
struct ReportStratum {
    rock: String,
    distance_band: String,
    pixels: u64,
    coverage: f64,
    connected_components: u64,
    one_pixel_fringe: u64,
    one_pixel_fringe_fraction: f64,
    median_luminance: f64,
    p10_luminance: f64,
    p90_luminance: f64,
    luminance_span: f64,
    rms_contrast_1px: f64,
    rms_contrast_4px: f64,
    rms_contrast_16px: f64,
    contrast_pairs_1px: u64,
    contrast_pairs_4px: u64,
    contrast_pairs_16px: u64,
    adjacent_sky_pairs: u64,
    silhouette_weber: f64,
    silhouette_weber_magnitude: f64,
    median_chroma: f64,
    greyscale_structure_score: f64,
    expected_fog_blend: f64,
    display_black_fraction: f64,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct QualificationReport {
    qualification_schema_version: u32,
    kind: String,
    baseline_commit: String,
    after_commit: String,
    passed: bool,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeodeCompositionReport {
    qualification_schema_version: u32,
    kind: String,
    evidence_commit: String,
    hero_capture_id: String,
    hero_report: String,
    hero_report_sha256: String,
    proof_capture_id: String,
    proof_report: String,
    proof_report_sha256: String,
    width: u32,
    height: u32,
    host_pixels: u64,
    host_fraction: f64,
    minimum_host_fraction: f64,
    quartz_pixels: u64,
    amethyst_pixels: u64,
    minimum_lining_pixels: u64,
    heart_dark_deep_pixels: u64,
    minimum_heart_dark_deep_pixels: u64,
    lip_left_pixels: u64,
    lip_right_pixels: u64,
    lip_top_pixels: u64,
    lip_bottom_pixels: u64,
    lip_sides: Vec<String>,
    minimum_lip_sides: usize,
    sky_pixels: u64,
    overlay_fraction: f64,
    maximum_overlay_fraction: f64,
    proof_host_fraction: f64,
    proof_quartz_pixels: u64,
    proof_amethyst_pixels: u64,
    sealed_reload_camera_match: bool,
    sealed_reload_environment_match: bool,
    sealed_reload_render_match: bool,
    passed: bool,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeodePerformanceReport {
    qualification_schema_version: u32,
    kind: String,
    evidence_commit: String,
    sample_count_per_phase: usize,
    sealed_draw_ms: Vec<f64>,
    opened_draw_ms: Vec<f64>,
    sealed_simulation_ms: Vec<f64>,
    opened_simulation_ms: Vec<f64>,
    sealed_median_draw_ms: f64,
    opened_median_draw_ms: f64,
    median_draw_regression_ms: f64,
    maximum_median_draw_regression_ms: f64,
    sealed_median_simulation_ms: f64,
    opened_median_simulation_ms: f64,
    median_simulation_regression_ms: f64,
    maximum_median_simulation_regression_ms: f64,
    passed: bool,
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
    campaign::source_sha256(root)
}

#[cfg(test)]
fn validate_strata_metrics(report: &VisualReport, label: &str) -> Result<(), String> {
    const ROCKS: &[&str] = &[
        "sandstone",
        "limestone",
        "shale",
        "granite",
        "marble",
        "slate",
        "quartzite",
        "basalt",
    ];
    const BANDS: &[&str] = &["near", "middle", "pre-fog", "fog"];
    let mut keys = BTreeSet::new();
    for row in &report.stratum {
        let finite_nonnegative = [
            row.median_luminance,
            row.p10_luminance,
            row.p90_luminance,
            row.luminance_span,
            row.rms_contrast_1px,
            row.rms_contrast_4px,
            row.rms_contrast_16px,
            row.silhouette_weber_magnitude,
            row.median_chroma,
            row.greyscale_structure_score,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0);
        if !ROCKS.contains(&row.rock.as_str())
            || !BANDS.contains(&row.distance_band.as_str())
            || !keys.insert((&row.rock, &row.distance_band))
            || row.pixels == 0
            || row.connected_components == 0
            || row.one_pixel_fringe > row.pixels
            || row.contrast_pairs_1px == 0
            || row.adjacent_sky_pairs > row.pixels.saturating_mul(4)
            || !finite_fraction(row.coverage)
            || !finite_fraction(row.one_pixel_fringe_fraction)
            || !finite_fraction(row.expected_fog_blend)
            || !finite_fraction(row.display_black_fraction)
            || !row.silhouette_weber.is_finite()
            || row.contrast_pairs_4px > row.contrast_pairs_1px.saturating_mul(2)
            || row.contrast_pairs_16px > row.contrast_pairs_1px.saturating_mul(2)
            || !finite_nonnegative
        {
            return Err(format!("{label} has invalid per-stratum metrics"));
        }
    }
    Ok(())
}

#[cfg(test)]
fn validate_declared_evidence(
    root: &Path,
    declaration_id: &str,
    sidecar_value: &str,
    report_value: &str,
    expected_scene: &str,
    expected_commit: &str,
    expected_size: (u32, u32),
) -> Result<(CaptureMetadata, VisualReport), String> {
    let sidecar_relative = safe_relative(sidecar_value, "toml")?;
    let report_relative = safe_relative(report_value, "toml")?;
    if !sidecar_value.starts_with("screenshots/")
        || !sidecar_value.ends_with(".capture.toml")
        || !report_value.starts_with("screenshots/visual-polish/")
        || !report_value.ends_with(".report.toml")
    {
        return Err(format!(
            "{declaration_id} uses the wrong evidence path role"
        ));
    }
    let (sidecar_bytes, sidecar): (Vec<u8>, CaptureSidecar) =
        read_toml(&root.join(sidecar_relative), "capture sidecar")?;
    let (_report_bytes, report): (Vec<u8>, VisualReport) =
        read_toml(&root.join(report_relative), "visual report")?;
    let metadata = &sidecar.metadata;
    if metadata.schema_version != CAPTURE_SCHEMA_VERSION
        || metadata.capture_id != declaration_id
        || metadata.scene_id != expected_scene
        || metadata.build.commit != expected_commit
        || metadata.build.dirty
        || metadata.world.seed != 20_260_802
        || metadata.world.generator_version != crate::world::WORLD_GENERATOR_VERSION
        || metadata.world.atlas_format_version != crate::planet_atlas::ATLAS_FORMAT_VERSION
        || metadata.world.atlas_algorithm_version != crate::planet_atlas::ATLAS_ALGORITHM_VERSION
        || metadata.world.atlas_content_hash != "010c5397ca037176"
        || metadata.world.atlas_genesis_checksum != "b053756eee79d7e7"
        || (metadata.render.width, metadata.render.height) != expected_size
        || metadata.render.adapter != "NVIDIA GeForce RTX 3090 [Dx12, DiscreteGpu]"
        || metadata.render.backend != "Dx12"
        || !metadata.render.hardware
        || !metadata.telemetry.settled
        || metadata.telemetry.settled_frames < crate::game::SHOT_SETTLE_FRAMES
        || metadata.family.is_empty()
    {
        return Err(format!(
            "{declaration_id} has incomplete or stale native-GPU identity"
        ));
    }
    if report.report_schema_version != REPORT_SCHEMA_VERSION
        || report.capture_schema_version != CAPTURE_SCHEMA_VERSION
        || report.capture_id != declaration_id
        || report.scene_id != expected_scene
        || report.sidecar != sidecar_value
        || report.sidecar_sha256 != sha256_hex(&sidecar_bytes)
        || report.width != metadata.render.width
        || report.height != metadata.render.height
        || report.color_ppm_sha256 != sidecar.artifacts.color_ppm_sha256
        || report.diagnostic_sha256 != sidecar.artifacts.diagnostic_sha256
        || report.material_pixels == 0
    {
        return Err(format!(
            "{declaration_id} has an incomplete or stale report"
        ));
    }
    validate_strata_metrics(&report, declaration_id)?;
    Ok((sidecar.metadata, report))
}

#[cfg(test)]
fn validate_visual_polish_manifest_at(
    root: &Path,
    manifest_path: &Path,
    source_root: &Path,
) -> Result<(), String> {
    let (_manifest_bytes, manifest): (Vec<u8>, VisualManifest) =
        read_toml(manifest_path, "visual-polish manifest")?;
    if manifest.schema_version != 4
        || manifest.status != "accepted"
        || !campaign::valid_date(&manifest.date)
    {
        return Err("visual-polish manifest is not accepted schema 4 evidence".into());
    }
    if !valid_hex(&manifest.evidence_commit, 40)
        || !valid_hex(&manifest.baseline_commit, 40)
        || !valid_hex(&manifest.after_commit, 40)
        || manifest.baseline_commit == manifest.after_commit
        || !valid_hex(&manifest.qualification_source_sha256, 64)
        || manifest.scene_id.is_empty()
    {
        return Err("visual-polish manifest identity is incomplete".into());
    }
    let current_source = qualification_source_sha256(source_root)?;
    if current_source != manifest.qualification_source_sha256 {
        return Err(format!(
            "visual-polish evidence is stale relative to qualification source: manifest {}, current {current_source}",
            manifest.qualification_source_sha256
        ));
    }
    if manifest.conversion != CONVERSION_ID
        || manifest.conversion_tool != "tools/verify_visual_polish.py"
        || !valid_hex(&manifest.conversion_tool_sha256, 64)
    {
        return Err("visual-polish conversion provenance is incomplete".into());
    }
    let tool_path = source_root.join(safe_relative(&manifest.conversion_tool, "py")?);
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

    let expected_rocks: BTreeSet<&str> = [
        "sandstone",
        "limestone",
        "shale",
        "granite",
        "marble",
        "slate",
        "quartzite",
        "basalt",
    ]
    .into_iter()
    .collect();
    if manifest.site.len() != expected_rocks.len() {
        return Err("strata manifest must record one production site per rock".into());
    }
    let mut site_rocks = BTreeSet::new();
    for site in &manifest.site {
        if !site_rocks.insert(site.rock.as_str())
            || !matches!(site.face.as_str(), "NegZ" | "PosX" | "NegX")
            || !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&site.u)
            || !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&site.v)
            || !(0..512).contains(&site.y)
            || !matches!(site.exposure.as_str(), "outdoor" | "cave")
        {
            return Err("strata manifest contains an invalid production site".into());
        }
    }
    if site_rocks != expected_rocks {
        return Err("strata manifest production sites do not cover all eight rocks".into());
    }

    if manifest.case.len() != 28 {
        return Err("strata manifest requires fourteen baseline and fourteen after cases".into());
    }
    let mut case_ids = BTreeSet::new();
    let mut phases = BTreeMap::<&str, usize>::new();
    let mut rocks = BTreeSet::new();
    let mut roles = BTreeSet::new();
    let mut lights = BTreeSet::new();
    let mut weather = BTreeSet::new();
    let mut packs = BTreeSet::new();
    let mut views = BTreeSet::new();
    for case in &manifest.case {
        if !case_ids.insert(case.id.as_str())
            || !matches!(case.phase.as_str(), "baseline" | "after")
            || !matches!(case.role.as_str(), "near" | "pre-fog" | "supplement")
        {
            return Err("strata manifest contains a duplicate or invalid case".into());
        }
        let expected_commit = if case.phase == "baseline" {
            &manifest.baseline_commit
        } else {
            &manifest.after_commit
        };
        let (metadata, report) = validate_declared_evidence(
            root,
            &case.id,
            &case.sidecar,
            &case.report,
            &format!("strata-production-{}", manifest.date.replace('-', "")),
            expected_commit,
            (1280, 720),
        )?;
        let expected_pack = if case.pack == "base" { "" } else { &case.pack };
        if metadata.world.name != "visual-polish-strata-baseline"
            || metadata.environment.weather != case.weather
            || metadata.render.pack != expected_pack
            || metadata.render.view_distance_chunks != case.view_distance_chunks
            || !report.stratum.iter().any(|row| row.rock == case.rock)
        {
            return Err(format!(
                "{} does not match its declared matrix axes",
                case.id
            ));
        }
        *phases.entry(&case.phase).or_default() += 1;
        rocks.insert(case.rock.as_str());
        roles.insert(case.role.as_str());
        lights.insert(case.light.as_str());
        weather.insert(case.weather.as_str());
        packs.insert(case.pack.as_str());
        views.insert(case.view_distance_chunks);
    }
    if phases.get("baseline") != Some(&14)
        || phases.get("after") != Some(&14)
        || !["sandstone", "limestone", "marble", "quartzite", "basalt"]
            .into_iter()
            .all(|value| rocks.contains(value))
        || !["near", "pre-fog", "supplement"]
            .into_iter()
            .all(|value| roles.contains(value))
        || !["noon", "dawn"]
            .into_iter()
            .all(|value| lights.contains(value))
        || !["clear", "overcast", "precipitation"]
            .into_iter()
            .all(|value| weather.contains(value))
        || !["base", "gemini", "dusk", "hewn"]
            .into_iter()
            .all(|value| packs.contains(value))
        || !views.contains(&4)
        || !views.contains(&12)
    {
        return Err("strata manifest does not cover the required capture matrix".into());
    }

    if manifest.performance.len() != 10 {
        return Err("strata performance evidence requires five matched captures per phase".into());
    }
    let mut performance_ids = BTreeSet::new();
    let mut performance_phases = BTreeMap::<&str, usize>::new();
    for declaration in &manifest.performance {
        if !performance_ids.insert(declaration.id.as_str())
            || !matches!(declaration.phase.as_str(), "baseline" | "after")
        {
            return Err("strata manifest contains duplicate performance evidence".into());
        }
        let expected_commit = if declaration.phase == "baseline" {
            &manifest.baseline_commit
        } else {
            &manifest.after_commit
        };
        let (metadata, _report) = validate_declared_evidence(
            root,
            &declaration.id,
            &declaration.sidecar,
            &declaration.report,
            &format!("strata-performance-{}", manifest.date.replace('-', "")),
            expected_commit,
            (1280, 720),
        )?;
        if metadata.world.name != "visual-polish-strata-perf"
            || metadata.environment.weather != "clear"
            || metadata.render.pack != "gemini"
            || metadata.render.view_distance_chunks != 12
        {
            return Err(format!(
                "{} is not the matched performance scene",
                declaration.id
            ));
        }
        *performance_phases.entry(&declaration.phase).or_default() += 1;
    }
    if performance_phases.get("baseline") != Some(&5) || performance_phases.get("after") != Some(&5)
    {
        return Err("strata performance phases are incomplete".into());
    }

    for (path_value, kind) in [
        (&manifest.readability_report, "strata-readability"),
        (&manifest.performance_report, "strata-performance"),
    ] {
        let path = safe_relative(path_value, "toml")?;
        if !path_value.starts_with("screenshots/visual-polish/")
            || !path_value.ends_with(".report.toml")
        {
            return Err("strata qualification report path has the wrong role".into());
        }
        let (_bytes, qualification): (Vec<u8>, QualificationReport) =
            read_toml(&root.join(path), "strata qualification")?;
        if qualification.qualification_schema_version != 1
            || qualification.kind != kind
            || qualification.baseline_commit != manifest.baseline_commit
            || qualification.after_commit != manifest.after_commit
            || !qualification.passed
        {
            return Err(format!("{kind} qualification is incomplete or failed"));
        }
    }
    validate_cracked_geode_manifest(root, &manifest, source_root)?;
    validate_closeout_manifest(root, &manifest, source_root)?;
    Ok(())
}

#[cfg(test)]
fn validate_cracked_geode_manifest(
    root: &Path,
    manifest: &VisualManifest,
    source_root: &Path,
) -> Result<(), String> {
    let date = manifest.date.replace('-', "");
    let scene = format!("cracked-geode-{date}");
    let performance_scene = format!("cracked-geode-performance-{date}");
    if !valid_hex(&manifest.geode_evidence_commit, 40)
        || !valid_hex(&manifest.geode_qualification_source_sha256, 64)
        || manifest.geode_verifier != "tools/verify_cracked_geode.py"
        || !valid_hex(&manifest.geode_verifier_sha256, 64)
        || manifest.geode_site_record != "screenshots/visual-polish/cracked-geode-site.toml"
        || manifest.geode_preparation_report
            != "screenshots/visual-polish/geode-preparation.report.toml"
        || manifest.geode_composition_report
            != "screenshots/visual-polish/geode-composition.report.toml"
        || manifest.geode_performance_report
            != "screenshots/visual-polish/geode-performance.report.toml"
    {
        return Err("cracked-geode manifest provenance is incomplete".into());
    }
    let current_source = qualification_source_sha256(source_root)?;
    if current_source != manifest.geode_qualification_source_sha256 {
        return Err(format!(
            "cracked-geode evidence is stale relative to qualification source: manifest {}, current {current_source}",
            manifest.geode_qualification_source_sha256
        ));
    }
    let verifier = source_root.join(safe_relative(&manifest.geode_verifier, "py")?);
    let verifier_hash = sha256_hex(
        &fs::read(&verifier)
            .map_err(|error| format!("read geode verifier {}: {error}", verifier.display()))?,
    );
    if verifier_hash != manifest.geode_verifier_sha256 {
        return Err("cracked-geode verifier hash is stale".into());
    }

    let (_site_bytes, site): (Vec<u8>, toml::Value) = read_toml(
        &root.join(safe_relative(&manifest.geode_site_record, "toml")?),
        "cracked-geode site",
    )?;
    let (_preparation_bytes, preparation): (Vec<u8>, toml::Value) = read_toml(
        &root.join(safe_relative(&manifest.geode_preparation_report, "toml")?),
        "cracked-geode preparation",
    )?;
    if site.get("site_id").and_then(toml::Value::as_str) != Some("cracked-geode")
        || site.get("deposit_id").and_then(toml::Value::as_integer) != Some(48)
        || site.get("host_geology").and_then(toml::Value::as_str) != Some("limestone")
        || site
            .get("shell_six_connected")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || site.get("heart_sealed").and_then(toml::Value::as_bool) != Some(true)
        || site
            .get("peak_rss_bytes")
            .and_then(toml::Value::as_integer)
            .unwrap_or(i64::MAX)
            > 512 * 1024 * 1024
        || preparation.get("site_id").and_then(toml::Value::as_str) != Some("cracked-geode")
        || preparation
            .get("deposit_id")
            .and_then(toml::Value::as_integer)
            != Some(48)
        || preparation
            .get("source_unchanged")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || preparation
            .get("before_balanced")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || preparation
            .get("after_balanced")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || preparation
            .get("reload_balanced")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || preparation
            .get("unexpected_edits")
            .and_then(toml::Value::as_integer)
            != Some(0)
        || preparation
            .get("save_succeeded")
            .and_then(toml::Value::as_bool)
            != Some(true)
        || preparation
            .get("reload_succeeded")
            .and_then(toml::Value::as_bool)
            != Some(true)
    {
        return Err("cracked-geode site or preparation proof is incomplete".into());
    }

    validate_geode_capture_group(
        root,
        manifest,
        &manifest.geode_capture,
        &manifest.geode_performance_capture,
        &GeodeGroupExpectation {
            id_prefix: "geode",
            scene: &scene,
            performance_scene: &performance_scene,
            commit: &manifest.geode_evidence_commit,
            composition_report: &manifest.geode_composition_report,
            performance_report: &manifest.geode_performance_report,
            composition_kind: "cracked-geode-composition",
            performance_kind: "cracked-geode-performance",
        },
    )
}

#[cfg(test)]
struct GeodeGroupExpectation<'a> {
    id_prefix: &'a str,
    scene: &'a str,
    performance_scene: &'a str,
    commit: &'a str,
    composition_report: &'a str,
    performance_report: &'a str,
    composition_kind: &'a str,
    performance_kind: &'a str,
}

#[cfg(test)]
fn validate_geode_capture_group(
    root: &Path,
    manifest: &VisualManifest,
    declarations: &[ManifestGeodeCapture],
    performance_declarations: &[ManifestPerformance],
    expected: &GeodeGroupExpectation<'_>,
) -> Result<(), String> {
    let sealed_id = format!("{}-sealed-context", expected.id_prefix);
    let proof_id = format!("{}-aperture-proof", expected.id_prefix);
    let hero_id = format!("{}-cracked-hero", expected.id_prefix);
    let reload_id = format!("{}-reload-proof", expected.id_prefix);
    let purposes: BTreeMap<&str, &str> = [
        (sealed_id.as_str(), "sealed-context"),
        (proof_id.as_str(), "aperture-proof"),
        (hero_id.as_str(), "hero"),
        (reload_id.as_str(), "reload-proof"),
    ]
    .into_iter()
    .collect();
    if declarations.len() != purposes.len() {
        return Err("cracked-geode evidence requires exactly four primary captures".into());
    }
    let mut captures = BTreeMap::<String, CaptureMetadata>::new();
    let mut reports = BTreeMap::<String, (VisualReport, Vec<u8>)>::new();
    for declaration in declarations {
        let Some(expected_purpose) = purposes.get(declaration.id.as_str()) else {
            return Err("cracked-geode manifest names an unexpected primary capture".into());
        };
        if declaration.purpose != *expected_purpose || captures.contains_key(&declaration.id) {
            return Err("cracked-geode primary capture purpose or identity is invalid".into());
        }
        let (metadata, report) = validate_declared_evidence(
            root,
            &declaration.id,
            &declaration.sidecar,
            &declaration.report,
            expected.scene,
            expected.commit,
            (1920, 1080),
        )?;
        let expected_world = if declaration.id == sealed_id {
            "visual-polish-geode-sealed"
        } else {
            "visual-polish-geode-opened"
        };
        if metadata.world.name != expected_world
            || metadata.render.pack != "gemini"
            || metadata.render.view_distance_chunks != 12
            || metadata.render.lights != 2
            || !metadata.render.point_grid
            || !metadata.render.stark
            || !metadata.render.bloom
            || metadata.camera.fov_degrees != 75.0
            || metadata.environment.weather != "clear"
            || report.conversion != CONVERSION_ID
            || report.conversion_tool != "tools/verify_visual_polish.py"
            || report.conversion_tool_sha256 != manifest.conversion_tool_sha256
        {
            return Err(format!(
                "{} does not match the declared geode scene",
                declaration.id
            ));
        }
        let (report_bytes, _): (Vec<u8>, VisualReport) = read_toml(
            &root.join(safe_relative(&declaration.report, "toml")?),
            "cracked-geode report",
        )?;
        captures.insert(declaration.id.clone(), metadata);
        reports.insert(declaration.id.clone(), (report, report_bytes));
    }
    let sealed = &captures[sealed_id.as_str()];
    let reloaded = &captures[reload_id.as_str()];
    if sealed.camera != reloaded.camera
        || sealed.environment != reloaded.environment
        || sealed.render != reloaded.render
        || sealed.camera.u != 5408.5
        || sealed.camera.y != 68.02
        || sealed.camera.v != 1723.5
        || sealed.camera.yaw != std::f32::consts::PI
        || sealed.camera.pitch != -1.35
    {
        return Err(
            "sealed and reload proof do not use the exact matched camera and render identity"
                .into(),
        );
    }
    let hero = &captures[hero_id.as_str()];
    if hero.camera.u != 5406.5
        || hero.camera.y != 62.000_008
        || hero.camera.v != 1723.5
        || hero.camera.yaw != std::f32::consts::PI
        || hero.camera.pitch != -0.08
        || hero.environment.time_of_day != 0.75
    {
        return Err("cracked-geode hero camera or lighting identity is stale".into());
    }

    if performance_declarations.len() != 10 {
        return Err("cracked-geode performance requires five matched captures per phase".into());
    }
    let mut performance_ids = BTreeSet::new();
    let mut phase_counts = BTreeMap::<&str, usize>::new();
    let mut samples = BTreeMap::<&str, Vec<f64>>::new();
    let mut reference_camera: Option<CameraIdentity> = None;
    let mut reference_environment: Option<EnvironmentIdentity> = None;
    let mut reference_render: Option<RenderIdentity> = None;
    for declaration in performance_declarations {
        if !performance_ids.insert(declaration.id.as_str())
            || !matches!(declaration.phase.as_str(), "sealed" | "opened")
            || !declaration.id.starts_with(&format!(
                "{}-performance-{}-",
                expected.id_prefix, declaration.phase
            ))
        {
            return Err("cracked-geode performance declaration is duplicate or malformed".into());
        }
        let (metadata, report) = validate_declared_evidence(
            root,
            &declaration.id,
            &declaration.sidecar,
            &declaration.report,
            expected.performance_scene,
            expected.commit,
            (1280, 720),
        )?;
        if metadata.world.name != format!("visual-polish-geode-{}", declaration.phase)
            || metadata.render.pack != "gemini"
            || metadata.render.view_distance_chunks != 12
            || metadata.environment.weather != "clear"
            || report.conversion_tool_sha256 != manifest.conversion_tool_sha256
        {
            return Err(format!(
                "{} is not the matched geode performance scene",
                declaration.id
            ));
        }
        if let Some(camera) = &reference_camera {
            if camera != &metadata.camera
                || reference_environment.as_ref() != Some(&metadata.environment)
                || reference_render.as_ref() != Some(&metadata.render)
            {
                return Err(
                    "cracked-geode performance captures are not exact matched pairs".into(),
                );
            }
        } else {
            reference_camera = Some(metadata.camera.clone());
            reference_environment = Some(metadata.environment.clone());
            reference_render = Some(metadata.render.clone());
        }
        *phase_counts.entry(&declaration.phase).or_default() += 1;
        samples
            .entry(if declaration.phase == "sealed" {
                "sealed_draw"
            } else {
                "opened_draw"
            })
            .or_default()
            .push(f64::from(metadata.telemetry.draw_ms));
        samples
            .entry(if declaration.phase == "sealed" {
                "sealed_simulation"
            } else {
                "opened_simulation"
            })
            .or_default()
            .push(f64::from(metadata.telemetry.simulation_ms));
    }
    if phase_counts.get("sealed") != Some(&5) || phase_counts.get("opened") != Some(&5) {
        return Err("cracked-geode performance phase coverage is incomplete".into());
    }

    let composition_path = root.join(safe_relative(expected.composition_report, "toml")?);
    let (_composition_bytes, composition): (Vec<u8>, GeodeCompositionReport) =
        read_toml(&composition_path, "cracked-geode composition")?;
    let hero_report = &reports[hero_id.as_str()];
    let proof_report = &reports[proof_id.as_str()];
    let lip_counts = [
        composition.lip_left_pixels,
        composition.lip_right_pixels,
        composition.lip_top_pixels,
        composition.lip_bottom_pixels,
    ];
    if composition.qualification_schema_version != 1
        || composition.kind != expected.composition_kind
        || composition.evidence_commit != expected.commit
        || composition.hero_capture_id != hero_id
        || composition.hero_report
            != declarations
                .iter()
                .find(|item| item.id == composition.hero_capture_id)
                .unwrap()
                .report
        || composition.hero_report_sha256 != sha256_hex(&hero_report.1)
        || composition.proof_capture_id != proof_id
        || composition.proof_report
            != declarations
                .iter()
                .find(|item| item.id == composition.proof_capture_id)
                .unwrap()
                .report
        || composition.proof_report_sha256 != sha256_hex(&proof_report.1)
        || (composition.width, composition.height) != (1920, 1080)
        || composition.host_pixels == 0
        || composition.host_fraction < composition.minimum_host_fraction
        || composition.minimum_host_fraction != 0.15
        || composition.quartz_pixels < composition.minimum_lining_pixels
        || composition.amethyst_pixels < composition.minimum_lining_pixels
        || composition.minimum_lining_pixels != 256
        || composition.heart_dark_deep_pixels < composition.minimum_heart_dark_deep_pixels
        || composition.minimum_heart_dark_deep_pixels != 256
        || composition.minimum_lip_sides != 3
        || composition.lip_sides.len() < composition.minimum_lip_sides
        || lip_counts
            .into_iter()
            .filter(|pixels| *pixels >= 256)
            .count()
            < 3
        || composition.sky_pixels != 0
        || composition.overlay_fraction > composition.maximum_overlay_fraction
        || composition.maximum_overlay_fraction != 0.04
        || composition.proof_host_fraction < 0.15
        || composition.proof_quartz_pixels < 256
        || composition.proof_amethyst_pixels < 256
        || !composition.sealed_reload_camera_match
        || !composition.sealed_reload_environment_match
        || !composition.sealed_reload_render_match
        || !composition.passed
    {
        return Err("cracked-geode composition qualification is incomplete or failed".into());
    }

    let performance_path = root.join(safe_relative(expected.performance_report, "toml")?);
    let (_performance_bytes, performance): (Vec<u8>, GeodePerformanceReport) =
        read_toml(&performance_path, "cracked-geode performance")?;
    let median = |values: &[f64]| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[sorted.len() / 2]
    };
    let sealed_draw = &samples["sealed_draw"];
    let opened_draw = &samples["opened_draw"];
    let sealed_simulation = &samples["sealed_simulation"];
    let opened_simulation = &samples["opened_simulation"];
    let close = |left: f64, right: f64| (left - right).abs() <= 0.000_002;
    if performance.qualification_schema_version != 1
        || performance.kind != expected.performance_kind
        || performance.evidence_commit != expected.commit
        || performance.sample_count_per_phase != 5
        || performance.sealed_draw_ms.len() != 5
        || performance.opened_draw_ms.len() != 5
        || performance.sealed_simulation_ms.len() != 5
        || performance.opened_simulation_ms.len() != 5
        || !performance
            .sealed_draw_ms
            .iter()
            .zip(sealed_draw)
            .all(|(a, b)| close(*a, *b))
        || !performance
            .opened_draw_ms
            .iter()
            .zip(opened_draw)
            .all(|(a, b)| close(*a, *b))
        || !performance
            .sealed_simulation_ms
            .iter()
            .zip(sealed_simulation)
            .all(|(a, b)| close(*a, *b))
        || !performance
            .opened_simulation_ms
            .iter()
            .zip(opened_simulation)
            .all(|(a, b)| close(*a, *b))
        || !close(performance.sealed_median_draw_ms, median(sealed_draw))
        || !close(performance.opened_median_draw_ms, median(opened_draw))
        || !close(
            performance.median_draw_regression_ms,
            median(opened_draw) - median(sealed_draw),
        )
        || performance.maximum_median_draw_regression_ms != 0.20
        || performance.median_draw_regression_ms > performance.maximum_median_draw_regression_ms
        || !close(
            performance.sealed_median_simulation_ms,
            median(sealed_simulation),
        )
        || !close(
            performance.opened_median_simulation_ms,
            median(opened_simulation),
        )
        || !close(
            performance.median_simulation_regression_ms,
            median(opened_simulation) - median(sealed_simulation),
        )
        || performance.maximum_median_simulation_regression_ms != 0.10
        || performance.median_simulation_regression_ms
            > performance.maximum_median_simulation_regression_ms
        || !performance.passed
    {
        return Err("cracked-geode performance qualification is incomplete or failed".into());
    }
    Ok(())
}

#[cfg(test)]
fn validate_closeout_manifest(
    root: &Path,
    manifest: &VisualManifest,
    source_root: &Path,
) -> Result<(), String> {
    let date = manifest.date.replace('-', "");
    let strata_scene = format!("closeout-strata-{date}");
    let strata_performance_scene = format!("closeout-strata-performance-{date}");
    let geode_scene = format!("closeout-geode-{date}");
    let geode_performance_scene = format!("closeout-geode-performance-{date}");
    let closeout = &manifest.closeout;
    if !valid_hex(&closeout.commit, 40)
        || closeout.commit == manifest.baseline_commit
        || closeout.date != manifest.date
        || closeout.verifier != "tools/verify_visual_closeout.py"
        || !valid_hex(&closeout.verifier_sha256, 64)
    {
        return Err("closeout identity is incomplete".into());
    }
    let verifier = source_root.join(safe_relative(&closeout.verifier, "py")?);
    let verifier_hash = sha256_hex(
        &fs::read(&verifier)
            .map_err(|error| format!("read closeout verifier {}: {error}", verifier.display()))?,
    );
    if verifier_hash != closeout.verifier_sha256 {
        return Err("closeout verifier hash is stale".into());
    }

    // The closeout matrix re-runs every goal-2 "after" case from one commit.
    let mut after_axes = BTreeMap::new();
    for case in &manifest.case {
        if case.phase == "after" {
            let suffix = case
                .id
                .strip_prefix("strata-after-")
                .ok_or_else(|| format!("{} does not use the after id prefix", case.id))?;
            after_axes.insert(
                suffix.to_string(),
                (
                    case.rock.clone(),
                    case.role.clone(),
                    case.light.clone(),
                    case.weather.clone(),
                    case.pack.clone(),
                    case.view_distance_chunks,
                ),
            );
        }
    }
    if closeout.case.len() != after_axes.len() {
        return Err("closeout must re-run every goal-2 after case".into());
    }
    let mut seen = BTreeSet::new();
    for case in &closeout.case {
        let suffix = case
            .id
            .strip_prefix("strata-closeout-")
            .ok_or_else(|| format!("{} does not use the closeout id prefix", case.id))?;
        let Some(axes) = after_axes.get(suffix) else {
            return Err(format!("{} does not mirror a goal-2 after case", case.id));
        };
        if !seen.insert(suffix.to_string())
            || case.phase != "closeout"
            || (
                case.rock.clone(),
                case.role.clone(),
                case.light.clone(),
                case.weather.clone(),
                case.pack.clone(),
                case.view_distance_chunks,
            ) != *axes
        {
            return Err(format!(
                "{} does not mirror its goal-2 matrix axes",
                case.id
            ));
        }
        let (metadata, report) = validate_declared_evidence(
            root,
            &case.id,
            &case.sidecar,
            &case.report,
            &strata_scene,
            &closeout.commit,
            (1280, 720),
        )?;
        let expected_pack = if case.pack == "base" { "" } else { &case.pack };
        if metadata.world.name != "visual-polish-strata-baseline"
            || metadata.environment.weather != case.weather
            || metadata.render.pack != expected_pack
            || metadata.render.view_distance_chunks != case.view_distance_chunks
            || !report.stratum.iter().any(|row| row.rock == case.rock)
        {
            return Err(format!(
                "{} does not match its declared matrix axes",
                case.id
            ));
        }
    }

    if closeout.performance.len() != 10 {
        return Err("closeout strata performance requires five matched captures per phase".into());
    }
    let mut performance_ids = BTreeSet::new();
    let mut performance_phases = BTreeMap::<&str, usize>::new();
    for declaration in &closeout.performance {
        if !performance_ids.insert(declaration.id.as_str())
            || !matches!(declaration.phase.as_str(), "baseline" | "closeout")
            || !declaration.id.starts_with(&format!(
                "strata-closeout-performance-{}-",
                declaration.phase
            ))
        {
            return Err("closeout strata performance declaration is duplicate or malformed".into());
        }
        let expected_commit = if declaration.phase == "baseline" {
            &manifest.baseline_commit
        } else {
            &closeout.commit
        };
        let (metadata, _report) = validate_declared_evidence(
            root,
            &declaration.id,
            &declaration.sidecar,
            &declaration.report,
            &strata_performance_scene,
            expected_commit,
            (1280, 720),
        )?;
        if metadata.world.name != "visual-polish-strata-perf"
            || metadata.environment.weather != "clear"
            || metadata.render.pack != "gemini"
            || metadata.render.view_distance_chunks != 12
        {
            return Err(format!(
                "{} is not the matched performance scene",
                declaration.id
            ));
        }
        *performance_phases.entry(&declaration.phase).or_default() += 1;
    }
    if performance_phases.get("baseline") != Some(&5)
        || performance_phases.get("closeout") != Some(&5)
    {
        return Err("closeout strata performance phases are incomplete".into());
    }

    for (path_value, kind) in [
        (&closeout.readability_report, "closeout-readability"),
        (
            &closeout.strata_performance_report,
            "closeout-strata-performance",
        ),
    ] {
        let path = safe_relative(path_value, "toml")?;
        if !path_value.starts_with("screenshots/visual-polish/")
            || !path_value.ends_with(".report.toml")
        {
            return Err("closeout qualification report path has the wrong role".into());
        }
        let (_bytes, qualification): (Vec<u8>, QualificationReport) =
            read_toml(&root.join(path), "closeout qualification")?;
        if qualification.qualification_schema_version != 1
            || qualification.kind != kind
            || qualification.baseline_commit != manifest.baseline_commit
            || qualification.after_commit != closeout.commit
            || !qualification.passed
        {
            return Err(format!("{kind} qualification is incomplete or failed"));
        }
    }

    validate_geode_capture_group(
        root,
        manifest,
        &closeout.geode_capture,
        &closeout.geode_performance_capture,
        &GeodeGroupExpectation {
            id_prefix: "closeout-geode",
            scene: &geode_scene,
            performance_scene: &geode_performance_scene,
            commit: &closeout.commit,
            composition_report: &closeout.geode_composition_report,
            performance_report: &closeout.geode_performance_report,
            composition_kind: "closeout-geode-composition",
            performance_kind: "closeout-geode-performance",
        },
    )?;

    // Motion evidence: still frames cannot judge shimmer, chunk walls, or
    // exposure pumping, so the closeout walks both scenes and records it.
    let motion_path = safe_relative(&closeout.motion_report, "toml")?;
    if !closeout
        .motion_report
        .starts_with("screenshots/visual-polish/")
        || !closeout.motion_report.ends_with(".report.toml")
    {
        return Err("closeout motion report path has the wrong role".into());
    }
    let (_motion_bytes, motion): (Vec<u8>, toml::Value) =
        read_toml(&root.join(motion_path), "closeout motion")?;
    if motion
        .get("qualification_schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
        || motion.get("kind").and_then(toml::Value::as_str) != Some("closeout-motion")
        || motion.get("evidence_commit").and_then(toml::Value::as_str)
            != Some(closeout.commit.as_str())
        || motion
            .get("reviewed_by")
            .and_then(toml::Value::as_str)
            .is_none_or(str::is_empty)
        || motion
            .get("review_method")
            .and_then(toml::Value::as_str)
            .is_none_or(str::is_empty)
        || motion.get("passed").and_then(toml::Value::as_bool) != Some(true)
    {
        return Err("closeout motion identity is incomplete or failed".into());
    }
    let Some(walks) = motion.get("walk").and_then(toml::Value::as_array) else {
        return Err("closeout motion report has no walks".into());
    };
    let mut walk_ids = BTreeSet::new();
    for walk in walks {
        let id = walk.get("id").and_then(toml::Value::as_str).unwrap_or("");
        let world = walk
            .get("world")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        let frame_count = walk
            .get("frame_count")
            .and_then(toml::Value::as_integer)
            .unwrap_or(0);
        let hashes = walk
            .get("frame_sha256")
            .and_then(toml::Value::as_array)
            .map(|values| values.as_slice())
            .unwrap_or_default();
        let max_delta = walk
            .get("max_static_luminance_delta")
            .and_then(toml::Value::as_float)
            .unwrap_or(f64::INFINITY);
        let expected_world = match id {
            "strata-site" => "closeout-motion-strata",
            "geode-approach" => "closeout-motion-geode",
            _ => return Err("closeout motion names an unexpected walk".into()),
        };
        if !walk_ids.insert(id.to_string())
            || world != expected_world
            || frame_count < 12
            || hashes.len() != frame_count as usize
            || !hashes
                .iter()
                .all(|value| value.as_str().is_some_and(|hash| valid_hex(hash, 64)))
            || !max_delta.is_finite()
            // Static holds still animate water, torch flame, and fading item
            // labels; exposure pumping is a full-frame swing well beyond 5%.
            || max_delta > 0.050
            || walk
                .get("exposure_pumping_detected")
                .and_then(toml::Value::as_bool)
                != Some(false)
            || walk.get("shimmer_observed").and_then(toml::Value::as_bool) != Some(false)
            || walk
                .get("chunk_wall_observed")
                .and_then(toml::Value::as_bool)
                != Some(false)
            || walk
                .get("awkward_reveal_observed")
                .and_then(toml::Value::as_bool)
                != Some(false)
        {
            return Err(format!("closeout motion walk {id} is incomplete or failed"));
        }
    }
    if walk_ids.len() != 2 {
        return Err("closeout motion requires the strata and geode walks".into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_visual_polish_manifest(root: &Path) -> Result<(), String> {
    let evidence = campaign::evidence_root(root)?;
    validate_visual_polish_manifest_at(
        &evidence,
        &evidence.join("screenshots/visual-polish.toml"),
        root,
    )
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
    fn visual_polish_manifest_is_complete() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(source_root).unwrap();
    }

    #[test]
    fn cracked_geode_capture_manifest_is_complete() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(source_root).unwrap();
    }

    #[test]
    fn cracked_geode_capture_contains_host_shell_lining_and_heart() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(source_root).unwrap();
        let root = campaign::evidence_root(source_root).unwrap();
        let (_bytes, report): (Vec<u8>, GeodeCompositionReport) = read_toml(
            &root.join("screenshots/visual-polish/geode-composition.report.toml"),
            "cracked-geode composition",
        )
        .unwrap();
        assert!(report.passed);
        assert!(report.host_fraction >= report.minimum_host_fraction);
        assert!(report.quartz_pixels >= report.minimum_lining_pixels);
        assert!(report.amethyst_pixels >= report.minimum_lining_pixels);
        assert!(report.heart_dark_deep_pixels >= report.minimum_heart_dark_deep_pixels);
        assert!(report.lip_sides.len() >= report.minimum_lip_sides);
    }

    #[test]
    fn strata_capture_metrics_meet_readability_budget() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(source_root).unwrap();
        let root = campaign::evidence_root(source_root).unwrap();
        for path in [
            "screenshots/visual-polish/strata-readability.report.toml",
            "screenshots/visual-polish/strata-performance.report.toml",
        ] {
            let (_bytes, report): (Vec<u8>, QualificationReport) =
                read_toml(&root.join(path), "strata qualification").unwrap();
            assert!(report.passed, "{path} records a failed acceptance gate");
        }
    }

    #[test]
    fn visual_polish_closeout_is_qualified() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        validate_visual_polish_manifest(source_root).unwrap();
        let root = campaign::evidence_root(source_root).unwrap();
        let (_bytes, manifest): (Vec<u8>, VisualManifest) = read_toml(
            &root.join("screenshots/visual-polish.toml"),
            "visual-polish manifest",
        )
        .unwrap();
        for path in [
            &manifest.closeout.readability_report,
            &manifest.closeout.strata_performance_report,
        ] {
            let (_bytes, report): (Vec<u8>, QualificationReport) =
                read_toml(&root.join(path), "closeout qualification").unwrap();
            assert!(report.passed, "{path} records a failed closeout gate");
        }
        let (_bytes, motion): (Vec<u8>, toml::Value) = read_toml(
            &root.join(&manifest.closeout.motion_report),
            "closeout motion",
        )
        .unwrap();
        assert_eq!(
            motion.get("passed").and_then(toml::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn visual_polish_validator_rejects_incomplete_and_stale_manifests() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = campaign::evidence_root(source_root).unwrap();
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
        assert!(validate_visual_polish_manifest_at(&root, &incomplete, source_root).is_err());

        let stale = scratch.join("stale.toml");
        let manifest: VisualManifest = toml::from_str(&original).unwrap();
        fs::write(
            &stale,
            original.replacen(&manifest.qualification_source_sha256, &"0".repeat(64), 1),
        )
        .unwrap();
        assert!(validate_visual_polish_manifest_at(&root, &stale, source_root).is_err());
        let _ = fs::remove_dir_all(&scratch);
    }
}
