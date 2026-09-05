//! Shared planetary spawn selection.
//!
//! The selector is deliberately renderer- and transport-independent: solo,
//! windowed hosts, and dedicated hosts must all begin at the same doorstep.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use super::preparation::{check_cancelled, generate_trial_region};

use super::*;
use crate::planet::{FACE_BLOCKS, Face, SurfacePos};
use crate::planet_atlas::{
    BIOME_FOREST, BIOME_JUNGLE, BIOME_TAIGA, EDAPHIC_SHALLOW_ROCK, MineralKind, PlanetAtlas,
    WaterBodyKind,
};
use serde::{Deserialize, Serialize};

const FRESH_WATER_ATLAS_STEPS: u16 = 3;
const MIN_VEGETATION: u8 = 72;
const MIN_FERTILITY: u8 = 48;
// A regional founding hinterland, roughly ten minutes of unimpeded walking.
// Deposits stay meaningfully distant; this only guarantees that one viable
// homeland shares a broad travel region with the two bootstrap metals. Seed 0
// proved that the old 2,100-block tin cutoff could reject an otherwise healthy
// planet whose nearest qualifying forest was 2,826 blocks away.
const PREFERRED_COPPER_ACCESS_BLOCKS: f64 = 3_000.0;
const PREFERRED_TIN_ACCESS_BLOCKS: f64 = 3_000.0;
const CLOSE_COPPER_ACCESS_BLOCKS: u32 = 2_600;
const CLOSE_TIN_ACCESS_BLOCKS: u32 = 2_100;
const HEART_PROTECTION_BLOCKS: f64 = 64.0;
const LOCAL_RESOURCE_REACH_BLOCKS: f64 = 48.0;
const FRESH_WATER_REACH_BLOCKS: f64 = 96.0;
pub const ENTRY_RADIUS_CHUNKS: i32 = 2;
const SPAWN_MANIFEST_VERSION: u32 = 1;
const MAX_SPAWN_MANIFEST_BYTES: u64 = 1024 * 1024;
// Atlas cells are 32-block summaries. A forest cell can still land its exact
// 80x80 trial region on a rocky shoulder with no reachable trunk, and nearby
// fresh water can fall just outside the walkable voxel component. Eight
// trials made otherwise valid seeds fail creation (seed 42 is the regression
// case). Keep the expensive search bounded, but wide enough to cross a local
// run of unlucky fine-detail samples.
const MAX_VOXEL_CANDIDATES: usize = 96;
const SPAWN_VERIFICATION_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpawnVerification {
    contract_version: u32,
    walkable_cells: usize,
    safe_standing: bool,
    reachable_wood: bool,
    reachable_fresh_water: bool,
    reachable_soil: bool,
    reachable_stone: bool,
    reachable_plants: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpawnManifest {
    version: u32,
    topology: String,
    face_blocks: u16,
    world_height: u16,
    seed: u32,
    generator_version: u32,
    atlas_format_version: u32,
    atlas_algorithm_version: u32,
    atlas_checksum: u64,
    content_hash: u64,
    spawn_surface: SurfacePos,
    spawn: crate::planet::EntityPos,
    prepared_radius_chunks: i32,
    chunks: Vec<ChunkPos>,
    prepared_digest: u64,
    verification: SpawnVerification,
    completed_unix_seconds: u64,
}

#[derive(Clone, Copy)]
struct SpawnCandidate {
    surface: SurfacePos,
    score: i64,
    fresh_water_steps: u16,
    copper_distance_blocks: u32,
    tin_distance_blocks: u32,
}

impl SpawnCandidate {
    fn preferred_resource_hinterland(self) -> bool {
        f64::from(self.copper_distance_blocks) <= PREFERRED_COPPER_ACCESS_BLOCKS
            && f64::from(self.tin_distance_blocks) <= PREFERRED_TIN_ACCESS_BLOCKS
    }

    fn farthest_bootstrap_resource(self) -> u32 {
        self.copper_distance_blocks.max(self.tin_distance_blocks)
    }

    fn close_bootstrap_hinterland(self) -> bool {
        self.copper_distance_blocks <= CLOSE_COPPER_ACCESS_BLOCKS
            && self.tin_distance_blocks <= CLOSE_TIN_ACCESS_BLOCKS
    }
}

#[derive(Default)]
struct SpawnSelectionDiagnostics {
    atlas_cells: usize,
    viable_habitat: usize,
    connected_land: usize,
    manageable_relief: usize,
    outside_protected_sites: usize,
    resources_before_protection: usize,
    resources_after_protection: usize,
    best_resource_pair_blocks: Option<(u32, u32)>,
    best_unprotected_resource_pair_blocks: Option<(u32, u32)>,
    resource_site: Option<(Face, u16, u16)>,
    resource_sites_near_heart: usize,
    resource_sites_near_volcano: usize,
}

impl std::fmt::Display for SpawnSelectionDiagnostics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} atlas cells, {} viable habitat, {} connected land, {} manageable relief, {} outside protected sites, {} resource-qualified before protection, {} after, best copper/tin distances {:?} blocks ({:?} outside protection), resource site {:?}, rejected near heart/volcano {}/{}",
            self.atlas_cells,
            self.viable_habitat,
            self.connected_land,
            self.manageable_relief,
            self.outside_protected_sites,
            self.resources_before_protection,
            self.resources_after_protection,
            self.best_resource_pair_blocks,
            self.best_unprotected_resource_pair_blocks,
            self.resource_site,
            self.resource_sites_near_heart,
            self.resource_sites_near_volcano,
        )
    }
}

