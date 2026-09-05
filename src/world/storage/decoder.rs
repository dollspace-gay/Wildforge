//! Bounded little-endian reads and RLE planes shared by saved-chunk versions.

use std::io;

pub(super) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub(super) struct Decoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub(super) fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or_else(|| invalid("chunk offset overflow"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| invalid(format!("truncated chunk at byte {}", self.offset)))?;
        let mut out = [0; N];
        out.copy_from_slice(bytes);
        self.offset = end;
        Ok(out)
    }

    pub(super) fn u8(&mut self) -> io::Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(super) fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub(super) fn i64(&mut self) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    pub(super) fn plane<T: Copy>(
        &mut self,
        label: &str,
        out: &mut [T],
        mut value: impl FnMut(&mut Self) -> io::Result<T>,
    ) -> io::Result<()> {
        let mut offset = 0;
        while offset < out.len() {
            let count = usize::from(self.u16()?);
            if count == 0 || count > out.len() - offset {
                return Err(invalid(format!("invalid {label} run length {count}")));
            }
            out[offset..offset + count].fill(value(self)?);
            offset += count;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> io::Result<()> {
        if self.offset != self.data.len() {
            return Err(invalid("trailing bytes after chunk reservoir records"));
        }
        Ok(())
    }
}
