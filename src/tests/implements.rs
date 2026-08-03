//! Physical implement identity, resolution, and lifecycle qualification.

use super::*;

fn summed_materials<'a>(
    reg: &crate::registry::Registry,
    names: impl IntoIterator<Item = &'a str>,
) -> crate::registry::MaterialVector {
    let mut total = crate::registry::MaterialVector::new();
    for name in names {
        let stack = ItemStack::new(reg, it(reg, name), 1);
        for (material, units) in crate::materials::stack_materials(reg, stack) {
            *total.entry(material).or_default() += units;
        }
    }
    total
}

#[test]
fn stable_item_id_cannot_merge_through_inventory_paths() {
    let reg = base_reg();
    let item = reg.item_id("base:stick").unwrap();
    let first = ItemStack {
        arcane_id: 41,
        ..ItemStack::new(&reg, item, 1)
    };
    let second = ItemStack {
        arcane_id: 42,
        ..ItemStack::new(&reg, item, 1)
    };
    let mut inventory = Inventory::new();
    assert_eq!(inventory.add_stack(&reg, first), 0);
    assert_eq!(inventory.add_stack(&reg, second), 0);
    assert_eq!(inventory.slots[0], Some(first));
    assert_eq!(inventory.slots[1], Some(second));

    let (slot, cursor) = click_stack(&reg, Some(first), Some(second), false);
    assert_eq!(slot, Some(second));
    assert_eq!(cursor, Some(first));
}

#[test]
fn ordinary_identity_free_items_still_merge() {
    let reg = base_reg();
    let item = reg.item_id("base:stick").unwrap();
    let mut inventory = Inventory::new();
    assert_eq!(inventory.add_stack(&reg, ItemStack::new(&reg, item, 3)), 0);
    assert_eq!(inventory.add_stack(&reg, ItemStack::new(&reg, item, 2)), 0);
    assert_eq!(inventory.slots[0].unwrap().count, 5);
}

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

fn write_implement_mod(root: &Path, id: &str, items: &str) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        format!("id = \"{id}\"\nworld_api = 2\ndepends = [\"base\"]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("items.toml"), items).unwrap();
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

fn embodied_implements_world(tag: &str) -> World {
    let reg = base_reg();
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_805, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    World::new_with_atlas(8_805, dir, reg, atlas)
}

fn install_frame_fixture(world: &mut World) -> (crate::planet::BlockPos, crate::planet::BlockPos) {
    let frame = bp(8, 100, 8);
    let vessel = bp(7, 100, 8);
    let center = frame.chunk();
    world.insert_empty_chunks_for_test(
        (-1..=1).flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv))),
    );
    for (pos, block) in [
        (frame, b(&world.reg, "base:binding_frame")),
        (bp(9, 100, 8), b(&world.reg, "base:focus_mount")),
        (bp(8, 100, 9), b(&world.reg, "base:arcane_conductor")),
        (bp(8, 100, 7), b(&world.reg, "base:containment_post")),
    ] {
        world.set_block_authored_at(pos, block, "implements integration fixture");
    }
    let vessel_stack = ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1);
    assert!(world.place_item_block_at(vessel, vessel_stack));
    let layout = world.binding_frame_layout(frame);
    assert!(layout.valid, "{:?}", layout.problems);
    (frame, vessel)
}

fn assemble_fixture_wand(world: &mut World, frame: crate::planet::BlockPos) -> (ItemStack, u64) {
    use crate::implements::FrameAction;

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
        "base:seasoned_wand_body",
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
    let assembled = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Assemble,
            Some(revision),
            "test",
        )
        .unwrap();
    let stack = match world.block_entity_at(&frame).unwrap() {
        crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
        _ => panic!("wrong frame entity"),
    };
    (stack, assembled.revision)
}

fn drain_fixture_item_to_spark(world: &mut World, pos: crate::planet::BlockPos, id: u64) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let owner = ArcaneOwner::Item(id);
    let (version, mut current) = {
        let account = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&owner)
            .unwrap();
        (account.version, account.current.clone())
    };
    let usable = crate::implements::usable_charge(current.total());
    if usable == 0 {
        return;
    }
    let moved = current.take_units(usable, std::iter::empty()).unwrap();
    let region = world.planet_atlas().unwrap().atlas_pos(pos.surface());
    let destination = ArcaneOwner::Ambient(region);
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        owner,
        version,
        destination.clone(),
        ledger.version_of(&destination),
        moved,
        ArcaneAuthority::System,
        "implements test fixture drained to structural spark",
    );
    ledger.commit(transaction).unwrap();
}