fn fresh_water_distances(atlas: &PlanetAtlas) -> Vec<u16> {
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

fn traversable_component_sizes(atlas: &PlanetAtlas) -> Vec<usize> {
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

fn qualified_atlas_candidates(
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
fn spawn_candidate_portfolio(candidates: &[SpawnCandidate]) -> Vec<SpawnCandidate> {
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

fn chunks_around(surface: SurfacePos, radius: i32) -> Vec<ChunkPos> {
    let center = ChunkPos::from_surface(surface);
    let mut chunks = Vec::new();
    for du in -radius..=radius {
        for dv in -radius..=radius {
            chunks.push(center.offset(du, dv));
        }
    }
    chunks.sort_unstable();
    chunks.dedup();
    chunks
}

fn entry_chunks(surface: SurfacePos) -> Vec<ChunkPos> {
    chunks_around(surface, ENTRY_RADIUS_CHUNKS)
}

pub(crate) fn player_entry_chunks(surface: SurfacePos) -> Vec<ChunkPos> {
    chunks_around(surface, 1)
}

#[derive(Clone, Copy)]
struct TrialColumn {
    height: i32,
    safe: bool,
    wood: bool,
    drinkable_water: bool,
    soil: bool,
    stone: bool,
    plant: bool,
}

#[derive(Debug)]
struct TrialQualification {
    spawn: SurfacePos,
    verification: SpawnVerification,
}

/// Why a trial region fell short, plus the best safe spawn it still offered (if
/// any). A region can miss the full six-resource bar yet still hold a safe
/// standing doorstep; keeping that as a fallback lets world creation degrade to
/// a playable-but-lean homeland instead of dead-ending on a demanding seed.
#[derive(Debug)]
struct TrialReject {
    reason: String,
    fallback: Option<TrialQualification>,
}

/// A chosen homeland: its atlas surface, the qualified spawn, and the trial
/// chunks already generated for it (committed as the world's first region).
type HomelandTrial = (SurfacePos, TrialQualification, Vec<(ChunkPos, Chunk)>);

impl SpawnVerification {
    fn is_fully_qualified(&self) -> bool {
        self.walkable_cells >= 32
            && self.reachable_wood
            && self.reachable_fresh_water
            && self.reachable_soil
            && self.reachable_stone
            && self.reachable_plants
    }

    /// How many of the five bootstrap resources this spawn can actually reach.
    fn resource_score(&self) -> u32 {
        [
            self.reachable_wood,
            self.reachable_fresh_water,
            self.reachable_soil,
            self.reachable_stone,
            self.reachable_plants,
        ]
        .into_iter()
        .filter(|&reachable| reachable)
        .count() as u32
    }
}

fn qualify_trial_region(
    reg: &Registry,
    atlas: &PlanetAtlas,
    center: SurfacePos,
    chunks: &[(ChunkPos, Chunk)],
) -> Result<TrialQualification, TrialReject> {
    if chunks.len() != entry_chunks(center).len()
        || chunks
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>()
            != entry_chunks(center)
    {
        return Err(TrialReject {
            reason: "trial chunk set does not match the required entry region".into(),
            fallback: None,
        });
    }
    let log_items = reg.tags.get("base:logs");
    let log_blocks = reg
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let item = reg.item_id(&block.name)?;
            log_items
                .is_some_and(|logs| logs.contains(&item))
                .then_some(crate::registry::BlockId(index as u16))
        })
        .collect::<HashSet<_>>();
    let is_soil = |name: &str| {
        matches!(
            name,
            "base:grass" | "base:dirt" | "base:mud" | "base:sand" | "base:clay"
        )
    };
    let is_stone = |name: &str| {
        matches!(
            name,
            "base:stone"
                | "base:gravel"
                | "base:sandstone"
                | "base:limestone"
                | "base:shale"
                | "base:granite"
                | "base:marble"
                | "base:slate"
                | "base:quartzite"
                | "base:basalt"
        )
    };
    let mut columns = HashMap::<SurfacePos, TrialColumn>::new();
    for (position, chunk) in chunks {
        let origin = position.block_origin();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                let surface =
                    SurfacePos::new(origin.face(), origin.u() + x as u16, origin.v() + z as u16)
                        .expect("chunk-local surface is canonical");
                let height = (0..CHUNK_Y)
                    .rev()
                    .find(|&y| reg.is_solid(chunk.get(x, y, z)))
                    .unwrap_or(0) as i32;
                let feet = (height + 1).clamp(0, CHUNK_Y as i32 - 1) as usize;
                let head = (height + 2).clamp(0, CHUNK_Y as i32 - 1) as usize;
                let clear = |block| !reg.is_solid(block) && !reg.is_fluid(block);
                let mut column = TrialColumn {
                    height,
                    safe: height > SEA_LEVEL + 1
                        && height + 2 < CHUNK_Y as i32
                        && reg.is_solid(chunk.get(x, height as usize, z))
                        && clear(chunk.get(x, feet, z))
                        && clear(chunk.get(x, head, z)),
                    wood: false,
                    drinkable_water: false,
                    soil: false,
                    stone: false,
                    plant: false,
                };
                let drinkable = atlas.hydrology_sample(surface.center()).salinity < 48;
                for y in 1..CHUNK_Y {
                    let block = chunk.get(x, y, z);
                    let definition = reg.block(block);
                    column.wood |= log_blocks.contains(&block);
                    column.drinkable_water |= drinkable && reg.water_volume(block).is_some();
                    column.soil |= is_soil(&definition.name);
                    column.stone |= is_stone(&definition.name);
                    column.plant |= (definition.cross && definition.burns > 0)
                        || definition.name.ends_with("_leaves")
                        || definition.name == "base:leaves";
                    if reg.is_lava(block) && (y as i32 - height).abs() <= 3 {
                        column.safe = false;
                    }
                }
                columns.insert(surface, column);
            }
        }
    }

    // A safe doorstep must be in the trial's center chunk. This keeps the
    // exact 5x5 prepared set centered on the final spawn even when the atlas
    // candidate itself lies on a chunk boundary.
    let center_chunk = ChunkPos::from_surface(center);
    let mut spawn = None;
    let mut spawn_score = f64::INFINITY;
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            let origin = center_chunk.block_origin();
            let surface =
                SurfacePos::new(origin.face(), origin.u() + x as u16, origin.v() + z as u16)
                    .expect("center chunk surface is canonical");
            let Some(column) = columns.get(&surface).copied() else {
                continue;
            };
            if !column.safe {
                continue;
            }
            let maximum_step = crate::planet::neighbors4(surface)
                .into_iter()
                .filter_map(|neighbor| columns.get(&neighbor))
                .map(|neighbor| (column.height - neighbor.height).abs())
                .max()
                .unwrap_or(i32::MAX);
            if maximum_step > 2 {
                continue;
            }
            let score = f64::from(maximum_step) * 100.0
                + crate::planet::geodesic_distance(center.center(), surface.center());
            if score < spawn_score {
                spawn_score = score;
                spawn = Some(surface);
            }
        }
    }
    let Some(spawn) = spawn else {
        return Err(TrialReject {
            reason: "no dry two-block-high standing cell in the center chunk".into(),
            fallback: None,
        });
    };

    let mut queue = VecDeque::from([spawn]);
    let mut visited = HashSet::from([spawn]);
    let mut wood = false;
    let mut water = false;
    let mut soil = false;
    let mut stone = false;
    let mut plant = false;
    let mut walkable_cells = 0;
    while let Some(surface) = queue.pop_front() {
        let column = columns[&surface];
        let distance = crate::planet::geodesic_distance(spawn.center(), surface.center());
        if distance <= LOCAL_RESOURCE_REACH_BLOCKS {
            walkable_cells += 1;
        }
        let nearby = std::iter::once(surface).chain(crate::planet::neighbors4(surface));
        for candidate in nearby {
            if let Some(signal) = columns.get(&candidate) {
                if distance <= LOCAL_RESOURCE_REACH_BLOCKS {
                    wood |= signal.wood;
                    soil |= signal.soil;
                    stone |= signal.stone;
                    plant |= signal.plant;
                }
                if distance <= FRESH_WATER_REACH_BLOCKS {
                    water |= signal.drinkable_water;
                }
            }
        }
        for neighbor in crate::planet::neighbors4(surface) {
            let Some(next) = columns.get(&neighbor) else {
                continue;
            };
            if !next.safe
                || (column.height - next.height).abs() > 1
                || crate::planet::geodesic_distance(spawn.center(), neighbor.center())
                    > FRESH_WATER_REACH_BLOCKS
                || !visited.insert(neighbor)
            {
                continue;
            }
            queue.push_back(neighbor);
        }
    }
    let verification = SpawnVerification {
        contract_version: SPAWN_VERIFICATION_VERSION,
        walkable_cells,
        safe_standing: true,
        reachable_wood: wood,
        reachable_fresh_water: water,
        reachable_soil: soil,
        reachable_stone: stone,
        reachable_plants: plant,
    };
    if verification.is_fully_qualified() {
        Ok(TrialQualification {
            spawn,
            verification,
        })
    } else {
        let reason = format!(
            "walkable={} wood={} fresh_water={} soil={} stone={} plants={}",
            verification.walkable_cells,
            verification.reachable_wood,
            verification.reachable_fresh_water,
            verification.reachable_soil,
            verification.reachable_stone,
            verification.reachable_plants,
        );
        // A safe doorstep with only some resources nearby is still somewhere a
        // player can stand and start — kept as a fallback for the caller.
        Err(TrialReject {
            reason,
            fallback: Some(TrialQualification {
                spawn,
                verification,
            }),
        })
    }
}

