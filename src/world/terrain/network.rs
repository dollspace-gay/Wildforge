//! Existing WFC6-WFC9 wire reconstruction into resident voxel planes.

use std::sync::Arc;
use crate::chunk::{Chunk, ChunkPos};
use crate::registry::{BlockId, Registry};
use super::TerrainStore;

impl TerrainStore {
    /// Insert a group received in one network poll. Current WFC9 payloads carry
    /// the host's settled light field. Legacy WFC6-WFC8 chunks settle their
    /// shared borders through one fallback lighting cascade.
    pub(in crate::world) fn insert_remote_chunks<'a>(
        &mut self,
        registry: &Arc<Registry>,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    ) {
        let mut needs_relight = Vec::new();
        for (pos, rle) in chunks {
            if self.insert_remote_chunk_unlit(registry, pos, rle, remap) == Some(false) {
                needs_relight.push(pos);
            }
        }
        self.relight_chunks_and_cascade(registry, needs_relight);
    }

    /// `Some(true)` means the payload supplied settled light, `Some(false)`
    /// requests a legacy relight, and `None` rejects an invalid payload.
    fn insert_remote_chunk_unlit(
        &mut self,
        registry: &Arc<Registry>,
        pos: ChunkPos,
        rle: &[u8],
        remap: &[BlockId],
    ) -> Option<bool> {
        let version9 = rle.starts_with(b"WFC9");
        let version8 = rle.starts_with(b"WFC8");
        let version7 = rle.starts_with(b"WFC7");
        if !version9 && !version8 && !version7 && !rle.starts_with(b"WFC6") {
            return None;
        }
        let mut chunk = Chunk::new();
        let out = chunk.raw_mut();
        let mut o = 0;
        let mut i = 4;
        while i + 4 <= rle.len() && o < out.len() {
            let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
            let stored = u16::from_le_bytes([rle[i + 2], rle[i + 3]]) as usize;
            let id = remap.get(stored).copied().unwrap_or(registry.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        if o != out.len() {
            return None;
        }
        let meta = chunk.meta_raw_mut();
        let mut offset = 0;
        while i + 3 <= rle.len() && offset < meta.len() {
            let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
            let value = rle[i + 2];
            let end = (offset + count).min(meta.len());
            meta[offset..end].fill(value);
            offset = end;
            i += 3;
        }
        if offset != meta.len() {
            return None;
        }
        if version9 || version8 || version7 {
            let salt = chunk.water_salt_raw_mut();
            let mut offset = 0;
            while i + 4 <= rle.len() && offset < salt.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = u16::from_le_bytes([rle[i + 2], rle[i + 3]]);
                let end = (offset + count).min(salt.len());
                salt[offset..end].fill(value);
                offset = end;
                i += 4;
            }
            if offset != salt.len() {
                return None;
            }
        }
        if version9 || version8 {
            let salinity = chunk.soil_salinity_raw_mut();
            let mut offset = 0;
            while i + 3 <= rle.len() && offset < salinity.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = rle[i + 2];
                let end = (offset + count).min(salinity.len());
                salinity[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != salinity.len() {
                return None;
            }
        }
        if version9 {
            let light = chunk.light_block_raw_mut();
            let mut offset = 0;
            while i + 5 <= rle.len() && offset < light.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = [rle[i + 2], rle[i + 3], rle[i + 4]];
                let end = (offset + count).min(light.len());
                light[offset..end].fill(value);
                offset = end;
                i += 5;
            }
            if offset != light.len() {
                return None;
            }
            let sky = chunk.light_sky_raw_mut();
            let mut offset = 0;
            while i + 3 <= rle.len() && offset < sky.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = rle[i + 2];
                let end = (offset + count).min(sky.len());
                sky[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != sky.len() {
                return None;
            }
        }
        if i + 2 > rle.len() {
            return None;
        }
        let records = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
        i += 2;
        let detailed_water = version9 || version8 || version7;
        let record_bytes = if detailed_water { 32 } else { 24 };
        if rle.len().saturating_sub(i) != records.saturating_mul(record_bytes) {
            return None;
        }
        let mut hydrology = Vec::with_capacity(records);
        for _ in 0..records {
            let Ok(reservoir) = rle[i..i + 8].try_into().map(u64::from_le_bytes) else {
                return None;
            };
            let Ok(baseline_units) = rle[i + 8..i + 16].try_into().map(u64::from_le_bytes) else {
                return None;
            };
            let Ok(residual_units) = rle[i + 16..i + 24].try_into().map(i64::from_le_bytes) else {
                return None;
            };
            let salt_mass = if detailed_water {
                let Ok(value) = rle[i + 24..i + 32].try_into().map(u64::from_le_bytes) else {
                    return None;
                };
                value
            } else {
                0
            };
            hydrology.push(crate::chunk::HydrologyVolumeRecord {
                reservoir,
                baseline_hu: if detailed_water {
                    baseline_units
                } else {
                    baseline_units.saturating_mul(32)
                },
                residual_hu: if detailed_water {
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
        chunk.compact();
        self.resident.insert(pos, chunk);
        // Neighbors need remeshing for the new border faces.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let n = pos.offset(dx, dz);
            if let Some(c) = self.resident.get_mut(&n) {
                c.dirty = true;
            }
        }
        Some(version9)
    }

}
