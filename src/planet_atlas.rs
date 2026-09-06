//! Persistent whole-planet atlas infrastructure.
//!
//! The atlas is the coarse, authoritative description of country that has
//! not been materialized as voxel chunks yet. Scientific goals replace the
//! placeholder values in these typed stage-owned layers; the address space,
//! persistence contract, topology, and query operations live here.

use crate::planet::{FACE_BLOCKS, SURFACE_FACES};

mod diagnostics;
pub use diagnostics::{AtlasCensus, AtlasExportReport, export_diagnostics};
mod biomes;
pub use biomes::*;
mod climate;
pub use climate::*;
mod geology;
pub use geology::*;
mod hydrology;
pub use hydrology::*;
mod water_cycle;
pub use water_cycle::*;

mod grid;
use grid::atlas_count;
pub use grid::{AtlasCell, AtlasGrid, AtlasPos, AtlasStep};
mod layers;
pub use layers::{
    BiomeCell, BoundaryClass, ClimateCell, GenesisLayers, GeometryCell, GroundCell, HydrologyCell,
    ResourceCell, TectonicCell, TerrainCell,
};
mod dynamic;
pub use dynamic::{DynamicCell, DynamicLayers, DynamicScan};
mod history;
pub use history::{HistoryLayers, NamedAtlasPlace};
mod manifest;
pub use manifest::{AtlasManifest, StageRecord};
mod generation_config;
pub use generation_config::{
    AtlasConfig, AtlasProgress, AtlasStage, CancellationToken, GenerationMode,
};
mod error;
pub use error::AtlasError;
mod identity;
pub use identity::genesis_content_hash;
use identity::{cell_hash, mix64, unit_noise};
mod generation;
mod sampling;
pub use sampling::{AtlasClimateSample, AtlasTectonicSample, AtlasTerrainSample};
mod codec;
mod storage;
mod validation;
use codec::{DYNAMIC_PREFIX_BYTES, DYNAMIC_RECORD_BYTES, FILE_HEADER_BYTES, GENESIS_RECORD_BYTES};
pub(crate) use storage::arcane::{
    ArcaneGeographyManifestCheckpoint, ArcaneManifestCheckpoint, arcane_geography_manifest_payload,
    update_arcane_geography_manifest, update_arcane_manifest, verify_arcane_geography_manifest,
    verify_arcane_manifest_checkpoint,
};

pub const ATLAS_CELL_BLOCKS: u16 = 32;
pub const ATLAS_FACE_SIDE: u16 = FACE_BLOCKS / ATLAS_CELL_BLOCKS;
pub const ATLAS_CELL_COUNT: usize =
    SURFACE_FACES * ATLAS_FACE_SIDE as usize * ATLAS_FACE_SIDE as usize;
pub const ATLAS_IMMUTABLE_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + ATLAS_CELL_COUNT * GENESIS_RECORD_BYTES) as u64;
pub const ATLAS_DYNAMIC_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + DYNAMIC_PREFIX_BYTES + ATLAS_CELL_COUNT * DYNAMIC_RECORD_BYTES) as u64;
pub const ATLAS_WATER_CYCLE_MIN_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + 156 + ATLAS_CELL_COUNT * 80) as u64;
pub const ATLAS_ESTIMATED_LOADED_BYTES: u64 = (ATLAS_CELL_COUNT
    * (std::mem::size_of::<GeometryCell>()
        + std::mem::size_of::<TectonicCell>()
        + std::mem::size_of::<TerrainCell>()
        + std::mem::size_of::<ClimateCell>()
        + std::mem::size_of::<HydrologyCell>()
        + std::mem::size_of::<GroundCell>()
        + std::mem::size_of::<BiomeCell>()
        + std::mem::size_of::<ResourceCell>()
        + std::mem::size_of::<DynamicCell>()
        + std::mem::size_of::<WaterCell>())) as u64;
/// Conservative generation peak: completed grids plus immutable and dynamic
/// encode buffers and one file container each.
pub const ATLAS_ESTIMATED_GENERATION_PEAK_BYTES: u64 =
    ATLAS_ESTIMATED_LOADED_BYTES + ATLAS_IMMUTABLE_FILE_BYTES * 2;

pub const ATLAS_FORMAT_VERSION: u32 = 6;
pub const ATLAS_ALGORITHM_VERSION: u32 = 7;
pub const ATLAS_DYNAMIC_VERSION: u32 = 3;
pub const WATER_CYCLE_SCHEMA_VERSION: u32 = 1;
pub const ATLAS_HISTORY_VERSION: u32 = 1;
pub const GEOLOGY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct PlanetAtlas {
    pub manifest: AtlasManifest,
    pub genesis: GenesisLayers,
    pub dynamic: DynamicLayers,
    pub water_cycle: WaterCycleState,
    pub history: HistoryLayers,
    pub geology: GeologyModel,
    pub hydrology: HydrologyModel,
    pub biomes: BiomeModel,
}
