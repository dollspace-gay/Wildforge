use super::*;

#[test]
fn fresh_survival_magic_route_is_public_solo_and_has_no_unique_gate() {
    let registry = base_reg();
    let required_recipe_outputs = [
        "base:field_ledger",
        "base:survey_folio",
        "base:writing_surface",
        "base:tuning_lens_frame",
        "base:tuning_lens",
        "base:experiment_apparatus",
        "base:lens_assembly_bench",
        "base:seasoned_wand_body",
        "base:small_vessel_reservoir",
        "base:ritual_rod_socket",
        "base:bronze_wand_binding",
        "base:binding_frame",
        "base:focus_mount",
        "base:arcane_conductor",
        "base:charge_vessel",
        "base:containment_post",
        "base:quiet_charm_blank",
        "base:bark_charm_blank",
        "base:hunger_charm_blank",
        "base:glass_bottle",
        "base:glass_jar",
        "base:filter_cloth",
        "base:alchemy_mortar",
        "base:infusion_basin",
        "base:alembic",
        "base:filter_stand",
    ];
    for output_name in required_recipe_outputs {
        let output = registry
            .item_id(output_name)
            .unwrap_or_else(|| panic!("missing survival item {output_name}"));
        let recipes = registry
            .recipes
            .iter()
            .filter(|recipe| recipe.output == output)
            .collect::<Vec<_>>();
        assert!(
            !recipes.is_empty(),
            "{output_name} is obtainable only through loot, admin state, or an undisclosed path"
        );
        assert!(recipes.iter().all(|recipe| {
            recipe
                .pattern
                .iter()
                .flatten()
                .all(|ingredient| match ingredient {
                    crate::registry::Ingredient::One(_) => true,
                    crate::registry::Ingredient::Any(items) => !items.is_empty(),
                })
        }));
    }

    for charm in ["quiet", "bark", "hunger"] {
        let blank = registry.item_id(&format!("base:{charm}_charm_blank"));
        assert!(
            blank.is_some(),
            "{charm} still depends on a ruin-only charm"
        );
    }
    assert_eq!(registry.workings.len(), 12);
    assert_eq!(registry.preparations.len(), 8);
    assert!(registry.workings.values().all(|working| {
        !working.description.trim().is_empty()
            && !working.target.is_empty()
            && working.max_targets != 0
    }));
    assert!(registry.preparations.values().all(|preparation| {
        !preparation.description.trim().is_empty()
            && !preparation.ingredients.is_empty()
            && preparation.doses != 0
            && preparation.solvent_units == u64::from(preparation.doses) * preparation.dose_units
    }));

    let wellglass = &registry.arcane_ecology["base:wellglass_bud"];
    assert_eq!(wellglass.kind, crate::registry::ArcaneEcologyKind::Crystal);
    assert_eq!(
        wellglass.reproduction,
        crate::registry::ReproductionMode::Bud
    );
    for finite in [
        "base:choirstone",
        "base:still_salt",
        "base:wake_iron",
        "base:echo_slate",
    ] {
        assert_eq!(
            registry.arcane_ecology[finite].kind,
            crate::registry::ArcaneEcologyKind::FiniteMineral,
            "{finite} stopped being a finite regional source"
        );
    }
}

#[test]
fn qualification_matrix_covers_every_source_bullet_and_named_catalog_entry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let matrix = std::fs::read_to_string(root.join("docs/magic-qualification-matrix.csv"))
        .expect("generate docs/magic-qualification-matrix.csv before qualification");
    let rows = matrix.lines().count().saturating_sub(1);
    assert!(rows >= 1_200, "only {rows} traceability rows were shipped");
    for document in [
        "docs/magic-sequence.md",
        "docs/magic-foundations-plan.md",
        "docs/magic-geography-plan.md",
        "docs/magic-ecology-plan.md",
        "docs/magic-discovery-plan.md",
        "docs/magic-implements-plan.md",
        "docs/magic-workings-plan.md",
        "docs/magic-alchemy-plan.md",
        "docs/magic-dross-plan.md",
    ] {
        assert!(matrix.contains(document), "matrix omitted {document}");
    }
    assert!(
        matrix.lines().skip(1).all(|row| row.contains(",proven,")),
        "the final matrix still contains a contradicted, missing, or insufficient row"
    );
}

#[test]
fn qualification_documentation_exposes_the_finite_contract_and_operator_surface() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path))
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
    };
    let readme = read("README.md");
    for command in [
        "--magic-qualification",
        "--arcane-audit",
        "--arcane-geography-export",
        "--arcane-atlas",
        "--alchemy-audit",
    ] {
        assert!(readme.contains(command), "README omitted {command}");
    }
    for law in [
        "**Current** is finite",
        "Dross is not Ire",
        "Recipes are public",
        "Magic cannot conjure",
        "Scarring is consequential but recoverable",
    ] {
        assert!(readme.contains(law), "README omitted the player law {law}");
    }

    let mod_guide = read("mods/README.md");
    for schema in ["arcane.toml", "workings.toml", "preparations.toml"] {
        assert!(mod_guide.contains(schema), "mod guide omitted {schema}");
    }
    for contract in [
        "Finite magic content",
        "untouched_host_only",
        "transmute_matter",
        "dross_scar",
    ] {
        assert!(mod_guide.contains(contract), "mod guide omitted {contract}");
    }

    let agent_guide = read("docs/agent-mcp-operations.md");
    assert!(agent_guide.contains("game protocol 40"));
    for tool in [
        "observe_magic",
        "run_magic_experiment",
        "binding_frame",
        "working",
        "alchemy",
    ] {
        assert!(agent_guide.contains(tool), "agent guide omitted {tool}");
    }
    assert!(agent_guide.contains("no tool for a global Current total"));

    let plan = read("docs/magic-qualification-plan.md");
    assert!(plan.contains("implemented and production-qualified (2026-08-04)"));
    let record = read("docs/magic-qualification-implementation.md");
    assert!(record.contains("docs/magic-qualification-report.txt"));
    assert!(root.join("docs/magic-qualification-report.txt").is_file());
}
