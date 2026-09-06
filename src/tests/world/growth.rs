//! Growth scenarios.

use super::*;

#[test]
fn saplings_parse_drop_and_grow() {
    let reg = base_reg();
    // Leaves carry sapling bonus drops of their own species.
    let oak_leaves = reg.block(reg.block_id("base:leaves").unwrap());
    let (bd_item, ch) = oak_leaves.bonus_drop.expect("leaves drop saplings");
    assert_eq!(reg.item(bd_item).name, "base:oak_sapling");
    assert!((ch - 0.1).abs() < 0.001);
    let spruce = reg.block(reg.block_id("base:spruce_leaves").unwrap());
    assert_eq!(
        reg.item(spruce.bonus_drop.unwrap().0).name,
        "base:spruce_sapling"
    );
    // Grow: sapling on dirt in open air becomes a real tree.
    let mut w = test_world("sapgrow");
    let dirt = reg.block_id("base:dirt").unwrap();
    let sap = reg.block_id("base:oak_sapling").unwrap();
    w.set_block(4, 199, 4, dirt);
    w.set_block(4, 200, 4, sap);
    let ire0 = {
        w.ire = 50.0;
        w.ire
    };
    assert!(w.try_grow_sapling(4, 200, 4, 7), "clear sky: grows");
    let log = reg.block_id("base:log").unwrap();
    assert_eq!(w.get_block(4, 200, 4), log, "trunk replaces the sapling");
    let leaves = reg.block_id("base:leaves").unwrap();
    let mut leaf_count = 0;
    for x in 0..9 {
        for z in 0..9 {
            for y in 200..212 {
                if w.get_block(x, y, z) == leaves {
                    leaf_count += 1;
                }
            }
        }
    }
    assert!(leaf_count > 8, "canopy grew ({leaf_count} leaves)");
    assert!(
        (w.ire - (ire0 - 2.0)).abs() < 0.01,
        "maturation refunds 2 ire"
    );
    // Blocked trunk: stays a sapling.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(8, 199, 8, dirt);
    w.set_block(8, 200, 8, sap);
    w.set_block(8, 202, 8, stone);
    assert!(!w.try_grow_sapling(8, 200, 8, 7), "blocked: stays");
    assert_eq!(w.get_block(8, 200, 8), sap);
}

#[test]
fn the_green_tide_seeds_only_natural_kind_ground() {
    use crate::world::SEASON_DAYS;
    let reg = base_reg();
    let mut w = test_world_with("greentide", reg.clone());
    w.set_calendar_day(SEASON_DAYS); // summer: growth season
    // Force chunk (0,0) UNMODIFIED after our stage-setting: we build
    // via raw grass/log placement then clear the flag through save.
    let grass = b(&reg, "base:grass");
    let log = b(&reg, "base:log");
    let sy = 140;
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(x, sy, z, grass);
        }
    }
    for dy in 1..=4 {
        w.set_block(8, sy + dy, 8, log);
    }
    // Blessed country; set_block is the sim's hand, not a player's,
    // so the chunk stays natural in the green tide's eyes.
    for _ in 0..10 {
        w.plant_ire_at(8, 8, 1.0);
    }
    let mut rng = 11u32;
    for _ in 0..6000 {
        w.random_tick(&mut rng);
    }
    let mut saplings = 0;
    for x in 0..16 {
        for z in 0..16 {
            if reg.block(w.get_block(x, sy + 1, z)).sapling.is_some() {
                saplings += 1;
            }
        }
    }
    assert!(saplings >= 1, "the forest thickens ({saplings})");
    assert!(saplings <= 8, "but never marches ({saplings})");
    // The same ground, resented: nothing seeds.
    let mut w2 = test_world_with("greentide2", reg.clone());
    w2.set_calendar_day(SEASON_DAYS);
    for x in 0..16 {
        for z in 0..16 {
            w2.set_block(x, sy, z, grass);
        }
    }
    for dy in 1..=4 {
        w2.set_block(8, sy + dy, 8, log);
    }
    for _ in 0..10 {
        w2.add_ire_at(8, 8, 1.0);
    }
    let mut rng2 = 11u32;
    for i in 0..3000 {
        // A valley full of fruiting bushes credits its own cell, and
        // the natural country around this pad is thick with them —
        // keep the grievance fresh, which is what the test is about.
        if i % 100 == 0 {
            w2.add_ire_at(8, 8, 5.0);
        }
        w2.random_tick(&mut rng2);
    }
    let mut saplings2 = 0;
    for x in 0..16 {
        for z in 0..16 {
            if reg.block(w2.get_block(x, sy + 1, z)).sapling.is_some() {
                saplings2 += 1;
            }
        }
    }
    assert_eq!(saplings2, 0, "angry or touched ground stays bare");
}
