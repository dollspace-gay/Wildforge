//! Evidence capture qualification contracts.

use super::*;

#[cfg(test)]
pub(super) fn valid_hex(value: &str, digits: usize) -> bool {
    value.len() == digits
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
pub(super) fn safe_relative<'a>(value: &'a str, extension: &str) -> Result<&'a Path, String> {
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
pub(super) fn read_toml<T: for<'de> Deserialize<'de>>(
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
pub(super) fn finite_fraction(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
pub(crate) fn qualification_source_sha256(root: &Path) -> Result<String, String> {
    campaign::source_sha256(root)
}

#[cfg(test)]
pub(super) fn validate_strata_metrics(report: &VisualReport, label: &str) -> Result<(), String> {
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
pub(super) struct EvidenceExpectation<'a> {
    pub(super) scene: &'a str,
    pub(super) commit: &'a str,
    pub(super) size: (u32, u32),
    pub(super) native: &'a native::NativeIdentity,
}

#[cfg(test)]
pub(super) fn validate_declared_evidence(
    root: &Path,
    declaration_id: &str,
    sidecar_value: &str,
    report_value: &str,
    expected: &EvidenceExpectation<'_>,
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
        || metadata.scene_id != expected.scene
        || metadata.build.commit != expected.commit
        || metadata.build.dirty
        || metadata.world.seed != 20_260_802
        || metadata.world.generator_version != crate::world::WORLD_GENERATOR_VERSION
        || metadata.world.atlas_format_version != crate::planet_atlas::ATLAS_FORMAT_VERSION
        || metadata.world.atlas_algorithm_version != crate::planet_atlas::ATLAS_ALGORITHM_VERSION
        || metadata.world.atlas_genesis_checksum != "b053756eee79d7e7"
        || (metadata.render.width, metadata.render.height) != expected.size
        || !expected.native.matches(metadata)
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
        || report.scene_id != expected.scene
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
