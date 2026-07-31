//! Planetary heart fixtures.
//!
//! The former suite addressed countries and heart blocks with unbounded
//! `(x,z)` / `(x,y,z)` tuples. Those fixtures could only exercise the PosZ
//! chart and actively hid seam mistakes. These tests retain the important
//! heart lifecycle evidence while using the same finite addresses as play.

use super::*;
use crate::planet::{BlockPos, FACE_BLOCKS, Face, SurfacePos};
use crate::world::{
    HEART_CUTTING_DAYS, HEART_DEATH_STRAIN, HEART_SICKEN_STRAIN, heart_block_name, heart_form,
    heart_height, seed_nature, seed_of_form,
};
use crate::worldgen::{Biome, ProvinceKey};

fn living_heart(seed: u32, tag: &str) -> (World, SurfacePos, ProvinceKey, crate::world::Heart) {
    let reg = base_reg();
    let mut world = World::new(seed, tmp_dir(tag), reg);
    for face in Face::ALL {
        for u in 0..9u8 {
            for v in 0..9u8 {
                let key = ProvinceKey { face, u, v };
                let site = world.generator.province_center_at(key);
                let province = world.generator.province_at(site);
                if province.key != key
                    || matches!(
                        province.biome,
                        Biome::Ocean | Biome::Mountains | Biome::Badlands
                    )
                    || world.generator.surface_estimate_at(site) <= SEA_LEVEL + 4
                {
                    continue;
                }
                world.ensure_chunk(ChunkPos::from_surface(site));
                if let Some(heart) = world.heart_at_surface(site) {
                    return (world, site, key, heart);
                }
            }
        }
    }
    panic!("seed {seed} has no generated dry living heart");
}

fn heart_block(world: &World, heart: crate::world::Heart) -> String {
    world.reg.block(world.get_block_at(heart.pos)).name.clone()
}

#[test]
fn generated_heart_has_a_finite_country_and_canonical_site() {
    let (world, site, key, heart) = living_heart(42, "planet-heart-find");
    assert_eq!(world.generator.province_at(site).key, key);
    assert_eq!(heart.pos.face(), site.face());
    assert!(heart.alive());
    assert!(heart.pos.u() < FACE_BLOCKS && heart.pos.v() < FACE_BLOCKS);

    let form = heart_form(world.generator.biome_at(heart.pos.surface()));
    assert_eq!(heart_block(&world, heart), heart_block_name(form, 2));
    for dy in 0..heart_height(form) {
        let block = heart.pos.offset(0, dy, 0).expect("heart height fits");
        assert!(
            world
                .reg
                .block(world.get_block_at(block))
                .name
                .starts_with("base:heart_")
        );
    }
}

#[test]
fn country_keys_include_the_face() {
    let reg = base_reg();
    let generator = crate::worldgen::Generator::new(9, &reg);
    let center = FACE_BLOCKS / 2;
    let a = SurfacePos::new(Face::PosZ, center, center).unwrap();
    let b = SurfacePos::new(Face::NegZ, center, center).unwrap();
    let pa = generator.province_at(a);
    let pb = generator.province_at(b);
    assert_eq!(pa.key.face, Face::PosZ);
    assert_eq!(pb.key.face, Face::NegZ);
    assert_ne!(pa.key, pb.key, "opposite faces cannot alias one country");
}

