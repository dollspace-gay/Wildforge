//! Resolver scenarios.

use super::*;

#[test]
fn every_base_component_combination_is_deterministic_bounded_and_has_tradeoffs() {
    use crate::implements::{ComponentRole, resolve_wand};

    let reg = base_reg();
    let mut roles = std::collections::BTreeMap::<
        ComponentRole,
        Vec<(String, crate::implements::WandComponentDef)>,
    >::new();
    for item in &reg.items {
        if let Some(component) = &item.wand_component {
            roles
                .entry(component.role)
                .or_default()
                .push((item.name.clone(), component.clone()));
        }
    }
    for role in ComponentRole::ALL {
        assert!(
            roles.get(&role).is_some_and(|parts| !parts.is_empty()),
            "missing {role:?}"
        );
    }

    let mut resolved = Vec::new();
    for body in &roles[&ComponentRole::Body] {
        for reservoir in &roles[&ComponentRole::Reservoir] {
            for focus in &roles[&ComponentRole::Focus] {
                for binding in &roles[&ComponentRole::Binding] {
                    let ordered = [
                        body.clone(),
                        reservoir.clone(),
                        focus.clone(),
                        binding.clone(),
                    ];
                    let shuffled = [
                        binding.clone(),
                        focus.clone(),
                        body.clone(),
                        reservoir.clone(),
                    ];
                    let (parts, first) = resolve_wand(&ordered).unwrap();
                    let (parts_again, second) = resolve_wand(&shuffled).unwrap();
                    assert_eq!(parts, parts_again);
                    assert_eq!(first, second);
                    assert!((1..=crate::implements::MAX_WAND_CAPACITY).contains(&first.capacity));
                    assert!(
                        (crate::implements::MIN_SAFE_TRANSFER
                            ..=crate::implements::MAX_SAFE_TRANSFER)
                            .contains(&first.safe_transfer)
                    );
                    assert!((1..=1_000).contains(&first.stability));
                    assert!((1..=500).contains(&first.dross_per_thousand));
                    assert!(first.containment <= 1_000);
                    assert!(!first.resonance.is_empty());
                    resolved.push((parts, first));
                }
            }
        }
    }
    assert!(
        resolved.len() >= 32,
        "base roster did not produce meaningful choice"
    );
    let max_capacity = resolved.iter().map(|(_, r)| r.capacity).max().unwrap();
    let max_transfer = resolved.iter().map(|(_, r)| r.safe_transfer).max().unwrap();
    let max_stability = resolved.iter().map(|(_, r)| r.stability).max().unwrap();
    let max_containment = resolved.iter().map(|(_, r)| r.containment).max().unwrap();
    let min_dross = resolved
        .iter()
        .map(|(_, r)| r.dross_per_thousand)
        .min()
        .unwrap();
    assert!(
        resolved.iter().all(|(_, r)| {
            !(r.capacity == max_capacity
                && r.safe_transfer == max_transfer
                && r.stability == max_stability
                && r.containment == max_containment
                && r.dross_per_thousand == min_dross)
        }),
        "one base combination is a linear best tier"
    );
    let capacity_choice = resolved.iter().max_by_key(|(_, r)| r.capacity).unwrap();
    assert!(
        capacity_choice.1.safe_transfer < max_transfer
            || capacity_choice.1.stability < max_stability
            || capacity_choice.1.dross_per_thousand > min_dross,
        "maximum capacity carried no meaningful cost"
    );
    let transfer_choice = resolved
        .iter()
        .max_by_key(|(_, r)| r.safe_transfer)
        .unwrap();
    assert!(
        transfer_choice.1.stability < max_stability
            || transfer_choice.1.containment < max_containment
            || transfer_choice.1.dross_per_thousand > min_dross,
        "maximum throughput carried no meaningful cost"
    );
}

