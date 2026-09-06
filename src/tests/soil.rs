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
    // Loose sand correctly falls if an uneven generated column leaves air
    // below the fixture. Sandstone is stable and carries the same sandy
    // neighborhood signal this tilling rule is meant to test.
    let sand = b(&reg, "base:sandstone");
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
    // Put both populations in the same generated chunk and in guaranteed
    // open sky. A pair of 16-cell strips produced only a handful of random
    // visits, so which strip won was mostly sampling noise rather than soil.
    let h = 200;
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // Two equal half-fields: rich loam west, exhausted dust east.
    for x in 0..16 {
        for z in 0..16 {
            let fertility = if x < 8 { FERT_MAX } else { 2 };
            w.set_block_meta(x, h, z, farm, soil::soil_meta(fertility, 0));
            w.set_block(x, h + 1, z, seed0);
            w.set_block(x, h + 2, z, AIR);
        }
    }
    let mut rng = 777u32;
    for _ in 0..2500 {
        w.random_tick(&mut rng);
    }
    let count = |xs: std::ops::Range<i32>, w: &crate::world::World| {
        xs.flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| w.get_block(x, h + 1, z) != seed0)
            .count()
    };
    let rich = count(0..8, &w);
    let dust = count(8..16, &w);
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
    ww.set_calendar_day(3 * crate::world::SEASON_DAYS);
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
        w.ensure_chunk(tchunk(0, 0));
        h = w.surface_height(4, 4);
        w.set_block_meta(4, h, 4, farm, soil::soil_meta(33, 2));
        w.set_soil_salinity_at(bp(4, h, 4), 153);
        save_world(&mut w);
    }
    let mut w = World::load_or_create(dir, reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    assert_eq!(w.get_block(4, h, 4), farm);
    assert_eq!(soil::fert_of(w.get_meta(4, h, 4)), 33, "fertility persists");
    assert_eq!(soil::family_of(w.get_meta(4, h, 4)), 2, "stamp persists");
    assert_eq!(
        w.get_soil_salinity_at(bp(4, h, 4)),
        153,
        "managed soil salinity persists in WFC8"
    );
    assert_eq!(w.fertility_at(4, h, 4), 33);
}

#[test]
fn saline_soil_blocks_crops_until_it_is_leached() {
    let reg = base_reg();
    let mut w = test_world_with("soil-salt-stress", reg.clone());
    let farm = b(&reg, "base:farmland");
    let h = w.surface_height(4, 4);
    let at = bp(4, h, 4);
    w.set_block_meta(4, h, 4, farm, soil::soil_meta(40, 0));
    w.set_soil_salinity_at(at, 0);
    let fresh = w.crop_soil_multiplier_at(at);
    w.set_soil_salinity_at(at, 190);
    assert_eq!(w.crop_soil_multiplier_at(at), 0.0);
    assert!(fresh > 0.5);
    assert!(
        w.soil_failure_at(at)
            .is_some_and(|reason| reason.contains("salt"))
    );
}

#[test]
fn crops_distinguish_waterlogged_well_drained_and_excessively_drained_soil() {
    use crate::planet::Face;
    use crate::planet_atlas::AtlasPos;

    let reg = base_reg();
    let mut atlas = crate::planet_atlas::PlanetAtlas::fixture(8_714, 16).unwrap();
    let sites = [
        AtlasPos::new(Face::PosZ, 2, 2, 16).unwrap(),
        AtlasPos::new(Face::PosZ, 3, 2, 16).unwrap(),
        AtlasPos::new(Face::PosZ, 4, 2, 16).unwrap(),
    ];
    for (site, drainage) in sites.into_iter().zip([24, 150, 245]) {
        atlas.genesis.ground.get_mut(site).unwrap().drainage = drainage;
    }
    let atlas = std::sync::Arc::new(atlas);
    let mut world = World::new_with_atlas(
        8_714,
        tmp_dir("soil-drainage-response"),
        reg.clone(),
        atlas.clone(),
    );
    let farm = b(&reg, "base:farmland");
    let mut fields = Vec::new();
    for site in sites {
        let center = site.center(atlas.side());
        let surface = crate::planet::SurfacePos::new(
            center.face,
            center.u.floor() as u16,
            center.v.floor() as u16,
        )
        .unwrap();
        world.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
        let field =
            crate::planet::BlockPos::new(surface.face(), surface.u(), 200, surface.v()).unwrap();
        world.set_block_meta_at(field, farm, soil::soil_meta(40, 0));
        world.set_soil_salinity_at(field, 0);
        fields.push(field);
    }

    let waterlogged = world.crop_soil_multiplier_at(fields[0]);
    let well_drained = world.crop_soil_multiplier_at(fields[1]);
    let excessive = world.crop_soil_multiplier_at(fields[2]);
    assert!(waterlogged < well_drained, "waterlogging slows roots");
    assert!(excessive < well_drained, "excessive drainage dries roots");
    assert!(
        world
            .soil_failure_at(fields[0])
            .is_some_and(|reason| reason.contains("waterlogged")),
        "the failure is communicated without a numerical dashboard"
    );
}

