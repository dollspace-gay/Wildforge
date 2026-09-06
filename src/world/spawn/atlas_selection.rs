//! Atlas selection for the common spawn contract.

use super::FRESH_WATER_ATLAS_STEPS;
use super::HEART_PROTECTION_BLOCKS;
use super::MAX_VOXEL_CANDIDATES;
use super::MIN_FERTILITY;
use super::MIN_VEGETATION;
use super::PREFERRED_COPPER_ACCESS_BLOCKS;
use super::PREFERRED_TIN_ACCESS_BLOCKS;
use super::SpawnCandidate;
use super::SpawnSelectionDiagnostics;
use crate::chunk::SEA_LEVEL;
use crate::planet::FACE_BLOCKS;
use crate::planet::SurfacePos;
use crate::planet_atlas::BIOME_FOREST;
use crate::planet_atlas::BIOME_JUNGLE;
use crate::planet_atlas::BIOME_TAIGA;
use crate::planet_atlas::EDAPHIC_SHALLOW_ROCK;
use crate::planet_atlas::MineralKind;
use crate::planet_atlas::PlanetAtlas;
use crate::planet_atlas::WaterBodyKind;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

pub(super) fn fresh_water_distances(atlas: &PlanetAtlas) -> Vec<u16> {
    let side = atlas.side();
    let mut distance = vec![u16::MAX; atlas.genesis.hydrology.len()];
    let mut queue = VecDeque::new();
    for (pos, water) in atlas.genesis.hydrology.iter() {
        let drinkable = water.salinity < 48
            && water.baseline_water_units > 0
            && matches!(
                water.water_body,
                WaterBodyKind::River
                    | WaterBodyKind::Lake
                    | WaterBodyKind::Delta
                    | WaterBodyKind::Wetland
            );
        if drinkable {
            distance[pos.index(side)] = 0;
            queue.push_back(pos);
        }
    }
    while let Some(pos) = queue.pop_front() {
        let next_distance = distance[pos.index(side)].saturating_add(1);
        if next_distance > FRESH_WATER_ATLAS_STEPS {
            continue;
        }
        for neighbor in pos.neighbors4(side) {
            let index = neighbor.index(side);
            if next_distance < distance[index] {
                distance[index] = next_distance;
                queue.push_back(neighbor);
            }
        }
    }
    distance
}

pub(super) fn traversable_component_sizes(atlas: &PlanetAtlas) -> Vec<usize> {
    let side = atlas.side();
    let count = atlas.genesis.terrain.len();
    let passable = (0..count)
        .map(|index| {
            let terrain = atlas.genesis.terrain.values()[index];
            let water = atlas.genesis.hydrology.values()[index];
            terrain.landmass_id != 0
                && terrain.eroded_elevation > (SEA_LEVEL + 2) as f32
                && water.water_body == WaterBodyKind::Land
        })
        .collect::<Vec<_>>();
    let mut component = vec![usize::MAX; count];
    let mut sizes = Vec::new();
    for start in 0..count {
        if !passable[start] || component[start] != usize::MAX {
            continue;
        }
        let id = sizes.len();
        component[start] = id;
        let mut queue =
            VecDeque::from([crate::planet_atlas::AtlasPos::from_index(start, side)
                .expect("atlas component index")]);
        let mut size = 0usize;
        while let Some(pos) = queue.pop_front() {
            size += 1;
            let elevation = atlas
                .genesis
                .terrain
                .get(pos)
                .expect("atlas component terrain")
                .eroded_elevation;
            for neighbor in pos.neighbors4(side) {
                let index = neighbor.index(side);
                if !passable[index] || component[index] != usize::MAX {
                    continue;
                }
                let next_elevation = atlas
                    .genesis
                    .terrain
                    .get(neighbor)
                    .expect("atlas neighbor terrain")
                    .eroded_elevation;
                if (elevation - next_elevation).abs() > 28.0 {
                    continue;
                }
                component[index] = id;
                queue.push_back(neighbor);
            }
        }
        sizes.push(size);
    }
    component
        .into_iter()
        .map(|id| sizes.get(id).copied().unwrap_or_default())
        .collect()
}

