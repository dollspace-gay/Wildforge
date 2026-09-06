use super::arcane_validation::validate_arcane_graph;
use super::linking::arcane_def;
use super::schema::ArcaneContentToml;
use super::{ArcaneDisposition, Ingredient, MaterialVector, RecipeDef, load};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn base_arcane_content_is_valid_and_single_instance() {
    let registry = load(Path::new("__no_arcane_schema_mods__"));
    assert!(
        registry.arcane_errors.is_empty(),
        "{}",
        registry.arcane_errors.join("\n")
    );
    assert!(
        registry
            .items
            .iter()
            .filter(|item| item.arcane.is_some())
            .all(|item| item.max_stack == 1)
    );
    for name in crate::arcane::BASE_RESONANCES {
        assert!(registry.arcane_registry.definitions.contains_key(name));
    }
    for name in ["base:plant_fiber", "base:living_wood"] {
        assert!(
            registry
                .item(registry.item_id(name).unwrap())
                .arcane
                .is_none(),
            "ordinary renewable material {name} must remain stackable and uncharged"
        );
    }
    for name in [
        "base:thorn_fiber",
        "base:dryad_heartwood",
        "base:lantern_fungus",
    ] {
        let item = registry.item(registry.item_id(name).unwrap());
        assert!(item.arcane.is_some(), "{name} must be magical content");
        assert_eq!(item.max_stack, 1, "{name} must identify one charged owner");
    }
    let fungus = registry.block(registry.block_id("base:lantern_fungus").unwrap());
    assert!(fungus.arcane.is_some());
    assert!(
        registry
            .block(registry.block_id("base:jungle_bush").unwrap())
            .arcane
            .is_none()
    );
}

#[test]
fn base_scars_cover_the_closed_lifecycle_and_removed_content_falls_back() {
    let registry = load(Path::new("__no_dross_scar_mods__"));
    assert!(registry.arcane_errors.is_empty());
    assert_eq!(
        registry
            .dross_scars
            .values()
            .filter(|definition| definition.provider == "base")
            .count(),
        crate::dross::ScarKind::ALL.len()
    );
    for kind in crate::dross::ScarKind::ALL {
        let fallback = registry
            .resolve_dross_scar("removed_provider:old_scar", kind)
            .expect("every climate kind has a safe base fallback");
        assert_eq!(fallback.kind, kind);
        assert_eq!(fallback.provider, "base");
    }
    let kind = crate::dross::ScarKind::WetFilm;
    let first = registry
        .select_dross_scar(
            kind,
            crate::dross::DrossCarrier::Water,
            crate::dross::DrossBand::Seep,
            &BTreeMap::new(),
            7,
        )
        .unwrap();
    let full = BTreeMap::from([(first.content_id.clone(), 1usize)]);
    assert!(
        registry
            .select_dross_scar(
                kind,
                crate::dross::DrossCarrier::Water,
                crate::dross::DrossBand::Seep,
                &full,
                7,
            )
            .is_none(),
        "a definition's regional cap must not be bypassed by fallback selection"
    );
}

