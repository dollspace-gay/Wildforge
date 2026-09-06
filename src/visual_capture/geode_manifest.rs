//! Geode manifest capture qualification contracts.

use super::*;

#[cfg(test)]
pub(super) fn validate_cracked_geode_manifest(
    root: &Path,
    manifest: &VisualManifest,
    source_root: &Path,
    native: &native::NativeIdentity,
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
        || site.get("atlas_content_hash").and_then(toml::Value::as_str)
            != Some(native.content_hash.as_str())
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
            native,
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
