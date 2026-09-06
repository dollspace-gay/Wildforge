//! Bounded little-endian field readers and writers.

use crate::planet_atlas::AtlasError;

pub(in crate::planet_atlas) struct ByteReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReader<'a> {
    pub(in crate::planet_atlas) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(in crate::planet_atlas) fn array<const N: usize>(&mut self) -> Result<[u8; N], AtlasError> {
        let end = self
            .cursor
            .checked_add(N)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| AtlasError::Corrupt("record ended unexpectedly".into()))?;
        let mut out = [0; N];
        out.copy_from_slice(&self.bytes[self.cursor..end]);
        self.cursor = end;
        Ok(out)
    }

    pub(in crate::planet_atlas) fn bytes(&mut self, len: usize) -> Result<&'a [u8], AtlasError> {
        let end = self
            .cursor
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| AtlasError::Corrupt("record ended unexpectedly".into()))?;
        let out = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(out)
    }

    pub(in crate::planet_atlas) fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }

    pub(in crate::planet_atlas) fn u8(&mut self) -> Result<u8, AtlasError> {
        Ok(self.array::<1>()?[0])
    }

    pub(in crate::planet_atlas) fn u16(&mut self) -> Result<u16, AtlasError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(in crate::planet_atlas) fn u32(&mut self) -> Result<u32, AtlasError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(in crate::planet_atlas) fn u64(&mut self) -> Result<u64, AtlasError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub(in crate::planet_atlas) fn i32(&mut self) -> Result<i32, AtlasError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub(in crate::planet_atlas) fn i16(&mut self) -> Result<i16, AtlasError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub(in crate::planet_atlas) fn f32(&mut self) -> Result<f32, AtlasError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

pub(in crate::planet_atlas) fn put_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}
pub(in crate::planet_atlas) fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
pub(in crate::planet_atlas) fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
pub(in crate::planet_atlas) fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
pub(in crate::planet_atlas) fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
pub(in crate::planet_atlas) fn put_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}
pub(in crate::planet_atlas) fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}
