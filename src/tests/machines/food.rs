//! Food scenarios.

use super::*;

#[test]
fn food_spoils_slower_in_a_cellar_and_salted_keeps() {
    use crate::world::{BlockEntity, ChestState};
    let reg = base_reg();
    let mut w = test_world_with("perish", reg.clone());
    let stone = b(&reg, "base:stone");
    let meat = reg.item_id("base:raw_venison").unwrap();
    let salted = reg.item_id("base:salted_meat").unwrap();
    let mush = reg.item_id("base:spoiled_mush").unwrap();
    let chest = b(&reg, "base:chest");
    // A surface chest in daylight and a buried chest in the dark.
    let sy = w.surface_height(4, 4);
    w.set_block(4, sy + 1, 4, chest);
    for dx in -1..=1 {
        for dy in 0..=2 {
            for dz in -1..=1 {
                w.set_block(20 + dx, 40 + dy, 4 + dz, stone);
            }
        }
    }
    w.set_block(20, 41, 4, chest);
    w.relight_and_cascade(crate::chunk::ChunkPos::of_world(20, 4));
    let mut open = ChestState::default();
    open.slots[0] = Some(ItemStack::new(&reg, meat, 4));
    open.slots[1] = Some(ItemStack::new(&reg, salted, 4));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Chest(open));
    let mut cellar = ChestState::default();
    cellar.slots[0] = Some(ItemStack::new(&reg, meat, 4));
    w.insert_block_entity((20, 41, 4), BlockEntity::Chest(cellar));
    // Long enough for the open chest's stack to burn its whole
    // freshness at whatever the current rate is, plus a margin. A bare
    // second count here stopped being long enough the moment the
    // calendar retuned FRESHNESS_PER_SEC.
    let full = reg.item(meat).durability as f32;
    let secs = full / crate::world::FRESHNESS_PER_SEC * 1.12;
    for _ in 0..(secs / 20.0).ceil() as u32 {
        w.tick_entities(20.0);
    }
    let surface_meat = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("chest"),
    };
    let cellar_meat = match w.block_entity(&(20, 41, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("cellar chest"),
    };
    assert_eq!(
        surface_meat.item, mush,
        "raw venison rots on the surface once its freshness runs out"
    );
    assert_eq!(cellar_meat.item, meat, "the cellar kept it");
    assert!(
        cellar_meat.durability as f32 >= full * 0.6,
        "cellar decay runs at quarter rate ({} of {full})",
        cellar_meat.durability
    );
    let salted_left = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[1].unwrap(),
        _ => panic!("chest"),
    };
    assert_eq!(salted_left.item, salted, "salted meat shrugs it off");
}

#[test]
fn legacy_food_stacks_initialize_instead_of_rotting() {
    use crate::world::{BlockEntity, ChestState};
    let reg = base_reg();
    let mut w = test_world_with("legacy-food", reg.clone());
    let berry = reg.item_id("base:berry").unwrap();
    let chest = b(&reg, "base:chest");
    let sy = w.surface_height(4, 4);
    w.set_block(4, sy + 1, 4, chest);
    let mut c = ChestState::default();
    // A pre-freshness save: durability 0 on a perishable.
    c.slots[0] = Some(ItemStack {
        item: berry,
        count: 5,
        durability: 0,
        arcane_id: 0,
    });
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Chest(c));
    w.tick_entities(20.0);
    let st = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("chest"),
    };
    assert_eq!(st.item, berry, "legacy berries survive the sweep");
    assert_eq!(st.durability, 1200, "and start their clock fresh");
}

#[test]
fn the_smoker_cures_over_a_live_torch() {
    use crate::world::{BlockEntity, SMOKE_SECS, SmokerState};
    let reg = base_reg();
    let mut w = test_world_with("smoker", reg.clone());
    let rack = b(&reg, "base:smoking_rack");
    let torch = b(&reg, "base:torch");
    let stone = b(&reg, "base:stone");
    let y = 200;
    w.set_block(4, y, 4, stone);
    w.set_block(4, y + 1, 4, torch);
    w.set_block(4, y + 2, 4, rack);
    let raw = reg.item_id("base:raw_boar").unwrap();
    let smoked = reg.item_id("base:smoked_meat").unwrap();
    let mut sm = SmokerState::default();
    sm.meat[0] = Some(ItemStack::new(&reg, raw, 1));
    sm.meat[2] = Some(ItemStack::new(&reg, raw, 1));
    w.insert_block_entity((4, y + 2, 4), BlockEntity::Smoker(sm));
    // Cold rack (no torch): nothing cures.
    w.set_block(4, y + 1, 4, crate::registry::AIR);
    for _ in 0..40 {
        w.tick_entities(20.0);
    }
    let Some(BlockEntity::Smoker(sm)) = w.block_entity(&(4, y + 2, 4)) else {
        panic!("rack stands");
    };
    assert_eq!(sm.meat[0].unwrap().item, raw, "cold smoke cures nothing");
    // Relight and let it cure through.
    w.set_block(4, y + 1, 4, torch);
    let steps = (SMOKE_SECS / 20.0) as i32 + 2;
    for _ in 0..steps {
        w.tick_entities(20.0);
    }
    let Some(BlockEntity::Smoker(sm)) = w.block_entity(&(4, y + 2, 4)) else {
        panic!("rack stands");
    };
    assert_eq!(sm.meat[0].unwrap().item, smoked, "the cut cured");
    assert_eq!(sm.meat[2].unwrap().item, smoked, "the whole load together");
    assert_eq!(
        sm.meat[0].unwrap().durability,
        reg.item(smoked).durability,
        "smoked keeps six days"
    );
}
