//! The hearts of the land: every country has one, and you can find it.

use super::*;
use crate::world::{HEART_CUTTING_DAYS, ROOT_DAYS, SEASON_DAYS};
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
    assert!(here.contains("here"), "{here}");
    assert!(here.contains("well"), "{here}");
    // A day's walk off: direction and distance, in plain words.
    let away = w.heart_report(sx - 300, sz);
    assert!(away.contains("blocks east"), "{away}");
    assert!(
        away.contains("2") || away.contains("3"),
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
        save_world(&mut w);
    }
    let mut w = World::load_or_create(dir, reg.clone()).unwrap();
    w.ensure_chunk(ChunkPos::of_world(sx, sz));
    let h = w.heart_at(sx, sz).expect("the ledger came back");
    assert_eq!(h.stage, 1, "a failing heart is still failing");
    assert!((h.strain - 6.5).abs() < 0.01, "its grievance persists");
    assert_eq!(w.generator.province(sx, sz).key, key);
}

/// A world with a heart loaded, and the key of its country. Skips the
/// badlands: their spirit went out before the world began, so a
/// province there is a scar and not a country with a heart to strain.
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
                && g.province(x, z).biome != Biome::Badlands
        })
        .expect("dry living country");
    w.ensure_chunk(ChunkPos::of_world(sx, sz));
    let key = w.generator.province(sx, sz).key;
    let h = w.heart_at(sx, sz).expect("the country has a heart");
    assert_eq!(h.stage, 2, "and it is alive");
    (w, key, (sx, sz))
}

#[test]
fn every_country_has_its_own_heart_and_its_own_seed() {
    use crate::world::{heart_block_name, heart_form, seed_nature, seed_of_form};
    use std::collections::HashSet;
    let reg = base_reg();
    // Not a wildcard match: naming all twelve means adding a thirteenth
    // country fails to compile here until it has a heart of its own.
    let all = [
        Biome::Forest,
        Biome::Plains,
        Biome::Desert,
        Biome::Jungle,
        Biome::Scrubland,
        Biome::Taiga,
        Biome::Arctic,
        Biome::Mountains,
        Biome::Swamp,
        Biome::Savanna,
        Biome::Tundra,
        Biome::Badlands,
    ];
    let (mut forms, mut seeds) = (HashSet::new(), HashSet::new());
    for b in all {
        let form = heart_form(b);
        assert!(
            forms.insert(form),
            "{b:?} shares its heart with another country"
        );
        for stage in 0..=2u8 {
            let name = heart_block_name(form, stage);
            assert!(reg.block_id(&name).is_some(), "missing block {name}");
        }
        let seed = seed_of_form(form);
        assert!(
            seeds.insert(seed),
            "{b:?} shares its seed with another country"
        );
        assert!(reg.item_id(seed).is_some(), "missing item {seed}");
        // The round trip that makes terraforming legible: the seed a
        // country gives is the seed that wakes that country.
        assert_eq!(
            seed_nature(seed),
            Some(b),
            "{b:?}'s own seed should carry {b:?}"
        );
    }
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
    // Counted rather than hardcoded, so the claim under test is the
    // design one ("a spirit takes SEASONS") and not a day number that
    // silently stops meaning a season when the calendar is retuned.
    let mut days = 0;
    while w.heart_at(sx, sz).unwrap().stage == 2 && days < SEASON_DAYS * 3 {
        for _ in 0..30 {
            w.add_ire_at(sx, sz, 4.0);
        }
        w.tick_ire(1.0);
        days += 1;
    }
    assert_eq!(
        w.heart_at(sx, sz).unwrap().stage,
        1,
        "the heart is failing after {days} days"
    );
    assert!(
        days >= SEASON_DAYS,
        "and it took a season of unbroken grievance, not a night ({days} days)"
    );
    assert!(w.heart_report(sx, sz).contains("FAILING"));
    // Stop taking, and it comes back: the wild forgives the patient.
    let mut days = 0;
    while w.heart_at(sx, sz).unwrap().stage == 1 && days < SEASON_DAYS * 3 {
        w.tick_ire(1.0);
        days += 1;
    }
    let h = w.heart_at(sx, sz).unwrap();
    assert_eq!(h.stage, 2, "tended country heals (strain {})", h.strain);
}

