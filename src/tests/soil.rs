//! Living soil: fertility drains, rests, rotates, and shows.

use super::*;
use crate::world::soil::{
    self, FERT_DRAIN, FERT_DRAIN_MONO, FERT_DRAIN_ROTATED, FERT_MAX, FERT_TILL_DIRT,
    FERT_TILL_GRASS,
};

#[test]
fn the_till_reads_the_ground() {
    let reg = base_reg();
    let mut w = test_world_with("till", reg.clone());
    let h = w.surface_height(4, 4);
    let grass = b(&reg, "base:grass");
    let dirt = b(&reg, "base:dirt");
    let sand = b(&reg, "base:sand");
    // Set the neighborhoods too: the till reads what is BESIDE the
    // cell, and the surrounding country supplies its own palette.
    for (cx, cz, fill) in [(4, 4, grass), (8, 8, dirt)] {
        for dx in -1..=1 {
            for dz in -1..=1 {
                w.set_block(cx + dx, h, cz + dz, fill);
            }
        }
    }
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.set_block(12 + dx, h, 12 + dz, dirt);
        }
    }
    w.set_block(13, h, 12, sand);
    let grassy = soil::fert_of(w.till_meta(4, h, 4));
    let dirty = soil::fert_of(w.till_meta(8, h, 8));
    let sandy = soil::fert_of(w.till_meta(12, h, 12));
    assert_eq!(grassy, FERT_TILL_GRASS, "grass-fed loam");
    assert_eq!(dirty, FERT_TILL_DIRT, "bare dirt starts leaner");
    assert!(sandy < dirty, "a sandy edge costs a step ({sandy})");
    // The quartile steps are visible: each band maps to its own tile.
    let farm = b(&reg, "base:farmland");
    let ft = reg.block(farm).fert_tiles.expect("farmland shows its soil");
    assert_eq!(ft.iter().collect::<std::collections::HashSet<_>>().len(), 4);
}

#[test]
fn fertile_soil_outgrows_dust() {
    let reg = base_reg();
    let mut w = test_world_with("fertgrow", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // Two strips: rich loam west, exhausted dust east.
    for z in 0..16 {
        w.set_block_meta(2, h, z, farm, soil::soil_meta(FERT_MAX, 0));
        w.set_block(2, h + 1, z, seed0);
        w.set_block_meta(13, h, z, farm, soil::soil_meta(2, 0));
        w.set_block(13, h + 1, z, seed0);
    }
    let mut rng = 777u32;
    for _ in 0..2500 {
        w.random_tick(&mut rng);
    }
    let count = |x: i32, w: &crate::world::World| {
        (0..16)
            .filter(|&z| w.get_block(x, h + 1, z) != seed0)
            .count()
    };
    let rich = count(2, &w);
    let dust = count(13, &w);
    assert!(
        rich > dust,
        "loam ({rich}) must outgrow dust ({dust}) over the same ticks"
    );
}

#[test]
fn maturing_crops_drain_and_rotation_is_gentler() {
    // The arithmetic of the rotation ledger, stated as facts.
    let rested = soil::soil_meta(40, 0);
    let after_first = soil::soil_after_harvest(rested, 1);
    assert_eq!(soil::fert_of(after_first), 40 - FERT_DRAIN);
    assert_eq!(soil::family_of(after_first), 1, "the family is stamped");
    let mono = soil::soil_after_harvest(after_first, 1);
    assert_eq!(
        soil::fert_of(mono),
        40 - FERT_DRAIN - FERT_DRAIN_MONO,
        "monoculture drains hardest"
    );
    let rotated = soil::soil_after_harvest(after_first, 2);
    assert_eq!(
        soil::fert_of(rotated),
        40 - FERT_DRAIN - FERT_DRAIN_ROTATED,
        "rotation drains gentlest"
    );
    // And through the world: a crop reaching ripe takes its meal.
    let reg = base_reg();
    let mut w = test_world_with("drain", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    for x in 0..16 {
        for z in 0..16 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(FERT_MAX, 0));
            w.set_block(x, h + 1, z, b(&reg, "base:wheat_seeds/stage1"));
        }
    }
    let mut rng = 31337u32;
    for _ in 0..4000 {
        w.random_tick(&mut rng);
    }
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    let drained = (0..16)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 1, z) == ripe)
        .filter(|&(x, z)| {
            soil::fert_of(w.get_meta(x, h, z)) == FERT_MAX - FERT_DRAIN
                && soil::family_of(w.get_meta(x, h, z)) == 1
        })
        .count();
    assert!(drained > 0, "some wheat ripened and drew from the soil");
}

