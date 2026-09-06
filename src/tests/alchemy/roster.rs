//! Roster scenarios.

use super::*;

#[test]
fn base_apothecary_roster_is_closed_physical_and_volume_balanced() {
    let reg = base_reg();
    assert_eq!(reg.preparations.len(), PreparationHandler::ALL.len());
    let handlers = reg
        .preparations
        .values()
        .map(|definition| definition.handler)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(handlers.len(), PreparationHandler::ALL.len());
    for definition in reg.preparations.values() {
        assert_eq!(
            definition.solvent_units,
            u64::from(definition.doses) * definition.dose_units,
            "{} copied or lost carrier volume",
            definition.id
        );
        assert!(!definition.ingredients.is_empty());
        assert!(!definition.steps.is_empty());
        assert_eq!(definition.steps[0], crate::alchemy::ProcessStep::Grind);
        assert!(
            definition
                .steps
                .contains(&crate::alchemy::ProcessStep::Charge)
        );
        for item in [
            &definition.solvent_item,
            &definition.output_item,
            &definition.empty_vessel,
            &definition.residue_item,
        ] {
            assert!(
                reg.item_id(item).is_some(),
                "{} references missing {item}",
                definition.id
            );
        }
        let output = reg.item(reg.item_id(&definition.output_item).unwrap());
        assert_eq!(output.max_stack, 1);
        assert!(
            output.arcane.is_some(),
            "{} dose has no durable Current shell",
            definition.id
        );
        assert!(definition.dross_units <= definition.charge_units);
    }
}

#[test]
fn closed_world_alchemy_audit_reconciles_all_parent_ledgers() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-closed-audit");
    let mut inventory = Inventory::new();
    let dose_id = mint_ready_dose(&mut world, &mut inventory, 0, "base:hearth_tonic", 17, 3);
    let stack = inventory.slots[0].take().unwrap();
    let mut entity = crate::entity::ItemEntity::new(
        ep(glam::vec3(0.5, 101.0, 0.5)),
        glam::Vec3::ZERO,
        stack.item,
        stack.count,
    );
    entity.durability = stack.durability;
    entity.arcane_id = stack.arcane_id;
    world.spawn_loose_item(entity);
    save_world(&mut world);

    let audit = crate::alchemy::audit_world(&world.save_dir_for_test()).unwrap();
    assert!(audit.is_qualified(), "{}", audit.render());
    assert_eq!(audit.containers, 1);
    assert_eq!(audit.clean_current, 17);
    assert_eq!(audit.dross_current, 3);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(dose_id))
            .is_some()
    );
}