#[test]
fn a_heart_held_in_grievance_dies_and_stays_dead() {
    let (mut w, _key, (sx, sz)) = world_with_heart(43, "hearts-die");
    let mut days = 0;
    while w.heart_at(sx, sz).unwrap().stage > 0 && days < SEASON_DAYS * 8 {
        for _ in 0..30 {
            w.add_ire_at(sx, sz, 4.0);
        }
        w.tick_ire(1.0);
        days += 1;
    }
    assert_eq!(
        w.heart_at(sx, sz).unwrap().stage,
        0,
        "the country is dead after {days} days"
    );
    assert!(
        days >= SEASON_DAYS * 2,
        "killing a spirit outright takes more than two seasons ({days} days)"
    );
    // And waiting never brings it back — that road is the long walk.
    for _ in 0..SEASON_DAYS * 4 {
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

#[test]
fn a_seed_will_not_take_in_dead_dirt_but_will_in_ground_made_ready() {
    let (mut w, key, (sx, sz)) = world_with_heart(49, "hearts-root");
    let reg = w.reg.clone();
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;
    // Dead dirt refuses the seed, and says why.
    let refusal = w.plant_heart_seed(hp.0, hp.1, hp.2).expect("refused");
    assert!(refusal.contains("Not ready"), "{refusal}");
    // Raise the soil by hand: the whole nutrient cycle, spent as a key.
    let farm = b(&reg, "base:farmland");
    let r = w.root_radius_at(hp.0, hp.2);
    // The disc reaches past the monument now, so it spans chunks a
    // fixture that loads one would leave as void.
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos::of_world(hp.0 + cx * 16, hp.2 + cz * 16));
        }
    }
    for dx in -r..=r {
        for dz in -r..=r {
            let (cx, cz) = (hp.0 + dx, hp.2 + dz);
            let y = w.surface_height(cx, cz);
            w.set_block_meta(cx, y, cz, farm, crate::world::soil::soil_meta(40, 0));
        }
    }
    let (ready, total) = w.root_ground_ready(hp.0, hp.1, hp.2);
    assert!(
        ready * 2 > total,
        "the ground is living now ({ready}/{total})"
    );
    assert!(w.plant_heart_seed(hp.0, hp.1, hp.2).is_none(), "it takes");
    // A rooting is a season's work, not a moment's.
    for _ in 0..(ROOT_DAYS as u32 / 2) {
        w.tick_ire(1.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 0, "still rooting");
    for _ in 0..(ROOT_DAYS as u32 / 2 + 2) {
        w.tick_ire(1.0);
    }
    let h = w.heart_at(sx, sz).unwrap();
    assert_eq!(h.stage, 2, "the country wakes");
    assert_eq!(h.rooting, 0.0);
    assert!(w.heart_alive_at(sx, sz), "and it gives again");
    // The site wears its living form once more.
    let form = crate::world::heart_form(w.generator.biome(sx, sz));
    let at = w.get_block(h.pos.0, h.pos.1, h.pos.2);
    assert_eq!(reg.block(at).name, crate::world::heart_block_name(form, 2));
}

#[test]
fn a_rooting_abandoned_is_a_rooting_lost() {
    let (mut w, key, (sx, sz)) = world_with_heart(50, "hearts-abandon");
    let reg = w.reg.clone();
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;
    let farm = b(&reg, "base:farmland");
    let r = w.root_radius_at(hp.0, hp.2);
    // The disc reaches past the monument now, so it spans chunks a
    // fixture that loads one would leave as void.
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos::of_world(hp.0 + cx * 16, hp.2 + cz * 16));
        }
    }
    for dx in -r..=r {
        for dz in -r..=r {
            let (cx, cz) = (hp.0 + dx, hp.2 + dz);
            let y = w.surface_height(cx, cz);
            w.set_block_meta(cx, y, cz, farm, crate::world::soil::soil_meta(40, 0));
        }
    }
    assert!(w.plant_heart_seed(hp.0, hp.1, hp.2).is_none());
    for _ in 0..4 {
        w.tick_ire(1.0);
    }
    assert!(w.heart_at(sx, sz).unwrap().rooting > 0.0, "taking");
    // Let the ground go back to nothing and the seed goes with it.
    let dirt = b(&reg, "base:dirt");
    for dx in -r..=r {
        for dz in -r..=r {
            let (cx, cz) = (hp.0 + dx, hp.2 + dz);
            let y = w.surface_height(cx, cz);
            w.set_block(cx, y, cz, dirt);
        }
    }
    w.tick_ire(1.0);
    assert_eq!(w.heart_at(sx, sz).unwrap().rooting, 0.0, "the seed is lost");
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 0, "the country waits");
}

#[test]
fn a_seed_only_goes_where_a_heart_died() {
    let (mut w, key, (sx, sz)) = world_with_heart(51, "hearts-wrongplace");
    // A living country refuses it outright.
    let hp = w.heart_at(sx, sz).unwrap().pos;
    let r = w.plant_heart_seed(hp.0, hp.1, hp.2).expect("refused");
    assert!(r.contains("still has a spirit"), "{r}");
    // And so does open ground far from the old site.
    w.set_heart_stage(key, 0);
    let r = w.plant_heart_seed(hp.0 + 40, hp.1, hp.2).expect("refused");
    assert!(r.contains("where the old heart stood"), "{r}");
}

