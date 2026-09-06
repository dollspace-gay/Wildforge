//! Bloomery multiblock fires batches and fears the rain scenarios.

use super::*;

#[test]
fn bloomery_multiblock_fires_batches_and_fears_the_rain() {
    use crate::world::{BLOOMERY_FIRE_SECS, BlockEntity, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("steel-fire", reg.clone());
    let my = 120; // open sky, far above terrain
    build_bloomery(&mut w, &reg, 10, my, 10);
    assert!(w.check_bloomery(10, my, 10).is_some(), "shell validates");
    // Any missing brick breaches it.
    let fb = reg.block_id("base:firebrick").unwrap();
    w.set_block(11, my + 2, 11, AIR);
    assert!(w.check_bloomery(10, my, 10).is_none(), "breach detected");
    w.set_block(11, my + 2, 11, fb);
    assert!(
        w.check_bloomery(10, my, 10).is_some(),
        "repair re-validates"
    );

    // Charge it full (8 iron + 8 charcoal), light, and fire to the end.
    let iron = reg.item_id("base:iron_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    let bloom = reg.item_id("base:steel_bloom").unwrap();
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, iron, 2));
        st.fuel[i] = Some(ItemStack::new(&reg, coal, 2));
    }
    w.insert_block_entity((10, my, 10), BlockEntity::Multiblock(st));
    assert!(w.light_bloomery(10, my, 10).is_ok(), "lights when charged");
    assert_eq!(
        w.get_block(10, my, 10),
        reg.block_id("base:bloomery_lit").unwrap(),
        "the mouth glows"
    );
    // Clear skies: full rate. Fire it through.
    w.force_local_weather("clear");
    let steps = (BLOOMERY_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(b)) = w.block_entity(&(10, my, 10)) else {
        panic!("bloomery survived");
    };
    assert!(!b.lit, "the firing ended");
    let blooms: u32 = b
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == bloom)
        .map(|s| s.count)
        .sum();
    assert_eq!(blooms, 6, "a full 8+8 firing yields 6 blooms");
    assert_eq!(
        w.get_block(10, my, 10),
        reg.block_id("base:bloomery").unwrap(),
        "the mouth cools"
    );

    // A partial 2+2 charge yields a single bloom.
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((10, my, 10), BlockEntity::Multiblock(st));
    w.light_bloomery(10, my, 10).unwrap();
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    let blooms: u32 = b
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == bloom)
        .map(|s| s.count)
        .sum();
    assert_eq!(blooms, 1, "2+2 makes one bloom");

    // Rain halves an unroofed stack; a storm douses it outright.
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((10, my, 10), BlockEntity::Multiblock(st));
    w.light_bloomery(10, my, 10).unwrap();
    w.force_local_weather("precip");
    for _ in 0..20 {
        w.tick_entities(1.0);
    }
    let Some(BlockEntity::Multiblock(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(
        (b.progress - 10.0).abs() < 0.6,
        "rain fires at half rate, got {}",
        b.progress
    );
    w.force_local_weather("storm");
    w.tick_entities(1.0);
    let Some(BlockEntity::Multiblock(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(!b.lit, "a storm douses the unroofed stack");
    let kept: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
    assert_eq!(kept, 2, "the charge survives a dousing");

    // Roofed, the same rain doesn't slow it. (Cover the core top.)
    let plank = reg.block_id("base:planks").unwrap();
    w.set_block(11, my + 4, 10, plank);
    let Some(BlockEntity::Multiblock(b)) = w.block_entity_mut(&(10, my, 10)) else {
        panic!()
    };
    b.lit = true;
    b.progress = 0.0;
    b.core = Some(bp(11, my, 10));
    w.force_local_weather("precip");
    for _ in 0..10 {
        w.tick_entities(1.0);
    }
    let Some(BlockEntity::Multiblock(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(
        (b.progress - 10.0).abs() < 0.6,
        "a roof keeps the fire honest, got {}",
        b.progress
    );
}
