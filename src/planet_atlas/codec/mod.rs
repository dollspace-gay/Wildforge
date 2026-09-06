//! Atlas binary schema constants and private encoding adapters.

pub(super) mod container;
pub(super) mod dynamic;
pub(super) mod fingerprints;
pub(super) mod genesis;
pub(super) mod models;
pub(super) mod primitives;

pub(in crate::planet_atlas) const GENESIS_MAGIC: &[u8; 4] = b"WFA6";
pub(in crate::planet_atlas) const DYNAMIC_MAGIC: &[u8; 4] = b"WFD3";
pub(in crate::planet_atlas) const WATER_CYCLE_MAGIC: &[u8; 4] = b"WFW1";
pub(in crate::planet_atlas) const GENESIS_RECORD_BYTES: usize = 335;
pub(in crate::planet_atlas) const DYNAMIC_PREFIX_BYTES: usize = 8;
pub(in crate::planet_atlas) const DYNAMIC_RECORD_BYTES: usize = 32;
pub(in crate::planet_atlas) const FILE_HEADER_BYTES: usize = 32;
