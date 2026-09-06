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
mod tests;
