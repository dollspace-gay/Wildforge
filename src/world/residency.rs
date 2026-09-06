//! Residency coordinator for the authoritative world.

use super::{Chunk, ChunkPos, ResidencyReport, SaveFailure, World, region};

impl World {
    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    /// Mark every loaded chunk for remesh. Used when something outside the
    /// world changes what a mesh should look like — switching texture packs
    /// can change which atlas slot a face draws, and that lives in the uvs.
    pub fn mark_all_chunks_dirty(&mut self) {
        self.chunks.mark_all_dirty();
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    pub fn mark_chunk_dirty(&mut self, pos: ChunkPos) {
        self.chunks.mark_dirty(pos, true);
    }

    /// Chunks outside `radius` of every one of `centers`.
    ///
    /// Chunk residency is the world's business, not the client's. It used to
    /// live only in the client's streaming path, which meant the dedicated
    /// server — the deployment that actually needs it — never evicted
    /// anything and grew by 448 KB for every chunk any guest ever walked
    /// through. With no centers at all nothing is resident: an empty server
    /// holds no world.
    pub fn chunks_outside_all(&self, centers: &[ChunkPos], radius: i32) -> Vec<ChunkPos> {
        self.chunks.outside(centers, radius)
    }

    /// Save and drop every chunk no longer near any of `centers`.
    ///
    /// Returns what left and what had to stay. Saving as a chunk departs is
    /// the incremental save: there is no short autosave timer, so this is how
    /// most of the world reaches disk.
    pub fn retain_chunks(&mut self, centers: &[ChunkPos], radius: i32) -> ResidencyReport {
        let far = self.chunks_outside_all(centers, radius);
        self.evict_chunks(far).0
    }

    /// Save and unload this exact set, returning both the report and the
    /// positions that actually left. Renderer-owning clients use the latter
    /// to release their matching GPU and light-cache state.
    pub fn evict_chunks(&mut self, candidates: Vec<ChunkPos>) -> (ResidencyReport, Vec<ChunkPos>) {
        let mut report = ResidencyReport::default();
        let mut released = Vec::new();
        if candidates.is_empty() {
            return (report, released);
        }
        self.settle_falling();
        for pos in candidates {
            match self.save_chunk_if_modified(pos) {
                Ok(_) => {
                    self.unload_chunk(pos);
                    report.released += 1;
                    released.push(pos);
                }
                Err(error) => {
                    // The in-memory chunk is the newest copy. Keep it dirty
                    // and resident so the next residency sweep can retry.
                    report.retained_dirty += 1;
                    report.failures.push(SaveFailure::new(
                        format!("chunk {pos:?}"),
                        region::region_path(&self.save_dir, pos),
                        error,
                    ));
                }
            }
        }
        (report, released)
    }

    pub fn unload_chunk(&mut self, pos: ChunkPos) {
        self.chunks.remove(&pos);
    }

    /// How many chunks are resident. The number a long-running server has to
    /// keep bounded.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[cfg(test)]
    pub fn fail_chunk_save_for_test(&mut self, pos: ChunkPos, fail: bool) {
        if fail {
            self.save_fail_chunks.insert(pos);
        } else {
            self.save_fail_chunks.remove(&pos);
        }
    }

    #[cfg(test)]
    pub fn fail_loose_item_save_for_test(&mut self, fail: bool) {
        self.fail_loose_item_save = fail;
    }

    #[cfg(test)]
    pub fn mark_structure_chunk_for_test(&mut self, pos: ChunkPos) {
        self.structure_chunks.insert(pos);
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.modified = true;
        }
    }
}
