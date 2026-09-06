//! Aquatic scenarios.

use super::*;

#[test]
fn fish_stay_wet_and_the_rod_takes_the_real_one_first() {
    let reg = base_reg();
    let mut w = test_world_with("pond", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    // A dug pond, three deep, filled with source water.
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    for x in 5..=11 {
        for z in 5..=11 {
            for dy in 1..=3 {
                w.set_block(
                    x,
                    h - dy,
                    z,
                    if x == 5 || x == 11 || z == 5 || z == 11 {
                        stone
                    } else {
                        water
                    },
                );
            }
            w.set_block(x, h, z, AIR);
        }
    }
    // Seal the floor.
    for x in 5..=11 {
        for z in 5..=11 {
            w.set_block(x, h - 4, z, stone);
        }
    }
    let trout_si = reg.animal_id("base:trout").unwrap();
    let mut f = beast(
        &reg,
        "base:trout",
        glam::Vec3::new(8.5, h as f32 - 2.5, 8.5),
    );
    f.tamed = true; // the tag that finds our fish among strangers
    w.spawn_mob(f);
    let mut rng = 41u32;
    for _ in 0..1200 {
        w.tick_mobs(
            &[ctx(glam::Vec3::new(8.5, h as f32 + 1.0, 2.5))],
            1.0,
            0.05,
            &mut rng,
        );
    }
    let fish = w
        .mobs()
        .iter()
        .find(|m| m.tamed && m.species == trout_si)
        .expect("the trout persists near a player");
    let cell = (
        fish.pos.x.floor() as i32,
        (fish.pos.y + 0.2).floor() as i32,
        fish.pos.z.floor() as i32,
    );
    assert!(
        reg.is_water(w.get_block(cell.0, cell.1, cell.2)),
        "a minute later the trout is still swimming (at {:?})",
        fish.pos
    );
    // The rod: the real fish comes out before any luck table.
    let hooked = w.catch_fish_near(glam::Vec3::new(8.5, h as f32 - 2.0, 8.5), 6.0);
    assert_eq!(hooked, Some(trout_si), "the strike lands the trout");
    assert!(
        !w.mobs().iter().any(|m| m.tamed && m.species == trout_si),
        "and the water is emptier for it"
    );
    // Empty water gives the rod nothing real.
    assert_eq!(
        w.catch_fish_near(glam::Vec3::new(8.5, h as f32 - 2.0, 8.5), 6.0),
        None
    );
}

#[test]
fn the_heron_works_the_shallows() {
    let reg = base_reg();
    let mut w = test_world_with("heron", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    for x in 6..=10 {
        for z in 6..=10 {
            w.set_block(
                x,
                h - 1,
                z,
                if x == 6 || x == 10 || z == 6 || z == 10 {
                    stone
                } else {
                    water
                },
            );
            w.set_block(x, h, z, AIR);
        }
    }
    let trout_si = reg.animal_id("base:trout").unwrap();
    w.spawn_mob(beast(
        &reg,
        "base:trout",
        glam::Vec3::new(8.5, h as f32 - 0.5, 8.5),
    ));
    let mut hb = beast(
        &reg,
        "base:heron",
        glam::Vec3::new(12.5, h as f32 + 3.0, 8.5),
    );
    hb.belly = -1.0;
    w.spawn_mob(hb);
    let mut rng = 43u32;
    let mut taken = false;
    for _ in 0..3000 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        if !w.mobs().iter().any(|m| m.species == trout_si) {
            taken = true;
            break;
        }
    }
    assert!(taken, "the heron speared the trout");
}

#[test]
fn the_crab_pinches_what_bothers_it() {
    let reg = base_reg();
    let mut w = test_world_with("crab", reg.clone());
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 16, 0, 16, h);
    w.spawn_mob(beast(
        &reg,
        "base:crab",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    ));
    let player = glam::Vec3::new(9.3, h as f32 + 1.0, 8.5);
    let mut rng = 47u32;
    let mut pinched = false;
    for _ in 0..1200 {
        let evs = w.tick_mobs(&[ctx(player)], 1.0, 0.05, &mut rng);
        if evs.iter().any(
            |e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, dmg: d, .. } if *d <= 1.5),
        ) {
            pinched = true;
            break;
        }
    }
    assert!(pinched, "stand on a crab, get pinched");
}

