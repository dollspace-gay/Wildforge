//! Persisted hydrology classifications, named records, and reservoir curves.

use serde::{Deserialize, Serialize};
use crate::planet::FACE_BLOCKS;
use crate::planet_atlas::{AtlasError, AtlasPos};

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum WaterBodyKind {
    #[default]
    Land = 0,
    Ocean = 1,
    River = 2,
    Lake = 3,
    Playa = 4,
    Delta = 5,
    Estuary = 6,
    Wetland = 7,
}

impl WaterBodyKind {
    pub(in crate::planet_atlas) fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::Land),
            1 => Ok(Self::Ocean),
            2 => Ok(Self::River),
            3 => Ok(Self::Lake),
            4 => Ok(Self::Playa),
            5 => Ok(Self::Delta),
            6 => Ok(Self::Estuary),
            7 => Ok(Self::Wetland),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown water-body classification {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LakeClass {
    ThroughFlowFresh,
    TerminalFresh,
    SalineTerminal,
    SeasonalPlaya,
    Rift,
    VolcanicCrater,
    GlacialAlpine,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoragePoint {
    pub elevation: f32,
    pub volume_units: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OceanBasinRecord {
    pub id: u16,
    pub name: String,
    pub cell_count: u32,
    pub area: f64,
    pub baseline_volume_units: u64,
    pub salinity: u8,
    pub connection_basin_id: u16,
    pub connection_sill_elevation: f32,
    pub volume_elevation_curve: Vec<StoragePoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LakeRecord {
    pub id: u32,
    pub name: String,
    pub class: LakeClass,
    pub sink: AtlasPos,
    pub outlet: Option<AtlasPos>,
    pub cell_count: u32,
    pub catchment_area: f64,
    pub surface_elevation: f32,
    pub spill_elevation: f32,
    pub baseline_inflow: f64,
    pub baseline_evaporation: f64,
    pub baseline_outflow: f64,
    pub groundwater_exchange_coefficient: f32,
    pub salinity: u8,
    pub seasonal_level_range: f32,
    pub baseline_volume_units: u64,
    pub voxel_volume_residual: i64,
    pub volume_elevation_curve: Vec<StoragePoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiverRecord {
    pub id: u32,
    pub name: String,
    pub watershed_id: u32,
    pub source: AtlasPos,
    pub mouth: AtlasPos,
    pub sink_name: String,
    pub length_blocks: f64,
    pub maximum_discharge: f32,
    pub maximum_width_blocks: f32,
    pub stream_order: u8,
    pub path: Vec<AtlasPos>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatershedRecord {
    pub id: u32,
    pub name: String,
    pub outlet: AtlasPos,
    pub terminal: bool,
    pub area: f64,
    pub mean_runoff: f64,
}

pub(super) fn storage_curve(
    side: u16,
    indices: &[usize],
    elevations: &[f32],
    maximum: f32,
) -> Vec<StoragePoint> {
    let column_area = f64::from(FACE_BLOCKS / side).powi(2);
    let minimum = indices
        .iter()
        .map(|index| elevations[*index])
        .min_by(f32::total_cmp)
        .unwrap_or(maximum)
        .min(maximum);
    (0..=4)
        .map(|step| {
            let t = step as f32 / 4.0;
            let elevation = minimum + (maximum - minimum) * t;
            let volume = indices
                .iter()
                .map(|index| {
                    let depth = f64::from((elevation - elevations[*index]).max(0.0));
                    depth * column_area * 8.0
                })
                .sum::<f64>()
                .round()
                .clamp(0.0, u64::MAX as f64) as u64;
            StoragePoint {
                elevation,
                volume_units: volume,
            }
        })
        .collect()
}

pub(super) fn generated_word(hash: u64) -> String {
    const ONSETS: [&str; 16] = [
        "Al", "Bar", "Cor", "Dun", "Esh", "Fen", "Gal", "Har", "Is", "Kel", "Lor", "Mor", "Nor",
        "Or", "Sel", "Var",
    ];
    const CODAS: [&str; 16] = [
        "a", "en", "eth", "ia", "in", "or", "un", "ara", "mere", "vale", "esh", "os", "yr", "ain",
        "ora", "ith",
    ];
    format!(
        "{}{}",
        ONSETS[(hash & 15) as usize],
        CODAS[((hash >> 8) & 15) as usize]
    )
}
