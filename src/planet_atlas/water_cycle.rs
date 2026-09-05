//! Exact fixed-point planetary water and salt ownership.
//!
//! Climate decides when and where transfers want to happen. This module owns
//! what they move. One hydro unit (HU) is 1/256 of a full voxel block; one
//! visible fluid level is 32 HU. Salt is integer mass, so concentration is a
//! derived presentation value and no mixing operation averages colours.

use super::AtlasGrid;

mod mass;
pub use mass::{ReservoirMass, WaterClass};
mod records;
pub use records::{WaterCell, AquiferLayer, SparseAquiferState, SurfaceReservoirKind, SurfaceReservoirState, SpringState, FluxInbox, ChunkWaterCommitment, WaterLedger, surface_reservoir_id, surface_reservoir_parts};
mod custody;
mod audit;
pub use audit::WaterAudit;
mod genesis;
pub(super) use genesis::initial_water_cycle;
mod codec;
pub(super) use codec::{encode_water_cycle, decode_water_cycle};

pub const HYDRO_UNITS_PER_BLOCK: u64 = 256;
pub const HYDRO_UNITS_PER_VISIBLE_LEVEL: u64 = 32;
pub const SALINITY_SCALE: u64 = 255;

#[derive(Clone, Debug, PartialEq)]
pub struct WaterCycleState {
    pub completed_surface_hours: u64,
    pub completed_groundwater_days: u64,
    pub cells: AtlasGrid<WaterCell>,
    pub aquifers: Vec<SparseAquiferState>,
    pub reservoirs: Vec<SurfaceReservoirState>,
    pub springs: Vec<SpringState>,
    pub inboxes: Vec<FluxInbox>,
    pub commitments: Vec<ChunkWaterCommitment>,
    pub ledger: WaterLedger,
}