/// The sea used to be empty. Fish shared the single per-chunk wildlife
/// slot with the landfolk and lost it to whatever was defined earlier
/// in the file — and in an ocean chunk that winner then failed to place
/// on dry ground, so the chunk stocked nothing at all.
#[test]
fn the_sea_keeps_its_own_life() {
    use crate::planet::{FACE_CHUNKS, Face, SurfacePos};
    use crate::worldgen::Biome;

    let reg = base_reg();
    let mut w = test_world_with("sea-roster", reg.clone());
    let cod_si = reg.animal_id("base:cod").unwrap();
    let mackerel_si = reg.animal_id("base:mackerel").unwrap();
    let gull_si = reg.animal_id("base:gull").unwrap();
    let trout_si = reg.animal_id("base:trout").unwrap();
    let has = |world: &World, species: usize| world.mobs().iter().any(|mob| mob.species == species);

    // Visit actual deep-ocean chunks all around the finite planet. The
    // old test swept a square around the PosZ chart origin, which is now
    // just one mostly-jungle country and is not representative of the sea.
    let mut open_water = 0;
    'ocean: for face in Face::ALL {
        for cu in (0..FACE_CHUNKS).step_by(7) {
            for cv in (0..FACE_CHUNKS).step_by(7) {
                let center = SurfacePos::new(
                    face,
                    cu * crate::chunk::CHUNK_X as u16 + 8,
                    cv * crate::chunk::CHUNK_Z as u16 + 8,
                )
                .unwrap();
                if w.generator.surface_estimate_at(center) >= crate::chunk::SEA_LEVEL - 4 {
                    continue;
                }
                w.ensure_chunk(crate::planet::ChunkPos::new(face, cu, cv).unwrap());
                if w.is_open_water_at(center) {
                    open_water += 1;
                }
                if open_water >= 24 && has(&w, cod_si) && has(&w, mackerel_si) && has(&w, gull_si) {
                    break 'ocean;
                }
            }
        }
    }
    assert!(open_water > 0, "this sweep has to contain some sea");

    // Fresh water belongs to the surrounding country. Load cold,
    // above-sea hydrology cells until the deterministic roster has had
    // a representative set of river/lake chunks in which to roll trout.
    'fresh: for face in Face::ALL {
        for cu in (0..FACE_CHUNKS).step_by(5) {
            for cv in (0..FACE_CHUNKS).step_by(5) {
                let center = SurfacePos::new(
                    face,
                    cu * crate::chunk::CHUNK_X as u16 + 8,
                    cv * crate::chunk::CHUNK_Z as u16 + 8,
                )
                .unwrap();
                if !matches!(
                    w.generator.biome_at(center),
                    Biome::Taiga | Biome::Arctic | Biome::Mountains | Biome::Tundra
                ) || w.generator.surface_estimate_at(center) <= crate::chunk::SEA_LEVEL + 2
                    || w.generator.water_features_at(center).is_none()
                {
                    continue;
                }
                w.ensure_chunk(crate::planet::ChunkPos::new(face, cu, cv).unwrap());
                if has(&w, trout_si) {
                    break 'fresh;
                }
            }
        }
    }

    let count = |name: &str| {
        let si = reg.animal_id(name).unwrap();
        w.mobs().iter().filter(|m| m.species == si).count()
    };
    // Salt water: its own natives, below the surface and over it.
    let cod = count("base:cod");
    let mackerel = count("base:mackerel");
    let gull = count("base:gull");
    let trout = count("base:trout");
    assert!(
        cod > 0,
        "cod in the sea (cod={cod}, mackerel={mackerel}, gull={gull}, trout={trout})"
    );
    assert!(
        mackerel > 0,
        "mackerel in the sea (cod={cod}, mackerel={mackerel}, gull={gull}, trout={trout})"
    );
    assert!(gull > 0, "gulls over it — a flier needs no ground");
    // Fresh water still stocks the country's own fish, so the new
    // roster did not simply replace the old one.
    assert!(trout > 0, "trout still in cold fresh water");
    // Every fish is actually IN water, not flopping on a hill.
    for m in w.mobs() {
        if !reg.animals[m.species].movement_swim {
            continue;
        }
        let at = m.pos.block().map_or(AIR, |pos| w.get_block_at(pos));
        assert!(
            reg.is_water(at),
            "{} spawned out of water at {:?}",
            reg.animals[m.species].name,
            m.pos
        );
    }
}

/// A drowned column is Ocean whatever its country grows. The country
/// itself keeps its culture — the heart of a coastal forest province
/// still knows it is a forest.
#[test]
fn open_water_reads_as_ocean_not_as_the_coast_behind_it() {
    use crate::worldgen::Biome;
    let reg = base_reg();
    let mut w = test_world_with("ocean-label", reg.clone());
    let (mut sea, mut land) = (None, None);
    for cx in -14..14 {
        for cz in -14..14 {
            w.ensure_chunk(tchunk(cx, cz));
            let (x, z) = (cx * 16 + 8, cz * 16 + 8);
            if w.is_open_water(x, z) {
                sea = sea.or(Some((x, z)));
            } else {
                land = land.or(Some((x, z)));
            }
        }
    }
    let (sx, sz) = sea.expect("some sea in this sweep");
    let (lx, lz) = land.expect("some land in this sweep");
    assert_eq!(
        w.biome_here_at(bp(sx, 0, sz).surface()),
        Biome::Ocean,
        "you are in the sea"
    );
    assert_ne!(
        w.country_biome(sx, sz),
        Biome::Ocean,
        "the country it belongs to is still a country"
    );
    assert_eq!(
        w.biome_here_at(bp(lx, 0, lz).surface()),
        w.country_biome(lx, lz),
        "on dry ground the two agree"
    );
    // Nothing classifies AS ocean: it is not a climate, so no province
    // can be labelled with it and no centroid can claim it.
    for x in (-200..200).step_by(37) {
        assert_ne!(w.generator.biome(x, x * 3), Biome::Ocean);
    }
}
