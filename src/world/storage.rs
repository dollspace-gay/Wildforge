//! Mob/chunk persistence, planetary chunk streaming, saves, and registry remapping.

mod decoder;
mod encoder;
#[cfg(test)]
pub(crate) use encoder::encode_chunk;
pub(crate) use encoder::encode_stream_chunk;
mod palette;
mod palette_store;
mod reader;
mod region_store;
pub(super) use palette_store::PaletteStore;
pub(crate) use reader::{ChunkLoader, ChunkRead, ChunkRevision};
pub(super) use region_store::RegionStore;

mod chunk_access;
mod chunk_save;
mod load_population;
mod loose_items;
mod registry_remap;
mod save_population;
mod save_world;
