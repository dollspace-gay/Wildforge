//! Charms scenarios.

use super::*;

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
