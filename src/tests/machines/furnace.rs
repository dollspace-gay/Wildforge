//! Furnace scenarios.

use super::*;

#[test]
fn furnace_smelts_with_fuel_over_time() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace", reg.clone());
    let pos = (2, 80, 2);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    let raw = it(&reg, "base:raw_copper");
    let log = it(&reg, "base:log");
    w.insert_block_entity(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, raw, 2)),
            fuel: Some(ItemStack::new(&reg, log, 2)),
            ..Default::default()
        }),
    );
    // 8s smelt at 0.1s ticks; the first log (15s) lights immediately.
    for _ in 0..85 {
        w.tick_entities(0.1);
    }
    let Some(BlockEntity::Furnace(f)) = w.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.output.unwrap().item, it(&reg, "base:copper_ingot"));
    assert_eq!(f.input.unwrap().count, 1, "one raw consumed");
    assert_eq!(f.fuel.unwrap().count, 1, "first log consumed for fuel");
    assert!(f.burn_left > 0.0, "log burns 15s, smelt took 8");
    // Second smelt needs the second log (relights at the 15s mark).
    for _ in 0..90 {
        w.tick_entities(0.1);
    }
    let Some(BlockEntity::Furnace(f)) = w.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.output.unwrap().count, 2);
    assert!(f.input.is_none());
    assert!(f.fuel.is_none(), "second log lit");
    // No fuel, no input: idle without panicking.
    for _ in 0..50 {
        w.tick_entities(0.1);
    }
}

#[test]
fn furnace_state_persists_and_breaks_drop_contents() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace-save", reg.clone());
    let pos = (3, 80, 3);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    w.insert_block_entity(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, it(&reg, "base:raw_tin"), 5)),
            fuel: Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 3)),
            ..Default::default()
        }),
    );
    save_world(&mut w);
    // Reload: state comes back by item name.
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(tchunk(x, z));
        }
    }
    let Some(BlockEntity::Furnace(f)) = w2.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.input.unwrap().count, 5);
    assert_eq!(f.fuel.unwrap().item, it(&reg, "base:charcoal"));
    // Breaking the block spills the contents.
    w2.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(!w2.has_block_entity(&pos));
    assert_eq!(w2.pending_drops().len(), 2, "input + fuel drop");
}

#[test]
fn ember_fuel_speeds_the_furnace() {
    let reg = base_reg();
    let mut w = test_world("emberfast");
    let f = crate::world::FurnaceState {
        input: Some(ItemStack::new(&reg, it(&reg, "base:raw_iron"), 1)),
        fuel: Some(ItemStack::new(&reg, it(&reg, "base:ember"), 1)),
        ..Default::default()
    };
    w.insert_block_entity((0, 90, 0), crate::world::BlockEntity::Furnace(f));
    // A 10 s iron smelt at the ember's 2x finishes in ~5 s; without the
    // speedup, 8 s of ticks would not be enough.
    for _ in 0..80 {
        w.tick_entities(0.1);
    }
    let Some(crate::world::BlockEntity::Furnace(f)) = w.block_entity(&(0, 90, 0)) else {
        panic!()
    };
    assert!(
        f.output.map(|s| reg.item(s.item).name.clone()).as_deref() == Some("base:iron_ingot"),
        "iron done in 8s of ember fire (progress {})",
        f.progress
    );
}

#[test]
fn charged_furnace_fuel_and_inputs_leave_no_orphan_current() {
    use crate::arcane::{ArcaneOwner, Reservoir};
    use crate::world::{BlockEntity, FurnaceState};

    let reg = base_reg();
    let mut world = World::load_or_create(
        tmp_dir("charged-furnace-lifecycle").join("world"),
        reg.clone(),
    )
    .unwrap();
    let at = crate::planet::BlockPos::of_world(0, 90, 0).unwrap();

    let mut ember = ItemStack::new(&reg, it(&reg, "base:ember"), 1);
    world
        .bind_arcane_stack_at(at, &mut ember, "charged furnace test fuel")
        .unwrap();
    let ember_owner = ArcaneOwner::Item(ember.arcane_id);
    let dross_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&Reservoir::Dross];
    world.insert_block_entity_at(
        at,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, it(&reg, "base:raw_iron"), 1)),
            fuel: Some(ember),
            ..Default::default()
        }),
    );
    world.tick_entities(0.1);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&ember_owner).is_none());
    assert_eq!(
        ledger.audit().unwrap().reservoirs[&Reservoir::Dross],
        dross_before + 512,
        "ember fuel disorders its exact charge"
    );

    let quartz_at = crate::planet::BlockPos::of_world(1, 90, 0).unwrap();
    let mut quartz = ItemStack::new(&reg, it(&reg, "base:quartz_shard"), 1);
    world
        .bind_arcane_stack_at(quartz_at, &mut quartz, "charged furnace test input")
        .unwrap();
    let quartz_owner = ArcaneOwner::Item(quartz.arcane_id);
    let ambient_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&Reservoir::Ambient];
    world.insert_block_entity_at(
        quartz_at,
        BlockEntity::Furnace(FurnaceState {
            input: Some(quartz),
            fuel: Some(ItemStack::new(&reg, it(&reg, "base:log"), 1)),
            ..Default::default()
        }),
    );
    for _ in 0..120 {
        world.tick_entities(0.1);
    }
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&quartz_owner).is_none());
    assert_eq!(
        ledger.audit().unwrap().reservoirs[&Reservoir::Ambient],
        ambient_before + 384,
        "transformed quartz returns its exact charge to the environment"
    );
    assert!(ledger.audit().unwrap().is_balanced());
}
