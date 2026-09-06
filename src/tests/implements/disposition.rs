//! Disposition scenarios.

use super::*;

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
