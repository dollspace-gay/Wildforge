//! Bounded water-cycle checkpoint encoding and decoding.

use crate::chunk::ChunkPos;
use crate::planet::Face;
use crate::planet_atlas::{AtlasError, AtlasGrid, AtlasPos};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::codec::primitives::{ByteReader, put_i32, put_u16, put_u32, put_u64, put_u8};
use super::{AquiferLayer, ChunkWaterCommitment, FluxInbox, ReservoirMass, SparseAquiferState, SpringState, SurfaceReservoirState, WaterCell, WaterCycleState, WaterLedger};

const WATER_PREFIX_BYTES: usize = 156;
const WATER_CELL_BYTES: usize = 80;

fn put_mass(out: &mut Vec<u8>, mass: ReservoirMass) {
    put_u64(out, mass.water_hu);
    put_u64(out, mass.salt_mass);
}

fn read_mass(reader: &mut ByteReader<'_>) -> Result<ReservoirMass, AtlasError> {
    Ok(ReservoirMass {
        water_hu: reader.u64()?,
        salt_mass: reader.u64()?,
    })
}

fn put_pos(out: &mut Vec<u8>, pos: AtlasPos) {
    put_u8(out, pos.face as u8);
    put_u16(out, pos.u);
    put_u16(out, pos.v);
}

fn read_pos(reader: &mut ByteReader<'_>, side: u16) -> Result<AtlasPos, AtlasError> {
    let face = Face::from_u8(reader.u8()?)
        .ok_or_else(|| AtlasError::Corrupt("water record has an invalid cube face".into()))?;
    AtlasPos::new(face, reader.u16()?, reader.u16()?, side)
}

fn put_layer(out: &mut Vec<u8>, layer: AquiferLayer) {
    put_u8(out, layer as u8);
}

fn read_layer(reader: &mut ByteReader<'_>) -> Result<AquiferLayer, AtlasError> {
    match reader.u8()? {
        0 => Ok(AquiferLayer::Shallow),
        1 => Ok(AquiferLayer::Perched),
        2 => Ok(AquiferLayer::DeepConfined),
        other => Err(AtlasError::Corrupt(format!(
            "unknown aquifer layer {other}"
        ))),
    }
}

pub(in crate::planet_atlas) fn encode_water_cycle(state: &WaterCycleState) -> Result<Vec<u8>, AtlasError> {
    let mut out =
        Vec::with_capacity(WATER_PREFIX_BYTES + state.cells.len().saturating_mul(WATER_CELL_BYTES));
    put_u64(&mut out, state.completed_surface_hours);
    put_u64(&mut out, state.completed_groundwater_days);
    for value in [
        state.ledger.initial_water_hu,
        state.ledger.initial_salt_mass,
        state.ledger.explicit_water_created_hu,
        state.ledger.explicit_water_destroyed_hu,
        state.ledger.explicit_salt_created,
        state.ledger.explicit_salt_destroyed,
    ] {
        put_u64(&mut out, value);
    }
    for portable in state.ledger.portable {
        put_mass(&mut out, portable);
    }
    put_mass(&mut out, state.ledger.industrial);
    put_u64(&mut out, state.ledger.precipitated_salt_mass);
    for count in [
        state.aquifers.len(),
        state.reservoirs.len(),
        state.springs.len(),
        state.inboxes.len(),
        state.commitments.len(),
    ] {
        put_u32(
            &mut out,
            count
                .try_into()
                .map_err(|_| AtlasError::Corrupt("too many water-cycle records".into()))?,
        );
    }
    debug_assert_eq!(out.len(), WATER_PREFIX_BYTES);
    for cell in state.cells.values() {
        put_mass(&mut out, cell.soil);
        put_mass(&mut out, cell.snow);
        put_mass(&mut out, cell.groundwater);
        put_mass(&mut out, cell.runoff);
        put_i32(&mut out, cell.groundwater_head_milliblocks);
        put_u32(&mut out, cell.last_recharge_hu);
        put_u32(&mut out, cell.last_spring_hu);
        put_u32(&mut out, cell.last_evaporation_hu);
    }
    for aquifer in &state.aquifers {
        put_pos(&mut out, aquifer.pos);
        put_layer(&mut out, aquifer.layer);
        put_mass(&mut out, aquifer.mass);
        put_u64(&mut out, aquifer.capacity_hu);
        put_i32(&mut out, aquifer.head_milliblocks);
        put_u16(&mut out, aquifer.permeability);
    }
    for reservoir in &state.reservoirs {
        put_u64(&mut out, reservoir.id);
        let name = reservoir.name.as_bytes();
        put_u16(
            &mut out,
            name.len()
                .try_into()
                .map_err(|_| AtlasError::Corrupt("water reservoir name is too long".into()))?,
        );
        out.extend_from_slice(name);
        put_mass(&mut out, reservoir.coarse);
        put_mass(&mut out, reservoir.committed);
        put_u64(&mut out, reservoir.initial_total_hu);
        put_i32(&mut out, reservoir.level_milliblocks);
    }
    for spring in &state.springs {
        put_pos(&mut out, spring.pos);
        put_layer(&mut out, spring.layer);
        put_i32(&mut out, spring.outlet_milliblocks);
        put_u32(&mut out, spring.last_discharge_hu);
        put_u8(&mut out, u8::from(spring.active));
    }
    for inbox in &state.inboxes {
        put_pos(&mut out, inbox.pos);
        put_u64(&mut out, inbox.reservoir);
        put_mass(&mut out, inbox.mass);
    }
    for commitment in &state.commitments {
        put_u8(&mut out, commitment.chunk.face() as u8);
        put_u16(&mut out, commitment.chunk.u());
        put_u16(&mut out, commitment.chunk.v());
        put_u64(&mut out, commitment.reservoir);
        put_mass(&mut out, commitment.mass);
    }
    Ok(out)
}

