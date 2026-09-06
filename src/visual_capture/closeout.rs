//! Closeout capture qualification contracts.

use super::*;

#[cfg(test)]
pub(super) fn validate_closeout_manifest(
    root: &Path,
    manifest: &VisualManifest,
    source_root: &Path,
    native: &native::NativeIdentity,
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
            &EvidenceExpectation {
                scene: &strata_scene,
                commit: &closeout.commit,
                size: (1280, 720),
                native,
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
            &EvidenceExpectation {
                scene: &strata_performance_scene,
                commit: expected_commit,
                size: (1280, 720),
                native,
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
            native,
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