#[test]
fn embodied_frame_calibrates_assembles_saves_disassembles_and_dismantles_conservatively() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-frame-lifecycle");
    let (frame, vessel_pos) = install_frame_fixture(&mut world);
    let before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let mut inventory = Inventory::new();

    // A fresh planet has no sparse regional Ambient account. Calibration must
    // therefore exercise the durable Geography -> Ambient -> vessel handoff.
    let calibration = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Calibrate,
            None,
            "test",
        )
        .unwrap();
    assert!(calibration.success);
    let vessel_stack = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel block-entity kind"),
    };
    assert_ne!(vessel_stack.arcane_id, 0);
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(vessel_stack.arcane_id)
            .is_some()
    );

    let components = [
        "base:seasoned_wand_body",
        "base:ritual_rod_socket",
        "base:echo_slate",
        "base:bronze_wand_binding",
    ];
    let mut revision = calibration.revision;
    for component in components {
        inventory.slots[0] = Some(ItemStack::new(&world.reg, it(&world.reg, component), 1));
        let result = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap();
        assert!(result.success);
        revision = result.revision;
        assert!(inventory.slots[0].is_none());
    }
    let assembled = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Assemble,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(assembled.success);
    revision = assembled.revision;
    let wand = match world.block_entity_at(&frame).unwrap() {
        crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
        _ => panic!("wrong frame block-entity kind"),
    };
    assert_ne!(wand.arcane_id, 0);
    let instance = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    let assembled_materials = summed_materials(&world.reg, components);
    assert_eq!(instance.tracked_materials().unwrap(), assembled_materials);
    assert!(matches!(
        instance.kind,
        crate::implements::ImplementKind::Wand { .. }
    ));
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(wand.arcane_id)
            .unwrap()
            >= 1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        before
    );

    save_world(&mut world);
    let dir = world.save_dir_for_test();
    drop(world);
    let reg = base_reg();
    let mut loaded = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    loaded.ensure_chunk(frame.chunk());
    let loaded_wand = match loaded.block_entity_at(&frame).unwrap() {
        crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
        _ => panic!("wrong loaded frame block-entity kind"),
    };
    assert_eq!(loaded_wand.arcane_id, wand.arcane_id);
    assert_eq!(
        loaded
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .unwrap()
            .tracked_materials()
            .unwrap(),
        assembled_materials,
        "save/load changed the wand's exact physical bill"
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        before
    );

    // A focus is a physical one-at-a-time mount change. The old focus comes
    // back as an ordinary item, while the stable wand identity and its exact
    // Current account survive the re-resolution.
    let mut focus_inventory = Inventory::new();
    focus_inventory.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:wake_iron"), 1));
    let mounted_focus = loaded
        .operate_binding_frame(
            frame,
            &mut focus_inventory,
            0,
            FrameAction::ExchangeSelected,
            Some(revision),
            "test",
        )
        .unwrap();
    revision = mounted_focus.revision;
    let charge_before_swap = loaded
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(wand.arcane_id)
        .unwrap();
    let swapped = loaded
        .operate_binding_frame(
            frame,
            &mut focus_inventory,
            0,
            FrameAction::SwapFocus,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(swapped.success);
    revision = swapped.revision;
    assert!(
        focus_inventory.slots.iter().flatten().any(|stack| {
            reg.item(stack.item).name == "base:echo_slate" && stack.arcane_id == 0
        })
    );
    let swapped_instance = loaded
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    let crate::implements::ImplementKind::Wand { parts, .. } = &swapped_instance.kind else {
        panic!("focus swap changed the implement kind")
    };
    assert_eq!(parts.focus, "base:wake_iron");
    assert!(swapped_instance.strain >= 20);
    let swapped_materials = summed_materials(
        &reg,
        [
            "base:seasoned_wand_body",
            "base:ritual_rod_socket",
            "base:wake_iron",
            "base:bronze_wand_binding",
        ],
    );
    assert_eq!(
        swapped_instance.tracked_materials().unwrap(),
        swapped_materials
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id),
        Some(charge_before_swap)
    );

    // Matching material plus a real hammer repairs wear/strain. The material
    // is consumed and the hammer wears, while Current remains conserved.
    let mut repair_inventory = Inventory::new();
    repair_inventory.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:stick"), 1));
    repair_inventory.slots[1] = Some(ItemStack::new(&reg, it(&reg, "base:smith_hammer"), 1));
    let hammer_before = repair_inventory.slots[1].unwrap().durability;
    let material_before_repair = loaded.material_ledger.as_ref().unwrap().audit();
    let repaired = loaded
        .operate_binding_frame(
            frame,
            &mut repair_inventory,
            0,
            FrameAction::Repair,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(repaired.success);
    revision = repaired.revision;
    assert!(repair_inventory.slots[0].is_none());
    assert_eq!(
        repair_inventory.slots[1].unwrap().durability,
        hammer_before - 1
    );
    let repaired_instance = loaded
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    assert_eq!(repaired_instance.strain, 0);
    assert_eq!(
        repaired_instance.tracked_materials().unwrap(),
        swapped_materials
    );
    let material_after_repair = loaded.material_ledger.as_ref().unwrap().audit();
    let stick_materials = summed_materials(&reg, ["base:stick"]);
    for (material, units) in stick_materials {
        assert_eq!(
            material_after_repair
                .consumption_loss
                .get(&material)
                .copied()
                .unwrap_or_default(),
            material_before_repair
                .consumption_loss
                .get(&material)
                .copied()
                .unwrap_or_default()
                + units,
            "repair did not place matching matter in an explicit finite sink"
        );
    }
    assert!(material_after_repair.is_balanced());
    assert_eq!(
        crate::materials::MaterialLedger::load(&dir)
            .unwrap()
            .audit(),
        material_after_repair,
        "the arcane-linked material repair delta did not survive an independent reopen"
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        before
    );

    let discharged = loaded
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::SafeDischarge,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(discharged.success);
    revision = discharged.revision;
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(wand.arcane_id),
        Some(crate::implements::STRUCTURAL_SPARK_UNITS)
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        before
    );

    let no_tool = loaded.operate_binding_frame(
        frame,
        &mut Inventory::new(),
        0,
        FrameAction::Disassemble,
        Some(revision),
        "test",
    );
    assert!(
        no_tool.unwrap_err().contains("requires shears"),
        "an explicit client action bypassed the physical tool prerequisite"
    );

    let mut recovered = Inventory::new();
    recovered.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:shears"), 1));
    let disassembled = loaded
        .operate_binding_frame(
            frame,
            &mut recovered,
            0,
            FrameAction::Disassemble,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(disassembled.success);
    assert!(
        loaded
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_none()
    );
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(wand.arcane_id)
            .is_none()
    );
    for component in [
        "base:seasoned_wand_body",
        "base:ritual_rod_socket",
        "base:wake_iron",
        "base:bronze_wand_binding",
    ] {
        assert!(
            recovered
                .slots
                .iter()
                .flatten()
                .any(|stack| reg.item(stack.item).name == component)
        );
    }
    let recovered_materials = recovered
        .slots
        .iter()
        .flatten()
        .filter(|stack| {
            [
                "base:seasoned_wand_body",
                "base:ritual_rod_socket",
                "base:wake_iron",
                "base:bronze_wand_binding",
            ]
            .contains(&reg.item(stack.item).name.as_str())
        })
        .fold(
            crate::registry::MaterialVector::new(),
            |mut total, stack| {
                for (material, units) in crate::materials::stack_materials(&reg, *stack) {
                    *total.entry(material).or_default() += units;
                }
                total
            },
        );
    assert_eq!(recovered_materials, swapped_materials);
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        before
    );

    let vessel_id = vessel_stack.arcane_id;
    let pick = it(&reg, "base:bronze_pickaxe");
    assert!(
        loaded
            .break_block_at(vessel_pos, Some(pick), true, false)
            .is_some()
    );
    assert!(loaded.pending_drops().iter().any(|(at, stack)| {
        *at == vessel_pos && stack.item == vessel_stack.item && stack.arcane_id == vessel_id
    }));
    assert!(
        loaded
            .binding_frame_layout(frame)
            .problems
            .iter()
            .any(|line| line.contains("vessel"))
    );
    let exact_vessel = loaded
        .take_pending_drops()
        .into_iter()
        .find_map(|(_, stack)| (stack.arcane_id == vessel_id).then_some(stack))
        .unwrap();
    assert!(loaded.place_item_block_at(vessel_pos, exact_vessel));
    assert!(loaded.binding_frame_layout(frame).valid);

    // Breaking and replacing the work surface invalidates its entity but not
    // the embodied surroundings, so the rebuilt frame starts empty and valid.
    assert!(
        loaded
            .break_block_at(frame, Some(pick), true, false)
            .is_some()
    );
    loaded.set_blocks_for_test([(8, 100, 8, b(&reg, "base:binding_frame"))]);
    assert!(loaded.binding_frame_layout(frame).valid);
    assert!(loaded.block_entity_at(&frame).is_none());
}

