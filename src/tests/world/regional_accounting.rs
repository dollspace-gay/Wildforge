//! Regional accounting scenarios.

use super::*;

#[test]
fn the_land_remembers_where() {
    let reg = base_reg();
    let dir = tmp_dir("rire");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        // A clearcut in one valley, a garden in another.
        for _ in 0..40 {
            w.add_ire_at(100, 100, 1.0);
        }
        for _ in 0..40 {
            w.plant_ire_at(3000, 3000, 1.0);
        }
        assert_eq!(w.regional_ire_at(100, 100), 20.0, "grudge clamps at 20");
        assert_eq!(w.regional_ire_at(3000, 3000), -20.0, "grace clamps at -20");
        assert_eq!(w.regional_ire_at(100, 3000), 0.0, "elsewhere is neutral");
        // The same world mood feels different on different ground.
        w.ire = 30.0; // globally tier 1
        assert_eq!(w.ire_tier(), 1);
        assert_eq!(w.ire_tier_at(100, 100), 3, "the angry forest hunts");
        assert_eq!(w.ire_tier_at(3000, 3000), 0, "the tended valley forgives");
        // Grudges fade: a full day decays 2 points.
        w.tick_ire(1.0);
        assert!(
            (w.regional_ire_at(100, 100) - 18.0).abs() < 0.01,
            "decay toward zero ({})",
            w.regional_ire_at(100, 100)
        );
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg).unwrap();
    assert!(
        w.regional_ire_at(100, 100) > 17.0,
        "the ledger persists ({})",
        w.regional_ire_at(100, 100)
    );
}

#[test]
fn regional_ledgers_are_face_aware_and_round_trip() {
    use crate::planet::{Face, SurfacePos};

    let reg = base_reg();
    let dir = tmp_dir("planet-ledgers");
    let pos_z = SurfacePos::new(Face::PosZ, 12, 34).unwrap();
    let pos_x = SurfacePos::new(Face::PosX, 12, 34).unwrap();
    let mut world = World::new(45, dir.clone(), reg.clone());
    world.add_ire_at_surface(pos_z, 3.0);
    world.add_ire_at_surface(pos_x, 7.0);
    world.add_bloom_at_surface(pos_z, 2.0);
    world.add_bloom_at_surface(pos_x, 5.0);
    assert_eq!(world.regional_ire_at_surface(pos_z), 3.0);
    assert_eq!(world.regional_ire_at_surface(pos_x), 7.0);
    assert_eq!(world.bloom_at_surface(pos_z), 2.0);
    assert_eq!(world.bloom_at_surface(pos_x), 5.0);

    let report = world.save_modified();
    assert!(report.is_ok(), "ledger save failed: {}", report.summary());
    drop(world);

    let loaded = World::load_or_create(dir, reg).unwrap();
    assert_eq!(loaded.regional_ire_at_surface(pos_z), 3.0);
    assert_eq!(loaded.regional_ire_at_surface(pos_x), 7.0);
    assert_eq!(loaded.bloom_at_surface(pos_z), 2.0);
    assert_eq!(loaded.bloom_at_surface(pos_x), 5.0);
}

#[test]
fn a_chunk_pays_only_for_what_it_actually_holds() {
    use crate::chunk::{CHUNK_CELLS, Chunk};

    // Every plane used to be allocated dense and unconditionally: 128 KB of
    // blocks, 64 KB of metadata, 192 KB of block light and 64 KB of sky light
    // for every chunk in memory, whether or not any of it said anything.
    const DENSE: usize = CHUNK_CELLS * (2 + 1 + 3 + 1);
    assert_eq!(DENSE, 458_752, "the old unconditional cost, 448 KiB");

    // A fresh chunk says nothing at all and costs nothing.
    let fresh = Chunk::new();
    assert_eq!(fresh.heap_bytes(), 0, "open air is free");

    // Real generated terrain, lit.
    let mut w = test_world("chunk-bytes");
    let pos = tchunk(0, 0);
    w.ensure_chunk(pos);
    let real = w.chunks()[&pos].heap_bytes();
    assert!(real > 0, "terrain costs something");
    assert!(
        real < DENSE,
        "a real chunk ({real} bytes) must cost less than the old flat {DENSE}"
    );
    // Blocks and sky light genuinely vary with terrain; block light and
    // metadata almost never do, and they were more than half the bill.
    assert!(
        real <= DENSE / 2,
        "a chunk with no torch and no block state should cost at most half \
         the old {DENSE} bytes, got {real}"
    );
}

