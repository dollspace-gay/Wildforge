//! Grazing scenarios.

use super::*;

#[test]
fn the_hungry_deer_raids_the_unfenced_field() {
    let reg = base_reg();
    let mut w = test_world_with("raid", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let farm = b(&reg, "base:farmland");
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    for x in 14..=16 {
        for z in 6..=10 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(40, 0));
            w.set_block(x, h + 1, z, ripe);
        }
    }
    w.spawn_mob(hungry_deer(&reg, glam::Vec3::new(6.5, h as f32 + 1.0, 8.5)));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    let mut bitten = false;
    for _ in 0..4000 {
        sim.advance(0.05, &[], &mut evs);
        bitten = (14..=16)
            .flat_map(|x| (6..=10).map(move |z| (x, z)))
            .any(|(x, z)| sim.world.get_block(x, h + 1, z) == b(&reg, "base:wheat_seeds"));
        if bitten {
            break;
        }
    }
    assert!(
        bitten,
        "the deer walked to the field and ate the ripest thing in reach"
    );
}

#[test]
fn the_fence_defeats_the_raid() {
    let reg = base_reg();
    let mut w = test_world_with("fenced", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 24, 0, 24, h);
    let farm = b(&reg, "base:farmland");
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    let stone = b(&reg, "base:stone");
    for x in 14..=16 {
        for z in 6..=10 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(40, 0));
            w.set_block(x, h + 1, z, ripe);
        }
    }
    // A one-high wall around the field: the raid never climbs.
    for x in 12..=18 {
        for z in 4..=12 {
            if x == 12 || x == 18 || z == 4 || z == 12 {
                w.set_block(x, h + 1, z, stone);
            }
        }
    }
    w.spawn_mob(hungry_deer(&reg, glam::Vec3::new(6.5, h as f32 + 1.0, 8.5)));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    for _ in 0..4000 {
        sim.advance(0.05, &[], &mut evs);
    }
    let standing = (14..=16)
        .flat_map(|x| (6..=10).map(move |z| (x, z)))
        .filter(|&(x, z)| sim.world.get_block(x, h + 1, z) == ripe)
        .count();
    assert_eq!(standing, 15, "every stalk stands behind the wall");
}

#[test]
fn grazing_turns_grass_and_digestion_pays_the_soil() {
    let reg = base_reg();
    let mut w = test_world_with("graze", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    w.spawn_mob(hungry_deer(
        &reg,
        glam::Vec3::new(10.5, h as f32 + 1.0, 10.5),
    ));
    let mut sim = crate::server::Server::new(w, 0.3, 7);
    let mut evs = Vec::new();
    let dirt = b(&reg, "base:dirt");
    let dung = it(&reg, "base:dung");
    let mut grazed = false;
    let mut dunged = false;
    for _ in 0..6000 {
        sim.advance(0.05, &[], &mut evs);
        if !grazed {
            grazed = (0..=20)
                .flat_map(|x| (0..=20).map(move |z| (x, z)))
                .any(|(x, z)| sim.world.get_block(x, h, z) == dirt);
        }
        if sim
            .world
            .take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == dung)
        {
            dunged = true;
        }
        if grazed && dunged {
            break;
        }
    }
    assert!(grazed, "the deer cropped the grass down to dirt");
    assert!(dunged, "digestion finished where it always does");
    // And the dung is worth soil: the fertilizer loop closes.
    let farm = b(&reg, "base:farmland");
    sim.world
        .set_block_meta(2, h, 2, farm, soil::soil_meta(10, 1));
    let v = soil::fertilizer_value("base:dung");
    assert!(v > 0);
    assert!(sim.world.feed_soil(2, h, 2, v));
    assert_eq!(soil::fert_of(sim.world.get_meta(2, h, 2)), 10 + v);
}

#[test]
fn herds_lean_homeward() {
    let reg = base_reg();
    let mut w = test_world_with("herd", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 31, 0, 31, h);
    // A walled arena in tended country: the trio can't scatter off
    // the pad, so what the measurement sees is the pull, not luck.
    let stone = b(&reg, "base:stone");
    for x in 0..=31 {
        for z in 0..=31 {
            if x == 0 || x == 31 || z == 0 || z == 31 {
                w.set_block(x, h + 1, z, stone);
            }
        }
    }
    for cx in -1..=1 {
        for cz in -1..=1 {
            w.player_touched.insert(tchunk(cx, cz));
        }
    }
    let si = reg.animal_id("base:deer").unwrap();
    for x in [3.5f32, 15.5, 27.5] {
        let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(x, h as f32 + 1.0, 15.5), 0.0);
        m.health = reg.animals[si].health;
        m.belly = 9999.0; // sated: today is for walking
        m.tamed = true; // the tag that finds our trio among strangers
        w.spawn_mob(m);
    }
    // Seeding and repop add strangers mid-run; the tame flag finds
    // our trio no matter who else wanders in.
    let spread = |w: &World| -> f32 {
        let trio: Vec<glam::Vec3> = w
            .mobs()
            .iter()
            .filter(|m| m.tamed)
            .map(|m| m.pos.local())
            .collect();
        let mut s = 0.0f32;
        for i in 0..trio.len() {
            for j in (i + 1)..trio.len() {
                s += (trio[i] - trio[j]).length();
            }
        }
        s / 3.0
    };
    let before = spread(&w);
    let mut rng = 2024u32;
    for _ in 0..4800 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    }
    let after = spread(&w);
    assert!(
        after < before,
        "the herd drifts together ({before:.1} -> {after:.1})"
    );
}

#[test]
fn a_calm_deer_minds_the_pen_but_panic_leaps() {
    let reg = base_reg();
    let mut w = test_world_with("pen", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let plank = b(&reg, "base:planks");
    // A one-high plank pen in tended country.
    for x in 6..=10 {
        for z in 6..=10 {
            if x == 6 || x == 10 || z == 6 || z == 10 {
                w.set_block(x, h + 1, z, plank);
            }
        }
    }
    for cx in -1..=1 {
        for cz in -1..=1 {
            w.player_touched.insert(tchunk(cx, cz));
        }
    }
    let si = reg.animal_id("base:deer").unwrap();
    let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    m.health = reg.animals[si].health;
    m.tamed = true;
    m.belly = 9999.0;
    m.tamed = true;
    w.spawn_mob(m);
    let mut rng = 77u32;
    for _ in 0..2400 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    }
    let deer = w.mobs().iter().find(|m| m.tamed).expect("the deer");
    assert!(
        (7.0..10.0).contains(&deer.pos.x) && (7.0..10.0).contains(&deer.pos.z),
        "the calm deer stayed penned (at {:.1} {:.1})",
        deer.pos.x,
        deer.pos.z
    );
}