#[test]
fn breaking_a_loaded_frame_routes_charge_and_spills_stable_physical_items() {
    use crate::arcane::{ArcaneOwner, DrossMedium};
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-loaded-frame-break");
    let (frame, vessel_pos) = install_frame_fixture(&mut world);
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let (wand, revision) = assemble_fixture_wand(&mut world, frame);
    let charged = world
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(charged.success);
    let mut spare = Inventory::new();
    spare.slots[0] = Some(ItemStack::new(
        &world.reg,
        it(&world.reg, "base:stormvine_tendril"),
        1,
    ));
    let mounted = world
        .operate_binding_frame(
            frame,
            &mut spare,
            0,
            FrameAction::ExchangeSelected,
            Some(charged.revision),
            "test",
        )
        .unwrap();
    assert!(mounted.success);
    assert!(spare.slots[0].is_none());

    let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let ledger = world.arcane_ledger.as_ref().unwrap();
    let wand_usable_before = ledger
        .item_clean_total(wand.arcane_id)
        .map(crate::implements::usable_charge)
        .unwrap();
    let vessel_usable_before = ledger
        .item_clean_total(vessel.arcane_id)
        .map(crate::implements::usable_charge)
        .unwrap();
    assert!(wand_usable_before > 0);
    let region = world
        .planet_atlas()
        .as_deref()
        .unwrap()
        .atlas_pos(frame.surface());
    let dross_before = [DrossMedium::Air, DrossMedium::Soil]
        .into_iter()
        .map(|medium| {
            ledger
                .account(&ArcaneOwner::Dross { region, medium })
                .map_or(0, |account| account.current.total())
        })
        .sum::<u64>();
    let construction = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap()
        .tracked_materials()
        .unwrap();

    let pick = it(&world.reg, "base:bronze_pickaxe");
    assert!(
        world
            .break_block_at(frame, Some(pick), true, false)
            .is_some()
    );
    assert!(world.block_entity_at(&frame).is_none());
    assert!(world.pending_drops().iter().any(|(at, stack)| {
        *at == frame && stack.arcane_id == wand.arcane_id && stack.item == wand.item
    }));
    assert!(world.pending_drops().iter().any(|(at, stack)| {
        *at == frame
            && world.reg.item(stack.item).name == "base:stormvine_tendril"
            && stack.arcane_id == 0
    }));
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(
        ledger.item_clean_total(wand.arcane_id),
        Some(crate::implements::STRUCTURAL_SPARK_UNITS)
    );
    assert_eq!(ledger.item_dross_total(wand.arcane_id), 0);
    assert!(
        ledger
            .item_clean_total(vessel.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap()
            >= vessel_usable_before
    );
    let dross_after = [DrossMedium::Air, DrossMedium::Soil]
        .into_iter()
        .map(|medium| {
            ledger
                .account(&ArcaneOwner::Dross { region, medium })
                .map_or(0, |account| account.current.total())
        })
        .sum::<u64>();
    assert!(dross_after > dross_before);
    assert_eq!(ledger.audit().unwrap().total, total);
    let instance = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    assert_eq!(instance.tracked_materials().unwrap(), construction);
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .audit
            .iter()
            .any(|event| event.kind == "frame_break" && event.instance_id == wand.arcane_id)
    );
    assert!(
        world
            .material_ledger
            .as_ref()
            .unwrap()
            .audit()
            .is_balanced()
    );
}

#[test]
fn creative_implements_are_explicitly_marked_and_still_use_finite_transactions() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-creative-marking");
    world.mode = "creative".into();
    let (frame, vessel_pos) = install_frame_fixture(&mut world);
    let (wand, revision) = assemble_fixture_wand(&mut world, frame);
    let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let state = world.implements_state.as_ref().unwrap();
    assert!(state.instance(wand.arcane_id).unwrap().creative);
    assert!(state.instance(vessel.arcane_id).unwrap().creative);
    let vessel_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(vessel.arcane_id)
        .unwrap();
    let transfer = world
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "creative test",
        )
        .unwrap();
    assert!(transfer.success);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(vessel.arcane_id)
            .unwrap()
            < vessel_before,
        "creative mode silently refilled an implement outside a finite transaction"
    );
    save_world(&mut world);
    let audit = crate::implements::audit_world(&world.save_dir_for_test()).unwrap();
    assert_eq!(audit.creative_marked, 2);
    assert!(audit.is_qualified(), "{}", audit.render());

    let mut survival = embodied_implements_world("implements-survival-marking");
    let (frame, _) = install_frame_fixture(&mut survival);
    let (wand, _) = assemble_fixture_wand(&mut survival, frame);
    assert!(
        !survival
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .unwrap()
            .creative
    );
}

#[test]
fn all_three_charms_migrate_debit_only_for_benefit_and_go_dormant_exactly() {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction, DrossMedium};

    let mut world = embodied_implements_world("implements-charms");
    let pos = bp(12, 100, 12);
    world.insert_empty_chunks_for_test([pos.chunk()]);
    let initial_total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    for (item_name, effect, medium) in [
        ("base:charm_quiet", "quiet", DrossMedium::Air),
        ("base:charm_bark", "bark", DrossMedium::Soil),
        ("base:charm_hunger", "hunger", DrossMedium::Water),
    ] {
        let mut inventory = Inventory::new();
        let mut armor = [None; 5];
        let mut cursor = None;
        armor[4] = Some(ItemStack::new(&world.reg, it(&world.reg, item_name), 1));
        assert_eq!(
            world.migrate_legacy_player_charms(
                pos,
                &mut inventory,
                &mut armor,
                &mut cursor,
                "CHARM TEST",
            ),
            1
        );
        let mut charm = armor[4].unwrap();
        let id = charm.arcane_id;
        assert_ne!(id, 0);
        assert_eq!(
            world.migrate_legacy_player_charms(
                pos,
                &mut inventory,
                &mut armor,
                &mut cursor,
                "CHARM TEST",
            ),
            0,
            "migration was not idempotent"
        );
        assert_eq!(armor[4].unwrap().arcane_id, id);
        let definition = world
            .reg
            .item(charm.item)
            .charm_def
            .as_ref()
            .unwrap()
            .clone();
        assert_eq!(definition.effect.id(), effect);
        assert_eq!(world.reg.item(charm.item).max_stack, 1);

        let clean_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(id)
            .unwrap();
        let region = world.planet_atlas().unwrap().atlas_pos(pos.surface());
        let dross_owner = ArcaneOwner::Dross { region, medium };
        let dross_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&dross_owner)
            .map_or(0, |account| account.current.total());
        assert!(!world.debit_charm_at(pos, &mut charm, "wrong-effect", "must not benefit"));
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().item_clean_total(id),
            Some(clean_before)
        );
        assert!(world.debit_charm_at(pos, &mut charm, effect, "qualified benefit"));
        let clean_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(id)
            .unwrap();
        assert_eq!(clean_before - clean_after, definition.charge_per_trigger);
        let expected_dross = definition
            .charge_per_trigger
            .saturating_mul(u64::from(definition.dross_per_transfer))
            .div_ceil(1_000)
            .min(definition.charge_per_trigger);
        let dross_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&dross_owner)
            .map_or(0, |account| account.current.total());
        assert_eq!(dross_after - dross_before, expected_dross);
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            initial_total
        );

        // Leave only the structural spark, then prove that both the public
        // readiness check and the authoritative debit refuse hidden benefit.
        let owner = ArcaneOwner::Item(id);
        let (version, current) = {
            let account = world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .account(&owner)
                .unwrap();
            (account.version, account.current.clone())
        };
        let usable = crate::implements::usable_charge(current.total());
        if usable != 0 {
            let mut source = current;
            let moved = source.take_units(usable, std::iter::empty()).unwrap();
            let ambient = ArcaneOwner::Ambient(region);
            let ledger = world.arcane_ledger.as_mut().unwrap();
            let tx = ArcaneTransaction::transfer(
                ledger.system_transaction_id().unwrap(),
                owner.clone(),
                version,
                ambient.clone(),
                ledger.version_of(&ambient),
                moved,
                ArcaneAuthority::System,
                "test depletion to structural spark",
            );
            ledger.commit(tx).unwrap();
        }
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().item_clean_total(id),
            Some(1)
        );
        assert!(!world.charm_can_pay(charm, effect));
        let dormant_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .ledger_checksum;
        assert!(!world.debit_charm_at(pos, &mut charm, effect, "depleted must not benefit"));
        assert_eq!(
            world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .audit()
                .unwrap()
                .ledger_checksum,
            dormant_before
        );
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            initial_total
        );
    }

    // The shipped effects retain their old modest caps: one armor point is
    // four percent, quiet is a two-block modifier, and slow hunger is 15%.
    assert_eq!(crate::implements::BARK_CHARM_ARMOR_POINTS, 1);
    assert!(
        (crate::reduced_damage(10.0, 0)
            - crate::reduced_damage(10.0, crate::implements::BARK_CHARM_ARMOR_POINTS,)
            - 0.4)
            .abs()
            < 0.001
    );
    assert_eq!(crate::implements::QUIET_CHARM_AGGRO_REDUCTION, 2.0);
    assert_eq!(crate::implements::HUNGER_CHARM_INTERVAL_SECS, 5.0);
    assert!((crate::implements::HUNGER_CHARM_MULTIPLIER - (1.0 - 0.15)).abs() < f32::EPSILON);
}