#[test]
fn a_stranger_heart_remakes_the_country_it_wakes_in() {
    let (mut w, key, (sx, sz)) = world_with_heart(52, "hearts-graft");
    let reg = w.reg.clone();
    let native = w.generator.province(sx, sz).biome;
    // Pick a seed from a DIFFERENT family than this country's.
    let native_form = crate::world::heart_form(native);
    let want = (1..=12u8)
        .filter_map(crate::worldgen::Biome::from_index)
        .find(|b| crate::world::heart_form(*b) != native_form)
        .expect("a stranger's kind exists");
    let seed = crate::world::seed_of_form(crate::world::heart_form(want));
    assert_eq!(crate::world::seed_nature(seed), Some(want));
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;
    let farm = b(&reg, "base:farmland");
    let r = w.root_radius_at(hp.0, hp.2);
    // The disc reaches past the monument now, so it spans chunks a
    // fixture that loads one would leave as void.
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos::of_world(hp.0 + cx * 16, hp.2 + cz * 16));
        }
    }
    for dx in -r..=r {
        for dz in -r..=r {
            let (cx, cz) = (hp.0 + dx, hp.2 + dz);
            let y = w.surface_height(cx, cz);
            w.set_block_meta(cx, y, cz, farm, crate::world::soil::soil_meta(40, 0));
        }
    }
    assert!(
        w.plant_heart_seed_from(hp.0, hp.1, hp.2, Some(want))
            .is_none(),
        "a stranger's seed still takes in ready ground"
    );
    for _ in 0..(ROOT_DAYS as u32 + 2) {
        w.tick_ire(1.0);
    }
    assert_eq!(w.heart_at(sx, sz).unwrap().stage, 2, "it wakes");
    // At first the country is still itself: the drift takes seasons.
    assert_eq!(w.country_biome(sx, sz), native, "not overnight");
    // Half the drift is what tips the country over, and the drift is
    // two seasons end to end.
    for _ in 0..SEASON_DAYS {
        w.tick_ire(1.0);
    }
    assert_eq!(
        w.country_biome(sx, sz),
        want,
        "the country becomes what its new heart is"
    );
    // The map never changed — the COUNTRY did.
    assert_eq!(w.generator.province(sx, sz).biome, native);
}

#[test]
fn its_own_kind_reawakens_rather_than_replaces() {
    let (mut w, key, (sx, sz)) = world_with_heart(53, "hearts-reawaken");
    let reg = w.reg.clone();
    let native = w.generator.province(sx, sz).biome;
    let own =
        crate::world::seed_nature(crate::world::seed_of_form(crate::world::heart_form(native)))
            .unwrap();
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;
    let farm = b(&reg, "base:farmland");
    let r = w.root_radius_at(hp.0, hp.2);
    // The disc reaches past the monument now, so it spans chunks a
    // fixture that loads one would leave as void.
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos::of_world(hp.0 + cx * 16, hp.2 + cz * 16));
        }
    }
    for dx in -r..=r {
        for dz in -r..=r {
            let (cx, cz) = (hp.0 + dx, hp.2 + dz);
            let y = w.surface_height(cx, cz);
            w.set_block_meta(cx, y, cz, farm, crate::world::soil::soil_meta(40, 0));
        }
    }
    let before = w.regional_ire_at(sx, sz);
    assert!(
        w.plant_heart_seed_from(hp.0, hp.1, hp.2, Some(own))
            .is_none(),
        "its own kind takes"
    );
    for _ in 0..(ROOT_DAYS as u32 + 2) {
        w.tick_ire(1.0);
    }
    let h = w.heart_at(sx, sz).unwrap();
    assert_eq!(h.stage, 2);
    assert!(h.graft.is_none(), "no graft: this is a reawakening");
    // It remembers who did it: the ground is blessed for good.
    assert!(
        w.regional_ire_at(sx, sz) < before,
        "the valley forgives you specifically"
    );
    // And it stays itself, forever.
    for _ in 0..40 {
        w.tick_ire(1.0);
    }
    assert_eq!(w.country_biome(sx, sz), native);
}

#[test]
fn the_seed_names_the_game_uses_are_real_items() {
    // The verb in actions.rs looks its items up BY NAME at runtime, so
    // a stale name compiles clean and silently disables the whole
    // feature. (It did, once.) Pin the names to the registry.
    let reg = base_reg();
    for biome in (1..=12u8).filter_map(crate::worldgen::Biome::from_index) {
        let form = crate::world::heart_form(biome);
        let seed = crate::world::seed_of_form(form);
        let id = reg
            .item_id(seed)
            .unwrap_or_else(|| panic!("{form} gives {seed}, which is not an item"));
        // And the name round-trips: what a heart gives is a seed the
        // planting verb recognises as carrying a nature.
        let nature = crate::world::seed_nature(&reg.item(id).name)
            .unwrap_or_else(|| panic!("{seed} carries no nature"));
        assert_eq!(
            crate::world::heart_form(nature),
            form,
            "{seed} should wake {form}'s kind"
        );
    }
    // And nothing else is mistaken for a seed.
    assert_eq!(crate::world::seed_nature("base:stick"), None);
}