pub(super) fn qualified_atlas_candidates(
    atlas: &PlanetAtlas,
) -> (Vec<SpawnCandidate>, SpawnSelectionDiagnostics) {
    let side = atlas.side();
    let fresh_water = fresh_water_distances(atlas);
    let traversable = traversable_component_sizes(atlas);
    let mut landmass_cells = HashMap::<u16, usize>::new();
    for terrain in atlas.genesis.terrain.values() {
        if terrain.landmass_id != 0 {
            *landmass_cells.entry(terrain.landmass_id).or_default() += 1;
        }
    }
    let minimum_landmass_cells = usize::from((side / 2).clamp(2, 16));
    let minimum_traversable_cells = usize::from((side / 2).clamp(8, 64));
    let mut candidates = Vec::new();
    let mut diagnostics = SpawnSelectionDiagnostics::default();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        diagnostics.atlas_cells += 1;
        let index = pos.index(side);
        let water = &atlas.genesis.hydrology.values()[index];
        let biome = &atlas.genesis.biomes.values()[index];
        let ground = &atlas.genesis.ground.values()[index];
        if terrain.landmass_id == 0
            || landmass_cells
                .get(&terrain.landmass_id)
                .copied()
                .unwrap_or_default()
                < minimum_landmass_cells
            || terrain.eroded_elevation <= (SEA_LEVEL + 2) as f32
            || water.water_body != WaterBodyKind::Land
            || fresh_water[index] > FRESH_WATER_ATLAS_STEPS
            || biome.vegetation_potential < MIN_VEGETATION
            || ground.baseline_fertility < MIN_FERTILITY
            || !matches!(
                biome.baseline_biome,
                BIOME_FOREST | BIOME_JUNGLE | BIOME_TAIGA
            )
            || biome.edaphic_flags & EDAPHIC_SHALLOW_ROCK != 0
            || f32::from(biome.tree_line_y) <= terrain.eroded_elevation + 8.0
        {
            continue;
        }
        diagnostics.viable_habitat += 1;
        if traversable[index] < minimum_traversable_cells {
            continue;
        }
        diagnostics.connected_land += 1;
        let maximum_relief = pos
            .neighbors8(side)
            .into_iter()
            .map(|neighbor| {
                (terrain.eroded_elevation
                    - atlas
                        .genesis
                        .terrain
                        .get(neighbor)
                        .expect("atlas neighbor")
                        .eroded_elevation)
                    .abs()
            })
            .fold(0.0f32, f32::max);
        if maximum_relief > 28.0 {
            continue;
        }
        diagnostics.manageable_relief += 1;
        let point = pos.center(side);
        let deposit_distance = |kind| {
            atlas
                .geology
                .deposits
                .iter()
                .filter(|deposit| deposit.mineral == kind)
                .map(|deposit| crate::planet::geodesic_distance(point, deposit.pos.center(side)))
                .fold(f64::INFINITY, f64::min)
        };
        let copper_distance = deposit_distance(MineralKind::Copper);
        let tin_distance = deposit_distance(MineralKind::Tin);
        let pair = (copper_distance.round() as u32, tin_distance.round() as u32);
        if diagnostics.best_resource_pair_blocks.is_none_or(|current| {
            pair.0.max(pair.1) < current.0.max(current.1)
                || (pair.0.max(pair.1) == current.0.max(current.1) && pair < current)
        }) {
            diagnostics.best_resource_pair_blocks = Some(pair);
        }
        let has_resources = atlas
            .nearest_deposit(point, MineralKind::Copper, PREFERRED_COPPER_ACCESS_BLOCKS)
            .is_some()
            && atlas
                .nearest_deposit(point, MineralKind::Tin, PREFERRED_TIN_ACCESS_BLOCKS)
                .is_some();
        diagnostics.resources_before_protection += usize::from(has_resources);
        if has_resources {
            diagnostics.resource_site = Some((point.face, point.u as u16, point.v as u16));
        }
        let near_heart = atlas.country(biome.country_id).is_some_and(|country| {
            crate::planet::geodesic_distance(point, country.heart_site.center(side))
                < HEART_PROTECTION_BLOCKS
        });
        let near_volcano = atlas.geology.volcanoes.iter().any(|volcano| {
            crate::planet::geodesic_distance(point, volcano.pos.center(side))
                <= f64::from(volcano.edifice_radius_blocks) + 48.0
        });
        diagnostics.resource_sites_near_heart += usize::from(has_resources && near_heart);
        diagnostics.resource_sites_near_volcano += usize::from(has_resources && near_volcano);
        if near_heart || near_volcano {
            continue;
        }
        diagnostics.outside_protected_sites += 1;
        if diagnostics
            .best_unprotected_resource_pair_blocks
            .is_none_or(|current| {
                pair.0.max(pair.1) < current.0.max(current.1)
                    || (pair.0.max(pair.1) == current.0.max(current.1) && pair < current)
            })
        {
            diagnostics.best_unprotected_resource_pair_blocks = Some(pair);
        }
        diagnostics.resources_after_protection += usize::from(has_resources);
        let surface = SurfacePos::new(
            point.face,
            point.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            point.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
        )
        .expect("atlas cell center is a canonical surface position");
        let score = i64::from(biome.vegetation_potential) * 8
            + i64::from(ground.baseline_fertility) * 6
            + i64::from(ground.organic) * 2
            + i64::from(ground.soil_depth_decimeters) * 4
            + i64::from(FRESH_WATER_ATLAS_STEPS - fresh_water[index]) * 80
            - (maximum_relief * 12.0).round() as i64
            - (terrain.eroded_elevation - 82.0).abs().round() as i64;
        candidates.push(SpawnCandidate {
            surface,
            score,
            fresh_water_steps: fresh_water[index],
            copper_distance_blocks: pair.0,
            tin_distance_blocks: pair.1,
        });
    }
    (candidates, diagnostics)
}

