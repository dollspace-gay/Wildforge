//! Deterministic climate transects and river profile exports.

use crate::chunk::SEA_LEVEL;
use crate::planet::geodesic_distance;
use crate::planet_atlas::{AtlasError, AtlasPos, HYDRO_RIVER, PlanetAtlas};
use std::collections::BTreeSet;
use std::path::Path;

pub(in crate::planet_atlas::diagnostics) fn export_climate_transects(
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), AtlasError> {
    let mut candidates = Vec::new();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let summit = atlas.climate_downstream(pos, 54.0);
        let lee = atlas.climate_downstream(summit, 54.0);
        let summit_elevation = atlas
            .genesis
            .terrain
            .get(summit)
            .expect("matching atlas grids")
            .eroded_elevation;
        let lee_elevation = atlas
            .genesis
            .terrain
            .get(lee)
            .expect("matching atlas grids")
            .eroded_elevation;
        let score = (summit_elevation - terrain.eroded_elevation).max(0.0)
            + (summit_elevation - lee_elevation).max(0.0);
        if score > 10.0 {
            candidates.push((score, pos));
        }
    }
    candidates.sort_by(|(a_score, a_pos), (b_score, b_pos)| {
        b_score
            .total_cmp(a_score)
            .then_with(|| a_pos.index(atlas.side()).cmp(&b_pos.index(atlas.side())))
    });
    let mut starts = Vec::new();
    for (_, candidate) in candidates {
        if starts.iter().all(|existing: &AtlasPos| {
            geodesic_distance(
                existing.center(atlas.side()),
                candidate.center(atlas.side()),
            ) > 1_000.0
        }) {
            starts.push(candidate);
            if starts.len() == 6 {
                break;
            }
        }
    }
    let mut csv = String::from(
        "transect,step,face,u,v,distance_blocks,elevation,wind_east,wind_north,moisture,precipitation,temperature,aridity\n",
    );
    for (transect, start) in starts.into_iter().enumerate() {
        let mut at = start;
        let mut seen = BTreeSet::new();
        let mut distance = 0.0;
        for step in 0..16 {
            if !seen.insert(at) {
                break;
            }
            let terrain = atlas.genesis.terrain.get(at).expect("matching atlas grids");
            let climate = atlas.genesis.climate.get(at).expect("matching atlas grids");
            csv.push_str(&format!(
                "{transect},{step},{},{},{},{distance:.3},{:.3},{:.6},{:.6},{:.3},{:.3},{:.3},{:.6}\n",
                at.face.name(),
                at.u,
                at.v,
                terrain.eroded_elevation,
                climate.seasonal_wind[1][0],
                climate.seasonal_wind[1][1],
                climate.mean_atmospheric_moisture,
                climate.seasonal_precipitation[1],
                climate.seasonal_temperature[1],
                climate.aridity,
            ));
            let next = atlas.climate_downstream(at, 54.0);
            distance += geodesic_distance(at.center(atlas.side()), next.center(atlas.side()));
            at = next;
        }
    }
    crate::persist::atomic_write(path, csv.as_bytes(), false)?;
    Ok(())
}

pub(in crate::planet_atlas::diagnostics) fn export_river_profiles(
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), AtlasError> {
    let mut selected: Vec<_> = atlas.hydrology.rivers.iter().collect();
    selected.sort_by(|a, b| {
        b.maximum_discharge
            .total_cmp(&a.maximum_discharge)
            .then_with(|| a.id.cmp(&b.id))
    });
    selected.truncate(16);
    let mut tributaries = vec![0u16; atlas.genesis.hydrology.len()];
    for cell in atlas.genesis.hydrology.values() {
        if cell.drainage_receiver != u32::MAX && cell.flags & HYDRO_RIVER != 0 {
            tributaries[cell.drainage_receiver as usize] =
                tributaries[cell.drainage_receiver as usize].saturating_add(1);
        }
    }
    let mut csv = String::from(
        "river_id,river_name,step,face,u,v,distance_blocks,terrain_elevation,bed_elevation,water_surface,discharge,width,depth,stream_order,tributaries,lake_id,ocean_id,sink_name\n",
    );
    for river in selected {
        let mut distance = 0.0;
        for (step, pos) in river.path.iter().copied().enumerate() {
            if step > 0 {
                distance += geodesic_distance(
                    river.path[step - 1].center(atlas.side()),
                    pos.center(atlas.side()),
                );
            }
            let terrain = atlas
                .genesis
                .terrain
                .get(pos)
                .expect("river profile terrain");
            let hydro = atlas
                .genesis
                .hydrology
                .get(pos)
                .expect("river profile hydrology");
            csv.push_str(&format!(
                "{},{},{step},{},{},{},{distance:.3},{:.3},{:.3},{:.3},{:.5},{:.3},{:.3},{},{},{},{},{}\n",
                river.id,
                river.name,
                pos.face.name(),
                pos.u,
                pos.v,
                terrain.eroded_elevation,
                hydro.channel_bed_elevation,
                hydro.water_surface_elevation,
                hydro.mean_discharge,
                f32::from(hydro.channel_width_centiblocks) / 100.0,
                f32::from(hydro.channel_depth_centiblocks) / 100.0,
                hydro.stream_order,
                tributaries[pos.index(atlas.side())],
                hydro.lake_basin_id,
                hydro.ocean_basin_id,
                river.sink_name,
            ));
        }
    }
    crate::persist::atomic_write(path, csv.as_bytes(), false)?;
    Ok(())
}