#[test]
fn the_lands_ledgers_do_not_grow_without_bound() {
    // regional_ire, bloom and blessed_streak are keyed per 256-block cell and
    // written every time anyone takes or tends anything. They are only
    // bounded because each one decays to nothing and drops its entry when it
    // gets there — behaviour nothing tested, in maps that are also persisted
    // and rewritten whole on every save.
    let mut w = test_world("ledger-bounds");

    // A thousand distinct regional cells distributed over the six finite
    // faces, all charged. The old arithmetic ran ever farther off PosZ and
    // was exactly the unbounded-world assumption this test now guards
    // against.
    for i in 0..1000 {
        let face = crate::planet::Face::ALL[i % crate::planet::Face::ALL.len()];
        let cell = i / crate::planet::Face::ALL.len();
        let u = ((cell % 32) * 256 + 128) as u16;
        let v = (((cell / 32) % 32) * 256 + 128) as u16;
        let pos = crate::planet::SurfacePos::new(face, u, v).unwrap();
        w.add_ire_at_surface(pos, 5.0);
        w.add_bloom_at_surface(pos, 2.0);
    }
    assert!(w.ledger_len() > 0, "charging the land records something");

    // A season passes with nobody touching any of it.
    for _ in 0..(crate::world::SEASON_DAYS * 4) {
        w.tick_ire(1.0);
    }
    assert_eq!(
        w.ledger_len(),
        0,
        "grudges, gratitude and blooms all fade to nothing and stop being \
         stored — a ledger that only ever grew would outlive the world"
    );
}

#[test]
fn industrial_machines_and_buildings_feed_regional_ire() {
    use crate::world::{BlockEntity, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("e12-industrial", reg.clone());
    let my = 120;
    assert_eq!(w.ire, 0.0);
    // Raising a machine mouth costs the valley once (capability E12).
    let mouth = reg.block_id("base:bloomery").unwrap();
    let spot = bp(20, my, 20);
    w.ensure_chunk(spot.chunk());
    w.set_block_at(spot, AIR); // make sure the cell is replaceable
    assert!(w.place_block_at(spot, mouth), "the building places");
    let after_build = w.ire;
    assert!(
        (after_build - crate::world::World::INDUSTRIAL_BUILDING_IRE).abs() < 0.001,
        "raising an industrial building charges ire: {after_build}"
    );
    // A lit bloomery feeds ire while it runs: ~0.01/s.
    build_bloomery(&mut w, &reg, 12, my, 12);
    let iron = it(&reg, "base:iron_ingot");
    let coal = it(&reg, "base:charcoal");
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((12, my, 12), BlockEntity::Multiblock(st));
    w.force_local_weather("clear");
    w.light_bloomery(12, my, 12).expect("lights when charged");
    let before = w.ire;
    for _ in 0..60 {
        w.tick_entities(1.0);
    }
    let fed = w.ire - before;
    assert!(fed > 0.5, "a minute of firing feeds regional ire: {fed}");
}

#[test]
fn a_mode_can_repoint_the_industrial_feed_off() {
    // Modes parse `industrial_ire`; verify the override plumbing directly.
    let mut ruleset = crate::ruleset::Ruleset::survival();
    assert!(ruleset.industrial_ire, "survival keeps the feed live");
    ruleset.apply_overrides(&crate::registry::ModeDef {
        id: "quiet".into(),
        base: None,
        creative: None,
        hunger: None,
        fall_damage: None,
        drowning: None,
        lava_burn: None,
        hostile_spawns: None,
        ire: None,
        hearts: None,
        weather_extremes: None,
        pvp: None,
        skills: None,
        equipment: None,
        industrial_ire: Some(false),
        nest_spawns: None,
    });
    assert!(!ruleset.industrial_ire, "the mode repoints the feed off");
}