#[test]
fn every_charm_binds_from_physical_reagents_and_recharges_from_a_vessel() {
    use crate::implements::FrameAction;

    for (tag, blank, bound, reagent, effect) in [
        (
            "quiet",
            "base:quiet_charm_blank",
            "base:charm_quiet",
            "base:echo_slate",
            "quiet",
        ),
        (
            "bark",
            "base:bark_charm_blank",
            "base:charm_bark",
            "base:hushwood_switch",
            "bark",
        ),
        (
            "hunger",
            "base:hunger_charm_blank",
            "base:charm_hunger",
            "base:ashlace_tissue",
            "hunger",
        ),
    ] {
        let mut world = embodied_implements_world(&format!("implements-bind-{tag}"));
        let (frame, vessel_pos) = install_frame_fixture(&mut world);
        let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
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
        let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
            crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
            _ => panic!("wrong vessel entity"),
        };
        let mut revision = calibrated.revision;

        inventory.slots[0] = Some(ItemStack::new(&world.reg, it(&world.reg, reagent), 1));
        let mounted = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap();
        revision = mounted.revision;
        inventory.slots[0] = Some(ItemStack::new(&world.reg, it(&world.reg, blank), 1));
        let blanked = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap();
        revision = blanked.revision;
        let bound_result = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::BindCharm,
                Some(revision),
                "test",
            )
            .unwrap();
        assert!(bound_result.success);
        revision = bound_result.revision;
        let mut charm = match world.block_entity_at(&frame).unwrap() {
            crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
            _ => panic!("wrong frame entity"),
        };
        assert_eq!(world.reg.item(charm.item).name, bound);
        assert!(world.charm_can_pay(charm, effect));
        assert!(world.debit_charm_at(frame, &mut charm, effect, "recharge qualification benefit"));

        let vessel_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(vessel.arcane_id)
            .unwrap();
        let charm_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(charm.arcane_id)
            .unwrap();
        let retained_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_dross_total(charm.arcane_id);
        let recharge = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::Transfer,
                Some(revision),
                "test",
            )
            .unwrap();
        assert!(recharge.success);
        let vessel_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(vessel.arcane_id)
            .unwrap();
        let charm_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(charm.arcane_id)
            .unwrap();
        let retained_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_dross_total(charm.arcane_id);
        let source_debit = vessel_before - vessel_after;
        assert!(source_debit > 0);
        assert_eq!(
            charm_after - charm_before + retained_after - retained_before,
            source_debit
        );
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            total
        );
    }
}

#[test]
fn damaged_vessels_leak_then_fail_visibly_and_overfill_is_defensively_settled() {
    use crate::implements::{FrameAction, ImplementKind};

    let mut leaking = embodied_implements_world("implements-vessel-leak");
    let (frame, vessel_pos) = install_frame_fixture(&mut leaking);
    let mut inventory = Inventory::new();
    leaking
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Calibrate,
            None,
            "test",
        )
        .unwrap();
    let vessel = match leaking.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    assert_eq!(
        leaking
            .reg
            .block(b(&leaking.reg, "base:charge_vessel"))
            .light_emit,
        0,
        "a dormant vessel still has unconditional block light"
    );
    let observer = frame.entity_center();
    let initial_cue = leaking
        .apparatus_cues_near(observer, 16.0)
        .into_iter()
        .find(|cue| cue.pos == vessel_pos)
        .expect("charged vessel exposes a qualitative presentation cue");
    assert!(initial_cue.charge_band > 0);
    assert_eq!(initial_cue.strain_band, 0);
    let mut guest_mirror = World::new(
        0,
        tmp_dir("implements-vessel-remote-cue"),
        leaking.reg.clone(),
    );
    guest_mirror.set_remote(true);
    guest_mirror.set_remote_apparatus(vec![initial_cue]);
    assert_eq!(
        guest_mirror.apparatus_cues_near(observer, 16.0),
        vec![initial_cue],
        "remote presentation did not match the authoritative charge band"
    );
    let total = leaking
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let vessel_materials = leaking
        .implements_state
        .as_ref()
        .unwrap()
        .instance(vessel.arcane_id)
        .unwrap()
        .tracked_materials()
        .unwrap();
    let clean_before = leaking
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    leaking.set_blocks_for_test([(6, 100, 8, b(&leaking.reg, "base:lava"))]);
    let mut cursor = 0;
    leaking.tick_implements(&mut cursor);
    let clean_after = leaking
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    assert!(
        clean_after < clean_before,
        "damaged finite vessel did not leak"
    );
    assert_eq!(
        leaking
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
    for _ in 0..80 {
        if leaking.get_block_at(vessel_pos) == AIR {
            break;
        }
        leaking.tick_implements(&mut cursor);
        assert_eq!(
            leaking
                .arcane_ledger
                .as_ref()
                .unwrap()
                .audit()
                .unwrap()
                .total,
            total
        );
    }
    assert_eq!(leaking.get_block_at(vessel_pos), AIR);
    let fragments = leaking
        .implements_state
        .as_ref()
        .unwrap()
        .instance(vessel.arcane_id)
        .unwrap();
    assert!(matches!(fragments.kind, ImplementKind::Fragments { .. }));
    assert_eq!(fragments.tracked_materials().unwrap(), vessel_materials);
    assert_eq!(
        leaking
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(vessel.arcane_id),
        Some(crate::implements::STRUCTURAL_SPARK_UNITS)
    );
    assert!(leaking.pending_drops().iter().any(|(_, stack)| {
        leaking.reg.item(stack.item).name == "base:implement_fragment"
            && stack.count == 1
            && stack.arcane_id == vessel.arcane_id
    }));

    // Defensive legacy/mod-capacity shrink: an already charged physical
    // vessel above its now-declared capacity must fail on the next bounded
    // apparatus tick rather than preserving an impossible state.
    let mut overfill = embodied_implements_world("implements-vessel-overfill");
    let (frame, vessel_pos) = install_frame_fixture(&mut overfill);
    let mut inventory = Inventory::new();
    overfill
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Calibrate,
            None,
            "test",
        )
        .unwrap();
    let vessel = match overfill.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let total = overfill
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let record = overfill
        .implements_state
        .as_mut()
        .unwrap()
        .instances
        .get_mut(&vessel.arcane_id)
        .unwrap();
    let ImplementKind::Vessel { capacity, .. } = &mut record.kind else {
        panic!("calibration did not create vessel metadata");
    };
    *capacity = 8;
    let mut cursor = 0;
    overfill.tick_implements(&mut cursor);
    assert_eq!(overfill.get_block_at(vessel_pos), AIR);
    assert!(matches!(
        overfill
            .implements_state
            .as_ref()
            .unwrap()
            .instance(vessel.arcane_id)
            .map(|instance| &instance.kind),
        Some(ImplementKind::Fragments { .. })
    ));
    assert_eq!(
        overfill
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
}

