//! Vessels scenarios.

use super::*;

#[test]
fn strained_ground_measurably_increases_real_recharge_waste() {
    use crate::dross::DrossBand;
    use crate::implements::FrameAction;

    let transfer = |tag: &str, band: DrossBand| {
        let mut world = embodied_implements_world(tag);
        let (frame, vessel_pos) = install_frame_fixture(&mut world);
        let (wand, revision) = assemble_fixture_wand(&mut world, frame);
        let vessel = match world.block_entity_at(&vessel_pos).unwrap() {
            crate::world::BlockEntity::ChargeVessel(state) => state.vessel.unwrap(),
            _ => panic!("wrong vessel entity"),
        };
        let region = world.planet_atlas().unwrap().atlas_pos(frame.surface());
        let side = world.planet_atlas().unwrap().side();
        world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .dross_state
            .cells[region.index(side)]
        .band = band;
        let source_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(vessel.arcane_id)
            .unwrap();
        let dross_before = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_dross_total(wand.arcane_id);
        let result = world
            .operate_binding_frame(
                frame,
                &mut Inventory::new(),
                0,
                FrameAction::Transfer,
                Some(revision),
                "environmental stability qualification",
            )
            .unwrap();
        assert!(result.success);
        let source_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_clean_total(vessel.arcane_id)
            .unwrap();
        let dross_after = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_dross_total(wand.arcane_id);
        assert!(
            world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .audit()
                .unwrap()
                .is_balanced()
        );
        (source_before - source_after, dross_after - dross_before)
    };

    let clear = transfer("implements-clear-ground-recharge", DrossBand::Clear);
    let strained = transfer("implements-strained-ground-recharge", DrossBand::Strained);
    assert_eq!(clear.0, strained.0, "the compared transfer pulses differed");
    assert!(
        strained.1 > clear.1,
        "strained ground retained {} dross versus {} on clear ground",
        strained.1,
        clear.1
    );
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
    let mut guest_mirror = ReplicaWorld::new(0, leaking.reg.clone(), 0.0);
    guest_mirror
        .observations_mut()
        .extend_apparatus(vec![initial_cue]);
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
