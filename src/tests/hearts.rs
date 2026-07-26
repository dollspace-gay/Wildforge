//! The hearts of the land: every country has one, and you can find it.

use super::*;
use crate::worldgen::Biome;

#[test]
fn every_country_raises_a_heart_you_can_find() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("hearts-find"), reg.clone());
    // Walk to a country's center and load the ground under it.
    let g = &w.generator;
    let site = (0..40)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .map(|(kx, kz)| (kx, kz, g.province_center(kx, kz)))
        .find(|&(_, _, (x, z))| {
            g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
                && g.plate_relief(&g.climate(x, z)) <= 30.0
        })
        .expect("a country on dry land");
    let (kx, kz, (sx, sz)) = site;
    let cp = ChunkPos::of_world(sx, sz);
    w.ensure_chunk(cp);
    // The heart stands at the site, in the form its country wears.
    let heart = w.heart_at(sx, sz).expect("the country has a spirit");
    assert_eq!(w.heart_at(sx, sz).map(|h| h.pos), Some(heart.pos));
    assert!(heart.alive(), "a fresh country's heart is alive");
    let form = crate::world::heart_form(w.generator.biome(sx, sz));
    let at = w.get_block(heart.pos.0, heart.pos.1, heart.pos.2);
    assert_eq!(
        reg.block(at).name,
        crate::world::heart_block_name(form, 2),
        "the site wears its country's form"
    );
    // It is a landmark, not a pebble: a bole and a stone stand tall.
    let tall = crate::world::heart_height(form);
    for dy in 0..tall {
        let b = w.get_block(heart.pos.0, heart.pos.1 + dy, heart.pos.2);
        assert!(
            reg.block(b).name.starts_with("base:heart_"),
            "the site stands {tall} high (missing at +{dy})"
        );
    }
    // And it is the heart of THIS province, keyed where it lives.
    assert_eq!(w.generator.province(sx, sz).key, (kx, kz));
}

#[test]
fn a_cairn_names_the_heart_and_its_condition() {
    let reg = base_reg();
    let mut w = World::new(7, tmp_dir("hearts-cairn"), reg.clone());
    let g = &w.generator;
    let (sx, sz) = (0..40)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .map(|(kx, kz)| g.province_center(kx, kz))
        .find(|&(x, z)| {
            g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
                && g.plate_relief(&g.climate(x, z)) <= 30.0
        })
        .expect("dry country");
    w.ensure_chunk(ChunkPos::of_world(sx, sz));
    let key = w.generator.province(sx, sz).key;
    // Standing on it: the report says so, and says it is well.
    let here = w.heart_report(sx, sz);
    assert!(here.contains("is here"), "{here}");
    assert!(here.contains("well"), "{here}");
    // A day's walk off: direction and distance, in plain words.
    let away = w.heart_report(sx - 300, sz);
    assert!(away.contains("blocks east"), "{away}");
    assert!(
        away.contains("~2") || away.contains("~3"),
        "a distance a person can use: {away}"
    );
    // Condition tracks the stage — a player never has to guess.
    w.set_heart_stage(key, 1);
    assert!(w.heart_report(sx, sz).contains("FAILING"));
    w.set_heart_stage(key, 0);
    let dead = w.heart_report(sx, sz);
    assert!(dead.contains("dead"), "{dead}");
    assert!(!w.heart_alive_at(sx, sz), "and the country knows it");
    // The blocks changed with it: the site is legible at a glance.
    let form = crate::world::heart_form(w.generator.biome(sx, sz));
    let h = w.heart_at(sx, sz).unwrap();
    let at = w.get_block(h.pos.0, h.pos.1, h.pos.2);
    assert_eq!(reg.block(at).name, crate::world::heart_block_name(form, 0));
}

#[test]
fn hearts_survive_the_save() {
    let reg = base_reg();
    let dir = tmp_dir("hearts-save");
    let (sx, sz, key);
    {
        let mut w = World::new(11, dir.clone(), reg.clone());
        let g = &w.generator;
        let found = (0..40)
            .flat_map(|r| {
                (-r..=r)
                    .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                    .collect::<Vec<_>>()
            })
            .map(|(kx, kz)| g.province_center(kx, kz))
            .find(|&(x, z)| {
                g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
                    && g.plate_relief(&g.climate(x, z)) <= 30.0
            })
            .expect("dry country");
        sx = found.0;
        sz = found.1;
        w.ensure_chunk(ChunkPos::of_world(sx, sz));
        key = w.generator.province(sx, sz).key;
        w.set_heart_stage(key, 1);
        if let Some(h) = w.hearts.get_mut(&key) {
            h.strain = 6.5;
        }
        w.save_modified();
    }
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos::of_world(sx, sz));
    let h = w.heart_at(sx, sz).expect("the ledger came back");
    assert_eq!(h.stage, 1, "a failing heart is still failing");
    assert!((h.strain - 6.5).abs() < 0.01, "its grievance persists");
    assert_eq!(w.generator.province(sx, sz).key, key);
}

