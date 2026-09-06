//! Identity scenarios.

use super::*;

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