#[test]
fn mod_scar_shell_loads_and_unsafe_lifecycles_fail_closed() {
    let root =
        std::env::temp_dir().join(format!("wildforge-dross-scar-mod-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let provider = root.join("safe_scar");
    std::fs::create_dir_all(provider.join("textures")).unwrap();
    std::fs::write(
        provider.join("mod.toml"),
        "id = \"safe_scar\"\nworld_api = 2\n",
    )
    .unwrap();
    std::fs::copy(
        Path::new("base/textures/cattail.png"),
        provider.join("textures/thread.png"),
    )
    .unwrap();
    let safe = r#"
[[block]]
id = "river_threads"
texture = "thread.png"
hardness = 0.2
solid = false
opaque = false
height = 0.08
drops = "base:scar_fragment"
item = false
observation = { categories = ["scar"], properties = ["dross", "resonance", "condition"] }
dross_scar = { kind = "wet_film", handler = "filament_growth", carriers = ["water"], min_band = "seep", status = "recovery_drag", activity = "animated_castoff", max_sites_per_region = 2 }
"#;
    std::fs::write(provider.join("blocks.toml"), safe).unwrap();
    let registry = load(&root);
    assert!(
        registry.arcane_errors.is_empty(),
        "{}",
        registry.arcane_errors.join("\n")
    );
    let definition = registry.dross_scars.get("safe_scar:river_threads").unwrap();
    assert_eq!(definition.max_sites_per_region, 2);
    assert_eq!(
        definition.handler,
        crate::dross::ScarHandler::FilamentGrowth
    );

    let unsafe_provider = root.join("unsafe_scar");
    std::fs::create_dir_all(unsafe_provider.join("textures")).unwrap();
    std::fs::write(
        unsafe_provider.join("mod.toml"),
        "id = \"unsafe_scar\"\nworld_api = 2\n",
    )
    .unwrap();
    std::fs::copy(
        Path::new("base/textures/cattail.png"),
        unsafe_provider.join("textures/thread.png"),
    )
    .unwrap();
    std::fs::write(
        unsafe_provider.join("blocks.toml"),
        safe.replace("id = \"river_threads\"", "id = \"bad_threads\"")
            .replace("solid = false", "solid = true")
            .replace("max_sites_per_region = 2", "max_sites_per_region = 255"),
    )
    .unwrap();
    let rejected = load(&root);
    assert!(rejected.arcane_errors.iter().any(|error| {
        error.contains("unsafe_scar:bad_threads")
            && (error.contains("sites per region") || error.contains("nonstructural"))
    }));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn transformation_graph_rejects_unbacked_charged_outputs() {
    let mut registry = load(Path::new("__no_arcane_output_mods__"));
    assert!(registry.arcane_errors.is_empty());
    registry.recipes.push(RecipeDef {
        w: 1,
        h: 1,
        pattern: vec![Some(Ingredient::One(
            registry.item_id("base:plant_fiber").unwrap(),
        ))],
        output: registry.item_id("base:ember").unwrap(),
        count: 1,
        station: None,
        loss: MaterialVector::new(),
        byproducts: Vec::new(),
        tech: None,
        blueprint: None,
    });
    validate_arcane_graph(&mut registry);
    assert!(
        registry
            .arcane_errors
            .iter()
            .any(|error| error.contains("recipe") && error.contains("base:ember")),
        "{:?}",
        registry.arcane_errors
    );
}

#[test]
fn schema_rejects_unknown_zero_and_overflowing_resonances() {
    let registry = crate::arcane::ResonanceRegistry::base();
    let unknown = ArcaneContentToml {
        capacity: 1,
        conductivity: 1,
        stability: 1,
        resonance: BTreeMap::from([("missing".into(), 1)]),
        on_destroy: ArcaneDisposition::Ambient,
    };
    assert!(
        arcane_def(Some(&unknown), "fixture", "fixture:item", &registry)
            .unwrap_err()
            .contains("unknown resonance")
    );

    let zero = ArcaneContentToml {
        capacity: 1,
        conductivity: 1,
        stability: 1,
        resonance: BTreeMap::from([("base:root".into(), 0)]),
        on_destroy: ArcaneDisposition::Ambient,
    };
    assert!(
        arcane_def(Some(&zero), "fixture", "fixture:item", &registry)
            .unwrap_err()
            .contains("zero weight")
    );

    let too_wide = ArcaneContentToml {
        capacity: 1,
        conductivity: 1_001,
        stability: 1,
        resonance: BTreeMap::from([("base:root".into(), 1)]),
        on_destroy: ArcaneDisposition::Ambient,
    };
    assert!(
        arcane_def(Some(&too_wide), "fixture", "fixture:item", &registry)
            .unwrap_err()
            .contains("0..=1000")
    );
}

#[test]
fn destruction_policy_is_mandatory_and_integer_overflow_is_actionable() {
    let missing = toml::from_str::<ArcaneContentToml>(
        "capacity=1\nconductivity=1\nstability=1\nresonance={root=1}",
    )
    .unwrap_err()
    .to_string();
    assert!(missing.contains("on_destroy"));
    let overflow = toml::from_str::<ArcaneContentToml>(
        "capacity=18446744073709551616\nconductivity=1\nstability=1\nresonance={root=1}\non_destroy='ambient'",
    )
    .unwrap_err()
    .to_string();
    assert!(overflow.contains("number") || overflow.contains("u64"));
}