#[test]
fn tuning_lens_reads_the_charge_vessels_real_account_not_the_surrounding_field() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-vessel-instrument-reading");
    let (frame, vessel_pos) = install_frame_fixture(&mut world);
    world
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Calibrate,
            None,
            "instrument test",
        )
        .unwrap();
    let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let mut field_ledger = ItemStack::new(&world.reg, it(&world.reg, "base:field_ledger"), 1);
    world
        .bind_discovery_stack_at(frame, &mut field_ledger)
        .unwrap();
    let observer = (crate::identity::PlayerId([9; 16]), "VESSEL TEST");
    let charged = world
        .record_observation(
            field_ledger.arcane_id,
            observer,
            crate::world::ObservationTarget::Block(vessel_pos),
            crate::discovery::CalibrationGrade::Plate,
            Some("charged vessel".into()),
            None,
        )
        .unwrap();
    assert_eq!(charged.phenomenon_id, "base:charge_vessel");
    assert!(
        !charged.reading.starts_with("still Current"),
        "{}",
        charged.reading
    );

    drain_fixture_item_to_spark(&mut world, frame, vessel.arcane_id);
    let dormant = world
        .record_observation(
            field_ledger.arcane_id,
            observer,
            crate::world::ObservationTarget::Block(vessel_pos),
            crate::discovery::CalibrationGrade::Plate,
            Some("dormant vessel".into()),
            None,
        )
        .unwrap();
    assert!(
        dormant.reading.starts_with("still Current"),
        "instrument followed regional ambience instead of the fitted vessel: {}",
        dormant.reading
    );
    assert_ne!(charged.reading, dormant.reading);
}

