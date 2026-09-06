//! Ire gains decay tiers and persistence scenarios.

use super::*;

#[test]
fn ire_gains_decay_tiers_and_persistence() {
    let reg = base_reg();
    let dir = tmp_dir("iresave");
    let mut w = World::new(3, dir.clone(), reg.clone());
    // Block classes.
    assert_eq!(w.ire_for_block(reg.block_id("base:log").unwrap()), 0.3);
    assert_eq!(
        w.ire_for_block(reg.block_id("base:copper_ore").unwrap()),
        0.4
    );
    assert_eq!(w.ire_for_block(reg.block_id("base:stone").unwrap()), 0.05);
    assert_eq!(w.ire_for_block(reg.block_id("base:planks").unwrap()), 0.0);
    // Tier thresholds.
    w.add_ire(30.0);
    assert_eq!(w.ire_tier(), 1, "uneasy");
    w.add_ire(60.0);
    assert_eq!(w.ire_tier(), 3, "wrathful");
    w.add_ire(500.0);
    assert_eq!(w.ire, 100.0, "clamped");
    // Decay: -4 per day.
    w.tick_ire(0.5);
    assert!((w.ire - 98.0).abs() < 0.01);
    // Planting refunds, capped at 8/day.
    for _ in 0..100 {
        w.plant_ire(0.5);
    }
    assert!((w.ire - 90.0).abs() < 0.01, "daily cap of 8, got {}", w.ire);
    w.tick_ire(0.6); // day rolls over -> cap resets
    w.plant_ire(0.5);
    assert!(w.ire < 90.0 - 2.0, "cap reset next day");
    // Persistence via world.toml.
    save_world(&mut w);
    let w2 = World::load_or_create(dir, reg).unwrap();
    assert!((w2.ire - w.ire).abs() < 0.01, "ire round-trips");
}
