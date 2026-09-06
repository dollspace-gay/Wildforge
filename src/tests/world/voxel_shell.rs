//! Voxel shell scenarios.

use super::*;

#[test]
fn bedrock_floor_is_unbreakable_and_reseals_on_load() {
    let reg = base_reg();
    let root = reg.block_id("base:bedrock").expect("bedrock registered");
    let dir = tmp_dir("floor");
    let mut w = World::new(42, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    // Every column of a fresh chunk is floored.
    for x in 0..16 {
        for z in 0..16 {
            assert_eq!(w.get_block(x, 0, z), root, "floor at ({x},0,{z})");
        }
    }
    // No tool gives it a hardness: it cannot be mined.
    let pick = reg.item_id("base:wood_pickaxe");
    assert!(
        reg.effective_hardness(root, pick).is_none(),
        "bedrock unbreakable"
    );
    // A hole knocked in the floor (a creative dig, an old bug) heals
    // when the chunk loads again.
    w.set_block(4, 0, 4, AIR);
    save_world(&mut w);
    drop(w);
    let mut w2 = World::new(42, dir, reg);
    w2.ensure_chunk(tchunk(0, 0));
    assert_eq!(w2.get_block(4, 0, 4), root, "floor resealed on load");
}
