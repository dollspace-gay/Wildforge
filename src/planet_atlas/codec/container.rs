//! Versioned atlas containers, checksums, and payload size contracts.

use crate::planet_atlas::AtlasError;
use crate::planet_atlas::codec::primitives::{ByteReader, put_u16, put_u32, put_u64};
use crate::planet_atlas::codec::{DYNAMIC_MAGIC, DYNAMIC_PREFIX_BYTES, FILE_HEADER_BYTES};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::identity::stable_hash;
use crate::planet_atlas::storage::read_bounded;
use std::path::Path;

pub(in crate::planet_atlas) fn container_bytes(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: usize,
    payload: &[u8],
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let expected = count
        .checked_mul(record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if payload.len() != expected || record_bytes > u16::MAX as usize {
        return Err(AtlasError::Corrupt(format!(
            "payload has {} bytes; expected {expected}",
            payload.len()
        )));
    }
    let mut out = Vec::with_capacity(FILE_HEADER_BYTES + payload.len());
    out.extend_from_slice(magic);
    put_u32(&mut out, version);
    put_u16(&mut out, side);
    put_u16(&mut out, record_bytes as u16);
    put_u32(
        &mut out,
        count.try_into().expect("atlas cell count fits u32"),
    );
    put_u64(&mut out, payload.len() as u64);
    put_u64(&mut out, stable_hash(payload));
    debug_assert_eq!(out.len(), FILE_HEADER_BYTES);
    out.extend_from_slice(payload);
    Ok(out)
}

pub(in crate::planet_atlas) fn container_bytes_owned(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: usize,
    payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let expected = count
        .checked_mul(record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if payload.len() != expected || record_bytes > u16::MAX as usize {
        return Err(AtlasError::Corrupt(format!(
            "payload has {} bytes; expected {expected}",
            payload.len()
        )));
    }
    prepend_container_header(magic, version, side, record_bytes as u16, count, payload)
}

pub(in crate::planet_atlas) fn variable_container_bytes(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    payload: &[u8],
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let mut out = Vec::with_capacity(FILE_HEADER_BYTES + payload.len());
    out.extend_from_slice(magic);
    put_u32(&mut out, version);
    put_u16(&mut out, side);
    put_u16(&mut out, 0);
    put_u32(
        &mut out,
        count.try_into().expect("atlas cell count fits u32"),
    );
    put_u64(&mut out, payload.len() as u64);
    put_u64(&mut out, stable_hash(payload));
    debug_assert_eq!(out.len(), FILE_HEADER_BYTES);
    out.extend_from_slice(payload);
    Ok(out)
}

pub(in crate::planet_atlas) fn variable_container_bytes_owned(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    prepend_container_header(magic, version, side, 0, count, payload)
}

pub(in crate::planet_atlas) fn prepend_container_header(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: u16,
    count: usize,
    mut payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let payload_len = payload.len();
    let checksum = stable_hash(&payload);
    payload.reserve(FILE_HEADER_BYTES);
    payload.resize(payload_len + FILE_HEADER_BYTES, 0);
    payload.copy_within(0..payload_len, FILE_HEADER_BYTES);
    let mut header = Vec::with_capacity(FILE_HEADER_BYTES);
    header.extend_from_slice(magic);
    put_u32(&mut header, version);
    put_u16(&mut header, side);
    put_u16(&mut header, record_bytes);
    put_u32(
        &mut header,
        count
            .try_into()
            .map_err(|_| AtlasError::Corrupt("atlas cell count exceeds u32".into()))?,
    );
    put_u64(&mut header, payload_len as u64);
    put_u64(&mut header, checksum);
    debug_assert_eq!(header.len(), FILE_HEADER_BYTES);
    payload[..FILE_HEADER_BYTES].copy_from_slice(&header);
    Ok(payload)
}

pub(in crate::planet_atlas) fn decode_variable_container(
    path: &Path,
    expected_magic: &[u8; 4],
    expected_version: u32,
    expected_side: u16,
    max_bytes: u64,
) -> Result<Vec<u8>, AtlasError> {
    let mut bytes = read_bounded(path, max_bytes)?;
    if bytes.len() < FILE_HEADER_BYTES {
        return Err(AtlasError::Corrupt(format!(
            "{} is shorter than its header",
            path.display()
        )));
    }
    let mut header = ByteReader::new(&bytes[..FILE_HEADER_BYTES]);
    let magic = header.array::<4>()?;
    let version = header.u32()?;
    let side = header.u16()?;
    let record_bytes = header.u16()?;
    let count = header.u32()? as usize;
    let payload_len = usize::try_from(header.u64()?)
        .map_err(|_| AtlasError::Corrupt("payload length does not fit memory".into()))?;
    let checksum = header.u64()?;
    if magic != *expected_magic
        || version != expected_version
        || side != expected_side
        || record_bytes != 0
        || count != atlas_count(expected_side)?
        || bytes.len() != FILE_HEADER_BYTES.saturating_add(payload_len)
    {
        return Err(AtlasError::Corrupt(format!(
            "{} has inconsistent water-container metadata",
            path.display()
        )));
    }
    let payload = bytes.split_off(FILE_HEADER_BYTES);
    if stable_hash(&payload) != checksum {
        return Err(AtlasError::Corrupt(format!(
            "{} failed its file checksum",
            path.display()
        )));
    }
    Ok(payload)
}

pub(in crate::planet_atlas) fn decode_container(
    path: &Path,
    expected_magic: &[u8; 4],
    expected_version: u32,
    expected_side: u16,
    expected_record_bytes: usize,
    max_bytes: u64,
) -> Result<Vec<u8>, AtlasError> {
    let mut bytes = read_bounded(path, max_bytes)?;
    if bytes.len() < FILE_HEADER_BYTES {
        return Err(AtlasError::Corrupt(format!(
            "{} is shorter than its header",
            path.display()
        )));
    }
    let mut header = ByteReader::new(&bytes[..FILE_HEADER_BYTES]);
    let magic = header.array::<4>()?;
    let version = header.u32()?;
    let side = header.u16()?;
    let record_bytes = usize::from(header.u16()?);
    let count = header.u32()? as usize;
    let payload_len = usize::try_from(header.u64()?)
        .map_err(|_| AtlasError::Corrupt("payload length does not fit memory".into()))?;
    let checksum = header.u64()?;
    let expected_count = atlas_count(expected_side)?;
    let expected_payload = expected_count
        .checked_mul(expected_record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if expected_magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if magic != *expected_magic {
        return Err(AtlasError::Corrupt(format!(
            "{} has the wrong file magic",
            path.display()
        )));
    }
    if version != expected_version {
        return Err(AtlasError::UnsupportedVersion(format!(
            "{} schema {version} (supported {expected_version})",
            path.display()
        )));
    }
    if side != expected_side
        || count != expected_count
        || record_bytes != expected_record_bytes
        || payload_len != expected_payload
        || bytes.len() != FILE_HEADER_BYTES + payload_len
    {
        return Err(AtlasError::Corrupt(format!(
            "{} has inconsistent dimensions or length",
            path.display()
        )));
    }
    let payload = bytes.split_off(FILE_HEADER_BYTES);
    if stable_hash(&payload) != checksum {
        return Err(AtlasError::Corrupt(format!(
            "{} failed its file checksum",
            path.display()
        )));
    }
    Ok(payload)
}
