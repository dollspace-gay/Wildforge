//! Conserved downstream placement of finite geological resource deposits.

use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasGrid, AtlasPos, BedrockFamily, DepositRecord, GeologyModel, HydrologyCell, MineralKind, ResourceCell, TectonicCell, TerrainCell};
use super::{HYDRO_DELTA, HYDRO_FLOODPLAIN};

pub(in crate::planet_atlas) fn route_placer_deposits(
    side: u16,
    terrain: &AtlasGrid<TerrainCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    resources: &mut AtlasGrid<ResourceCell>,
    geology: &mut GeologyModel,
) {
    let sources: Vec<(usize, DepositRecord)> = geology
        .deposits
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, site)| matches!(site.mineral, MineralKind::Gold | MineralKind::RareEarth))
        .collect();
    for (source_index, source) in sources {
        // Divert one block of the source's per-chunk extraction ceiling into
        // a small downstream placer. Primary tonnage is generated exactly at
        // its declared ceiling, so looking only for nonexistent "excess"
        // would make this route unreachable.
        let retained_quota = source.max_blocks_per_chunk.saturating_sub(1);
        if retained_quota == 0 {
            continue;
        }
        let minimum_source_budget =
            u64::from(retained_quota) * u64::from(source.eligible_chunk_upper_bound);
        let available = source.tonnage_blocks.saturating_sub(minimum_source_budget);
        if available < 64 {
            continue;
        }
        let mut at = source.pos;
        let mut target = None;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..usize::from(side).saturating_mul(6) {
            if !seen.insert(at) {
                break;
            }
            let cell = hydrology.get(at).expect("placer drainage");
            if at != source.pos
                && cell.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0
                && terrain
                    .get(at)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32)
            {
                target = Some(at);
                break;
            }
            let Some(next) = (cell.drainage_receiver != u32::MAX)
                .then(|| AtlasPos::from_index(cell.drainage_receiver as usize, side))
                .flatten()
            else {
                break;
            };
            at = next;
        }
        let Some(target) = target else {
            continue;
        };
        let upper = 32u32;
        let per_chunk = 2u16;
        let placer_tonnage = available.min(u64::from(upper) * u64::from(per_chunk));
        if placer_tonnage < u64::from(upper) * u64::from(per_chunk) {
            continue;
        }
        geology.deposits[source_index].max_blocks_per_chunk = retained_quota;
        geology.deposits[source_index].tonnage_blocks -= placer_tonnage;
        let id = geology.deposits.len() as u32 + 1;
        let target_tectonics = tectonics.get(target).expect("placer host");
        geology.deposits.push(DepositRecord {
            id,
            mineral: source.mineral,
            pos: target,
            host: BedrockFamily::from_id(target_tectonics.bedrock_family),
            geological_province: target_tectonics.geological_province,
            landmass_id: terrain.get(target).expect("placer terrain").landmass_id,
            source_body_id: source.id,
            radius_blocks: 72,
            depth_min: SEA_LEVEL.max(1) as u16,
            depth_max: (SEA_LEVEL + 18) as u16,
            grade_ppm: source.grade_ppm.saturating_div(3).max(1),
            tonnage_blocks: placer_tonnage,
            max_blocks_per_chunk: per_chunk,
            eligible_chunk_upper_bound: upper,
        });
        let resource = resources.get_mut(target).expect("placer resource cell");
        if resource.deposit_site_ref == 0 {
            resource.deposit_site_ref = id;
        }
        resource.deposit_site_count = resource.deposit_site_count.saturating_add(1);
    }
}
