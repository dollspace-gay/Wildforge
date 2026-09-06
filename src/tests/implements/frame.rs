//! Frame scenarios.

use super::*;

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
