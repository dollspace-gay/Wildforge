//! WFC8 stored IDs and WFC9 runtime IDs share metadata and plane encoding.

use super::palette_store::PaletteSnapshot;
use crate::chunk::Chunk;
use std::io;

pub(super) fn encode_saved_chunk(chunk: &Chunk, palette: &PaletteSnapshot) -> io::Result<Vec<u8>> {
    let mapping = &palette.mapping()?.encode;
    if chunk
        .block_runs()
        .any(|(runtime, _)| usize::from(runtime) >= mapping.len())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "chunk contains an unknown runtime block ID",
        ));
    }
    // Both traversals visit compressed runs; no expanded block plane or
    // temporary chunk is allocated. Validation bounds every immutable lookup.
    Ok(encode_chunk_state(
        chunk,
        false,
        chunk
            .block_runs()
            .map(|(runtime, count)| (mapping[usize::from(runtime)], count)),
    ))
}

#[cfg(test)]
pub(crate) fn encode_chunk(chunk: &Chunk) -> Vec<u8> {
    encode_chunk_state(chunk, false, chunk.block_runs())
}

/// Live network form. Unlike the disk codec, WFC9 includes settled block and
/// sky light so every guest does not recompute the host's identical derived
/// field while a view is streaming in.
pub(crate) fn encode_stream_chunk(chunk: &Chunk) -> Vec<u8> {
    encode_chunk_state(chunk, true, chunk.block_runs())
}

fn encode_chunk_state(
    chunk: &Chunk,
    include_light: bool,
    blocks: impl Iterator<Item = (u16, usize)>,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    buf.extend_from_slice(if include_light { b"WFC9" } else { b"WFC8" });
    // Runs come straight off the plane, so a uniform plane is one step rather
    // than a scan of every cell. Long runs are split for the u16 wire field.
    for (value, mut run) in blocks {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
            run -= take;
        }
    }
    for (value, mut run) in chunk.meta_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.push(value);
            run -= take;
        }
    }
    for (value, mut run) in chunk.water_salt_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
            run -= take;
        }
    }
    for (value, mut run) in chunk.soil_salinity_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.push(value);
            run -= take;
        }
    }
    if include_light {
        for (value, mut run) in chunk.light_block_runs() {
            while run > 0 {
                let take = run.min(u16::MAX as usize);
                buf.extend_from_slice(&(take as u16).to_le_bytes());
                buf.extend_from_slice(&value);
                run -= take;
            }
        }
        for (value, mut run) in chunk.light_sky_runs() {
            while run > 0 {
                let take = run.min(u16::MAX as usize);
                buf.extend_from_slice(&(take as u16).to_le_bytes());
                buf.push(value);
                run -= take;
            }
        }
    }
    let records = chunk.hydrology_volumes();
    buf.extend_from_slice(&(records.len().min(u16::MAX as usize) as u16).to_le_bytes());
    for record in records.iter().take(u16::MAX as usize) {
        buf.extend_from_slice(&record.reservoir.to_le_bytes());
        buf.extend_from_slice(&record.baseline_hu.to_le_bytes());
        buf.extend_from_slice(&record.residual_hu.to_le_bytes());
        buf.extend_from_slice(&record.salt_mass.to_le_bytes());
    }
    buf
}