#[test]
fn conductor_networks_obey_adjacency_unloaded_boundaries_size_breaks_and_throughput() {
    use crate::implements::{FrameAction, MAX_CONDUCTOR_NETWORK};

    // A conductor endpoint at a loaded chunk edge sees the missing neighbor
    // and pauses the whole frame. Loading that exact boundary restores it;
    // a separated wire still cannot count as an endpoint.
    let mut boundary = embodied_implements_world("implements-conductor-boundary");
    let frame = bp(14, 100, 8);
    boundary.insert_empty_chunks_for_test([frame.chunk()]);
    let focus = frame.offset(0, 0, 1).unwrap();
    let vessel_pos = frame.offset(0, 0, -1).unwrap();
    let conductor = frame.offset(1, 0, 0).unwrap();
    let containment = frame.offset(-1, 0, 0).unwrap();
    boundary.set_block_at(frame, b(&boundary.reg, "base:binding_frame"));
    boundary.set_block_at(focus, b(&boundary.reg, "base:focus_mount"));
    boundary.set_block_at(conductor, b(&boundary.reg, "base:arcane_conductor"));
    boundary.set_block_at(containment, b(&boundary.reg, "base:containment_post"));
    assert!(boundary.place_item_block_at(
        vessel_pos,
        ItemStack::new(&boundary.reg, it(&boundary.reg, "base:charge_vessel"), 1),
    ));
    let layout = boundary.binding_frame_layout(frame);
    assert!(layout.touches_unloaded);
    assert!(!layout.valid);
    let across: Vec<_> = (-1..=1)
        .flat_map(|du| {
            (-1..=1).filter_map(move |dv| conductor.offset(du, 0, dv).map(|at| at.chunk()))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|chunk| !boundary.has_chunk(*chunk))
        .collect();
    boundary.insert_empty_chunks_for_test(across);
    assert!(boundary.binding_frame_layout(frame).valid);
    boundary.set_block_at(conductor, AIR);
    assert!(
        boundary
            .binding_frame_layout(frame)
            .problems
            .iter()
            .any(|problem| problem.contains("endpoint"))
    );

    // A connected component has a hard bounded census. Breaking one segment
    // deterministically disconnects the far tail and restores a bounded local
    // network rather than retaining stale connectivity.
    let mut capped = embodied_implements_world("implements-conductor-cap");
    let (frame, _) = install_frame_fixture(&mut capped);
    let first = frame.offset(0, 0, 1).unwrap();
    let positions: Vec<_> = (0..=MAX_CONDUCTOR_NETWORK)
        .map(|step| first.offset(0, 0, step as i32).unwrap())
        .collect();
    let chunks = positions
        .iter()
        .flat_map(|pos| {
            (-1..=1).flat_map(move |du| {
                (-1..=1).filter_map(move |dv| pos.offset(du, 0, dv).map(|at| at.chunk()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    let chunks: Vec<_> = chunks
        .into_iter()
        .filter(|chunk| !capped.has_chunk(*chunk))
        .collect();
    capped.insert_empty_chunks_for_test(chunks);
    for pos in &positions {
        capped.set_block_at(*pos, b(&capped.reg, "base:arcane_conductor"));
    }
    let overflow = capped.binding_frame_layout(frame);
    assert_eq!(overflow.network_size as usize, MAX_CONDUCTOR_NETWORK + 1);
    assert!(!overflow.valid);
    assert!(
        overflow
            .problems
            .iter()
            .any(|problem| problem.contains("exceeds"))
    );
    let cut = positions[MAX_CONDUCTOR_NETWORK / 2];
    capped.set_block_at(cut, AIR);
    let rebuilt = capped.binding_frame_layout(frame);
    assert!(rebuilt.valid, "{:?}", rebuilt.problems);
    assert_eq!(rebuilt.network_size as usize, MAX_CONDUCTOR_NETWORK / 2);
    let solve_started = std::time::Instant::now();
    for _ in 0..1_000 {
        let measured = capped.binding_frame_layout(frame);
        assert_eq!(measured.network_size as usize, MAX_CONDUCTOR_NETWORK / 2);
    }
    assert!(
        solve_started.elapsed() < std::time::Duration::from_secs(2),
        "1,000 bounded conductor solves took {:?}",
        solve_started.elapsed()
    );

    // One directly attached segment provides exactly one 32-unit route budget
    // before source/target conductivity and dross reduce the useful delivery.
    let mut throughput = embodied_implements_world("implements-conductor-throughput");
    let (frame, vessel_pos) = install_frame_fixture(&mut throughput);
    let (wand, revision) = assemble_fixture_wand(&mut throughput, frame);
    let vessel = match throughput.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };
    let source_before = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    let target_before = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(wand.arcane_id)
        .unwrap();
    let total = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let moved = throughput
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(moved.success);
    let source_after = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel.arcane_id)
        .unwrap();
    let target_after = throughput
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(wand.arcane_id)
        .unwrap();
    assert!((1..=32).contains(&(source_before - source_after)));
    assert!((1..=32).contains(&(target_after - target_before)));
    assert_eq!(
        throughput
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
}

#[test]
fn a_loaded_conductor_extracts_only_from_a_real_local_confluence_or_well() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-place-conductor-source");
    let (site, side) = {
        let geography = world.arcane_geography.as_ref().unwrap();
        let site = geography
            .catalog
            .sites
            .iter()
            .find(|site| {
                site.active
                    && matches!(
                        site.kind,
                        crate::arcane_geography::ArcanePlaceType::Confluence
                            | crate::arcane_geography::ArcanePlaceType::Well
                    )
                    && geography.dynamic.cells[site.center.index(geography.manifest.side)]
                        .ambient_total()
                        > 64
            })
            .cloned()
            .expect("fixture has no charged confluence/well");
        (site, geography.manifest.side)
    };
    let center = site.center.center(side);
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let frame =
        crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let focus = frame.offset(1, 0, 0).unwrap();
    let vessel_pos = frame.offset(-1, 0, 0).unwrap();
    let conductor = frame.offset(0, 0, 1).unwrap();
    let containment = frame.offset(0, 0, -1).unwrap();
    let chunks = [frame, focus, vessel_pos, conductor, containment]
        .into_iter()
        .flat_map(|pos| {
            (-1..=1).flat_map(move |du| {
                (-1..=1).filter_map(move |dv| pos.offset(du, 0, dv).map(|at| at.chunk()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    world.insert_empty_chunks_for_test(chunks);
    world.set_block_at(frame, b(&world.reg, "base:binding_frame"));
    world.set_block_at(focus, b(&world.reg, "base:focus_mount"));
    world.set_block_at(conductor, b(&world.reg, "base:arcane_conductor"));
    world.set_block_at(containment, b(&world.reg, "base:containment_post"));
    assert!(world.place_item_block_at(
        vessel_pos,
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    assert!(world.binding_frame_layout(frame).valid);
    let (wand, revision) = assemble_fixture_wand(&mut world, frame);
    let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
        _ => panic!("wrong vessel entity"),
    };

    // A conductor request has no authority to name or scan arbitrary item
    // owners. Put a separately charged charm in a nearby chest and prove the
    // place route neither debits it nor moves its stable identity, even though
    // it is closer than part of the connected conductor component.
    let chest = frame.offset(2, 0, 0).unwrap();
    world.set_block_authored_at(
        chest,
        b(&world.reg, "base:chest"),
        "unowned conductor authority fixture",
    );
    world.insert_block_entity_at(chest, crate::world::BlockEntity::Chest(Default::default()));
    let mut unowned = Inventory::new();
    let legacy_charm = ItemStack::new(&world.reg, it(&world.reg, "base:charm_quiet"), 1);
    world
        .record_external_stack(legacy_charm, "unowned conductor authority fixture")
        .unwrap();
    unowned.slots[0] = Some(legacy_charm);
    assert_eq!(
        world.migrate_legacy_player_charms(
            frame,
            &mut unowned,
            &mut Default::default(),
            &mut None,
            "a different chest owner",
        ),
        1
    );
    let unowned_charm = unowned.slots[0].take().unwrap();
    let unowned_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(unowned_charm.arcane_id)
        .unwrap();
    match world.block_entity_mut_at(&chest).unwrap() {
        crate::world::BlockEntity::Chest(state) => state.slots[0] = Some(unowned_charm),
        _ => panic!("unowned fixture changed block-entity kind"),
    }

    drain_fixture_item_to_spark(&mut world, frame, wand.arcane_id);
    drain_fixture_item_to_spark(&mut world, frame, vessel.arcane_id);
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let geography_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Geography)
        .unwrap()
        .current
        .total();
    let result = world
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Transfer,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(result.success, "{}", result.message);
    assert!(result.message.contains(site.kind.label()));
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(wand.arcane_id)
            .map(crate::implements::usable_charge)
            .unwrap_or(0)
            > 0
    );
    let geography_after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Geography)
        .unwrap()
        .current
        .total();
    assert!(geography_after < geography_before);
    let chest_charm = match world.block_entity_at(&chest).unwrap() {
        crate::world::BlockEntity::Chest(state) => state.slots[0].unwrap(),
        _ => panic!("unowned fixture changed block-entity kind"),
    };
    assert_eq!(chest_charm, unowned_charm);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(unowned_charm.arcane_id),
        Some(unowned_before),
        "a place conductor extracted from an unowned item"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total
    );
}

#[test]
fn wand_strain_failure_and_fire_lava_despawn_dispositions_are_conservative() {
    use crate::implements::{FrameAction, ImplementCue, ImplementKind, MAX_WAND_STRAIN};

    let mut failed = embodied_implements_world("implements-wand-failure");
    let (frame, _) = install_frame_fixture(&mut failed);
    let (wand, revision) = assemble_fixture_wand(&mut failed, frame);
    let wand_materials = failed
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap()
        .tracked_materials()
        .unwrap();
    let total = failed
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    failed
        .implements_state
        .as_mut()
        .unwrap()
        .instances
        .get_mut(&wand.arcane_id)
        .unwrap()
        .strain = MAX_WAND_STRAIN - 1;
    let result = failed
        .operate_binding_frame(
            frame,
            &mut Inventory::new(),
            0,
            FrameAction::Calibrate,
            Some(revision),
            "test",
        )
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.cue, ImplementCue::Failure);
    assert!(matches!(
        failed.block_entity_at(&frame),
        Some(crate::world::BlockEntity::BindingFrame(state)) if state.output.is_none()
    ));
    let fragment_state = failed
        .implements_state
        .as_ref()
        .unwrap()
        .instance(wand.arcane_id)
        .unwrap();
    assert!(matches!(
        fragment_state.kind,
        ImplementKind::Fragments { .. }
    ));
    assert_eq!(fragment_state.tracked_materials().unwrap(), wand_materials);
    assert_eq!(
        failed
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id),
        Some(crate::implements::STRUCTURAL_SPARK_UNITS)
    );
    assert!(failed.pending_drops().iter().any(|(_, stack)| {
        failed.reg.item(stack.item).name == "base:implement_fragment"
            && stack.count == 1
            && stack.arcane_id == wand.arcane_id
    }));
    assert_eq!(
        failed
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );

    for (tag, reason) in [
        ("despawn", "item entity despawn"),
        ("fire", "item burned in fire"),
        ("lava", "item destroyed by lava"),
    ] {
        let mut world = embodied_implements_world(&format!("implements-wand-{tag}"));
        let (frame, _) = install_frame_fixture(&mut world);
        let (wand, revision) = assemble_fixture_wand(&mut world, frame);
        let tracked = world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .unwrap()
            .tracked_materials()
            .unwrap();
        let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
        let mut inventory = Inventory::new();
        let retrieved = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap();
        assert!(retrieved.success);
        assert_eq!(inventory.slots[0], Some(wand));
        assert!(world.retire_arcane_stack_at(frame, wand, reason));
        let heat = tag == "fire" || tag == "lava";
        let state = world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id);
        assert_eq!(state.is_some(), heat);
        if let Some(state) = state {
            assert!(matches!(state.kind, ImplementKind::Fragments { .. }));
            assert_eq!(state.tracked_materials().unwrap(), tracked);
        }
        assert_eq!(
            world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .item_current_total(wand.arcane_id),
            heat.then_some(crate::implements::STRUCTURAL_SPARK_UNITS)
        );
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            total
        );
        assert_eq!(
            world.pending_drops().iter().any(|(_, stack)| {
                world.reg.item(stack.item).name == "base:implement_fragment"
                    && stack.count == 1
                    && stack.arcane_id == wand.arcane_id
            }),
            heat,
            "fire and lava should leave the same conserved stable fragment bundle"
        );
        assert!(
            world
                .material_ledger
                .as_ref()
                .unwrap()
                .audit()
                .is_balanced()
        );
    }
}

#[test]
fn authored_implement_art_and_remote_or_removed_component_fallbacks_are_legible() {
    use crate::implements::{
        IMPLEMENT_RESOLVER_VERSION, ImplementKind, ImplementPublicState, ResolvedWand, WandParts,
    };

    let reg = base_reg();
    for (finished, ancestor, file) in [
        (
            "base:charm_quiet",
            "base:quiet_charm_blank",
            "crafted_charm_quiet.png",
        ),
        (
            "base:charm_bark",
            "base:bark_charm_blank",
            "crafted_charm_bark.png",
        ),
        (
            "base:charm_hunger",
            "base:hunger_charm_blank",
            "crafted_charm_hunger.png",
        ),
        ("base:bound_wand", "base:stick", "bound_wand.png"),
    ] {
        assert_ne!(
            reg.item(it(&reg, finished)).icon,
            reg.item(it(&reg, ancestor)).icon,
            "{finished} still uses its procedural ancestor art"
        );
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("base/textures")
            .join(file);
        let metadata = std::fs::metadata(path).unwrap();
        assert!(metadata.len() > 64, "authored implement texture is empty");
    }

    // Public implement state is enough to draw a remote held wand. Removed
    // mod components retain their stable ids/stats, while each missing visual
    // role falls back to a base silhouette rather than hiding the wand.
    let mut world = embodied_implements_world("implements-remote-visual");
    let instance_id = 77;
    world.set_remote_arcane_items(vec![(instance_id, 61)]);
    world.set_remote_implements(vec![ImplementPublicState {
        instance_id,
        kind: ImplementKind::Wand {
            parts: WandParts {
                body: "removedmod:body".into(),
                reservoir: "removedmod:reservoir".into(),
                focus: "removedmod:focus".into(),
                binding: "removedmod:binding".into(),
            },
            resolved: ResolvedWand {
                resolver_version: IMPLEMENT_RESOLVER_VERSION,
                capacity: 120,
                safe_transfer: 12,
                stability: 700,
                dross_per_thousand: 30,
                resonance: std::collections::BTreeMap::from([("base:echo".into(), 1)]),
                heat_sensitive: false,
                saturation_instability: 0,
                containment: 200,
            },
        },
        wear: 4,
        strain: 8,
        dross: 2,
    }]);
    let stack = ItemStack {
        arcane_id: instance_id,
        ..ItemStack::new(&reg, it(&reg, "base:bound_wand"), 1)
    };
    let visual = world.implement_visual(stack).unwrap();
    assert_eq!(visual.body, it(&reg, "base:seasoned_wand_body").0);
    assert_eq!(visual.reservoir, it(&reg, "base:ritual_rod_socket").0);
    assert_eq!(visual.focus, it(&reg, "base:echo_slate").0);
    assert_eq!(visual.binding, it(&reg, "base:bronze_wand_binding").0);
    assert_eq!(visual.charge_band, 2);
    assert!(!world.implement_tooltip(stack, true).is_empty());
}

#[test]
fn stable_identity_survives_inventory_cursor_container_cargo_and_save_load() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-transport-identity");
    let (frame, _) = install_frame_fixture(&mut world);
    let (wand, revision) = assemble_fixture_wand(&mut world, frame);
    let exact_current = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(wand.arcane_id)
        .unwrap();

    // Retrieve through the ordinary inventory path, then put the singular
    // identity through the same cursor transaction used by local and remote
    // containers.
    let mut inventory = Inventory::new();
    world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::ExchangeSelected,
            Some(revision),
            "transport test",
        )
        .unwrap();
    assert_eq!(inventory.slots[0], Some(wand));
    let (_, cursor) = click_stack(&world.reg, inventory.slots[0].take(), None, false);
    assert_eq!(cursor, Some(wand));
    let (container_slot, cursor) = click_stack(&world.reg, None, cursor, false);
    assert_eq!(cursor, None);
    assert_eq!(container_slot, Some(wand));

    let chest = frame.offset(3, 0, 0).unwrap();
    world.set_block_at(chest, b(&world.reg, "base:chest"));
    world.insert_block_entity_at(chest, crate::world::BlockEntity::Chest(Default::default()));
    let Some(crate::world::BlockEntity::Chest(state)) = world.block_entity_mut_at(&chest) else {
        panic!("placed chest has no container authority")
    };
    state.slots[0] = container_slot;
    save_world(&mut world);
    let root = world.save_dir_for_test();
    drop(world);

    let mut loaded = World::load_or_create(root.clone(), base_reg()).unwrap();
    loaded.ensure_chunk(chest.chunk());
    let from_chest = match loaded.block_entity_mut_at(&chest).unwrap() {
        crate::world::BlockEntity::Chest(state) => state.slots[0].take().unwrap(),
        _ => panic!("loaded block entity changed kind"),
    };
    assert_eq!(from_chest, wand);
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id),
        Some(exact_current)
    );
    assert!(
        loaded
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_some()
    );

    // Cargo uses a different persistent owner file from block entities.
    // Moving the same object into a real pack animal proves the id rather
    // than a copied charge number crosses that boundary.
    let species = loaded.reg.animal_id("base:deer").unwrap();
    let mut carrier = crate::mobs::Mob::new_at(species, frame.entity_center(), 0.0);
    let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
    cargo[4] = Some(from_chest);
    carrier.cargo = Some(cargo);
    loaded.replace_mobs(vec![carrier]);
    save_world(&mut loaded);
    drop(loaded);

    let reloaded = World::load_or_create(root, base_reg()).unwrap();
    let cargo_wand = reloaded
        .mobs()
        .iter()
        .find_map(|mob| mob.cargo.as_ref().and_then(|cargo| cargo[4]))
        .expect("persistent cargo retained the wand");
    assert_eq!(cargo_wand, wand);
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id),
        Some(exact_current)
    );
    assert!(
        reloaded
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_some()
    );
}

