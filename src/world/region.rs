//! Region files: 32x32 chunks per file, with an offset table.
//!
//! Chunks used to be one file each — `c.{x}.{z}.wfc`, about 14 KB apiece, all
//! in a single flat directory. A well-explored world puts tens of thousands of
//! them there, which costs on every backup, copy and directory scan, and on
//! some filesystems on every individual open.
//!
//! Layout: a four-byte magic, then 1024 `(offset u32, length u32)` slots, then
//! payloads. Writes append and then update the slot, in that order — a crash
//! between the two leaves the slot pointing at the previous payload, which is
//! stale but whole. Rewriting a chunk strands its old payload, so the file is
//! compacted once the dead weight outgrows the live.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::chunk::ChunkPos;

const MAGIC: &[u8; 4] = b"WFR1";
const REGION_SHIFT: i32 = 5;
const REGION_SIDE: i32 = 1 << REGION_SHIFT;
const SLOTS: usize = (REGION_SIDE * REGION_SIDE) as usize;
const INDEX_BYTES: usize = SLOTS * 8;
const HEADER_BYTES: u64 = 4 + INDEX_BYTES as u64;

/// Compact once a file is more than this multiple of its live bytes.
const COMPACT_RATIO: u64 = 2;
/// ...and never for the sake of less than this much dead space.
const COMPACT_FLOOR: u64 = 4 * 1024 * 1024;

fn region_of(pos: ChunkPos) -> (i32, i32) {
    (pos.x >> REGION_SHIFT, pos.z >> REGION_SHIFT)
}

fn slot_of(pos: ChunkPos) -> usize {
    let lx = pos.x.rem_euclid(REGION_SIDE) as usize;
    let lz = pos.z.rem_euclid(REGION_SIDE) as usize;
    lz * REGION_SIDE as usize + lx
}

pub fn region_path(dir: &Path, pos: ChunkPos) -> PathBuf {
    let (rx, rz) = region_of(pos);
    dir.join(format!("r.{rx}.{rz}.wfr"))
}

/// The pre-region per-chunk file, still read so existing worlds keep working.
pub fn legacy_chunk_path(dir: &Path, pos: ChunkPos) -> PathBuf {
    dir.join(format!("c.{}.{}.wfc", pos.x, pos.z))
}

fn read_index(file: &mut fs::File) -> std::io::Result<Vec<(u32, u32)>> {
    file.seek(SeekFrom::Start(4))?;
    let mut raw = vec![0u8; INDEX_BYTES];
    file.read_exact(&mut raw)?;
    Ok(raw
        .chunks_exact(8)
        .map(|e| {
            (
                u32::from_le_bytes(e[0..4].try_into().unwrap()),
                u32::from_le_bytes(e[4..8].try_into().unwrap()),
            )
        })
        .collect())
}

/// One chunk's stored bytes, or None if this region has never held it.
///
/// Falls back to the legacy per-chunk file, so a world saved before region
/// files still loads; the next save of that chunk moves it into the region.
pub fn read_chunk(dir: &Path, pos: ChunkPos) -> Option<Vec<u8>> {
    let path = region_path(dir, pos);
    let found = (|| -> std::io::Result<Option<Vec<u8>>> {
        let mut file = fs::File::open(&path)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Ok(None);
        }
        let index = read_index(&mut file)?;
        let (offset, len) = index[slot_of(pos)];
        if len == 0 {
            return Ok(None);
        }
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        Ok(Some(buf))
    })()
    .ok()
    .flatten();
    if found.is_some() {
        return found;
    }
    fs::read(legacy_chunk_path(dir, pos)).ok()
}

/// Store one chunk's bytes, appending and then pointing the slot at them.
pub fn write_chunk(dir: &Path, pos: ChunkPos, bytes: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = region_path(dir, pos);
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;
    let len = file.metadata()?.len();
    if len < HEADER_BYTES {
        // Fresh (or truncated) region: lay down the magic and an empty index.
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(MAGIC)?;
        file.write_all(&vec![0u8; INDEX_BYTES])?;
    }
    let offset = file.seek(SeekFrom::End(0))?;
    if offset > u32::MAX as u64 {
        return Err(std::io::Error::other("region file is full"));
    }
    // Payload first: a crash before the slot update leaves the old payload
    // addressed, which is stale but never torn.
    file.write_all(bytes)?;
    let slot = slot_of(pos);
    file.seek(SeekFrom::Start(4 + slot as u64 * 8))?;
    file.write_all(&(offset as u32).to_le_bytes())?;
    file.write_all(&(bytes.len() as u32).to_le_bytes())?;
    file.flush()?;

    // The old copy of this chunk is now dead weight.
    let total = file.metadata()?.len();
    let live: u64 = read_index(&mut file)?
        .iter()
        .map(|(_, l)| *l as u64)
        .sum::<u64>()
        + HEADER_BYTES;
    if total > live * COMPACT_RATIO && total - live > COMPACT_FLOOR {
        drop(file);
        compact(&path)?;
    }
    // A chunk that lived in a legacy file now lives in the region.
    let _ = fs::remove_file(legacy_chunk_path(dir, pos));
    Ok(())
}

