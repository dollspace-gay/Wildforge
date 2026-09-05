//! Immutable saved-chunk decoding, independent of the live World owner.

use std::path::PathBuf;
use std::sync::Arc;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos};
use crate::registry::Registry;

/// Immutable save decoder that can be cloned into cold-terrain workers.
/// Disk I/O and WFC decoding therefore never need the simulation-owned World.
#[derive(Clone)]
pub(crate) struct ChunkLoader {
    pub(super) save_dir: PathBuf,
    pub(super) load_remap: Vec<crate::registry::BlockId>,
    pub(super) reg: Arc<Registry>,
    pub(super) palette_stale: bool,
}

impl ChunkLoader {
    pub(crate) fn load(&self, pos: ChunkPos) -> Option<Chunk> {
        let data = crate::world::region::read_chunk(&self.save_dir, pos)?;
        let mut chunk = Chunk::new();
        let version8 = data.starts_with(b"WFC8");
        let version7 = data.starts_with(b"WFC7");
        if !version8 && !version7 && !data.starts_with(b"WFC6") {
            return None;
        }
        let out = chunk.raw_mut();
        let mut o = 0;
        // (count u16, id u16) pairs, remapped through the palette.
        let mut i = 4;
        while i + 4 <= data.len() && o < out.len() {
            let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            let stored = u16::from_le_bytes([data[i + 2], data[i + 3]]) as usize;
            let id = self
                .load_remap
                .get(stored)
                .copied()
                .unwrap_or(self.reg.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        if o != out.len() {
            return None; // corrupt; regenerate
        }
        if out.iter().all(|&block| block == self.reg.unknown_block.0) {
            // The old palette-less-save bug decoded even air as the
            // placeholder and could then persist a solid 16x16x256 magenta
            // tower. No legitimate chunk can contain only unknown blocks;
            // its original contents are already unrecoverable, so regenerate
            // terrain instead of keeping the poisoned chunk forever.
            eprintln!(
                "world: regenerating all-placeholder chunk {},{}",
                pos.u(),
                pos.v()
            );
            return None;
        }
        let meta = chunk.meta_raw_mut();
        let mut offset = 0;
        while i + 3 <= data.len() && offset < meta.len() {
            let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            let value = data[i + 2];
            let end = (offset + count).min(meta.len());
            meta[offset..end].fill(value);
            offset = end;
            i += 3;
        }
        if offset != meta.len() {
            return None;
        }
        if version8 || version7 {
            let salt = chunk.water_salt_raw_mut();
            let mut offset = 0;
            while i + 4 <= data.len() && offset < salt.len() {
                let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
                let value = u16::from_le_bytes([data[i + 2], data[i + 3]]);
                let end = (offset + count).min(salt.len());
                salt[offset..end].fill(value);
                offset = end;
                i += 4;
            }
            if offset != salt.len() {
                return None;
            }
        } else {
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    for y in 0..CHUNK_Y {
                        let volume = self.reg.water_volume(chunk.get(x, y, z)).unwrap_or(0);
                        let salt = u16::from(volume)
                            .saturating_mul(32)
                            .saturating_mul(u16::from(chunk.meta(x, y, z)));
                        chunk.set_water_salt(x, y, z, salt);
                    }
                }
            }
        }
        if version8 {
            let salinity = chunk.soil_salinity_raw_mut();
            let mut offset = 0;
            while i + 3 <= data.len() && offset < salinity.len() {
                let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
                let value = data[i + 2];
                let end = (offset + count).min(salinity.len());
                salinity[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != salinity.len() {
                return None;
            }
        }
        if i + 2 > data.len() {
            return None;
        }
        let records = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
        i += 2;
        let record_bytes = if version8 || version7 { 32 } else { 24 };
        if data.len().saturating_sub(i) != records.saturating_mul(record_bytes) {
            return None;
        }
        let mut hydrology = Vec::with_capacity(records);
        for _ in 0..records {
            let reservoir = u64::from_le_bytes(data[i..i + 8].try_into().ok()?);
            let baseline_units = u64::from_le_bytes(data[i + 8..i + 16].try_into().ok()?);
            let residual_units = i64::from_le_bytes(data[i + 16..i + 24].try_into().ok()?);
            let salt_mass = if version8 || version7 {
                u64::from_le_bytes(data[i + 24..i + 32].try_into().ok()?)
            } else {
                0
            };
            hydrology.push(crate::chunk::HydrologyVolumeRecord {
                reservoir,
                baseline_hu: if version8 || version7 {
                    baseline_units
                } else {
                    baseline_units.saturating_mul(32)
                },
                residual_hu: if version8 || version7 {
                    residual_units
                } else {
                    residual_units.saturating_mul(32)
                },
                salt_mass,
            });
            i += record_bytes;
        }
        chunk.set_hydrology_volumes(hydrology);
        chunk.dirty = true;
        // Planes the file turned out uniform in (no block state anywhere,
        // most often) shrink back to a single value.
        chunk.compact();
        // A chunk that came off disk already matches its file, so it only
        // needs saving again once something edits it. The exception is a
        // registry change: the ids in that file are about to be reinterpreted
        // under a new palette, so it has to be rewritten in current ids.
        // Marking every loaded chunk modified unconditionally meant a 20s
        // autosave rewrote the entire explored world, forever.
        chunk.modified = self.palette_stale;
        Some(chunk)
    }
}