#[test]
fn interrupted_physical_owner_save_rolls_back_ledger_and_implement_sidecar_together() {
    let mut world = embodied_implements_world("implements-owner-crash-recovery");
    let (frame, _) = install_frame_fixture(&mut world);
    let (wand, _) = assemble_fixture_wand(&mut world, frame);
    let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    save_world(&mut world);
    let root = world.save_dir_for_test();
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_some()
    );
    drop(world);

    // Simulate stopping after the linked ledger + implements commit but
    // before entities.toml materialized the finished object.
    std::fs::remove_file(root.join("entities.toml")).unwrap();
    let recovered = World::load_or_create(root, base_reg()).unwrap();
    assert!(
        recovered
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(wand.arcane_id)
            .is_none()
    );
    assert!(
        recovered
            .implements_state
            .as_ref()
            .unwrap()
            .instance(wand.arcane_id)
            .is_none(),
        "ledger rollback left ghost implement construction metadata"
    );
    assert_eq!(
        recovered
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total
    );
    assert!(
        recovered
            .implements_state
            .as_ref()
            .unwrap()
            .audit
            .iter()
            .any(|event| event.kind == "crash_recovery")
    );
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

#[test]
fn stale_concurrent_and_hostile_frame_requests_cannot_duplicate_or_forge_results() {
    use crate::implements::FrameAction;

    let mut world = embodied_implements_world("implements-frame-authority");
    let (frame, _) = install_frame_fixture(&mut world);
    let mut empty = Inventory::new();
    let revision = world
        .operate_binding_frame(frame, &mut empty, 0, FrameAction::Calibrate, None, "host")
        .unwrap()
        .revision;
    let body = ItemStack::new(&world.reg, it(&world.reg, "base:seasoned_wand_body"), 1);
    let mut first = Inventory::new();
    first.slots[0] = Some(body);
    let mut second = Inventory::new();
    second.slots[0] = Some(body);
    let committed = world
        .operate_binding_frame(
            frame,
            &mut first,
            0,
            FrameAction::ExchangeSelected,
            Some(revision),
            "first remote actor",
        )
        .unwrap();
    assert!(committed.success);
    let stale = world
        .operate_binding_frame(
            frame,
            &mut second,
            0,
            FrameAction::ExchangeSelected,
            Some(revision),
            "racing remote actor",
        )
        .unwrap();
    assert!(!stale.success);
    assert_eq!(stale.revision, committed.revision);
    assert_eq!(second.slots[0], Some(body));
    assert!(matches!(
        world.block_entity_at(&frame),
        Some(crate::world::BlockEntity::BindingFrame(state))
            if state.body == Some(body) && state.reservoir.is_none()
                && state.focus.is_none() && state.binding.is_none()
    ));

    let instances_before = world.implements_state.as_ref().unwrap().instances.len();
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    assert!(
        world
            .operate_binding_frame(
                frame,
                &mut second,
                0,
                FrameAction::Assemble,
                Some(committed.revision),
                "hostile remote actor",
            )
            .is_err(),
        "an action-only request forged missing parts or resolved stats"
    );
    assert_eq!(
        world.implements_state.as_ref().unwrap().instances.len(),
        instances_before
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );

    // The client request carries only a location, an existing server slot,
    // an enum verb, and an optimistic revision. There is no wire field for
    // component ids, resolved statistics, charge, dross, or output stacks.
    let request = crate::net::C2S::OperateBindingFrame {
        pos: frame,
        slot: 0,
        action: FrameAction::Assemble,
        expected_revision: Some(committed.revision),
    };
    let wire = postcard::to_allocvec(&request).unwrap();
    assert!(
        wire.len() <= 32,
        "frame request metadata grew to {} bytes",
        wire.len()
    );
    match postcard::from_bytes::<crate::net::C2S>(&wire).unwrap() {
        crate::net::C2S::OperateBindingFrame {
            pos,
            slot,
            action,
            expected_revision,
        } => {
            assert_eq!(pos, frame);
            assert_eq!(slot, 0);
            assert_eq!(action, FrameAction::Assemble);
            assert_eq!(expected_revision, Some(committed.revision));
        }
        other => panic!("frame request decoded as {other:?}"),
    }
}

