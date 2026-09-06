//! Disturbance scenarios.

use super::*;

#[test]
fn lightning_strikes_only_what_was_always_wild() {
    let reg = base_reg();
    let mut w = test_world_with("bolt", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let charred = b(&reg, "base:charred_soil");
    // Natural country takes the strike: char plus banked bloom.
    let hit = w.lightning_strike(8, 8);
    assert!(hit.is_some(), "the bolt lands on wild ground");
    let (sx, sy, sz) = hit.unwrap();
    assert_eq!(w.get_block(sx, sy, sz), charred, "the ground is scorched");
    assert!(w.bloom_at(8, 8) > 0.0, "and the bloom is banked");
    // Touched country is off the target list, absolutely.
    let mut wt = test_world_with("bolt-touched", reg.clone());
    let ht = wt.surface_height(8, 8);
    pad(&mut wt, &reg, 0, 16, 0, 16, ht);
    wt.player_touched.insert(tchunk(0, 0));
    assert!(
        wt.lightning_strike(8, 8).is_none(),
        "the wild never touches what players built"
    );
    // And the scorch tills into the richest field in the game.
    let meta = crate::world::soil::soil_meta(crate::world::soil::FERT_MAX, 0);
    assert_eq!(
        crate::world::soil::fert_of(meta),
        crate::world::soil::FERT_MAX
    );
}

#[test]
fn the_bloom_erupts_and_burns_down() {
    let reg = base_reg();
    let mut w = test_world_with("bloom", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    // A parent log so the green tide has a species to seed.
    let log = b(&reg, "base:log");
    for dy in 1..=3 {
        w.set_block(12, h + dy, 12, log);
    }
    w.add_bloom(8, 8, 3.0);
    assert!(w.bloom_at(8, 8) > 0.0);
    let flowers = [b(&reg, "base:meadow_bloom"), b(&reg, "base:ember_poppy")];
    let mut rng = 71u32;
    let mut bloomed = 0;
    for _ in 0..8000 {
        w.random_tick(&mut rng);
        bloomed = (0..=24)
            .flat_map(|x| (0..=24).map(move |z| (x, z)))
            .filter(|&(x, z)| flowers.contains(&w.get_block(x, h + 1, z)))
            .count();
        if bloomed >= 3 {
            break;
        }
    }
    assert!(
        bloomed >= 3,
        "the charged cell erupts in flowers ({bloomed})"
    );
    // The charge burns down day by day and the ledger survives a save.
    let before = w.bloom_at(8, 8);
    w.tick_ire(1.0);
    assert!(w.bloom_at(8, 8) < before, "blooms fade");
    // Persistence round-trip.
    let dir = tmp_dir("bloom-save");
    let mut w2 = World::new(9, dir.clone(), reg.clone());
    w2.ensure_chunk(tchunk(0, 0));
    w2.add_bloom(40, 40, 2.5);
    save_world(&mut w2);
    let w3 = World::load_or_create(dir, reg.clone()).unwrap();
    assert!(w3.bloom_at(40, 40) > 2.0, "the bloom ledger persists");
}
