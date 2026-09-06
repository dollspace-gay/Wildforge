//! Qualification site selection for geology.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasPos, DetailedBoundary, PlanetAtlas, VolcanoSource};

pub(super) fn collect(atlas: &PlanetAtlas, insert: &mut impl FnMut(&str, AtlasPos, String)) {
    let strongest = |detail: DetailedBoundary, land: Option<bool>, maximum: bool| {
        atlas
            .genesis
            .terrain
            .iter()
            .filter(|(pos, terrain)| {
                let tectonics = atlas
                    .genesis
                    .tectonics
                    .get(*pos)
                    .expect("matching atlas grids");
                tectonics.boundary_detail == detail
                    && land.is_none_or(|required| {
                        (terrain.eroded_elevation > SEA_LEVEL as f32) == required
                    })
            })
            .max_by(|(a_pos, a), (b_pos, b)| {
                let a_value = if maximum {
                    a.tectonic_contribution
                } else {
                    -a.tectonic_contribution
                };
                let b_value = if maximum {
                    b.tectonic_contribution
                } else {
                    -b.tectonic_contribution
                };
                a_value
                    .partial_cmp(&b_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a_pos.index(atlas.side()).cmp(&b_pos.index(atlas.side())))
            })
            .map(|(pos, terrain)| (pos, *terrain))
    };

    if let Some((pos, terrain)) =
        strongest(DetailedBoundary::ContinentalCollision, Some(true), true)
    {
        insert(
            "continent_collision_range",
            pos,
            format!(
                "continental collision; tectonic relief +{:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some((pos, terrain)) = strongest(
        DetailedBoundary::OceanContinentSubduction,
        Some(false),
        false,
    ) {
        insert(
            "subduction_trench",
            pos,
            format!(
                "ocean-continent subduction; tectonic relief {:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some(volcano) = atlas
        .geology
        .volcanoes
        .iter()
        .filter(|site| {
            site.source == VolcanoSource::ContinentalArc
                && atlas
                    .genesis
                    .terrain
                    .get(site.pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by_key(|site| site.edifice_height_blocks)
    {
        insert(
            "coastal_volcanic_arc",
            volcano.pos,
            format!(
                "continental arc volcano {}; edifice {} blocks",
                volcano.id, volcano.edifice_height_blocks
            ),
        );
    }
    if let Some((pos, terrain)) = strongest(DetailedBoundary::ContinentalRift, Some(true), false) {
        insert(
            "continental_rift_valley",
            pos,
            format!(
                "continental rift; tectonic relief {:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some(volcano) = atlas
        .geology
        .volcanoes
        .iter()
        .filter(|site| {
            site.source == VolcanoSource::IslandArc
                && atlas
                    .genesis
                    .terrain
                    .get(site.pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by_key(|site| site.edifice_height_blocks)
    {
        insert(
            "island_arc",
            volcano.pos,
            format!(
                "island arc volcano {}; edifice {} blocks",
                volcano.id, volcano.edifice_height_blocks
            ),
        );
    }
    if let Some(chain_id) = atlas
        .geology
        .volcanoes
        .iter()
        .find(|site| site.source == VolcanoSource::Hotspot)
        .map(|site| site.chain_id)
    {
        let chain: Vec<_> = atlas
            .geology
            .volcanoes
            .iter()
            .filter(|site| site.source == VolcanoSource::Hotspot && site.chain_id == chain_id)
            .collect();
        if let Some(youngest) = chain.iter().min_by_key(|site| site.age_myr) {
            insert(
                "hotspot_chain_youngest",
                youngest.pos,
                format!("hotspot chain {chain_id}; age {} Myr", youngest.age_myr),
            );
        }
        if let Some(oldest) = chain.iter().max_by_key(|site| site.age_myr) {
            insert(
                "hotspot_chain_oldest",
                oldest.pos,
                format!(
                    "hotspot chain {chain_id}; age {} Myr; erosion {}",
                    oldest.age_myr, oldest.erosion
                ),
            );
        }
    }
    if let Some((pos, cell)) = atlas
        .genesis
        .tectonics
        .iter()
        .filter(|(_, cell)| {
            cell.boundary_detail == DetailedBoundary::ContinentalCollision
                && (1..=5).contains(&cell.boundary_distance)
        })
        .max_by_key(|(_, cell)| cell.fault_intensity)
    {
        insert(
            "folded_strata",
            pos,
            format!(
                "collision fold belt; strike {:?}; fault intensity {}",
                cell.boundary_strike, cell.fault_intensity
            ),
        );
    }
    if let Some(intrusion) = atlas
        .geology
        .intrusions
        .iter()
        .min_by_key(|site| site.top_depth_blocks)
    {
        insert(
            "contact_aureole",
            intrusion.pos,
            format!(
                "{:?} intrusion {}; top depth {}; contact radius {} blocks",
                intrusion.kind,
                intrusion.id,
                intrusion.top_depth_blocks,
                intrusion.contact_radius_blocks
            ),
        );
    }
}
