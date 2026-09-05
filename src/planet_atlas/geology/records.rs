//! Persisted geological site and provenance records, independent of generation.

use serde::{Deserialize, Serialize};
use crate::planet_atlas::AtlasPos;
use super::{VolcanoSource, MagmaChemistry, IntrusionKind, MineralKind, BedrockFamily, BasinKind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlateRecord {
    pub id: u16,
    pub site_unit: [f32; 3],
    pub euler_pole: [f32; 3],
    pub angular_speed: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CratonRecord {
    pub id: u16,
    pub center_unit: [f32; 3],
    pub nuclei: [[f32; 3]; 3],
    pub radius_radians: f32,
    pub interior_age_myr: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolcanoRecord {
    pub id: u32,
    pub pos: AtlasPos,
    pub source: VolcanoSource,
    pub plate_id: u16,
    pub chain_id: u16,
    pub age_myr: u16,
    pub edifice_radius_blocks: u16,
    pub edifice_height_blocks: u16,
    pub crater_radius_blocks: u16,
    pub crater_depth_blocks: u16,
    pub chamber_depth_blocks: u16,
    pub chamber_radius_blocks: u16,
    pub chamber_volume_blocks: u32,
    pub chemistry: MagmaChemistry,
    pub hydrothermal_radius_blocks: u16,
    pub erosion: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntrusionRecord {
    pub id: u32,
    pub pos: AtlasPos,
    pub kind: IntrusionKind,
    pub plate_id: u16,
    pub radius_blocks: u16,
    pub top_depth_blocks: u16,
    pub bottom_depth_blocks: u16,
    pub volume_blocks: u64,
    pub contact_radius_blocks: u16,
    pub chemistry: MagmaChemistry,
    pub age_myr: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepositRecord {
    pub id: u32,
    pub mineral: MineralKind,
    pub pos: AtlasPos,
    pub host: BedrockFamily,
    pub geological_province: u16,
    pub landmass_id: u16,
    pub source_body_id: u32,
    pub radius_blocks: u16,
    pub depth_min: u16,
    pub depth_max: u16,
    pub grade_ppm: u32,
    /// Hard upper bound on materialized ore blocks.
    pub tonnage_blocks: u64,
    pub max_blocks_per_chunk: u16,
    pub eligible_chunk_upper_bound: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StratigraphicStackRecord {
    pub id: u16,
    pub name: String,
    pub layers_bottom_to_top: Vec<BedrockFamily>,
    pub nominal_thicknesses: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeologicalProvinceRecord {
    pub id: u16,
    pub name: String,
    pub dominant_bedrock: BedrockFamily,
    pub plate_id: u16,
    pub craton_id: u16,
    pub basin: BasinKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContinentRecord {
    pub id: u16,
    pub cell_count: u32,
    pub area: f64,
    pub share_of_land: f64,
    pub major: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeologyAttemptRecord {
    pub attempt: u8,
    pub failed_constraints: Vec<String>,
}
