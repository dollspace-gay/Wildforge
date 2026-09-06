//! Chunk save storage transaction coordination.

use super::encoder;
use crate::chunk::ChunkPos;
use crate::world::World;

impl World {
    pub(in crate::world) fn save_chunk(&self, pos: ChunkPos) -> std::io::Result<()> {
        // Deep chunks (capability E10) never persist: dungeon runs are
        // ephemeral by construction, so every entry regenerates fresh.
        if pos.face().is_deep() {
            return Ok(());
        }
        #[cfg(test)]
        if self.save_fail_chunks.contains(&pos) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected chunk save failure",
            ));
        }
        let chunk = self.chunks.get(&pos).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("chunk {pos:?} is not resident"),
            )
        })?;
        let palette = self.palette.publish(&self.reg)?;
        let buf = encoder::encode_saved_chunk(chunk, &palette)?;
        self.region_store.write(pos, &buf)
    }

    /// Persist a single departing chunk (unload path): only its own
    /// file, only if edited. With the autosave timer gone this is how
    /// most of the world reaches disk: a chunk is written once, as it
    /// leaves the view, instead of the whole world on a clock.
    pub fn save_chunk_if_modified(&self, pos: ChunkPos) -> std::io::Result<bool> {
        if self.chunks.get(&pos).is_some_and(|chunk| chunk.modified) {
            self.save_chunk(pos)?;
            return Ok(true);
        }
        Ok(false)
    }
}