/// Kill `n` countries in a world, returning how many hearts it knows.
fn kill_countries(w: &mut World, n: usize) -> usize {
    let keys: Vec<(i32, i32)> = w.hearts.keys().copied().take(n).collect();
    for k in keys {
        w.set_heart_stage(k, 0);
    }
    w.hearts.len()
}

#[test]
fn enough_dead_countries_stop_the_year_and_relighting_starts_it() {
    let reg = base_reg();
    let mut w = World::new(60, tmp_dir("longwinter"), reg.clone());
    // Load the chunks that actually HOLD sites: countries are ~900
    // blocks apart, so a tidy grid of chunks misses every one.
    let sites: Vec<(i32, i32)> = (-4..4)
        .flat_map(|kx| (-4..4).map(move |kz| (kx, kz)))
        .map(|(kx, kz)| w.generator.province_center(kx, kz))
        .collect();
    for (sx, sz) in sites {
        w.ensure_chunk(ChunkPos::of_world(sx, sz));
    }
    let known = w.hearts.len();
    assert!(known >= 4, "the world knows some countries ({known})");
    assert!(!w.long_winter, "the year turns to begin with");
    let summer = crate::world::SEASON_DAYS;
    w.day = summer; // high summer
    assert_eq!(w.season(), 1, "summer");
    // A couple of deaths is a tragedy, not a winter.
    kill_countries(&mut w, 2);
    w.tick_ire(0.1);
    assert!(!w.long_winter, "two dead countries do not stop the year");
    // Enough of them, and the year stops.
    let keys: Vec<(i32, i32)> = w.hearts.keys().copied().collect();
    for k in keys.iter().take(known.div_ceil(2) + 1) {
        w.set_heart_stage(*k, 0);
    }
    w.tick_ire(0.1);
    assert!(w.long_winter, "the year has stopped");
    assert_eq!(w.season(), 3, "and it is winter in high summer");
    // Everything winter already means arrives for free.
    let (dead, total) = w.dead_countries();
    assert!(dead * 2 >= total, "{dead} of {total} countries dead");
    // Relight enough of them and spring comes back.
    for k in keys.iter() {
        if w.hearts.get(k).is_some_and(|h| h.stage == 0) {
            w.set_heart_stage(*k, 2);
        }
        w.tick_ire(0.1);
        if !w.long_winter {
            break;
        }
    }
    assert!(!w.long_winter, "the year turns again");
    assert_eq!(w.season(), 1, "and summer was waiting");
}

#[test]
fn the_long_winter_is_survivable_by_the_tools_already_shipped() {
    // A glasshouse still grows through a stopped year: the greenhouse
    // rule, salt, smoke and the crock are what the takers lacked.
    let reg = base_reg();
    let mut w = test_world_with("longwinter-farm", reg.clone());
    w.long_winter = true;
    assert_eq!(w.season(), 3);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    let glass = b(&reg, "base:glass");
    let sy = 140;
    for x in 4..10 {
        for z in 4..10 {
            w.set_block_meta(x, sy, z, farm, crate::world::soil::soil_meta(50, 0));
            w.set_block(x, sy + 1, z, seed0);
            w.set_block(x, sy + 4, z, glass);
            for dy in 2..4 {
                if w.get_block(x, sy + dy, z) != AIR {
                    w.set_block(x, sy + dy, z, AIR);
                }
            }
        }
    }
    let mut rng = 7u32;
    for _ in 0..9000 {
        w.random_tick(&mut rng);
    }
    let grown = (4..10)
        .flat_map(|x| (4..10).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, sy + 1, z) != seed0)
        .count();
    assert!(grown > 0, "under glass, the year does not matter ({grown})");
}

