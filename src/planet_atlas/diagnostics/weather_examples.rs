//! Diagnostic weather tracks and maps from an isolated weather owner.

use super::images::write_png;
use crate::planet_atlas::{
    AtlasError, LocalWeather, PlanetAtlas, PlanetaryWeather, weather_sample,
};
use glam::DVec3;
use std::path::Path;
pub(in crate::planet_atlas::diagnostics) fn export_weather_examples(
    atlas: &PlanetAtlas,
    map_dir: &Path,
    tracks_path: &Path,
) -> Result<(), AtlasError> {
    let side = u32::from(atlas.side());
    let width = side * 3;
    let height = side * 2;
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let mut tracks = String::from(
        "hour,storm_weight,centroid_x,centroid_y,centroid_z,clear_cells,overcast_cells,precipitation_cells,storm_cells,precipitation_units,water_drift\n",
    );
    for hour in 1..=24u64 {
        let day = hour as f64 / 24.0;
        let report = weather.complete_hour(atlas, day, hour - 1, 0.0)?;
        let mut center = DVec3::ZERO;
        let mut weight = 0.0f64;
        let mut weather_counts = [0u64; 4];
        for (pos, geometry) in atlas.genesis.geometry.iter() {
            let storm = f64::from(
                weather
                    .cells
                    .cells
                    .get(pos)
                    .expect("matching weather grids")
                    .storm_energy,
            );
            center += DVec3::from_array(geometry.unit_direction.map(f64::from)) * storm;
            weight += storm;
            let sample = weather_sample(
                atlas.genesis.climate.values()[pos.index(atlas.side())],
                weather.cells.cells.values()[pos.index(atlas.side())],
                day,
                false,
            );
            weather_counts[sample.kind as usize] += 1;
        }
        let center = if weight > 0.0 {
            (center / weight).normalize_or_zero()
        } else {
            DVec3::ZERO
        };
        tracks.push_str(&format!(
            "{hour},{weight:.3},{:.8},{:.8},{:.8},{},{},{},{},{},{}\n",
            center.x,
            center.y,
            center.z,
            weather_counts[LocalWeather::Clear as usize],
            weather_counts[LocalWeather::Overcast as usize],
            weather_counts[LocalWeather::Precipitation as usize],
            weather_counts[LocalWeather::Storm as usize],
            report.precipitation_units,
            report.unexplained_water_drift,
        ));
        if [1, 6, 12, 24].contains(&hour) {
            let mut pixels = vec![0u8; width as usize * height as usize * 3];
            for (pos, _) in atlas.genesis.geometry.iter() {
                let index = pos.index(atlas.side());
                let sample = weather_sample(
                    atlas.genesis.climate.values()[index],
                    weather.cells.cells.values()[index],
                    day,
                    false,
                );
                let color = match sample.kind {
                    LocalWeather::Clear => [50, 105, 175],
                    LocalWeather::Overcast => [125, 132, 142],
                    LocalWeather::Precipitation => [35, 175, 215],
                    LocalWeather::Storm => [130, 45, 170],
                };
                let face_index = pos.face.index() as u32;
                let x = (face_index % 3) * side + u32::from(pos.u);
                let y = (face_index / 3) * side + u32::from(pos.v);
                let target = (y * width + x) as usize * 3;
                pixels[target..target + 3].copy_from_slice(&color);
            }
            write_png(
                &map_dir.join(format!("weather_hour_{hour:02}.png")),
                width,
                height,
                &pixels,
            )?;
        }
    }
    crate::persist::atomic_write(tracks_path, tracks.as_bytes(), false)?;
    Ok(())
}
