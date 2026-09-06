//! Preservation scenarios.

use super::*;

#[test]
fn frostlace_never_reverses_age_and_its_current_owner_survives_reopen_reconcile() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-frostlace");
    let item_id = 77;
    let status_id = world
        .alchemy_state
        .as_mut()
        .unwrap()
        .allocate_status_id()
        .unwrap();
    world.alchemy_state.as_mut().unwrap().coatings.insert(
        item_id,
        SpecimenCoating {
            status_id,
            item_id,
            source_batch: 1,
            actor: [5; 16],
            applied_pos: bp(8, 100, 8),
            applied_tick: 0,
            expires_tick: 10_000,
            preservation_permille: 250,
            maximum_temperature_millic: 12_000,
            age_paid: 0,
        },
    );
    assert_eq!(world.coated_specimen_age_advance(item_id, 100, 5_000), 25);
    assert_eq!(world.coated_specimen_age_advance(item_id, 1, 5_000), 1);
    assert_eq!(world.coated_specimen_age_advance(item_id, 100, 13_000), 100);
    assert!(
        world
            .alchemy_state()
            .unwrap()
            .active_arcane_ids()
            .contains(&status_owner_id(status_id))
    );
}

#[test]
fn embodied_frostlace_coats_one_botanical_and_returns_jar_and_spent_carrier() {
    let mut world = crate::tests::implements::embodied_implements_world("alchemy-frostlace-use");
    let actor = [26; 16];
    let at = bp(8, 100, 8);
    let definition = world.reg.preparations["base:frostlace_suspension"].clone();
    let mut inventory = Inventory::new();
    mint_ready_dose(&mut world, &mut inventory, 0, &definition.id, 8, 2);
    let specimen_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .allocate_item_id()
        .unwrap();
    fund_owner_current(
        &mut world,
        ArcaneOwner::Item(specimen_id),
        "base:stone",
        1,
        Some("base:frostlace_frond"),
    );
    let specimen_item = it(&world.reg, "base:frostlace_frond");
    assert!(world.reg.item(specimen_item).arcane_ecology.is_some());
    inventory.slots[1] = Some(ItemStack {
        item: specimen_item,
        count: 1,
        durability: world.reg.item(specimen_item).durability,
        arcane_id: specimen_id,
    });
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let result = world
        .use_preparation(
            actor,
            "frostlace fixture",
            at,
            &mut inventory,
            0,
            crate::alchemy::AlchemyTarget::Item(specimen_id),
        )
        .unwrap();
    assert_eq!(result.returned_vessel.unwrap().item_name, "base:glass_jar");
    assert_eq!(result.byproduct.unwrap().item_name, "base:spent_carrier");
    let coating = world.alchemy_state().unwrap().coatings[&specimen_id].clone();
    assert_eq!(
        world.coated_specimen_age_advance(specimen_id, 100, 5_000),
        25
    );
    assert!(world.coated_specimen_age_advance(specimen_id, 100, 5_000) > 0);
    world.set_simulation_clock(coating.expires_tick.saturating_add(1) as f64 / 20.0);
    for _ in 0..4 {
        world.tick_alchemy(128).unwrap();
    }
    assert!(
        !world
            .alchemy_state()
            .unwrap()
            .coatings
            .contains_key(&specimen_id)
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn chest_storage_uses_the_same_preparation_clock_and_cellars_slow_it() {
    use crate::world::{BlockEntity, ChestState};

    let mut world = crate::tests::implements::embodied_implements_world("alchemy-cellar-storage");
    let open_pos = bp(8, 100, 8);
    let cellar_pos = bp(12, 20, 8);
    let center = open_pos.chunk();
    world.insert_empty_chunks_for_test(
        (-1..=1)
            .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect::<Vec<_>>(),
    );
    let chest_block = b(&world.reg, "base:chest");
    let stone = b(&world.reg, "base:stone");
    world.set_block_authored_at(open_pos, chest_block, "alchemy open-storage fixture");
    if let Some(above) = open_pos.offset(0, 1, 0) {
        world.set_block_authored_at(above, crate::registry::AIR, "open sky fixture");
    }
    for du in -1..=1 {
        for dy in 0..=2 {
            for dv in -1..=1 {
                if (du, dy, dv) == (0, 0, 0) || (du, dy, dv) == (0, 1, 0) {
                    continue;
                }
                world.set_block_authored_at(
                    cellar_pos.offset(du, dy, dv).unwrap(),
                    stone,
                    "alchemy cellar enclosure",
                );
            }
        }
    }
    world.set_block_authored_at(cellar_pos, chest_block, "alchemy cellar-storage fixture");
    world.relight_and_cascade(center);
    assert_eq!(world.light_at_pos(open_pos.offset(0, 1, 0).unwrap()).1, 15);
    assert_eq!(world.light_at_pos(cellar_pos.offset(0, 1, 0).unwrap()).1, 0);

    let mut inventory = Inventory::new();
    let open_id = mint_ready_dose(
        &mut world,
        &mut inventory,
        0,
        "base:clear_eye_tincture",
        1,
        0,
    );
    let cellar_id = mint_ready_dose(
        &mut world,
        &mut inventory,
        1,
        "base:clear_eye_tincture",
        1,
        0,
    );
    let before = world.alchemy_state().unwrap().containers[&open_id].expires_tick;
    assert_eq!(
        world.alchemy_state().unwrap().containers[&cellar_id].expires_tick,
        before
    );
    let mut open_chest = ChestState::default();
    open_chest.slots[0] = inventory.slots[0].take();
    world.insert_block_entity_at(open_pos, BlockEntity::Chest(open_chest));
    let mut cellar_chest = ChestState::default();
    cellar_chest.slots[0] = inventory.slots[1].take();
    world.insert_block_entity_at(cellar_pos, BlockEntity::Chest(cellar_chest));

    world.set_simulation_clock(20.0);
    world.tick_entities(20.0);
    let open = &world.alchemy_state().unwrap().containers[&open_id];
    let cellar = &world.alchemy_state().unwrap().containers[&cellar_id];
    assert_eq!(open.last_storage_tick, 400);
    assert_eq!(cellar.last_storage_tick, 400);
    assert!(
        cellar.expires_tick >= open.expires_tick.saturating_add(300),
        "cellar storage did not quarter the same 400-tick aging interval: open {}, cellar {}",
        open.expires_tick,
        cellar.expires_tick
    );
}