#[test]
fn the_long_winter_survives_the_save() {
    let reg = base_reg();
    let dir = tmp_dir("longwinter-save");
    {
        let mut w = World::new(61, dir.clone(), reg.clone());
        w.ensure_chunk(ChunkPos { x: 0, z: 0 });
        w.long_winter = true;
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    assert!(w.long_winter, "a stopped year is still stopped");
    assert_eq!(w.season(), 3);
}

#[test]
fn the_tablets_confess_what_the_takers_did() {
    // The lore delivery vehicle already existed; the sequence is new.
    // Pin it, because it is the answer to the whole arc.
    let text = std::fs::read_to_string("src/game/interaction.rs").unwrap();
    for line in [
        "We farmed their rage",
        "took axes to the bole",
        "No wardens tonight",
        "gives us nothing",
        "will not take the offering",
    ] {
        assert!(text.contains(line), "the confession is missing: {line}");
    }
}

/// A living heart parted with a seed on every right-click, forever, at
/// +1 ire a time — a dispenser you could stand at, and a way to farm
/// ire besides. It gives one, then has nothing to spare for half a
/// season.
#[test]
fn a_living_heart_gives_one_cutting_then_needs_a_season() {
    let (mut w, _key, (sx, sz)) = world_with_heart(42, "hearts-cutting");
    assert!(w.take_heart_cutting(sx, sz), "a living heart gives");
    assert!(
        !w.take_heart_cutting(sx, sz),
        "and does not give again on the next click"
    );
    // Clicking a spent heart costs nothing: the refusal is free, so it
    // cannot be used to pump ire either.
    let before = w.regional_ire_at(sx, sz);
    for _ in 0..20 {
        assert!(!w.take_heart_cutting(sx, sz));
    }
    assert_eq!(w.regional_ire_at(sx, sz), before, "asking is not taking");

    // Almost long enough is not long enough...
    for _ in 0..(HEART_CUTTING_DAYS as u32 - 1) {
        w.tick_ire(1.0);
    }
    assert!(!w.take_heart_cutting(sx, sz), "a day short is still short");
    w.tick_ire(1.0);
    assert!(w.take_heart_cutting(sx, sz), "half a season on, it gives");
}

/// Only a heart that is actually well has anything to give.
#[test]
fn a_sickening_or_dead_heart_gives_no_cutting() {
    let (mut w, key, (sx, sz)) = world_with_heart(42, "hearts-cutting-sick");
    w.set_heart_stage(key, 1);
    assert!(!w.take_heart_cutting(sx, sz), "a dying heart gives nothing");
    w.set_heart_stage(key, 0);
    assert!(!w.take_heart_cutting(sx, sz), "a dead one gives nothing");
    w.set_heart_stage(key, 2);
    assert!(w.take_heart_cutting(sx, sz), "a well one does");
}

/// The timer is worth nothing if it resets when you quit. The hearts
/// file grew a field, so it also has to keep reading saves written
/// before it did — the old layout is headerless and 34 bytes a record,
/// which a 19-heart save makes ambiguous with the new 38.
#[test]
fn the_cutting_timer_survives_a_save_and_old_saves_still_load() {
    let reg = base_reg();
    let (mut w, key, (sx, sz)) = world_with_heart(42, "hearts-cutting-save");
    let dir = w.save_dir().to_path_buf();
    assert!(w.take_heart_cutting(sx, sz));
    let pos = w.heart_at(sx, sz).unwrap().pos;
    save_world(&mut w);
    drop(w);

    let w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let loaded = w.heart_at(pos.0, pos.2).expect("the heart came back");
    assert!(
        (loaded.regrow - crate::world::HEART_CUTTING_DAYS).abs() < 0.01,
        "the rest survived the save (regrow {})",
        loaded.regrow
    );
    drop(w);

    // A pre-timer save: no header, 34 bytes a record. It has to load,
    // with its hearts simply ready to give.
    let mut old = Vec::new();
    old.extend_from_slice(&key.0.to_le_bytes());
    old.extend_from_slice(&key.1.to_le_bytes());
    old.extend_from_slice(&pos.0.to_le_bytes());
    old.extend_from_slice(&pos.1.to_le_bytes());
    old.extend_from_slice(&pos.2.to_le_bytes());
    old.push(2);
    old.extend_from_slice(&3.5f32.to_le_bytes()); // strain
    old.extend_from_slice(&0f32.to_le_bytes()); // rooting
    old.push(0); // graft
    old.extend_from_slice(&0f32.to_le_bytes()); // drift
    assert_eq!(old.len(), 34, "the shape of the old record");
    std::fs::write(dir.join("hearts"), &old).unwrap();
    let w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    let loaded = w.heart_at(pos.0, pos.2).expect("an old heart still loads");
    assert!((loaded.strain - 3.5).abs() < 0.01, "its grievance survived");
    assert_eq!(loaded.regrow, 0.0, "and it is ready to give");
}

/// The generator caches a block id per country. When the three
/// archetype hearts became twelve, that cache went on resolving three
/// names that no longer existed — so it laid the placeholder block
/// where every spirit should have stood, nothing registered a heart,
/// and twenty tests failed at once with no mention of worldgen. Pin
/// the two sides together.
#[test]
fn the_generator_lays_the_block_each_country_actually_wears() {
    let reg = base_reg();
    let g = crate::worldgen::Generator::new(7, &reg);
    for biome in (1..=12u8).filter_map(Biome::from_index) {
        let form = crate::world::heart_form(biome);
        let want = reg
            .block_id(form)
            .unwrap_or_else(|| panic!("{biome:?} wears {form}, which is not a block"));
        assert_ne!(
            want, reg.unknown_block,
            "{biome:?}'s heart is the placeholder"
        );
        assert_eq!(g.heart_block(biome), want, "{biome:?}");
    }
}

/// The badlands are the receipt. Their spirit went out long before
/// anyone alive walked there, which is why nothing grows and why the
/// takers' cities stand intact in ground that stopped feeding them —
/// the ruins already biased there; now the cause exists.
#[test]
fn the_badlands_are_born_dead_and_can_be_woken() {
    let reg = base_reg();
    let mut w = World::new(31, tmp_dir("hearts-badlands"), reg.clone());
    let g = &w.generator;
    let scar = (0..60)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .map(|(kx, kz)| g.province_center(kx, kz))
        .find(|&(x, z)| {
            g.province(x, z).biome == Biome::Badlands
                && g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
        })
        .expect("some badlands in this world");
    w.ensure_chunk(ChunkPos::of_world(scar.0, scar.1));
    let h = w.heart_at(scar.0, scar.1).expect("the scar has a site");
    assert_eq!(h.stage, 0, "the badlands were not always badlands");
    assert!(!w.heart_alive_at(scar.0, scar.1));
    assert!(w.is_ancient_scar(scar.0, scar.1));

    // The site wears the husk, not a living heart.
    let form = crate::world::heart_form(Biome::Badlands);
    let at = w.get_block(h.pos.0, h.pos.1, h.pos.2);
    assert_eq!(
        reg.block(at).name,
        crate::world::heart_block_name(form, 0),
        "a dry spring stands there"
    );

    // And it is restorable like any other dead country — that is the
    // point of it. A scar you can walk to is a campaign you can start.
    w.set_heart_stage(w.generator.province(scar.0, scar.1).key, 2);
    assert!(w.heart_alive_at(scar.0, scar.1));
}

/// Ancient scars must not count toward the Long Winter. Walking
/// through three badlands provinces early would otherwise stop the
/// world's year over history the player never touched.
#[test]
fn ancient_scars_do_not_stop_the_world_but_countries_you_kill_do() {
    let reg = base_reg();
    let mut w = World::new(31, tmp_dir("hearts-scar-winter"), reg.clone());
    // Load sites until enough of each kind has actually REGISTERED. A
    // province center only raises a heart where the generated ground
    // is dry and has headroom, which surface_estimate only predicts.
    let centers: Vec<(i32, i32)> = (0..70)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .collect();
    let (mut scars, mut living) = (Vec::new(), Vec::new());
    // The ring walk visits (0,0) four times over, and neighbouring
    // keys can share a site; a country counted twice is not two.
    let mut seen = std::collections::HashSet::new();
    for (kx, kz) in centers {
        if scars.len() >= 4 && living.len() >= 2 {
            break;
        }
        let (x, z) = w.generator.province_center(kx, kz);
        if !seen.insert((x, z)) {
            continue;
        }
        let scar = w.is_ancient_scar(x, z);
        if (scar && scars.len() >= 4) || (!scar && living.len() >= 2) {
            continue;
        }
        w.ensure_chunk(ChunkPos::of_world(x, z));
        if w.heart_at(x, z).is_none() {
            continue; // drowned or cramped: this country has no site
        }
        if scar { &mut scars } else { &mut living }.push((x, z));
    }
    assert_eq!(scars.len(), 4, "need four scars to prove the point");
    assert_eq!(living.len(), 2, "and two countries with spirits in them");

    let (dead, known) = w.dead_countries();
    assert_eq!(dead, 0, "four dead badlands count as none");
    assert_eq!(known, 2, "only the living countries are in the pool");
    w.tick_ire(0.01);
    assert!(!w.long_winter, "ancient history does not stop the year");

    // Kill the ones that were alive, and the tally moves.
    for &(x, z) in &living {
        w.set_heart_stage(w.generator.province(x, z).key, 0);
    }
    assert_eq!(w.dead_countries(), (2, 2), "your own killings do count");
}

/// A country's heart is a landmark you can see, not a stub you have to
/// grid-search for. Doll went looking for one and had to sweep an area
/// by hand — this is the shape of the fix.
#[test]
fn every_country_raises_an_edifice_over_its_heart() {
    use crate::edifice::{Materials, block_at, edifice_of};
    let reg = base_reg();
    let m = Materials {
        shell: b(&reg, "base:stone"),
        crown: b(&reg, "base:planks"),
    };
    for biome in (1..=12u8).filter_map(Biome::from_index) {
        let e = edifice_of(biome);
        assert!(e.rise >= 11, "{biome:?} raises only {} blocks", e.rise);
        assert!(e.reach >= 8, "{biome:?} is only {} wide", e.reach);
        // Real materials, not the placeholder.
        for name in [e.shell, e.crown] {
            assert!(
                reg.block_id(name).is_some_and(|b| b != reg.unknown_block),
                "{biome:?} builds with {name}, which is not a block"
            );
        }
        // It has mass...
        let solid = (-e.reach..=e.reach)
            .flat_map(|dx| (0..=e.rise).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| block_at(&e, &m, dx, dy, 0).is_some())
            .count();
        assert!(solid > 40, "{biome:?} is barely there ({solid} blocks)");
        // ...and it is a wrapper, never a lock. The axe has to reach
        // the heart, and the heart's own column is the spirit's.
        for dy in -4..=e.rise {
            assert!(
                block_at(&e, &m, 0, dy, 0).is_none(),
                "{biome:?} builds in the site column at +{dy}"
            );
        }
    }
}

/// The ledger finds its heart even with a monument standing over it.
/// Before this it took `surface_height` and demanded the block THERE be
/// a heart, so anything overhead — an edifice, or a roof a player put
/// up — meant the country registered no heart at all. Not a dead one:
/// none. Wardens kept spawning and offerings kept being accepted while
/// the whole arc quietly did not happen there.
#[test]
fn a_heart_registers_under_whatever_stands_over_it() {
    let (mut w, _key, (sx, sz)) = world_with_heart(42, "hearts-buried");
    let reg = w.reg.clone();
    let before = w.heart_at(sx, sz).expect("found in the open");
    // Roof it over, well clear of the site, and reload the chunk.
    let stone = b(&reg, "base:stone");
    let top = w.surface_height(sx, sz);
    for dx in -2..=2 {
        for dz in -2..=2 {
            w.set_block(sx + dx, top + 12, sz + dz, stone);
        }
    }
    let pos = ChunkPos::of_world(sx, sz);
    save_world(&mut w);
    w.unload_chunk(pos);
    w.ensure_chunk(pos);
    let after = w.heart_at(sx, sz).expect("still found under a roof");
    assert_eq!(after.pos, before.pos, "and keyed to the same site");
}

/// The walk between provinces is 900 blocks and you can see 112 of it.
/// Flowers thicken toward a heart so the ground itself tells you which
/// way to go — no compass, no marker, no map.
#[test]
fn the_ground_thickens_toward_a_heart() {
    let reg = base_reg();
    let g = crate::worldgen::Generator::new(9, &reg);
    let (sx, sz) = g.province_center(0, 0);
    let at = |d: i32| g.heart_nearness(sx + d, sz);
    assert!(at(0) > 0.95, "on the site it is unmistakable");
    assert!(at(0) > at(120), "and it falls off with distance");
    assert!(at(120) > at(260));
    assert!(at(260) > 0.0, "still readable a good way out");
    assert_eq!(at(400), 0.0, "and gone across the province");
}

/// A cutting knows where it is needed. Countries are 900 blocks apart
/// and you can see a few hundred, so this is the only navigation that
/// works at province range — and it has to work without having been
/// there, because you cannot visit what you cannot find.
#[test]
fn a_seed_points_at_ground_that_would_take_it() {
    let reg = base_reg();
    let mut w = World::new(31, tmp_dir("seed-bearing"), reg.clone());
    // A badlands scar is dead by definition, so a seed can point at one
    // in a country nobody has ever loaded a chunk of.
    let g = &w.generator;
    let scar = (0..70)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                .collect::<Vec<_>>()
        })
        .map(|(kx, kz)| g.province_center(kx, kz))
        .find(|&(x, z)| {
            g.province(x, z).biome == Biome::Badlands
                && g.surface_estimate(x, z) > crate::chunk::SEA_LEVEL + 4
        })
        .expect("some badlands in this world");
    assert!(w.heart_at(scar.0, scar.1).is_none(), "never visited");

    // Stand well to the west of it and read the seed.
    let from = glam::Vec3::new((scar.0 - 400) as f32, 80.0, scar.1 as f32);
    let line = w.seed_bearing(from);
    assert!(line.contains("east"), "it leans toward the scar: {line}");
    assert!(line.contains("blocks"), "and says how far: {line}");

    // Standing on it, it says so instead of giving a bearing.
    let on = glam::Vec3::new(scar.0 as f32, 80.0, scar.1 as f32);
    let line = w.seed_bearing(on);
    assert!(line.contains("here"), "on the site: {line}");

    // A country this world watched die counts too, not only the scars.
    let (mut w2, key, (sx, sz)) = world_with_heart(42, "seed-bearing-killed");
    w2.set_heart_stage(key, 0);
    let line = w2.seed_bearing(glam::Vec3::new(sx as f32, 80.0, sz as f32));
    assert!(
        line.contains("here"),
        "a heart you killed is ground too: {line}"
    );
    let _ = &mut w;
}

