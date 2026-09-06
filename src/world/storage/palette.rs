//! Append-only stored block names and bounded palette parsing.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::registry::{BlockId, Registry};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IDS: usize = u16::MAX as usize + 1;

#[derive(Clone, Debug, Default)]
pub(super) struct Palette {
    names: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub(super) struct Mapping {
    pub(super) decode: Vec<BlockId>,
    pub(super) encode: Vec<u16>,
}

impl Palette {
    /// Missing/empty historical palettes used the active registry's IDs.
    /// Other read or parse failures must not authorize a replacement table.
    pub(super) fn read(path: &Path) -> io::Result<Option<Self>> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("block palette exceeds the supported size"));
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| invalid("block palette is not UTF-8"))?;
        Self::parse(text)
    }

    fn parse(text: &str) -> io::Result<Option<Self>> {
        let mut palette = Self::default();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let (id, name) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| invalid("block palette entry has no name"))?;
            let id = id
                .parse::<u16>()
                .map_err(|_| invalid("block palette ID is outside the u16 range"))?;
            let name = name.trim();
            if name.is_empty() {
                return Err(invalid("block palette entry has an empty name"));
            }
            palette
                .names
                .resize_with(palette.names.len().max(usize::from(id) + 1), || None);
            let slot = &mut palette.names[usize::from(id)];
            if slot.as_ref().is_some_and(|existing| existing != name) {
                return Err(invalid("block palette assigns one ID to different names"));
            }
            *slot = Some(name.to_owned());
        }
        Ok((!palette.names.is_empty()).then_some(palette))
    }

    /// Keep every existing binding, including removed names and vacant IDs.
    /// Build the complete extension before changing the stored table.
    pub(super) fn bind(&mut self, reg: &Registry) -> io::Result<(Mapping, bool)> {
        if reg.blocks.len() > MAX_IDS {
            return Err(invalid("registry exceeds the stored block ID range"));
        }
        let mut by_name = HashMap::with_capacity(self.names.len());
        for (id, name) in self.names.iter().enumerate() {
            if let Some(name) = name {
                by_name.entry(name.as_str()).or_insert(id);
            }
        }
        let mut additions = Vec::new();
        let mut encode = Vec::with_capacity(reg.blocks.len());
        for block in &reg.blocks {
            if block.name.is_empty()
                || block.name.trim() != block.name
                || block.name.contains(['\n', '\r'])
            {
                return Err(invalid(
                    "block name cannot be represented in the saved palette",
                ));
            }
            let id = *by_name.entry(&block.name).or_insert_with(|| {
                let id = self.names.len() + additions.len();
                additions.push(block.name.clone());
                id
            });
            encode.push(u16::try_from(id).map_err(|_| invalid("stored block palette is full"))?);
        }
        let bytes: usize = self
            .names
            .iter()
            .flatten()
            .chain(additions.iter())
            .map(|name| name.len().saturating_add(7))
            .sum();
        if bytes as u64 > MAX_BYTES {
            return Err(invalid("block palette exceeds the supported size"));
        }
        let changed = !additions.is_empty();
        self.names.extend(additions.into_iter().map(Some));
        let decode = self
            .names
            .iter()
            .map(|name| {
                name.as_deref()
                    .and_then(|name| reg.block_id(name))
                    .unwrap_or(reg.unknown_block)
            })
            .collect();
        Ok((Mapping { decode, encode }, changed))
    }

    pub(super) fn text(&self) -> String {
        let mut text = String::new();
        for (id, name) in self.names.iter().enumerate() {
            if let Some(name) = name {
                text.push_str(&format!("{id} {name}\n"));
            }
        }
        text
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
