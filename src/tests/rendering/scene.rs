//! Scene scenarios.

use super::*;

#[test]
fn season_tint_repaints_foliage() {
    let px = crate::atlas::ATLAS_TILES * 8;
    let summer = crate::atlas::build_procedural(8);
    let grass = *crate::atlas::builtin_slots().get("grass_top").unwrap() as u32;
    let tile_px = |img: &Vec<u8>, slot: u32| -> Vec<u8> {
        let tp = 8u32;
        let (tx, ty) = (slot % 16 * tp, slot / 16 * tp);
        let mut out = Vec::new();
        for y in ty..ty + tp {
            for x in tx..tx + tp {
                let i = ((y * px + x) * 4) as usize;
                out.extend_from_slice(&summer[i..i + 3]);
                let _ = img;
            }
        }
        out
    };
    let reference = tile_px(&summer, grass);
    for season in [0usize, 2, 3] {
        let mut img = summer.clone();
        crate::atlas::season_tint(&mut img, px, season);
        let tp = 8u32;
        let (tx, ty) = (
            grass % crate::atlas::ATLAS_TILES * tp,
            grass / crate::atlas::ATLAS_TILES * tp,
        );
        let mut changed = false;
        let mut k = 0;
        for y in ty..ty + tp {
            for x in tx..tx + tp {
                let i = ((y * px + x) * 4) as usize;
                changed |= img[i..i + 3] != reference[k..k + 3];
                k += 3;
            }
        }
        assert!(changed, "season {season} repaints grass");
    }
    let mut img = summer.clone();
    crate::atlas::season_tint(&mut img, px, 1);
    assert_eq!(img, summer, "summer is the reference look");
}

#[test]
fn particle_pool_caps_culls_and_stays_in_tile() {
    use crate::particles::{CAP, Pool};
    let mut pool = Pool::default();
    let mut rng = 12345u32;

    // Flood far past the cap; the pool holds its line.
    for _ in 0..80 {
        pool.burst(Vec3::new(0.0, 64.0, 0.0), 3, 12, 2.0, &mut rng);
    }
    assert_eq!(pool.v.len(), CAP, "pool caps at {CAP}");

    // Everything dies by max ttl (0.7s for bursts).
    pool.tick(1.0);
    assert!(pool.v.is_empty(), "ttl culls the whole pool");

    // Sub-tile UVs stay inside the source tile for corner slots.
    let tiles = crate::atlas::ATLAS_TILES;
    let ts = 1.0 / tiles as f32;
    for slot in [0u16, (tiles * tiles - 1) as u16] {
        pool.burst(Vec3::ZERO, slot, 12, 2.0, &mut rng);
        let (mut verts, mut idx) = (Vec::new(), Vec::new());
        pool.emit(&mut verts, &mut idx);
        let (tx, ty) = (slot as u32 % tiles, slot as u32 / tiles);
        for v in &verts {
            assert!(
                v.uv[0] >= tx as f32 * ts - 1e-5 && v.uv[0] <= (tx + 1) as f32 * ts + 1e-5,
                "u {} outside tile {slot}",
                v.uv[0]
            );
            assert!(
                v.uv[1] >= ty as f32 * ts - 1e-5 && v.uv[1] <= (ty + 1) as f32 * ts + 1e-5,
                "v {} outside tile {slot}",
                v.uv[1]
            );
        }
        pool.v.clear();
    }
}

#[test]
fn player_style_packs_clamps_and_names_align() {
    use crate::style::*;
    // Every field combination survives the u32 round trip.
    for skin in 0..SKIN_TONES.len() as u8 {
        for hair in 0..HAIR_COLORS.len() as u8 {
            for hair_style in 0..HAIR_STYLE_NAMES.len() as u8 {
                for beard in 0..BEARD_NAMES.len() as u8 {
                    let s = Style {
                        skin,
                        hair,
                        shirt: (hair % SHIRT_COLORS.len() as u8),
                        trousers: (skin % TROUSER_COLORS.len() as u8),
                        hair_style,
                        beard,
                        legwear: skin % 2,
                        build: hair % 3,
                    };
                    assert_eq!(Style::unpack(s.pack()), s);
                }
            }
        }
    }
    // Garbage clamps into range instead of exploding the palette index.
    let wild = Style::unpack(u32::MAX);
    assert!((wild.skin as usize) < SKIN_TONES.len());
    assert!((wild.hair as usize) < HAIR_COLORS.len());
    assert!((wild.shirt as usize) < SHIRT_COLORS.len());
    assert!((wild.trousers as usize) < TROUSER_COLORS.len());
    assert!((wild.hair_style as usize) < HAIR_STYLE_NAMES.len());
    assert!((wild.beard as usize) < BEARD_NAMES.len());
    assert!((wild.legwear as usize) < LEGWEAR_NAMES.len());
    assert!((wild.build as usize) < BUILD_NAMES.len());
    // Display names track their palettes.
    assert_eq!(HAIR_NAMES.len(), HAIR_COLORS.len());
    assert_eq!(SHIRT_NAMES.len(), SHIRT_COLORS.len());
    assert_eq!(TROUSER_NAMES.len(), TROUSER_COLORS.len());
    // Variant slots stay above the mod region (compile-time consts,
    // but the relationship is the contract worth pinning).
    let (base, span) = (VARIANT_BASE as u32, VARIANT_SLOTS as u32);
    assert!(base >= crate::atlas::FIRST_FREE_SLOT as u32);
    assert_eq!(
        base + span,
        crate::atlas::ATLAS_TILES * crate::atlas::ATLAS_TILES,
        "variants cap the grid, wherever it ends"
    );

    let c = crate::config::Config::from_text("appearance=66051\n");
    assert_eq!(c.appearance, 66051, "appearance persists in config");
}