/// Readiness is measured at the heart, not at the roof of the world.
/// `surface_height` was fine while a heart stood in the open; under an
/// edifice it returns the crest of the mass overhead, so the plots
/// being counted were the outside of a pyramid twenty blocks up and no
/// amount of work at the chamber floor could ever satisfy it. Doll hit
/// exactly this at a badlands ziggurat.
#[test]
fn ground_readiness_is_measured_at_the_heart_not_under_the_sky() {
    let (mut w, key, (sx, sz)) = world_with_heart(42, "root-under-cover");
    let reg = w.reg.clone();
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;

    // The disc spans several chunks and a site can back onto a cliff,
    // neither of which this test is about. Load them, then lay a level
    // floor of living soil at the heart's own level — tilled and fed,
    // the way a player would leave it.
    let farm = b(&reg, "base:farmland");
    let r = w.root_radius_at(hp.0, hp.2);
    // The disc reaches past the monument now, so it spans chunks a
    // fixture that loads one would leave as void.
    for cx in -2..=2 {
        for cz in -2..=2 {
            w.ensure_chunk(ChunkPos::of_world(hp.0 + cx * 16, hp.2 + cz * 16));
        }
    }
    w.edit_fixture_for_test(|world| {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx * dx + dz * dz > r * r {
                    continue;
                }
                let (cx, cz) = (hp.0 + dx, hp.2 + dz);
                if (dx, dz) != (0, 0) {
                    for up in 0..=6 {
                        world.set_block(cx, hp.1 + up, cz, AIR);
                    }
                }
                world.set_block(cx, hp.1 - 1, cz, farm);
                world.feed_soil(cx, hp.1 - 1, cz, 60);
            }
        }
    });
    let (ready, total) = w.root_ground_ready(hp.0, hp.1, hp.2);
    assert!(ready * 2 > total, "the ground is living ({ready}/{total})");
    assert!(w.plant_heart_seed(hp.0, hp.1, hp.2).is_none(), "it takes");

    // Now roof the whole site over, as an edifice does, and ask again.
    // The answer must not change: the work is at the heart's level.
    let stone = b(&reg, "base:stone");
    w.edit_fixture_for_test(|world| {
        for dx in -r..=r {
            for dz in -r..=r {
                for up in 8..14 {
                    world.set_block(hp.0 + dx, hp.1 + up, hp.2 + dz, stone);
                }
            }
        }
    });
    let (roofed, total2) = w.root_ground_ready(hp.0, hp.1, hp.2);
    assert_eq!(
        (roofed, total2),
        (ready, total),
        "a mass overhead does not un-till the ground beneath it"
    );
}