#[test]
fn implement_save_wire_and_operation_budgets_are_measured_and_enforced() {
    use crate::implements::{
        MAX_IMPLEMENT_FILE_BYTES, MAX_IMPLEMENT_ID_BYTES, MAX_IMPLEMENT_INSTANCE_BYTES,
        MAX_IMPLEMENT_INSTANCES, MAX_IMPLEMENT_PROVENANCE_BYTES, MAX_IMPLEMENT_PUBLIC_BYTES,
    };

    let mut world = embodied_implements_world("implements-budget-audit");
    let (frame, _) = install_frame_fixture(&mut world);
    let (wand, _) = assemble_fixture_wand(&mut world, frame);
    save_world(&mut world);
    let root = world.save_dir_for_test();
    let audit = crate::implements::audit_world(&root).unwrap();
    assert!(audit.is_qualified(), "{}", audit.render());
    assert!(audit.max_instance_bytes <= MAX_IMPLEMENT_INSTANCE_BYTES);
    assert!(audit.max_public_bytes <= MAX_IMPLEMENT_PUBLIC_BYTES);
    assert!(audit.file_bytes <= MAX_IMPLEMENT_FILE_BYTES);

    // Measure a 10,000-instance representative checkpoint using worst-width
    // u64 identities, then conservatively extrapolate the fixed-format base
    // roster to the entire declared census. The hard encoder limit remains
    // authoritative if future content grows per-instance metadata.
    let mut scale = world.implements_state.as_ref().unwrap().clone();
    let template = scale.instance(wand.arcane_id).unwrap().clone();
    scale.instances.clear();
    const SAMPLE: usize = 10_000;
    for index in 0..SAMPLE {
        let mut instance = template.clone();
        instance.instance_id = u64::MAX - index as u64;
        scale.instances.insert(instance.instance_id, instance);
    }
    let encode_started = std::time::Instant::now();
    let bytes = scale.encode().unwrap();
    assert!(
        encode_started.elapsed() < std::time::Duration::from_secs(2),
        "representative implement checkpoint encoded in {:?}",
        encode_started.elapsed()
    );
    let projected = (bytes.len() as u128)
        .saturating_mul(MAX_IMPLEMENT_INSTANCES as u128)
        .div_ceil(SAMPLE as u128);
    assert!(
        projected <= u128::from(MAX_IMPLEMENT_FILE_BYTES),
        "measured full-census projection {projected} exceeds {MAX_IMPLEMENT_FILE_BYTES} bytes"
    );

    let public = crate::implements::ImplementPublicState::from_authority(&template, 17);
    assert!(postcard::to_allocvec(&template).unwrap().len() <= MAX_IMPLEMENT_INSTANCE_BYTES);
    assert!(postcard::to_allocvec(&public).unwrap().len() <= MAX_IMPLEMENT_PUBLIC_BYTES);

    let mut oversized = scale;
    oversized.instances.clear();
    let mut bad_instance = template;
    bad_instance.provenance = "x".repeat(MAX_IMPLEMENT_PROVENANCE_BYTES + 1);
    oversized
        .instances
        .insert(bad_instance.instance_id, bad_instance);
    assert!(oversized.encode().is_err());
    let mut bad_component = world
        .reg
        .item(it(&world.reg, "base:seasoned_wand_body"))
        .wand_component
        .clone()
        .unwrap();
    bad_component.repair_material = "x".repeat(MAX_IMPLEMENT_ID_BYTES + 1);
    assert!(crate::implements::validate_component("fixture:body", &bad_component).is_err());
}
