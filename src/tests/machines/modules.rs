//! Modules scenarios.

use super::*;

#[test]
fn folded_stats_and_a_tier_swap_drive_the_heat_multiplier() {
    use crate::world::multiblock::fold_stats;
    let reg = base_reg();
    let mut w = test_world_with("fold-stats", reg.clone());
    let (mx, my, mz) = (10, 120, 10);
    build_bloomery(&mut w, &reg, mx, my, mz);
    let anchor = bp(mx, my, mz);
    let matched = reg
        .machine_kind("base:bloomery")
        .unwrap_or_default()
        .validate(&w, anchor)
        .expect("a fresh shell folds");
    let stats = fold_stats(&w, &matched.matched);
    assert_eq!(
        stats.heat, 23,
        "seven ring cells a course fold their retention, the mouth takes the eighth"
    );
    assert_eq!(stats.heat_cells, 23);
    assert_eq!(stats.heat_multiplier(), 1.0, "an all-base ring is baseline");

    // Swap one ring cell to the advanced tier: heat re-folds without a
    // mouth special-case, and the shell fires proportionally faster.
    let adv = reg.block_id("base:firebrick_advanced").unwrap();
    w.set_block(mx + 2, my, mz + 1, adv);
    let matched = reg
        .machine_kind("base:bloomery")
        .unwrap_or_default()
        .validate(&w, anchor)
        .expect("advanced firebrick still satisfies the ring tag");
    let stats = fold_stats(&w, &matched.matched);
    assert_eq!(stats.heat, 24, "one advanced cell adds a point of heat");
    assert_eq!(stats.heat_cells, 23);
    assert!(
        (stats.heat_multiplier() - 24.0 / 23.0).abs() < 1e-6,
        "a hotter ring fires faster, no rebuild"
    );
}

#[test]
fn the_edit_hook_revalidates_only_shell_blocks_and_refreshes_stats() {
    use crate::world::{BlockEntity, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("reval-scope", reg.clone());
    let (mx, my, mz) = (20, 130, 20);
    build_bloomery(&mut w, &reg, mx, my, mz);
    w.insert_block_entity(
        (mx, my, mz),
        BlockEntity::Multiblock(MachineInstance {
            kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
            ..Default::default()
        }),
    );
    let base = w.multiblock_revalidations();

    // Far away: no instance in range, the counter is untouched.
    w.set_block(mx + 20, my, mz, reg.block_id("base:firebrick").unwrap());
    assert_eq!(
        w.multiblock_revalidations(),
        base,
        "an outside edit is free"
    );
    // One cell above the shell's three courses: still not our instance.
    w.set_block(mx, my + 3, mz, reg.block_id("base:firebrick").unwrap());
    assert_eq!(
        w.multiblock_revalidations(),
        base,
        "one cell off the shell costs nothing"
    );

    // A real shell edit re-validates exactly our one instance...
    let adv = reg.block_id("base:firebrick_advanced").unwrap();
    w.set_block(mx + 2, my, mz + 1, adv);
    assert_eq!(
        w.multiblock_revalidations(),
        base + 1,
        "a ring edit re-validates the shell"
    );
    // ...and the instance's folded stats follow immediately: no rebuild,
    // no tick, no re-match poll.
    let Some(BlockEntity::Multiblock(m)) = w.block_entity(&(mx, my, mz)) else {
        panic!("instance")
    };
    assert_eq!(m.stats.heat, 24, "the tier swap re-folded the shell");
    assert_eq!(m.stats.heat_cells, 23);
}

#[test]
fn a_slot_module_swaps_in_place_and_refolds_without_disturbing_the_instance() {
    use crate::world::{BlockEntity, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("slot-swap", reg.clone());
    let (mx, my, mz) = (30, 120, 24);
    build_bloomery(&mut w, &reg, mx, my, mz);
    let anchor = bp(mx, my, mz);
    let matched = reg
        .machine_kind("base:bloomery")
        .unwrap_or_default()
        .validate(&w, anchor)
        .expect("a fresh shell validates");
    let (slot, _) = matched
        .slots
        .iter()
        .find(|(_, cat)| **cat == "casing")
        .map(|(pos, cat)| (*pos, *cat))
        .expect("the stack declares its casing slot");
    assert_eq!(matched.slots.len(), 1, "one proof-of-concept slot a frame");

    // A charged, registered instance: the charge must survive a swap.
    let iron = reg.item_id("base:iron_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    w.insert_block_entity(
        (mx, my, mz),
        BlockEntity::Multiblock(MachineInstance {
            kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
            charge: [Some(ItemStack::new(&reg, iron, 2)), None, None, None],
            fuel: [Some(ItemStack::new(&reg, coal, 2)), None, None, None],
            ..Default::default()
        }),
    );

    // Refusals: a block outside the catalog, and a wrong category.
    let stone = reg.block_id("base:stone").unwrap();
    assert!(
        w.swap_slot_module_at(slot, "casing", stone).is_err(),
        "a non-catalog module is refused"
    );
    let porcelain = reg.block_id("base:casing_porcelain").unwrap();
    assert!(
        w.swap_slot_module_at(slot, "output", porcelain).is_err(),
        "a wrong category is refused"
    );

    // Swap in the porcelain tile: the capability folds in and the entity
    // (charge, fuel) is untouched.
    assert!(w.swap_slot_module_at(slot, "casing", porcelain).is_ok());
    let Some(BlockEntity::Multiblock(m)) = w.block_entity(&(mx, my, mz)) else {
        panic!("instance")
    };
    assert!(
        m.capabilities.ceramic,
        "porcelain grants the ceramic capability"
    );
    assert_eq!(m.charge[0].as_ref().unwrap().item, iron, "charge survives");
    assert_eq!(m.fuel[0].as_ref().unwrap().item, coal, "fuel survives");
    assert_eq!(m.stats.heat, 23, "a heat-1 tile keeps the ring's heat");

    // Swap to the hotter tier: the numeric change is observable at once,
    // no break/rebuild, and the capability is gone with the tile.
    let adv = reg.block_id("base:firebrick_advanced").unwrap();
    assert!(w.swap_slot_module_at(slot, "casing", adv).is_ok());
    let Some(BlockEntity::Multiblock(m)) = w.block_entity(&(mx, my, mz)) else {
        panic!("instance")
    };
    assert_eq!(m.stats.heat, 24, "a hotter casing shows up immediately");
    assert_eq!(m.stats.heat_cells, 23);
    assert!(
        !m.capabilities.ceramic,
        "advanced firebrick grants no capability"
    );
    assert_eq!(m.charge[0].as_ref().unwrap().item, iron, "still charged");
}
