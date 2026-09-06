//! Embodied frame calibrates assembles saves disassembles and dismantles conservatively scenarios.

use super::*;

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