/// Atlas summaries cannot prove that fine voxels put a trunk and water on the
/// same walkable component. Interleave independent notions of a good homeland
/// instead of spending the whole bounded search on one geographic cluster.
pub(super) fn spawn_candidate_portfolio(candidates: &[SpawnCandidate]) -> Vec<SpawnCandidate> {
    let habitat_order = |left: &SpawnCandidate, right: &SpawnCandidate| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.fresh_water_steps.cmp(&right.fresh_water_steps))
            .then_with(|| {
                right
                    .preferred_resource_hinterland()
                    .cmp(&left.preferred_resource_hinterland())
            })
            .then_with(|| {
                left.farthest_bootstrap_resource()
                    .cmp(&right.farthest_bootstrap_resource())
            })
            .then_with(|| left.surface.cmp(&right.surface))
    };
    let mut water = candidates.to_vec();
    water.sort_by(|left, right| {
        left.fresh_water_steps
            .cmp(&right.fresh_water_steps)
            .then_with(|| habitat_order(left, right))
    });
    let mut habitat = candidates.to_vec();
    habitat.sort_by(habitat_order);
    let mut bootstrap = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.close_bootstrap_hinterland())
        .collect::<Vec<_>>();
    bootstrap.sort_by(habitat_order);

    let orders = [&water, &habitat, &bootstrap];
    let mut selected = Vec::new();
    let mut surfaces = HashSet::new();
    for rank in 0..orders.iter().map(|order| order.len()).max().unwrap_or(0) {
        for order in orders {
            let Some(candidate) = order.get(rank).copied() else {
                continue;
            };
            if surfaces.insert(candidate.surface) {
                selected.push(candidate);
                if selected.len() == MAX_VOXEL_CANDIDATES {
                    return selected;
                }
            }
        }
    }
    selected
}
