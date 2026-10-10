//! Geode group capture qualification contracts.

use super::*;

#[cfg(test)]
pub(super) struct GeodeGroupExpectation<'a> {
    pub(super) native: &'a native::NativeIdentity,
    pub(super) id_prefix: &'a str,
    pub(super) scene: &'a str,
    pub(super) performance_scene: &'a str,
    pub(super) commit: &'a str,
    pub(super) composition_report: &'a str,
    pub(super) performance_report: &'a str,
    pub(super) composition_kind: &'a str,
    pub(super) performance_kind: &'a str,
}

#[cfg(test)]
pub(super) fn validate_geode_capture_group(
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
            &EvidenceExpectation {
                scene: expected.scene,
                commit: expected.commit,
                size: (1920, 1080),
                native: expected.native,
            },
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
    let mut performance_render = sealed.render.clone();
    performance_render.width = 1280;
    performance_render.height = 720;
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
            &EvidenceExpectation {
                scene: expected.performance_scene,
                commit: expected.commit,
                size: (1280, 720),
                native: expected.native,
            },
        )?;
        if metadata.world.name != format!("visual-polish-geode-{}", declaration.phase)
            || metadata.camera != sealed.camera
            || metadata.render != performance_render
            || metadata.environment.day != sealed.environment.day
            || metadata.environment.time_of_day != sealed.environment.time_of_day
            || metadata.environment.season != sealed.environment.season
            || metadata.environment.weather != sealed.environment.weather
            || metadata.environment.precipitation != sealed.environment.precipitation
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
        let (cpu, gpu) = super::render_work::validate_capture(&metadata.telemetry)?;
        let (cpu_key, gpu_key) = if declaration.phase == "sealed" {
            ("sealed_cpu_submission", "sealed_gpu")
        } else {
            ("opened_cpu_submission", "opened_gpu")
        };
        samples.entry(cpu_key).or_default().push(cpu);
        samples.entry(gpu_key).or_default().push(gpu);
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
    super::geode_performance::validate_report(
        &performance,
        expected.performance_kind,
        expected.commit,
        &samples,
    )
}