pub(in crate::planet_atlas) fn decode_water_cycle(side: u16, payload: &[u8]) -> Result<WaterCycleState, AtlasError> {
    let count = atlas_count(side)?;
    let minimum = WATER_PREFIX_BYTES
        .checked_add(count.checked_mul(WATER_CELL_BYTES).ok_or_else(|| {
            AtlasError::Corrupt("water-cycle cell payload length overflow".into())
        })?)
        .ok_or_else(|| AtlasError::Corrupt("water-cycle payload length overflow".into()))?;
    if payload.len() < minimum {
        return Err(AtlasError::Corrupt(
            "water-cycle payload is truncated".into(),
        ));
    }
    let mut reader = ByteReader::new(payload);
    let completed_surface_hours = reader.u64()?;
    let completed_groundwater_days = reader.u64()?;
    let ledger = WaterLedger {
        initial_water_hu: reader.u64()?,
        initial_salt_mass: reader.u64()?,
        explicit_water_created_hu: reader.u64()?,
        explicit_water_destroyed_hu: reader.u64()?,
        explicit_salt_created: reader.u64()?,
        explicit_salt_destroyed: reader.u64()?,
        portable: [
            read_mass(&mut reader)?,
            read_mass(&mut reader)?,
            read_mass(&mut reader)?,
        ],
        industrial: read_mass(&mut reader)?,
        precipitated_salt_mass: reader.u64()?,
    };
    let counts = [
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
    ];
    if counts.iter().any(|value| *value > count.saturating_mul(16)) {
        return Err(AtlasError::Corrupt(
            "water-cycle sparse record count exceeds its bound".into(),
        ));
    }
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        cells.push(WaterCell {
            soil: read_mass(&mut reader)?,
            snow: read_mass(&mut reader)?,
            groundwater: read_mass(&mut reader)?,
            runoff: read_mass(&mut reader)?,
            groundwater_head_milliblocks: reader.i32()?,
            last_recharge_hu: reader.u32()?,
            last_spring_hu: reader.u32()?,
            last_evaporation_hu: reader.u32()?,
        });
    }
    let mut aquifers = Vec::with_capacity(counts[0]);
    for _ in 0..counts[0] {
        aquifers.push(SparseAquiferState {
            pos: read_pos(&mut reader, side)?,
            layer: read_layer(&mut reader)?,
            mass: read_mass(&mut reader)?,
            capacity_hu: reader.u64()?,
            head_milliblocks: reader.i32()?,
            permeability: reader.u16()?,
        });
    }
    let mut reservoirs = Vec::with_capacity(counts[1]);
    for _ in 0..counts[1] {
        let id = reader.u64()?;
        let name_len = usize::from(reader.u16()?);
        let name = std::str::from_utf8(reader.bytes(name_len)?)
            .map_err(|_| AtlasError::Corrupt("water reservoir name is not UTF-8".into()))?
            .to_owned();
        reservoirs.push(SurfaceReservoirState {
            id,
            name,
            coarse: read_mass(&mut reader)?,
            committed: read_mass(&mut reader)?,
            initial_total_hu: reader.u64()?,
            level_milliblocks: reader.i32()?,
        });
    }
    let mut springs = Vec::with_capacity(counts[2]);
    for _ in 0..counts[2] {
        springs.push(SpringState {
            pos: read_pos(&mut reader, side)?,
            layer: read_layer(&mut reader)?,
            outlet_milliblocks: reader.i32()?,
            last_discharge_hu: reader.u32()?,
            active: match reader.u8()? {
                0 => false,
                1 => true,
                _ => {
                    return Err(AtlasError::Corrupt(
                        "spring active flag is not boolean".into(),
                    ));
                }
            },
        });
    }
    let mut inboxes = Vec::with_capacity(counts[3]);
    for _ in 0..counts[3] {
        inboxes.push(FluxInbox {
            pos: read_pos(&mut reader, side)?,
            reservoir: reader.u64()?,
            mass: read_mass(&mut reader)?,
        });
    }
    let mut commitments = Vec::with_capacity(counts[4]);
    for _ in 0..counts[4] {
        let face = Face::from_u8(reader.u8()?)
            .ok_or_else(|| AtlasError::Corrupt("commitment has an invalid cube face".into()))?;
        let chunk = ChunkPos::new(face, reader.u16()?, reader.u16()?)
            .map_err(|error| AtlasError::Corrupt(format!("invalid committed chunk: {error}")))?;
        commitments.push(ChunkWaterCommitment {
            chunk,
            reservoir: reader.u64()?,
            mass: read_mass(&mut reader)?,
        });
    }
    if !reader.is_empty() {
        return Err(AtlasError::Corrupt(
            "water-cycle payload contains trailing bytes".into(),
        ));
    }
    Ok(WaterCycleState {
        completed_surface_hours,
        completed_groundwater_days,
        cells: AtlasGrid::from_values(side, cells)?,
        aquifers,
        reservoirs,
        springs,
        inboxes,
        commitments,
        ledger,
    })
}
