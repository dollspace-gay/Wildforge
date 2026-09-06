//! Shared planetary spawn selection.
//!
//! The selector is deliberately renderer- and transport-independent: solo,
//! windowed hosts, and dedicated hosts must all begin at the same doorstep.

use crate::planet::{Face, SurfacePos};
use crate::world::{Chunk, ChunkPos};
#[cfg(test)]
use crate::{planet::BlockPos, registry::AIR, world::BlockEntity};
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

mod atlas_selection;
mod discovery_sites;
mod entry;
mod position;
mod saved_validation;
mod voxel_trial;
use atlas_selection::{qualified_atlas_candidates, spawn_candidate_portfolio};
use saved_validation::{prepared_chunk_digest, validate_spawn_ledgers};
use voxel_trial::qualify_trial_region;

#[cfg(test)]
mod tests {
    use super::atlas_selection::fresh_water_distances;
    use super::*;
    use crate::planet_atlas::AtlasPos;
    use crate::{
        chunk::{CHUNK_X, CHUNK_Z},
        planet::FACE_BLOCKS,
        planet_atlas::{PlanetAtlas, WaterBodyKind},
        world::World,
    };
    use std::{collections::HashMap, sync::Arc};

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
            .iter()
            .map(|(_, chunk)| {
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