/// Rewrite a region with its live payloads only.
fn compact(path: &Path) -> std::io::Result<()> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Ok(());
    }
    let index = read_index(&mut file)?;
    let mut payloads: Vec<(usize, Vec<u8>)> = Vec::new();
    for (slot, &(offset, len)) in index.iter().enumerate() {
        if len == 0 {
            continue;
        }
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut buf = vec![0u8; len as usize];
        if file.read_exact(&mut buf).is_ok() {
            payloads.push((slot, buf));
        }
    }
    drop(file);

    // Built beside the original and moved over it, so an interrupted compaction
    // cannot lose a region that was perfectly readable a moment ago.
    let tmp = path.with_extension("wfr.tmp");
    let mut new_index = vec![(0u32, 0u32); SLOTS];
    let mut body = Vec::new();
    for (slot, buf) in payloads {
        let offset = HEADER_BYTES + body.len() as u64;
        new_index[slot] = (offset as u32, buf.len() as u32);
        body.extend_from_slice(&buf);
    }
    let mut out = fs::File::create(&tmp)?;
    out.write_all(MAGIC)?;
    for (offset, len) in &new_index {
        out.write_all(&offset.to_le_bytes())?;
        out.write_all(&len.to_le_bytes())?;
    }
    out.write_all(&body)?;
    out.flush()?;
    drop(out);
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("wildforge-region-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn chunks_round_trip_through_one_region_file() {
        let dir = tmp("round-trip");
        let a = ChunkPos { x: 3, z: 9 };
        let b = ChunkPos { x: 31, z: 0 };
        write_chunk(&dir, a, b"first chunk").unwrap();
        write_chunk(&dir, b, b"second chunk").unwrap();
        assert_eq!(read_chunk(&dir, a).unwrap(), b"first chunk");
        assert_eq!(read_chunk(&dir, b).unwrap(), b"second chunk");
        // Both landed in the same file: that is the whole point.
        assert_eq!(region_path(&dir, a), region_path(&dir, b));
        let files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(files.len(), 1, "one file, not one per chunk");
    }

    #[test]
    fn negative_coordinates_land_in_their_own_region() {
        let dir = tmp("negatives");
        // -1 belongs to region -1, not region 0: a floor shift, not a
        // truncating divide, or the whole western half of a world collides
        // with the eastern half.
        let west = ChunkPos { x: -1, z: -1 };
        let east = ChunkPos { x: 0, z: 0 };
        assert_ne!(region_path(&dir, west), region_path(&dir, east));
        write_chunk(&dir, west, b"west").unwrap();
        write_chunk(&dir, east, b"east").unwrap();
        assert_eq!(read_chunk(&dir, west).unwrap(), b"west");
        assert_eq!(read_chunk(&dir, east).unwrap(), b"east");
        assert_ne!(slot_of(west), slot_of(east));
    }

    #[test]
    fn rewriting_a_chunk_replaces_it() {
        let dir = tmp("rewrite");
        let pos = ChunkPos { x: 5, z: 5 };
        for n in 0..12 {
            write_chunk(&dir, pos, format!("version {n}").as_bytes()).unwrap();
        }
        assert_eq!(read_chunk(&dir, pos).unwrap(), b"version 11");
        // Neighbours in the same region are undisturbed by the churn.
        let other = ChunkPos { x: 6, z: 5 };
        write_chunk(&dir, other, b"neighbour").unwrap();
        write_chunk(&dir, pos, b"final").unwrap();
        assert_eq!(read_chunk(&dir, other).unwrap(), b"neighbour");
        assert_eq!(read_chunk(&dir, pos).unwrap(), b"final");
    }

    #[test]
    fn a_missing_chunk_reads_as_absent() {
        let dir = tmp("absent");
        assert!(read_chunk(&dir, ChunkPos { x: 0, z: 0 }).is_none());
        write_chunk(&dir, ChunkPos { x: 0, z: 0 }, b"here").unwrap();
        assert!(read_chunk(&dir, ChunkPos { x: 1, z: 0 }).is_none());
    }

    #[test]
    fn a_world_saved_before_regions_still_loads() {
        let dir = tmp("legacy");
        let pos = ChunkPos { x: 2, z: -7 };
        fs::write(legacy_chunk_path(&dir, pos), b"old flat file").unwrap();
        assert_eq!(read_chunk(&dir, pos).unwrap(), b"old flat file");
        // Saving it moves it into the region and retires the old file.
        write_chunk(&dir, pos, b"new").unwrap();
        assert!(!legacy_chunk_path(&dir, pos).exists());
        assert_eq!(read_chunk(&dir, pos).unwrap(), b"new");
    }

    #[test]
    fn compaction_keeps_every_live_chunk() {
        let dir = tmp("compact");
        let a = ChunkPos { x: 1, z: 1 };
        let b = ChunkPos { x: 2, z: 2 };
        write_chunk(&dir, b, b"b stays").unwrap();
        // Enough churn to trip the compaction thresholds.
        let big = vec![7u8; 512 * 1024];
        for _ in 0..20 {
            write_chunk(&dir, a, &big).unwrap();
        }
        assert_eq!(read_chunk(&dir, b).unwrap(), b"b stays");
        assert_eq!(read_chunk(&dir, a).unwrap().len(), big.len());
        let size = fs::metadata(region_path(&dir, a)).unwrap().len();
        assert!(
            size < big.len() as u64 * 4,
            "twenty rewrites of a 512 KB chunk left {size} bytes behind"
        );
    }
}