#[test]
fn mod_components_and_charms_share_the_resolver_and_reject_raw_effects() {
    let root = tmp_dir("implements-mod-schema");
    write_implement_mod(
        &root,
        "goodwand",
        r#"
[[item]]
id = "reed_body"
name = "Reed Wand Body"
texture = "@stick"
wand_component = { role = "body", capacity = 120, conductivity = 515, stability = 775, resonance = { "base:root" = 2, "base:tide" = 1 }, repair_material = "base:stick", containment = 90 }

[[item]]
id = "patient_charm"
name = "Patient Charm"
texture = "@charm_quiet"
charm = { effect = "quiet", charge_per_trigger = 3, capacity = 144, stability = 820, dross_per_transfer = 19 }
"#,
    );
    let good = registry::load(&root);
    assert!(good.arcane_errors.is_empty(), "{:?}", good.arcane_errors);
    let component = good
        .item(good.item_id("goodwand:reed_body").unwrap())
        .wand_component
        .as_ref()
        .unwrap();
    let base_reservoir = good
        .item(good.item_id("base:wellglass_shard").unwrap())
        .wand_component
        .as_ref()
        .unwrap();
    let base_focus = good
        .item(good.item_id("base:echo_slate").unwrap())
        .wand_component
        .as_ref()
        .unwrap();
    let base_binding = good
        .item(good.item_id("base:bronze_wand_binding").unwrap())
        .wand_component
        .as_ref()
        .unwrap();
    let (_, stats) = crate::implements::resolve_wand(&[
        ("goodwand:reed_body".into(), component.clone()),
        ("base:wellglass_shard".into(), base_reservoir.clone()),
        ("base:echo_slate".into(), base_focus.clone()),
        ("base:bronze_wand_binding".into(), base_binding.clone()),
    ])
    .unwrap();
    assert!((1..=crate::implements::MAX_WAND_CAPACITY).contains(&stats.capacity));

    let bad_root = tmp_dir("implements-bad-mod-schema");
    write_implement_mod(
        &bad_root,
        "badwand",
        r#"
[[item]]
id = "god_charm"
name = "God Charm"
texture = "@charm_quiet"
charm = { effect = "quiet", charge_per_trigger = 1, capacity = 100, stability = 900, dross_per_transfer = 1, raw_armor = 99 }

[[item]]
id = "infinite_body"
name = "Infinite Body"
texture = "@stick"
wand_component = { role = "body", capacity = 999999999, conductivity = 1000, stability = 1000, resonance = { "base:root" = 1 }, repair_material = "base:stick" }
"#,
    );
    let bad = registry::load(&bad_root);
    let info = bad.mods.iter().find(|info| info.id == "badwand").unwrap();
    assert!(
        info.error.is_some(),
        "unknown raw effect field was silently accepted"
    );
    assert!(bad.item_id("badwand:god_charm").is_none());
    assert!(bad.item_id("badwand:infinite_body").is_none());
}

#[test]
fn removed_mod_components_keep_identity_stats_visuals_and_declared_salvage() {
    use crate::implements::{FrameAction, ImplementKind};

    let mods = tmp_dir("implements-removed-mod-content");
    write_implement_mod(
        &mods,
        "willowcraft",
        r#"
[[item]]
id = "willow_body"
name = "Willow Wand Body"
texture = "@stick"
wand_component = { role = "body", capacity = 173, conductivity = 411, stability = 733, resonance = { "base:root" = 3 }, repair_material = "base:stick", containment = 77 }
"#,
    );
    let modded_reg = std::sync::Arc::new(registry::load(&mods));
    assert!(
        modded_reg.arcane_errors.is_empty(),
        "{:?}",
        modded_reg.arcane_errors
    );
    let root = tmp_dir("implements-removed-mod-world");
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_805, 16).unwrap());
    atlas.write_new(&root).unwrap();
    let mut world = World::new_with_atlas(8_805, root.clone(), modded_reg, atlas);
    let (frame, _) = install_frame_fixture(&mut world);
    let mut inventory = Inventory::new();
    let calibrated = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Calibrate,
            None,
            "test",
        )
        .unwrap();
    let mut revision = calibrated.revision;
    for component in [
        "willowcraft:willow_body",
        "base:ritual_rod_socket",
        "base:echo_slate",
        "base:bronze_wand_binding",
    ] {
        inventory.slots[0] = Some(ItemStack::new(&world.reg, it(&world.reg, component), 1));
        revision = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap()
            .revision;
    }
    revision = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Assemble,
            Some(revision),
            "test",
        )
        .unwrap()
        .revision;
    let wand = match world.block_entity_at(&frame).unwrap() {
        crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
        _ => panic!("wrong frame entity"),
    };
    let stored_kind = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap()
        .kind
        .clone();
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .component_manifests
            .contains_key("willowcraft:willow_body")
    );
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    save_world(&mut world);
    drop(world);

    let mut removed = World::load_or_create(root, base_reg()).unwrap();
    removed.ensure_chunk(frame.chunk());
    let preserved = removed
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    let preserved_materials = preserved.tracked_materials().unwrap();
    assert_eq!(preserved.kind, stored_kind);
    let ImplementKind::Wand { parts, .. } = &preserved.kind else {
        panic!("removed provider changed implement kind")
    };
    assert_eq!(parts.body, "willowcraft:willow_body");
    assert_eq!(
        removed.implement_visual(wand).unwrap().body,
        it(&removed.reg, "base:seasoned_wand_body").0
    );

    let mut salvage = Inventory::new();
    salvage.slots[0] = Some(ItemStack::new(
        &removed.reg,
        it(&removed.reg, "base:shears"),
        1,
    ));
    let result = removed
        .operate_binding_frame(
            frame,
            &mut salvage,
            0,
            FrameAction::Disassemble,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(result.success);
    assert!(removed.pending_drops().iter().any(|(_, stack)| {
        removed.reg.item(stack.item).name == "base:implement_fragment" && stack.count == 1
    }));
    let conserved = removed
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    assert!(matches!(conserved.kind, ImplementKind::Fragments { .. }));
    assert!(!conserved.construction.is_empty());
    assert!(
        conserved
            .construction
            .iter()
            .all(|component| component.content_id.starts_with("willowcraft:"))
    );
    assert_ne!(conserved.tracked_materials().unwrap(), preserved_materials);
    assert_eq!(
        removed
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id),
        Some(crate::implements::STRUCTURAL_SPARK_UNITS)
    );
    assert_eq!(
        removed
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
}
