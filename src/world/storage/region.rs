//! Region files: 32x32 chunks per file, with an offset table.
//!
//! Chunks used to be one file each — `c.{x}.{z}.wfc`, about 14 KB apiece, all
//! in a single flat directory. A well-explored world puts tens of thousands of
//! them there, which costs on every backup, copy and directory scan, and on
//! some filesystems on every individual open.
//!
//! Layout: a four-byte magic, then 1024 `(offset u32, length u32)` slots, then
//! payloads. Writes append and then update the slot. The session RegionStore
//! coordinates readers with this publication; a process crash during the slot
//! update can still tear the index, which validation reports as an error.
//! Rewriting a chunk strands its old payload, so the file is compacted once
//! the dead weight outgrows the live.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::chunk::ChunkPos;

const MAGIC: &[u8; 4] = b"WFR2";
const REGION_SHIFT: u16 = 5;
const REGION_SIDE: u16 = 1 << REGION_SHIFT;
const SLOTS: usize = (REGION_SIDE * REGION_SIDE) as usize;
const INDEX_BYTES: usize = SLOTS * 8;
const HEADER_BYTES: u64 = 4 + INDEX_BYTES as u64;

/// Compact once a file is more than this multiple of its live bytes.
const COMPACT_RATIO: u64 = 2;
/// ...and never for the sake of less than this much dead space.
const COMPACT_FLOOR: u64 = 4 * 1024 * 1024;

fn region_of(pos: ChunkPos) -> (u16, u16) {
    (pos.u() >> REGION_SHIFT, pos.v() >> REGION_SHIFT)
}

fn slot_of(pos: ChunkPos) -> usize {
    let lx = (pos.u() % REGION_SIDE) as usize;
    let lz = (pos.v() % REGION_SIDE) as usize;
    lz * REGION_SIDE as usize + lx
}

pub fn region_path(dir: &Path, pos: ChunkPos) -> PathBuf {
    let (ru, rv) = region_of(pos);
    dir.join(pos.face().name()).join(format!("r.{ru}.{rv}.wfr"))
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

/// Upper bound for a WFC8 payload, including worst-case RLE and reservoir records.
/// The exact codec bound is below 3 MiB; retain headroom without trusting file offsets.
const MAX_CHUNK_BYTES: u32 = 4 * 1024 * 1024;

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

fn validate_slot(offset: u32, len: u32, file_len: u64) -> std::io::Result<()> {
    if u64::from(offset) < HEADER_BYTES
        || len > MAX_CHUNK_BYTES
        || u64::from(offset) + u64::from(len) > file_len
    {
        return Err(invalid(
            "chunk slot points outside the region payload bounds",
        ));
    }
    Ok(())
}

/// Read stored bytes, distinguishing an absent slot from invalid or unreadable data.
pub fn read_chunk(dir: &Path, pos: ChunkPos) -> std::io::Result<Option<Vec<u8>>> {
    let path = region_path(dir, pos);
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let file_len = file.metadata()?.len();
    if file_len < HEADER_BYTES {
        return Err(invalid(format!(
            "truncated region header: {}",
            path.display()
        )));
    }
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid(format!(
            "unsupported region header: {}",
            path.display()
        )));
    }
    let index = read_index(&mut file)?;
    let (offset, len) = index[slot_of(pos)];
    if len == 0 {
        return Ok(None);
    }
    validate_slot(offset, len, file_len)?;
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Store one chunk's bytes, appending and then pointing the slot at them.
pub fn write_chunk(dir: &Path, pos: ChunkPos, bytes: &[u8]) -> std::io::Result<()> {
    if bytes.len() > MAX_CHUNK_BYTES as usize {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "chunk payload exceeds region limit",
        ));
    }
    let path = region_path(dir, pos);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    let len = file.metadata()?.len();
    if len == 0 {
        // Only a newly created region receives a fresh header.
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(MAGIC)?;
        file.write_all(&vec![0u8; INDEX_BYTES])?;
    } else {
        if len < HEADER_BYTES {
            return Err(invalid("refusing to overwrite a truncated region header"));
        }
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(invalid(
                "refusing to overwrite an unsupported region header",
            ));
        }
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
    Ok(())
}

/// Rewrite a region with its live payloads only.
fn compact(path: &Path) -> std::io::Result<()> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid("unsupported region header during compaction"));
    }
    let index = read_index(&mut file)?;
    let mut payloads: Vec<(usize, Vec<u8>)> = Vec::new();
    let file_len = file.metadata()?.len();
    for (slot, &(offset, len)) in index.iter().enumerate() {
        if len == 0 {
            continue;
        }
        validate_slot(offset, len, file_len)?;
        file.seek(SeekFrom::Start(offset as u64))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        payloads.push((slot, buf));
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
#[path = "region_tests.rs"]
mod tests;
