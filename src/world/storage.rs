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



mod loose_items;
mod save_population;
mod load_population;
mod chunk_access;
mod chunk_save;
mod save_world;
mod registry_remap;

