//! Containers scenarios.

use super::*;

#[test]
fn chest_stores_spills_and_persists() {
    let reg = base_reg();
    let dir = tmp_dir("chestsave");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let chest = reg.block_id("base:chest").unwrap();
    assert_eq!(reg.block(chest).interaction.as_deref(), Some("chest"));
    let pos = (4, 100, 4);
    w.set_block(pos.0, pos.1, pos.2, chest);
    let mut state = crate::world::ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:bread"), 3));
    state.slots[26] = Some(ItemStack::new(&reg, it(&reg, "base:bronze_ingot"), 7));
    w.insert_block_entity(pos, crate::world::BlockEntity::Chest(state));
    save_world(&mut w);

    // Round-trip by name, plus an unknown item that must skip cleanly.
    let path = dir.join("entities.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[[chest]]\npos = { face = \"PosZ\", u = 4105, y = 90, v = 4105 }\n[[chest.slot]]\nindex = 0\nitem = \"gone:widget\"\ncount = 5\ndurability = 0\n");
    std::fs::write(&path, text).unwrap();
    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    let Some(crate::world::BlockEntity::Chest(c)) = w2.block_entity(&pos) else {
        panic!("chest reloaded")
    };
    assert_eq!(
        c.slots[0].map(|s| (reg.item(s.item).name.clone(), s.count)),
        Some(("base:bread".to_string(), 3))
    );
    assert_eq!(c.slots[26].map(|s| s.count), Some(7));
    let Some(crate::world::BlockEntity::Chest(c2)) = w2.block_entity(&(9, 90, 9)) else {
        panic!("second chest reloaded")
    };
    assert!(c2.slots.iter().all(|s| s.is_none()), "unknown item skipped");

    // Breaking the chest spills every stack.
    let mut w3 = w2;
    // Worlds load chunks lazily. Load the saved voxel chunk before editing
    // the chest that lives in it.
    assert!(w3.ensure_chunk(tchunk(0, 0)));
    w3.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(!w3.has_block_entity(&pos));
    let spilled: Vec<_> = w3.pending_drops().iter().map(|(_, s)| s.count).collect();
    assert_eq!(
        spilled.iter().sum::<u32>(),
        10,
        "3 bread + 7 ingots spilled"
    );

    // Recipe: 8 planks in a ring.
    let mut g = vec![None; 9];
    for (i, slot) in g.iter_mut().enumerate() {
        if i != 4 {
            *slot = Some(ItemStack::new(&reg, it(&reg, "base:planks"), 1));
        }
    }
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("chest recipe");
    assert_eq!(r.output, it(&reg, "base:chest"));
}

#[test]
fn signs_hold_their_words_through_save_and_load() {
    use crate::world::{BlockEntity, SignState};
    let reg = base_reg();
    let dir = tmp_dir("signsave");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let sign = b(&reg, "base:sign");
        let sy = w.surface_height(4, 4);
        w.set_block(4, sy + 1, 4, sign);
        w.insert_block_entity(
            (4, sy + 1, 4),
            BlockEntity::Sign(SignState {
                lines: [
                    "SALT FAIR".to_string(),
                    "PRICES".to_string(),
                    "NE 400".to_string(),
                ],
            }),
        );
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let found: Vec<_> = w.sign_texts().collect();
    assert_eq!(found.len(), 1, "the sign persisted");
    assert_eq!(found[0].1.lines[0], "SALT FAIR");
    assert_eq!(found[0].1.lines[2], "NE 400");
}

#[test]
fn stall_validates_and_persists_its_shop() {
    use crate::world::{BlockEntity, StallState};
    let reg = base_reg();
    let dir = tmp_dir("stall");
    let counter = b(&reg, "base:stall_counter");
    let log = b(&reg, "base:log");
    let planks = b(&reg, "base:planks");
    let salt = it(&reg, "base:salt_crystal");
    let silver = it(&reg, "base:silver_ingot");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let y = 200;
        w.set_block(4, y, 4, counter);
        assert!(!w.check_stall(4, y, 4), "a bare counter is not a stall");
        // Posts and awning along x.
        for side in [-1i32, 1] {
            w.set_block(4 + side, y, 4, log);
            w.set_block(4 + side, y + 1, 4, log);
        }
        for i in -1i32..=1 {
            w.set_block(4 + i, y + 2, 4, planks);
        }
        assert!(w.check_stall(4, y, 4), "posts + awning make the stall");
        // Break the awning: trading stops.
        w.set_block(4, y + 2, 4, AIR);
        assert!(!w.check_stall(4, y, 4), "no awning, no stall");
        w.set_block(4, y + 2, 4, planks);
        let mut st = StallState {
            owner: [7; 16],
            owner_name: "DOLL".to_string(),
            ..Default::default()
        };
        st.goods[0] = Some(ItemStack::new(&reg, salt, 20));
        st.price = Some(ItemStack::new(&reg, silver, 1));
        w.insert_block_entity((4, y, 4), BlockEntity::Stall(st));
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let Some(BlockEntity::Stall(st)) = w.block_entity(&(4, 200, 4)) else {
        panic!("the stall persisted");
    };
    assert_eq!(st.owner, [7; 16], "ownership survives");
    assert_eq!(st.owner_name, "DOLL");
    assert_eq!(st.goods[0].unwrap().count, 20);
    assert_eq!(st.price.unwrap().item, silver, "the price template holds");
}