#[test]
fn irrigation_uses_a_debited_aquifer_parcel_and_wet_roots_read_it() {
    let reg = base_reg();
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_713, 16).unwrap());
    let (index, atlas_pos) = atlas
        .genesis
        .ground
        .iter()
        .enumerate()
        .find_map(|(index, (pos, ground))| {
            let water = atlas.water_cycle.cells.get(pos)?;
            let climate = atlas.genesis.climate.get(pos)?;
            let terrain = atlas.genesis.terrain.get(pos)?;
            let hydrology = atlas.genesis.hydrology.get(pos)?;
            let baseline = (climate.mean_precipitation * 4.0).max(256.0)
                * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as f32;
            let moisture = water.soil.water_hu as f32 / baseline;
            (ground.aquifer_permeability >= 8_192
                && water.groundwater.water_hu >= crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
                && water.groundwater_head_milliblocks > 8_000
                && terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32
                && hydrology.ocean_basin_id == 0
                && moisture < 1.0)
                .then_some((index, pos))
        })
        .expect("fixture contains a dry field over a productive aquifer");
    let point = atlas_pos.center(atlas.side());
    let surface =
        crate::planet::SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16)
            .unwrap();
    let mut world = World::new_with_atlas(
        8_713,
        tmp_dir("finite-irrigation"),
        reg.clone(),
        atlas.clone(),
    );
    world.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
    let head = atlas.water_cycle.cells.values()[index].groundwater_head_milliblocks / 1_000;
    let y = head.clamp(6, crate::chunk::CHUNK_Y as i32 - 4) - 1;
    let outlet =
        crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).unwrap();
    let soil_pos = outlet.offset(1, 0, 0).unwrap();
    world.set_block_at(outlet, b(&reg, "base:stone"));
    world.set_block_meta_at(soil_pos, b(&reg, "base:farmland"), soil::soil_meta(40, 0));
    world.set_soil_salinity_at(soil_pos, 0);
    let dry = world.managed_soil_moisture_at(soil_pos);
    let groundwater_before = world
        .planetary_weather_for_test()
        .unwrap()
        .water
        .cells
        .values()[index]
        .groundwater;

    world.break_block_at(outlet, None, false, false).unwrap();
    let parcel = world
        .water_mass_at(outlet)
        .expect("the excavation becomes a spring-fed channel");
    let groundwater_after = world
        .planetary_weather_for_test()
        .unwrap()
        .water
        .cells
        .values()[index]
        .groundwater;
    assert_eq!(
        groundwater_before.water_hu - groundwater_after.water_hu,
        parcel.water_hu,
        "visible irrigation water was pumped out of the named aquifer store"
    );
    assert_eq!(
        groundwater_before.salt_mass - groundwater_after.salt_mass,
        parcel.salt_mass
    );
    let irrigated = world.managed_soil_moisture_at(soil_pos);
    assert!(
        irrigated > dry && irrigated >= 1.0,
        "real channel water changes root moisture ({dry:.2} -> {irrigated:.2})"
    );
    let weather = world.planetary_weather_for_test().unwrap();
    let atmosphere = crate::planet_atlas::ReservoirMass::fresh(
        crate::planet_atlas::dynamic_water_total(&weather.cells) as u64,
    );
    let audit = weather.water.audit(atmosphere);
    assert_eq!(audit.unexplained_water_delta_hu, 0);
    assert_eq!(audit.unexplained_salt_delta, 0);
}