/// A refusal that does not name the work is a locked door. Only tilled
/// soil carries fertility — grass and bare dirt read as zero however
/// green they look — so Doll laid turf and berries around a dead spring
/// for nothing and the game never said why.
#[test]
fn the_refusal_says_what_the_ground_needs() {
    let (mut w, key, (sx, sz)) = world_with_heart(43, "root-refusal");
    w.set_heart_stage(key, 0);
    let hp = w.heart_at(sx, sz).unwrap().pos;
    let refusal = w
        .plant_heart_seed(hp.0, hp.1, hp.2)
        .expect("bare ground refuses");
    for want in ["hoe", "soil", "plots living"] {
        assert!(
            refusal.to_lowercase().contains(want),
            "the refusal should name {want:?}: {refusal}"
        );
    }
}

/// Nobody should have to excavate a pyramid. The ground a rooting
/// answers for has to clear the monument standing over the site, or
/// restoring a badlands ziggurat means hollowing out its interior.
#[test]
fn the_rooting_ground_clears_the_monument() {
    let reg = base_reg();
    let w = test_world_with("root-clear", reg.clone());
    for biome in (1..=12u8).filter_map(Biome::from_index) {
        let reach = crate::edifice::edifice_of(biome).reach;
        let radius = crate::world::ROOT_RADIUS.max(reach + 4);
        assert!(
            radius > reach,
            "{biome:?}: ground of {radius} must reach past a {reach}-wide mass"
        );
    }
    // And the world agrees with the rule at a real site.
    let (sx, sz) = w.generator.province_center(0, 0);
    let reach = crate::edifice::edifice_of(w.generator.biome(sx, sz)).reach;
    assert!(w.root_radius_at(sx, sz) > reach);
}

/// Placed soil arrives freshly turned. Farmland laid at zero fertility
/// looks tilled, grows nothing and counts for nothing — which is why a
/// hand-laid field around a dead heart did absolutely nothing.
#[test]
fn placed_soil_is_living_ground() {
    let reg = base_reg();
    let mut w = test_world_with("placed-soil", reg.clone());
    let farm = b(&reg, "base:farmland");
    let y = w.surface_height(4, 4);
    assert!(w.place_block((4, y + 1, 4), farm), "it goes down");
    assert!(
        w.fertility_at(4, y + 1, 4) >= crate::world::ROOT_READY_FERT,
        "and it is living ground, not a green-looking rock"
    );
}
