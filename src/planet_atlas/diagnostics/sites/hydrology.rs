//! Qualification site selection for hydrology.

use crate::planet_atlas::{AtlasPos, HYDRO_DELTA, HYDRO_ESTUARY, PlanetAtlas};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    if let Some(river) = atlas.hydrology.rivers.iter().max_by(|a, b| {
        a.maximum_discharge
            .total_cmp(&b.maximum_discharge)
            .then_with(|| b.id.cmp(&a.id))
    }) {
        insert(
            "major_river_source",
            river.source,
            format!(
                "{} source; {:.0} blocks long; order {}",
                river.name, river.length_blocks, river.stream_order
            ),
        );
        insert(
            "major_river_mouth",
            river.mouth,
            format!(
                "{} mouth into {}; discharge {:.2}; width {:.1}",
                river.name, river.sink_name, river.maximum_discharge, river.maximum_width_blocks
            ),
        );
    }
    if let Some(river) = atlas.hydrology.rivers.iter().find(|river| {
        river
            .path
            .windows(2)
            .any(|edge| edge[0].face != edge[1].face)
    }) && let Some(crossing) = river
        .path
        .windows(2)
        .find(|edge| edge[0].face != edge[1].face)
    {
        insert(
            "river_face_seam",
            crossing[0],
            format!(
                "{} crosses from {} to {}",
                river.name, crossing[0].face, crossing[1].face
            ),
        );
    }
    if let Some(lake) = atlas
        .hydrology
        .lakes
        .iter()
        .find(|lake| lake.outlet.is_some())
    {
        insert(
            "through_flow_lake",
            lake.sink,
            format!(
                "{}; {:?}; surface {:.2}; spill {:.2}; outlet {}",
                lake.name,
                lake.class,
                lake.surface_elevation,
                lake.spill_elevation,
                lake.outlet.expect("filtered outlet").face
            ),
        );
    }
    if let Some(lake) = atlas
        .hydrology
        .lakes
        .iter()
        .filter(|lake| lake.outlet.is_none())
        .max_by_key(|lake| lake.salinity)
    {
        insert(
            "terminal_salt_lake",
            lake.sink,
            format!(
                "{}; {:?}; salinity {}; seasonal range {:.2}",
                lake.name, lake.class, lake.salinity, lake.seasonal_level_range
            ),
        );
    }
    if let Some((pos, cell)) = atlas
        .genesis
        .hydrology
        .iter()
        .filter(|(pos, cell)| {
            let climate = atlas.genesis.climate.get(*pos).expect("grid");
            cell.flags & (HYDRO_DELTA | HYDRO_ESTUARY) != 0
                && climate.mean_temperature >= 10.0
                && climate.snow_persistence < 0.08
        })
        .max_by(|(_, a), (_, b)| a.mean_discharge.total_cmp(&b.mean_discharge))
    {
        insert(
            "delta_or_estuary",
            pos,
            format!(
                "{:?}; discharge {:.2}; salinity {}",
                cell.water_body, cell.mean_discharge, cell.salinity
            ),
        );
    }
}
