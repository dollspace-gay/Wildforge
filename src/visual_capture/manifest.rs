//! Manifest capture qualification contracts.

use super::*;

#[cfg(test)]
pub(super) fn validate_visual_polish_manifest_at(
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
    let native = native::NativeIdentity::from_capture(first)?;
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
            &EvidenceExpectation {
                scene: &format!("strata-production-{}", manifest.date.replace('-', "")),
                commit: expected_commit,
                size: (1280, 720),
                native: &native,
            },
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
            &EvidenceExpectation {
                scene: &format!("strata-performance-{}", manifest.date.replace('-', "")),
                commit: expected_commit,
                size: (1280, 720),
                native: &native,
            },
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
    validate_cracked_geode_manifest(root, &manifest, source_root, &native)?;
    validate_closeout_manifest(root, &manifest, source_root, &native)?;
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