#[test]
fn a_country_form_matches_its_kind() {
    // Wooded country grows a bole; dry country keeps a spring; the
    // cold and open ground raises a stone.
    use crate::world::heart_form;
    for b in [Biome::Forest, Biome::Taiga, Biome::Jungle, Biome::Swamp] {
        assert_eq!(heart_form(b), "base:heart_tree", "{b:?}");
    }
    for b in [
        Biome::Desert,
        Biome::Badlands,
        Biome::Savanna,
        Biome::Scrubland,
    ] {
        assert_eq!(heart_form(b), "base:heart_spring", "{b:?}");
    }
    for b in [
        Biome::Arctic,
        Biome::Tundra,
        Biome::Mountains,
        Biome::Plains,
    ] {
        assert_eq!(heart_form(b), "base:heart_stone", "{b:?}");
    }
    // Every form has all three stages registered as real blocks.
    let reg = base_reg();
    for form in ["base:heart_tree", "base:heart_spring", "base:heart_stone"] {
        for stage in 0..=2u8 {
            let name = crate::world::heart_block_name(form, stage);
            assert!(reg.block_id(&name).is_some(), "missing {name}");
        }
    }
}

/// A world with a heart loaded, and the key of its country.
fn world_with_heart(seed: u32, tag: &str) -> (World, (i32, i32), (i32, i32)) {
    let reg = base_reg();
    let mut w = World::new(seed, tmp_dir(tag), reg);
    let g = &w.generator;
    let (sx, sz) = (0..40)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .map(|(kx, kz)| g.province_center(kx, kz))
        .find(|&(x, z)| {
            g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
                && g.plate_relief(&g.climate(x, z)) <= 30.0
        })
        .expect("dry country");
    w.ensure_chunk(ChunkPos::of_world(sx, sz));
    let key = w.generator.province(sx, sz).key;
    assert!(w.heart_at(sx, sz).is_some(), "the country has a heart");
    (w, key, (sx, sz))
}