#[test]
fn humanoid_stands_full_height_with_hands() {
    use crate::mobs::{HeldArt, HumanoidArt, emit_humanoid};
    let art = HumanoidArt {
        skin: 1,
        face: 2,
        hair: Some(3),
        hair_front: 3,
        hair_top: 4,
        beard: None,
        shirt: 5,
        trousers: 6,
        boot: 7,
        long_hair: false,
        skirt: false,
        build: 1,
    };
    let (mut verts, mut idx) = (Vec::new(), Vec::new());
    emit_humanoid(
        ep(Vec3::ZERO),
        0.0,
        &art,
        (0.0, 0.0),
        HeldArt::None,
        ([1.0; 3], 1.0),
        &mut verts,
        &mut idx,
    );
    let origin = ep(Vec3::ZERO);
    let render_origin = origin.render_pos();
    let up = crate::planet::local_frame(origin.surface_point())
        .up
        .as_vec3();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for v in &verts {
        let radial_height = (Vec3::from_array(v.pos) - render_origin).dot(up);
        lo = lo.min(radial_height);
        hi = hi.max(radial_height);
    }
    // The sinking bug, pinned: feet at the position, head at the hitbox.
    assert!(lo > -0.01, "nothing below the feet (was: waist-deep)");
    assert!(
        (1.75..=1.87).contains(&(hi - lo)),
        "full height, got {}",
        hi - lo
    );
    // 11 parts minus the hair's skipped bottom face = 65 quads; hands
    // and hair are present or this count collapses.
    assert_eq!(idx.len() / 6, 65, "boots, hands, and hair all present");

    // A held block adds its six faces to the right hand.
    let (mut v2, mut i2) = (Vec::new(), Vec::new());
    emit_humanoid(
        ep(Vec3::ZERO),
        0.0,
        &art,
        (0.0, 0.0),
        HeldArt::Cube([9; 6]),
        ([1.0; 3], 1.0),
        &mut v2,
        &mut i2,
    );
    assert_eq!(i2.len() / 6, 71, "held cube rides the hand");

    // Shape choices add and remove real geometry.
    let quads = |art: &HumanoidArt| {
        let (mut v, mut i) = (Vec::new(), Vec::new());
        emit_humanoid(
            ep(Vec3::ZERO),
            0.0,
            art,
            (0.0, 0.0),
            HeldArt::None,
            ([1.0; 3], 1.0),
            &mut v,
            &mut i,
        );
        i.len() / 6
    };
    let base = HumanoidArt {
        skin: 1,
        face: 2,
        hair: Some(3),
        hair_front: 3,
        hair_top: 4,
        beard: None,
        shirt: 5,
        trousers: 6,
        boot: 7,
        long_hair: false,
        skirt: false,
        build: 1,
    };
    let bald = HumanoidArt { hair: None, ..base };
    assert_eq!(quads(&bald), 60, "bald drops the hair shell");
    let bearded = HumanoidArt {
        beard: Some(8),
        ..base
    };
    assert_eq!(quads(&bearded), 66, "a beard is one face band");
    let long = HumanoidArt {
        long_hair: true,
        ..base
    };
    assert_eq!(quads(&long), 71, "long hair adds the back panel");
    let skirted = HumanoidArt {
        skirt: true,
        ..base
    };
    assert_eq!(quads(&skirted), 71, "the skirt is a real box");
}

#[test]
fn frozen_clock_holds_the_sun_without_stopping_the_sim() {
    let reg = base_reg();
    let make = || {
        let world = World::new(7, tmp_dir("freeze"), reg.clone());
        crate::server::Server::new(world, 0.3, 42)
    };
    let step = |server: &mut crate::server::Server| {
        for _ in 0..40 {
            server.advance(crate::server::TICK, &[], &mut Vec::new());
        }
    };

    let mut running = make();
    let start = running.time_of_day;
    step(&mut running);
    assert!(running.time_of_day > start);

    let mut frozen = make();
    frozen.freeze_clock = true;
    frozen.world.ire = 50.0;
    let start = frozen.time_of_day;
    let ire = frozen.world.ire;
    step(&mut frozen);
    assert_eq!(frozen.time_of_day, start);
    assert!(
        frozen.world.ire < ire,
        "non-calendar simulation still advances"
    );
}