fn prepared_chunk_digest(
    store: &storage::RegionStore,
    chunks: &[ChunkPos],
) -> std::io::Result<u64> {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for position in chunks {
        for byte in [position.face() as u8]
            .into_iter()
            .chain(position.u().to_le_bytes())
            .chain(position.v().to_le_bytes())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3);
        }
        let payload = store.read(*position)?.0.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("prepared spawn chunk {position:?} is missing"),
            )
        })?;
        for byte in payload {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3);
        }
    }
    Ok(hash)
}

fn validate_spawn_ledgers(world: &World) -> std::io::Result<()> {
    let weather = world
        .planetary_weather
        .as_ref()
        .ok_or_else(|| std::io::Error::other("planetary spawn requires a water ledger"))?;
    let water = weather
        .water
        .audit(crate::planet_atlas::ReservoirMass::fresh(
            crate::planet_atlas::dynamic_water_total(&weather.cells) as u64,
        ));
    if water.unexplained_water_delta_hu != 0 || water.unexplained_salt_delta != 0 {
        return Err(std::io::Error::other(format!(
            "prepared homeland water audit failed: {} HU, {} salt",
            water.unexplained_water_delta_hu, water.unexplained_salt_delta
        )));
    }
    let materials = world
        .material_ledger
        .as_ref()
        .ok_or_else(|| std::io::Error::other("planetary spawn requires a material ledger"))?
        .audit();
    if !materials.is_balanced() {
        return Err(std::io::Error::other(
            "prepared homeland material audit has unexplained deltas",
        ));
    }
    if !materials.is_qualified() {
        return Err(std::io::Error::other(format!(
            "prepared homeland material qualification failed: {}",
            materials.qualification_failures.join("; ")
        )));
    }
    Ok(())
}

