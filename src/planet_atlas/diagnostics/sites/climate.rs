//! Qualification site selection for climate.

use crate::chunk::{SEA_LEVEL};
use crate::planet_atlas::{AtlasPos, PlanetAtlas};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    if let Some((pos, climate)) = atlas
        .genesis
        .climate
        .iter()
        .filter(|(pos, _)| {
            atlas
                .genesis
                .terrain
                .get(*pos)
                .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by(|(_, a), (_, b)| a.mean_precipitation.total_cmp(&b.mean_precipitation))
    {
        insert(
            "wettest_land",
            pos,
            format!(
                "annual precipitation {:.1}; aridity {:.3}",
                climate.mean_precipitation, climate.aridity
            ),
        );
    }
    if let Some((pos, climate)) = atlas
        .genesis
        .climate
        .iter()
        .filter(|(pos, climate)| {
            climate.mean_temperature > 18.0
                && atlas
                    .genesis
                    .terrain
                    .get(*pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by(|(_, a), (_, b)| a.aridity.total_cmp(&b.aridity))
    {
        insert(
            "driest_warm_land",
            pos,
            format!(
                "aridity {:.3}; annual precipitation {:.1}; temperature {:.1} C",
                climate.aridity, climate.mean_precipitation, climate.mean_temperature
            ),
        );
    }
    if let Some((windward, leeward, drop)) = atlas
        .genesis
        .terrain
        .iter()
        .filter(|(_, terrain)| terrain.eroded_elevation > SEA_LEVEL as f32)
        .filter_map(|(pos, _)| {
            let leeward = atlas.climate_downstream(pos, 54.0);
            let wet_climate = atlas.genesis.climate.get(pos)?;
            let dry_climate = atlas.genesis.climate.get(leeward)?;
            if wet_climate.mean_temperature < 10.0
                || dry_climate.mean_temperature < 10.0
                || wet_climate.snow_persistence > 0.08
                || dry_climate.snow_persistence > 0.08
            {
                return None;
            }
            let wet = wet_climate.seasonal_precipitation[1];
            let dry = dry_climate.seasonal_precipitation[1];
            (wet > dry).then_some((pos, leeward, wet - dry))
        })
        .max_by(|(_, _, a), (_, _, b)| a.total_cmp(b))
    {
        insert(
            "rain_shadow_windward",
            windward,
            format!("summer windward precipitation drop {drop:.1}"),
        );
        insert(
            "rain_shadow_leeward",
            leeward,
            format!("downwind of rain-shadow pair; precipitation drop {drop:.1}"),
        );
    }

}
