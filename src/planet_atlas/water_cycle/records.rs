//! Water cell, aquifer, surface reservoir, and custody record schemas.

use crate::chunk::ChunkPos;
use crate::planet_atlas::AtlasPos;
use super::ReservoirMass;

const RESERVOIR_KIND_SHIFT: u32 = 62;
const RESERVOIR_ID_MASK: u64 = (1u64 << RESERVOIR_KIND_SHIFT) - 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaterCell {
    pub soil: ReservoirMass,
    pub snow: ReservoirMass,
    pub groundwater: ReservoirMass,
    pub runoff: ReservoirMass,
    pub groundwater_head_milliblocks: i32,
    pub last_recharge_hu: u32,
    pub last_spring_hu: u32,
    pub last_evaporation_hu: u32,
}

impl WaterCell {
    pub fn total(self) -> ReservoirMass {
        [self.soil, self.snow, self.groundwater, self.runoff]
            .into_iter()
            .fold(ReservoirMass::default(), |mut total, mass| {
                total.water_hu = total.water_hu.saturating_add(mass.water_hu);
                total.salt_mass = total.salt_mass.saturating_add(mass.salt_mass);
                total
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AquiferLayer {
    Shallow = 0,
    Perched = 1,
    DeepConfined = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SparseAquiferState {
    pub pos: AtlasPos,
    pub layer: AquiferLayer,
    pub mass: ReservoirMass,
    pub capacity_hu: u64,
    pub head_milliblocks: i32,
    pub permeability: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SurfaceReservoirKind {
    Dynamic = 0,
    Ocean = 1,
    Lake = 2,
    River = 3,
}

pub const fn surface_reservoir_id(kind: SurfaceReservoirKind, id: u32) -> u64 {
    ((kind as u64) << RESERVOIR_KIND_SHIFT) | id as u64
}

pub const fn surface_reservoir_parts(id: u64) -> (SurfaceReservoirKind, u32) {
    let kind = match id >> RESERVOIR_KIND_SHIFT {
        1 => SurfaceReservoirKind::Ocean,
        2 => SurfaceReservoirKind::Lake,
        3 => SurfaceReservoirKind::River,
        _ => SurfaceReservoirKind::Dynamic,
    };
    (kind, (id & RESERVOIR_ID_MASK) as u32)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceReservoirState {
    pub id: u64,
    pub name: String,
    /// Coarse, uncommitted water. This is the only surface mass that may be
    /// materialized into a never-before-generated chunk.
    pub coarse: ReservoirMass,
    /// Audit index for mass whose detailed authority is a saved chunk.
    pub committed: ReservoirMass,
    pub initial_total_hu: u64,
    pub level_milliblocks: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpringState {
    pub pos: AtlasPos,
    pub layer: AquiferLayer,
    pub outlet_milliblocks: i32,
    pub last_discharge_hu: u32,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FluxInbox {
    pub pos: AtlasPos,
    pub reservoir: u64,
    pub mass: ReservoirMass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkWaterCommitment {
    pub chunk: ChunkPos,
    pub reservoir: u64,
    pub mass: ReservoirMass,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaterLedger {
    pub initial_water_hu: u64,
    pub initial_salt_mass: u64,
    pub explicit_water_created_hu: u64,
    pub explicit_water_destroyed_hu: u64,
    pub explicit_salt_created: u64,
    pub explicit_salt_destroyed: u64,
    pub portable: [ReservoirMass; 3],
    pub industrial: ReservoirMass,
    pub precipitated_salt_mass: u64,
}
