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
fn dead_and_living_hearts_leave_planetary_climate_and_water_identical() {
    let reg = base_reg();
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_712, 8).unwrap());
    let country = atlas.biomes.countries.first().expect("a country");
    let point = country.heart_site.center(atlas.side());
    let site = SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16).unwrap();
    let mut living = World::new_with_atlas(
        8_712,
        tmp_dir("living-heart-conservation"),
        reg.clone(),
        atlas.clone(),
    );
    let mut dead = World::new_with_atlas(8_712, tmp_dir("dead-heart-conservation"), reg, atlas);
    let chunk = ChunkPos::from_surface(site);
    living.ensure_chunk(chunk);
    dead.ensure_chunk(chunk);
    let key = dead.generator.province_at(site).key;
    assert!(dead.heart_at_surface(site).is_some());
    dead.set_heart_stage(key, 0);
    assert!(!dead.heart_alive_at_surface(site));
    assert!(living.heart_alive_at_surface(site));

    assert!(living.tick_planetary_weather(usize::MAX).unwrap().is_some());
    assert!(dead.tick_planetary_weather(usize::MAX).unwrap().is_some());
    let living_weather = living.planetary_weather_for_test().unwrap();
    let dead_weather = dead.planetary_weather_for_test().unwrap();
    assert_eq!(living_weather.cells, dead_weather.cells);
    assert_eq!(living_weather.water, dead_weather.water);
    let atmosphere = |weather: &crate::planet_atlas::PlanetaryWeather| {
        crate::planet_atlas::ReservoirMass::fresh(crate::planet_atlas::dynamic_water_total(
            &weather.cells,
        ) as u64)
    };
    for weather in [living_weather, dead_weather] {
        let audit = weather.water.audit(atmosphere(weather));
        assert_eq!(audit.unexplained_water_delta_hu, 0);
        assert_eq!(audit.unexplained_salt_delta, 0);
    }
}

#[test]
fn foreign_grafts_follow_climate_and_marginal_ones_need_real_water() {
    use crate::planet_atlas::GraftCompatibility;

    let reg = base_reg();
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_714, 32).unwrap());
    let mut native = 0usize;
    let mut incompatible = 0usize;
    let mut marginal_site = None;
    for country in &atlas.biomes.countries {
        let point = country.heart_site.center(atlas.side());
        let site =
            SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16).unwrap();
        assert_eq!(
            atlas.graft_compatibility_at(site, atlas.biome_sample(site).zonal_biome),
            GraftCompatibility::Compatible,
            "native ecology is always physically supportable"
        );
        native += 1;
        for target in 1..=12u8 {
            match atlas.graft_compatibility_at(site, target) {
                GraftCompatibility::Incompatible => incompatible += 1,
                GraftCompatibility::Marginal
                    if marginal_site.is_none()
                        && atlas
                            .water_cycle
                            .cells
                            .get(country.heart_site)
                            .unwrap()
                            .groundwater
                            .water_hu
                            >= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK =>
                {
                    marginal_site = Some((country.clone(), site, target));
                }
                _ => {}
            }
        }
    }
    assert!(native > 0 && incompatible > 0);
    let (country, site, target) = marginal_site.expect("a marginal graft over a pumpable aquifer");
    let target = Biome::from_index(target).unwrap();
    let mut world = World::new_with_atlas(
        8_714,
        tmp_dir("marginal-graft-support"),
        reg.clone(),
        atlas.clone(),
    );
    world.ensure_chunk(ChunkPos::from_surface(site));
    let heart = world
        .heart_at_surface(site)
        .expect("generated country heart");
    let key = world.generator.province_at(site).key;
    world.set_heart_stage(key, 2);
    {
        let state = world.hearts.get_mut(&key).unwrap();
        state.graft = Some(target);
        state.drift = 0.25;
    }
    let atlas_index = country.heart_site.index(atlas.side());
    world
        .planetary_weather_for_test_mut()
        .unwrap()
        .water
        .cells
        .values_mut()[atlas_index]
        .soil = crate::planet_atlas::ReservoirMass::default();
    let probes = [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 0, 1),
        (0, 0, -1),
        (1, 1, 0),
        (-1, 1, 0),
        (0, 1, 1),
        (0, 1, -1),
    ];
    // A country heart may sit on a chunk edge. Missing chunks read as air,
    // but cannot retain the detailed irrigation voxel written below, so the
    // complete test chamber must be resident before choosing a channel.
    for neighbor in probes
        .into_iter()
        .filter_map(|(du, dy, dv)| heart.pos.offset(du, dy, dv))
    {
        world.ensure_chunk(neighbor.chunk());
    }
    for neighbor in probes
        .into_iter()
        .filter_map(|(du, dy, dv)| heart.pos.offset(du, dy, dv))
    {
        if reg.is_water(world.get_block_at(neighbor)) {
            world.set_block_at(neighbor, AIR);
        }
    }
    assert!(world.managed_soil_moisture_at(heart.pos) < 0.85);
    world.tick_ire(1.0);
    let unsupported = world.hearts[&key].drift;
    assert!(unsupported < 0.25, "an unsupported marginal graft recedes");

    let parcel = world
        .planetary_weather_for_test_mut()
        .unwrap()
        .pump_groundwater(
            country.heart_site,
            crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
        );
    assert_eq!(parcel.water_hu, crate::planet_atlas::HYDRO_UNITS_PER_BLOCK);
    let class = world
        .planetary_weather_for_test_mut()
        .unwrap()
        .move_detailed_to_portable(parcel)
        .unwrap();
    let channel = probes
        .into_iter()
        .filter_map(|(du, dy, dv)| heart.pos.offset(du, dy, dv))
        .find(|neighbor| world.get_block_at(*neighbor) == AIR)
        .expect("heart chamber has room for an irrigation channel");
    assert!(world.place_portable_water_at(channel, class));
    assert!(world.managed_soil_moisture_at(heart.pos) >= 0.85);
    world.hearts.get_mut(&key).unwrap().drift = 0.0;
    world.tick_ire(1.0);
    assert!(
        world.hearts[&key].drift > 0.0,
        "the same marginal graft advances only while supplied with audited water"
    );
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
fn planetary_heart_record_round_trips_wfh4() {
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
        Some(b"WFH4".as_slice())
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
