//! Voxel custody scenarios.

use super::*;

#[test]
fn ocean_fed_cave_materialization_debits_the_ocean_and_round_trips() {
    let atlas = PlanetAtlas::fixture(6_013, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let id = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| {
            surface_reservoir_parts(reservoir.id).0 == SurfaceReservoirKind::Ocean
                && reservoir.coarse.water_hu >= 256
        })
        .unwrap()
        .id;
    let before = weather.water.reservoir_mut(id).unwrap().clone();
    let parcel = weather.materialize_surface_water(id, 256);
    assert_eq!(parcel.water_hu, 256);
    let flooded = weather.water.reservoir_mut(id).unwrap().clone();
    assert_eq!(before.coarse.water_hu - flooded.coarse.water_hu, 256);
    assert_eq!(flooded.committed.water_hu - before.committed.water_hu, 256);
    assert!(weather.dematerialize_surface_water(id, parcel));
    let restored = weather.water.reservoir_mut(id).unwrap();
    assert_eq!(restored.coarse, before.coarse);
    assert_eq!(restored.committed, before.committed);
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn opening_a_cave_below_the_sea_moves_ocean_water_into_the_voxel() {
    let atlas = std::sync::Arc::new(PlanetAtlas::fixture(6_015, 8).unwrap());
    let (index, hydro) = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .find(|(_, hydro)| hydro.ocean_basin_id != 0)
        .unwrap();
    let atlas_pos = AtlasPos::from_index(index, atlas.side()).unwrap();
    let center = atlas_pos.center(atlas.side());
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 10, surface.v()).unwrap();
    let id = crate::planet_atlas::surface_reservoir_id(
        SurfaceReservoirKind::Ocean,
        u32::from(hydro.ocean_basin_id),
    );
    let reg = base_reg();
    let mut world =
        crate::world::World::new_with_atlas(6_015, tmp_dir("ocean-cave-seep"), reg.clone(), atlas);
    world.ensure_chunk(pos.chunk());
    world.set_block_at(pos, reg.block_id("base:stone").unwrap());
    let before = world
        .planetary_weather_for_test()
        .unwrap()
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == id)
        .unwrap()
        .clone();
    world.break_block_at(pos, None, false, false).unwrap();
    assert_eq!(
        world.water_mass_at(pos).unwrap().water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    let weather = world.planetary_weather_for_test().unwrap();
    let after = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == id)
        .unwrap();
    assert_eq!(
        before.coarse.water_hu - after.coarse.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        after.committed.water_hu - before.committed.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(weather))
            .unexplained_water_delta_hu,
        0
    );
}

#[test]
fn preferred_basin_debits_shrink_its_chunk_commitment() {
    let atlas = PlanetAtlas::fixture(6_014, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let id = weather
        .water
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.coarse.water_hu >= 256)
        .unwrap()
        .id;
    let parcel = weather.materialize_surface_water(id, 256);
    let chunk = crate::planet::ChunkPos::new(Face::PosZ, 1, 1).unwrap();
    weather.water.commitments.push(ChunkWaterCommitment {
        chunk,
        reservoir: id,
        mass: parcel,
    });
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 0,
    };
    assert!(weather.credit_detailed_vapor_from(pos, Some(id), ReservoirMass::fresh(32),));
    let commitment = weather
        .water
        .commitments
        .iter()
        .find(|commitment| commitment.reservoir == id)
        .unwrap();
    assert_eq!(commitment.mass.water_hu, 224);
    assert_eq!(commitment.mass.salt_mass, parcel.salt_mass);
    weather
        .water
        .validate(atlas.side(), atmosphere(&weather))
        .unwrap();
}

#[test]
fn perennial_channels_take_baseflow_but_intermittent_channels_do_not() {
    let initial = ReservoirMass::with_salinity(65_536, 12);
    let mut perennial = WaterCell {
        groundwater: initial,
        ..WaterCell::default()
    };
    let mut intermittent = perennial;
    let perennial_hydro = HydrologyCell {
        river_id: 7,
        stream_order: 3,
        ..HydrologyCell::default()
    };
    let intermittent_hydro = HydrologyCell {
        river_id: 8,
        stream_order: 2,
        ..HydrologyCell::default()
    };
    let (_, baseflow) = take_river_baseflow(perennial_hydro, &mut perennial).unwrap();
    assert!(baseflow.water_hu > 0);
    assert_eq!(
        perennial.groundwater.water_hu + baseflow.water_hu,
        initial.water_hu
    );
    assert_eq!(
        perennial.groundwater.salt_mass + baseflow.salt_mass,
        initial.salt_mass
    );
    assert!(take_river_baseflow(intermittent_hydro, &mut intermittent).is_none());
    assert_eq!(intermittent.groundwater, initial);
}