#[test]
fn resented_country_sickens_slowly_and_forgives() {
    let (mut w, _key, (sx, sz)) = world_with_heart(42, "hearts-sicken");
    // Grievance, held. The sickening takes seasons, not minutes.
    for _ in 0..30 {
        w.add_ire_at(sx, sz, 4.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 2, "not yet");
    for _ in 0..4 {
        w.tick_ire(1.0);
    }
    assert_eq!(
        w.heart_at(sx, sz).unwrap().stage,
        2,
        "four days of anger is not a death sentence"
    );
    // Held long enough, it shows — and it shows BEFORE it kills.
    for _ in 0..14 {
        for _ in 0..30 {
            w.add_ire_at(sx, sz, 4.0);
        }
        w.tick_ire(1.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 1, "the heart is failing");
    assert!(w.heart_report(sx, sz).contains("FAILING"));
    // Stop taking, and it comes back: the wild forgives the patient.
    for _ in 0..40 {
        w.tick_ire(1.0);
    }
    let h = w.heart_at(sx, sz).unwrap();
    assert_eq!(h.stage, 2, "tended country heals (strain {})", h.strain);
}

#[test]
fn a_heart_held_in_grievance_dies_and_stays_dead() {
    let (mut w, _key, (sx, sz)) = world_with_heart(43, "hearts-die");
    for _ in 0..40 {
        for _ in 0..30 {
            w.add_ire_at(sx, sz, 4.0);
        }
        w.tick_ire(1.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 0, "the country is dead");
    // And waiting never brings it back — that road is the long walk.
    for _ in 0..60 {
        w.tick_ire(1.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 0, "still dead");
    assert!(!w.heart_alive_at(sx, sz));
}

#[test]
fn the_axe_works_and_that_is_the_trap() {
    let (mut w, _key, (sx, sz)) = world_with_heart(44, "hearts-axe");
    let h = w.heart_at(sx, sz).unwrap();
    assert!(h.alive());
    // One cut, unwarned, and it takes.
    w.break_block((h.pos.0, h.pos.1, h.pos.2), None, false, false);
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 0, "the spirit is gone");
    // The whole site goes with it — a half-cut heart is not a thing.
    let form = crate::world::heart_form(w.generator.biome(sx, sz));
    for dy in 1..crate::world::heart_height(form) {
        let b = w.get_block(h.pos.0, h.pos.1 + dy, h.pos.2);
        let name = w.reg.block(b).name.clone();
        assert!(
            name == crate::world::heart_block_name(form, 0) || name == "base:air",
            "the husk stands or nothing does, not {name}"
        );
    }
}

#[test]
fn the_wardens_fall_silent_where_the_heart_is_dead() {
    let reg = base_reg();
    let (mut w, key, (sx, sz)) = world_with_heart(45, "hearts-silence");
    // Angry country, deep night: the wild should be hunting.
    for _ in 0..40 {
        w.add_ire_at(sx, sz, 3.0);
    }
    w.ire = 95.0;
    let here = glam::Vec3::new(sx as f32, 80.0, sz as f32);
    let spawn = glam::Vec3::new(sx as f32 + 400.0, 80.0, sz as f32);
    for dx in -3..=3 {
        for dz in -3..=3 {
            let cp = ChunkPos::of_world(sx + dx * 16, sz + dz * 16);
            w.ensure_chunk(cp);
        }
    }
    let mut rng = 5u32;
    let mut alive_spawns = 0;
    for _ in 0..600 {
        w.tick_hostile_spawns(here, spawn, 0.0, 5.0, &mut rng);
        alive_spawns = w
            .mobs()
            .iter()
            .filter(|m| reg.animals[m.species].hostile)
            .count();
        if alive_spawns > 0 {
            break;
        }
    }
    assert!(alive_spawns > 0, "a living angry country hunts");
    // Kill the heart. The raids stop — forever.
    w.replace_mobs(Vec::new());
    w.set_heart_stage(key, 0);
    let mut rng = 5u32;
    for _ in 0..600 {
        w.tick_hostile_spawns(here, spawn, 0.0, 5.0, &mut rng);
    }
    let after = w
        .mobs()
        .iter()
        .filter(|m| reg.animals[m.species].hostile)
        .count();
    assert_eq!(after, 0, "nothing comes, and nothing ever will again");
}

#[test]
fn a_dead_country_does_not_answer_the_offering_stone() {
    let (mut w, key, (sx, sz)) = world_with_heart(46, "hearts-offering");
    let reg = w.reg.clone();
    let stone = reg.block_id("base:offering_stone").unwrap();
    let sy = w.surface_height(sx + 3, sz + 3) + 1;
    w.set_block(sx + 3, sy, sz + 3, stone);
    let diamond = reg.item_id("base:diamond").unwrap();
    let put = |w: &mut World| {
        w.insert_block_entity(
            (sx + 3, sy, sz + 3),
            crate::world::BlockEntity::Offering(crate::world::OfferingState {
                slots: [
                    Some(crate::inventory::ItemStack::new(&reg, diamond, 1)),
                    None,
                    None,
                ],
            }),
        );
    };
    // A living country takes what is offered.
    put(&mut w);
    assert!(w.accept_offerings() > 0.0, "the wild accepts");
    // A dead one leaves it lying there at dawn.
    w.set_heart_stage(key, 0);
    put(&mut w);
    assert_eq!(w.accept_offerings(), 0.0, "unreceived, not refused");
    let still_there = matches!(
        w.block_entity(&(sx + 3, sy, sz + 3)),
        Some(crate::world::BlockEntity::Offering(o)) if o.slots[0].is_some()
    );
    assert!(still_there, "the diamond is still on the stone");
}

#[test]
fn a_dead_country_gives_nothing_but_your_own_farm_still_works() {
    let (mut w, key, (sx, sz)) = world_with_heart(47, "hearts-sterile");
    let reg = w.reg.clone();
    let grass = b(&reg, "base:grass");
    let dirt = b(&reg, "base:dirt");
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    let log = b(&reg, "base:log");
    let sy = 150;
    // A pad in this country: bare dirt to heal, grass to seed the
    // tide (with a parent log), and a tended field of the player's.
    for x in sx - 8..sx + 8 {
        for z in sz - 8..sz + 8 {
            w.set_block(x, sy, z, grass);
            for dy in 1..6 {
                if w.get_block(x, sy + dy, z) != AIR {
                    w.set_block(x, sy + dy, z, AIR);
                }
            }
        }
    }
    for dy in 1..=3 {
        w.set_block(sx, sy + dy, sz, log);
    }
    for x in sx - 6..sx - 2 {
        for z in sz - 6..sz - 2 {
            w.set_block(x, sy, z, dirt);
        }
    }
    for x in sx + 2..sx + 6 {
        for z in sz + 2..sz + 6 {
            w.set_block_meta(x, sy, z, farm, crate::world::soil::soil_meta(30, 0));
            w.set_block(x, sy + 1, z, seed0);
        }
    }
    // Kill the country's spirit.
    w.set_heart_stage(key, 0);
    let mut rng = 3u32;
    for _ in 0..9000 {
        w.random_tick(&mut rng);
    }
    // Nothing the WILD gives: no tide, no healing.
    let saplings = (sx - 8..sx + 8)
        .flat_map(|x| (sz - 8..sz + 8).map(move |z| (x, z)))
        .filter(|&(x, z)| reg.block(w.get_block(x, sy + 1, z)).sapling.is_some())
        .count();
    assert_eq!(saplings, 0, "the tide does not run in dead country");
    let healed = (sx - 6..sx - 2)
        .flat_map(|x| (sz - 6..sz - 2).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, sy, z) == grass)
        .count();
    assert_eq!(healed, 0, "and the scars stay open");
    // But the player's own field grows: farms work, wilderness does not.
    let grown = (sx + 2..sx + 6)
        .flat_map(|x| (sz + 2..sz + 6).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, sy + 1, z) != seed0)
        .count();
    assert!(grown > 0, "what you feed yourself still grows ({grown})");
}

#[test]
fn the_bloom_is_finite_if_you_never_give_back() {
    let reg = base_reg();
    let mut w = test_world_with("bloom-debt", reg.clone());
    // Farm the storm: bank bloom over and over, tending nothing.
    let mut total = 0.0;
    for _ in 0..40 {
        let before = w.bloom_at(500, 500);
        w.add_bloom(500, 500, 3.0);
        total += (w.bloom_at(500, 500) - before).max(0.0);
        // Spend it, the way days do.
        for _ in 0..12 {
            w.tick_ire(1.0);
        }
    }
    assert!(
        total < crate::world::BLOOM_EXHAUSTION * 1.5,
        "the ground's gift runs out ({total:.1})"
    );
    assert_eq!(
        w.bloom_at(500, 500),
        0.0,
        "and a cell farmed dry blooms no more"
    );
    // Tend it, and the willingness comes back.
    for _ in 0..40 {
        w.plant_ire_at(500, 500, 1.0);
    }
    w.add_bloom(500, 500, 3.0);
    assert!(w.bloom_at(500, 500) > 0.0, "tended ground blooms again");
}

#[test]
fn the_dead_countrys_wardens_keep_walking() {
    let reg = base_reg();
    let (mut w, key, (sx, sz)) = world_with_heart(48, "hearts-masterless");
    // A warden abroad when the heart dies.
    let wi = reg
        .animals
        .iter()
        .position(|a| a.hostile && a.name.contains("thornling"))
        .expect("a warden");
    let mut m = crate::mobs::Mob::new(
        wi,
        glam::Vec3::new(
            sx as f32 + 4.0,
            w.surface_height(sx + 4, sz) as f32 + 1.0,
            sz as f32,
        ),
        0.0,
    );
    m.health = reg.animals[wi].health;
    w.spawn_mob(m);
    w.set_heart_stage(key, 0);
    assert!(
        w.mobs().iter().any(|m| m.masterless),
        "the heart's death orphans what it sent"
    );
    // Daylight does not dissolve them: nothing is left to recall them.
    let here = glam::Vec3::new(sx as f32, 80.0, sz as f32);
    let ctx = crate::server::PlayerCtx {
        id: 0,
        pos: here,
        spawn: here,
        attackable: true,
        aggro_mod: 0.0,
    };
    let mut rng = 9u32;
    for _ in 0..200 {
        w.tick_mobs(&[ctx], 1.0, 0.05, &mut rng);
    }
    assert!(
        w.mobs().iter().any(|m| m.masterless),
        "still walking at noon"
    );
}
