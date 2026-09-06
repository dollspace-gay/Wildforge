//! Immutable saved-chunk decoding, independent of the live World owner.

use std::io;
use std::sync::Arc;

use super::decoder::{Decoder, invalid};
use super::palette_store::PaletteSnapshot;
use super::region_store::{ChunkRevision as SavedRevision, RegionStore};
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, HydrologyVolumeRecord};
use crate::registry::Registry;

/// Missing content and the historically supported placeholder repair are explicit.
pub(crate) enum ChunkRead {
    Missing,
    Present(Chunk),
    LegacyPlaceholder,
}

/// Immutable save decoder that can be cloned into cold-terrain workers.
#[derive(Clone)]
pub(crate) struct ChunkLoader {
    pub(super) store: RegionStore,
    pub(super) palette: Arc<PaletteSnapshot>,
    pub(super) reg: Arc<Registry>,
}

pub(crate) struct ChunkLoad {
    pub(crate) content: ChunkRead,
    pub(crate) revision: ChunkRevision,
}

/// Prepared bytes are meaningful only with the registry and palette that
/// decoded them, as well as the saved-chunk revision they were read from.
pub(crate) struct ChunkRevision {
    saved: SavedRevision,
    context: ChunkLoader,
}

impl ChunkRevision {
    pub(in crate::world) fn is_current(&self, pos: ChunkPos, loader: &ChunkLoader) -> bool {
        self.context.matches(loader) && loader.store.is_current(pos, &self.saved)
    }
}

impl ChunkLoader {
    pub(crate) fn registry(&self) -> &Arc<Registry> {
        &self.reg
    }

