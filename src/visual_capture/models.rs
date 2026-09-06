//! Models capture qualification contracts.

use serde::Deserialize;

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualManifest {
    pub(super) schema_version: u32,
    pub(super) date: String,
    pub(super) status: String,
    pub(super) evidence_commit: String,
    pub(super) baseline_commit: String,
    pub(super) after_commit: String,
    pub(super) scene_id: String,
    pub(super) qualification_source_sha256: String,
    pub(super) conversion: String,
    pub(super) conversion_tool: String,
    pub(super) conversion_tool_sha256: String,
    pub(super) comparison: String,
    pub(super) readability_report: String,
    pub(super) performance_report: String,
    pub(super) geode_evidence_commit: String,
    pub(super) geode_qualification_source_sha256: String,
    pub(super) geode_verifier: String,
    pub(super) geode_verifier_sha256: String,
    pub(super) geode_site_record: String,
    pub(super) geode_preparation_report: String,
    pub(super) geode_composition_report: String,
    pub(super) geode_performance_report: String,
    pub(super) capture: Vec<ManifestCapture>,
    pub(super) site: Vec<ManifestSite>,
    pub(super) case: Vec<ManifestCase>,
    pub(super) performance: Vec<ManifestPerformance>,
    pub(super) geode_capture: Vec<ManifestGeodeCapture>,
    pub(super) geode_performance_capture: Vec<ManifestPerformance>,
    pub(super) closeout: ManifestCloseout,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestCloseout {
    pub(super) commit: String,
    pub(super) date: String,
    pub(super) verifier: String,
    pub(super) verifier_sha256: String,
    pub(super) readability_report: String,
    pub(super) strata_performance_report: String,
    pub(super) geode_composition_report: String,
    pub(super) geode_performance_report: String,
    pub(super) motion_report: String,
    pub(super) case: Vec<ManifestCase>,
    pub(super) performance: Vec<ManifestPerformance>,
    pub(super) geode_capture: Vec<ManifestGeodeCapture>,
    pub(super) geode_performance_capture: Vec<ManifestPerformance>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestCapture {
    pub(super) id: String,
    pub(super) sidecar: String,
    pub(super) report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestSite {
    pub(super) rock: String,
    pub(super) face: String,
    pub(super) u: i32,
    pub(super) y: i32,
    pub(super) v: i32,
    pub(super) exposure: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestCase {
    pub(super) id: String,
    pub(super) phase: String,
    pub(super) rock: String,
    pub(super) role: String,
    pub(super) light: String,
    pub(super) weather: String,
    pub(super) pack: String,
    pub(super) view_distance_chunks: i32,
    pub(super) sidecar: String,
    pub(super) report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestPerformance {
    pub(super) id: String,
    pub(super) phase: String,
    pub(super) sidecar: String,
    pub(super) report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestGeodeCapture {
    pub(super) id: String,
    pub(super) purpose: String,
    pub(super) sidecar: String,
    pub(super) report: String,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualReport {
    pub(super) report_schema_version: u32,
    pub(super) capture_schema_version: u32,
    pub(super) capture_id: String,
    pub(super) scene_id: String,
    pub(super) identity_sha256: String,
    pub(super) sidecar: String,
    pub(super) sidecar_sha256: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color_ppm_sha256: String,
    pub(super) diagnostic_sha256: String,
    pub(super) color_png: String,
    pub(super) color_png_sha256: String,
    pub(super) diagnostic_png: String,
    pub(super) diagnostic_png_sha256: String,
    pub(super) conversion: String,
    pub(super) conversion_tool: String,
    pub(super) conversion_tool_sha256: String,
    pub(super) python: String,
    pub(super) zlib: String,
    pub(super) pixel_count: u64,
    pub(super) sky_pixels: u64,
    pub(super) overlay_pixels: u64,
    pub(super) material_pixels: u64,
    pub(super) sky_fraction: f64,
    pub(super) overlay_fraction: f64,
    pub(super) material_fraction: f64,
    pub(super) luminance_mean: f64,
    pub(super) luminance_stddev: f64,
    pub(super) local_contrast_rms: f64,
    pub(super) family: Vec<ReportFamily>,
    #[serde(default)]
    pub(super) stratum: Vec<ReportStratum>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportFamily {
    pub(super) diagnostic_id: u16,
    pub(super) atlas_slot: u16,
    pub(super) names: Vec<String>,
    pub(super) pixels: u64,
    pub(super) pixel_fraction: f64,
    pub(super) mean_normalized_depth: f64,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportStratum {
    pub(super) rock: String,
    pub(super) distance_band: String,
    pub(super) pixels: u64,
    pub(super) coverage: f64,
    pub(super) connected_components: u64,
    pub(super) one_pixel_fringe: u64,
    pub(super) one_pixel_fringe_fraction: f64,
    pub(super) median_luminance: f64,
    pub(super) p10_luminance: f64,
    pub(super) p90_luminance: f64,
    pub(super) luminance_span: f64,
    pub(super) rms_contrast_1px: f64,
    pub(super) rms_contrast_4px: f64,
    pub(super) rms_contrast_16px: f64,
    pub(super) contrast_pairs_1px: u64,
    pub(super) contrast_pairs_4px: u64,
    pub(super) contrast_pairs_16px: u64,
    pub(super) adjacent_sky_pairs: u64,
    pub(super) silhouette_weber: f64,
    pub(super) silhouette_weber_magnitude: f64,
    pub(super) median_chroma: f64,
    pub(super) greyscale_structure_score: f64,
    pub(super) expected_fog_blend: f64,
    pub(super) display_black_fraction: f64,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
pub(super) struct QualificationReport {
    pub(super) qualification_schema_version: u32,
    pub(super) kind: String,
    pub(super) baseline_commit: String,
    pub(super) after_commit: String,
    pub(super) passed: bool,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GeodeCompositionReport {
    pub(super) qualification_schema_version: u32,
    pub(super) kind: String,
    pub(super) evidence_commit: String,
    pub(super) hero_capture_id: String,
    pub(super) hero_report: String,
    pub(super) hero_report_sha256: String,
    pub(super) proof_capture_id: String,
    pub(super) proof_report: String,
    pub(super) proof_report_sha256: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) host_pixels: u64,
    pub(super) host_fraction: f64,
    pub(super) minimum_host_fraction: f64,
    pub(super) quartz_pixels: u64,
    pub(super) amethyst_pixels: u64,
    pub(super) minimum_lining_pixels: u64,
    pub(super) heart_dark_deep_pixels: u64,
    pub(super) minimum_heart_dark_deep_pixels: u64,
    pub(super) lip_left_pixels: u64,
    pub(super) lip_right_pixels: u64,
    pub(super) lip_top_pixels: u64,
    pub(super) lip_bottom_pixels: u64,
    pub(super) lip_sides: Vec<String>,
    pub(super) minimum_lip_sides: usize,
    pub(super) sky_pixels: u64,
    pub(super) overlay_fraction: f64,
    pub(super) maximum_overlay_fraction: f64,
    pub(super) proof_host_fraction: f64,
    pub(super) proof_quartz_pixels: u64,
    pub(super) proof_amethyst_pixels: u64,
    pub(super) sealed_reload_camera_match: bool,
    pub(super) sealed_reload_environment_match: bool,
    pub(super) sealed_reload_render_match: bool,
    pub(super) passed: bool,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GeodePerformanceReport {
    pub(super) qualification_schema_version: u32,
    pub(super) kind: String,
    pub(super) evidence_commit: String,
    pub(super) sample_count_per_phase: usize,
    pub(super) sealed_draw_ms: Vec<f64>,
    pub(super) opened_draw_ms: Vec<f64>,
    pub(super) sealed_simulation_ms: Vec<f64>,
    pub(super) opened_simulation_ms: Vec<f64>,
    pub(super) sealed_median_draw_ms: f64,
    pub(super) opened_median_draw_ms: f64,
    pub(super) median_draw_regression_ms: f64,
    pub(super) maximum_median_draw_regression_ms: f64,
    pub(super) sealed_median_simulation_ms: f64,
    pub(super) opened_median_simulation_ms: f64,
    pub(super) median_simulation_regression_ms: f64,
    pub(super) maximum_median_simulation_regression_ms: f64,
    pub(super) passed: bool,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualComparison {
    pub(super) comparison_schema_version: u32,
    pub(super) scene_id: String,
    pub(super) identity_sha256: String,
    pub(super) first_report: String,
    pub(super) first_report_sha256: String,
    pub(super) second_report: String,
    pub(super) second_report_sha256: String,
    pub(super) same_identity: bool,
    pub(super) same_dimensions: bool,
    pub(super) segmentation_agreement: f64,
    pub(super) minimum_segmentation_agreement: f64,
    pub(super) family_total_variation: f64,
    pub(super) maximum_family_total_variation: f64,
    pub(super) depth_rmse: f64,
    pub(super) maximum_depth_rmse: f64,
    pub(super) luminance_mean_delta: f64,
    pub(super) maximum_luminance_mean_delta: f64,
    pub(super) local_contrast_delta: f64,
    pub(super) maximum_local_contrast_delta: f64,
    pub(super) sky_fraction_delta: f64,
    pub(super) maximum_sky_fraction_delta: f64,
    pub(super) equivalent: bool,
}