#[test]
fn every_country_form_has_a_distinct_seed_nature() {
    let biomes = [
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
    let mut seeds = std::collections::HashSet::new();
    for biome in biomes {
        let form = heart_form(biome);
        let seed = seed_of_form(form);
        assert_eq!(seed_nature(seed), Some(biome));
        assert!(seeds.insert(seed), "{biome:?} reuses another seed");
    }
}

#[test]
fn heart_stage_changes_reface_the_typed_site() {
    let (mut world, _, key, heart) = living_heart(7, "planet-heart-stage");
    let form = heart_form(world.generator.biome_at(heart.pos.surface()));
    world.set_heart_stage(key, 1);
    assert_eq!(
        world.reg.block(world.get_block_at(heart.pos)).name,
        heart_block_name(form, 1)
    );
    assert!(
        world
            .heart_report_at(heart.pos.surface())
            .contains("FAILING")
    );

    world.set_heart_stage(key, 0);
    assert_eq!(
        world.reg.block(world.get_block_at(heart.pos)).name,
        heart_block_name(form, 0)
    );
    assert!(!world.heart_alive_at_surface(heart.pos.surface()));
}

#[test]
fn planetary_heart_record_round_trips_wfh3() {
    let reg = base_reg();
    let dir = tmp_dir("planet-heart-save");
    let (site, key, expected);
    {
        let (_, found, found_key, _) = living_heart(11, "planet-heart-save-source");
        // Persist into the stable directory this test reloads.
        let mut world = World::new(11, dir.clone(), reg.clone());
        world.ensure_chunk(ChunkPos::from_surface(found));
        world
            .heart_at_surface(found)
            .expect("same seed produces the same planetary heart");
        site = found;
        key = found_key;
        world.set_heart_stage(key, 1);
        world.hearts.get_mut(&key).unwrap().strain = 6.5;
        expected = world.hearts[&key].pos;
        save_world(&mut world);
    }
    assert_eq!(
        std::fs::read(dir.join("hearts")).unwrap().get(..4),
        Some(b"WFH3".as_slice())
    );
    let mut loaded = World::load_or_create(dir, reg).unwrap();
    loaded.ensure_chunk(ChunkPos::from_surface(site));
    let heart = loaded
        .heart_at_surface(site)
        .expect("planetary ledger loaded");
    assert_eq!(heart.pos, expected);
    assert_eq!(heart.stage, 1);
    assert!((heart.strain - 6.5).abs() < 0.01);
    assert_eq!(loaded.generator.province_at(site).key, key);
}

#[test]
fn legacy_heart_ledger_is_not_reinterpreted_as_planetary() {
    let reg = base_reg();
    let dir = tmp_dir("planet-heart-reject-wfh2");
    std::fs::write(dir.join("hearts"), b"WFH2not-a-planet-record").unwrap();
    let world = World::new(19, dir, reg);
    assert!(
        world.hearts.is_empty(),
        "flat WFH2 coordinates must never be silently assigned a face"
    );
}

#[test]
fn cutting_has_a_real_regrowth_clock() {
    let (mut world, site, key, _) = living_heart(27, "planet-heart-cutting");
    assert!(world.take_heart_cutting_at(site));
    assert!(!world.take_heart_cutting_at(site));
    assert!((world.hearts[&key].regrow - HEART_CUTTING_DAYS).abs() < 0.01);
    world.tick_ire(HEART_CUTTING_DAYS + 0.01);
    assert!(world.take_heart_cutting_at(site));
}

#[test]
fn sustained_local_ire_sickens_and_kills_only_that_planetary_country() {
    let (mut world, site, key, _) = living_heart(33, "planet-heart-strain");
    world.add_ire_at_surface(site, 20.0);
    world.hearts.get_mut(&key).unwrap().strain = HEART_SICKEN_STRAIN - 0.1;
    world.tick_ire(1.0);
    assert_eq!(world.hearts[&key].stage, 1);
    world.hearts.get_mut(&key).unwrap().strain = HEART_DEATH_STRAIN - 0.1;
    world.tick_ire(1.0);
    assert_eq!(world.hearts[&key].stage, 0);
}

#[test]
fn reports_use_geodesic_bearing_across_a_face_seam() {
    let reg = base_reg();
    let mut world = World::new(5, tmp_dir("planet-heart-seam-report"), reg);
    let query = SurfacePos::new(Face::PosZ, FACE_BLOCKS - 2, FACE_BLOCKS / 2).unwrap();
    let across = crate::planet::step4(query, crate::planet::Direction4::East).pos;
    let key = world.generator.province_at(query).key;
    let site = BlockPos::new(across.face(), across.u(), 70, across.v()).unwrap();
    world.register_heart(key, site);
    let report = world.heart_report_at(query);
    assert!(
        report.contains("here") || report.contains("east"),
        "{report}"
    );
    assert!(
        !report.contains("pos_") && !report.contains("face"),
        "ordinary reports never leak chart coordinates: {report}"
    );
}

#[test]
fn seed_bearing_can_choose_a_dead_country_across_a_seam() {
    let reg = base_reg();
    let mut world = World::new(13, tmp_dir("planet-heart-seam-seed"), reg);
    let from_surface = SurfacePos::new(Face::PosZ, FACE_BLOCKS - 3, FACE_BLOCKS / 2).unwrap();
    let across = crate::planet::SurfacePos::canonicalized(
        from_surface.face(),
        i32::from(from_surface.u()) + 80,
        i32::from(from_surface.v()),
    )
    .unwrap();
    let key = world.generator.province_at(across).key;
    let site = world.generator.province_center_at(key);
    let heart = BlockPos::new(site.face(), site.u(), 70, site.v()).unwrap();
    world.register_heart(key, heart);
    world.set_heart_stage(key, 0);
    let from = crate::planet::EntityPos::new(
        from_surface.face(),
        from_surface.u() as f32 + 0.5,
        72.0,
        from_surface.v() as f32 + 0.5,
    )
    .unwrap();
    let reading = world.seed_bearing_at(from);
    assert!(
        reading.contains("blocks") || reading.contains("here"),
        "{reading}"
    );
}

#[test]
fn root_readiness_walks_through_seams_without_leaving_the_planet() {
    let reg = base_reg();
    let mut world = World::new(23, tmp_dir("planet-heart-root-seam"), reg.clone());
    let surface = SurfacePos::new(Face::PosZ, 1, FACE_BLOCKS / 2).unwrap();
    let key = world.generator.province_at(surface).key;
    let heart = BlockPos::new(surface.face(), surface.u(), 70, surface.v()).unwrap();
    world.register_heart(key, heart);
    world.set_heart_stage(key, 0);
    let radius = world.root_radius_at(surface);
    let mut chunks = std::collections::HashSet::new();
    for du in -radius..=radius {
        for dv in -radius..=radius {
            if du * du + dv * dv > radius * radius {
                continue;
            }
            let Some(at) = heart.offset(du, 0, dv) else {
                continue;
            };
            chunks.insert(at.chunk());
        }
    }
    world.insert_empty_chunks_for_test(chunks);
    let (_, total) = world.root_ground_ready_at(heart);
    assert!(total > 0);
    assert!(
        heart
            .offset(-radius, 0, 0)
            .is_some_and(|p| p.face() != heart.face()),
        "the fixture really crosses a face seam"
    );
}