#[test]
fn fallow_fields_recover_and_winter_is_the_soils_turn() {
    let reg = base_reg();
    let mut w = test_world_with("fallow", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    for x in 0..16 {
        for z in 0..16 {
            w.set_block_meta(x, h, z, farm, soil::soil_meta(10, 2));
        }
    }
    let total = |w: &crate::world::World| -> u32 {
        (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .map(|(x, z)| soil::fert_of(w.get_meta(x, h, z)) as u32)
            .sum()
    };
    let start = total(&w);
    let mut rng = 99u32;
    for _ in 0..600 {
        w.random_tick(&mut rng);
    }
    let summered = total(&w);
    assert!(
        summered > start,
        "fallow land recovers ({start}->{summered})"
    );
    // A winter twin recovers faster over the same ticks.
    let mut ww = test_world_with("fallow-winter", reg.clone());
    ww.day = 3 * crate::world::SEASON_DAYS;
    let hw = ww.surface_height(4, 4);
    for x in 0..16 {
        for z in 0..16 {
            ww.set_block_meta(x, hw, z, farm, soil::soil_meta(10, 2));
        }
    }
    let wstart: u32 = (0..16)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .map(|(x, z)| soil::fert_of(ww.get_meta(x, hw, z)) as u32)
        .sum();
    let mut rng = 99u32;
    for _ in 0..600 {
        ww.random_tick(&mut rng);
    }
    let wintered: u32 = (0..16)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .map(|(x, z)| soil::fert_of(ww.get_meta(x, hw, z)) as u32)
        .sum();
    assert!(
        wintered - wstart > summered - start,
        "winter restores double ({} vs {})",
        wintered - wstart,
        summered - start
    );
    // Fully rested soil forgets its last crop.
    let mut wf = test_world_with("forget", reg.clone());
    let hf = wf.surface_height(4, 4);
    wf.set_block_meta(4, hf, 4, farm, soil::soil_meta(FERT_MAX - 1, 2));
    wf.feed_soil(4, hf, 4, 5);
    assert_eq!(soil::fert_of(wf.get_meta(4, hf, 4)), FERT_MAX);
    assert_eq!(soil::family_of(wf.get_meta(4, hf, 4)), 0, "stamp cleared");
}

#[test]
fn grass_heals_bare_dirt_under_the_sky() {
    let reg = base_reg();
    let mut w = test_world_with("heal", reg.clone());
    // A pad well clear of the terrain: whatever country this world
    // rolled, the scar and its sky are the test's own.
    let h = 140;
    // (a ring of living grass around the scar, so healing has edges
    // to spread from — the surrounding country supplies none up here)
    let grass = b(&reg, "base:grass");
    let dirt = b(&reg, "base:dirt");
    let stone = b(&reg, "base:stone");
    // A dirt scar ringed with grass, open to the sky.
    for x in 3..13 {
        for z in 3..13 {
            let edge = x == 3 || x == 12 || z == 3 || z == 12;
            w.set_block(x, h, z, if edge { grass } else { dirt });
            for dy in 1..4 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
    // A buried control square sees no sky and must stay dirt.
    for x in 4..8 {
        for z in 20..24 {
            w.set_block(x, 20, z, dirt);
            w.set_block(x, 21, z, AIR);
            w.set_block(x, 22, z, stone);
        }
    }
    let mut rng = 4242u32;
    for _ in 0..12000 {
        w.random_tick(&mut rng);
    }
    let healed = (4..12)
        .flat_map(|x| (4..12).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h, z) == grass)
        .count();
    assert!(healed > 0, "the scar closes from the grassy edge");
    let dark = (4..8)
        .flat_map(|x| (20..24).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, 20, z) == grass)
        .count();
    assert_eq!(dark, 0, "no grass in the dark");
}

#[test]
fn soil_survives_the_save() {
    let reg = base_reg();
    let dir = tmp_dir("soil-save");
    let farm = b(&reg, "base:farmland");
    let h;
    {
        let mut w = World::new(7, dir.clone(), reg.clone());
        w.ensure_chunk(ChunkPos { x: 0, z: 0 });
        h = w.surface_height(4, 4);
        w.set_block_meta(4, h, 4, farm, soil::soil_meta(33, 2));
        w.save_modified();
    }
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(w.get_block(4, h, 4), farm);
    assert_eq!(soil::fert_of(w.get_meta(4, h, 4)), 33, "fertility persists");
    assert_eq!(soil::family_of(w.get_meta(4, h, 4)), 2, "stamp persists");
    assert_eq!(w.fertility_at(4, h, 4), 33);
}
