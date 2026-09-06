//! Climate scenarios.

use super::*;

#[test]
fn wet_country_has_denser_more_perennial_water_than_arid_country() {
    let atlas = atlas();
    let mut wet = (0u64, 0u64, 0u64);
    let mut dry = (0u64, 0u64, 0u64);
    for index in 0..atlas.genesis.hydrology.len() {
        let terrain = atlas.genesis.terrain.values()[index];
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let climate = atlas.genesis.climate.values()[index];
        let hydro = atlas.genesis.hydrology.values()[index];
        let bucket = if climate.aridity < 0.72 {
            &mut wet
        } else if climate.aridity > 1.35 {
            &mut dry
        } else {
            continue;
        };
        bucket.0 += 1;
        bucket.1 += u64::from(u8::from(hydro.flags & HYDRO_RIVER != 0));
        bucket.2 += u64::from(u8::from(hydro.flags & HYDRO_PERENNIAL != 0));
    }
    let wet_density = wet.1 as f64 / wet.0.max(1) as f64;
    let dry_density = dry.1 as f64 / dry.0.max(1) as f64;
    let wet_perennial = wet.2 as f64 / wet.1.max(1) as f64;
    let dry_perennial = dry.2 as f64 / dry.1.max(1) as f64;
    assert!(
        wet_density > dry_density,
        "wet={wet_density} dry={dry_density}"
    );
    assert!(
        wet_perennial > dry_perennial,
        "wet perennial={wet_perennial} dry={dry_perennial}"
    );
}

#[test]
fn mountain_rain_shadows_reduce_runoff_and_perennial_flow() {
    let atlas = atlas();
    let mut windward_runoff = Vec::new();
    let mut leeward_runoff = Vec::new();
    let mut windward_perennial = 0u64;
    let mut leeward_perennial = 0u64;
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let summit = atlas.climate_downstream(pos, 54.0);
        let lee = atlas.climate_downstream(summit, 54.0);
        let summit_elevation = atlas.genesis.terrain.get(summit).unwrap().eroded_elevation;
        let lee_elevation = atlas.genesis.terrain.get(lee).unwrap().eroded_elevation;
        if summit_elevation > terrain.eroded_elevation + 8.0
            && lee_elevation < summit_elevation - 5.0
        {
            let wet = atlas.genesis.hydrology.get(pos).unwrap();
            let dry = atlas.genesis.hydrology.get(lee).unwrap();
            windward_runoff.push(f64::from(wet.mean_runoff));
            leeward_runoff.push(f64::from(dry.mean_runoff));
            windward_perennial += u64::from(u8::from(wet.flags & HYDRO_PERENNIAL != 0));
            leeward_perennial += u64::from(u8::from(dry.flags & HYDRO_PERENNIAL != 0));
        }
    }
    assert!(windward_runoff.len() >= 12);
    let wet = average(windward_runoff.into_iter());
    let dry = average(leeward_runoff.into_iter());
    assert!(wet > dry * 1.08, "windward runoff={wet} leeward={dry}");
    assert!(windward_perennial >= leeward_perennial);
}
