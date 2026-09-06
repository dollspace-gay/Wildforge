//! Complete campaign freshness and evidence rejection scenarios.

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