    /// Pointer identity stays valid while any worker or prepared result owns
    /// its snapshot. This check performs no save reads or palette allocation.
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.store.same_instance(&other.store)
            && Arc::ptr_eq(&self.reg, &other.reg)
            && Arc::ptr_eq(&self.palette, &other.palette)
    }

    pub(crate) fn load(&self, pos: ChunkPos) -> io::Result<ChunkRead> {
        self.load_versioned(pos).map(|loaded| loaded.content)
    }

    pub(crate) fn load_versioned(&self, pos: ChunkPos) -> io::Result<ChunkLoad> {
        self.palette.validate()?;
        let (data, saved) = self.store.read(pos)?;
        let content = match data {
            Some(data) => self.decode(&data)?,
            None => ChunkRead::Missing,
        };
        Ok(ChunkLoad {
            content,
            revision: ChunkRevision {
                saved,
                context: self.clone(),
            },
        })
    }

    fn decode(&self, data: &[u8]) -> io::Result<ChunkRead> {
        let remap = &self.palette.mapping()?.decode;
        let mut bytes = Decoder::new(data);
        let version = match &bytes.array::<4>()? {
            b"WFC8" => 8,
            b"WFC7" => 7,
            b"WFC6" => 6,
            _ => return Err(invalid("unsupported saved chunk format")),
        };
        let mut chunk = Chunk::new();
        let blocks = chunk.raw_mut();
        bytes.plane("block", blocks, |bytes| {
            let stored = usize::from(bytes.u16()?);
            Ok(remap
                .get(stored)
                .copied()
                .unwrap_or(self.reg.unknown_block)
                .0)
        })?;
        let placeholder = blocks
            .iter()
            .all(|&block| block == self.reg.unknown_block.0);
        bytes.plane("metadata", chunk.meta_raw_mut(), Decoder::u8)?;
        if version >= 7 {
            bytes.plane("water salt", chunk.water_salt_raw_mut(), Decoder::u16)?;
        } else {
            // WFC6 stored visible water levels and concentration in metadata.
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
        if version >= 8 {
            bytes.plane("soil salinity", chunk.soil_salinity_raw_mut(), Decoder::u8)?;
        }
        let count = usize::from(bytes.u16()?);
        let mut hydrology = Vec::with_capacity(count);
        for _ in 0..count {
            let reservoir = bytes.u64()?;
            let baseline = bytes.u64()?;
            let residual = bytes.i64()?;
            let salt_mass = if version >= 7 { bytes.u64()? } else { 0 };
            hydrology.push(HydrologyVolumeRecord {
                reservoir,
                baseline_hu: if version >= 7 {
                    baseline
                } else {
                    baseline.saturating_mul(32)
                },
                residual_hu: if version >= 7 {
                    residual
                } else {
                    residual.saturating_mul(32)
                },
                salt_mass,
            });
        }
        bytes.finish()?;
        if placeholder {
            // Preserve the named preexisting repair only for a valid payload;
            // malformed data must not be mistaken for recoverable old content.
            return Ok(ChunkRead::LegacyPlaceholder);
        }
        chunk.set_hydrology_volumes(hydrology);
        chunk.dirty = true;
        chunk.compact();
        chunk.modified = false;
        Ok(ChunkRead::Present(chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::{ChunkLoader, ChunkRead, PaletteSnapshot, RegionStore};
    use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z};
    use crate::registry::{self, AIR};
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn loader() -> ChunkLoader {
        let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
        ChunkLoader {
            store: RegionStore::new(PathBuf::new()),
            palette: PaletteSnapshot::for_decode(vec![
                AIR,
                reg.block_id("base:water").unwrap(),
                reg.unknown_block,
            ]),
            reg,
        }
    }

    fn plane(bytes: &mut Vec<u8>, value: &[u8]) {
        let mut remaining = CHUNK_X * CHUNK_Y * CHUNK_Z;
        while remaining != 0 {
            let run = remaining.min(u16::MAX as usize);
            bytes.extend_from_slice(&(run as u16).to_le_bytes());
            bytes.extend_from_slice(value);
            remaining -= run;
        }
    }

    fn payload(version: u8, block: u16) -> Vec<u8> {
        let mut bytes = format!("WFC{version}").into_bytes();
        plane(&mut bytes, &block.to_le_bytes());
        plane(&mut bytes, &[7]);
        if version >= 7 {
            plane(&mut bytes, &42u16.to_le_bytes());
        }
        if version >= 8 {
            plane(&mut bytes, &[11]);
        }
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&5u64.to_le_bytes());
        bytes.extend_from_slice(&9u64.to_le_bytes());
        bytes.extend_from_slice(&(-4i64).to_le_bytes());
        if version >= 7 {
            bytes.extend_from_slice(&42u64.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn saved_versions_preserve_palette_units_and_modified_flags() {
        let loader = loader();
        for version in 6..=8 {
            let ChunkRead::Present(chunk) = loader.decode(&payload(version, 1)).unwrap() else {
                panic!("valid saved chunk was not decoded");
            };
            assert_eq!(
                chunk.get(0, 10, 0),
                loader.palette.mapping().unwrap().decode[1]
            );
            assert_eq!(chunk.meta(0, 10, 0), 7);
            let salt = if version == 6 {
                u16::from(
                    loader
                        .reg
                        .water_volume(loader.palette.mapping().unwrap().decode[1])
                        .unwrap(),
                ) * 32
                    * 7
            } else {
                42
            };
            assert_eq!(chunk.water_salt(0, 10, 0), salt);
            assert_eq!(
                chunk.soil_salinity(0, 10, 0),
                if version == 8 { 11 } else { 0 }
            );
            assert!(chunk.dirty);
            assert!(!chunk.modified);
            let record = chunk.hydrology_volumes()[0];
            assert_eq!(record.reservoir, 5);
            assert_eq!(record.baseline_hu, if version == 6 { 288 } else { 9 });
            assert_eq!(record.residual_hu, if version == 6 { -128 } else { -4 });
            assert_eq!(record.salt_mass, if version == 6 { 0 } else { 42 });
        }
    }

    #[test]
    fn malformed_runs_truncation_and_trailing_bytes_are_errors() {
        let loader = loader();
        let valid = payload(8, 1);
        for end in 0..valid.len() {
            assert_eq!(
                loader.decode(&valid[..end]).err().unwrap().kind(),
                ErrorKind::InvalidData
            );
        }
        let mut zero = valid.clone();
        zero[4..6].copy_from_slice(&0u16.to_le_bytes());
        let mut overflow = valid.clone();
        overflow[8..10].copy_from_slice(&2u16.to_le_bytes());
        let mut trailing = valid;
        trailing.push(0);
        for bad in [zero, overflow, trailing] {
            assert_eq!(
                loader.decode(&bad).err().unwrap().kind(),
                ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn only_a_valid_all_placeholder_payload_qualifies_for_legacy_repair() {
        let loader = loader();
        let mut data = payload(8, 2);
        assert!(matches!(
            loader.decode(&data).unwrap(),
            ChunkRead::LegacyPlaceholder
        ));
        data.pop();
        assert_eq!(
            loader.decode(&data).err().unwrap().kind(),
            ErrorKind::InvalidData
        );
    }
}