impl World {
    /// Census-backed discovery sites. New planets receive three independent
    /// observational ruins in the prepared homeland. Existing planets use an
    /// untouched prepared chunk when possible; if every candidate was already
    /// worked, a single surface remnant is added without replacing authored
    /// voxels. The persisted site keys make this retrogen idempotent.
    fn ensure_spawn_discovery_sites(
        &mut self,
        spawn: SurfacePos,
    ) -> std::io::Result<Vec<ChunkPos>> {
        let structure = self
            .reg
            .structures
            .iter()
            .position(|structure| structure.name == "base:observational_outpost")
            .ok_or_else(|| {
                std::io::Error::other("base observational outpost content is missing")
            })?;
        let prepared_chunks = entry_chunks(spawn);
        let prepared = prepared_chunks.iter().copied().collect::<HashSet<_>>();
        // Site placement is a consequence of the already-qualified homeland,
        // not another reason to reject it. Build the dry component a player
        // can actually walk from the accepted doorstep, then choose origins
        // nearest the three desired bearings. Fixed offsets failed valid
        // coastal homelands whenever one bearing happened to land in water.
        let mut dry_columns = HashMap::<SurfacePos, i32>::new();
        for position in &prepared_chunks {
            let origin = position.block_origin();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    let surface = SurfacePos::new(
                        origin.face(),
                        origin.u() + x as u16,
                        origin.v() + z as u16,
                    )
                    .expect("prepared chunks contain canonical surface cells");
                    let y = self.surface_height_at(surface);
                    let clear = |y: i32| {
                        BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).is_ok_and(
                            |pos| {
                                let block = self.get_block_at(pos);
                                !self.reg.is_solid(block) && !self.reg.is_fluid(block)
                            },
                        )
                    };
                    if y > SEA_LEVEL + 1 && y < CHUNK_Y as i32 - 8 && clear(y + 1) && clear(y + 2) {
                        dry_columns.insert(surface, y);
                    }
                }
            }
        }
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        if dry_columns.contains_key(&spawn) {
            reachable.insert(spawn);
            queue.push_back(spawn);
        }
        while let Some(surface) = queue.pop_front() {
            let height = dry_columns[&surface];
            for neighbor in crate::planet::neighbors4(surface) {
                if reachable.contains(&neighbor)
                    || !prepared.contains(&ChunkPos::from_surface(neighbor))
                {
                    continue;
                }
                let Some(next_height) = dry_columns.get(&neighbor) else {
                    continue;
                };
                if (height - next_height).abs() <= 1 {
                    reachable.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }
        if reachable.is_empty() {
            return Err(std::io::Error::other(
                "prepared homeland discovery census cannot reach its accepted doorstep",
            ));
        }
        let desired = [(24, 0), (-24, 0), (0, 24)];
        let mut changed = Vec::new();
        let mut chosen = Vec::<SurfacePos>::new();
        for (index, (base_u, base_v)) in desired.into_iter().enumerate() {
            let key = format!("spawn-observational-site-v1-{index}");
            if self.discovery_site_installed(&key) {
                continue;
            }
            let target = SurfacePos::canonicalized(
                spawn.face(),
                i32::from(spawn.u()) + base_u,
                i32::from(spawn.v()) + base_v,
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            let mut dry = reachable
                .iter()
                .copied()
                .filter(|surface| {
                    chosen.iter().all(|prior| {
                        crate::planet::geodesic_distance(prior.center(), surface.center()) >= 8.0
                    })
                })
                .map(|surface| (surface, dry_columns[&surface]))
                .collect::<Vec<_>>();
            dry.sort_by(|(left, _), (right, _)| {
                self.player_touched
                    .contains(&ChunkPos::from_surface(*left))
                    .cmp(
                        &self
                            .player_touched
                            .contains(&ChunkPos::from_surface(*right)),
                    )
                    .then_with(|| {
                        crate::planet::geodesic_distance(target.center(), left.center()).total_cmp(
                            &crate::planet::geodesic_distance(target.center(), right.center()),
                        )
                    })
                    .then_with(|| left.cmp(right))
            });
            let Some((origin_surface, surface_y)) = dry.first().copied() else {
                return Err(std::io::Error::other(format!(
                    "prepared homeland has fewer than three distinct reachable observational sites (stopped at {index})"
                )));
            };
            chosen.push(origin_surface);
            let origin = BlockPos::new(
                origin_surface.face(),
                origin_surface.u(),
                surface_y as u8,
                origin_surface.v(),
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            if !self.player_touched.contains(&origin.chunk()) {
                let seed = self.seed ^ 0xd15c_0000 ^ index as u32;
                self.place_structure_at(structure, origin, seed);
            } else {
                // Explicit remnant fallback for an evolved homeland: add one
                // brushable clue in the air above existing terrain. Never
                // rewrite a player-authored cell to fake an untouched ruin.
                let remnant = origin
                    .offset(0, 1, 0)
                    .ok_or_else(|| std::io::Error::other("discovery remnant exceeds world"))?;
                if self.get_block_at(remnant) != AIR {
                    return Err(std::io::Error::other(format!(
                        "worked homeland has no non-destructive remnant cell for site {index}"
                    )));
                }
                let cracked = self
                    .reg
                    .block_id("base:cracked_masonry")
                    .ok_or_else(|| std::io::Error::other("cracked masonry content is missing"))?;
                self.set_block_authored_at(
                    remnant,
                    cracked,
                    "discovery remnant retrogen into worked homeland",
                );
            }
            self.mark_discovery_site_installed(&key);
            changed.push(origin.chunk());
        }
        Ok(changed)
    }

    /// Pick one deterministic, naturally viable common spawn for every play
    /// mode. Voxel-level refinement remains `safe_spawn_at`; unlike the old
    /// dedicated path, its starting country is already dry, living land.
    #[cfg(test)]
    pub fn qualified_spawn_surface(&self) -> Option<SurfacePos> {
        let Some(atlas) = self.planet_atlas.as_deref() else {
            // Atlas-free fixtures retain a deterministic local doorstep.
            return SurfacePos::new(Face::PosZ, FACE_BLOCKS / 2, FACE_BLOCKS / 2).ok();
        };
        let (candidates, _) = qualified_atlas_candidates(atlas);
        candidates.first().map(|candidate| candidate.surface)
    }

    /// Load or create the world's persisted, fully materialized common
    /// doorstep. The manifest is published only after all entry chunks and
    /// conservation ledgers are durable.
    pub fn prepare_common_spawn(
        &mut self,
        progress: impl FnMut(&str, usize, usize),
    ) -> std::io::Result<crate::planet::EntityPos> {
        self.prepare_common_spawn_cancellable(
            &crate::planet_atlas::CancellationToken::default(),
            progress,
        )
    }

    pub(crate) fn prepare_common_spawn_cancellable(
        &mut self,
        cancel: &crate::planet_atlas::CancellationToken,
        mut progress: impl FnMut(&str, usize, usize),
    ) -> std::io::Result<crate::planet::EntityPos> {
        check_cancelled(cancel)?;
        let path = self.save_dir.join("spawn.toml");
        if path.is_file() {
            let metadata = fs::metadata(&path)?;
            if metadata.len() > MAX_SPAWN_MANIFEST_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "spawn manifest exceeds its size bound",
                ));
            }
            let text = fs::read_to_string(&path)?;
            let manifest: SpawnManifest = toml::from_str(&text).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid spawn manifest: {error}"),
                )
            })?;
            let atlas = self
                .planet_atlas
                .clone()
                .ok_or_else(|| std::io::Error::other("planetary spawn requires an atlas"))?;
            if manifest.version != SPAWN_MANIFEST_VERSION
                || manifest.topology != WORLD_TOPOLOGY
                || manifest.face_blocks != FACE_BLOCKS
                || manifest.world_height != CHUNK_Y as u16
                || manifest.seed != self.seed
                || !(super::MIN_SUPPORTED_WORLD_GENERATOR_VERSION..=WORLD_GENERATOR_VERSION)
                    .contains(&manifest.generator_version)
                || manifest.atlas_format_version != crate::planet_atlas::ATLAS_FORMAT_VERSION
                || manifest.atlas_algorithm_version > crate::planet_atlas::ATLAS_ALGORITHM_VERSION
                || manifest.atlas_checksum != atlas.manifest.genesis_checksum
                || manifest.content_hash != atlas.manifest.content_hash
                || manifest.spawn_surface != manifest.spawn.surface()
                || manifest.prepared_radius_chunks != ENTRY_RADIUS_CHUNKS
                || manifest.chunks != entry_chunks(manifest.spawn_surface)
                || manifest.verification.contract_version != SPAWN_VERIFICATION_VERSION
                || manifest.prepared_digest == 0
                || manifest.completed_unix_seconds == 0
                || manifest.verification.walkable_cells < 32
                || !manifest.verification.safe_standing
                || !manifest.verification.reachable_wood
                || !manifest.verification.reachable_fresh_water
                || !manifest.verification.reachable_soil
                || !manifest.verification.reachable_stone
                || !manifest.verification.reachable_plants
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "spawn manifest is incompatible with this planet",
                ));
            }
            // `prepared_digest` is immutable creation evidence, not an
            // eternal checksum: ecology, fluids, fire, and player edits may
            // legitimately change these chunks after publication. Presence,
            // decoding, ledger validation, and a live safe-entry check below
            // distinguish a usable evolved homeland from corruption without
            // rejecting ordinary play.
            for (index, position) in manifest.chunks.iter().copied().enumerate() {
                check_cancelled(cancel)?;
                self.try_ensure_chunk(position)?;
                if !self.has_chunk(position) {
                    return Err(std::io::Error::other(format!(
                        "could not load homeland chunk {position:?}"
                    )));
                }
                progress(
                    "loading prepared homeland",
                    index + 1,
                    manifest.chunks.len(),
                );
            }
            validate_spawn_ledgers(self)?;
            let spawn = self.prepared_spawn_position(manifest.spawn).ok_or_else(|| {
                std::io::Error::other(
                    "prepared homeland no longer contains a dry safe entry; explicit spawn repair is required",
                )
            })?;
            let region = atlas.atlas_pos(spawn.surface());
            if !self
                .arcane_geography
                .as_ref()
                .is_some_and(|geography| geography.early_discovery_reachable(&atlas, region))
            {
                return Err(std::io::Error::other(
                    "prepared homeland has no same-landmass early magical observation site",
                ));
            }
            check_cancelled(cancel)?;
            // Complete any durable retrogen transaction even if cancellation
            // arrives during it; the caller will then discard session entry.
            let changed = self.ensure_spawn_discovery_sites(spawn.surface())?;
            if !changed.is_empty() {
                // A structure origin can write through a neighboring chunk.
                // Persist the complete prepared homeland so retrogen cannot
                // leave half an outpost only resident in memory.
                for position in entry_chunks(spawn.surface()) {
                    self.save_chunk(position)?;
                }
                let save = self.save_modified();
                if !save.is_ok() {
                    return Err(std::io::Error::other(format!(
                        "could not persist discovery-site retrogen: {}",
                        save.summary()
                    )));
                }
            }
            self.common_spawn = Some(spawn);
            return Ok(spawn);
        }

        let atlas = self
            .planet_atlas
            .clone()
            .ok_or_else(|| std::io::Error::other("planetary spawn requires an atlas"))?;
        progress("selecting homeland", 0, 1);
        let (candidates, selection_diagnostics) = qualified_atlas_candidates(&atlas);
        let candidates = spawn_candidate_portfolio(&candidates);
        if candidates.is_empty() {
            return Err(std::io::Error::other(format!(
                "planet has no dry forest homeland with fresh water ({selection_diagnostics})",
            )));
        }

        let mut winner = None;
        // The best safe-but-resource-incomplete homeland seen so far, kept only
        // to keep world creation from dead-ending: if nothing clears the full
        // bar, spawning somewhere lean beats refusing to make the world at all.
        let mut fallback: Option<HomelandTrial> = None;
        let mut fallback_score = (0u32, 0usize);
        let mut rejections = Vec::new();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            check_cancelled(cancel)?;
            progress("testing homeland", candidate_index, candidates.len());
            let positions = entry_chunks(candidate.surface);
            let generated = generate_trial_region(
                self.seed,
                Arc::clone(&self.reg),
                Arc::clone(&atlas),
                &positions,
                cancel,
                |completed, total| progress("generating homeland", completed, total),
            )?;
            match qualify_trial_region(&self.reg, &atlas, candidate.surface, &generated) {
                Ok(qualification) => {
                    winner = Some((candidate.surface, qualification, generated));
                    break;
                }
                Err(reject) => {
                    rejections.push(format!(
                        "{}:{}:{}: {}",
                        candidate.surface.face().name(),
                        candidate.surface.u(),
                        candidate.surface.v(),
                        reject.reason,
                    ));
                    // Rank incomplete homelands by resources reachable, then by
                    // how much room there is to walk. Only used if no candidate
                    // clears the full bar.
                    if let Some(effort) = reject.fallback {
                        let score = (
                            effort.verification.resource_score(),
                            effort.verification.walkable_cells,
                        );
                        if fallback.is_none() || score > fallback_score {
                            fallback_score = score;
                            fallback = Some((candidate.surface, effort, generated));
                        }
                    }
                }
            }
        }
        let (wanted, qualification, generated) = match winner.or(fallback) {
            Some(chosen) => chosen,
            None => {
                return Err(std::io::Error::other(format!(
                    "no candidate homeland had a safe standing doorstep: {}",
                    rejections.join("; ")
                )));
            }
        };
        if !qualification.verification.is_fully_qualified() {
            let v = &qualification.verification;
            eprintln!(
                "spawn: no homeland cleared the full resource bar for seed {}; \
                 using the best safe doorstep at {}:{}:{} \
                 (walkable={} wood={} water={} soil={} stone={} plants={}); \
                 {} candidate(s) rejected",
                self.seed,
                wanted.face().name(),
                wanted.u(),
                wanted.v(),
                v.walkable_cells,
                v.reachable_wood,
                v.reachable_fresh_water,
                v.reachable_soil,
                v.reachable_stone,
                v.reachable_plants,
                rejections.len(),
            );
        }
        let spawn_surface = qualification.spawn;

        // Once adoption starts, complete the ledger/chunk/manifest publication
        // as one preparation operation; do not abandon partially saved state.
        check_cancelled(cancel)?;
        let total = generated.len();
        for (index, (position, chunk)) in generated.into_iter().enumerate() {
            self.adopt_generated(position, chunk);
            progress("committing homeland", index + 1, total);
        }
        debug_assert_eq!(
            entry_chunks(wanted),
            entry_chunks(spawn_surface),
            "voxel refinement remains in the candidate's center chunk"
        );
        let height = self.surface_height_at(spawn_surface) + 1;
        let spawn = crate::planet::EntityPos::new(
            spawn_surface.face(),
            spawn_surface.u() as f32 + 0.5,
            height as f32 + 0.2,
            spawn_surface.v() as f32 + 0.5,
        )
        .expect("prepared spawn cell is canonical");
        let region = atlas.atlas_pos(spawn.surface());
        if !self
            .arcane_geography
            .as_ref()
            .is_some_and(|geography| geography.early_discovery_reachable(&atlas, region))
        {
            return Err(std::io::Error::other(
                "qualified homeland has no same-landmass early magical observation site",
            ));
        }
        let _ = self.ensure_spawn_discovery_sites(spawn.surface())?;

        let initial_save = self.save_modified();
        if !initial_save.is_ok() {
            return Err(std::io::Error::other(format!(
                "could not persist homeland ledgers: {}",
                initial_save.summary()
            )));
        }
        let chunks = entry_chunks(spawn.surface());
        for (index, position) in chunks.iter().copied().enumerate() {
            self.save_chunk(position)?;
            progress("persisting homeland", index + 1, chunks.len());
        }
        let final_save = self.save_modified();
        if !final_save.is_ok() {
            return Err(std::io::Error::other(format!(
                "could not finalize homeland ledgers: {}",
                final_save.summary()
            )));
        }
        validate_spawn_ledgers(self)?;
        let prepared_digest = prepared_chunk_digest(&self.region_store, &chunks)?;
        let completed_unix_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let manifest = SpawnManifest {
            version: SPAWN_MANIFEST_VERSION,
            topology: WORLD_TOPOLOGY.to_string(),
            face_blocks: FACE_BLOCKS,
            world_height: CHUNK_Y as u16,
            seed: self.seed,
            generator_version: WORLD_GENERATOR_VERSION,
            atlas_format_version: crate::planet_atlas::ATLAS_FORMAT_VERSION,
            atlas_algorithm_version: crate::planet_atlas::ATLAS_ALGORITHM_VERSION,
            atlas_checksum: atlas.manifest.genesis_checksum,
            content_hash: atlas.manifest.content_hash,
            spawn_surface,
            spawn,
            prepared_radius_chunks: ENTRY_RADIUS_CHUNKS,
            chunks,
            prepared_digest,
            verification: qualification.verification,
            completed_unix_seconds,
        };
        let text = toml::to_string_pretty(&manifest)
            .map_err(|error| std::io::Error::other(format!("spawn manifest: {error}")))?;
        crate::persist::atomic_write(&path, text.as_bytes(), false)?;
        progress("homeland ready", 1, 1);
        self.common_spawn = Some(spawn);
        Ok(spawn)
    }

    fn prepared_spawn_position(
        &self,
        wanted: crate::planet::EntityPos,
    ) -> Option<crate::planet::EntityPos> {
        let origin = wanted.surface();
        let mut best = None;
        let mut best_score = i32::MAX;
        for du in -32i32..=32 {
            for dv in -32i32..=32 {
                let Ok(surface) = SurfacePos::canonicalized(
                    origin.face(),
                    i32::from(origin.u()) + du,
                    i32::from(origin.v()) + dv,
                ) else {
                    continue;
                };
                if !self.chunks.contains_key(&ChunkPos::from_surface(surface)) {
                    continue;
                }
                let height = self.surface_height_at(surface);
                let feet = height + 1;
                if height <= SEA_LEVEL + 1 || !self.standable_at(surface, feet) {
                    continue;
                }
                let slope = crate::planet::neighbors4(surface)
                    .into_iter()
                    .map(|neighbor| (height - self.surface_height_at(neighbor)).abs())
                    .max()
                    .unwrap_or_default();
                if slope > 2 {
                    continue;
                }
                let score = slope * 100 + du.abs() + dv.abs();
                if score < best_score {
                    best_score = score;
                    best = crate::planet::EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        feet as f32 + 0.2,
                        f32::from(surface.v()) + 0.5,
                    )
                    .ok();
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet_atlas::AtlasPos;

    #[test]
    fn freshwater_distance_crosses_cube_face_edges() {
        let atlas = PlanetAtlas::fixture(8_101, 4).unwrap();
        let distances = fresh_water_distances(&atlas);
        assert_eq!(distances.len(), atlas.genesis.hydrology.len());
        for face in Face::ALL {
            for u in 0..atlas.side() {
                for v in [0, atlas.side() - 1] {
                    let pos = AtlasPos { face, u, v };
                    for neighbor in pos.neighbors4(atlas.side()) {
                        let a = distances[pos.index(atlas.side())];
                        let b = distances[neighbor.index(atlas.side())];
                        if a != u16::MAX && b != u16::MAX {
                            assert!(a.abs_diff(b) <= 1);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn atlas_free_world_has_the_canonical_fallback_doorstep() {
        let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let world = World::new(99, std::path::PathBuf::new(), reg);
        let spawn = world.qualified_spawn_surface().unwrap();
        assert_eq!(spawn.face(), Face::PosZ);
        assert_eq!(spawn.u(), FACE_BLOCKS / 2);
        assert_eq!(spawn.v(), FACE_BLOCKS / 2);
    }

    #[test]
    fn voxel_trial_requires_a_safe_walkable_resource_doorstep() {
        let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let atlas = Arc::new(PlanetAtlas::fixture(1_337, 64).unwrap());
        let atlas_pos = atlas
            .genesis
            .hydrology
            .iter()
            .find(|(pos, water)| {
                water.water_body == WaterBodyKind::Land
                    && pos.u > 2
                    && pos.v > 2
                    && pos.u + 3 < atlas.side()
                    && pos.v + 3 < atlas.side()
            })
            .map(|(pos, _)| pos)
            .unwrap();
        let point = atlas_pos.center(atlas.side());
        let center = SurfacePos::new(point.face, point.u as u16, point.v as u16).unwrap();
        let stone = reg.block_id("base:stone").unwrap();
        let dirt = reg.block_id("base:dirt").unwrap();
        let grass = reg.block_id("base:grass").unwrap();
        let log = reg.block_id("base:log").unwrap();
        let bush = reg.block_id("base:berry_bush").unwrap();
        let water = reg.water_for_volume(8);
        let positions = entry_chunks(center);
        let mut chunks = positions
            .iter()
            .map(|position| {
                let mut chunk = Chunk::new();
                for x in 0..CHUNK_X {
                    for z in 0..CHUNK_Z {
                        chunk.set(x, 64, z, stone);
                        chunk.set(x, 65, z, dirt);
                        chunk.set(x, 66, z, grass);
                    }
                }
                (*position, chunk)
            })
            .collect::<Vec<_>>();
        let center_chunk = ChunkPos::from_surface(center);
        let chunk = chunks
            .iter_mut()
            .find(|(position, _)| *position == center_chunk)
            .map(|(_, chunk)| chunk)
            .unwrap();
        chunk.set(5, 67, 5, log);
        chunk.set(6, 67, 5, water);
        chunk.set(7, 67, 5, bush);

        let qualification = qualify_trial_region(&reg, &atlas, center, &chunks).unwrap();
        assert!(qualification.verification.walkable_cells >= 32);
        assert!(qualification.verification.reachable_wood);
        assert!(qualification.verification.reachable_fresh_water);
        assert!(qualification.verification.reachable_soil);
        assert!(qualification.verification.reachable_stone);
        assert!(qualification.verification.reachable_plants);
    }

    #[test]
    fn homeland_census_installs_three_redundant_observational_sites_once() {
        let root =
            std::env::temp_dir().join(format!("wildforge-discovery-spawn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let mut world = World::new(99, root.clone(), reg.clone());
        world.discovery_state = Some(
            crate::discovery::DiscoveryState::load_or_initialize(&root, 99, reg.content_hash)
                .unwrap(),
        );
        let spawn = world.qualified_spawn_surface().unwrap();
        let stone = reg.block_id("base:stone").unwrap();
        let dirt = reg.block_id("base:dirt").unwrap();
        let grass = reg.block_id("base:grass").unwrap();
        for position in entry_chunks(spawn) {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 64, z, stone);
                    chunk.set(x, 65, z, dirt);
                    chunk.set(x, 66, z, grass);
                }
            }
            world.adopt_generated(position, chunk);
        }
        let installed = world.ensure_spawn_discovery_sites(spawn).unwrap();
        assert_eq!(installed.len(), 3);
        assert!(
            world
                .ensure_spawn_discovery_sites(spawn)
                .unwrap()
                .is_empty()
        );
        let mut evidence = HashMap::<String, usize>::new();
        for (_, entity) in world.block_entities() {
            if let BlockEntity::Chest(chest) = entity {
                for stack in chest.slots.iter().flatten() {
                    if let Some(class) = reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .and_then(|definition| definition.evidence_class.clone())
                    {
                        *evidence.entry(class).or_default() += 1;
                    }
                }
            }
        }
        for class in crate::discovery::EVIDENCE_CLASSES {
            assert!(
                evidence.get(class).copied().unwrap_or_default() >= 3,
                "foundational clue {class} is not independently redundant"
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn discovery_census_routes_around_a_wet_nominal_site() {
        let root = std::env::temp_dir().join(format!(
            "wildforge-discovery-wet-bearing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let mut world = World::new(20260802, root.clone(), reg.clone());
        world.discovery_state = Some(
            crate::discovery::DiscoveryState::load_or_initialize(&root, 20260802, reg.content_hash)
                .unwrap(),
        );
        let spawn = world.qualified_spawn_surface().unwrap();
        let stone = reg.block_id("base:stone").unwrap();
        let dirt = reg.block_id("base:dirt").unwrap();
        let grass = reg.block_id("base:grass").unwrap();
        for position in entry_chunks(spawn) {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 64, z, stone);
                    chunk.set(x, 65, z, dirt);
                    chunk.set(x, 66, z, grass);
                }
            }
            world.adopt_generated(position, chunk);
        }
        // Remove every column the old five-point fixed-offset search tried
        // for its first outpost. The accepted homeland remains broadly dry.
        for (du, dv) in [(24, 0), (24, 8), (32, 0), (24, -8), (16, 0)] {
            let surface = SurfacePos::canonicalized(
                spawn.face(),
                i32::from(spawn.u()) + du,
                i32::from(spawn.v()) + dv,
            )
            .unwrap();
            for y in 64..=66 {
                let pos = BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap();
                world.set_block_at(pos, AIR);
            }
        }

        assert_eq!(world.ensure_spawn_discovery_sites(spawn).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn discovery_retrogen_preserves_worked_homeland_and_adds_explicit_remnants() {
        let root = std::env::temp_dir().join(format!(
            "wildforge-discovery-retrogen-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let reg = Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let mut world = World::new(101, root.clone(), reg.clone());
        world.discovery_state = Some(
            crate::discovery::DiscoveryState::load_or_initialize(&root, 101, reg.content_hash)
                .unwrap(),
        );
        let spawn = world.qualified_spawn_surface().unwrap();
        let stone = reg.block_id("base:stone").unwrap();
        let dirt = reg.block_id("base:dirt").unwrap();
        let grass = reg.block_id("base:grass").unwrap();
        let prepared = entry_chunks(spawn);
        for position in &prepared {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 64, z, stone);
                    chunk.set(x, 65, z, dirt);
                    chunk.set(x, 66, z, grass);
                }
            }
            world.adopt_generated(*position, chunk);
        }
        world.player_touched.extend(prepared);
        let sentinel = BlockPos::new(spawn.face(), spawn.u(), 67, spawn.v()).unwrap();
        let planks = reg.block_id("base:planks").unwrap();
        world.set_block_at(sentinel, planks);

        assert_eq!(world.ensure_spawn_discovery_sites(spawn).unwrap().len(), 3);
        assert_eq!(world.get_block_at(sentinel), planks);
        assert!(
            world
                .block_entities()
                .all(|(_, entity)| !matches!(entity, BlockEntity::Chest(_)))
        );
        let cracked = reg.block_id("base:cracked_masonry").unwrap();
        let remnants = world
            .chunks
            .values()
            .map(|chunk| {
                (0..CHUNK_X)
                    .flat_map(|x| (0..CHUNK_Z).map(move |z| (x, z)))
                    .flat_map(|(x, z)| (67..72).map(move |y| (x, y, z)))
                    .filter(|(x, y, z)| chunk.get(*x, *y, *z) == cracked)
                    .count()
            })
            .sum::<usize>();
        assert_eq!(remnants, 3);
        assert!(
            world
                .ensure_spawn_discovery_sites(spawn)
                .unwrap()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "operator probe for WILDFORGE_PROBE_WORLD production atlas"]
    fn production_spawn_selection_probe() {
        let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
            .map(std::path::PathBuf::from)
            .expect("set WILDFORGE_PROBE_WORLD");
        let atlas = PlanetAtlas::load(&root).unwrap();
        let (candidates, diagnostics) = qualified_atlas_candidates(&atlas);
        eprintln!("{diagnostics}; candidates={}", candidates.len());
        assert!(!candidates.is_empty());
    }
}
