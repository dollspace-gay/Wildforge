//! Groundwater scenarios.

use super::*;

#[test]
fn groundwater_crosses_a_cube_seam_without_mass_leak() {
    let atlas = PlanetAtlas::fixture(6_003, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    for cell in weather.water.cells.values_mut() {
        cell.groundwater_head_milliblocks = 0;
    }
    let source = AtlasPos {
        face: Face::PosZ,
        u: 0,
        v: 1,
    };
    let target = source.step(Direction4::West, atlas.side()).pos;
    let room = weather
        .water
        .cells
        .get_mut(target)
        .unwrap()
        .groundwater
        .take(512);
    weather
        .water
        .cells
        .get_mut(source)
        .unwrap()
        .groundwater
        .add_assign(room)
        .unwrap();
    weather
        .water
        .cells
        .get_mut(source)
        .unwrap()
        .groundwater_head_milliblocks = 20_000;
    let target_before = weather
        .water
        .cells
        .get(target)
        .unwrap()
        .groundwater
        .water_hu;
    let before = weather.water.audit(atmosphere(&weather));
    weather.advance_groundwater_day_for_test(&atlas).unwrap();
    let after = weather.water.audit(atmosphere(&weather));
    assert!(
        weather
            .water
            .cells
            .get(target)
            .unwrap()
            .groundwater
            .water_hu
            > target_before
    );
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
}

#[test]
fn a_spring_weakens_after_drawdown_and_can_recover() {
    let atlas = PlanetAtlas::fixture(6_004, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::PosZ,
        u: 1,
        v: 1,
    };
    let cell = weather.water.cells.get_mut(pos).unwrap();
    cell.groundwater_head_milliblocks = 20_000;
    weather.water.springs = vec![SpringState {
        pos,
        layer: AquiferLayer::Shallow,
        outlet_milliblocks: 10_000,
        last_discharge_hu: 0,
        active: false,
    }];
    weather.complete_hour(&atlas, 20.0, 0, 0.0).unwrap();
    let flowing = weather.water.springs[0].last_discharge_hu;
    assert!(flowing > 0);
    let _ = weather.pump_groundwater(pos, u64::MAX);
    weather
        .water
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = 0;
    weather.complete_hour(&atlas, 20.0, 1, 0.0).unwrap();
    assert_eq!(weather.water.springs[0].last_discharge_hu, 0);
    weather
        .water
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = 20_000;
    weather.complete_hour(&atlas, 20.0, 2, 0.0).unwrap();
    assert!(weather.water.springs[0].last_discharge_hu <= flowing);
}

#[test]
fn pumping_makes_a_drawdown_cone_that_groundwater_refills() {
    let atlas = PlanetAtlas::fixture(6_010, 4).unwrap();
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let pos = AtlasPos {
        face: Face::NegX,
        u: 1,
        v: 1,
    };
    let neighbor = pos.step(Direction4::East, atlas.side()).pos;
    let head_before = weather
        .water
        .cells
        .get(pos)
        .unwrap()
        .groundwater_head_milliblocks;
    let neighbor_before = weather
        .water
        .cells
        .get(neighbor)
        .unwrap()
        .groundwater
        .water_hu;
    let pumped = weather.pump_groundwater(pos, 256);
    assert_eq!(pumped.water_hu, 256);
    assert!(
        weather
            .water
            .cells
            .get(pos)
            .unwrap()
            .groundwater_head_milliblocks
            < head_before
    );
    weather.advance_groundwater_day_for_test(&atlas).unwrap();
    assert_ne!(
        weather
            .water
            .cells
            .get(neighbor)
            .unwrap()
            .groundwater
            .water_hu,
        neighbor_before
    );
    assert_eq!(
        weather
            .water
            .audit(atmosphere(&weather))
            .unexplained_water_delta_hu,
        0
    );
}
