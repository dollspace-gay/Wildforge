//! Qualification site selection for weather.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{Direction4, geodesic_distance};
use crate::planet_atlas::{AtlasPos, PlanetAtlas, PlanetaryWeather};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    // One accepted local-weather hour supplies two positions from the same
    // authoritative state for in-world simultaneous-weather captures.
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    if weather.complete_hour(atlas, 0.0, 0, 0.0).is_ok() {
        let local_relief = |pos: AtlasPos, elevation: f32| {
            Direction4::ALL
                .into_iter()
                .filter_map(|direction| {
                    atlas
                        .genesis
                        .terrain
                        .get(pos.step(direction, atlas.side()).pos)
                })
                .map(|neighbor| (neighbor.eroded_elevation - elevation).abs())
                .fold(0.0f32, f32::max)
        };
        // Qualification sites are also capture sites. Prefer a broad, gentle
        // shelf where falling weather is readable; retain a wet-land fallback
        // so diagnostics still prove locality on unusually rugged seeds.
        let preferred_rain = atlas
            .genesis
            .terrain
            .iter()
            .filter_map(|(pos, terrain)| {
                let cell = *weather.cells.cells.get(pos)?;
                let relief = local_relief(pos, terrain.eroded_elevation);
                (terrain.eroded_elevation > SEA_LEVEL as f32 + 4.0
                    && terrain.eroded_elevation < SEA_LEVEL as f32 + 48.0
                    && relief <= 6.0
                    && cell.precipitation_rate > 0)
                    .then_some((pos, cell, relief))
            })
            .max_by_key(|(_, cell, _)| cell.precipitation_rate);
        let rain = preferred_rain.or_else(|| {
            atlas
                .genesis
                .terrain
                .iter()
                .filter(|(_, terrain)| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
                .filter_map(|(pos, terrain)| {
                    Some((
                        pos,
                        *weather.cells.cells.get(pos)?,
                        local_relief(pos, terrain.eroded_elevation),
                    ))
                })
                .filter(|(_, cell, _)| cell.precipitation_rate > 0)
                .max_by_key(|(_, cell, _)| cell.precipitation_rate)
        });
        if let Some((rain_pos, rain_cell, relief)) = rain {
            insert(
                "weather_raining_hour_1",
                rain_pos,
                format!(
                    "same hour as weather_clear_hour_1; precipitation units {}; storm energy {}; local relief {:.1} blocks",
                    rain_cell.precipitation_rate, rain_cell.storm_energy, relief
                ),
            );
            if let Some((clear_pos, clear_cell)) = atlas
                .genesis
                .terrain
                .iter()
                .filter(|(pos, terrain)| {
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && geodesic_distance(
                            pos.center(atlas.side()),
                            rain_pos.center(atlas.side()),
                        ) > 3_000.0
                })
                .filter_map(|(pos, _)| {
                    let cell = weather.cells.cells.get(pos)?;
                    (cell.precipitation_rate == 0).then_some((pos, cell))
                })
                .min_by_key(|(_, cell)| cell.cloud_water)
            {
                insert(
                    "weather_clear_hour_1",
                    clear_pos,
                    format!(
                        "same hour as weather_raining_hour_1; cloud water {}; storm energy {}",
                        clear_cell.cloud_water, clear_cell.storm_energy
                    ),
                );
            }
        }
    }
}
