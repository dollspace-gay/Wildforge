//! Qualification scenarios.

#[test]
fn planetary_visual_capture_manifest_is_complete() {
    use std::collections::HashSet;
    use std::path::{Component, Path};

    let assert_relative_png = |relative: &str| {
        let path = Path::new(relative);
        assert!(
            !path.is_absolute(),
            "PNG declaration is relative: {relative}"
        );
        assert_eq!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("png"),
            "PNG declaration has a .png suffix: {relative}"
        );
        assert!(
            path.components()
                .all(|component| matches!(component, Component::Normal(_))),
            "PNG declaration cannot escape the evidence directory: {relative}"
        );
    };

    let screenshots = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    let manifest_path = screenshots.join("planetary-qualification.toml");
    let text = std::fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
        panic!(
            "read visual qualification manifest {}: {error}",
            manifest_path.display()
        )
    });
    let manifest: toml::Value = toml::from_str(&text).expect("parse visual manifest");
    assert_eq!(
        manifest["qualification_version"].as_integer(),
        Some(1),
        "visual manifest contract version"
    );

    let atlas = manifest["atlas"].as_table().expect("atlas evidence table");
    for field in [
        "id",
        "directory",
        "preview",
        "location",
        "season",
        "time",
        "settings",
        "purpose",
    ] {
        assert!(
            atlas
                .get(field)
                .and_then(toml::Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "atlas evidence has non-empty {field}"
        );
    }
    assert!(atlas["seed"].as_integer().is_some());
    assert!(atlas["generator_version"].as_integer().is_some());
    let atlas_root = screenshots.join(atlas["directory"].as_str().unwrap());
    assert_relative_png(atlas["preview"].as_str().unwrap());
    // The persisted manifest intentionally contains the full u64 checksum
    // domain, while generic `toml::Value` is limited to TOML's signed integer
    // domain. The production atlas loader validates those checksums; this
    // evidence test only needs the small format/completion fields.
    let atlas_manifest = std::fs::read_to_string(atlas_root.join("manifest.toml"))
        .expect("read exported atlas manifest");
    let expected_format = atlas["atlas_format_version"].as_integer().unwrap();
    assert!(
        atlas_manifest
            .lines()
            .any(|line| line.trim() == format!("format_version = {expected_format}")),
        "exported atlas format matches capture metadata"
    );
    assert!(
        atlas_manifest
            .lines()
            .any(|line| line.trim() == "complete = true"),
        "exported atlas manifest is complete"
    );
    let report: toml::Value = toml::from_str(
        &std::fs::read_to_string(atlas_root.join("validation-report.toml"))
            .expect("read atlas validation report"),
    )
    .expect("parse atlas validation report");
    assert_eq!(report["validation"].as_str(), Some("passed"));
    let registered = report["registered_layers"]
        .as_array()
        .expect("registered atlas layers");
    let exported = report["exported_maps"]
        .as_array()
        .expect("exported atlas maps");
    assert_eq!(registered.len(), 132, "current qualification layer count");
    assert_eq!(exported.len(), registered.len());
    for map in exported {
        let relative = map.as_str().expect("map path is text");
        assert_relative_png(relative);
        let path = atlas_root.join(relative);
        assert_eq!(path.parent(), Some(atlas_root.join("maps").as_path()));
        let legend = path.with_extension("legend.txt");
        assert!(
            legend.is_file(),
            "exported atlas legend exists: {}",
            legend.display()
        );
    }

    let captures = manifest["capture"].as_array().expect("visual capture list");
    assert!(
        captures.len() >= 6,
        "qualification includes representative live captures"
    );
    let mut ids = HashSet::new();
    let mut files = HashSet::new();
    for capture in captures {
        let capture = capture.as_table().expect("capture table");
        for field in [
            "id", "file", "location", "season", "time", "settings", "purpose",
        ] {
            assert!(
                capture
                    .get(field)
                    .and_then(toml::Value::as_str)
                    .is_some_and(|value| !value.is_empty()),
                "capture has non-empty {field}"
            );
        }
        assert!(capture["seed"].as_integer().is_some());
        assert!(capture["generator_version"].as_integer().is_some());
        assert!(ids.insert(capture["id"].as_str().unwrap()));
        let file = capture["file"].as_str().unwrap();
        assert!(files.insert(file), "capture files are unique");
        assert_relative_png(file);
        assert_eq!(Path::new(file).components().count(), 1);
    }
}

/// The shaders are only compiled by naga at device-init time, so a typo in the
/// WGSL would ship undetected by `cargo build`/`test`. Parse and validate both
/// shader files here to fail loudly at CI instead of on someone's screen.
#[test]
fn wgsl_shaders_validate() {
    for (name, src) in [
        ("shader.wgsl", crate::shader::WORLD),
        ("post.wgsl", include_str!("../../post.wgsl")),
    ] {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: WGSL parse error:\n{}", e.emit_to_string(src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: WGSL validation failed: {e:?}"));
    }
}
