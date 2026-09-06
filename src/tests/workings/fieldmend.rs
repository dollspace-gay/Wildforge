//! Fieldmend scenarios.

use super::*;

#[test]
fn fieldmend_consumes_matching_matter_leaves_residue_and_cannot_upgrade() {
    let (mut world, source, wand) = workings_world("workings-fieldmend");
    let actor = [25; 16];
    let tool = it(&world.reg, "base:bronze_pickaxe");
    let repair = it(&world.reg, "base:bronze_pickaxe/forge_scrap");
    let residue = it(&world.reg, "base:bronze_pickaxe/primitive_scale");
    let mut inventory = Inventory::new();
    let mut damaged = ItemStack::new(&world.reg, tool, 1);
    damaged.durability -= 20;
    inventory.slots[1] = Some(damaged);
    inventory.slots[2] = Some(ItemStack::new(&world.reg, repair, 1));
    let material_before = world.material_ledger.as_ref().unwrap().audit();
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_fieldmend_working(
            actor,
            "fieldmend-worker",
            source,
            wand.arcane_id,
            &inventory,
            1,
            2,
            16,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let landed = world
        .complete_inventory_working(started.stable_id, &mut inventory)
        .unwrap();
    assert_eq!(
        landed.phase,
        Some(crate::workings::WorkingPhase::PendingApply)
    );
    assert_eq!(inventory.slots[1].unwrap().item, tool);
    assert_eq!(
        inventory.slots[1].unwrap().durability,
        damaged.durability + 16
    );
    assert_eq!(inventory.slots[2].unwrap().item, residue);
    world.finish_inventory_working(started.stable_id).unwrap();
    assert!(world.finish_inventory_working(started.stable_id).is_err());

    let material_after = world.material_ledger.as_ref().unwrap().audit();
    assert!(material_after.is_balanced());
    assert_ne!(
        material_after.consumption_loss, material_before.consumption_loss,
        "inefficient field repair must account for lost matching matter"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let mut wrong = Inventory::new();
    wrong.slots[1] = Some(damaged);
    wrong.slots[2] = Some(ItemStack::new(
        &world.reg,
        it(&world.reg, "base:copper_pickaxe/forge_scrap"),
        1,
    ));
    assert!(
        world
            .begin_fieldmend_working(
                actor,
                "fieldmend-worker",
                source,
                wand.arcane_id,
                &wrong,
                1,
                2,
                16,
                false,
            )
            .unwrap_err()
            .contains("needs one separated stack")
    );
    let mut pristine = Inventory::new();
    pristine.slots[1] = Some(ItemStack::new(&world.reg, tool, 1));
    pristine.slots[2] = Some(ItemStack::new(&world.reg, repair, 1));
    assert!(
        world
            .begin_fieldmend_working(
                actor,
                "fieldmend-worker",
                source,
                wand.arcane_id,
                &pristine,
                1,
                2,
                16,
                false,
            )
            .is_err()
    );
}
